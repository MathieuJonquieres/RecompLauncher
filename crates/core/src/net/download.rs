//! Streaming downloads with resume, progress, cancellation and SHA-256 checks.
//!
//! Downloads are written to `<destination>.part`; the final path is only
//! created after the optional digest matches. A `.part` file is kept on
//! cancellation/network failure so the next attempt can resume with a
//! `Range: bytes=<len>-` request.

use std::path::{Path, PathBuf};
use std::time::Duration;

use futures::StreamExt;
use reqwest::header::{CONTENT_RANGE, RANGE};
use reqwest::StatusCode;
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

use crate::hash;
use crate::net::http_client::{HttpClient, HttpError};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DownloadError {
    #[error("HTTP error: {0}")]
    Http(#[from] HttpError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("download cancelled")]
    Cancelled,

    #[error("SHA-256 mismatch for downloaded file")]
    HashMismatch,

    #[error("download exceeds the {0} byte limit")]
    TooLarge(u64),

    #[error("invalid Content-Range header: {0}")]
    InvalidContentRange(String),

    #[error("hash error: {0}")]
    Hash(#[from] crate::hash::HashError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadOutcome {
    pub bytes: u64,
    pub resumed: bool,
}

#[derive(Debug, Clone)]
pub struct DownloadOptions {
    pub max_bytes: u64,
    pub expected_sha256: Option<String>,
    pub resume: bool,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            max_bytes: 4 * 1024 * 1024 * 1024,
            expected_sha256: None,
            resume: true,
        }
    }
}

/// Downloads `url` to `destination`, resuming a previous `.part` when possible.
pub async fn download_to_file(
    client: &HttpClient,
    url: &str,
    destination: &Path,
    options: &DownloadOptions,
    progress: Option<UnboundedSender<DownloadProgress>>,
    cancel: Option<CancellationToken>,
) -> Result<DownloadOutcome, DownloadError> {
    let url = client.validate_url(url)?;
    if let Some(parent) = destination.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let part = part_path(destination);
    if !options.resume {
        let _ = tokio::fs::remove_file(&part).await;
    }

    if cancel.as_ref().is_some_and(CancellationToken::is_cancelled) {
        return Err(DownloadError::Cancelled);
    }

    let mut attempt = 0_u32;
    loop {
        match download_once(
            client,
            url.as_str(),
            &part,
            options,
            progress.as_ref(),
            cancel.as_ref(),
        )
        .await
        {
            Ok(outcome) => {
                if let Some(expected) = &options.expected_sha256 {
                    let part_for_hash = part.clone();
                    let expected = expected.clone();
                    let matches = tokio::task::spawn_blocking(move || {
                        hash::verify_sha256(&part_for_hash, &expected)
                    })
                    .await
                    .map_err(|error| DownloadError::Io(std::io::Error::other(error)))??;
                    if !matches {
                        let _ = tokio::fs::remove_file(&part).await;
                        return Err(DownloadError::HashMismatch);
                    }
                }

                if tokio::fs::try_exists(destination).await? {
                    tokio::fs::remove_file(destination).await?;
                }
                tokio::fs::rename(&part, destination).await?;
                return Ok(outcome);
            }
            Err(DownloadAttemptError::Retryable(error)) if attempt < 3 => {
                attempt += 1;
                tracing::warn!(%error, attempt, "download attempt failed, retrying");
                tokio::time::sleep(Duration::from_millis(300_u64 << attempt.min(3))).await;
            }
            Err(DownloadAttemptError::Retryable(error)) => {
                return Err(error);
            }
            Err(DownloadAttemptError::Fatal(error)) => return Err(error),
        }
    }
}

fn part_path(destination: &Path) -> PathBuf {
    let mut name = destination
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "download".into());
    name.push(".part");
    destination.with_file_name(name)
}

enum DownloadAttemptError {
    Retryable(DownloadError),
    Fatal(DownloadError),
}

async fn download_once(
    client: &HttpClient,
    url: &str,
    part: &Path,
    options: &DownloadOptions,
    progress: Option<&UnboundedSender<DownloadProgress>>,
    cancel: Option<&CancellationToken>,
) -> Result<DownloadOutcome, DownloadAttemptError> {
    let existing = match tokio::fs::metadata(part).await {
        Ok(metadata) => metadata.len(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
        Err(error) => return Err(DownloadAttemptError::Fatal(DownloadError::Io(error))),
    };
    let resumed = existing > 0;
    if existing > options.max_bytes {
        return Err(DownloadAttemptError::Fatal(DownloadError::TooLarge(
            options.max_bytes,
        )));
    }

    let mut request = client
        .raw_client()
        .get(url)
        .timeout(Duration::from_secs(3600));
    if resumed {
        request = request.header(RANGE, format!("bytes={existing}-"));
    }
    let response = match request.send().await {
        Ok(response) => response,
        Err(error) => {
            let mapped = HttpError::Network(error);
            return Err(if is_retryable_http(&mapped) {
                DownloadAttemptError::Retryable(DownloadError::Http(mapped))
            } else {
                DownloadAttemptError::Fatal(DownloadError::Http(mapped))
            });
        }
    };

    let status = response.status();
    if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        return Err(DownloadAttemptError::Retryable(DownloadError::Http(
            HttpError::Status(status.as_u16()),
        )));
    }
    if status == StatusCode::RANGE_NOT_SATISFIABLE && resumed {
        // The server considers the part complete or invalid. Let the digest
        // check decide; a corrupt part will be reported as a mismatch.
        let total = response
            .headers()
            .get(CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_content_range_total);
        if let Some(total) = total {
            if existing == total {
                return Ok(DownloadOutcome {
                    bytes: existing,
                    resumed: true,
                });
            }
        }
        return Err(DownloadAttemptError::Fatal(
            DownloadError::InvalidContentRange("range not satisfiable".to_string()),
        ));
    }
    if !status.is_success() {
        return Err(DownloadAttemptError::Fatal(DownloadError::Http(
            HttpError::Status(status.as_u16()),
        )));
    }

    let append = if status == StatusCode::PARTIAL_CONTENT {
        true
    } else if status == StatusCode::OK {
        false
    } else {
        return Err(DownloadAttemptError::Fatal(DownloadError::Http(
            HttpError::Status(status.as_u16()),
        )));
    };

    let remaining = response.content_length();
    if let Some(remaining) = remaining {
        if existing.saturating_add(remaining) > options.max_bytes {
            return Err(DownloadAttemptError::Fatal(DownloadError::TooLarge(
                options.max_bytes,
            )));
        }
    }

    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(part)
        .await
        .map_err(DownloadError::Io)
        .map_err(DownloadAttemptError::Fatal)?;

    let mut downloaded = if append { existing } else { 0 };
    let total = remaining.map(|remaining| downloaded.saturating_add(remaining));
    if let Some(progress) = progress {
        let _ = progress.send(DownloadProgress { downloaded, total });
    }

    let cancel_future = async {
        match cancel {
            Some(cancel) => cancel.cancelled().await,
            None => std::future::pending().await,
        }
    };
    tokio::pin!(cancel_future);
    let mut stream = response.bytes_stream();
    loop {
        tokio::select! {
            _ = &mut cancel_future => {
                let _ = file.flush().await;
                return Err(DownloadAttemptError::Fatal(DownloadError::Cancelled));
            }
            chunk = stream.next() => {
                match chunk {
                    Some(Ok(chunk)) => {
                        downloaded = downloaded.saturating_add(chunk.len() as u64);
                        if downloaded > options.max_bytes {
                            return Err(DownloadAttemptError::Fatal(DownloadError::TooLarge(options.max_bytes)));
                        }
                        file.write_all(&chunk).await
                            .map_err(DownloadError::Io)
                            .map_err(DownloadAttemptError::Fatal)?;
                        if let Some(progress) = progress {
                            let _ = progress.send(DownloadProgress { downloaded, total });
                        }
                    }
                    Some(Err(error)) => {
                        return Err(DownloadAttemptError::Retryable(DownloadError::Http(HttpError::Network(error))));
                    }
                    None => break,
                }
            }
        }
    }
    file.flush()
        .await
        .map_err(DownloadError::Io)
        .map_err(DownloadAttemptError::Fatal)?;
    drop(file);

    Ok(DownloadOutcome {
        bytes: downloaded,
        resumed: append && existing > 0,
    })
}

fn is_retryable_http(error: &HttpError) -> bool {
    match error {
        HttpError::Network(error) => error.is_timeout() || error.is_connect() || error.is_request(),
        HttpError::Status(status) => *status == 429 || (500..=599).contains(status),
        _ => false,
    }
}

fn parse_content_range_total(value: &str) -> Option<u64> {
    // `bytes 0-4/10`
    value.rsplit('/').next()?.trim().parse().ok()
}

/// Removes a partial file after an unrecoverable error.
pub async fn discard_partial(destination: &Path) {
    let _ = tokio::fs::remove_file(part_path(destination)).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn client_for(server: &MockServer) -> HttpClient {
        HttpClient::for_tests([server.address().ip().to_string()]).expect("client")
    }

    #[tokio::test]
    async fn downloads_and_verifies_sha256() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/game.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"hello world".to_vec()))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().expect("tempdir");
        let destination = dir.path().join("game.zip");
        let options = DownloadOptions {
            expected_sha256: Some(
                "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9".to_string(),
            ),
            ..DownloadOptions::default()
        };
        let outcome = download_to_file(
            &client_for(&server).await,
            &format!("{}/game.zip", server.uri()),
            &destination,
            &options,
            None,
            None,
        )
        .await
        .expect("download");
        assert_eq!(outcome.bytes, 11);
        assert!(!outcome.resumed);
        assert_eq!(std::fs::read(&destination).expect("read"), b"hello world");
        assert!(!part_path(&destination).exists());
    }

    #[tokio::test]
    async fn resumes_with_range_request() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/game.zip"))
            .and(header("range", "bytes=6-"))
            .respond_with(
                ResponseTemplate::new(206)
                    .insert_header("content-range", "bytes 6-10/11")
                    .set_body_bytes(b"world".to_vec()),
            )
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().expect("tempdir");
        let destination = dir.path().join("game.zip");
        std::fs::write(part_path(&destination), b"hello ").expect("part");

        let outcome = download_to_file(
            &client_for(&server).await,
            &format!("{}/game.zip", server.uri()),
            &destination,
            &DownloadOptions::default(),
            None,
            None,
        )
        .await
        .expect("resume");
        assert_eq!(outcome.bytes, 11);
        assert!(outcome.resumed);
        assert_eq!(std::fs::read(&destination).expect("read"), b"hello world");
    }

    #[tokio::test]
    async fn rejects_hash_mismatch_and_removes_part() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"hello".to_vec()))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().expect("tempdir");
        let destination = dir.path().join("game.bin");
        let options = DownloadOptions {
            expected_sha256: Some("0".repeat(64)),
            ..DownloadOptions::default()
        };
        let error = download_to_file(
            &client_for(&server).await,
            &format!("{}/game.bin", server.uri()),
            &destination,
            &options,
            None,
            None,
        )
        .await
        .expect_err("hash mismatch");
        assert!(matches!(error, DownloadError::HashMismatch));
        assert!(!part_path(&destination).exists());
        assert!(!destination.exists());
    }

    #[tokio::test]
    async fn enforces_max_bytes_from_content_length() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0_u8; 100]))
            .mount(&server)
            .await;

        let dir = tempfile::tempdir().expect("tempdir");
        let options = DownloadOptions {
            max_bytes: 10,
            ..DownloadOptions::default()
        };
        let error = download_to_file(
            &client_for(&server).await,
            &format!("{}/big", server.uri()),
            &dir.path().join("big.bin"),
            &options,
            None,
            None,
        )
        .await
        .expect_err("too large");
        assert!(matches!(error, DownloadError::TooLarge(_)));
    }

    #[tokio::test]
    async fn respects_pre_cancelled_token() {
        let server = MockServer::start().await;
        let dir = tempfile::tempdir().expect("tempdir");
        let cancel = CancellationToken::new();
        cancel.cancel();
        let error = download_to_file(
            &client_for(&server).await,
            &format!("{}/never", server.uri()),
            &dir.path().join("never.bin"),
            &DownloadOptions::default(),
            None,
            Some(cancel),
        )
        .await
        .expect_err("cancelled");
        assert!(matches!(error, DownloadError::Cancelled));
    }
}
