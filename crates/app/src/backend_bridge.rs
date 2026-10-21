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
use recomp_core::library::install::{
    remove_install_preserving_userdata, AssetDownload, InstallManager,
};
use recomp_core::library::LibraryGame;
use recomp_core::mods::ModsSnapshot;
use recomp_core::settings::KEY_GITHUB_TOKEN;
use recomp_core::AppContext;

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
    ImportMods {
        slug: String,
        paths: Vec<PathBuf>,
    },
    OpenUrl(String),
    OpenPath(PathBuf),
}

/// Results pushed back to the UI.
#[derive(Debug, Clone)]
pub enum Event {
    Catalog(CatalogSnapshot),
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
    UserFileVerified {
        slug: String,
        label: String,
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
            Command::Uninstall { slug } => {
                spawn_uninstall(Arc::clone(&context), slug, events.clone(), egui_ctx.clone());
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
            Command::ImportMods { slug, paths } => {
                spawn_mods_task(
                    Arc::clone(&context),
                    slug,
                    ModsAction::Import(paths),
                    events.clone(),
                    egui_ctx.clone(),
                );
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
            Event::Error(format!("Actualisation du catalogue: {error}")),
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

fn spawn_play(
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
        match context.launcher().launch(&entry, &game).await {
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
                Event::Error(format!("Lancement de {}: {error}", entry.title)),
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
                Event::Info("Sélection annulée.".to_string()),
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
                    Event::UserFileVerified {
                        slug: slug.clone(),
                        label,
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
                    Event::Error(format!(
                        "Hash refusé pour {}. Calculé: {computed}. Attendu: {}",
                        entry.title,
                        accepted.join(", ")
                    )),
                );
            }
            Ok(_) => {
                emit(
                    &events,
                    &egui_ctx,
                    Event::Error("Fichier non vérifiable.".to_string()),
                );
            }
            Err(error) => emit(
                &events,
                &egui_ctx,
                Event::Error(format!("Vérification du fichier: {error}")),
            ),
        }
    });
}

fn spawn_uninstall(
    context: Arc<AppContext>,
    slug: String,
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
        emit(&events, &egui_ctx, Event::Uninstalled { slug });
        emit_library(&context, &events, &egui_ctx).await;
    });
}
