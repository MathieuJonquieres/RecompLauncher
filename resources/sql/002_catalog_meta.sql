CREATE TABLE IF NOT EXISTS catalog_meta (
    provider_id TEXT PRIMARY KEY,
    fetched_at  INTEGER NOT NULL,
    entry_count INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_catalog_cache_fetched ON catalog_cache(fetched_at);
CREATE INDEX IF NOT EXISTS idx_metadata_cache_fetched ON metadata_cache(fetched_at);
