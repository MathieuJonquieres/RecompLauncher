//! Manifest parsing and JSON Schema validation.
//!
//! Two layers: JSON Schema (Draft 2020-12) catches structural errors, then
//! `serde` deserialises into strong types and `GameEntry::validate` checks
//! cross-field invariants. Invalid entries are logged and skipped.

use std::path::PathBuf;

use async_trait::async_trait;
use jsonschema::JSONSchema;
use serde_json::Value;
use tracing::{debug, warn};

use super::model::{GameEntry, ManifestError};
use super::provider::{CatalogError, CatalogProvider, ProviderInfo};
use crate::net::http_client::HttpClient;

/// Embedded schema, kept in `resources/schema/game.schema.json`.
pub const GAME_SCHEMA_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../resources/schema/game.schema.json"
));

/// Curated fallback catalog, verified by hand (see docs/DATA_SOURCES.md).
pub const SEED_CATALOG_JSON: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../resources/catalog.seed.json"
));

/// Hard cap for local/remote catalog documents (protects against huge bodies).
pub const MAX_CATALOG_BYTES: usize = 8 * 1024 * 1024;

/// Outcome of parsing a catalog document.
#[derive(Debug, Clone, Default)]
pub struct ParsedCatalog {
    pub entries: Vec<GameEntry>,
    pub rejected: usize,
}

impl ParsedCatalog {
    pub fn total(&self) -> usize {
        self.entries.len() + self.rejected
    }
}

/// Reusable validator; compile once per refresh rather than per entry.
pub struct CatalogValidator {
    schema: JSONSchema,
}

impl CatalogValidator {
    pub fn new() -> Result<Self, CatalogError> {
        let schema: Value = serde_json::from_str(GAME_SCHEMA_JSON)
            .map_err(|error| CatalogError::InvalidDocument(format!("embedded schema: {error}")))?;
        let schema = JSONSchema::compile(&schema).map_err(|error| {
            CatalogError::InvalidDocument(format!("embedded schema does not compile: {error}"))
        })?;
        Ok(Self { schema })
    }

    /// Validates one entry against the JSON Schema, then deserialises it.
    pub fn parse_entry(&self, value: &Value) -> Result<GameEntry, String> {
        if let Err(errors) = self.schema.validate(value) {
            let messages = errors.map(|error| error.to_string()).collect::<Vec<_>>();
            return Err(messages.join("; "));
        }

        let entry: GameEntry = serde_json::from_value(value.clone())
            .map_err(|error| format!("strong typing: {error}"))?;
        entry.validate().map_err(|error| error.to_string())?;
        Ok(entry)
    }

    /// Accepts `{ "schema": 1, "entries": [...] }` or a bare array.
    pub fn parse_catalog(&self, value: &Value) -> Result<ParsedCatalog, CatalogError> {
        let entries = match value {
            Value::Array(entries) => entries,
            Value::Object(map) => {
                map.get("entries")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        CatalogError::InvalidDocument("missing `entries` array".to_string())
                    })?
            }
            _ => {
                return Err(CatalogError::InvalidDocument(
                    "catalog must be an object or an array".to_string(),
                ));
            }
        };

        let mut parsed = ParsedCatalog::default();
        for (index, raw) in entries.iter().enumerate() {
            match self.parse_entry(raw) {
                Ok(entry) => parsed.entries.push(entry),
                Err(reason) => {
                    parsed.rejected += 1;
                    warn!(index, reason = %reason, "skipping invalid catalog entry");
                }
            }
        }
        debug!(
            accepted = parsed.entries.len(),
            rejected = parsed.rejected,
            "catalog document parsed"
        );
        Ok(parsed)
    }

    pub fn parse_catalog_str(&self, document: &str) -> Result<ParsedCatalog, CatalogError> {
        if document.len() > MAX_CATALOG_BYTES {
            return Err(CatalogError::InvalidDocument(format!(
                "catalog exceeds {} bytes",
                MAX_CATALOG_BYTES
            )));
        }
        let value: Value = serde_json::from_str(document)
            .map_err(|error| CatalogError::InvalidDocument(error.to_string()))?;
        self.parse_catalog(&value)
    }
}

/// Convenience wrapper for callers that do not need to reuse the validator.
pub fn parse_catalog_str(document: &str) -> Result<ParsedCatalog, CatalogError> {
    CatalogValidator::new()?.parse_catalog_str(document)
}

/// Local, embedded or remote manifest provider.
#[derive(Debug, Clone)]
pub enum ManifestSource {
    Embedded,
    File(PathBuf),
    Remote { url: String, client: HttpClient },
}

/// Guaranteed fallback provider (spec §3.B).
pub struct ManifestProvider {
    info: ProviderInfo,
    source: ManifestSource,
}

impl ManifestProvider {
    pub fn embedded() -> Self {
        Self {
            info: ProviderInfo {
                id: "manifest-seed".to_string(),
                name: "Catalogue embarqué".to_string(),
                url: "embedded://catalog.seed.json".to_string(),
            },
            source: ManifestSource::Embedded,
        }
    }

    pub fn from_file(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        Self {
            info: ProviderInfo {
                id: "manifest-local".to_string(),
                name: "Manifeste local".to_string(),
                url: format!("file://{}", path.display()),
            },
            source: ManifestSource::File(path),
        }
    }

    /// Remote manifest fetched over HTTPS with ETag caching and a size cap.
    pub fn remote(client: HttpClient, url: impl Into<String>) -> Self {
        let url = url.into();
        Self {
            info: ProviderInfo {
                id: "manifest-remote".to_string(),
                name: "Manifeste distant".to_string(),
                url: url.clone(),
            },
            source: ManifestSource::Remote { url, client },
        }
    }

    async fn load_document(&self) -> Result<String, CatalogError> {
        match &self.source {
            ManifestSource::Embedded => Ok(SEED_CATALOG_JSON.to_string()),
            ManifestSource::File(path) => {
                let metadata = tokio::fs::metadata(path).await?;
                if metadata.len() > MAX_CATALOG_BYTES as u64 {
                    return Err(CatalogError::InvalidDocument(format!(
                        "{} exceeds {} bytes",
                        path.display(),
                        MAX_CATALOG_BYTES
                    )));
                }
                let bytes = tokio::fs::read(path).await?;
                String::from_utf8(bytes)
                    .map_err(|error| CatalogError::InvalidDocument(error.to_string()))
            }
            ManifestSource::Remote { url, client } => {
                let response = client.get_limited(url, MAX_CATALOG_BYTES).await?;
                response.text().map_err(|error| {
                    CatalogError::InvalidDocument(format!("remote manifest is not UTF-8: {error}"))
                })
            }
        }
    }
}

#[async_trait]
impl CatalogProvider for ManifestProvider {
    fn info(&self) -> ProviderInfo {
        self.info.clone()
    }

    async fn fetch_all(&self) -> Result<Vec<GameEntry>, CatalogError> {
        let document = self.load_document().await?;
        let parsed = CatalogValidator::new()?.parse_catalog_str(&document)?;
        let mut entries = parsed.entries;
        for entry in &mut entries {
            entry.provider = Some(self.info.id.clone());
        }
        Ok(entries)
    }
}

/// Converts a typed validation failure for callers that only have an entry.
pub fn validate_entry(entry: &GameEntry) -> Result<(), ManifestError> {
    entry.validate()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::test_support::entry_json;
    use serde_json::json;

    fn validator() -> CatalogValidator {
        CatalogValidator::new().expect("embedded schema compiles")
    }

    #[test]
    fn embedded_schema_compiles() {
        let _ = validator();
    }

    #[test]
    fn embedded_seed_is_valid_and_complete() {
        let parsed = parse_catalog_str(SEED_CATALOG_JSON).expect("seed parses");
        assert!(parsed.entries.len() >= 6, "expected curated entries");
        assert_eq!(parsed.rejected, 0);
        let mut slugs: Vec<_> = parsed
            .entries
            .iter()
            .map(|entry| entry.slug.as_str())
            .collect();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), parsed.entries.len(), "duplicate slugs");
        for entry in &parsed.entries {
            assert!(
                entry.supports_os(super::super::model::Os::Windows)
                    || entry.supports_os(super::super::model::Os::Linux),
                "{} has no desktop build",
                entry.slug
            );
        }
    }

    #[test]
    fn seed_slug_snapshot() {
        let parsed = parse_catalog_str(SEED_CATALOG_JSON).expect("seed parses");
        let slugs: Vec<_> = parsed
            .entries
            .iter()
            .map(|entry| entry.slug.as_str())
            .collect();
        insta::assert_debug_snapshot!("seed_slugs", slugs);
    }

    #[test]
    fn invalid_entry_is_skipped() {
        let document = json!({
            "schema": 1,
            "entries": [
                entry_json("valid-game"),
                { "schema": 1, "slug": "broken" }
            ]
        });
        let parsed = validator().parse_catalog(&document).expect("parses");
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(parsed.rejected, 1);
        assert_eq!(parsed.entries[0].slug.as_str(), "valid-game");
    }

    #[test]
    fn schema_rejects_bad_enum() {
        let mut value = entry_json("bad-kind");
        value["kind"] = json!("emulator");
        assert!(validator().parse_entry(&value).is_err());
    }

    #[test]
    fn schema_rejects_non_https_repo() {
        let mut value = entry_json("bad-repo");
        value["project"]["repo"] = json!("http://github.com/example/game");
        assert!(validator().parse_entry(&value).is_err());
    }

    #[test]
    fn unknown_fields_do_not_break_entries() {
        let mut value = entry_json("future-proof");
        value["future"] = json!({ "nested": [1, 2, 3] });
        assert!(validator().parse_entry(&value).is_ok());
    }

    #[tokio::test]
    async fn embedded_provider_tags_entries() {
        let provider = ManifestProvider::embedded();
        let entries = provider.fetch_all().await.expect("seed loads");
        assert!(!entries.is_empty());
        assert!(entries
            .iter()
            .all(|entry| entry.provider.as_deref() == Some("manifest-seed")));
    }

    #[tokio::test]
    async fn remote_provider_fetches_and_validates() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/catalog.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "schema": 1,
                "entries": [entry_json("remote-game")]
            })))
            .mount(&server)
            .await;

        let client = HttpClient::for_tests(["127.0.0.1".to_string()]).expect("client");
        let provider = ManifestProvider::remote(client, format!("{}/catalog.json", server.uri()));
        let entries = provider.fetch_all().await.expect("remote catalog");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].slug.as_str(), "remote-game");
        assert_eq!(entries[0].provider.as_deref(), Some("manifest-remote"));
    }
}
