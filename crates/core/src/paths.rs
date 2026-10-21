//! Filesystem layout and portable mode.
//!
//! `AppPaths` is created once in `main` and injected everywhere through
//! `AppContext`. Tests build one with `AppPaths::new(tempdir)`; nothing in this
//! module mutates global state.

use std::path::{Path, PathBuf};

use directories::BaseDirs;

use crate::error::{CoreError, Result};

/// Resolved application directories.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppPaths {
    root: PathBuf,
    portable: bool,
}

impl AppPaths {
    /// Uses `<exe dir>` when a `portable.flag` file sits next to the executable,
    /// otherwise the platform data directory (e.g. `%APPDATA%/RecompLauncher`).
    pub fn discover() -> Result<Self> {
        let exe_dir = std::env::current_exe()
            .map_err(|error| CoreError::Config(format!("cannot locate executable: {error}")))?
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| CoreError::Config("executable has no parent directory".into()))?;

        if exe_dir.join("portable.flag").is_file() {
            return Ok(Self {
                root: exe_dir,
                portable: true,
            });
        }

        let dirs = BaseDirs::new()
            .ok_or_else(|| CoreError::Config("no platform data directory available".to_string()))?;
        Ok(Self {
            root: dirs.data_dir().join("RecompLauncher"),
            portable: false,
        })
    }

    /// Explicit root, primarily for tests.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            portable: false,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn is_portable(&self) -> bool {
        self.portable
    }

    pub fn db_path(&self) -> PathBuf {
        self.root.join("launcher.db")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn downloads_dir(&self) -> PathBuf {
        self.cache_dir().join("downloads")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn games_dir(&self) -> PathBuf {
        self.root.join("games")
    }

    pub fn game_dir(&self, slug: &str) -> PathBuf {
        self.games_dir().join(safe_component(slug))
    }

    pub fn version_dir(&self, slug: &str, version: &str) -> PathBuf {
        self.game_dir(slug).join(safe_component(version))
    }

    pub fn mods_dir(&self, slug: &str, dir_name: &str) -> PathBuf {
        self.game_dir(slug).join(safe_component(dir_name))
    }

    pub fn userdata_dir(&self, slug: &str) -> PathBuf {
        self.game_dir(slug).join("userdata")
    }

    /// Creates the base directories needed before the database is opened.
    pub fn ensure_base_dirs(&self) -> Result<()> {
        for dir in [
            self.root.clone(),
            self.cache_dir(),
            self.downloads_dir(),
            self.logs_dir(),
            self.games_dir(),
        ] {
            std::fs::create_dir_all(&dir)?;
        }
        Ok(())
    }
}

/// Replaces characters that are illegal on Windows and neutralises `.`/`..`.
pub fn safe_component(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_control() || matches!(ch, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
            out.push('_');
        } else {
            out.push(ch);
        }
    }
    while out.ends_with('.') || out.ends_with(' ') {
        out.pop();
    }
    if out.is_empty() || out == "." || out == ".." {
        out = "_".to_string();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn portable_root_uses_given_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path());
        assert_eq!(paths.games_dir(), dir.path().join("games"));
        assert_eq!(paths.db_path(), dir.path().join("launcher.db"));
    }

    #[test]
    fn slug_traversal_is_neutralised() {
        assert_eq!(safe_component("../evil"), ".._evil");
        assert_eq!(safe_component("a/b"), "a_b");
        assert_eq!(safe_component(".."), "_");
    }

    #[test]
    fn ensure_base_dirs_creates_tree() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(dir.path());
        paths.ensure_base_dirs().expect("create dirs");
        assert!(paths.cache_dir().is_dir());
        assert!(paths.logs_dir().is_dir());
        assert!(paths.games_dir().is_dir());
    }
}
