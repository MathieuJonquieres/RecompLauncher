//! Crate-wide error type used at module boundaries.

use thiserror::Error;

/// Errors that can cross a `core` module boundary.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CoreError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("database layer error: {0}")]
    DbLayer(#[from] crate::db::DbError),

    #[error("HTTP client error: {0}")]
    HttpClient(#[from] crate::net::http_client::HttpError),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("invalid path: {0}")]
    InvalidPath(String),

    #[error("invalid manifest: {0}")]
    InvalidManifest(String),

    #[error("operation cancelled")]
    Cancelled,
}

/// Convenient result alias for the crate.
pub type Result<T> = std::result::Result<T, CoreError>;
