//! Per-game adapters for the rare cases that need logic beyond a manifest.
//!
//! Adapters are registered by slug in an [`AdapterRegistry`]; launch code
//! never contains `if slug == "..."` branches.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use thiserror::Error;

use crate::catalog::model::GameEntry;
use crate::library::LibraryGame;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AdapterError {
    #[error("adapter `{0}` failed: {1}")]
    Failed(String, String),
}

/// Mutable launch context handed to adapters.
pub struct AdapterContext<'a> {
    pub entry: &'a GameEntry,
    pub game: &'a LibraryGame,
    pub args: Vec<String>,
    pub cwd: std::path::PathBuf,
}

/// Optional server status returned by a game-specific parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerStatus {
    pub players: Option<u32>,
    pub max_players: Option<u32>,
    pub name: Option<String>,
}

/// Hooks called by the launcher and installer. Every hook has a safe default.
pub trait GameAdapter: Send + Sync {
    /// Called before the process starts. The adapter may mutate args/cwd.
    fn pre_launch(&self, _context: &mut AdapterContext<'_>) -> Result<(), AdapterError> {
        Ok(())
    }

    /// Extra arguments appended after manifest and mod arguments.
    fn build_args(&self, _context: &AdapterContext<'_>) -> Vec<String> {
        Vec::new()
    }

    /// Called after a successful installation.
    fn post_install(&self, _slug: &str, _install_dir: &Path) -> Result<(), AdapterError> {
        Ok(())
    }

    /// Parses a game-specific server status payload, if the game exposes one.
    fn parse_server_status(&self, _raw: &str) -> Option<ServerStatus> {
        None
    }
}

/// Registry keyed by catalogue slug.
#[derive(Clone, Default)]
pub struct AdapterRegistry {
    adapters: HashMap<String, Arc<dyn GameAdapter>>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, slug: impl Into<String>, adapter: Arc<dyn GameAdapter>) {
        self.adapters.insert(slug.into(), adapter);
    }

    pub fn get(&self, slug: &str) -> Option<Arc<dyn GameAdapter>> {
        self.adapters.get(slug).cloned()
    }

    pub fn post_install(&self, slug: &str, install_dir: &Path) -> Result<(), AdapterError> {
        match self.get(slug) {
            Some(adapter) => adapter.post_install(slug, install_dir),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct RecordingAdapter;

    impl GameAdapter for RecordingAdapter {
        fn build_args(&self, _context: &AdapterContext<'_>) -> Vec<String> {
            vec!["--adapter".to_string()]
        }
    }

    #[test]
    fn registry_dispatches_by_slug() {
        let mut registry = AdapterRegistry::new();
        registry.register("special-game", Arc::new(RecordingAdapter));
        assert!(registry.get("special-game").is_some());
        assert!(registry.get("other").is_none());
    }
}
