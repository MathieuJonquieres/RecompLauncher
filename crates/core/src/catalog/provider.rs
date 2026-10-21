//! Catalog provider abstraction (spec §3).

use async_trait::async_trait;
use thiserror::Error;

use super::model::GameEntry;

/// Stable identity of a provider, shown in the UI and stored in the cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub url: String,
}

/// Errors a provider may return. One failing provider never aborts the whole
/// catalog refresh; the service aggregates errors.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CatalogError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("HTTP client error: {0}")]
    HttpClient(#[from] crate::net::http_client::HttpError),

    #[error("invalid catalog document: {0}")]
    InvalidDocument(String),

    #[error("provider is not configured: {0}")]
    NotConfigured(String),

    #[error("operation cancelled")]
    Cancelled,
}

/// A source of game manifests.
#[async_trait]
pub trait CatalogProvider: Send + Sync {
    fn info(&self) -> ProviderInfo;

    /// Returns the COMPLETE catalog. Pagination is handled internally.
    /// Invalid entries are skipped and logged, never fatal.
    async fn fetch_all(&self) -> Result<Vec<GameEntry>, CatalogError>;
}
