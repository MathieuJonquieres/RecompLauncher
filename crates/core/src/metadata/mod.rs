//! Game metadata enrichment (Wikipedia REST API + 30-day SQLite cache).
//!
//! The launcher never copies or uploads game content. Only public encyclopedic
//! metadata is fetched, cached and displayed with its source attribution.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use url::Url;

use crate::db::{now_unix, Db, DbError};
use crate::net::http_client::{HttpClient, HttpError};

/// Metadata is refreshed after thirty days.
pub const METADATA_TTL_SECONDS: i64 = 30 * 24 * 60 * 60;
const MAX_SUMMARY_BYTES: usize = 256 * 1024;
const ATTRIBUTION: &str = "Wikipédia — CC BY-SA 4.0";

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum MetadataError {
    #[error("metadata HTTP error: {0}")]
    Http(#[from] HttpError),

    #[error("metadata database error: {0}")]
    Db(#[from] DbError),

    #[error("invalid Wikipedia URL: {0}")]
    Url(String),

    #[error("a Wikipedia title is required")]
    EmptyTitle,
}

/// Normalised public metadata for one game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameMetadata {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub year: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub developer: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<String>,
    pub attribution: String,
    pub fetched_at: i64,
}

impl GameMetadata {
    fn new(title: String) -> Self {
        Self {
            title,
            summary: None,
            description: None,
            thumbnail_url: None,
            source_url: None,
            year: None,
            developer: None,
            publisher: None,
            attribution: ATTRIBUTION.to_string(),
            fetched_at: now_unix(),
        }
    }
}

/// Result of a metadata lookup, including whether the cache was used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataOutcome {
    pub metadata: GameMetadata,
    pub from_cache: bool,
    pub stale: bool,
}

/// Object-safe provider contract so providers can be swapped or mocked.
#[async_trait]
pub trait MetadataProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn fetch(&self, title: &str) -> Result<GameMetadata, MetadataError>;
}

/// Wikipedia summary API provider.
pub struct WikipediaProvider {
    http: HttpClient,
    base: String,
    language: String,
}

impl WikipediaProvider {
    pub fn new(http: HttpClient) -> Self {
        Self::with_base(
            http,
            "https://en.wikipedia.org/api/rest_v1/page/summary/".to_string(),
            "en".to_string(),
        )
    }

    /// Test/configuration hook. `base` must end with a slash.
    pub fn with_base(http: HttpClient, base: String, language: String) -> Self {
        Self {
            http,
            base,
            language,
        }
    }

    fn summary_url(&self, title: &str) -> Result<String, MetadataError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(MetadataError::EmptyTitle);
        }
        let mut url =
            Url::parse(&self.base).map_err(|error| MetadataError::Url(error.to_string()))?;
        url.path_segments_mut()
            .map_err(|_| MetadataError::Url(self.base.clone()))?
            .pop_if_empty()
            .push(title);
        Ok(url.to_string())
    }
}

#[derive(Debug, Deserialize)]
struct WikipediaSummary {
    title: String,
    #[serde(default)]
    extract: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    thumbnail: Option<WikipediaImage>,
    #[serde(default)]
    content_urls: Option<WikipediaContentUrls>,
}

#[derive(Debug, Deserialize)]
struct WikipediaImage {
    source: String,
}

#[derive(Debug, Deserialize)]
struct WikipediaContentUrls {
    desktop: WikipediaPageUrl,
}

#[derive(Debug, Deserialize)]
struct WikipediaPageUrl {
    page: String,
}

#[async_trait]
impl MetadataProvider for WikipediaProvider {
    fn name(&self) -> &'static str {
        "wikipedia"
    }

    async fn fetch(&self, title: &str) -> Result<GameMetadata, MetadataError> {
        let url = self.summary_url(title)?;
        let summary: WikipediaSummary = self.http.get_json(&url, MAX_SUMMARY_BYTES).await?;
        let mut metadata = GameMetadata::new(summary.title);
        metadata.summary = summary.extract.filter(|value| !value.trim().is_empty());
        metadata.description = summary.description.filter(|value| !value.trim().is_empty());
        metadata.thumbnail_url = summary.thumbnail.map(|image| image.source);
        metadata.source_url = summary.content_urls.map(|urls| urls.desktop.page);
        metadata.attribution = format!("Wikipédia ({}) — CC BY-SA 4.0", self.language);
        Ok(metadata)
    }
}

/// SQLite-backed cache for metadata records.
#[derive(Clone)]
pub struct MetadataStore {
    db: Db,
}

impl MetadataStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn get(&self, slug: &str) -> Result<Option<(GameMetadata, i64)>, DbError> {
        let slug = slug.to_string();
        self.db
            .with_conn(move |conn| {
                let mut statement =
                    conn.prepare("SELECT json, fetched_at FROM metadata_cache WHERE slug = ?1")?;
                let mut rows = statement.query_map([slug], |row| {
                    let json: String = row.get(0)?;
                    let fetched_at: i64 = row.get(1)?;
                    Ok((json, fetched_at))
                })?;
                let Some(row) = rows.next().transpose()? else {
                    return Ok(None);
                };
                let (json, fetched_at) = row;
                let metadata: GameMetadata = serde_json::from_str(&json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?;
                Ok(Some((metadata, fetched_at)))
            })
            .await
    }

    pub async fn put(&self, slug: &str, metadata: &GameMetadata) -> Result<(), DbError> {
        let slug = slug.to_string();
        let json = serde_json::to_string(metadata).map_err(|error| {
            DbError::Sqlite(rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
        })?;
        let fetched_at = metadata.fetched_at;
        self.db
            .with_conn(move |conn| {
                conn.execute(
                    "INSERT INTO metadata_cache (slug, json, fetched_at) VALUES (?1, ?2, ?3) \
                     ON CONFLICT(slug) DO UPDATE SET json = excluded.json, \
                     fetched_at = excluded.fetched_at",
                    rusqlite::params![slug, json, fetched_at],
                )?;
                Ok(())
            })
            .await
    }
}

/// Coordinates the provider and the persistent cache.
#[derive(Clone)]
pub struct MetadataService {
    provider: Arc<dyn MetadataProvider>,
    store: MetadataStore,
    ttl: Duration,
}

impl MetadataService {
    pub fn new(db: Db, provider: Arc<dyn MetadataProvider>) -> Self {
        Self {
            provider,
            store: MetadataStore::new(db),
            ttl: Duration::from_secs(METADATA_TTL_SECONDS as u64),
        }
    }

    pub fn provider_name(&self) -> &'static str {
        self.provider.name()
    }

    /// Returns fresh cache, otherwise fetches. If the network fails, an
    /// expired cached record is returned with `stale = true`.
    pub async fn load(&self, slug: &str, title: &str) -> Result<MetadataOutcome, MetadataError> {
        let cached = self.store.get(slug).await?;
        let now = now_unix();
        if let Some((metadata, fetched_at)) = &cached {
            if now.saturating_sub(*fetched_at) <= self.ttl.as_secs() as i64 {
                return Ok(MetadataOutcome {
                    metadata: metadata.clone(),
                    from_cache: true,
                    stale: false,
                });
            }
        }

        match self.provider.fetch(title).await {
            Ok(metadata) => {
                self.store.put(slug, &metadata).await?;
                Ok(MetadataOutcome {
                    metadata,
                    from_cache: false,
                    stale: false,
                })
            }
            Err(error) => match cached {
                Some((metadata, _)) => Ok(MetadataOutcome {
                    metadata,
                    from_cache: true,
                    stale: true,
                }),
                None => Err(error),
            },
        }
    }

    /// Clears every cached metadata record.
    pub async fn clear_all(&self) -> Result<(), DbError> {
        self.store
            .db
            .with_conn(|conn| {
                conn.execute("DELETE FROM metadata_cache", [])?;
                Ok(())
            })
            .await
    }

    /// Clears one cached record (used by the settings cache action).
    pub async fn clear(&self, slug: &str) -> Result<(), DbError> {
        let slug = slug.to_string();
        self.store
            .db
            .with_conn(move |conn| {
                conn.execute("DELETE FROM metadata_cache WHERE slug = ?1", [slug])?;
                Ok(())
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn summary_json() -> serde_json::Value {
        serde_json::json!({
            "title": "The Legend of Zelda: Ocarina of Time",
            "extract": "A 1998 action-adventure game.",
            "description": "1998 video game",
            "thumbnail": { "source": "https://upload.wikimedia.org/example.png" },
            "content_urls": { "desktop": { "page": "https://en.wikipedia.org/wiki/Ocarina_of_Time" } }
        })
    }

    fn client_for(server: &MockServer) -> HttpClient {
        HttpClient::for_tests([server.address().ip().to_string()]).expect("test client")
    }

    #[tokio::test]
    async fn wikipedia_fetch_and_cache_hit() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/rest_v1/page/summary/Ocarina_of_Time"))
            .respond_with(ResponseTemplate::new(200).set_body_json(summary_json()))
            .mount(&server)
            .await;
        let db = Db::open_in_memory().await.expect("db");
        let provider = Arc::new(WikipediaProvider::with_base(
            client_for(&server),
            format!("{}/api/rest_v1/page/summary/", server.uri()),
            "en".to_string(),
        ));
        let service = MetadataService::new(db, provider);
        let first = service
            .load("zelda64-recomp", "Ocarina_of_Time")
            .await
            .expect("first");
        assert!(!first.from_cache);
        assert_eq!(first.metadata.attribution, "Wikipédia (en) — CC BY-SA 4.0");
        assert!(first.metadata.summary.as_deref().unwrap().contains("1998"));

        let second = service
            .load("zelda64-recomp", "Ocarina_of_Time")
            .await
            .expect("second");
        assert!(second.from_cache);
        assert_eq!(second.metadata, first.metadata);
    }

    struct FailingProvider;

    #[async_trait]
    impl MetadataProvider for FailingProvider {
        fn name(&self) -> &'static str {
            "failing"
        }

        async fn fetch(&self, _title: &str) -> Result<GameMetadata, MetadataError> {
            Err(MetadataError::EmptyTitle)
        }
    }

    #[tokio::test]
    async fn expired_cache_is_used_when_network_fails() {
        let db = Db::open_in_memory().await.expect("db");
        let store = MetadataStore::new(db.clone());
        let mut metadata = GameMetadata::new("Cached".to_string());
        metadata.fetched_at = now_unix() - METADATA_TTL_SECONDS - 10;
        store.put("cached", &metadata).await.expect("put");

        let service = MetadataService::new(db, Arc::new(FailingProvider));
        let outcome = service.load("cached", "Cached").await.expect("stale");
        assert!(outcome.from_cache);
        assert!(outcome.stale);
        assert_eq!(outcome.metadata.title, "Cached");
    }

    #[test]
    fn summary_url_encodes_title() {
        let http = HttpClient::for_tests(["127.0.0.1".to_string()]).expect("client");
        let provider = WikipediaProvider::with_base(
            http,
            "https://en.wikipedia.org/api/rest_v1/page/summary/".to_string(),
            "en".to_string(),
        );
        let url = provider.summary_url("The Legend of Zelda").expect("url");
        assert!(url.contains("The%20Legend%20of%20Zelda"), "{url}");
    }
}
