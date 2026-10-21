//! Main application shell: custom window chrome, sidebar navigation, event
//! pump and page routing.

use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, Id, Layout, Margin, Order, Rect, Sense, Stroke,
    Vec2, ViewportCommand,
};
use recomp_core::paths::AppPaths;
use recomp_core::AppContext;
use tokio::runtime::Runtime;

use crate::backend_bridge::{BackendBridge, Command, Event, UserFileCheckStatus};
use crate::ui::text::Weight;
use crate::ui::{components, fonts, icons, text, theme};
use crate::views::catalog::CatalogUi;
use crate::views::game_detail::DetailUi;
use crate::views::install_modal::{self, InstallModal};
use crate::views::library::LibraryUi;
use crate::views::settings::SettingsUi;
use crate::views::uninstall_modal::{self, UninstallModal};
use crate::views::{catalog, game_detail, library, settings, Action, InstallProgress, Store};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Catalog,
    Library,
    Settings,
}

impl Page {
    fn label(self) -> &'static str {
        match self {
            Self::Catalog => "Catalogue",
            Self::Library => "Bibliothèque",
            Self::Settings => "Paramètres",
        }
    }

    fn glyph(self) -> char {
        match self {
            Self::Catalog => icons::GRID_2X2,
            Self::Library => icons::LIBRARY,
            Self::Settings => icons::SETTINGS,
        }
    }
}

struct Toast {
    message: String,
    error: bool,
    until: Instant,
}

pub struct LauncherApp {
    bridge: BackendBridge,
    paths: AppPaths,
    page: Page,
    detail_slug: Option<String>,
    detail_from_library: bool,
    dark_theme: bool,
    applied_dark: Option<bool>,
    store: Store,
    catalog_ui: CatalogUi,
    library_ui: LibraryUi,
    detail_ui: DetailUi,
    settings_ui: SettingsUi,
    install_modal: Option<InstallModal>,
    uninstall_modal: Option<UninstallModal>,
    toast: Option<Toast>,
}

impl LauncherApp {
    pub fn new(
        creation_context: &eframe::CreationContext<'_>,
        context: AppContext,
        runtime: Runtime,
    ) -> Self {
        fonts::install(&creation_context.egui_ctx);
        theme::apply(&creation_context.egui_ctx, true);
        let window_shadow = theme::current(&creation_context.egui_ctx).modal_shadow;
        creation_context.egui_ctx.style_mut(|style| {
            style.visuals.window_shadow = window_shadow;
        });

        let paths = context.paths().clone();
        let bridge = BackendBridge::new(
            runtime,
            std::sync::Arc::new(context),
            creation_context.egui_ctx.clone(),
        );
        bridge.send(Command::Bootstrap);
        bridge.send(Command::LoadSettings);
        bridge.send(Command::LoadStorageStats);

        Self {
            bridge,
            paths,
            page: Page::Catalog,
            detail_slug: None,
            detail_from_library: false,
            dark_theme: true,
            applied_dark: Some(true),
            store: Store::default(),
            catalog_ui: CatalogUi::default(),
            library_ui: LibraryUi::default(),
            detail_ui: DetailUi::default(),
            settings_ui: SettingsUi::default(),
            install_modal: None,
            uninstall_modal: None,
            toast: None,
        }
    }

    fn set_toast(&mut self, message: impl Into<String>, error: bool) {
        let seconds = if error { 8 } else { 6 };
        self.toast = Some(Toast {
            message: message.into(),
            error,
            until: Instant::now() + Duration::from_secs(seconds),
        });
    }

    fn game_title(&self, slug: &str) -> String {
        self.store
            .entry(slug)
            .map(|entry| entry.title.clone())
            .or_else(|| self.store.library_game(slug).map(|game| game.title.clone()))
            .unwrap_or_else(|| slug.to_owned())
    }

    fn pump_events(&mut self, ctx: &egui::Context) {
        let sender = self.bridge.sender().clone();
        for event in self.bridge.drain() {
            match event {
                Event::Catalog(snapshot) => {
                    self.store.refreshing = false;
                    self.store.apply_snapshot(snapshot);
                }
                Event::CatalogFailed(message) => {
                    self.store.refreshing = false;
                    self.set_toast(message, true);
                }
                Event::CatalogRefreshing => {
                    self.store.refreshing = true;
                }
                Event::Library(games) => {
                    self.store.apply_library(games);
                    let _ = sender.send(Command::LoadStorageStats);
                }
                Event::InstallStarted { slug, repaired } => {
                    self.store.installs.insert(
                        slug.clone(),
                        InstallProgress {
                            downloaded: 0,
                            total: None,
                            repairing: repaired,
                        },
                    );
                    if let Some(modal) = &mut self.install_modal {
                        if modal.slug == slug {
                            modal.on_started(repaired);
                        }
                    }
                    self.set_toast(
                        if repaired {
                            "Réparation en cours…"
                        } else {
                            "Installation en cours…"
                        },
                        false,
                    );
                }
                Event::InstallProgress {
                    slug,
                    downloaded,
                    total,
                } => {
                    let repairing = self
                        .store
                        .installs
                        .get(&slug)
                        .map(|progress| progress.repairing)
                        .unwrap_or(false);
                    self.store.installs.insert(
                        slug.clone(),
                        InstallProgress {
                            downloaded,
                            total,
                            repairing,
                        },
                    );
                    if let Some(modal) = &mut self.install_modal {
                        if modal.slug == slug {
                            modal.on_progress(downloaded, total);
                        }
                    }
                }
                Event::InstallFinished {
                    slug,
                    version,
                    user_file_required,
                    repaired,
                } => {
                    self.store.installs.remove(&slug);
                    if let Some(modal) = &mut self.install_modal {
                        if modal.slug == slug {
                            modal.on_finished(version.clone(), user_file_required, repaired);
                        }
                    }
                    let _ = sender.send(Command::RefreshLibrary);
                    let _ = sender.send(Command::LoadStorageStats);
                    self.set_toast(
                        format!(
                            "{} terminée : {version}",
                            if repaired {
                                "Réparation"
                            } else {
                                "Installation"
                            }
                        ),
                        false,
                    );
                }
                Event::InstallFailed { slug, message } => {
                    self.store.installs.remove(&slug);
                    if let Some(modal) = &mut self.install_modal {
                        if modal.slug == slug {
                            modal.on_failed(message.clone());
                        }
                    }
                    self.set_toast(message, true);
                }
                Event::UserFileCheck { slug, status } => {
                    let rejected = matches!(&status, UserFileCheckStatus::Rejected(_));
                    if let Some(modal) = &mut self.install_modal {
                        if modal.slug == slug {
                            modal.on_file_check(status.clone());
                        }
                    }
                    let message = match status {
                        UserFileCheckStatus::Verified(label) => {
                            format!("Fichier utilisateur vérifié : {label}")
                        }
                        UserFileCheckStatus::Rejected(message) => message,
                        UserFileCheckStatus::Cancelled => "Sélection annulée.".to_owned(),
                    };
                    self.set_toast(message, rejected);
                    let _ = sender.send(Command::RefreshLibrary);
                }
                Event::Uninstalled { slug } => {
                    self.store.library.retain(|game| game.slug != slug);
                    self.store.installs.remove(&slug);
                    if let Some(modal) = &mut self.uninstall_modal {
                        if modal.slug == slug {
                            modal.on_uninstalled();
                        }
                    }
                    let _ = sender.send(Command::RefreshLibrary);
                    let _ = sender.send(Command::LoadStorageStats);
                    self.set_toast("Jeu désinstallé.", false);
                }
                Event::GameStarted {
                    slug,
                    pid,
                    executable,
                } => {
                    tracing::debug!(pid, ?executable, %slug, "game process started");
                    let title = self.game_title(&slug);
                    self.set_toast(format!("{title} est lancé."), false);
                }
                Event::GameExited {
                    slug,
                    seconds,
                    success,
                    code,
                } => {
                    tracing::debug!(?code, %slug, "game process exited");
                    let title = self.game_title(&slug);
                    self.set_toast(
                        format!(
                            "{title} terminé après {} ({})",
                            crate::ui::format::duration(seconds),
                            if success { "succès" } else { "échec" }
                        ),
                        !success,
                    );
                    let _ = sender.send(Command::RefreshLibrary);
                }
                Event::Mods { slug, snapshot } => {
                    self.store.apply_mods(slug, snapshot);
                }
                Event::ModsDirReady { slug, path } => {
                    let title = self.game_title(&slug);
                    self.set_toast(
                        format!("Dossier de mods de {title} prêt : {}", path.display()),
                        false,
                    );
                }
                Event::ModsImported { slug, count } => {
                    let title = self.game_title(&slug);
                    self.set_toast(format!("{count} mod(s) importé(s) pour {title}."), false);
                }
                Event::ModsFailed { slug, message } => {
                    self.store.mods_failed(&slug, message.clone());
                    self.set_toast(message, true);
                }
                Event::Metadata {
                    slug,
                    metadata,
                    from_cache,
                    stale,
                    error,
                } => {
                    self.store
                        .apply_metadata(slug, metadata, from_cache, stale, error);
                }
                Event::Servers { slug, list } => {
                    self.store.apply_servers(slug, list);
                }
                Event::StorageStats(stats) => {
                    self.store.stats = stats;
                }
                Event::Settings(snapshot) => {
                    self.dark_theme = snapshot.dark_theme;
                    self.settings_ui.apply(snapshot);
                    if self.applied_dark != Some(self.dark_theme) {
                        theme::apply(ctx, self.dark_theme);
                        self.applied_dark = Some(self.dark_theme);
                    }
                }
                Event::Info(message) => self.set_toast(message, false),
                Event::Error(message) => {
                    tracing::error!(%message, "backend error");
                    if let Some(modal) = &mut self.install_modal {
                        if modal.step == 1 && modal.error.is_none() {
                            modal.on_failed(message.clone());
                        }
                    }
                    if let Some(modal) = &mut self.uninstall_modal {
                        if modal.step == 1 {
                            modal.on_failed(message.clone());
                        }
                    }
                    self.set_toast(message, true);
                }
            }
        }
    }

    fn handle_action(&mut self, action: Action) {
        let sender = self.bridge.sender().clone();
        match action {
            Action::None => {}
            Action::OpenGame { slug, from_library } => {
                self.page = if from_library {
                    Page::Library
                } else {
                    Page::Catalog
                };
                self.detail_slug = Some(slug.clone());
                self.detail_from_library = from_library;
                self.detail_ui = DetailUi::default();
                self.ensure_game_data(&slug, &sender);
            }
            Action::BackToLibrary => {
                self.detail_slug = None;
                self.page = if self.detail_from_library {
                    Page::Library
                } else {
                    Page::Catalog
                };
            }
            Action::BrowseCatalog => {
                self.detail_slug = None;
                self.page = Page::Catalog;
            }
            Action::Install(slug) => {
                self.ensure_game_data(&slug, &sender);
                let mut modal = InstallModal::new(slug.clone());
                if let Some(progress) = self.store.playing(&slug) {
                    modal.on_started(progress.repairing);
                    modal.on_progress(progress.downloaded, progress.total);
                }
                self.install_modal = Some(modal);
            }
            Action::Uninstall(slug) => {
                self.uninstall_modal = Some(UninstallModal::new(slug));
            }
            Action::Play(slug) => {
                let _ = sender.send(Command::Play { slug });
            }
            Action::SelectUserFile(slug) => {
                let _ = sender.send(Command::SelectUserFile { slug });
            }
            Action::RefreshCatalog => {
                self.catalog_ui.begin_refresh();
                self.store.refreshing = true;
                let _ = sender.send(Command::RefreshCatalog);
            }
        }
    }

    fn ensure_game_data(
        &mut self,
        slug: &str,
        sender: &tokio::sync::mpsc::UnboundedSender<Command>,
    ) {
        let Some(entry) = self.store.entry(slug).cloned() else {
            return;
        };
        if self.store.request_metadata(slug) {
            let _ = sender.send(Command::LoadMetadata {
                slug: slug.to_owned(),
            });
        }
        if entry.mods.supported && self.store.request_mods(slug) {
            let _ = sender.send(Command::RefreshMods {
                slug: slug.to_owned(),
            });
        }
        if entry.online.supported && self.store.request_servers(slug) {
            let _ = sender.send(Command::LoadServers {
                slug: slug.to_owned(),
            });
        }
    }

    fn go_to_page(&mut self, page: Page) {
        self.page = page;
        self.detail_slug = None;
        let sender = self.bridge.sender().clone();
        match page {
            Page::Library | Page::Settings => {
                let _ = sender.send(Command::RefreshLibrary);
                let _ = sender.send(Command::LoadStorageStats);
            }
            Page::Catalog => {}
        }
    }

    fn topbar(&mut self, ctx: &egui::Context) {
        let tokens = theme::current(ctx);
        egui::TopBottomPanel::top("recomp-topbar")
            .exact_height(48.0)
            .frame(
                egui::Frame::new()
                    .fill(tokens.app_bg)
                    .inner_margin(Margin::symmetric(16, 0)),
            )
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                ui.painter().line_segment(
                    [rect.left_bottom(), rect.right_bottom()],
                    Stroke::new(1.0_f32, tokens.border),
                );

                // Register the drag zone before the toolbar content so the
                // window buttons and status text always win hit-testing.
                let drag_rect = Rect::from_min_max(
                    egui::pos2(rect.left() + 190.0, rect.top()),
                    egui::pos2(rect.right() - 280.0, rect.bottom()),
                );
                let drag = ui.interact(drag_rect, Id::new("topbar-drag"), Sense::click_and_drag());
                if drag.drag_started() {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }
                if drag.double_clicked() {
                    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                }

                ui.horizontal_centered(|ui| {
                    let (logo, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
                    crate::ui::art::brand(ui, logo);
                    icons::paint(
                        ui.painter(),
                        logo.center(),
                        icons::GAMEPAD_2,
                        17.0,
                        Color32::WHITE,
                    );
                    ui.add_space(8.0);
                    text::label(ui, "RecompLauncher", 15.0, Weight::SemiBold, tokens.text);
                    ui.add_space(12.0);
                    let (separator, _) =
                        ui.allocate_exact_size(Vec2::new(1.0, 20.0), Sense::hover());
                    ui.painter()
                        .rect_filled(separator, CornerRadius::ZERO, tokens.border);
                    ui.add_space(12.0);
                    let page_title = self
                        .detail_slug
                        .as_ref()
                        .and_then(|slug| self.store.entry(slug))
                        .map(|entry| entry.title.clone())
                        .unwrap_or_else(|| self.page.label().to_owned());
                    let page_title =
                        text::ellipsize(ctx, &page_title, text::font(11.0, Weight::Medium), 360.0);
                    text::label(ui, page_title, 11.0, Weight::Medium, tokens.text_2);

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if window_button(ui, icons::X, true).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Close);
                        }
                        if window_button(ui, icons::SQUARE, false).clicked() {
                            let maximized =
                                ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                            ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                        }
                        if window_button(ui, icons::MINUS, false).clicked() {
                            ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
                        }
                        ui.add_space(10.0);
                        if self.store.refreshing {
                            text::label(
                                ui,
                                "Synchronisation…",
                                9.0,
                                Weight::Regular,
                                tokens.text_3,
                            );
                            components::spinner(ui, 13.0, tokens.text_3);
                        } else if self.store.offline {
                            text::label(ui, "Hors ligne", 9.0, Weight::Medium, tokens.orange);
                            components::dot(ui, tokens.orange);
                        } else {
                            text::label(ui, "En ligne", 9.0, Weight::Medium, tokens.text_2);
                            components::dot(ui, tokens.green);
                        }
                    });
                });
            });
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        let tokens = theme::current(ctx);
        egui::SidePanel::left("recomp-sidebar")
            .exact_width(228.0)
            .resizable(false)
            .frame(
                egui::Frame::new()
                    .fill(tokens.surface)
                    .inner_margin(Margin::symmetric(14, 16)),
            )
            .show(ctx, |ui| {
                let border_x = ui.max_rect().right() + 1.0;
                ui.painter().line_segment(
                    [
                        egui::pos2(border_x, ui.max_rect().top()),
                        egui::pos2(border_x, ui.max_rect().bottom()),
                    ],
                    Stroke::new(1.0_f32, tokens.border),
                );

                for page in [Page::Catalog, Page::Library] {
                    let active = self.page == page;
                    if components::selectable_row(ui, page.label(), page.glyph(), active, 38.0)
                        .clicked()
                    {
                        self.go_to_page(page);
                    }
                    ui.add_space(2.0);
                }

                ui.add_space(18.0);
                ui.horizontal(|ui| {
                    text::label(ui, "MA BIBLIOTHÈQUE", 8.0, Weight::Bold, tokens.text_3);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        text::label(
                            ui,
                            self.store.library.len().to_string(),
                            8.0,
                            Weight::Medium,
                            tokens.text_3,
                        );
                    });
                });
                ui.add_space(8.0);

                let list_height = (ui.available_height() - 150.0).max(80.0);
                let mut open = None;
                egui::ScrollArea::vertical()
                    .max_height(list_height)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if self.store.library.is_empty() {
                            text::paragraph(ui, "Aucun jeu installé.", 9.0, tokens.text_3, 15.0);
                        }
                        for game in &self.store.library {
                            let active = self.detail_slug.as_deref() == Some(game.slug.as_str());
                            let needs_file = self.store.needs_user_file(&game.slug);
                            if sidebar_game(ui, game, active, needs_file).clicked() {
                                open = Some(game.slug.clone());
                            }
                        }
                    });
                if let Some(slug) = open {
                    self.handle_action(Action::OpenGame {
                        slug,
                        from_library: true,
                    });
                }

                let remaining = ui.available_height();
                if remaining > 140.0 {
                    ui.add_space(remaining - 140.0);
                }
                let settings_active = self.page == Page::Settings;
                if components::selectable_row(
                    ui,
                    "Paramètres",
                    icons::SETTINGS,
                    settings_active,
                    38.0,
                )
                .clicked()
                {
                    self.go_to_page(Page::Settings);
                }
                ui.add_space(10.0);
                components::divider(ui);
                ui.add_space(10.0);
                text::label(ui, "THÈME", 8.0, Weight::Bold, tokens.text_3);
                ui.add_space(5.0);
                let before = self.dark_theme;
                components::segmented(ui, &mut self.dark_theme);
                if self.dark_theme != before && self.settings_ui.loaded {
                    let sender = self.bridge.sender().clone();
                    self.settings_ui.save(self.dark_theme, &sender);
                }
            });
    }

    fn central(&mut self, ctx: &egui::Context) {
        let tokens = theme::current(ctx);
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(tokens.app_bg)
                    .inner_margin(Margin::symmetric(32, 24)),
            )
            .show(ctx, |ui| {
                let compact = ui.available_width() < 980.0;
                let action = if let Some(slug) = self.detail_slug.clone() {
                    game_detail::show(
                        ui,
                        &mut self.store,
                        &mut self.detail_ui,
                        &slug,
                        self.detail_from_library,
                        self.bridge.sender(),
                    )
                } else {
                    match self.page {
                        Page::Catalog => {
                            catalog::show(ui, &mut self.store, &mut self.catalog_ui, compact)
                        }
                        Page::Library => library::show(ui, &mut self.store, &mut self.library_ui),
                        Page::Settings => {
                            settings::show(
                                ui,
                                &self.store,
                                &mut self.settings_ui,
                                &mut self.dark_theme,
                                self.bridge.sender(),
                            );
                            Action::None
                        }
                    }
                };
                self.handle_action(action);
            });
    }

    fn render_overlays(&mut self, ctx: &egui::Context) {
        let sender = self.bridge.sender().clone();
        let mut close_install = false;
        if let Some(modal) = &mut self.install_modal {
            close_install = install_modal::show(ctx, modal, &self.store, &self.paths, &sender);
        }
        if close_install {
            self.install_modal = None;
        }

        let mut close_uninstall = false;
        if let Some(modal) = &mut self.uninstall_modal {
            close_uninstall = uninstall_modal::show(ctx, modal, &self.store, &sender);
        }
        if close_uninstall {
            self.uninstall_modal = None;
        }
    }

    fn toast(&mut self, ctx: &egui::Context) {
        let Some(toast) = &self.toast else {
            return;
        };
        if Instant::now() >= toast.until {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(toast.until - Instant::now());
        let tokens = theme::current(ctx);
        let mut close = false;
        egui::Area::new(Id::new("recomp-toast"))
            .order(Order::Foreground)
            .anchor(Align2::RIGHT_TOP, [-16.0, 62.0])
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(tokens.elevated)
                    .stroke(Stroke::new(1.0_f32, tokens.border_strong))
                    .corner_radius(CornerRadius::same(10))
                    .shadow(tokens.card_shadow)
                    .inner_margin(Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_max_width(380.0);
                        ui.horizontal(|ui| {
                            icons::icon(
                                ui,
                                if toast.error {
                                    icons::CIRCLE_ALERT
                                } else {
                                    icons::CIRCLE_CHECK
                                },
                                16.0,
                                if toast.error {
                                    tokens.red
                                } else {
                                    tokens.green
                                },
                            );
                            ui.allocate_ui_with_layout(
                                Vec2::new(300.0, 36.0),
                                Layout::top_down(Align::Min),
                                |ui| {
                                    ui.set_max_width(300.0);
                                    text::paragraph(
                                        ui,
                                        toast.message.clone(),
                                        10.0,
                                        tokens.text,
                                        15.0,
                                    );
                                },
                            );
                            if components::icon_button_sized(ui, icons::X, 12.0, 20.0).clicked() {
                                close = true;
                            }
                        });
                    });
            });
        if close {
            self.toast = None;
        }
    }

    fn resize_handles(&self, ctx: &egui::Context) {
        let screen = ctx.screen_rect();
        let thickness = 5.0;
        let corner = 10.0;
        egui::Area::new(Id::new("recomp-resize"))
            .order(Order::Foreground)
            .fixed_pos(screen.min)
            .default_size(Vec2::ZERO)
            // The area itself must stay zero-sized: every Area registers a
            // background widget spanning its own rect, and a whole-window
            // widget would passivate every other layer during hit-testing,
            // killing all clicks and hovers. The handles below only intersect
            // their own strips with this Ui's clip rect (the full screen), so
            // a zero-sized area is enough to host them.
            .interactable(false)
            .show(ctx, |ui| {
                let handles = [
                    (
                        Rect::from_min_size(
                            egui::pos2(screen.left(), screen.top()),
                            Vec2::new(screen.width(), thickness),
                        ),
                        egui::viewport::ResizeDirection::North,
                        egui::CursorIcon::ResizeVertical,
                    ),
                    (
                        Rect::from_min_size(
                            egui::pos2(screen.left(), screen.bottom() - thickness),
                            Vec2::new(screen.width(), thickness),
                        ),
                        egui::viewport::ResizeDirection::South,
                        egui::CursorIcon::ResizeVertical,
                    ),
                    (
                        Rect::from_min_size(
                            egui::pos2(screen.left(), screen.top()),
                            Vec2::new(thickness, screen.height()),
                        ),
                        egui::viewport::ResizeDirection::West,
                        egui::CursorIcon::ResizeHorizontal,
                    ),
                    (
                        Rect::from_min_size(
                            egui::pos2(screen.right() - thickness, screen.top()),
                            Vec2::new(thickness, screen.height()),
                        ),
                        egui::viewport::ResizeDirection::East,
                        egui::CursorIcon::ResizeHorizontal,
                    ),
                    (
                        Rect::from_min_size(screen.left_top(), Vec2::splat(corner)),
                        egui::viewport::ResizeDirection::NorthWest,
                        egui::CursorIcon::ResizeNwSe,
                    ),
                    (
                        Rect::from_min_size(
                            egui::pos2(screen.right() - corner, screen.top()),
                            Vec2::splat(corner),
                        ),
                        egui::viewport::ResizeDirection::NorthEast,
                        egui::CursorIcon::ResizeNeSw,
                    ),
                    (
                        Rect::from_min_size(
                            egui::pos2(screen.left(), screen.bottom() - corner),
                            Vec2::splat(corner),
                        ),
                        egui::viewport::ResizeDirection::SouthWest,
                        egui::CursorIcon::ResizeNeSw,
                    ),
                    (
                        Rect::from_min_size(
                            egui::pos2(screen.right() - corner, screen.bottom() - corner),
                            Vec2::splat(corner),
                        ),
                        egui::viewport::ResizeDirection::SouthEast,
                        egui::CursorIcon::ResizeNwSe,
                    ),
                ];
                for (index, (rect, direction, cursor)) in handles.into_iter().enumerate() {
                    let response =
                        ui.interact(rect, Id::new(("recomp-resize", index)), Sense::drag());
                    if response.hovered() {
                        ui.ctx().set_cursor_icon(cursor);
                    }
                    if response.drag_started() {
                        ui.ctx()
                            .send_viewport_cmd(ViewportCommand::BeginResize(direction));
                    }
                }
            });
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.pump_events(ctx);
        if self.applied_dark != Some(self.dark_theme) {
            theme::apply(ctx, self.dark_theme);
            self.applied_dark = Some(self.dark_theme);
        }

        self.topbar(ctx);
        self.sidebar(ctx);
        self.central(ctx);
        self.render_overlays(ctx);
        self.resize_handles(ctx);
        self.toast(ctx);
    }
}

fn window_button(ui: &mut egui::Ui, glyph: char, danger: bool) -> egui::Response {
    let tokens = theme::current(ui.ctx());
    let (rect, allocated) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
    let response = ui.interact(rect, allocated.id, Sense::click());
    let hovered = response.hovered();
    if hovered {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(6),
            if danger {
                tokens.red_soft
            } else {
                tokens.surface_2
            },
        );
    }
    icons::paint(
        ui.painter(),
        rect.center(),
        glyph,
        14.0,
        if hovered && danger {
            tokens.red
        } else if hovered {
            tokens.text
        } else {
            tokens.text_2
        },
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn sidebar_game(
    ui: &mut egui::Ui,
    game: &recomp_core::library::LibraryGame,
    active: bool,
    needs_file: bool,
) -> egui::Response {
    let tokens = theme::current(ui.ctx());
    let width = ui.available_width();
    let (rect, allocated) = ui.allocate_exact_size(Vec2::new(width, 34.0), Sense::click());
    let response = ui.interact(rect, allocated.id, Sense::click());
    let hovered = response.hovered();
    let fill = if active {
        tokens.accent_soft
    } else if hovered {
        tokens.surface_2
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius::same(7), fill);
    let thumb = Rect::from_min_size(
        egui::pos2(rect.left() + 5.0, rect.top() + 5.0),
        Vec2::splat(24.0),
    );
    crate::ui::art::paint(ui, thumb, crate::ui::art::palette_for(&game.slug), 6);
    let initial = crate::ui::art::initial_for(&game.slug, &game.title);
    crate::ui::art::paint_initial(
        ui,
        thumb,
        crate::ui::art::Initial {
            text: &initial,
            size: 12.0,
            opacity: 0.9,
            offset: Vec2::ZERO,
        },
    );
    let color = if active { tokens.text } else { tokens.text_2 };
    let title = crate::ui::text::ellipsize(
        ui.ctx(),
        &game.title,
        crate::ui::text::font(10.0, Weight::Medium),
        rect.width() - 52.0,
    );
    ui.painter().galley(
        egui::pos2(rect.left() + 36.0, rect.center().y - 7.0),
        ui.painter()
            .layout_no_wrap(title, crate::ui::text::font(10.0, Weight::Medium), color),
        color,
    );
    ui.painter().circle_filled(
        egui::pos2(rect.right() - 8.0, rect.center().y),
        2.5,
        if needs_file {
            tokens.orange
        } else {
            tokens.green
        },
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}
