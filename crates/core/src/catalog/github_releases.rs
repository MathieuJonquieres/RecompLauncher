//! GitHub Releases utility (spec §3.C).
//!
//! Only official `github.com` release assets are used. Responses are cached
//! with ETag by `HttpClient`; an optional token raises the API rate limit.

use std::sync::Arc;

use serde::Deserialize;
use tracing::debug;

use super::model::{AssetPattern, GameEntry, Os};
use crate::net::http_client::{HttpClient, HttpError};

const MAX_RELEASE_BYTES: usize = 4 * 1024 * 1024;

/// One downloadable asset of a GitHub release.
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub digest: Option<String>,
}

impl GitHubAsset {
    /// Published SHA-256 digest, when GitHub provides one.
    pub fn sha256(&self) -> Option<&str> {
        self.digest
            .as_deref()
            .and_then(|digest| digest.strip_prefix("sha256:"))
            .filter(|hex| hex.len() == 64)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubRelease {
    pub tag_name: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub assets: Vec<GitHubAsset>,
}

/// Resolved asset plus its expected integrity hash.
#[derive(Debug, Clone)]
pub struct SelectedAsset {
    pub asset: GitHubAsset,
    pub expected_sha256: Option<String>,
}

/// Fetches releases for one repository.
pub struct GitHubReleasesProvider {
    client: HttpClient,
    owner: String,
    repo: String,
    token: Option<String>,
    api_base: String,
}

impl GitHubReleasesProvider {
    pub fn new(client: HttpClient, repo_slug: &str, token: Option<String>) -> Result<Self, String> {
        let mut parts = repo_slug.split('/');
        let owner = parts.next().unwrap_or_default();
        let repo = parts.next().unwrap_or_default();
        if owner.is_empty() || repo.is_empty() || parts.next().is_some() {
            return Err(format!("invalid GitHub repository `{repo_slug}`"));
        }
        if !owner
            .chars()
            .chain(repo.chars())
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
        {
            return Err(format!("invalid GitHub repository `{repo_slug}`"));
        }
        Ok(Self {
            client,
            owner: owner.to_string(),
            repo: repo.to_string(),
            token,
            api_base: "https://api.github.com".to_string(),
        })
    }

    /// Test hook: point the provider at a wiremock server.
    #[cfg(test)]
    fn with_api_base(mut self, base: impl Into<String>) -> Self {
        self.api_base = base.into();
        self
    }

    pub fn from_repo_url(
        client: HttpClient,
        repo_url: &str,
        token: Option<String>,
    ) -> Result<Self, String> {
        let trimmed = repo_url
            .trim_end_matches('/')
            .strip_prefix("https://github.com/")
            .ok_or_else(|| format!("not a GitHub repository URL: {repo_url}"))?;
        Self::new(client, trimmed, token)
    }

    fn headers(&self) -> Vec<(&str, &str)> {
        match &self.token {
            Some(token) if !token.is_empty() => vec![
                ("Accept", "application/vnd.github+json"),
                ("X-GitHub-Api-Version", "2022-11-28"),
                ("Authorization", token.as_str()),
            ],
            _ => vec![
                ("Accept", "application/vnd.github+json"),
                ("X-GitHub-Api-Version", "2022-11-28"),
            ],
        }
    }

    /// Latest non-draft release; `None` when the repository has none.
    pub async fn fetch_latest(&self) -> Result<Option<GitHubRelease>, HttpError> {
        let url = format!(
            "{}/repos/{}/{}/releases/latest",
            self.api_base, self.owner, self.repo
        );
        match self
            .client
            .get_limited_with_headers(&url, MAX_RELEASE_BYTES, &self.headers())
            .await
        {
            Ok(response) => {
                let release: GitHubRelease =
                    serde_json::from_slice(&response.body).map_err(|error| {
                        HttpError::InvalidUrl(format!("invalid release JSON: {error}"))
                    })?;
                debug!(repo = %self.repo, tag = %release.tag_name, "latest release resolved");
                Ok(Some(release))
            }
            Err(HttpError::Status(404)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    /// Up to `per_page` releases, newest first (used for version history).
    pub async fn fetch_releases(&self, per_page: u8) -> Result<Vec<GitHubRelease>, HttpError> {
        let per_page = per_page.clamp(1, 100);
        let url = format!(
            "{}/repos/{}/{}/releases?per_page={per_page}",
            self.api_base, self.owner, self.repo
        );
        let response = self
            .client
            .get_limited_with_headers(&url, MAX_RELEASE_BYTES, &self.headers())
            .await?;
        serde_json::from_slice(&response.body)
            .map_err(|error| HttpError::InvalidUrl(format!("invalid releases JSON: {error}")))
    }
}

/// Selects the asset matching the manifest pattern for `os`.
pub fn select_asset(entry: &GameEntry, release: &GitHubRelease, os: Os) -> Option<SelectedAsset> {
    let pattern = entry.best_asset(os)?;
    let asset = release
        .assets
        .iter()
        .find(|asset| pattern_matches(&pattern.pattern, &asset.name))?
        .clone();
    let expected_sha256 = pattern
        .sha256
        .as_ref()
        .map(|hash| hash.as_str().to_string())
        .or_else(|| asset.sha256().map(str::to_string));
    Some(SelectedAsset {
        asset,
        expected_sha256,
    })
}

/// Matches a manifest asset pattern (`*` and `?` wildcards).
pub fn pattern_matches(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    let (mut p, mut n) = (0_usize, 0_usize);
    let (mut star, mut resume) = (None, 0_usize);

    while n < name.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            p += 1;
            resume = n;
        } else if let Some(star_index) = star {
            p = star_index + 1;
            resume += 1;
            n = resume;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

/// Whether an asset pattern targets the given OS/arch pair.
pub fn pattern_matches_target(pattern: &AssetPattern, os: Os) -> bool {
    pattern.os == os && pattern.arch_matches()
}

/// Shared client wrapper for tests.
pub type SharedReleases = Arc<GitHubReleasesProvider>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::test_support::minimal_entry;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn wildcard_matching() {
        assert!(pattern_matches("*Windows.zip", "Game-v1-Windows.zip"));
        assert!(pattern_matches("Game-*.zip", "Game-1.0.zip"));
        assert!(pattern_matches("Game-?.zip", "Game-1.zip"));
        assert!(!pattern_matches("Game-?.zip", "Game-10.zip"));
        assert!(!pattern_matches("*Linux*", "Game-Windows.zip"));
        assert!(pattern_matches("*", "anything"));
    }

    #[test]
    fn selects_asset_and_digest() {
        let mut entry = minimal_entry("test");
        entry.releases.assets[0].sha256 = None;
        // The fixture targets x86_64; clear the arch so this assertion stays
        // independent of the host running the test suite.
        entry.releases.assets[0].arch = None;
        let release = GitHubRelease {
            tag_name: "v1.0.0".to_string(),
            name: "v1.0.0".to_string(),
            published_at: None,
            prerelease: false,
            assets: vec![GitHubAsset {
                name: "Game-v1.0.0-Windows.zip".to_string(),
                browser_download_url:
                    "https://github.com/example/game/releases/download/v1.0.0/Game.zip".to_string(),
                size: 42,
                digest: Some(format!("sha256:{}", "a".repeat(64))),
            }],
        };
        let selected = select_asset(&entry, &release, Os::Windows).expect("asset");
        assert_eq!(selected.expected_sha256, Some("a".repeat(64)));
    }

    #[tokio::test]
    async fn fetches_latest_release_from_mock() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/owner/repo/releases/latest"))
            .and(header("accept", "application/vnd.github+json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tag_name": "v2.0.0",
                "name": "v2",
                "assets": [
                    {
                        "name": "Game-v2.0.0-Windows.zip",
                        "browser_download_url": "https://github.com/owner/repo/releases/download/v2/Game.zip",
                        "size": 1234,
                        "digest": format!("sha256:{}", "b".repeat(64))
                    }
                ]
            })))
            .mount(&server)
            .await;

        let client = HttpClient::for_tests(["127.0.0.1".to_string()]).expect("client");
        let provider = GitHubReleasesProvider::new(client, "owner/repo", None)
            .expect("provider")
            .with_api_base(server.uri());
        let release = provider
            .fetch_latest()
            .await
            .expect("fetch")
            .expect("release");
        assert_eq!(release.tag_name, "v2.0.0");
        let expected = "b".repeat(64);
        assert_eq!(release.assets[0].sha256(), Some(expected.as_str()));
    }
}
