//! Game launch planning, process spawning and playtime accounting.
//!
//! Games are started with `std::process::Command` directly: no shell, no
//! string concatenation. The executable path comes from the manifest and is
//! validated as a relative path before it is joined to the install directory.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus};
use std::time::Instant;

use thiserror::Error;

use crate::adapters::{AdapterContext, AdapterError, AdapterRegistry};
use crate::archive::SafeRelPath;
use crate::catalog::model::{GameEntry, Os};
use crate::db::DbError;
use crate::library::{InstallSource, Library, LibraryGame};
use crate::mods::{tokenize_args, ModApplication, ModError, ModManager};

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LaunchError {
    #[error("database error: {0}")]
    Db(#[from] DbError),

    #[error("mod error: {0}")]
    Mod(#[from] ModError),

    #[error("adapter error: {0}")]
    Adapter(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("game is not in the library: {0}")]
    NotInLibrary(String),

    #[error("no executable is declared for this operating system")]
    UnsupportedOs,

    #[error("executable not found: {0}")]
    MissingExecutable(PathBuf),

    #[error("user file required but missing: {0}")]
    MissingUserFile(PathBuf),

    #[error("invalid executable path: {0}")]
    InvalidPath(String),

    #[error("process wait task failed: {0}")]
    Join(String),
}

/// Everything needed to start a game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchPlan {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub mods: ModApplication,
}

/// A spawned process tracked by the backend.
pub struct RunningGame {
    pub slug: String,
    pub pid: u32,
    pub executable: PathBuf,
    pub started: Instant,
    child: Child,
}

/// Result of waiting for a game process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExitInfo {
    pub slug: String,
    pub seconds: u64,
    pub code: Option<i32>,
    pub success: bool,
}

/// Resolves launch plans, spawns processes and records playtime.
#[derive(Clone)]
pub struct GameLauncher {
    library: Library,
    mods: ModManager,
    adapters: AdapterRegistry,
}

impl GameLauncher {
    pub fn new(library: Library, mods: ModManager) -> Self {
        Self {
            library,
            mods,
            adapters: AdapterRegistry::new(),
        }
    }

    /// Registers per-game adapters. Launch code dispatches by slug only through
    /// this registry.
    pub fn with_adapters(mut self, adapters: AdapterRegistry) -> Self {
        self.adapters = adapters;
        self
    }

    pub fn adapters(&self) -> &AdapterRegistry {
        &self.adapters
    }

    /// Builds a launch plan without starting anything.
    pub async fn resolve(
        &self,
        entry: &GameEntry,
        game: &LibraryGame,
    ) -> Result<LaunchPlan, LaunchError> {
        self.resolve_with_extra_args(entry, game, &[]).await
    }

    /// Same as [`resolve`](Self::resolve) with extra arguments (for example a
    /// server endpoint declared by the manifest join template).
    pub async fn resolve_with_extra_args(
        &self,
        entry: &GameEntry,
        game: &LibraryGame,
        extra_args: &[String],
    ) -> Result<LaunchPlan, LaunchError> {
        if entry.slug.as_str() != game.slug {
            return Err(LaunchError::NotInLibrary(game.slug.clone()));
        }
        let os = Os::current();

        if game.source == InstallSource::External {
            return self.resolve_external_with_args(game, extra_args);
        }

        let install_path = game
            .install_path
            .clone()
            .ok_or_else(|| LaunchError::NotInLibrary(game.slug.clone()))?;
        let executable = if install_path.is_file() {
            install_path.clone()
        } else {
            let declared = entry.launch.exe_for(os).ok_or(LaunchError::UnsupportedOs)?;
            let relative = SafeRelPath::new(declared)
                .map_err(|error| LaunchError::InvalidPath(error.to_string()))?;
            install_path.join(relative.to_path_buf())
        };
        if !executable.is_file() {
            return Err(LaunchError::MissingExecutable(executable));
        }

        if entry
            .user_files
            .as_ref()
            .is_some_and(|user_files| user_files.required)
        {
            match &game.user_file {
                Some(path) if path.is_file() => {}
                Some(path) => return Err(LaunchError::MissingUserFile(path.clone())),
                None => return Err(LaunchError::MissingUserFile(PathBuf::new())),
            }
        }

        let cwd = executable
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| LaunchError::InvalidPath(executable.display().to_string()))?;
        let mods = self
            .mods
            .prepare_for_launch(entry, game, &cwd)
            .await
            .map_err(LaunchError::Mod)?;
        let mut args = entry.launch.args.clone();
        args.extend(mods.extra_args.iter().cloned());
        args.extend(extra_args.iter().cloned());
        let mut cwd = cwd;
        if let Some(adapter) = self.adapters.get(entry.slug.as_str()) {
            let mut context = AdapterContext {
                entry,
                game,
                args,
                cwd,
            };
            adapter
                .pre_launch(&mut context)
                .map_err(|error: AdapterError| LaunchError::Adapter(error.to_string()))?;
            let extra = adapter.build_args(&context);
            context.args.extend(extra);
            args = context.args;
            cwd = context.cwd;
        }
        Ok(LaunchPlan {
            executable,
            args,
            cwd,
            mods,
        })
    }

    /// Resolves a launcher-managed external game without a catalogue entry.
    pub fn resolve_external(&self, game: &LibraryGame) -> Result<LaunchPlan, LaunchError> {
        self.resolve_external_with_args(game, &[])
    }

    fn resolve_external_with_args(
        &self,
        game: &LibraryGame,
        extra_args: &[String],
    ) -> Result<LaunchPlan, LaunchError> {
        if game.source != InstallSource::External {
            return Err(LaunchError::NotInLibrary(game.slug.clone()));
        }
        let executable = game
            .external_exe
            .clone()
            .ok_or_else(|| LaunchError::MissingExecutable(PathBuf::new()))?;
        if !executable.is_file() {
            return Err(LaunchError::MissingExecutable(executable));
        }
        let cwd = game
            .external_cwd
            .clone()
            .filter(|path| path.is_dir())
            .or_else(|| executable.parent().map(Path::to_path_buf))
            .ok_or_else(|| LaunchError::InvalidPath(executable.display().to_string()))?;
        let mut args = match game.external_args.as_deref() {
            Some(template) if !template.trim().is_empty() => tokenize_args(template)?,
            _ => Vec::new(),
        };
        args.extend(extra_args.iter().cloned());
        Ok(LaunchPlan {
            executable,
            args,
            cwd,
            mods: ModApplication::default(),
        })
    }

    /// Launches an external game (no catalogue entry required).
    pub fn launch_external(&self, game: &LibraryGame) -> Result<RunningGame, LaunchError> {
        self.launch_external_with_args(game, &[])
    }

    pub fn launch_external_with_args(
        &self,
        game: &LibraryGame,
        extra_args: &[String],
    ) -> Result<RunningGame, LaunchError> {
        let plan = self.resolve_external_with_args(game, extra_args)?;
        let mut command = Command::new(&plan.executable);
        command.args(&plan.args).current_dir(&plan.cwd);
        let started = Instant::now();
        let child = command.spawn()?;
        Ok(RunningGame {
            slug: game.slug.clone(),
            pid: child.id(),
            executable: plan.executable,
            started,
            child,
        })
    }

    /// Resolves the plan, starts the process and returns its PID.
    pub async fn launch(
        &self,
        entry: &GameEntry,
        game: &LibraryGame,
    ) -> Result<RunningGame, LaunchError> {
        self.launch_with_extra_args(entry, game, &[]).await
    }

    /// Launches with extra arguments appended after manifest/mods arguments.
    pub async fn launch_with_extra_args(
        &self,
        entry: &GameEntry,
        game: &LibraryGame,
        extra_args: &[String],
    ) -> Result<RunningGame, LaunchError> {
        let plan = self
            .resolve_with_extra_args(entry, game, extra_args)
            .await?;
        let mut command = Command::new(&plan.executable);
        command.args(&plan.args).current_dir(&plan.cwd);
        let started = Instant::now();
        let child = command.spawn()?;
        Ok(RunningGame {
            slug: game.slug.clone(),
            pid: child.id(),
            executable: plan.executable,
            started,
            child,
        })
    }

    /// Waits for the process, records elapsed playtime and returns exit details.
    pub async fn wait_for_exit(&self, running: RunningGame) -> Result<ExitInfo, LaunchError> {
        let RunningGame {
            slug,
            started,
            mut child,
            ..
        } = running;
        let status: ExitStatus = tokio::task::spawn_blocking(move || child.wait())
            .await
            .map_err(|error| LaunchError::Join(error.to_string()))??;
        let seconds = started.elapsed().as_secs();
        self.library.record_playtime(&slug, seconds).await?;
        Ok(ExitInfo {
            slug,
            seconds,
            code: status.code(),
            success: status.success(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::model::ModStrategy;
    use crate::catalog::test_support::minimal_entry;
    use crate::db::Db;
    use crate::paths::AppPaths;

    async fn setup() -> (tempfile::TempDir, AppPaths, Library, GameEntry, LibraryGame) {
        let root = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(root.path());
        paths.ensure_base_dirs().expect("dirs");
        let library = Library::new(Db::open_in_memory().await.expect("db"));
        let install = paths.version_dir("launch-test", "v1");
        std::fs::create_dir_all(&install).expect("install");
        std::fs::write(install.join("game.exe"), b"fake").expect("exe");
        let game = library
            .upsert_managed("launch-test", "Launch Test", &install, "v1", None)
            .await
            .expect("game");
        let mut entry = minimal_entry("launch-test");
        entry.user_files = None;
        entry
            .launch
            .exe
            .insert(Os::current(), "game.exe".to_string());
        entry.mods.supported = true;
        entry.mods.dir_name = "mods".to_string();
        entry.mods.format_hint = Some(".nrm".to_string());
        (root, paths, library, entry, game)
    }

    #[tokio::test]
    async fn mod_args_are_included_in_the_launch_plan() {
        let (_root, paths, library, mut entry, game) = setup().await;
        let mods = ModManager::new(paths.clone(), library.clone());
        let mods_dir = mods.enabled_dir(&game.slug, "mods");
        std::fs::create_dir_all(&mods_dir).expect("mods");
        std::fs::write(mods_dir.join("alpha.nrm"), b"mod").expect("mod");
        entry.mods.strategy = ModStrategy::Args;
        entry.mods.args_template = Some("--mod={path}".to_string());
        let launcher = GameLauncher::new(library, mods);
        let plan = launcher.resolve(&entry, &game).await.expect("plan");
        assert_eq!(plan.args.len(), 1);
        assert!(plan.args[0].starts_with("--mod="));
        assert!(plan.args[0].ends_with("alpha.nrm"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn detected_mod_args_reach_a_fake_executable() {
        let root = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(root.path());
        paths.ensure_base_dirs().expect("dirs");
        let library = Library::new(Db::open_in_memory().await.expect("db"));
        let command = std::env::var("COMSPEC").expect("COMSPEC");
        let game = library
            .upsert_managed(
                "fake-launch",
                "Fake Launch",
                Path::new(&command),
                "v1",
                None,
            )
            .await
            .expect("game");
        let mut entry = minimal_entry("fake-launch");
        entry.user_files = None;
        entry.launch.args = vec!["/C".to_string(), "echo".to_string()];
        entry.mods.supported = true;
        entry.mods.dir_name = "mods".to_string();
        entry.mods.format_hint = Some(".nrm".to_string());
        entry.mods.strategy = ModStrategy::Args;
        let output = root.path().join("received.txt");
        entry.mods.args_template = Some(format!("--mod={{path}} > \"{}\"", output.display()));
        let mods = ModManager::new(paths, library.clone());
        let mods_dir = mods.enabled_dir(&game.slug, "mods");
        std::fs::create_dir_all(&mods_dir).expect("mods dir");
        std::fs::write(mods_dir.join("detected.nrm"), b"mod").expect("mod");
        let launcher = GameLauncher::new(library, mods);
        let running = launcher.launch(&entry, &game).await.expect("launch");
        let exit = launcher.wait_for_exit(running).await.expect("wait");
        assert!(exit.success);
        let received = std::fs::read_to_string(&output).expect("received args");
        assert!(received.contains("detected.nrm"), "received: {received}");
    }

    #[tokio::test]
    async fn missing_user_file_blocks_launch() {
        let (_root, paths, library, mut entry, game) = setup().await;
        entry.user_files = Some(crate::catalog::model::UserFiles {
            required: true,
            description: "dump".to_string(),
            accepted_hashes: Vec::new(),
            prompt_ext: vec![".z64".to_string()],
        });
        let mods = ModManager::new(paths, library.clone());
        let launcher = GameLauncher::new(library, mods);
        assert!(matches!(
            launcher.resolve(&entry, &game).await,
            Err(LaunchError::MissingUserFile(_))
        ));
    }

    #[tokio::test]
    async fn waiting_records_playtime() {
        let root = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(root.path());
        paths.ensure_base_dirs().expect("dirs");
        let library = Library::new(Db::open_in_memory().await.expect("db"));
        let (executable, args) = short_command();
        library
            .add_external("external-test", "External", &executable, Some(&args), None)
            .await
            .expect("external");
        let game = library
            .get("external-test")
            .await
            .expect("get")
            .expect("row");
        let entry = minimal_entry("external-test");
        let mods = ModManager::new(paths, library.clone());
        let launcher = GameLauncher::new(library.clone(), mods);
        let running = launcher.launch(&entry, &game).await.expect("launch");
        let exit = launcher.wait_for_exit(running).await.expect("wait");
        assert_eq!(exit.slug, "external-test");
        assert!(exit.success);
        let game = library
            .get("external-test")
            .await
            .expect("get")
            .expect("row");
        assert!(game.last_played.is_some());
    }

    #[cfg(windows)]
    fn short_command() -> (PathBuf, String) {
        let executable = std::env::var("COMSPEC")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("cmd.exe"));
        (executable, "/C exit 0".to_string())
    }

    #[cfg(unix)]
    fn short_command() -> (PathBuf, String) {
        // `/bin/true` is not present on every Unix (macOS with a merged
        // filesystem, for example), so pick the first real binary.
        let executable = ["/usr/bin/true", "/bin/true"]
            .into_iter()
            .map(PathBuf::from)
            .find(|path| path.is_file())
            .unwrap_or_else(|| PathBuf::from("true"));
        (executable, String::new())
    }
}
