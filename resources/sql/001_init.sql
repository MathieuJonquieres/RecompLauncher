PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS catalog_cache (
    provider_id TEXT NOT NULL,
    slug        TEXT NOT NULL,
    json        TEXT NOT NULL,
    fetched_at  INTEGER NOT NULL,
    PRIMARY KEY (provider_id, slug)
);

CREATE TABLE IF NOT EXISTS library_games (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    slug            TEXT NOT NULL UNIQUE,
    title           TEXT NOT NULL,
    install_path    TEXT,
    version         TEXT,
    installed_at    INTEGER NOT NULL,
    source          TEXT NOT NULL CHECK (source IN ('managed', 'manual', 'external')),
    last_played     INTEGER,
    playtime_s      INTEGER NOT NULL DEFAULT 0,
    external_exe    TEXT,
    external_args   TEXT,
    external_cwd    TEXT,
    user_file       TEXT
);

CREATE TABLE IF NOT EXISTS mods (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    game_id  INTEGER NOT NULL REFERENCES library_games(id) ON DELETE CASCADE,
    name     TEXT NOT NULL,
    path     TEXT NOT NULL,
    enabled  INTEGER NOT NULL DEFAULT 1,
    ord      INTEGER NOT NULL DEFAULT 0,
    UNIQUE (game_id, name)
);

CREATE TABLE IF NOT EXISTS servers (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    game_slug    TEXT NOT NULL,
    name         TEXT NOT NULL,
    address      TEXT NOT NULL,
    port         INTEGER NOT NULL,
    source       TEXT NOT NULL,
    last_ping_ms INTEGER,
    players      INTEGER,
    max_players  INTEGER,
    favorite     INTEGER NOT NULL DEFAULT 0,
    UNIQUE (game_slug, address, port)
);

CREATE TABLE IF NOT EXISTS friends (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    display_name TEXT NOT NULL,
    notes        TEXT
);

CREATE TABLE IF NOT EXISTS friend_handles (
    friend_id INTEGER NOT NULL REFERENCES friends(id) ON DELETE CASCADE,
    service   TEXT NOT NULL,
    handle    TEXT NOT NULL,
    PRIMARY KEY (friend_id, service)
);

CREATE TABLE IF NOT EXISTS metadata_cache (
    slug       TEXT PRIMARY KEY,
    json       TEXT NOT NULL,
    fetched_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    k TEXT PRIMARY KEY,
    v TEXT
);

CREATE INDEX IF NOT EXISTS idx_library_source ON library_games(source);
CREATE INDEX IF NOT EXISTS idx_mods_game ON mods(game_id);
CREATE INDEX IF NOT EXISTS idx_servers_game ON servers(game_slug);
