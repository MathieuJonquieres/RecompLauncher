//! Command/Event bridge between the egui thread and the tokio backend.
//!
//! The UI thread sends `Command`s and drains `Event`s once per frame. The
//! backend owns the runtime and performs every blocking/network operation off
//! the render thread. Installs run in their own tasks so the command loop can
//! still receive cancellation and other requests.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use tokio::runtime::Runtime;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;

use recomp_core::catalog::github_releases::{select_asset, GitHubReleasesProvider, SelectedAsset};
use recomp_core::catalog::model::{ModStrategy, Os};
use recomp_core::catalog::service::CatalogSnapshot;
use recomp_core::fs_usage;
use recomp_core::library::install::{
    remove_install_preserving_userdata, AssetDownload, InstallManager,
};
use recomp_core::library::LibraryGame;
use recomp_core::metadata::GameMetadata;
use recomp_core::mods::ModsSnapshot;
use recomp_core::online::{join_entry_args, ServerInfo, ServerList};
use recomp_core::settings::{
    KEY_CATALOG_MANIFEST_URL, KEY_DIAGNOSTICS, KEY_GITHUB_TOKEN, KEY_INSTALL_DIR, KEY_LANGUAGE,
    KEY_PROXY, KEY_SERVERS_URL, KEY_THEME,
};
use recomp_core::AppContext;

/// Settings exposed to the UI. Secrets are never returned here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsSnapshot {
    pub language: String,
    pub dark_theme: bool,
    pub diagnostics: bool,
    pub proxy: String,
    pub manifest_url: String,
    pub servers_url: String,
    pub install_dir: String,
    pub data_dir: String,
    pub portable: bool,
}

/// Disk usage shown by the library statistics and the settings page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StorageStats {
    pub games: u64,
    pub cache: u64,
    pub per_game: HashMap<String, u64>,
}

/// Requests sent by the UI.
#[derive(Debug, Clone)]
pub enum Command {
    /// Load the cache immediately, then refresh if stale.
    Bootstrap,
    RefreshCatalog,
    RefreshLibrary,
    Install {
        slug: String,
    },
    Repair {
        slug: String,
    },
    CancelInstall {
        slug: String,
    },
    SelectUserFile {
        slug: String,
    },
    Uninstall {
        slug: String,
        delete_saves: bool,
    },
    Play {
        slug: String,
    },
    CreateModsDir {
        slug: String,
    },
    OpenModsDir {
        slug: String,
    },
    RefreshMods {
        slug: String,
    },
    ToggleMod {
        slug: String,
        name: String,
        enabled: bool,
    },
    ReorderMods {
        slug: String,
        names: Vec<String>,
    },
    PickMods {
        slug: String,
    },
    LoadMetadata {
        slug: String,
    },
    LoadServers {
        slug: String,
    },
    AddServer {
        slug: String,
        name: String,
        address: String,
        port: u16,
    },
    RemoveServer {
        slug: String,
        id: i64,
    },
    PingServers {
        slug: String,
    },
    DiscoverLan {
        slug: String,
    },
    JoinServer {
        slug: String,
        address: String,
        port: u16,
    },
    LoadSettings,
    SaveSettings {
        language: String,
        dark_theme: bool,
        diagnostics: bool,
        proxy: String,
        manifest_url: String,
        servers_url: String,
    },
    LoadStorageStats,
    ClearCache,
    ExportLogs,
    OpenDataDir,
    OpenUrl(String),
    OpenPath(PathBuf),
}

/// Results pushed back to the UI.
#[derive(Debug, Clone)]
pub enum Event {
    Catalog(CatalogSnapshot),
    CatalogFailed(String),
    CatalogRefreshing,
    Library(Vec<LibraryGame>),
    InstallStarted {
        slug: String,
        repaired: bool,
    },
    InstallProgress {
        slug: String,
        downloaded: u64,
        total: Option<u64>,
    },
    InstallFinished {
        slug: String,
        version: String,
        user_file_required: bool,
        repaired: bool,
    },
    InstallFailed {
        slug: String,
        message: String,
    },
    UserFileCheck {
        slug: String,
        status: UserFileCheckStatus,
    },
    Uninstalled {
        slug: String,
    },
    GameStarted {
        slug: String,
        pid: u32,
        executable: PathBuf,
    },
    GameExited {
        slug: String,
        seconds: u64,
        code: Option<i32>,
        success: bool,
    },
    Mods {
        slug: String,
        snapshot: ModsSnapshot,
    },
    ModsDirReady {
        slug: String,
        path: PathBuf,
    },
    ModsImported {
        slug: String,
        count: usize,
    },
    ModsFailed {
        slug: String,
        message: String,
    },
    Metadata {
        slug: String,
        metadata: Option<GameMetadata>,
        from_cache: bool,
        stale: bool,
        error: Option<String>,
    },
    Servers {
        slug: String,
        list: ServerList,
    },
    StorageStats(StorageStats),
    Settings(SettingsSnapshot),
    Error(String),
    Info(String),
}

enum ModsAction {
    CreateDir,
    OpenDir,
    Refresh,
    Toggle { name: String, enabled: bool },
    Reorder(Vec<String>),
    Import(Vec<PathBuf>),
}

enum ServerAction {
    Load,
    Add {
        name: String,
        address: String,
        port: u16,
    },
    Remove(i64),
    Ping,
    DiscoverLan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserFileCheckStatus {
    Verified(String),
    Rejected(String),
    Cancelled,
}

/// Owns the backend runtime and channels.
pub struct BackendBridge {
    _runtime: Runtime,
    commands: UnboundedSender<Command>,
    events: UnboundedReceiver<Event>,
}

impl BackendBridge {
    pub fn new(
        runtime: Runtime,
        context: Arc<AppContext>,
        egui_ctx: eframe::egui::Context,
    ) -> Self {
        let (command_tx, command_rx) = tokio::sync::mpsc::unbounded_channel();
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        runtime.spawn(backend_loop(context, command_rx, event_tx, egui_ctx));
        Self {
            _runtime: runtime,
            commands: command_tx,
            events: event_rx,
        }
    }

    pub fn send(&self, command: Command) {
        if self.commands.send(command).is_err() {
            tracing::error!("backend command channel is closed");
        }
    }

    /// Borrowed sender for views that only need to emit commands.
    pub fn sender(&self) -> &UnboundedSender<Command> {
        &self.commands
    }

    /// Drains every pending event without blocking.
    pub fn drain(&mut self) -> Vec<Event> {
        let mut events = Vec::new();
        while let Ok(event) = self.events.try_recv() {
            events.push(event);
        }
        events
    }
}

fn emit(events: &UnboundedSender<Event>, egui_ctx: &eframe::egui::Context, event: Event) {
    if events.send(event).is_err() {
        tracing::debug!("UI event channel closed");
    }
    egui_ctx.request_repaint();
}

async fn backend_loop(
    context: Arc<AppContext>,
    mut commands: UnboundedReceiver<Command>,
    events: UnboundedSender<Event>,
    egui_ctx: eframe::egui::Context,
) {
    let active_installs: Arc<Mutex<HashMap<String, CancellationToken>>> =
        Arc::new(Mutex::new(HashMap::new()));

    while let Some(command) = commands.recv().await {
        match command {
            Command::Bootstrap => {
                match context.load_cached_catalog().await {
                    Ok(snapshot) => {
                        let stale = context.catalog().is_stale(&snapshot);
                        emit(&events, &egui_ctx, Event::Catalog(snapshot));
                        if stale {
                            emit(&events, &egui_ctx, Event::CatalogRefreshing);
                            refresh_catalog(&context, &events, &egui_ctx).await;
                        }
                    }
                    Err(error) => emit(
                        &events,
                        &egui_ctx,
                        Event::Error(format!("Chargement du cache: {error}")),
                    ),
                }
                emit_library(&context, &events, &egui_ctx).await;
            }
            Command::RefreshCatalog => {
                emit(&events, &egui_ctx, Event::CatalogRefreshing);
                refresh_catalog(&context, &events, &egui_ctx).await;
            }
            Command::RefreshLibrary => {
                emit_library(&context, &events, &egui_ctx).await;
            }
            Command::Install { slug } => {
                let token = CancellationToken::new();
                if let Ok(mut active) = active_installs.lock() {
                    if let Some(previous) = active.insert(slug.clone(), token.clone()) {
                        previous.cancel();
                    }
                }
                emit(
                    &events,
                    &egui_ctx,
                    Event::InstallStarted {
                        slug: slug.clone(),
                        repaired: false,
                    },
                );
                spawn_install(
                    Arc::clone(&context),
                    slug,
                    events.clone(),
                    egui_ctx.clone(),
                    token,
                    Arc::clone(&active_installs),
                    false,
                );
            }
            Command::Repair { slug } => {
                let token = CancellationToken::new();
                if let Ok(mut active) = active_installs.lock() {
                    if let Some(previous) = active.insert(slug.clone(), token.clone()) {
                        previous.cancel();
                    }
                }
                emit(
                    &events,
                    &egui_ctx,
                    Event::InstallStarted {
                        slug: slug.clone(),
                        repaired: true,
                    },
                );
                spawn_install(
                    Arc::clone(&context),
                    slug,
                    events.clone(),
                    egui_ctx.clone(),
                    token,
                    Arc::clone(&active_installs),
                    true,
                );
            }
            Command::CancelInstall { slug } => {
                if let Ok(active) = active_installs.lock() {
                    if let Some(token) = active.get(&slug) {
                        token.cancel();
                    }
                }
                emit(
                    &events,
                    &egui_ctx,
                    Event::Info(format!("Annulation de l'installation de {slug}")),
                );
            }
            Command::SelectUserFile { slug } => {
                spawn_user_file(Arc::clone(&context), slug, events.clone(), egui_ctx.clone());
            }
            Command::Uninstall { slug, delete_saves } => {
                spawn_uninstall(
                    Arc::clone(&context),
                    slug,
                    delete_saves,
                    events.clone(),
                    egui_ctx.clone(),
                );
            }
            Command::LoadStorageStats => {
                spawn_storage_stats(Arc::clone(&context), events.clone(), egui_ctx.clone());
            }
            Command::Play { slug } => {
                spawn_play(Arc::clone(&context), slug, events.clone(), egui_ctx.clone());
            }
            Command::CreateModsDir { slug } => {
                spawn_mods_task(
                    Arc::clone(&context),
                    slug,
                    ModsAction::CreateDir,
                    events.clone(),
                    egui_ctx.clone(),
                );
            }
            Command::OpenModsDir { slug } => {
                spawn_mods_task(
                    Arc::clone(&context),
                    slug,
                    ModsAction::OpenDir,
                    events.clone(),
                    egui_ctx.clone(),
                );
            }
            Command::RefreshMods { slug } => {
                spawn_mods_task(
                    Arc::clone(&context),
                    slug,
                    ModsAction::Refresh,
                    events.clone(),
                    egui_ctx.clone(),
                );
            }
            Command::ToggleMod {
                slug,
                name,
                enabled,
            } => {
                spawn_mods_task(
                    Arc::clone(&context),
                    slug,
                    ModsAction::Toggle { name, enabled },
                    events.clone(),
                    egui_ctx.clone(),
                );
            }
            Command::ReorderMods { slug, names } => {
                spawn_mods_task(
                    Arc::clone(&context),
                    slug,
                    ModsAction::Reorder(names),
                    events.clone(),
                    egui_ctx.clone(),
                );
            }
            Command::PickMods { slug } => {
                spawn_pick_mods(Arc::clone(&context), slug, events.clone(), egui_ctx.clone());
            }
            Command::LoadMetadata { slug } => {
                spawn_metadata(Arc::clone(&context), slug, events.clone(), egui_ctx.clone());
            }
            Command::LoadServers { slug } => {
                spawn_server_list(
                    Arc::clone(&context),
                    slug,
                    ServerAction::Load,
                    events.clone(),
                    egui_ctx.clone(),
                );
            }
            Command::AddServer {
                slug,
                name,
                address,
                port,
            } => {
                spawn_server_list(
                    Arc::clone(&context),
                    slug,
                    ServerAction::Add {
                        name,
                        address,
                        port,
                    },
                    events.clone(),
                    egui_ctx.clone(),
                );
            }
            Command::RemoveServer { slug, id } => {
                spawn_server_list(
                    Arc::clone(&context),
                    slug,
                    ServerAction::Remove(id),
                    events.clone(),
                    egui_ctx.clone(),
                );
            }
            Command::PingServers { slug } => {
                spawn_server_list(
                    Arc::clone(&context),
                    slug,
                    ServerAction::Ping,
                    events.clone(),
                    egui_ctx.clone(),
                );
            }
            Command::DiscoverLan { slug } => {
                spawn_server_list(
                    Arc::clone(&context),
                    slug,
                    ServerAction::DiscoverLan,
                    events.clone(),
                    egui_ctx.clone(),
                );
            }
            Command::JoinServer {
                slug,
                address,
                port,
            } => {
                spawn_join(
                    Arc::clone(&context),
                    slug,
                    address,
                    port,
                    events.clone(),
                    egui_ctx.clone(),
                );
            }
            Command::LoadSettings => {
                spawn_load_settings(Arc::clone(&context), events.clone(), egui_ctx.clone());
            }
            Command::SaveSettings {
                language,
                dark_theme,
                diagnostics,
                proxy,
                manifest_url,
                servers_url,
            } => {
                spawn_save_settings(
                    Arc::clone(&context),
                    SettingsSnapshot {
                        language,
                        dark_theme,
                        diagnostics,
                        proxy,
                        manifest_url,
                        servers_url,
                        ..SettingsSnapshot::default()
                    },
                    events.clone(),
                    egui_ctx.clone(),
                );
            }
            Command::ClearCache => {
                spawn_clear_cache(Arc::clone(&context), events.clone(), egui_ctx.clone());
            }
            Command::ExportLogs => {
                spawn_export_logs(Arc::clone(&context), events.clone(), egui_ctx.clone());
            }
            Command::OpenDataDir => {
                let root = context.paths().root().to_path_buf();
                match opener::open(&root) {
                    Ok(()) => emit(
                        &events,
                        &egui_ctx,
                        Event::Info(format!("Dossier ouvert : {}", root.display())),
                    ),
                    Err(error) => emit(
                        &events,
                        &egui_ctx,
                        Event::Error(format!("Ouverture du dossier : {error}")),
                    ),
                }
            }
            Command::OpenUrl(url) => {
                if !url.starts_with("https://") {
                    emit(
                        &events,
                        &egui_ctx,
                        Event::Error(format!("URL refusée (HTTPS requis): {url}")),
                    );
                    continue;
                }
                let result = tokio::task::spawn_blocking(move || opener::open(url)).await;
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => emit(
                        &events,
                        &egui_ctx,
                        Event::Error(format!("Ouverture du lien: {error}")),
                    ),
                    Err(error) => emit(
                        &events,
                        &egui_ctx,
                        Event::Error(format!("Tâche d'ouverture: {error}")),
                    ),
                }
            }
            Command::OpenPath(path) => {
                if !path.exists() {
                    emit(
                        &events,
                        &egui_ctx,
                        Event::Error(format!("Chemin introuvable: {}", path.display())),
                    );
                    continue;
                }
                let result = tokio::task::spawn_blocking(move || opener::open(path)).await;
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => emit(
                        &events,
                        &egui_ctx,
                        Event::Error(format!("Ouverture du dossier: {error}")),
                    ),
                    Err(error) => emit(
                        &events,
                        &egui_ctx,
                        Event::Error(format!("Tâche d'ouverture: {error}")),
                    ),
                }
            }
        }
    }
}

async fn refresh_catalog(
    context: &Arc<AppContext>,
    events: &UnboundedSender<Event>,
    egui_ctx: &eframe::egui::Context,
) {
    match context.refresh_catalog().await {
        Ok(snapshot) => {
            for error in &snapshot.errors {
                tracing::warn!(%error, "catalog provider error");
            }
            emit(events, egui_ctx, Event::Catalog(snapshot));
        }
        Err(error) => emit(
            events,
            egui_ctx,
            Event::CatalogFailed(format!("Actualisation du catalogue: {error}")),
        ),
    }
}

async fn emit_library(
    context: &Arc<AppContext>,
    events: &UnboundedSender<Event>,
    egui_ctx: &eframe::egui::Context,
) {
    match context.library().list().await {
        Ok(games) => emit(events, egui_ctx, Event::Library(games)),
        Err(error) => emit(
            events,
            egui_ctx,
            Event::Error(format!("Chargement de la bibliothèque: {error}")),
        ),
    }
}

fn spawn_storage_stats(
    context: Arc<AppContext>,
    events: UnboundedSender<Event>,
    egui_ctx: eframe::egui::Context,
) {
    tokio::spawn(async move {
        let games = context.library().list().await.unwrap_or_default();
        let paths = context.paths().clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut stats = StorageStats {
                cache: fs_usage::dir_size(&paths.cache_dir()),
                ..StorageStats::default()
            };
            for game in games {
                let Some(path) = game.install_path.as_ref() else {
                    continue;
                };
                let size = fs_usage::dir_size(path);
                stats.games = stats.games.saturating_add(size);
                stats.per_game.insert(game.slug, size);
            }
            stats
        })
        .await;
        match result {
            Ok(stats) => emit(&events, &egui_ctx, Event::StorageStats(stats)),
            Err(error) => emit(
                &events,
                &egui_ctx,
                Event::Error(format!("Calcul de l'espace disque: {error}")),
            ),
        }
    });
}

fn spawn_load_settings(
    context: Arc<AppContext>,
    events: UnboundedSender<Event>,
    egui_ctx: eframe::egui::Context,
) {
    tokio::spawn(async move {
        match read_settings(&context).await {
            Ok(snapshot) => emit(&events, &egui_ctx, Event::Settings(snapshot)),
            Err(error) => emit(
                &events,
                &egui_ctx,
                Event::Error(format!("Lecture des paramètres : {error}")),
            ),
        }
    });
}

async fn read_settings(context: &AppContext) -> Result<SettingsSnapshot, String> {
    let settings = context.settings();
    let language = settings
        .get_or(KEY_LANGUAGE, "fr-FR")
        .await
        .map_err(|error| error.to_string())?;
    let theme = settings
        .get_or(KEY_THEME, "dark")
        .await
        .map_err(|error| error.to_string())?;
    let diagnostics = settings
        .get_or(KEY_DIAGNOSTICS, "false")
        .await
        .map_err(|error| error.to_string())?;
    Ok(SettingsSnapshot {
        language,
        dark_theme: theme != "light",
        diagnostics: diagnostics == "true",
        proxy: settings
            .get_or(KEY_PROXY, "")
            .await
            .map_err(|error| error.to_string())?,
        manifest_url: settings
            .get_or(KEY_CATALOG_MANIFEST_URL, "")
            .await
            .map_err(|error| error.to_string())?,
        servers_url: settings
            .get_or(KEY_SERVERS_URL, "")
            .await
            .map_err(|error| error.to_string())?,
        install_dir: settings
            .get_or(
                KEY_INSTALL_DIR,
                &context.paths().games_dir().display().to_string(),
            )
            .await
            .map_err(|error| error.to_string())?,
        data_dir: context.paths().root().display().to_string(),
        portable: context.paths().is_portable(),
    })
}

fn spawn_save_settings(
    context: Arc<AppContext>,
    snapshot: SettingsSnapshot,
    events: UnboundedSender<Event>,
    egui_ctx: eframe::egui::Context,
) {
    tokio::spawn(async move {
        let result = async {
            let language = if snapshot.language.to_ascii_lowercase().starts_with("en") {
                "en-US"
            } else {
                "fr-FR"
            };
            for (label, value) in [
                ("manifest", snapshot.manifest_url.trim()),
                ("servers", snapshot.servers_url.trim()),
            ] {
                if !value.is_empty() && !value.starts_with("https://") {
                    return Err(format!("{label} URL must use HTTPS"));
                }
            }
            let proxy = snapshot.proxy.trim();
            if !proxy.is_empty() && !proxy.starts_with("http://") && !proxy.starts_with("https://")
            {
                return Err("proxy URL must use HTTP or HTTPS".to_string());
            }
            for (key, value) in [
                (KEY_LANGUAGE, language),
                (
                    KEY_THEME,
                    if snapshot.dark_theme { "dark" } else { "light" },
                ),
                (
                    KEY_DIAGNOSTICS,
                    if snapshot.diagnostics {
                        "true"
                    } else {
                        "false"
                    },
                ),
                (KEY_PROXY, snapshot.proxy.trim()),
                (KEY_CATALOG_MANIFEST_URL, snapshot.manifest_url.trim()),
                (KEY_SERVERS_URL, snapshot.servers_url.trim()),
            ] {
                if value.is_empty() {
                    context
                        .settings()
                        .remove(key)
                        .await
                        .map_err(|error| error.to_string())?;
                } else {
                    context
                        .settings()
                        .set(key, value)
                        .await
                        .map_err(|error| error.to_string())?;
                }
            }
            read_settings(&context).await
        }
        .await;
        match result {
            Ok(snapshot) => {
                emit(&events, &egui_ctx, Event::Settings(snapshot));
                emit(
                    &events,
                    &egui_ctx,
                    Event::Info("Paramètres enregistrés.".to_string()),
                );
            }
            Err(error) => emit(
                &events,
                &egui_ctx,
                Event::Error(format!("Enregistrement des paramètres : {error}")),
            ),
        }
    });
}

fn spawn_clear_cache(
    context: Arc<AppContext>,
    events: UnboundedSender<Event>,
    egui_ctx: eframe::egui::Context,
) {
    tokio::spawn(async move {
        let result = async {
            context
                .catalog()
                .clear_cache()
                .await
                .map_err(|error| error.to_string())?;
            context
                .metadata()
                .clear_all()
                .await
                .map_err(|error| error.to_string())?;
            Ok::<(), String>(())
        }
        .await;
        match result {
            Ok(()) => emit(&events, &egui_ctx, Event::Info("Cache vidé.".to_string())),
            Err(error) => emit(
                &events,
                &egui_ctx,
                Event::Error(format!("Vidage du cache : {error}")),
            ),
        }
    });
}

fn spawn_export_logs(
    context: Arc<AppContext>,
    events: UnboundedSender<Event>,
    egui_ctx: eframe::egui::Context,
) {
    tokio::spawn(async move {
        let picked = tokio::task::spawn_blocking(|| {
            rfd::FileDialog::new()
                .set_title("Choisir le dossier d'export des logs")
                .pick_folder()
        })
        .await;
        let Some(destination) = picked.ok().flatten() else {
            return;
        };
        let logs = context.paths().logs_dir();
        let result = tokio::task::spawn_blocking(move || {
            let mut copied = 0_usize;
            for entry in std::fs::read_dir(&logs)? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    std::fs::copy(entry.path(), destination.join(entry.file_name()))?;
                    copied += 1;
                }
            }
            Ok::<usize, std::io::Error>(copied)
        })
        .await;
        match result {
            Ok(Ok(copied)) => emit(
                &events,
                &egui_ctx,
                Event::Info(format!("{copied} fichier(s) de log exporté(s).")),
            ),
            Ok(Err(error)) => emit(
                &events,
                &egui_ctx,
                Event::Error(format!("Export des logs : {error}")),
            ),
            Err(error) => emit(
                &events,
                &egui_ctx,
                Event::Error(format!("Export des logs : {error}")),
            ),
        }
    });
}

fn spawn_metadata(
    context: Arc<AppContext>,
    slug: String,
    events: UnboundedSender<Event>,
    egui_ctx: eframe::egui::Context,
) {
    tokio::spawn(async move {
        let entry = match context.catalog().find(&slug).await {
            Ok(Some(entry)) => entry,
            Ok(None) => {
                emit(
                    &events,
                    &egui_ctx,
                    Event::Metadata {
                        slug,
                        metadata: None,
                        from_cache: false,
                        stale: false,
                        error: Some("Jeu inconnu dans le catalogue.".to_string()),
                    },
                );
                return;
            }
            Err(error) => {
                emit(
                    &events,
                    &egui_ctx,
                    Event::Metadata {
                        slug,
                        metadata: None,
                        from_cache: false,
                        stale: false,
                        error: Some(format!("Lecture du catalogue: {error}")),
                    },
                );
                return;
            }
        };
        let title = entry
            .metadata
            .wikipedia_title
            .clone()
            .unwrap_or_else(|| entry.title.clone());
        match context.metadata().load(&slug, &title).await {
            Ok(outcome) => emit(
                &events,
                &egui_ctx,
                Event::Metadata {
                    slug,
                    metadata: Some(outcome.metadata),
                    from_cache: outcome.from_cache,
                    stale: outcome.stale,
                    error: None,
                },
            ),
            Err(error) => {
                tracing::warn!(%slug, %error, "metadata lookup failed");
                emit(
                    &events,
                    &egui_ctx,
                    Event::Metadata {
                        slug,
                        metadata: None,
                        from_cache: false,
                        stale: false,
                        error: Some(format!("Fiche Wikipédia indisponible: {error}")),
                    },
                );
            }
        }
    });
}

fn spawn_server_list(
    context: Arc<AppContext>,
    slug: String,
    action: ServerAction,
    events: UnboundedSender<Event>,
    egui_ctx: eframe::egui::Context,
) {
    tokio::spawn(async move {
        let mut already_emitted = false;
        let result = match action {
            ServerAction::Load => Ok(()),
            ServerAction::Add {
                name,
                address,
                port,
            } => {
                let server = ServerInfo {
                    id: None,
                    game_slug: slug.clone(),
                    name,
                    address,
                    port,
                    source: recomp_core::online::ServerSource::Manual,
                    last_ping_ms: None,
                    players: None,
                    max_players: None,
                    favorite: false,
                };
                context
                    .online()
                    .add_manual(&server)
                    .await
                    .map(|_| ())
                    .map_err(|error| format!("Ajout du serveur: {error}"))
            }
            ServerAction::Remove(id) => context
                .online()
                .remove_manual(id)
                .await
                .map(|_| ())
                .map_err(|error| format!("Suppression du serveur: {error}")),
            ServerAction::Ping => {
                let list = context.online().ping_all(&slug).await;
                emit(
                    &events,
                    &egui_ctx,
                    Event::Servers {
                        slug: slug.clone(),
                        list,
                    },
                );
                already_emitted = true;
                Ok(())
            }
            ServerAction::DiscoverLan => {
                let list = context.online().discover_lan(&slug).await;
                emit(
                    &events,
                    &egui_ctx,
                    Event::Servers {
                        slug: slug.clone(),
                        list,
                    },
                );
                already_emitted = true;
                Ok(())
            }
        };
        if let Err(message) = result {
            emit(&events, &egui_ctx, Event::Error(message));
        }
        if !already_emitted {
            let list = context.online().list(&slug).await;
            emit(&events, &egui_ctx, Event::Servers { slug, list });
        }
    });
}

fn spawn_join(
    context: Arc<AppContext>,
    slug: String,
    address: String,
    port: u16,
    events: UnboundedSender<Event>,
    egui_ctx: eframe::egui::Context,
) {
    tokio::spawn(async move {
        let entry = match context.catalog().find(&slug).await {
            Ok(Some(entry)) => entry,
            Ok(None) => {
                emit(
                    &events,
                    &egui_ctx,
                    Event::Error(format!("Jeu inconnu dans le catalogue: {slug}")),
                );
                return;
            }
            Err(error) => {
                emit(
                    &events,
                    &egui_ctx,
                    Event::Error(format!("Lecture du catalogue: {error}")),
                );
                return;
            }
        };
        if !entry.online.supported {
            emit(
                &events,
                &egui_ctx,
                Event::Error(format!("{} ne déclare pas d'option en ligne.", entry.title)),
            );
            return;
        }
        let args = join_entry_args(&entry, &address, port);
        if args.is_empty() {
            emit(
                &events,
                &egui_ctx,
                Event::Error(format!(
                    "{} n'a pas de modèle de connexion dans son manifeste.",
                    entry.title
                )),
            );
            return;
        }
        spawn_play_with_args(context, slug, args, events, egui_ctx);
    });
}

fn spawn_play(
    context: Arc<AppContext>,
    slug: String,
    events: UnboundedSender<Event>,
    egui_ctx: eframe::egui::Context,
) {
    spawn_play_with_args(context, slug, Vec::new(), events, egui_ctx);
}

fn spawn_play_with_args(
    context: Arc<AppContext>,
    slug: String,
    extra_args: Vec<String>,
    events: UnboundedSender<Event>,
    egui_ctx: eframe::egui::Context,
) {
    tokio::spawn(async move {
        let entry = match context.catalog().find(&slug).await {
            Ok(entry) => entry,
            Err(error) => {
                emit(
                    &events,
                    &egui_ctx,
                    Event::Error(format!("Lecture du catalogue: {error}")),
                );
                return;
            }
        };
        let game = match context.library().get(&slug).await {
            Ok(Some(game)) => game,
            Ok(None) => {
                emit(
                    &events,
                    &egui_ctx,
                    Event::Error(format!("{slug} n'est pas dans la bibliothèque.")),
                );
                return;
            }
            Err(error) => {
                emit(
                    &events,
                    &egui_ctx,
                    Event::Error(format!("Lecture de la bibliothèque: {error}")),
                );
                return;
            }
        };
        let title = entry
            .as_ref()
            .map(|entry| entry.title.clone())
            .unwrap_or_else(|| game.title.clone());
        let launched = if let Some(entry) = &entry {
            context
                .launcher()
                .launch_with_extra_args(entry, &game, &extra_args)
                .await
        } else if game.source == recomp_core::library::InstallSource::External {
            context
                .launcher()
                .launch_external_with_args(&game, &extra_args)
        } else {
            emit(
                &events,
                &egui_ctx,
                Event::Error(format!("Jeu inconnu dans le catalogue: {slug}")),
            );
            return;
        };
        match launched {
            Ok(running) => {
                let pid = running.pid;
                let executable = running.executable.clone();
                emit(
                    &events,
                    &egui_ctx,
                    Event::GameStarted {
                        slug: slug.clone(),
                        pid,
                        executable,
                    },
                );
                let launcher = context.launcher().clone();
                let context_for_wait = Arc::clone(&context);
                let events_for_wait = events.clone();
                let ctx_for_wait = egui_ctx.clone();
                tokio::spawn(async move {
                    match launcher.wait_for_exit(running).await {
                        Ok(exit) => {
                            emit(
                                &events_for_wait,
                                &ctx_for_wait,
                                Event::GameExited {
                                    slug: exit.slug.clone(),
                                    seconds: exit.seconds,
                                    code: exit.code,
                                    success: exit.success,
                                },
                            );
                            emit_library(&context_for_wait, &events_for_wait, &ctx_for_wait).await;
                        }
                        Err(error) => emit(
                            &events_for_wait,
                            &ctx_for_wait,
                            Event::Error(format!("Suivi du processus: {error}")),
                        ),
                    }
                });
            }
            Err(error) => emit(
                &events,
                &egui_ctx,
                Event::Error(format!("Lancement de {title}: {error}")),
            ),
        }
    });
}

fn spawn_mods_task(
    context: Arc<AppContext>,
    slug: String,
    action: ModsAction,
    events: UnboundedSender<Event>,
    egui_ctx: eframe::egui::Context,
) {
    tokio::spawn(async move {
        if let Err(message) = run_mods_action(&context, &slug, action, &events, &egui_ctx).await {
            emit(
                &events,
                &egui_ctx,
                Event::ModsFailed {
                    slug: slug.clone(),
                    message,
                },
            );
        }
    });
}

async fn run_mods_action(
    context: &Arc<AppContext>,
    slug: &str,
    action: ModsAction,
    events: &UnboundedSender<Event>,
    egui_ctx: &eframe::egui::Context,
) -> Result<(), String> {
    let entry = context
        .catalog()
        .find(slug)
        .await
        .map_err(|error| format!("Lecture du catalogue: {error}"))?
        .ok_or_else(|| format!("Jeu inconnu dans le catalogue: {slug}"))?;
    if !entry.mods.supported || entry.mods.strategy == ModStrategy::None {
        return Err(format!("{} ne supporte pas les mods.", entry.title));
    }
    match action {
        ModsAction::CreateDir => {
            let path = context
                .mods()
                .create_dir(slug, &entry.mods)
                .await
                .map_err(|error| format!("Création du dossier mods: {error}"))?;
            emit(
                events,
                egui_ctx,
                Event::ModsDirReady {
                    slug: slug.to_string(),
                    path,
                },
            );
            emit_mods(context, slug, events, egui_ctx).await?;
        }
        ModsAction::OpenDir => {
            let dir = context.mods().enabled_dir(slug, &entry.mods.dir_name);
            if !dir.is_dir() {
                return Err(format!(
                    "Le dossier mods n'existe pas encore: {}",
                    dir.display()
                ));
            }
            tokio::task::spawn_blocking(move || opener::open(dir))
                .await
                .map_err(|error| format!("Tâche d'ouverture: {error}"))?
                .map_err(|error| format!("Ouverture du dossier mods: {error}"))?;
        }
        ModsAction::Refresh => {
            emit_mods(context, slug, events, egui_ctx).await?;
        }
        ModsAction::Toggle { name, enabled } => {
            context
                .mods()
                .toggle(slug, &entry.mods, &name, enabled)
                .await
                .map_err(|error| format!("Activation du mod {name}: {error}"))?;
            emit_mods(context, slug, events, egui_ctx).await?;
        }
        ModsAction::Reorder(names) => {
            context
                .mods()
                .reorder(slug, &entry.mods, names)
                .await
                .map_err(|error| format!("Réordonnancement des mods: {error}"))?;
            emit_mods(context, slug, events, egui_ctx).await?;
        }
        ModsAction::Import(paths) => {
            let mut count = 0;
            for path in paths {
                context
                    .mods()
                    .import_path(slug, &entry.mods, &path)
                    .await
                    .map_err(|error| format!("Import de {}: {error}", path.display()))?;
                count += 1;
            }
            emit(
                events,
                egui_ctx,
                Event::ModsImported {
                    slug: slug.to_string(),
                    count,
                },
            );
            emit_mods(context, slug, events, egui_ctx).await?;
        }
    }
    Ok(())
}

async fn emit_mods(
    context: &Arc<AppContext>,
    slug: &str,
    events: &UnboundedSender<Event>,
    egui_ctx: &eframe::egui::Context,
) -> Result<(), String> {
    let entry = context
        .catalog()
        .find(slug)
        .await
        .map_err(|error| format!("Lecture du catalogue: {error}"))?
        .ok_or_else(|| format!("Jeu inconnu dans le catalogue: {slug}"))?;
    let snapshot = context
        .mods()
        .snapshot(slug, &entry.mods)
        .await
        .map_err(|error| format!("Scan des mods: {error}"))?;
    emit(
        events,
        egui_ctx,
        Event::Mods {
            slug: slug.to_string(),
            snapshot,
        },
    );
    Ok(())
}

fn spawn_pick_mods(
    context: Arc<AppContext>,
    slug: String,
    events: UnboundedSender<Event>,
    egui_ctx: eframe::egui::Context,
) {
    tokio::spawn(async move {
        let title = context
            .catalog()
            .find(&slug)
            .await
            .ok()
            .flatten()
            .map(|entry| entry.title)
            .unwrap_or_else(|| slug.clone());
        let files = rfd::AsyncFileDialog::new()
            .set_title(format!("Importer des mods pour {title}"))
            .add_filter("Archives", &["zip", "7z", "rar", "tar", "gz"])
            .add_filter("Tous les fichiers", &["*"])
            .pick_files()
            .await;
        let Some(files) = files else {
            return;
        };
        let paths: Vec<PathBuf> = files.iter().map(|file| file.path().to_path_buf()).collect();
        spawn_mods_task(context, slug, ModsAction::Import(paths), events, egui_ctx);
    });
}

fn spawn_install(
    context: Arc<AppContext>,
    slug: String,
    events: UnboundedSender<Event>,
    egui_ctx: eframe::egui::Context,
    cancel: CancellationToken,
    active: Arc<Mutex<HashMap<String, CancellationToken>>>,
    repaired: bool,
) {
    tokio::spawn(async move {
        let result = install_one(&context, &slug, &events, &egui_ctx, cancel, repaired).await;
        if let Ok(mut active) = active.lock() {
            active.remove(&slug);
        }
        match result {
            Ok(()) => {
                emit_library(&context, &events, &egui_ctx).await;
            }
            Err(error) => {
                emit(
                    &events,
                    &egui_ctx,
                    Event::InstallFailed {
                        slug: slug.clone(),
                        message: error,
                    },
                );
            }
        }
    });
}

async fn install_one(
    context: &Arc<AppContext>,
    slug: &str,
    events: &UnboundedSender<Event>,
    egui_ctx: &eframe::egui::Context,
    cancel: CancellationToken,
    repaired: bool,
) -> Result<(), String> {
    let entry = context
        .catalog()
        .find(slug)
        .await
        .map_err(|error| format!("Lecture du catalogue: {error}"))?
        .ok_or_else(|| format!("Jeu inconnu dans le catalogue: {slug}"))?;
    let os = Os::current();

    if repaired {
        if let Some(game) = context
            .library()
            .get(slug)
            .await
            .map_err(|error| format!("Lecture de la bibliothèque: {error}"))?
        {
            let health = context.installer().check_install(&entry, &game);
            match health {
                recomp_core::library::install::InstallHealth::Healthy => {
                    emit(
                        events,
                        egui_ctx,
                        Event::InstallFinished {
                            slug: slug.to_string(),
                            version: game.version.clone().unwrap_or_default(),
                            user_file_required: entry
                                .user_files
                                .as_ref()
                                .is_some_and(|user_files| user_files.required),
                            repaired: true,
                        },
                    );
                    return Ok(());
                }
                recomp_core::library::install::InstallHealth::MissingUserFile(_) => {
                    emit(
                        events,
                        egui_ctx,
                        Event::Info(format!(
                            "{} est intact, mais un fichier utilisateur est requis.",
                            entry.title
                        )),
                    );
                    emit(
                        events,
                        egui_ctx,
                        Event::InstallFinished {
                            slug: slug.to_string(),
                            version: game.version.clone().unwrap_or_default(),
                            user_file_required: true,
                            repaired: true,
                        },
                    );
                    return Ok(());
                }
                other => emit(
                    events,
                    egui_ctx,
                    Event::Info(format!("Réparation : {}", other.message())),
                ),
            }
        }
    }

    if !entry.supports_os(os) {
        return Err(format!(
            "{} n'a pas de binaire {} dans son manifeste.",
            entry.title,
            os.label()
        ));
    }

    let token = context
        .settings()
        .get(KEY_GITHUB_TOKEN)
        .await
        .map_err(|error| format!("Lecture du token GitHub: {error}"))?;
    let provider =
        GitHubReleasesProvider::from_repo_url(context.http().clone(), &entry.project.repo, token)
            .map_err(|error| format!("Dépôt GitHub: {error}"))?;
    let release = provider
        .fetch_latest()
        .await
        .map_err(|error| format!("Récupération de la release: {error}"))?
        .ok_or_else(|| format!("Aucune release publiée pour {}.", entry.title))?;
    let selected: SelectedAsset = select_asset(&entry, &release, os).ok_or_else(|| {
        format!(
            "Aucun asset {} ne correspond au manifeste dans la release {}.",
            os.label(),
            release.tag_name
        )
    })?;

    let asset = AssetDownload {
        url: selected.asset.browser_download_url.clone(),
        file_name: selected.asset.name.clone(),
        expected_sha256: selected.expected_sha256,
        size: (selected.asset.size > 0).then_some(selected.asset.size),
    };

    let (progress_tx, mut progress_rx) =
        tokio::sync::mpsc::unbounded_channel::<recomp_core::net::download::DownloadProgress>();
    let slug_for_progress = slug.to_string();
    let events_for_progress = events.clone();
    let egui_for_progress = egui_ctx.clone();
    let progress_task = tokio::spawn(async move {
        while let Some(progress) = progress_rx.recv().await {
            emit(
                &events_for_progress,
                &egui_for_progress,
                Event::InstallProgress {
                    slug: slug_for_progress.clone(),
                    downloaded: progress.downloaded,
                    total: progress.total,
                },
            );
        }
    });

    let version = release.tag_name.clone();
    let installer = context.installer();
    let installed = if repaired {
        installer
            .repair(&entry, &version, &asset, Some(progress_tx), Some(cancel))
            .await
    } else {
        installer
            .install(&entry, &version, &asset, Some(progress_tx), Some(cancel))
            .await
    };
    let _ = progress_task.await;
    let installed =
        installed.map_err(|error| format!("Installation de {}: {error}", entry.title))?;

    if let Some(install_dir) = installed.game.install_dir() {
        if let Err(error) = context.adapters().post_install(slug, install_dir) {
            tracing::warn!(%slug, %error, "adapter post_install failed");
        }
    }

    emit(
        events,
        egui_ctx,
        Event::InstallFinished {
            slug: slug.to_string(),
            version: installed
                .game
                .version
                .clone()
                .unwrap_or_else(|| version.clone()),
            user_file_required: installed.user_file_required,
            repaired,
        },
    );
    if installed.user_file_required {
        emit(
            events,
            egui_ctx,
            Event::Info(format!(
                "{} est installé. Sélectionnez votre propre dump pour terminer.",
                entry.title
            )),
        );
    }
    Ok(())
}

fn spawn_user_file(
    context: Arc<AppContext>,
    slug: String,
    events: UnboundedSender<Event>,
    egui_ctx: eframe::egui::Context,
) {
    tokio::spawn(async move {
        let entry = match context.catalog().find(&slug).await {
            Ok(Some(entry)) => entry,
            Ok(None) => {
                emit(
                    &events,
                    &egui_ctx,
                    Event::Error(format!("Jeu inconnu: {slug}")),
                );
                return;
            }
            Err(error) => {
                emit(
                    &events,
                    &egui_ctx,
                    Event::Error(format!("Lecture du catalogue: {error}")),
                );
                return;
            }
        };
        let mut dialog = rfd::AsyncFileDialog::new()
            .set_title(format!("Sélectionnez votre fichier pour {}", entry.title));
        if let Some(user_files) = &entry.user_files {
            for extension in &user_files.prompt_ext {
                let extension = extension.trim_start_matches('.');
                dialog = dialog.add_filter(extension, &[extension]);
            }
        }
        let Some(file) = dialog.pick_file().await else {
            emit(
                &events,
                &egui_ctx,
                Event::UserFileCheck {
                    slug: slug.clone(),
                    status: UserFileCheckStatus::Cancelled,
                },
            );
            return;
        };
        let path = file.path().to_path_buf();
        match InstallManager::verify_user_file(&entry, &path) {
            Ok(verdict) if verdict.is_acceptable() => {
                if let Err(error) = context.library().set_user_file(&slug, Some(&path)).await {
                    emit(
                        &events,
                        &egui_ctx,
                        Event::Error(format!("Enregistrement du fichier: {error}")),
                    );
                    return;
                }
                let _ = context
                    .settings()
                    .set_user_file(&slug, &path.to_string_lossy())
                    .await;
                let label = match verdict {
                    recomp_core::hash::HashVerdict::Match { algo, label } => label
                        .map(|label| format!("{label} ({algo})"))
                        .unwrap_or_else(|| format!("hash {algo} accepté")),
                    _ => "fichier accepté (aucun hash publié)".to_string(),
                };
                emit(
                    &events,
                    &egui_ctx,
                    Event::UserFileCheck {
                        slug: slug.clone(),
                        status: UserFileCheckStatus::Verified(label),
                    },
                );
            }
            Ok(recomp_core::hash::HashVerdict::Mismatch { computed, accepted }) => {
                let computed = computed
                    .iter()
                    .map(|(algo, value)| format!("{algo}={value}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                emit(
                    &events,
                    &egui_ctx,
                    Event::UserFileCheck {
                        slug: slug.clone(),
                        status: UserFileCheckStatus::Rejected(format!(
                            "Hash refusé. Calculé : {computed}. Attendu : {}",
                            accepted.join(", ")
                        )),
                    },
                );
            }
            Ok(_) => {
                emit(
                    &events,
                    &egui_ctx,
                    Event::UserFileCheck {
                        slug: slug.clone(),
                        status: UserFileCheckStatus::Rejected(
                            "Fichier non vérifiable.".to_string(),
                        ),
                    },
                );
            }
            Err(error) => emit(
                &events,
                &egui_ctx,
                Event::UserFileCheck {
                    slug: slug.clone(),
                    status: UserFileCheckStatus::Rejected(format!(
                        "Vérification du fichier : {error}"
                    )),
                },
            ),
        }
    });
}

fn spawn_uninstall(
    context: Arc<AppContext>,
    slug: String,
    delete_saves: bool,
    events: UnboundedSender<Event>,
    egui_ctx: eframe::egui::Context,
) {
    tokio::spawn(async move {
        let game = match context.library().get(&slug).await {
            Ok(Some(game)) => game,
            Ok(None) => {
                emit(
                    &events,
                    &egui_ctx,
                    Event::Error(format!("Jeu absent de la bibliothèque: {slug}")),
                );
                return;
            }
            Err(error) => {
                emit(
                    &events,
                    &egui_ctx,
                    Event::Error(format!("Lecture de la bibliothèque: {error}")),
                );
                return;
            }
        };
        if let Err(error) = context.library().remove(&slug).await {
            emit(
                &events,
                &egui_ctx,
                Event::Error(format!("Suppression de la fiche: {error}")),
            );
            return;
        }
        if game.source == recomp_core::library::InstallSource::Managed {
            if let Some(path) = game.install_path.clone() {
                let path_for_remove = path.clone();
                match tokio::task::spawn_blocking(move || {
                    remove_install_preserving_userdata(&path_for_remove)
                })
                .await
                {
                    Ok(Ok(true)) => emit(
                        &events,
                        &egui_ctx,
                        Event::Info(format!(
                            "Fiche supprimée. userdata/ et mods/ ont été conservés dans {}.",
                            path.display()
                        )),
                    ),
                    Ok(Ok(false)) => {}
                    Ok(Err(error)) => emit(
                        &events,
                        &egui_ctx,
                        Event::Error(format!(
                            "Fiche supprimée, mais fichiers non effacés ({}): {error}",
                            path.display()
                        )),
                    ),
                    Err(error) => emit(
                        &events,
                        &egui_ctx,
                        Event::Error(format!("Tâche de suppression: {error}")),
                    ),
                }
            }
        }
        if delete_saves {
            let userdata = context.paths().userdata_dir(&slug);
            let path_for_remove = userdata.clone();
            match tokio::task::spawn_blocking(move || std::fs::remove_dir_all(&path_for_remove))
                .await
            {
                Ok(Ok(())) => emit(
                    &events,
                    &egui_ctx,
                    Event::Info(format!(
                        "Sauvegardes supprimées dans {}.",
                        userdata.display()
                    )),
                ),
                Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {}
                Ok(Err(error)) => emit(
                    &events,
                    &egui_ctx,
                    Event::Error(format!(
                        "Sauvegardes non supprimées ({}): {error}",
                        userdata.display()
                    )),
                ),
                Err(error) => emit(
                    &events,
                    &egui_ctx,
                    Event::Error(format!("Tâche de suppression des sauvegardes: {error}")),
                ),
            }
        }
        emit(&events, &egui_ctx, Event::Uninstalled { slug });
        emit_library(&context, &events, &egui_ctx).await;
    });
}
