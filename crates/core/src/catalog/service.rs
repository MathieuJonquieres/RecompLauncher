//! Catalog aggregation and SQLite cache (spec §3).
//!
//! `refresh` queries every provider concurrently, deduplicates by slug, stores
//! successful providers in SQLite and always leaves a usable offline snapshot.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use futures::future::join_all;
use rusqlite::OptionalExtension;
use tracing::{info, warn};

use super::model::GameEntry;
use super::provider::{CatalogProvider, ProviderInfo};
use crate::db::{now_unix, Db, DbError};

/// Catalog state handed to the UI.
#[derive(Debug, Clone, Default)]
pub struct CatalogSnapshot {
    pub entries: Vec<GameEntry>,
    pub last_updated: Option<i64>,
    /// True when no provider succeeded and cached data is shown.
    pub offline: bool,
    pub errors: Vec<String>,
}

/// Aggregates providers and owns the cache policy.
pub struct CatalogService {
    providers: Vec<Arc<dyn CatalogProvider>>,
    db: Db,
    ttl_seconds: i64,
}

impl CatalogService {
    pub fn new(db: Db) -> Self {
        Self {
            providers: Vec::new(),
            db,
            ttl_seconds: 6 * 60 * 60,
        }
    }

    pub fn add_provider(&mut self, provider: Arc<dyn CatalogProvider>) {
        self.providers.push(provider);
    }

    pub fn providers(&self) -> Vec<ProviderInfo> {
        self.providers
            .iter()
            .map(|provider| provider.info())
            .collect()
    }

    pub fn set_ttl_hours(&mut self, hours: i64) {
        self.ttl_seconds = hours.max(1) * 60 * 60;
    }

    pub fn is_stale(&self, snapshot: &CatalogSnapshot) -> bool {
        match snapshot.last_updated {
            Some(timestamp) => now_unix().saturating_sub(timestamp) > self.ttl_seconds,
            None => true,
        }
    }

    /// Loads the last cached catalog (even expired). Never touches the network.
    pub async fn load_cached(&self) -> Result<CatalogSnapshot, DbError> {
        let rows: Vec<(String, String, i64)> = self
            .db
            .with_conn(|conn| {
                let mut statement = conn.prepare(
                    "SELECT provider_id, json, fetched_at FROM catalog_cache \
                     ORDER BY provider_id, slug",
                )?;
                let rows = statement
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await?;

        let mut entries = Vec::with_capacity(rows.len());
        let mut seen = HashSet::new();
        let mut last_updated = None;
        for (provider, json, fetched_at) in rows {
            last_updated =
                Some(last_updated.map_or(fetched_at, |current: i64| current.max(fetched_at)));
            match serde_json::from_str::<GameEntry>(&json) {
                Ok(mut entry) => {
                    if entry.provider.is_none() {
                        entry.provider = Some(provider);
                    }
                    if seen.insert(entry.slug.clone()) {
                        entries.push(entry);
                    }
                }
                Err(error) => warn!(%error, "ignoring corrupt cached entry"),
            }
        }

        Ok(CatalogSnapshot {
            entries,
            last_updated,
            offline: true,
            errors: Vec::new(),
        })
    }

    /// Fetches every provider, updates the cache and returns the merged view.
    pub async fn refresh(&self) -> Result<CatalogSnapshot, DbError> {
        let futures = self.providers.iter().map(|provider| {
            let provider = Arc::clone(provider);
            async move {
                let info = provider.info();
                let result = provider.fetch_all().await;
                (info, result)
            }
        });
        let results = join_all(futures).await;

        let mut errors = Vec::new();
        let mut successes = 0_usize;
        for (info, result) in results {
            match result {
                Ok(entries) => {
                    successes += 1;
                    if let Err(error) = self.save_provider(&info.id, &entries).await {
                        errors.push(format!("{}: cache write failed: {error}", info.name));
                    }
                    info!(provider = %info.id, count = entries.len(), "catalog provider refreshed");
                }
                Err(error) => {
                    warn!(provider = %info.id, %error, "catalog provider failed");
                    errors.push(format!("{}: {error}", info.name));
                }
            }
        }

        let mut snapshot = self.load_cached().await?;
        snapshot.offline = successes == 0;
        snapshot.errors = errors;
        Ok(snapshot)
    }

    /// Refreshes only when the cached snapshot is missing or older than the TTL.
    pub async fn refresh_if_stale(&self) -> Result<CatalogSnapshot, DbError> {
        let cached = self.load_cached().await?;
        if self.is_stale(&cached) {
            self.refresh().await
        } else {
            let mut cached = cached;
            cached.offline = false;
            Ok(cached)
        }
    }

    async fn save_provider(&self, provider_id: &str, entries: &[GameEntry]) -> Result<(), DbError> {
        let fetched_at = now_unix();
        let provider_id = provider_id.to_string();
        let rows: Vec<(String, String)> = entries
            .iter()
            .map(|entry| {
                serde_json::to_string(entry)
                    .map(|json| (entry.slug.as_str().to_string(), json))
                    .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))
            })
            .collect::<rusqlite::Result<_>>()?;

        self.db
            .with_conn(move |conn| {
                let transaction = conn.transaction()?;
                transaction.execute(
                    "DELETE FROM catalog_cache WHERE provider_id = ?1",
                    [&provider_id],
                )?;
                {
                    let mut insert = transaction.prepare(
                        "INSERT OR REPLACE INTO catalog_cache (provider_id, slug, json, fetched_at) \
                         VALUES (?1, ?2, ?3, ?4)",
                    )?;
                    for (slug, json) in &rows {
                        insert.execute(rusqlite::params![&provider_id, slug, json, fetched_at])?;
                    }
                }
                transaction.execute(
                    "INSERT INTO catalog_meta (provider_id, fetched_at, entry_count) \
                     VALUES (?1, ?2, ?3) \
                     ON CONFLICT(provider_id) DO UPDATE SET \
                       fetched_at = excluded.fetched_at, entry_count = excluded.entry_count",
                    rusqlite::params![&provider_id, fetched_at, rows.len() as i64],
                )?;
                transaction.commit()?;
                Ok(())
            })
            .await
    }

    /// Clears both catalog tables (Settings → "vider le cache").
    pub async fn clear_cache(&self) -> Result<(), DbError> {
        self.db
            .with_conn(|conn| {
                conn.execute_batch("DELETE FROM catalog_cache; DELETE FROM catalog_meta;")?;
                Ok(())
            })
            .await
    }

    /// Cache metadata for the UI ("dernière MAJ").
    pub async fn provider_status(&self) -> Result<Vec<(String, i64, i64)>, DbError> {
        self.db
            .with_conn(|conn| {
                let mut statement =
                    conn.prepare("SELECT provider_id, fetched_at, entry_count FROM catalog_meta")?;
                let rows = statement
                    .query_map([], |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                        ))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                Ok(rows)
            })
            .await
    }

    /// Returns one entry from the cache by slug.
    pub async fn find(&self, slug: &str) -> Result<Option<GameEntry>, DbError> {
        let slug = slug.to_string();
        let json: Option<String> = self
            .db
            .with_conn(move |conn| {
                conn.query_row(
                    "SELECT json FROM catalog_cache WHERE slug = ?1 LIMIT 1",
                    [&slug],
                    |row| row.get(0),
                )
                .optional()
            })
            .await?;
        Ok(json.and_then(|json| serde_json::from_str(&json).ok()))
    }

    /// Debounce helper for callers that want a blocking refresh policy.
    pub fn ttl(&self) -> Duration {
        Duration::from_secs(self.ttl_seconds as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::manifest::ManifestProvider;
    use crate::catalog::provider::CatalogError;
    use crate::catalog::test_support::minimal_entry;
    use async_trait::async_trait;

    struct StaticProvider {
        id: String,
        entries: Vec<GameEntry>,
        fail: bool,
    }

    #[async_trait]
    impl CatalogProvider for StaticProvider {
        fn info(&self) -> ProviderInfo {
            ProviderInfo {
                id: self.id.clone(),
                name: self.id.clone(),
                url: "test://provider".to_string(),
            }
        }

        async fn fetch_all(&self) -> Result<Vec<GameEntry>, CatalogError> {
            if self.fail {
                Err(CatalogError::NotConfigured("boom".to_string()))
            } else {
                Ok(self.entries.clone())
            }
        }
    }

    fn static_provider(id: &str, slugs: &[&str], fail: bool) -> Arc<dyn CatalogProvider> {
        Arc::new(StaticProvider {
            id: id.to_string(),
            entries: slugs.iter().map(|slug| minimal_entry(slug)).collect(),
            fail,
        })
    }

    #[tokio::test]
    async fn refresh_then_offline_cache() {
        let db = Db::open_in_memory().await.expect("db");
        let mut service = CatalogService::new(db.clone());
        service.add_provider(Arc::new(ManifestProvider::embedded()));
        let snapshot = service.refresh().await.expect("refresh");
        assert!(!snapshot.offline);
        assert!(snapshot.entries.len() >= 6);
        assert!(snapshot.last_updated.is_some());

        let offline = CatalogService::new(db)
            .load_cached()
            .await
            .expect("offline load");
        assert_eq!(offline.entries.len(), snapshot.entries.len());
        assert!(offline.offline);
    }

    #[tokio::test]
    async fn duplicate_slugs_are_deduplicated() {
        let db = Db::open_in_memory().await.expect("db");
        let mut service = CatalogService::new(db);
        service.add_provider(static_provider("a", &["same-game", "only-a"], false));
        service.add_provider(static_provider("b", &["same-game", "only-b"], false));
        let snapshot = service.refresh().await.expect("refresh");
        assert_eq!(snapshot.entries.len(), 3);
    }

    #[tokio::test]
    async fn failed_refresh_falls_back_to_cache() {
        let db = Db::open_in_memory().await.expect("db");
        let mut healthy = CatalogService::new(db.clone());
        healthy.add_provider(static_provider("healthy", &["kept"], false));
        healthy.refresh().await.expect("seed cache");

        let mut broken = CatalogService::new(db);
        broken.add_provider(static_provider("broken", &[], true));
        let snapshot = broken.refresh().await.expect("fallback");
        assert!(snapshot.offline);
        assert_eq!(snapshot.entries.len(), 1);
        assert_eq!(snapshot.errors.len(), 1);
    }

    #[tokio::test]
    async fn clear_cache_removes_entries() {
        let db = Db::open_in_memory().await.expect("db");
        let mut service = CatalogService::new(db);
        service.add_provider(static_provider("p", &["game"], false));
        service.refresh().await.expect("refresh");
        service.clear_cache().await.expect("clear");
        assert!(service
            .load_cached()
            .await
            .expect("load")
            .entries
            .is_empty());
    }

    #[tokio::test]
    async fn ttl_is_configurable() {
        let db = Db::open_in_memory().await.expect("db");
        let mut service = CatalogService::new(db);
        service.set_ttl_hours(12);
        assert_eq!(service.ttl(), Duration::from_secs(12 * 3600));
    }
}
