//! SQLite connection, migrations and small helpers.
//!
//! One `Connection` is shared behind a mutex; every operation runs inside
//! `spawn_blocking`, and the guard is released before returning to async code.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};
use thiserror::Error;

/// Database-layer error.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),

    #[error("migration error: {0}")]
    Migration(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("database task failed: {0}")]
    Join(String),
}

fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../resources/sql/001_init.sql"
        ))),
        M::up(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../resources/sql/002_catalog_meta.sql"
        ))),
    ])
}

/// Thread-safe handle to the launcher database.
#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    /// Opens (and migrates) a file database.
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = tokio::task::spawn_blocking(move || -> Result<Connection, DbError> {
            let mut conn = Connection::open(path)?;
            conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")?;
            migrations()
                .to_latest(&mut conn)
                .map_err(|error| DbError::Migration(error.to_string()))?;
            Ok(conn)
        })
        .await
        .map_err(|error| DbError::Join(error.to_string()))??;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// In-memory database for tests.
    pub async fn open_in_memory() -> Result<Self, DbError> {
        let conn = tokio::task::spawn_blocking(|| -> Result<Connection, DbError> {
            let mut conn = Connection::open_in_memory()?;
            conn.execute_batch("PRAGMA foreign_keys = ON;")?;
            migrations()
                .to_latest(&mut conn)
                .map_err(|error| DbError::Migration(error.to_string()))?;
            Ok(conn)
        })
        .await
        .map_err(|error| DbError::Join(error.to_string()))??;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Runs a blocking closure with exclusive access to the connection.
    pub async fn with_conn<F, T>(&self, operation: F) -> Result<T, DbError>
    where
        F: FnOnce(&mut Connection) -> rusqlite::Result<T> + Send + 'static,
        T: Send + 'static,
    {
        let conn = Arc::clone(&self.conn);
        tokio::task::spawn_blocking(move || {
            let mut guard = conn
                .lock()
                .map_err(|_| DbError::Join("database mutex poisoned".to_string()))?;
            operation(&mut guard).map_err(DbError::Sqlite)
        })
        .await
        .map_err(|error| DbError::Join(error.to_string()))?
    }

    /// Current `user_version`, i.e. the number of applied migrations.
    pub async fn schema_version(&self) -> Result<i64, DbError> {
        self.with_conn(|conn| conn.query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0)))
            .await
    }

    /// Test helper: migrate a fresh connection only up to `version`.
    #[cfg(test)]
    async fn migrate_to_for_test(path: &Path, version: usize) -> Result<(), DbError> {
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || -> Result<(), DbError> {
            let mut conn = Connection::open(path)?;
            migrations()
                .to_version(&mut conn, version)
                .map_err(|error| DbError::Migration(error.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|error| DbError::Join(error.to_string()))??;
        Ok(())
    }
}

/// Unix timestamp in seconds.
pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn migrations_create_schema() {
        let db = Db::open_in_memory().await.expect("db");
        assert_eq!(db.schema_version().await.expect("version"), 2);
        let tables: i64 = db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN \
                     ('catalog_cache','library_games','mods','servers','friends','friend_handles',\
                      'metadata_cache','settings','catalog_meta')",
                    [],
                    |row| row.get(0),
                )
            })
            .await
            .expect("count");
        assert_eq!(tables, 9);
    }

    #[tokio::test]
    async fn migrations_are_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("launcher.db");
        let db = Db::open(&path).await.expect("open");
        drop(db);
        let db = Db::open(&path).await.expect("reopen");
        assert_eq!(db.schema_version().await.expect("version"), 2);
    }

    #[tokio::test]
    async fn migrates_from_previous_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("old.db");
        Db::migrate_to_for_test(&path, 1).await.expect("v1");
        let db = Db::open(&path).await.expect("upgrade");
        assert_eq!(db.schema_version().await.expect("version"), 2);
        let exists: i64 = db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='catalog_meta'",
                    [],
                    |row| row.get(0),
                )
            })
            .await
            .expect("query");
        assert_eq!(exists, 1);
    }
}
