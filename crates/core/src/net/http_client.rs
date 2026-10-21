//! HTTPS client with allowlist, timeouts, retries and ETag cache.
//!
//! Security rules (spec §10):
//! - HTTPS only; TLS verification can never be disabled.
//! - Every redirect is re-checked against the allowlist.
//! - Response bodies are capped by the caller.
//!
//! The ETag cache is in-memory and bounded; catalog persistence lives in SQLite.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use reqwest::header::{ETAG, IF_NONE_MATCH};
use reqwest::{Client, StatusCode, Url};
use serde::de::DeserializeOwned;
use thiserror::Error;

/// Hosts the launcher may contact by default.
pub const DEFAULT_ALLOWED_HOSTS: &[&str] = &[
    "github.com",
    "api.github.com",
    "objects.githubusercontent.com",
    "release-assets.githubusercontent.com",
    "raw.githubusercontent.com",
    "codeload.github.com",
    "wikipedia.org",
    "en.wikipedia.org",
    "fr.wikipedia.org",
    "upload.wikimedia.org",
];

const MAX_REDIRECTS: usize = 5;
const MAX_ETAG_ENTRIES: usize = 64;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum HttpError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("host `{0}` is not allowed")]
    ForbiddenHost(String),

    #[error("only HTTPS is allowed")]
    InsecureScheme,

    #[error("HTTP status {0}")]
    Status(u16),

    #[error("response exceeds the {0} byte limit")]
    TooLarge(usize),

    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("operation cancelled")]
    Cancelled,

    #[error("hash mismatch for downloaded file")]
    HashMismatch,
}

/// Tunable client settings.
#[derive(Debug, Clone)]
pub struct HttpConfig {
    pub timeout: Duration,
    pub max_retries: u32,
    pub user_agent: String,
    /// Optional HTTP(S) proxy, e.g. `http://127.0.0.1:8080`.
    pub proxy: Option<String>,
    /// Test-only escape hatch for wiremock (`http://127.0.0.1`). Never set in
    /// production code.
    pub allow_insecure_loopback: bool,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(20),
            max_retries: 3,
            user_agent: format!(
                "RecompLauncher/{} (desktop launcher)",
                env!("CARGO_PKG_VERSION")
            ),
            proxy: None,
            allow_insecure_loopback: false,
        }
    }
}

#[derive(Debug, Clone)]
struct CachedBody {
    etag: String,
    body: Vec<u8>,
}

/// Successful HTTP response.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Vec<u8>,
    pub etag: Option<String>,
    /// True when the server answered 304 and the cached body was reused.
    pub from_cache: bool,
}

impl HttpResponse {
    pub fn text(&self) -> Result<String, HttpError> {
        String::from_utf8(self.body.clone())
            .map_err(|_| HttpError::InvalidUrl("response is not valid UTF-8".to_string()))
    }
}

/// Reusable, cheaply cloneable HTTP client.
#[derive(Clone)]
pub struct HttpClient {
    inner: Arc<Inner>,
}

struct Inner {
    client: Client,
    allowed: HashSet<String>,
    etag_cache: Mutex<HashMap<String, CachedBody>>,
    config: HttpConfig,
}

impl std::fmt::Debug for HttpClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HttpClient")
            .field("allowed_hosts", &self.inner.allowed.len())
            .field("timeout", &self.inner.config.timeout)
            .field(
                "proxy",
                &self.inner.config.proxy.as_ref().map(|_| "<configured>"),
            )
            .finish()
    }
}

impl HttpClient {
    pub fn new(
        config: HttpConfig,
        extra_hosts: impl IntoIterator<Item = String>,
    ) -> Result<Self, HttpError> {
        let mut allowed: HashSet<String> = DEFAULT_ALLOWED_HOSTS
            .iter()
            .map(|host| (*host).to_string())
            .collect();
        allowed.extend(extra_hosts);

        let redirect_allowed = allowed.clone();
        let redirect_policy = reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= MAX_REDIRECTS {
                return attempt.error("too many redirects");
            }
            let host = attempt
                .url()
                .host_str()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if redirect_allowed.contains(&host) {
                attempt.follow()
            } else {
                attempt.error(format!("redirect to forbidden host `{host}`"))
            }
        });

        let client = Client::builder()
            .timeout(config.timeout)
            .connect_timeout(Duration::from_secs(10))
            .user_agent(config.user_agent.clone())
            .redirect(redirect_policy)
            .http2_adaptive_window(true);

        let client = match &config.proxy {
            Some(proxy) if !proxy.trim().is_empty() => client.proxy(
                reqwest::Proxy::all(proxy)
                    .map_err(|_| HttpError::InvalidUrl(format!("invalid proxy `{proxy}`")))?,
            ),
            _ => client,
        }
        .build()?;

        Ok(Self {
            inner: Arc::new(Inner {
                client,
                allowed,
                etag_cache: Mutex::new(HashMap::new()),
                config,
            }),
        })
    }

    pub fn with_defaults() -> Result<Self, HttpError> {
        Self::new(HttpConfig::default(), std::iter::empty())
    }

    /// Internal access for the streaming downloader. Kept crate-private so the
    /// allowlist/redirect policy cannot be bypassed by callers.
    pub(crate) fn raw_client(&self) -> &Client {
        &self.inner.client
    }

    /// Test constructor: allows plain HTTP on loopback only.
    pub fn for_tests(extra_hosts: impl IntoIterator<Item = String>) -> Result<Self, HttpError> {
        let config = HttpConfig {
            allow_insecure_loopback: true,
            max_retries: 2,
            timeout: Duration::from_secs(5),
            ..HttpConfig::default()
        };
        Self::new(config, extra_hosts)
    }

    /// Parses and validates a URL against the scheme/host policy.
    pub fn validate_url(&self, raw: &str) -> Result<Url, HttpError> {
        let url = Url::parse(raw).map_err(|_| HttpError::InvalidUrl(raw.to_string()))?;
        let scheme_ok = url.scheme() == "https"
            || (self.inner.config.allow_insecure_loopback
                && url.scheme() == "http"
                && is_loopback(url.host_str().unwrap_or_default()));
        if !scheme_ok {
            return Err(HttpError::InsecureScheme);
        }
        let host = url
            .host_str()
            .ok_or_else(|| HttpError::InvalidUrl(raw.to_string()))?
            .to_ascii_lowercase();
        if !self.inner.allowed.contains(&host) {
            return Err(HttpError::ForbiddenHost(host));
        }
        Ok(url)
    }

    /// GET with retry, timeout, ETag revalidation and a hard body limit.
    pub async fn get_limited(
        &self,
        raw_url: &str,
        max_bytes: usize,
    ) -> Result<HttpResponse, HttpError> {
        self.get_limited_with_headers(raw_url, max_bytes, &[]).await
    }

    /// Same as [`get_limited`] with extra request headers (e.g. GitHub token).
    pub async fn get_limited_with_headers(
        &self,
        raw_url: &str,
        max_bytes: usize,
        headers: &[(&str, &str)],
    ) -> Result<HttpResponse, HttpError> {
        let url = self.validate_url(raw_url)?;
        let cache_key = url.as_str().to_string();

        let cached = self
            .inner
            .etag_cache
            .lock()
            .ok()
            .and_then(|cache| cache.get(&cache_key).cloned());

        let mut attempt = 0_u32;
        loop {
            let mut request = self.inner.client.get(url.clone());
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            if let Some(entry) = &cached {
                request = request.header(IF_NONE_MATCH, entry.etag.as_str());
            }

            let result = request.send().await;
            match result {
                Ok(response) => {
                    let status = response.status();
                    if status == StatusCode::NOT_MODIFIED {
                        if let Some(entry) = cached {
                            return Ok(HttpResponse {
                                status: status.as_u16(),
                                body: entry.body,
                                etag: Some(entry.etag),
                                from_cache: true,
                            });
                        }
                        return Err(HttpError::Status(status.as_u16()));
                    }

                    if status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
                        if attempt < self.inner.config.max_retries {
                            sleep_backoff(attempt).await;
                            attempt += 1;
                            continue;
                        }
                        return Err(HttpError::Status(status.as_u16()));
                    }

                    if !status.is_success() {
                        return Err(HttpError::Status(status.as_u16()));
                    }

                    if response
                        .content_length()
                        .is_some_and(|len| len > max_bytes as u64)
                    {
                        return Err(HttpError::TooLarge(max_bytes));
                    }

                    let etag = response
                        .headers()
                        .get(ETAG)
                        .and_then(|value| value.to_str().ok())
                        .map(str::to_string);

                    let body = response.bytes().await?.to_vec();
                    if body.len() > max_bytes {
                        return Err(HttpError::TooLarge(max_bytes));
                    }

                    if let Some(etag_value) = &etag {
                        if let Ok(mut cache) = self.inner.etag_cache.lock() {
                            if cache.len() >= MAX_ETAG_ENTRIES {
                                cache.clear();
                            }
                            cache.insert(
                                cache_key,
                                CachedBody {
                                    etag: etag_value.clone(),
                                    body: body.clone(),
                                },
                            );
                        }
                    }

                    return Ok(HttpResponse {
                        status: status.as_u16(),
                        body,
                        etag,
                        from_cache: false,
                    });
                }
                Err(error) => {
                    if attempt < self.inner.config.max_retries && is_retryable(&error) {
                        sleep_backoff(attempt).await;
                        attempt += 1;
                        continue;
                    }
                    return Err(HttpError::Network(error));
                }
            }
        }
    }

    pub async fn get_json<T: DeserializeOwned>(
        &self,
        url: &str,
        max_bytes: usize,
    ) -> Result<T, HttpError> {
        let response = self.get_limited(url, max_bytes).await?;
        serde_json::from_slice(&response.body)
            .map_err(|error| HttpError::InvalidUrl(format!("invalid JSON from {url}: {error}")))
    }
}

fn is_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "::1" | "localhost")
}

fn is_retryable(error: &reqwest::Error) -> bool {
    error.is_timeout() || error.is_connect() || error.is_request()
}

async fn sleep_backoff(attempt: u32) {
    let millis = 250_u64.saturating_mul(1_u64 << attempt.min(4));
    tokio::time::sleep(Duration::from_millis(millis)).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn client_for(server: &MockServer) -> HttpClient {
        let host = server.address().ip().to_string();
        HttpClient::for_tests([host]).expect("test client")
    }

    #[tokio::test]
    async fn rejects_forbidden_hosts() {
        let client = HttpClient::with_defaults().expect("client");
        let error = client
            .get_limited("https://evil.example/game.json", 1024)
            .await
            .expect_err("host must be rejected");
        assert!(matches!(error, HttpError::ForbiddenHost(_)));
    }

    #[tokio::test]
    async fn rejects_http_outside_tests() {
        let client = HttpClient::with_defaults().expect("client");
        let error = client
            .get_limited("http://github.com/file", 1024)
            .await
            .expect_err("http must be rejected");
        assert!(matches!(error, HttpError::InsecureScheme));
    }

    #[tokio::test]
    async fn fetches_json_with_limit() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/catalog.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "schema": 1,
                "entries": []
            })))
            .mount(&server)
            .await;

        let client = client_for(&server).await;
        let response = client
            .get_limited(&format!("{}/catalog.json", server.uri()), 4096)
            .await
            .expect("fetch");
        assert_eq!(response.status, 200);
        assert!(response.text().expect("utf8").contains("entries"));
    }

    #[tokio::test]
    async fn enforces_body_limit() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(4096)))
            .mount(&server)
            .await;

        let client = client_for(&server).await;
        let error = client
            .get_limited(&format!("{}/big", server.uri()), 128)
            .await
            .expect_err("too large");
        assert!(matches!(error, HttpError::TooLarge(_)));
    }

    #[tokio::test]
    async fn retries_server_errors_then_succeeds() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(1)
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .with_priority(2)
            .mount(&server)
            .await;

        let client = client_for(&server).await;
        let response = client
            .get_limited(&format!("{}/retry", server.uri()), 1024)
            .await
            .expect("eventually succeeds");
        assert_eq!(response.body, b"ok");
    }

    #[tokio::test]
    async fn revalidates_with_etag() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(header("if-none-match", "\"v1\""))
            .respond_with(ResponseTemplate::new(304))
            .with_priority(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("ETag", "\"v1\"")
                    .set_body_string("cached-body"),
            )
            .with_priority(2)
            .mount(&server)
            .await;

        let client = client_for(&server).await;
        let url = format!("{}/etag", server.uri());
        let first = client.get_limited(&url, 1024).await.expect("first");
        assert_eq!(first.body, b"cached-body");
        assert!(!first.from_cache);

        let second = client.get_limited(&url, 1024).await.expect("second");
        assert!(second.from_cache);
        assert_eq!(second.body, b"cached-body");
    }
}
