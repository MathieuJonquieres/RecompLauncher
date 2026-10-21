//! End-to-end installation: download → verify → extract → atomic install → DB.
//!
//! No game content is ever bundled or copied from the user. The downloaded
//! archive is a project binary release; user dumps stay where they are and are
//! only hash-checked.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use thiserror::Error;
use tokio::sync::mpsc::UnboundedSender;
use tokio_util::sync::CancellationToken;

use crate::archive::{self, ArchiveError, ExtractionLimits};
use crate::catalog::model::{GameEntry, Os};
use crate::db::{now_unix, DbError};
use crate::hash::{self, HashError, HashVerdict};
use crate::library::{InstallSource, Library, LibraryGame};
use crate::net::download::{self, DownloadError, DownloadOptions, DownloadProgress};
use crate::net::http_client::HttpClient;
use crate::paths::{safe_component, AppPaths};

const MAX_DOWNLOAD_BYTES: u64 = 8 * 1024 * 1024 * 1024;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum InstallError {
    #[error("download error: {0}")]
    Download(#[from] DownloadError),

    #[error("archive error: {0}")]
    Archive(#[from] ArchiveError),

    #[error("database error: {0}")]
    Db(#[from] DbError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("hash error: {0}")]
    Hash(#[from] HashError),

    #[error("game is not compatible with {0}")]
    UnsupportedOs(&'static str),

    #[error("release version `{0}` is not a safe path component")]
    InvalidVersion(String),

    #[error("executable `{0}` was not found in the downloaded release")]
    ExecutableNotFound(String),

    #[error("game `{0}` is not in the library")]
    NotInLibrary(String),

    #[error("background task failed: {0}")]
    Join(String),

    #[error("installation cancelled")]
    Cancelled,

    #[error("configuration error: {0}")]
    Config(#[from] crate::error::CoreError),
}

/// A resolved asset ready to install.
#[derive(Debug, Clone)]
pub struct AssetDownload {
    pub url: String,
    pub file_name: String,
    pub expected_sha256: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct InstalledGame {
    pub game: LibraryGame,
    pub executable: PathBuf,
    pub user_file_required: bool,
}

/// Result of checking an installed library entry without downloading anything.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum InstallHealth {
    Healthy,
    MissingInstallDirectory(PathBuf),
    MissingExecutable(PathBuf),
    MissingUserFile(PathBuf),
    UnsupportedOs(&'static str),
}

impl InstallHealth {
    pub const fn is_healthy(&self) -> bool {
        matches!(self, Self::Healthy)
    }

    pub fn message(&self) -> String {
        match self {
            Self::Healthy => "Installation saine.".to_string(),
            Self::MissingInstallDirectory(path) => {
                format!("Dossier d'installation introuvable : {}", path.display())
            }
            Self::MissingExecutable(path) => {
                format!("Exécutable introuvable : {}", path.display())
            }
            Self::MissingUserFile(path) => {
                format!("Fichier utilisateur introuvable : {}", path.display())
            }
            Self::UnsupportedOs(os) => format!("Aucun binaire compatible avec {os}."),
        }
    }
}

#[derive(Clone)]
pub struct InstallManager {
    paths: AppPaths,
    http: HttpClient,
    library: Library,
}

impl InstallManager {
    pub fn new(paths: AppPaths, http: HttpClient, library: Library) -> Self {
        Self {
            paths,
            http,
            library,
        }
    }

    /// Downloads and installs `asset` for `entry`.
    pub async fn install(
        &self,
        entry: &GameEntry,
        version: &str,
        asset: &AssetDownload,
        progress: Option<UnboundedSender<DownloadProgress>>,
        cancel: Option<CancellationToken>,
    ) -> Result<InstalledGame, InstallError> {
        let os = Os::current();
        if !entry.supports_os(os) {
            return Err(InstallError::UnsupportedOs(os.label()));
        }
        let expected_exe = entry
            .launch
            .exe_for(os)
            .ok_or(InstallError::UnsupportedOs(os.label()))?;
        validate_version(version)?;

        self.paths.ensure_base_dirs()?;
        let slug = entry.slug.as_str();
        let previous_user_file = self
            .library
            .get(slug)
            .await?
            .and_then(|game| game.user_file);

        let temp = tempfile::Builder::new()
            .prefix("recomp-install-")
            .tempdir_in(self.paths.cache_dir())?;
        let staging = temp.path().join("staging");
        std::fs::create_dir_all(&staging)?;

        let download_name = safe_asset_name(&asset.file_name);
        let download_path = self
            .paths
            .downloads_dir()
            .join(safe_component(slug))
            .join(safe_component(version))
            .join(download_name);
        if let Some(parent) = download_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let download_options = DownloadOptions {
            max_bytes: asset
                .size
                .filter(|size| *size > 0)
                .map(|size| size.saturating_add(1).min(MAX_DOWNLOAD_BYTES))
                .unwrap_or(MAX_DOWNLOAD_BYTES),
            expected_sha256: asset.expected_sha256.clone(),
            resume: true,
        };
        let _ = download::download_to_file(
            &self.http,
            &asset.url,
            &download_path,
            &download_options,
            progress,
            cancel.clone(),
        )
        .await?;

        let cancel_flag = Arc::new(AtomicBool::new(
            cancel.as_ref().is_some_and(CancellationToken::is_cancelled),
        ));
        let cancel_for_extract = cancel.map(|token| {
            let flag = Arc::clone(&cancel_flag);
            let flag_for_task = Arc::clone(&flag);
            let child = token.child_token();
            tokio::spawn(async move {
                child.cancelled().await;
                flag_for_task.store(true, std::sync::atomic::Ordering::Relaxed);
            });
            flag
        });
        // When no cancellation token is supplied, still pass a shared flag so
        // the extraction loop can observe it.
        let extraction_flag = cancel_for_extract.unwrap_or_else(|| Arc::clone(&cancel_flag));

        let format = archive::ArchiveFormat::detect(&download_path).ok();
        if let Some(_format) = format {
            let archive_path = download_path.clone();
            let staging_path = staging.clone();
            let limits = ExtractionLimits::default();
            let flag = Arc::clone(&extraction_flag);
            tokio::task::spawn_blocking(move || {
                archive::extract(&archive_path, &staging_path, &limits, Some(flag))
            })
            .await
            .map_err(|error| InstallError::Join(error.to_string()))??;
        } else {
            let target = staging.join(expected_exe);
            if let Some(parent) = target.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            tokio::fs::copy(&download_path, &target).await?;
            set_executable(&target)?;
        }

        if extraction_flag.load(std::sync::atomic::Ordering::Relaxed) {
            return Err(InstallError::Cancelled);
        }

        let install_root = find_install_root(&staging, expected_exe)?;
        let executable = install_root.join(expected_exe);
        if !executable.is_file() {
            return Err(InstallError::ExecutableNotFound(expected_exe.to_string()));
        }
        set_executable(&executable)?;

        let final_dir = self.paths.version_dir(slug, version);
        if let Some(parent) = final_dir.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let backup = backup_path(&final_dir);
        let had_previous = tokio::fs::try_exists(&final_dir).await?;
        if had_previous {
            tokio::fs::rename(&final_dir, &backup).await?;
        }

        if let Err(error) = move_path(&install_root, &final_dir).await {
            if had_previous {
                let _ = tokio::fs::rename(&backup, &final_dir).await;
            }
            return Err(error);
        }

        let installed = self
            .library
            .upsert_managed(
                slug,
                &entry.title,
                &final_dir,
                version,
                previous_user_file.as_deref(),
            )
            .await;
        match installed {
            Ok(game) => {
                if had_previous {
                    let _ = tokio::fs::remove_dir_all(&backup).await;
                }
                let final_executable = final_dir.join(expected_exe);
                Ok(InstalledGame {
                    game,
                    executable: final_executable,
                    user_file_required: entry
                        .user_files
                        .as_ref()
                        .is_some_and(|user_files| user_files.required),
                })
            }
            Err(error) => {
                let _ = tokio::fs::remove_dir_all(&final_dir).await;
                if had_previous {
                    let _ = tokio::fs::rename(&backup, &final_dir).await;
                }
                Err(InstallError::Db(error))
            }
        }
    }

    /// Checks that the files referenced by a library row still exist.
    pub fn check_install(&self, entry: &GameEntry, game: &LibraryGame) -> InstallHealth {
        let os = Os::current();
        let expected_exe = match entry.launch.exe_for(os) {
            Some(exe) => exe,
            None => return InstallHealth::UnsupportedOs(os.label()),
        };

        match game.source {
            InstallSource::External => {
                let Some(executable) = &game.external_exe else {
                    return InstallHealth::MissingExecutable(PathBuf::new());
                };
                if !executable.is_file() {
                    return InstallHealth::MissingExecutable(executable.clone());
                }
            }
            InstallSource::Managed | InstallSource::Manual => {
                let Some(install_path) = &game.install_path else {
                    return InstallHealth::MissingInstallDirectory(PathBuf::new());
                };
                if !install_path.exists() {
                    return InstallHealth::MissingInstallDirectory(install_path.clone());
                }
                let executable = if install_path.is_file() {
                    install_path.clone()
                } else {
                    install_path.join(expected_exe)
                };
                if !executable.is_file() {
                    return InstallHealth::MissingExecutable(executable);
                }
            }
        }

        if let Some(user_files) = &entry.user_files {
            if user_files.required {
                let Some(user_file) = &game.user_file else {
                    return InstallHealth::MissingUserFile(PathBuf::new());
                };
                if !user_file.is_file() {
                    return InstallHealth::MissingUserFile(user_file.clone());
                }
            }
        }
        InstallHealth::Healthy
    }

    /// Repairs a managed installation by reinstalling it when its files are
    /// missing. A healthy installation is returned untouched.
    pub async fn repair(
        &self,
        entry: &GameEntry,
        version: &str,
        asset: &AssetDownload,
        progress: Option<UnboundedSender<DownloadProgress>>,
        cancel: Option<CancellationToken>,
    ) -> Result<InstalledGame, InstallError> {
        let game = self
            .library
            .get(entry.slug.as_str())
            .await?
            .ok_or_else(|| InstallError::NotInLibrary(entry.slug.as_str().to_string()))?;
        let health = self.check_install(entry, &game);
        if health.is_healthy() {
            let os = Os::current();
            let executable = match game.source {
                InstallSource::External => game.external_exe.clone().unwrap_or_default(),
                _ => {
                    let expected = entry.launch.exe_for(os).unwrap_or_default();
                    game.install_path
                        .as_ref()
                        .map(|path| path.join(expected))
                        .unwrap_or_default()
                }
            };
            return Ok(InstalledGame {
                user_file_required: entry
                    .user_files
                    .as_ref()
                    .is_some_and(|user_files| user_files.required),
                game,
                executable,
            });
        }
        self.install(entry, version, asset, progress, cancel).await
    }

    /// Verifies a user-selected dump against the manifest hashes.
    pub fn verify_user_file(entry: &GameEntry, path: &Path) -> Result<HashVerdict, InstallError> {
        let accepted = entry
            .user_files
            .as_ref()
            .map(|user_files| user_files.accepted_hashes.as_slice())
            .unwrap_or(&[]);
        Ok(hash::verify_file(path, accepted)?)
    }

    pub async fn library(&self) -> &Library {
        &self.library
    }
}

fn validate_version(version: &str) -> Result<(), InstallError> {
    if version.is_empty()
        || version.len() > 128
        || version.contains(['/', '\\', ':', '\0'])
        || version == "."
        || version == ".."
        || version.ends_with('.')
        || version.ends_with(' ')
    {
        return Err(InstallError::InvalidVersion(version.to_string()));
    }
    Ok(())
}

fn safe_asset_name(name: &str) -> String {
    let component = Path::new(name)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("asset.bin");
    let safe = safe_component(component);
    if safe.is_empty() {
        "asset.bin".to_string()
    } else {
        safe
    }
}

fn backup_path(final_dir: &Path) -> PathBuf {
    let name = final_dir
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "install".to_string());
    final_dir.with_file_name(format!(".{name}.old-{}", now_unix()))
}

fn find_install_root(staging: &Path, expected_exe: &str) -> Result<PathBuf, InstallError> {
    if staging.join(expected_exe).is_file() {
        return Ok(staging.to_path_buf());
    }
    let expected = Path::new(expected_exe);
    for candidate in walk_files(staging)? {
        if candidate.ends_with(expected) {
            let mut root = candidate;
            for _ in expected.components() {
                root.pop();
            }
            if root.is_dir() {
                return Ok(root);
            }
        }
    }
    // Last chance: match only the file name (archives sometimes add a
    // top-level directory or rename the binary).
    let file_name = expected.file_name().unwrap_or_default();
    for candidate in walk_files(staging)? {
        if candidate.file_name() == Some(file_name) {
            if let Some(parent) = candidate.parent() {
                return Ok(parent.to_path_buf());
            }
        }
    }
    Err(InstallError::ExecutableNotFound(expected_exe.to_string()))
}

fn walk_files(root: &Path) -> Result<Vec<PathBuf>, InstallError> {
    let mut files = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                stack.push(entry.path());
            } else if file_type.is_file() {
                files.push(entry.path());
            }
        }
    }
    Ok(files)
}

async fn move_path(source: &Path, destination: &Path) -> Result<(), InstallError> {
    match tokio::fs::rename(source, destination).await {
        Ok(()) => Ok(()),
        Err(rename_error) => {
            // Cross-device moves fail with EXDEV on Unix. Fall back to a copy.
            if !source.is_dir() {
                return Err(InstallError::Io(rename_error));
            }
            let source = source.to_path_buf();
            let destination = destination.to_path_buf();
            let source_for_copy = source.clone();
            tokio::task::spawn_blocking(move || copy_dir_all(&source_for_copy, &destination))
                .await
                .map_err(|error| InstallError::Join(error.to_string()))??;
            let _ = std::fs::remove_dir_all(source);
            Ok(())
        }
    }
}

fn copy_dir_all(source: &Path, destination: &Path) -> Result<(), InstallError> {
    std::fs::create_dir_all(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn set_executable(path: &Path) -> Result<(), InstallError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(path)?.permissions();
        permissions.set_mode(permissions.mode() | 0o755);
        std::fs::set_permissions(path, permissions)?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// True when this library row was installed by the launcher.
pub fn is_managed(game: &LibraryGame) -> bool {
    game.source == InstallSource::Managed
}

/// Removes launcher-managed files while preserving `userdata/` and `mods/`.
///
/// Returns `true` when at least one of those user-owned directories was kept.
pub fn remove_install_preserving_userdata(path: &Path) -> Result<bool, std::io::Error> {
    if !path.exists() {
        return Ok(false);
    }
    let mut preserved = false;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.eq_ignore_ascii_case("userdata") || name.eq_ignore_ascii_case("mods") {
            preserved = true;
            continue;
        }
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            std::fs::remove_dir_all(entry.path())?;
        } else {
            std::fs::remove_file(entry.path())?;
        }
    }
    if !preserved {
        std::fs::remove_dir(path)?;
    }
    Ok(preserved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::test_support::minimal_entry;
    use crate::db::Db;
    use std::io::Write;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn entry_for(exe: &str) -> GameEntry {
        let mut entry = minimal_entry("install-test");
        entry.title = "Install Test".to_string();
        entry.launch.exe.insert(Os::current(), exe.to_string());
        entry.releases.assets = vec![crate::catalog::model::AssetPattern {
            os: Os::current(),
            arch: None,
            pattern: "*".to_string(),
            sha256: None,
        }];
        entry
    }

    fn zip_bytes() -> Vec<u8> {
        use zip::write::SimpleFileOptions;
        let mut buffer = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            writer.start_file("game.exe", options).expect("start");
            writer.write_all(b"fake binary").expect("write");
            writer.finish().expect("finish");
        }
        buffer.into_inner()
    }

    fn evil_zip_bytes() -> Vec<u8> {
        use zip::write::SimpleFileOptions;
        let mut buffer = std::io::Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            writer.start_file("../evil.exe", options).expect("start");
            writer.write_all(b"owned").expect("write");
            writer.finish().expect("finish");
        }
        buffer.into_inner()
    }

    async fn setup(bytes: Vec<u8>) -> (MockServer, AppPaths, Library, tempfile::TempDir) {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/release.zip"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes))
            .mount(&server)
            .await;
        let root = tempfile::tempdir().expect("tempdir");
        let paths = AppPaths::new(root.path());
        paths.ensure_base_dirs().expect("dirs");
        let library = Library::new(Db::open_in_memory().await.expect("db"));
        (server, paths, library, root)
    }

    #[tokio::test]
    async fn installs_zip_and_records_library() {
        let (server, paths, library, _root) = setup(zip_bytes()).await;
        let entry = entry_for("game.exe");
        let manager = InstallManager::new(
            paths.clone(),
            HttpClient::for_tests([server.address().ip().to_string()]).expect("client"),
            library.clone(),
        );
        let asset = AssetDownload {
            url: format!("{}/release.zip", server.uri()),
            file_name: "release.zip".to_string(),
            expected_sha256: None,
            size: Some(zip_bytes().len() as u64),
        };
        let installed = manager
            .install(&entry, "v1.0.0", &asset, None, None)
            .await
            .expect("install");
        assert!(installed.executable.is_file());
        assert_eq!(
            std::fs::read(&installed.executable).expect("read"),
            b"fake binary"
        );
        let game = library
            .get("install-test")
            .await
            .expect("get")
            .expect("row");
        assert_eq!(game.source, InstallSource::Managed);
        assert_eq!(game.version.as_deref(), Some("v1.0.0"));
        assert!(game.install_path.expect("path").is_dir());
    }

    #[tokio::test]
    async fn repair_restores_missing_executable() {
        let (server, paths, library, _root) = setup(zip_bytes()).await;
        let entry = entry_for("game.exe");
        let manager = InstallManager::new(
            paths.clone(),
            HttpClient::for_tests([server.address().ip().to_string()]).expect("client"),
            library.clone(),
        );
        let asset = AssetDownload {
            url: format!("{}/release.zip", server.uri()),
            file_name: "release.zip".to_string(),
            expected_sha256: None,
            size: None,
        };
        let installed = manager
            .install(&entry, "v1.0.0", &asset, None, None)
            .await
            .expect("install");
        std::fs::remove_file(&installed.executable).expect("remove exe");

        let game = library
            .get("install-test")
            .await
            .expect("get")
            .expect("row");
        assert!(matches!(
            manager.check_install(&entry, &game),
            InstallHealth::MissingExecutable(_)
        ));

        let repaired = manager
            .repair(&entry, "v1.0.0", &asset, None, None)
            .await
            .expect("repair");
        assert!(repaired.executable.is_file());
        assert!(matches!(
            manager.check_install(&entry, &game),
            InstallHealth::MissingUserFile(_)
        ));
    }

    #[tokio::test]
    async fn uninstall_preserves_userdata_and_mods() {
        let (server, paths, library, _root) = setup(zip_bytes()).await;
        let entry = entry_for("game.exe");
        let manager = InstallManager::new(
            paths.clone(),
            HttpClient::for_tests([server.address().ip().to_string()]).expect("client"),
            library,
        );
        let asset = AssetDownload {
            url: format!("{}/release.zip", server.uri()),
            file_name: "release.zip".to_string(),
            expected_sha256: None,
            size: None,
        };
        let installed = manager
            .install(&entry, "v1.0.0", &asset, None, None)
            .await
            .expect("install");
        let install_dir = installed.game.install_path.clone().expect("install path");
        std::fs::create_dir_all(install_dir.join("userdata")).expect("userdata");
        std::fs::write(install_dir.join("userdata/save.dat"), b"save").expect("save");
        std::fs::create_dir_all(install_dir.join("mods")).expect("mods");

        let preserved = remove_install_preserving_userdata(&install_dir).expect("remove");
        assert!(preserved);
        assert!(install_dir.join("userdata/save.dat").is_file());
        assert!(install_dir.join("mods").is_dir());
        assert!(!installed.executable.exists());
    }

    #[tokio::test]
    async fn malicious_archive_rolls_back_without_db_row() {
        let (server, paths, library, _root) = setup(evil_zip_bytes()).await;
        let entry = entry_for("game.exe");
        let manager = InstallManager::new(
            paths.clone(),
            HttpClient::for_tests([server.address().ip().to_string()]).expect("client"),
            library.clone(),
        );
        let asset = AssetDownload {
            url: format!("{}/release.zip", server.uri()),
            file_name: "release.zip".to_string(),
            expected_sha256: None,
            size: None,
        };
        let error = manager
            .install(&entry, "v1.0.0", &asset, None, None)
            .await
            .expect_err("must reject");
        assert!(matches!(error, InstallError::Archive(_)));
        assert!(!paths.version_dir("install-test", "v1.0.0").exists());
        assert!(library.list().await.expect("list").is_empty());
    }

    #[tokio::test]
    async fn rejects_unsafe_version() {
        let (server, paths, library, _root) = setup(zip_bytes()).await;
        let entry = entry_for("game.exe");
        let manager = InstallManager::new(
            paths,
            HttpClient::for_tests([server.address().ip().to_string()]).expect("client"),
            library,
        );
        let asset = AssetDownload {
            url: format!("{}/release.zip", server.uri()),
            file_name: "release.zip".to_string(),
            expected_sha256: None,
            size: None,
        };
        let error = manager
            .install(&entry, "../escape", &asset, None, None)
            .await
            .expect_err("must reject");
        assert!(matches!(error, InstallError::InvalidVersion(_)));
    }

    #[tokio::test]
    async fn verifies_user_file_hash_without_copying() {
        let entry = minimal_entry("user-file-test");
        let dir = tempfile::tempdir().expect("tempdir");
        let dump = dir.path().join("dump.z64");
        std::fs::write(&dump, b"not a real rom").expect("write");
        let verdict = InstallManager::verify_user_file(&entry, &dump).expect("verify");
        assert!(verdict.is_acceptable());
        assert!(dump.is_file());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn installs_single_file_appimage() {
        let (server, paths, library, _root) = setup(b"appimage".to_vec()).await;
        let mut entry = entry_for("game.appimage");
        entry
            .releases
            .assets
            .push(crate::catalog::model::AssetPattern {
                os: Os::current(),
                arch: None,
                pattern: "*".to_string(),
                sha256: None,
            });
        let manager = InstallManager::new(
            paths.clone(),
            HttpClient::for_tests([server.address().ip().to_string()]).expect("client"),
            library,
        );
        let asset = AssetDownload {
            url: format!("{}/release.zip", server.uri()),
            file_name: "game.appimage".to_string(),
            expected_sha256: None,
            size: None,
        };
        let installed = manager
            .install(&entry, "v1", &asset, None, None)
            .await
            .expect("install");
        assert_eq!(
            std::fs::read(&installed.executable).expect("read"),
            b"appimage"
        );
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            std::fs::metadata(&installed.executable)
                .expect("metadata")
                .permissions()
                .mode()
                & 0o111,
            0
        );
    }
}
