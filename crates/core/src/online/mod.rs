//! Online server discovery, manual servers, pings and the local friends book.
//!
//! Only games whose manifest declares `online.supported = true` expose these
//! features. LAN discovery is best-effort mDNS; manual `host:port` entries
//! always work and never require a third-party account.

use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures::stream::{self, StreamExt};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::catalog::model::{GameEntry, OnlineSpec};
use crate::db::{now_unix, Db, DbError};
use crate::net::http_client::{HttpClient, HttpError};

pub const PING_TIMEOUT: Duration = Duration::from_secs(2);
pub const MAX_CONCURRENT_PINGS: usize = 16;
pub const DEFAULT_LAN_SERVICE: &str = "_recomp._udp.local.";
const MAX_SERVER_DOCUMENT_BYTES: usize = 256 * 1024;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum OnlineError {
    #[error("online HTTP error: {0}")]
    Http(#[from] HttpError),

    #[error("online database error: {0}")]
    Db(#[from] DbError),

    #[error("invalid server address: {0}")]
    InvalidAddress(String),

    #[error("LAN discovery error: {0}")]
    Lan(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ServerSource {
    Manifest,
    Manual,
    Lan,
}

impl ServerSource {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Manifest => "Liste du projet",
            Self::Manual => "Ajout manuel",
            Self::Lan => "Découverte LAN",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    #[serde(default)]
    pub game_slug: String,
    pub name: String,
    pub address: String,
    pub port: u16,
    #[serde(default = "default_source")]
    pub source: ServerSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_ping_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub players: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_players: Option<u32>,
    #[serde(default)]
    pub favorite: bool,
}

fn default_source() -> ServerSource {
    ServerSource::Manifest
}

impl ServerInfo {
    pub fn endpoint(&self) -> String {
        format!("{}:{}", self.address, self.port)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerList {
    pub servers: Vec<ServerInfo>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ServerDocument {
    #[serde(default)]
    servers: Vec<ServerInfo>,
}

/// Provider contract for a project-maintained public server list.
#[async_trait]
pub trait ServerProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn list(&self, game_slug: &str) -> Result<Vec<ServerInfo>, OnlineError>;
}

pub struct JsonServerProvider {
    http: HttpClient,
    url: String,
}

impl JsonServerProvider {
    pub fn new(http: HttpClient, url: String) -> Self {
        Self { http, url }
    }
}

#[async_trait]
impl ServerProvider for JsonServerProvider {
    fn name(&self) -> &'static str {
        "json-server-list"
    }

    async fn list(&self, game_slug: &str) -> Result<Vec<ServerInfo>, OnlineError> {
        let document: ServerDocument = self
            .http
            .get_json(&self.url, MAX_SERVER_DOCUMENT_BYTES)
            .await?;
        let mut servers = document.servers;
        for server in &mut servers {
            if server.game_slug.is_empty() {
                server.game_slug = game_slug.to_string();
            }
            server.source = ServerSource::Manifest;
        }
        servers.retain(|server| server.game_slug == game_slug);
        Ok(servers)
    }
}

/// Persistent manual server entries.
#[derive(Clone)]
pub struct ServerStore {
    db: Db,
}

impl ServerStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn list(&self, game_slug: &str) -> Result<Vec<ServerInfo>, DbError> {
        let game_slug = game_slug.to_string();
        self.db
            .with_conn(move |conn| {
                let mut statement = conn.prepare(
                    "SELECT id, game_slug, name, address, port, last_ping_ms, players, \
                     max_players, favorite FROM servers WHERE game_slug = ?1 \
                     ORDER BY favorite DESC, name COLLATE NOCASE",
                )?;
                let rows = statement.query_map([game_slug], |row| {
                    Ok(ServerInfo {
                        id: Some(row.get(0)?),
                        game_slug: row.get(1)?,
                        name: row.get(2)?,
                        address: row.get(3)?,
                        port: row.get::<_, i64>(4)? as u16,
                        source: ServerSource::Manual,
                        last_ping_ms: row
                            .get::<_, Option<i64>>(5)?
                            .map(|value| value.max(0) as u64),
                        players: row
                            .get::<_, Option<i64>>(6)?
                            .map(|value| value.max(0) as u32),
                        max_players: row
                            .get::<_, Option<i64>>(7)?
                            .map(|value| value.max(0) as u32),
                        favorite: row.get::<_, i64>(8)? != 0,
                    })
                })?;
                rows.collect()
            })
            .await
    }

    pub async fn add(&self, server: &ServerInfo) -> Result<ServerInfo, OnlineError> {
        validate_endpoint(&server.address, server.port)?;
        let server = server.clone();
        self.db
            .with_conn(move |conn| {
                conn.execute(
                    "INSERT INTO servers \
                     (game_slug, name, address, port, source, last_ping_ms, players, max_players, favorite) \
                     VALUES (?1, ?2, ?3, ?4, 'manual', ?5, ?6, ?7, ?8) \
                     ON CONFLICT(game_slug, address, port) DO UPDATE SET \
                       name = excluded.name, favorite = excluded.favorite",
                    rusqlite::params![
                        server.game_slug,
                        server.name,
                        server.address,
                        i64::from(server.port),
                        server.last_ping_ms.map(|value| value as i64),
                        server.players.map(i64::from),
                        server.max_players.map(i64::from),
                        server.favorite as i64,
                    ],
                )?;
                let id = conn.query_row(
                    "SELECT id FROM servers WHERE game_slug = ?1 AND address = ?2 AND port = ?3",
                    rusqlite::params![server.game_slug, server.address, i64::from(server.port)],
                    |row| row.get(0),
                )?;
                Ok(ServerInfo { id: Some(id), ..server })
            })
            .await
            .map_err(OnlineError::from)
    }

    pub async fn remove(&self, id: i64) -> Result<bool, DbError> {
        self.db
            .with_conn(move |conn| {
                let changed = conn.execute("DELETE FROM servers WHERE id = ?1", [id])?;
                Ok(changed > 0)
            })
            .await
    }

    pub async fn update_ping(&self, id: i64, ping_ms: Option<u64>) -> Result<(), DbError> {
        self.db
            .with_conn(move |conn| {
                conn.execute(
                    "UPDATE servers SET last_ping_ms = ?2 WHERE id = ?1",
                    rusqlite::params![id, ping_ms.map(|value| value as i64)],
                )?;
                Ok(())
            })
            .await
    }

    pub async fn clear(&self) -> Result<(), DbError> {
        self.db
            .with_conn(|conn| {
                conn.execute("DELETE FROM servers", [])?;
                Ok(())
            })
            .await
    }
}

/// Aggregates manifest, manual and LAN servers for a game.
#[derive(Clone)]
pub struct OnlineService {
    store: ServerStore,
    provider: Option<Arc<dyn ServerProvider>>,
    lan_service: String,
}

impl OnlineService {
    pub fn new(db: Db, provider: Option<Arc<dyn ServerProvider>>, lan_service: String) -> Self {
        Self {
            store: ServerStore::new(db),
            provider,
            lan_service,
        }
    }

    pub fn store(&self) -> &ServerStore {
        &self.store
    }

    pub async fn list(&self, game_slug: &str) -> ServerList {
        let mut result = ServerList::default();
        match self.store.list(game_slug).await {
            Ok(servers) => result.servers.extend(servers),
            Err(error) => result.errors.push(error.to_string()),
        }
        if let Some(provider) = &self.provider {
            match provider.list(game_slug).await {
                Ok(servers) => result.servers.extend(servers),
                Err(error) => result.errors.push(error.to_string()),
            }
        }
        dedupe(&mut result.servers);
        result
    }

    pub async fn add_manual(&self, server: &ServerInfo) -> Result<ServerInfo, OnlineError> {
        self.store.add(server).await
    }

    pub async fn remove_manual(&self, id: i64) -> Result<bool, OnlineError> {
        Ok(self.store.remove(id).await?)
    }

    /// Pings every server with a two-second timeout, at most sixteen at once.
    pub async fn ping_all(&self, game_slug: &str) -> ServerList {
        let mut list = self.list(game_slug).await;
        let servers = list.servers.clone();
        let pings = stream::iter(servers.into_iter().map(|server| async move {
            let ping = ping_server(&server.address, server.port).await;
            (server, ping)
        }))
        .buffer_unordered(MAX_CONCURRENT_PINGS)
        .collect::<Vec<_>>()
        .await;

        for (server, ping) in pings {
            if let (Some(id), Some(_)) = (server.id, ping) {
                if let Err(error) = self.store.update_ping(id, ping).await {
                    list.errors.push(error.to_string());
                }
            }
            if let Some(item) = list
                .servers
                .iter_mut()
                .find(|item| item.address == server.address && item.port == server.port)
            {
                item.last_ping_ms = ping;
            }
        }
        list
    }

    /// Browses mDNS for the configured service type for two seconds and merges
    /// the results with the manual/public list.
    pub async fn discover_lan(&self, game_slug: &str) -> ServerList {
        let mut list = self.list(game_slug).await;
        match discover_lan(&self.lan_service, Duration::from_secs(2)).await {
            Ok(servers) => {
                for mut server in servers {
                    server.game_slug = game_slug.to_string();
                    list.servers.push(server);
                }
            }
            Err(error) => list.errors.push(error.to_string()),
        }
        dedupe(&mut list.servers);
        list
    }
}

fn dedupe(servers: &mut Vec<ServerInfo>) {
    let mut seen = HashSet::new();
    servers.retain(|server| seen.insert((server.address.clone(), server.port)));
}

/// Joins a server using the manifest template when one is declared.
pub fn join_args(online: &OnlineSpec, address: &str, port: u16) -> Vec<String> {
    let Some(template) = &online.join_template else {
        return Vec::new();
    };
    let endpoint = format!("{address}:{port}");
    template
        .split_whitespace()
        .map(|token| {
            token
                .replace("{address}", address)
                .replace("{port}", &port.to_string())
                .replace("{endpoint}", &endpoint)
        })
        .collect()
}

/// Returns the manifest join arguments for a catalogue entry.
pub fn join_entry_args(entry: &GameEntry, address: &str, port: u16) -> Vec<String> {
    join_args(&entry.online, address, port)
}

/// TCP connect latency, `None` after the two-second timeout.
pub async fn ping_server(address: &str, port: u16) -> Option<u64> {
    if validate_endpoint(address, port).is_err() {
        return None;
    }
    let start = Instant::now();
    match tokio::time::timeout(
        PING_TIMEOUT,
        tokio::net::TcpStream::connect((address, port)),
    )
    .await
    {
        Ok(Ok(_)) => Some(start.elapsed().as_millis().min(u128::from(u64::MAX)) as u64),
        _ => None,
    }
}

fn validate_endpoint(address: &str, port: u16) -> Result<(), OnlineError> {
    let address = address.trim();
    if address.is_empty() || port == 0 {
        return Err(OnlineError::InvalidAddress(format!("{address}:{port}")));
    }
    if address.chars().any(|ch| ch.is_whitespace()) {
        return Err(OnlineError::InvalidAddress(address.to_string()));
    }
    Ok(())
}

/// Best-effort mDNS browse. Returns an empty list when no service answers.
pub async fn discover_lan(
    service_type: &str,
    duration: Duration,
) -> Result<Vec<ServerInfo>, OnlineError> {
    use mdns_sd::{ServiceDaemon, ServiceEvent};

    let daemon = ServiceDaemon::new().map_err(|error| OnlineError::Lan(error.to_string()))?;
    let receiver = daemon
        .browse(service_type)
        .map_err(|error| OnlineError::Lan(error.to_string()))?;
    let deadline = tokio::time::Instant::now() + duration;
    let mut servers = Vec::new();
    loop {
        tokio::select! {
            _ = tokio::time::sleep_until(deadline) => break,
            event = receiver.recv_async() => {
                match event {
                    Ok(ServiceEvent::ServiceResolved(info)) => {
                        let port = info.get_port();
                        let address = info
                            .get_addresses()
                            .iter()
                            .find(|address| !address.is_loopback())
                            .or_else(|| info.get_addresses().iter().next())
                            .map(IpAddr::to_string);
                        if let Some(address) = address {
                            servers.push(ServerInfo {
                                id: None,
                                game_slug: String::new(),
                                name: info.get_fullname().trim_end_matches('.').to_string(),
                                address,
                                port,
                                source: ServerSource::Lan,
                                last_ping_ms: None,
                                players: None,
                                max_players: None,
                                favorite: false,
                            });
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
        }
    }
    let _ = daemon.shutdown();
    Ok(servers)
}

/// Local friends book. No account is ever created or uploaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Friend {
    pub id: i64,
    pub display_name: String,
    pub notes: Option<String>,
    pub handles: Vec<(String, String)>,
}

#[derive(Clone)]
pub struct FriendsStore {
    db: Db,
}

impl FriendsStore {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn list(&self) -> Result<Vec<Friend>, DbError> {
        self.db
            .with_conn(|conn| {
                let mut statement = conn.prepare(
                    "SELECT id, display_name, notes FROM friends ORDER BY display_name COLLATE NOCASE",
                )?;
                let rows = statement.query_map([], |row| {
                    Ok(Friend {
                        id: row.get(0)?,
                        display_name: row.get(1)?,
                        notes: row.get(2)?,
                        handles: Vec::new(),
                    })
                })?;
                let mut friends: Vec<Friend> = rows.collect::<rusqlite::Result<_>>()?;
                for friend in &mut friends {
                    let mut handles = conn.prepare(
                        "SELECT service, handle FROM friend_handles WHERE friend_id = ?1 \
                         ORDER BY service",
                    )?;
                    friend.handles = handles
                        .query_map([friend.id], |row| Ok((row.get(0)?, row.get(1)?)))?
                        .collect::<rusqlite::Result<_>>()?;
                }
                Ok(friends)
            })
            .await
    }

    pub async fn add(
        &self,
        display_name: &str,
        notes: Option<&str>,
        handles: Vec<(String, String)>,
    ) -> Result<Friend, DbError> {
        let display_name = display_name.trim().to_string();
        let notes = notes.map(str::to_string);
        self.db
            .with_conn(move |conn| {
                conn.execute(
                    "INSERT INTO friends (display_name, notes) VALUES (?1, ?2)",
                    rusqlite::params![display_name, notes],
                )?;
                let id = conn.last_insert_rowid();
                for (service, handle) in &handles {
                    conn.execute(
                        "INSERT INTO friend_handles (friend_id, service, handle) VALUES (?1, ?2, ?3)",
                        rusqlite::params![id, service, handle],
                    )?;
                }
                Ok(Friend {
                    id,
                    display_name,
                    notes,
                    handles,
                })
            })
            .await
    }

    pub async fn remove(&self, id: i64) -> Result<bool, DbError> {
        self.db
            .with_conn(move |conn| {
                let changed = conn.execute("DELETE FROM friends WHERE id = ?1", [id])?;
                Ok(changed > 0)
            })
            .await
    }
}

/// Utility used by settings to report cache age without touching the DB from
/// the UI thread.
pub fn age_seconds(fetched_at: i64) -> i64 {
    now_unix().saturating_sub(fetched_at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::test_support::minimal_entry;

    #[tokio::test]
    async fn manual_servers_are_persisted_and_deduped() {
        let db = Db::open_in_memory().await.expect("db");
        let service = OnlineService::new(db, None, DEFAULT_LAN_SERVICE.to_string());
        let server = ServerInfo {
            id: None,
            game_slug: "game".to_string(),
            name: "Local".to_string(),
            address: "127.0.0.1".to_string(),
            port: 7777,
            source: ServerSource::Manual,
            last_ping_ms: None,
            players: None,
            max_players: None,
            favorite: true,
        };
        let saved = service.add_manual(&server).await.expect("add");
        assert!(saved.id.is_some());
        service.add_manual(&server).await.expect("upsert");
        let list = service.list("game").await;
        assert_eq!(list.servers.len(), 1);
        assert!(list.servers[0].favorite);
        assert!(service
            .remove_manual(saved.id.expect("id"))
            .await
            .expect("remove"));
    }

    #[tokio::test]
    async fn ping_reports_open_and_closed_ports() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let port = listener.local_addr().expect("addr").port();
        assert!(ping_server("127.0.0.1", port).await.is_some());
        drop(listener);
        assert!(ping_server("127.0.0.1", port).await.is_none());
        assert!(ping_server("", 0).await.is_none());
    }

    #[test]
    fn join_args_use_manifest_template() {
        let entry = minimal_entry("online-game");
        let mut online = entry.online.clone();
        online.join_template = Some("{address} {port} {endpoint}".to_string());
        assert_eq!(
            join_args(&online, "10.0.0.2", 7777),
            vec!["10.0.0.2", "7777", "10.0.0.2:7777"]
        );
    }

    #[tokio::test]
    async fn json_provider_filters_by_game_slug() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/servers.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "servers": [
                    { "game_slug": "game", "name": "Public", "address": "203.0.113.1", "port": 7777 },
                    { "game_slug": "other", "name": "Other", "address": "203.0.113.2", "port": 7777 }
                ]
            })))
            .mount(&server)
            .await;
        let http = HttpClient::for_tests([server.address().ip().to_string()]).expect("client");
        let provider = JsonServerProvider::new(http, format!("{}/servers.json", server.uri()));
        let servers = provider.list("game").await.expect("list");
        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].name, "Public");
        assert_eq!(servers[0].source, ServerSource::Manifest);
    }

    #[tokio::test]
    async fn friends_book_roundtrip() {
        let db = Db::open_in_memory().await.expect("db");
        let friends = FriendsStore::new(db);
        let friend = friends
            .add(
                "Alice",
                Some("local"),
                vec![("discord".to_string(), "alice#1".to_string())],
            )
            .await
            .expect("add");
        let list = friends.list().await.expect("list");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].handles[0].1, "alice#1");
        assert!(friends.remove(friend.id).await.expect("remove"));
        assert!(friends.list().await.expect("list").is_empty());
    }
}
