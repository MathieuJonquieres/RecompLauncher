//! Mod discovery, enable/disable, ordering and launch preparation.
//!
//! The launcher never executes mod content. It only copies user-provided files
//! into a per-game directory and, for the `native_dir` strategy, mirrors the
//! enabled entries into the game's working directory. Disabled entries live in
//! `<dir>.disabled/` so a game that scans `<dir>/` never sees them.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::archive::SafeRelPath;
use crate::catalog::model::{GameEntry, ModStrategy, ModsSpec};
use crate::db::DbError;
use crate::library::{InstallSource, Library, LibraryGame, ModRecord, ModSeed};
use crate::paths::{safe_component, AppPaths};

const MAX_MOD_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_MOD_ENTRIES: usize = 20_000;
const MIRROR_MANIFEST: &str = ".recomp-launcher-mirror";

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ModError {
    #[error("database error: {0}")]
    Db(#[from] DbError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid mod path: {0}")]
    InvalidPath(String),

    #[error("game is not in the library: {0}")]
    GameNotFound(String),

    #[error("a mod with this name already exists: {0}")]
    AlreadyExists(String),

    #[error("mod not found: {0}")]
    NotFound(String),

    #[error("mods are not supported by this project")]
    Unsupported,

    #[error("invalid arguments template: {0}")]
    InvalidTemplate(String),

    #[error("mod copy limit exceeded: {0}")]
    LimitExceeded(String),
}

/// Snapshot sent to the UI after a scan or a toggle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModsSnapshot {
    pub supported: bool,
    pub strategy: ModStrategy,
    pub dir_name: String,
    pub dir: PathBuf,
    pub mods: Vec<ModRecord>,
}

/// The result of preparing mods for a launch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModApplication {
    pub strategy: ModStrategy,
    pub enabled: Vec<String>,
    pub extra_args: Vec<String>,
    pub native_dir: Option<PathBuf>,
}

/// Owns mod scanning and the filesystem side of enable/disable.
#[derive(Clone)]
pub struct ModManager {
    paths: AppPaths,
    library: Library,
}

impl ModManager {
    pub fn new(paths: AppPaths, library: Library) -> Self {
        Self { paths, library }
    }

    /// Directory containing enabled mods: `games/<slug>/<dir_name>/`.
    pub fn enabled_dir(&self, slug: &str, dir_name: &str) -> PathBuf {
        self.paths.mods_dir(
            slug,
            &normalize_dir_name(dir_name).unwrap_or_else(|_| "mods".to_string()),
        )
    }

    /// Directory containing disabled mods: `games/<slug>/<dir_name>.disabled/`.
    pub fn disabled_dir(&self, slug: &str, dir_name: &str) -> PathBuf {
        let dir_name = normalize_dir_name(dir_name).unwrap_or_else(|_| "mods".to_string());
        self.paths
            .game_dir(slug)
            .join(format!("{dir_name}.disabled"))
    }

    /// Creates the enabled mods directory if needed.
    pub async fn create_dir(&self, slug: &str, spec: &ModsSpec) -> Result<PathBuf, ModError> {
        let dir_name = normalize_dir_name(&spec.dir_name)?;
        let dir = self.enabled_dir(slug, &dir_name);
        tokio::fs::create_dir_all(&dir).await?;
        Ok(dir)
    }

    /// Scans both the enabled and disabled directories, reconciles the
    /// `mods` table, and returns the persisted list ordered by user order.
    pub async fn refresh(&self, slug: &str, spec: &ModsSpec) -> Result<Vec<ModRecord>, ModError> {
        if !spec.supported {
            return Err(ModError::Unsupported);
        }
        let dir_name = normalize_dir_name(&spec.dir_name)?;
        let enabled_dir = self.enabled_dir(slug, &dir_name);
        let disabled_dir = self.disabled_dir(slug, &dir_name);
        let hint = spec.format_hint.as_deref();

        let mut seeds = Vec::new();
        let mut seen = HashSet::new();
        for (name, path) in scan_dir(&enabled_dir, hint).await? {
            if seen.insert(name.clone()) {
                seeds.push(ModSeed {
                    name,
                    path,
                    enabled: true,
                });
            }
        }
        for (name, path) in scan_dir(&disabled_dir, hint).await? {
            if seen.insert(name.clone()) {
                seeds.push(ModSeed {
                    name,
                    path,
                    enabled: false,
                });
            }
        }
        self.library
            .sync_mods(slug, seeds)
            .await
            .map_err(Into::into)
    }

    /// Returns the current mod list plus the canonical directory for the UI.
    pub async fn snapshot(&self, slug: &str, spec: &ModsSpec) -> Result<ModsSnapshot, ModError> {
        let dir_name = normalize_dir_name(&spec.dir_name)?;
        let dir = self.enabled_dir(slug, &dir_name);
        let supported = spec.supported && spec.strategy != ModStrategy::None;
        let mods = if supported {
            self.refresh(slug, spec).await?
        } else {
            Vec::new()
        };
        Ok(ModsSnapshot {
            supported,
            strategy: spec.strategy,
            dir_name,
            dir,
            mods,
        })
    }

    /// Moves a mod between the enabled and disabled directories.
    pub async fn toggle(
        &self,
        slug: &str,
        spec: &ModsSpec,
        name: &str,
        enabled: bool,
    ) -> Result<Vec<ModRecord>, ModError> {
        let dir_name = normalize_dir_name(&spec.dir_name)?;
        let mods = self.refresh(slug, spec).await?;
        let record = mods
            .iter()
            .find(|record| record.name == name)
            .cloned()
            .ok_or_else(|| ModError::NotFound(name.to_string()))?;
        if record.enabled == enabled {
            return Ok(mods);
        }

        let enabled_dir = self.enabled_dir(slug, &dir_name);
        let disabled_dir = self.disabled_dir(slug, &dir_name);
        let source_parent = record.path.parent();
        if source_parent != Some(enabled_dir.as_path())
            && source_parent != Some(disabled_dir.as_path())
        {
            return Err(ModError::InvalidPath(record.path.display().to_string()));
        }
        let target_root = if enabled { &enabled_dir } else { &disabled_dir };
        tokio::fs::create_dir_all(target_root).await?;
        let target = target_root.join(&record.name);
        if target.exists() {
            return Err(ModError::AlreadyExists(record.name));
        }
        tokio::fs::rename(&record.path, &target).await?;
        self.library
            .set_mod_state(slug, &record.name, &target, enabled)
            .await?;
        self.library.list_mods(slug).await.map_err(Into::into)
    }

    /// Persists a new order and returns the reordered list.
    pub async fn reorder(
        &self,
        slug: &str,
        spec: &ModsSpec,
        names: Vec<String>,
    ) -> Result<Vec<ModRecord>, ModError> {
        if !spec.supported {
            return Err(ModError::Unsupported);
        }
        self.library.set_mod_order(slug, names).await?;
        self.library.list_mods(slug).await.map_err(Into::into)
    }

    /// Copies a dropped file or directory into the enabled mods directory.
    ///
    /// Only the final path component is used, so a dropped path cannot inject
    /// `..` or an absolute destination.
    pub async fn import_path(
        &self,
        slug: &str,
        spec: &ModsSpec,
        source: &Path,
    ) -> Result<PathBuf, ModError> {
        if !spec.supported {
            return Err(ModError::Unsupported);
        }
        let dir_name = normalize_dir_name(&spec.dir_name)?;
        let file_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| ModError::InvalidPath(source.display().to_string()))?;
        let safe_name = safe_entry_name(file_name)?;
        let target_dir = self.enabled_dir(slug, &dir_name);
        tokio::fs::create_dir_all(&target_dir).await?;
        let target = target_dir.join(&safe_name);
        if target.exists() {
            return Err(ModError::AlreadyExists(safe_name));
        }
        let metadata = tokio::fs::symlink_metadata(source).await?;
        if metadata.file_type().is_symlink() {
            return Err(ModError::InvalidPath(format!(
                "symbolic links are not imported: {}",
                source.display()
            )));
        }
        if metadata.is_dir() {
            let source = source.to_path_buf();
            let target_for_copy = target.clone();
            tokio::task::spawn_blocking(move || {
                let mut budget = CopyBudget::default();
                copy_tree_sync(&source, &target_for_copy, &mut budget)
            })
            .await
            .map_err(|error| ModError::Io(std::io::Error::other(error.to_string())))??;
        } else if metadata.is_file() {
            tokio::fs::copy(source, &target).await?;
        } else {
            return Err(ModError::InvalidPath(source.display().to_string()));
        }
        Ok(target)
    }

    /// Applies the manifest strategy and returns arguments/mirror information.
    pub async fn prepare_for_launch(
        &self,
        entry: &GameEntry,
        game: &LibraryGame,
        install_dir: &Path,
    ) -> Result<ModApplication, ModError> {
        if !entry.mods.supported {
            return Ok(ModApplication::default());
        }
        let dir_name = normalize_dir_name(&entry.mods.dir_name)?;
        let mods = self.refresh(&game.slug, &entry.mods).await?;
        let enabled: Vec<ModRecord> = mods.into_iter().filter(|record| record.enabled).collect();
        let enabled_names = enabled.iter().map(|record| record.name.clone()).collect();

        match entry.mods.strategy {
            ModStrategy::None => Ok(ModApplication {
                strategy: ModStrategy::None,
                enabled: enabled_names,
                extra_args: Vec::new(),
                native_dir: None,
            }),
            ModStrategy::Args => {
                let extra_args = build_mod_args(entry.mods.args_template.as_deref(), &enabled)?;
                Ok(ModApplication {
                    strategy: ModStrategy::Args,
                    enabled: enabled_names,
                    extra_args,
                    native_dir: None,
                })
            }
            ModStrategy::NativeDir => {
                if game.source != InstallSource::Managed {
                    // Never rewrite a folder the user chose manually. The
                    // canonical mods directory is still available in the UI.
                    return Ok(ModApplication {
                        strategy: ModStrategy::NativeDir,
                        enabled: enabled_names,
                        extra_args: Vec::new(),
                        native_dir: None,
                    });
                }
                let mirror = self
                    .sync_native_dir(game, &dir_name, &enabled, install_dir)
                    .await?;
                Ok(ModApplication {
                    strategy: ModStrategy::NativeDir,
                    enabled: enabled_names,
                    extra_args: Vec::new(),
                    native_dir: Some(mirror),
                })
            }
        }
    }

    async fn sync_native_dir(
        &self,
        game: &LibraryGame,
        dir_name: &str,
        enabled: &[ModRecord],
        install_dir: &Path,
    ) -> Result<PathBuf, ModError> {
        let mirror = install_dir.join(dir_name);
        let source_root = self.enabled_dir(&game.slug, dir_name);
        let records = enabled.to_vec();
        let mirror_for_copy = mirror.clone();
        tokio::task::spawn_blocking(move || {
            let mut budget = CopyBudget::default();
            sync_native_dir_sync(&source_root, &mirror_for_copy, &records, &mut budget)
        })
        .await
        .map_err(|error| ModError::Io(std::io::Error::other(error.to_string())))??;
        Ok(mirror)
    }
}

async fn scan_dir(
    dir: &Path,
    format_hint: Option<&str>,
) -> Result<Vec<(String, PathBuf)>, ModError> {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut found = Vec::new();
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name.ends_with(".part") || name.ends_with(".tmp") {
            continue;
        }
        let file_type = entry.file_type().await?;
        if file_type.is_symlink() {
            continue;
        }
        if !file_type.is_dir() && !file_type.is_file() {
            continue;
        }
        if file_type.is_file() && !matches_format_hint(&name, format_hint) {
            continue;
        }
        let safe_name = match safe_entry_name(&name) {
            Ok(name) => name,
            Err(_) => continue,
        };
        found.push((safe_name, entry.path()));
    }
    found.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(found)
}

fn matches_format_hint(name: &str, hint: Option<&str>) -> bool {
    let Some(hint) = hint.map(str::trim).filter(|hint| !hint.is_empty()) else {
        return true;
    };
    if hint == "*" {
        return true;
    }
    let hint = hint.trim_start_matches('.').to_ascii_lowercase();
    name.rsplit_once('.')
        .map(|(_, extension)| extension.eq_ignore_ascii_case(&hint))
        .unwrap_or(false)
}

fn normalize_dir_name(raw: &str) -> Result<String, ModError> {
    let safe = SafeRelPath::new(raw).map_err(|error| ModError::InvalidPath(error.to_string()))?;
    if safe.as_str().contains('/') {
        return Err(ModError::InvalidPath(format!(
            "mods.dir_name must be a single directory: {raw}"
        )));
    }
    Ok(safe_component(safe.as_str()))
}

fn safe_entry_name(raw: &str) -> Result<String, ModError> {
    let safe = SafeRelPath::new(raw).map_err(|error| ModError::InvalidPath(error.to_string()))?;
    if safe.as_str().contains('/') {
        return Err(ModError::InvalidPath(format!(
            "a mod name must be a single component: {raw}"
        )));
    }
    Ok(safe_component(safe.as_str()))
}

fn build_mod_args(template: Option<&str>, enabled: &[ModRecord]) -> Result<Vec<String>, ModError> {
    let Some(template) = template.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(enabled
            .iter()
            .map(|record| record.path.to_string_lossy().into_owned())
            .collect());
    };
    let tokens = tokenize_args(template)?;
    let has_path = tokens.iter().any(|token| token.contains("{path}"));
    let has_name = tokens.iter().any(|token| token.contains("{name}"));
    if !has_path && !has_name {
        return Ok(tokens);
    }
    let mut args = Vec::new();
    for record in enabled {
        let path = record.path.to_string_lossy();
        for token in &tokens {
            args.push(
                token
                    .replace("{path}", &path)
                    .replace("{name}", &record.name),
            );
        }
    }
    Ok(args)
}

/// Splits a manifest argument template. Quotes are honoured and substitutions
/// happen per token, so a path containing spaces remains one argument.
pub fn tokenize_args(template: &str) -> Result<Vec<String>, ModError> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;
    let mut chars = template.chars().peekable();
    while let Some(ch) = chars.next() {
        match quote {
            Some(active) => {
                if ch == active {
                    quote = None;
                } else if ch == '\\' && active == '"' {
                    // Inside double quotes, only \" and \\ are escapes.
                    // Other backslashes are literal Windows path separators.
                    match chars.peek() {
                        Some('"') | Some('\\') => {
                            if let Some(next) = chars.next() {
                                current.push(next);
                            }
                        }
                        _ => current.push('\\'),
                    }
                } else {
                    current.push(ch);
                }
            }
            None => match ch {
                '"' | '\'' => {
                    quote = Some(ch);
                    started = true;
                }
                '\\' => match chars.peek() {
                    Some('"') | Some('\'') | Some(' ') | Some('\\') => {
                        if let Some(next) = chars.next() {
                            current.push(next);
                        }
                    }
                    _ => current.push('\\'),
                },
                ch if ch.is_whitespace() => {
                    if started {
                        tokens.push(std::mem::take(&mut current));
                        started = false;
                    }
                }
                _ => {
                    current.push(ch);
                    started = true;
                }
            },
        }
    }
    if quote.is_some() {
        return Err(ModError::InvalidTemplate(template.to_string()));
    }
    if started {
        tokens.push(current);
    }
    Ok(tokens)
}

#[derive(Default)]
struct CopyBudget {
    bytes: u64,
    entries: usize,
}

fn sync_native_dir_sync(
    source_root: &Path,
    mirror: &Path,
    enabled: &[ModRecord],
    budget: &mut CopyBudget,
) -> Result<(), ModError> {
    if mirror.exists() && !mirror.is_dir() {
        return Err(ModError::InvalidPath(mirror.display().to_string()));
    }
    std::fs::create_dir_all(mirror)?;
    let manifest = mirror.join(MIRROR_MANIFEST);
    if manifest.is_file() {
        let previous = std::fs::read_to_string(&manifest)?;
        for name in previous.lines() {
            let Ok(safe_name) = safe_entry_name(name) else {
                continue;
            };
            let path = mirror.join(safe_name);
            if path.is_dir() {
                std::fs::remove_dir_all(path)?;
            } else if path.is_file() {
                std::fs::remove_file(path)?;
            }
        }
    }

    let mut names = Vec::new();
    for record in enabled {
        if record.path.parent() != Some(source_root) {
            return Err(ModError::InvalidPath(record.path.display().to_string()));
        }
        let target = mirror.join(&record.name);
        if target.exists() {
            return Err(ModError::AlreadyExists(record.name.clone()));
        }
        if record.path.is_dir() {
            copy_tree_sync(&record.path, &target, budget)?;
        } else if record.path.is_file() {
            copy_file_sync(&record.path, &target, budget)?;
        } else {
            return Err(ModError::NotFound(record.path.display().to_string()));
        }
        names.push(record.name.clone());
    }
    std::fs::write(manifest, names.join("\n"))?;
    Ok(())
}

fn copy_tree_sync(source: &Path, target: &Path, budget: &mut CopyBudget) -> Result<(), ModError> {
    let metadata = std::fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        return Err(ModError::InvalidPath(format!(
            "symbolic links are not copied: {}",
            source.display()
        )));
    }
    if metadata.is_file() {
        return copy_file_sync(source, target, budget);
    }
    if !metadata.is_dir() {
        return Err(ModError::InvalidPath(source.display().to_string()));
    }
    budget.entries += 1;
    if budget.entries > MAX_MOD_ENTRIES {
        return Err(ModError::LimitExceeded("too many files".to_string()));
    }
    std::fs::create_dir_all(target)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let safe_name = safe_entry_name(&name)?;
        let child_source = entry.path();
        let child_target = target.join(safe_name);
        let file_type = entry.file_type()?;
        if file_type.is_symlink() {
            return Err(ModError::InvalidPath(format!(
                "symbolic links are not copied: {}",
                child_source.display()
            )));
        }
        if file_type.is_dir() {
            copy_tree_sync(&child_source, &child_target, budget)?;
        } else if file_type.is_file() {
            copy_file_sync(&child_source, &child_target, budget)?;
        }
    }
    Ok(())
}

fn copy_file_sync(source: &Path, target: &Path, budget: &mut CopyBudget) -> Result<(), ModError> {
    let size = std::fs::metadata(source)?.len();
    budget.bytes = budget.bytes.saturating_add(size);
    budget.entries += 1;
    if budget.bytes > MAX_MOD_BYTES {
        return Err(ModError::LimitExceeded("too many bytes".to_string()));
    }
    if budget.entries > MAX_MOD_ENTRIES {
        return Err(ModError::LimitExceeded("too many files".to_string()));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(source, target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::model::Os;
    use crate::catalog::test_support::minimal_entry;
    use crate::db::Db;

    async fn setup() -> (tempfile::TempDir, AppPaths, Library, GameEntry, LibraryGame) {
        let root = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(root.path());
        paths.ensure_base_dirs().expect("dirs");
        let library = Library::new(Db::open_in_memory().await.expect("db"));
        let game = library
            .upsert_managed(
                "mod-test",
                "Mod Test",
                &paths.version_dir("mod-test", "v1"),
                "v1",
                None,
            )
            .await
            .expect("library");
        let mut entry = minimal_entry("mod-test");
        entry
            .launch
            .exe
            .insert(Os::current(), "game.exe".to_string());
        entry.mods.supported = true;
        entry.mods.dir_name = "mods".to_string();
        entry.mods.format_hint = Some(".nrm".to_string());
        (root, paths, library, entry, game)
    }

    fn write_mod(path: &Path, name: &str) {
        std::fs::create_dir_all(path).expect("mod dir");
        std::fs::write(path.join(name), b"mod").expect("mod file");
    }

    #[tokio::test]
    async fn scans_detects_toggles_and_reorders() {
        let (_root, paths, library, entry, game) = setup().await;
        let manager = ModManager::new(paths.clone(), library.clone());
        let enabled = manager.enabled_dir(&game.slug, "mods");
        write_mod(&enabled, "beta.nrm");
        write_mod(&enabled, "alpha.nrm");
        std::fs::write(enabled.join("readme.txt"), b"not a mod").expect("readme");

        let mods = manager
            .refresh(&game.slug, &entry.mods)
            .await
            .expect("refresh");
        assert_eq!(mods.len(), 2);
        assert_eq!(mods[0].name, "alpha.nrm");
        assert!(mods.iter().all(|record| record.enabled));

        let mods = manager
            .toggle(&game.slug, &entry.mods, "beta.nrm", false)
            .await
            .expect("disable");
        let beta = mods
            .iter()
            .find(|record| record.name == "beta.nrm")
            .expect("beta");
        assert!(!beta.enabled);
        assert!(manager
            .disabled_dir(&game.slug, "mods")
            .join("beta.nrm")
            .is_file());
        assert!(!enabled.join("beta.nrm").exists());

        let names = vec!["beta.nrm".to_string(), "alpha.nrm".to_string()];
        let mods = manager
            .reorder(&game.slug, &entry.mods, names)
            .await
            .expect("reorder");
        assert_eq!(mods[0].name, "beta.nrm");
        assert_eq!(mods[1].name, "alpha.nrm");

        let mods = manager
            .toggle(&game.slug, &entry.mods, "beta.nrm", true)
            .await
            .expect("enable");
        assert!(mods
            .iter()
            .any(|record| record.name == "beta.nrm" && record.enabled));
    }

    #[tokio::test]
    async fn native_dir_mirrors_only_enabled_mods() {
        let (_root, paths, library, entry, game) = setup().await;
        let manager = ModManager::new(paths.clone(), library.clone());
        let enabled = manager.enabled_dir(&game.slug, "mods");
        write_mod(&enabled, "enabled.nrm");
        write_mod(&enabled, "disabled.nrm");
        manager
            .refresh(&game.slug, &entry.mods)
            .await
            .expect("refresh");
        manager
            .toggle(&game.slug, &entry.mods, "disabled.nrm", false)
            .await
            .expect("disable");

        let install = paths.version_dir(&game.slug, "v1");
        std::fs::create_dir_all(&install).expect("install");
        let application = manager
            .prepare_for_launch(&entry, &game, &install)
            .await
            .expect("prepare");
        let mirror = application.native_dir.expect("mirror");
        assert!(mirror.join("enabled.nrm").is_file());
        assert!(!mirror.join("disabled.nrm").exists());
        assert_eq!(application.enabled, vec!["enabled.nrm".to_string()]);
    }

    #[tokio::test]
    async fn args_strategy_expands_one_argument_per_enabled_mod() {
        let (_root, paths, library, mut entry, game) = setup().await;
        let manager = ModManager::new(paths.clone(), library.clone());
        let enabled = manager.enabled_dir(&game.slug, "mods");
        write_mod(&enabled, "one.nrm");
        write_mod(&enabled, "two.nrm");
        entry.mods.strategy = ModStrategy::Args;
        entry.mods.args_template = Some("--mod {path} --name={name}".to_string());
        manager
            .refresh(&game.slug, &entry.mods)
            .await
            .expect("refresh");

        let install = paths.version_dir(&game.slug, "v1");
        std::fs::create_dir_all(&install).expect("install");
        let application = manager
            .prepare_for_launch(&entry, &game, &install)
            .await
            .expect("prepare");
        assert_eq!(application.extra_args.len(), 6);
        assert_eq!(application.extra_args[0], "--mod");
        assert!(application.extra_args[1].ends_with("one.nrm"));
        assert_eq!(application.extra_args[2], "--name=one.nrm");
        assert_eq!(application.extra_args[3], "--mod");
        assert!(application.extra_args[4].ends_with("two.nrm"));
        assert_eq!(application.extra_args[5], "--name=two.nrm");
    }

    #[tokio::test]
    async fn imports_a_file_without_path_traversal() {
        let (_root, paths, library, entry, game) = setup().await;
        let manager = ModManager::new(paths.clone(), library.clone());
        let source_dir = tempfile::tempdir().expect("source");
        let source = source_dir.path().join("imported.nrm");
        std::fs::write(&source, b"mod").expect("source");
        let target = manager
            .import_path(&game.slug, &entry.mods, &source)
            .await
            .expect("import");
        assert!(target.is_file());
        assert_eq!(
            target.file_name().and_then(|name| name.to_str()),
            Some("imported.nrm")
        );
        let mods = manager
            .refresh(&game.slug, &entry.mods)
            .await
            .expect("refresh");
        assert_eq!(mods.len(), 1);
    }

    #[test]
    fn tokenizer_keeps_quoted_paths_together() {
        let tokens = tokenize_args(r#"--mod "{path}" --flag"#).expect("tokens");
        assert_eq!(tokens, vec!["--mod", "{path}", "--flag"]);
    }

    #[test]
    fn tokenizer_preserves_windows_backslashes() {
        let tokens = tokenize_args(r#"--mod="C:\Games\mods\one.nrm" --other C:\Games\mods"#)
            .expect("tokens");
        assert_eq!(
            tokens,
            vec![
                "--mod=C:\\Games\\mods\\one.nrm",
                "--other",
                "C:\\Games\\mods"
            ]
        );
    }
}
