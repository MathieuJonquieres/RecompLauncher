//! UI views. Each view renders real application state and sends commands
//! through the backend bridge; no demo data is fabricated here.

pub mod catalog;
pub mod game_detail;
pub mod install_modal;
pub mod library;
pub mod settings;
pub mod uninstall_modal;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;

use recomp_core::catalog::model::GameEntry;
use recomp_core::library::{LibraryGame, ModRecord};
use recomp_core::metadata::GameMetadata;
use recomp_core::mods::ModsSnapshot;
use recomp_core::online::ServerList;

use crate::backend_bridge::StorageStats;

/// Mods state for one game, shared by the catalog detail and library pages.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModsUiState {
    pub supported: bool,
    pub strategy: String,
    pub dir_name: String,
    pub dir: PathBuf,
    pub mods: Vec<ModRecord>,
    pub error: Option<String>,
    pub loading: bool,
}

impl ModsUiState {
    pub fn from_snapshot(snapshot: ModsSnapshot) -> Self {
        Self {
            supported: snapshot.supported,
            strategy: snapshot.strategy.label().to_string(),
            dir_name: snapshot.dir_name,
            dir: snapshot.dir,
            mods: snapshot.mods,
            error: None,
            loading: false,
        }
    }

    pub fn loading() -> Self {
        Self {
            loading: true,
            ..Self::default()
        }
    }
}

/// Public encyclopedic metadata state for one game.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MetadataUiState {
    pub metadata: Option<GameMetadata>,
    pub from_cache: bool,
    pub stale: bool,
    pub error: Option<String>,
    pub loading: bool,
}

impl MetadataUiState {
    pub fn loading() -> Self {
        Self {
            loading: true,
            ..Self::default()
        }
    }
}

/// Online server state for one game.
#[derive(Debug, Clone)]
pub struct ServersUiState {
    pub list: ServerList,
    pub loading: bool,
}

impl Default for ServersUiState {
    fn default() -> Self {
        Self {
            list: ServerList {
                servers: Vec::new(),
                errors: Vec::new(),
            },
            loading: false,
        }
    }
}

/// Download progress displayed by the catalog and library pages.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct InstallProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub repairing: bool,
}

impl InstallProgress {
    pub fn fraction(self) -> Option<f32> {
        match self.total {
            Some(total) if total > 0 => {
                Some((self.downloaded as f32 / total as f32).clamp(0.0, 1.0))
            }
            _ => None,
        }
    }
}

/// Everything the backend has told us about games.
#[derive(Debug, Default)]
pub struct Store {
    pub entries: Vec<GameEntry>,
    pub library: Vec<LibraryGame>,
    pub metadata: HashMap<String, MetadataUiState>,
    pub mods: HashMap<String, ModsUiState>,
    pub servers: HashMap<String, ServersUiState>,
    pub installs: HashMap<String, InstallProgress>,
    pub stats: StorageStats,
    pub refreshing: bool,
    pub offline: bool,
    pub last_updated: Option<i64>,
    pub errors: Vec<String>,
    requested_metadata: HashSet<String>,
    requested_mods: HashSet<String>,
    requested_servers: HashSet<String>,
}

impl Store {
    pub fn entry(&self, slug: &str) -> Option<&GameEntry> {
        self.entries
            .iter()
            .find(|entry| entry.slug.as_str() == slug)
    }

    pub fn library_game(&self, slug: &str) -> Option<&LibraryGame> {
        self.library.iter().find(|game| game.slug == slug)
    }

    pub fn is_installed(&self, slug: &str) -> bool {
        self.library_game(slug).is_some()
    }

    pub fn playing(&self, slug: &str) -> Option<InstallProgress> {
        self.installs.get(slug).copied()
    }

    /// True when the game is installed but still requires its user-supplied
    /// file before it can be launched.
    pub fn needs_user_file(&self, slug: &str) -> bool {
        self.entry(slug)
            .and_then(|entry| entry.user_files.as_ref())
            .is_some_and(|user_files| user_files.required)
            && self.library_game(slug).is_some()
            && self
                .library_game(slug)
                .is_some_and(|game| game.user_file.is_none())
    }

    pub fn metadata(&self, slug: &str) -> Option<&MetadataUiState> {
        self.metadata.get(slug)
    }

    pub fn mods(&self, slug: &str) -> Option<&ModsUiState> {
        self.mods.get(slug)
    }

    pub fn servers(&self, slug: &str) -> Option<&ServersUiState> {
        self.servers.get(slug)
    }

    /// Marks metadata as loading unless a request is already in flight or
    /// cached. Returns true when the caller should send `LoadMetadata`.
    pub fn request_metadata(&mut self, slug: &str) -> bool {
        if self.metadata.contains_key(slug) || !self.requested_metadata.insert(slug.to_owned()) {
            return false;
        }
        self.metadata
            .insert(slug.to_owned(), MetadataUiState::loading());
        true
    }

    pub fn request_mods(&mut self, slug: &str) -> bool {
        if self.mods.contains_key(slug) || !self.requested_mods.insert(slug.to_owned()) {
            return false;
        }
        self.mods.insert(slug.to_owned(), ModsUiState::loading());
        true
    }

    pub fn request_servers(&mut self, slug: &str) -> bool {
        if self.servers.contains_key(slug) || !self.requested_servers.insert(slug.to_owned()) {
            return false;
        }
        self.servers.insert(
            slug.to_owned(),
            ServersUiState {
                loading: true,
                ..ServersUiState::default()
            },
        );
        true
    }

    pub fn apply_snapshot(&mut self, snapshot: recomp_core::catalog::service::CatalogSnapshot) {
        self.entries = snapshot.entries;
        self.offline = snapshot.offline;
        self.last_updated = snapshot.last_updated;
        self.errors = snapshot.errors;
    }

    pub fn apply_library(&mut self, games: Vec<LibraryGame>) {
        // Install progress is owned by the install events (`InstallStarted`,
        // `InstallFinished`, `InstallFailed`); a library refresh must not clear
        // a download or repair that is still running.
        self.library = games;
    }

    pub fn apply_metadata(
        &mut self,
        slug: String,
        metadata: Option<GameMetadata>,
        from_cache: bool,
        stale: bool,
        error: Option<String>,
    ) {
        self.metadata.insert(
            slug,
            MetadataUiState {
                metadata,
                from_cache,
                stale,
                error,
                loading: false,
            },
        );
    }

    pub fn apply_mods(&mut self, slug: String, snapshot: ModsSnapshot) {
        self.mods.insert(slug, ModsUiState::from_snapshot(snapshot));
    }

    pub fn mods_failed(&mut self, slug: &str, message: String) {
        let state = self.mods.entry(slug.to_owned()).or_default();
        state.loading = false;
        state.error = Some(message);
    }

    pub fn apply_servers(&mut self, slug: String, list: ServerList) {
        self.servers.insert(
            slug,
            ServersUiState {
                list,
                loading: false,
            },
        );
    }

    pub fn total_playtime(&self) -> u64 {
        self.library.iter().map(|game| game.playtime_s).sum()
    }
}

/// Tells the shell which high-level transition the user asked for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    OpenGame { slug: String, from_library: bool },
    BackToLibrary,
    Install(String),
    Uninstall(String),
    Play(String),
    SelectUserFile(String),
    RefreshCatalog,
    BrowseCatalog,
}

/// Shared refresh delay used by the catalog skeleton and file checks.
pub const REFRESH_SKELETON: std::time::Duration = std::time::Duration::from_secs(1);
pub const FILE_CHECK_MINIMUM: std::time::Duration = std::time::Duration::from_millis(1200);

pub fn remaining_until(deadline: Option<Instant>) -> Option<std::time::Duration> {
    deadline.and_then(|deadline| deadline.checked_duration_since(Instant::now()))
}
