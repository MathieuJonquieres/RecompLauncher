//! Main application shell: sidebar navigation, event pump and page routing.

use std::time::Duration;

use recomp_core::paths::AppPaths;
use recomp_core::AppContext;
use tokio::runtime::Runtime;

use crate::backend_bridge::{BackendBridge, Command, Event};
use crate::views::catalog::CatalogView;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Page {
    Catalog,
    Library,
    Friends,
    Settings,
}

impl Page {
    const ALL: [Page; 4] = [Page::Catalog, Page::Library, Page::Friends, Page::Settings];

    fn label(self) -> &'static str {
        match self {
            Page::Catalog => "Catalogue",
            Page::Library => "Bibliothèque",
            Page::Friends => "Amis",
            Page::Settings => "Paramètres",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Page::Catalog => {
                "Parcourez les jeux recompilés et décompilés, filtrez par plateforme, type, \
                 état et système, puis consultez leur fiche."
            }
            Page::Library => {
                "Vos jeux installés par le launcher, ajoutés manuellement ou déclarés comme \
                 jeux externes. Lancement, mise à jour, réparation et désinstallation."
            }
            Page::Friends => {
                "Serveurs communautaires, découverte LAN et ajout manuel host:port. Le carnet \
                 d'amis reste entièrement local."
            }
            Page::Settings => "Langue, thème, dossiers, proxy, clés API optionnelles et cache.",
        }
    }
}

pub struct LauncherApp {
    bridge: BackendBridge,
    paths: AppPaths,
    page: Page,
    catalog: CatalogView,
    dark_theme: bool,
    status: Option<String>,
}

impl LauncherApp {
    pub fn new(
        creation_context: &eframe::CreationContext<'_>,
        context: AppContext,
        runtime: Runtime,
    ) -> Self {
        creation_context
            .egui_ctx
            .set_visuals(eframe::egui::Visuals::dark());

        let paths = context.paths().clone();
        let bridge = BackendBridge::new(
            runtime,
            std::sync::Arc::new(context),
            creation_context.egui_ctx.clone(),
        );
        bridge.send(Command::Bootstrap);

        Self {
            bridge,
            paths,
            page: Page::Catalog,
            catalog: CatalogView::default(),
            dark_theme: true,
            status: None,
        }
    }

    fn pump_events(&mut self) {
        for event in self.bridge.drain() {
            match event {
                Event::Catalog(snapshot) => self.catalog.apply_snapshot(snapshot),
                Event::CatalogRefreshing => self.catalog.refreshing = true,
                Event::Error(message) => {
                    tracing::error!(%message, "backend error");
                    self.status = Some(message);
                }
            }
        }
    }

    fn sidebar(&mut self, ui: &mut eframe::egui::Ui) {
        ui.add_space(18.0);
        ui.heading("RecompLauncher");
        ui.label(
            eframe::egui::RichText::new("Launcher de jeux recompilés")
                .small()
                .weak(),
        );
        ui.add_space(20.0);

        for page in Page::ALL {
            let selected = self.page == page;
            let label = match page {
                Page::Catalog if !self.catalog.entries.is_empty() => {
                    format!("{} ({})", page.label(), self.catalog.entries.len())
                }
                _ => page.label().to_string(),
            };
            if ui
                .add_sized(
                    [ui.available_width(), 36.0],
                    eframe::egui::SelectableLabel::new(selected, label),
                )
                .clicked()
            {
                self.page = page;
            }
            ui.add_space(4.0);
        }

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);
        if self.catalog.offline && !self.catalog.entries.is_empty() {
            ui.label(
                eframe::egui::RichText::new("● Hors ligne (cache)")
                    .small()
                    .color(eframe::egui::Color32::from_rgb(255, 190, 90)),
            );
        } else if self.catalog.refreshing {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(eframe::egui::RichText::new("Actualisation…").small());
            });
        } else if !self.catalog.entries.is_empty() {
            ui.label(
                eframe::egui::RichText::new("● Catalogue à jour")
                    .small()
                    .color(eframe::egui::Color32::from_rgb(120, 200, 140)),
            );
        }

        ui.with_layout(
            eframe::egui::Layout::bottom_up(eframe::egui::Align::LEFT),
            |ui| {
                ui.add_space(10.0);
                ui.label(
                    eframe::egui::RichText::new(format!("v{}", env!("CARGO_PKG_VERSION")))
                        .small()
                        .weak(),
                );
                ui.label(
                    eframe::egui::RichText::new(self.paths.root().display().to_string())
                        .small()
                        .weak(),
                );
            },
        );
    }

    fn placeholder(&mut self, ui: &mut eframe::egui::Ui) {
        ui.add_space(10.0);
        ui.heading(self.page.label());
        ui.add_space(6.0);
        ui.label(self.page.description());
        ui.add_space(24.0);

        if self.page == Page::Settings {
            ui.separator();
            ui.add_space(12.0);
            if ui.checkbox(&mut self.dark_theme, "Thème sombre").changed() {
                ui.ctx().set_visuals(if self.dark_theme {
                    eframe::egui::Visuals::dark()
                } else {
                    eframe::egui::Visuals::light()
                });
            }
            ui.add_space(12.0);
            ui.label(format!(
                "Dossier de données : {}",
                self.paths.root().display()
            ));
            ui.label(format!(
                "Base de données : {}",
                self.paths.db_path().display()
            ));
        }

        ui.add_space(24.0);
        eframe::egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.label("Cette page sera remplie aux jalons M4 à M8.");
            ui.label("Aucun accès réseau ni disque n'est effectué depuis ce thread.");
        });
    }
}

impl eframe::App for LauncherApp {
    fn update(&mut self, ctx: &eframe::egui::Context, _frame: &mut eframe::Frame) {
        self.pump_events();

        eframe::egui::TopBottomPanel::top("titlebar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("RecompLauncher");
                ui.separator();
                ui.label(self.page.label());
                if let Some(status) = &self.status {
                    ui.separator();
                    ui.label(
                        eframe::egui::RichText::new(status)
                            .small()
                            .color(eframe::egui::Color32::LIGHT_RED),
                    );
                }
            });
        });

        eframe::egui::SidePanel::left("sidebar")
            .exact_width(230.0)
            .resizable(false)
            .show(ctx, |ui| self.sidebar(ui));

        if self.page == Page::Catalog {
            self.catalog.show_detail(ctx, self.bridge.sender());
        }

        eframe::egui::CentralPanel::default().show(ctx, |ui| match self.page {
            Page::Catalog => self.catalog.show_central(ui, self.bridge.sender()),
            _ => self.placeholder(ui),
        });

        if self.catalog.refreshing {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
    }
}
