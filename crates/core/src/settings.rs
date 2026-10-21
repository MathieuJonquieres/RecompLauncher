//! Typed key/value settings stored in SQLite (`settings` table).
//!
//! API tokens are NOT stored here: see the future `secrets` module (keyring).
//! Non-secret preferences only.

use crate::db::{Db, DbError};

pub const KEY_LANGUAGE: &str = "ui.language";
pub const KEY_THEME: &str = "ui.theme";
pub const KEY_INSTALL_DIR: &str = "install.dir";
pub const KEY_PROXY: &str = "network.proxy";
pub const KEY_CATALOG_MANIFEST_URL: &str = "catalog.manifest_url";
pub const KEY_RECOMP_SITE_URL: &str = "catalog.recomp_site_url";
pub const KEY_CATALOG_TTL_HOURS: &str = "catalog.ttl_hours";
pub const KEY_USER_FILE_PREFIX: &str = "userfile.";
pub const KEY_GITHUB_TOKEN: &str = "secrets.github_token";
pub const KEY_IGDB_CLIENT_ID: &str = "secrets.igdb_client_id";
pub const KEY_IGDB_TOKEN: &str = "secrets.igdb_token";

#[derive(Clone)]
pub struct Settings {
    db: Db,
}

impl Settings {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>, DbError> {
        let key = key.to_string();
        self.db
            .with_conn(move |conn| {
                conn.query_row("SELECT v FROM settings WHERE k = ?1", [&key], |row| {
                    row.get::<_, Option<String>>(0)
                })
                .or_else(|error| match error {
                    rusqlite::Error::QueryReturnedNoRows => Ok(None),
                    other => Err(other),
                })
            })
            .await
    }

    pub async fn get_or(&self, key: &str, fallback: &str) -> Result<String, DbError> {
        Ok(self.get(key).await?.unwrap_or_else(|| fallback.to_string()))
    }

    pub async fn set(&self, key: &str, value: &str) -> Result<(), DbError> {
        let key = key.to_string();
        let value = value.to_string();
        self.db
            .with_conn(move |conn| {
                conn.execute(
                    "INSERT INTO settings (k, v) VALUES (?1, ?2) \
                     ON CONFLICT(k) DO UPDATE SET v = excluded.v",
                    [&key, &value],
                )?;
                Ok(())
            })
            .await
    }

    pub async fn remove(&self, key: &str) -> Result<(), DbError> {
        let key = key.to_string();
        self.db
            .with_conn(move |conn| {
                conn.execute("DELETE FROM settings WHERE k = ?1", [&key])?;
                Ok(())
            })
            .await
    }

    pub async fn get_bool(&self, key: &str, fallback: bool) -> Result<bool, DbError> {
        Ok(match self.get(key).await? {
            Some(value) => matches!(value.as_str(), "1" | "true" | "yes" | "on"),
            None => fallback,
        })
    }

    pub async fn set_bool(&self, key: &str, value: bool) -> Result<(), DbError> {
        self.set(key, if value { "1" } else { "0" }).await
    }

    pub async fn get_i64(&self, key: &str, fallback: i64) -> Result<i64, DbError> {
        Ok(self
            .get(key)
            .await?
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(fallback))
    }

    /// Stores the path of a user-provided file. The file itself is never copied.
    pub async fn set_user_file(&self, slug: &str, path: &str) -> Result<(), DbError> {
        self.set(&format!("{KEY_USER_FILE_PREFIX}{slug}"), path)
            .await
    }

    pub async fn user_file(&self, slug: &str) -> Result<Option<String>, DbError> {
        self.get(&format!("{KEY_USER_FILE_PREFIX}{slug}")).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn settings_roundtrip() {
        let db = Db::open_in_memory().await.expect("db");
        let settings = Settings::new(db);
        assert_eq!(settings.get("missing").await.expect("get"), None);
        settings.set("ui.theme", "dark").await.expect("set");
        assert_eq!(
            settings.get("ui.theme").await.expect("get"),
            Some("dark".to_string())
        );
        settings.set("ui.theme", "light").await.expect("upsert");
        assert_eq!(
            settings.get_or("ui.theme", "dark").await.expect("get"),
            "light"
        );
    }

    #[tokio::test]
    async fn booleans_and_integers() {
        let db = Db::open_in_memory().await.expect("db");
        let settings = Settings::new(db);
        settings.set_bool("flag", true).await.expect("set");
        assert!(settings.get_bool("flag", false).await.expect("get"));
        settings.set("ttl", "12").await.expect("set");
        assert_eq!(settings.get_i64("ttl", 6).await.expect("get"), 12);
        assert_eq!(settings.get_i64("missing", 6).await.expect("get"), 6);
    }

    #[tokio::test]
    async fn user_file_paths_are_per_slug() {
        let db = Db::open_in_memory().await.expect("db");
        let settings = Settings::new(db);
        settings
            .set_user_file("zelda64-recomp", "/dumps/mm.z64")
            .await
            .expect("set");
        assert_eq!(
            settings.user_file("zelda64-recomp").await.expect("get"),
            Some("/dumps/mm.z64".to_string())
        );
        assert_eq!(settings.user_file("other").await.expect("get"), None);
    }
}
