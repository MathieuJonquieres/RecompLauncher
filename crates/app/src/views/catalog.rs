//! Catalog page: virtualised grid, filters, and game detail panel.
//!
//! Pure presentation: filtering happens on an in-memory slice, every action
//! that touches the network/disk is sent as a `Command`.

use std::collections::{BTreeSet, HashMap, HashSet};

use eframe::egui::{self, Align, Color32, FontId, Layout, RichText, Sense, Vec2};
use recomp_core::catalog::filter::{CatalogFilter, CatalogSort};
use recomp_core::catalog::model::{GameEntry, GameKind, GameStatus, Os};
use recomp_core::catalog::service::CatalogSnapshot;
use recomp_core::db::now_unix;
use recomp_core::library::{InstallSource, LibraryGame};
use tokio::sync::mpsc::UnboundedSender;

use crate::backend_bridge::Command;
use crate::views::{InstallProgress, ModsUiState};

const CARD_HEIGHT: f32 = 164.0;
const CARD_MIN_WIDTH: f32 = 260.0;

#[derive(Default)]
pub struct CatalogView {
    pub entries: Vec<GameEntry>,
    pub filter: CatalogFilter,
    pub selected: Option<recomp_core::catalog::Slug>,
    pub offline: bool,
    pub refreshing: bool,
    pub last_updated: Option<i64>,
    pub errors: Vec<String>,
    pub library: Vec<LibraryGame>,
    pub installs: HashMap<String, InstallProgress>,
    pub mods: HashMap<String, ModsUiState>,
    mods_requested: HashSet<String>,
    pending_uninstall: Option<String>,
}

impl CatalogView {
    pub fn apply_snapshot(&mut self, snapshot: CatalogSnapshot) {
        self.entries = snapshot.entries;
        self.offline = snapshot.offline;
        self.last_updated = snapshot.last_updated;
        self.errors = snapshot.errors;
        self.refreshing = false;
        if let Some(selected) = &self.selected {
            if !self.entries.iter().any(|entry| &entry.slug == selected) {
                self.selected = None;
            }
        }
    }

    pub fn apply_library(&mut self, games: Vec<LibraryGame>) {
        self.library = games;
    }

    pub fn install_started(&mut self, slug: &str) {
        self.installs
            .insert(slug.to_string(), InstallProgress::default());
    }

    pub fn install_progress(&mut self, slug: &str, downloaded: u64, total: Option<u64>) {
        self.installs
            .insert(slug.to_string(), InstallProgress { downloaded, total });
    }

    pub fn install_finished(&mut self, slug: &str) {
        self.installs.remove(slug);
    }

    pub fn apply_mods(&mut self, slug: &str, state: ModsUiState) {
        self.mods_requested.insert(slug.to_string());
        self.mods.insert(slug.to_string(), state);
    }

    pub fn mods_failed(&mut self, slug: &str, message: impl Into<String>) {
        let state = self
            .mods
            .entry(slug.to_string())
            .or_insert_with(ModsUiState::loading);
        state.loading = false;
        state.error = Some(message.into());
    }

    fn installed(&self, slug: &str) -> Option<&LibraryGame> {
        self.library.iter().find(|game| game.slug == slug)
    }

    pub fn show_central(&mut self, ui: &mut egui::Ui, commands: &UnboundedSender<Command>) {
        self.toolbar(ui, commands);
        ui.separator();
        if self.offline {
            egui::Frame::group(ui.style())
                .fill(Color32::from_rgb(70, 54, 20))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(
                            "Hors ligne — affichage du dernier catalogue en cache. \
                             Les installations et actualisations nécessitent Internet.",
                        )
                        .color(Color32::from_rgb(255, 214, 120)),
                    );
                });
            ui.add_space(6.0);
        }
        if !self.errors.is_empty() {
            ui.collapsing(
                format!("{} erreur(s) de fournisseur", self.errors.len()),
                |ui| {
                    for error in &self.errors {
                        ui.label(RichText::new(error).small().color(Color32::LIGHT_RED));
                    }
                },
            );
        }
        self.grid(ui, commands);
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, commands: &UnboundedSender<Command>) {
        let platforms = CatalogFilter::platforms(&self.entries);

        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.filter.search)
                    .hint_text("Rechercher un jeu, une plateforme…")
                    .desired_width(240.0),
            );

            let mut platform = self.filter.platform.clone();
            egui::ComboBox::from_id_salt("catalog-platform")
                .selected_text(platform.as_deref().unwrap_or("Toutes plateformes"))
                .width(170.0)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(platform.is_none(), "Toutes plateformes")
                        .clicked()
                    {
                        platform = None;
                    }
                    for value in &platforms {
                        if ui
                            .selectable_label(
                                platform.as_deref() == Some(value.as_str()),
                                value.as_str(),
                            )
                            .clicked()
                        {
                            platform = Some(value.clone());
                        }
                    }
                });
            self.filter.platform = platform;

            let mut kind = self.filter.kind;
            egui::ComboBox::from_id_salt("catalog-kind")
                .selected_text(kind.map_or("Tous types", GameKind::label))
                .width(130.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut kind, None, "Tous types");
                    ui.selectable_value(&mut kind, Some(GameKind::Recomp), "Recomp");
                    ui.selectable_value(&mut kind, Some(GameKind::Decomp), "Décomp");
                    ui.selectable_value(&mut kind, Some(GameKind::NativePort), "Port natif");
                });
            self.filter.kind = kind;

            let mut status = self.filter.status;
            egui::ComboBox::from_id_salt("catalog-status")
                .selected_text(status.map_or("Tous états", GameStatus::label))
                .width(120.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut status, None, "Tous états");
                    ui.selectable_value(&mut status, Some(GameStatus::Playable), "Jouable");
                    ui.selectable_value(&mut status, Some(GameStatus::Beta), "Bêta");
                    ui.selectable_value(&mut status, Some(GameStatus::Wip), "WIP");
                });
            self.filter.status = status;

            ui.checkbox(&mut self.filter.os_only, "Compatibles avec mon OS");

            egui::ComboBox::from_id_salt("catalog-sort")
                .selected_text(match self.filter.sort {
                    CatalogSort::Title => "Tri : titre",
                    CatalogSort::Platform => "Tri : plateforme",
                    CatalogSort::Status => "Tri : état",
                })
                .width(150.0)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.filter.sort, CatalogSort::Title, "Titre");
                    ui.selectable_value(&mut self.filter.sort, CatalogSort::Platform, "Plateforme");
                    ui.selectable_value(&mut self.filter.sort, CatalogSort::Status, "État");
                });

            ui.separator();
            if ui
                .add_enabled(!self.refreshing, egui::Button::new("Actualiser"))
                .clicked()
            {
                let _ = commands.send(Command::RefreshCatalog);
            }
            if self.refreshing {
                ui.spinner();
            }
            ui.label(
                RichText::new(format!("Dernière MAJ : {}", format_age(self.last_updated)))
                    .small()
                    .weak(),
            );
        });
    }

    fn grid(&mut self, ui: &mut egui::Ui, commands: &UnboundedSender<Command>) {
        let filtered = self.filter.apply(&self.entries);
        if filtered.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(if self.entries.is_empty() {
                        "Aucun jeu chargé. Lancez une actualisation du catalogue."
                    } else {
                        "Aucun jeu ne correspond à ces filtres."
                    })
                    .weak(),
                );
            });
            return;
        }

        let available = ui.available_width();
        let spacing = ui.spacing().item_spacing.x;
        let columns = ((available / (CARD_MIN_WIDTH + spacing)).floor() as usize).clamp(1, 6);
        let card_width = ((available - spacing * columns.saturating_sub(1) as f32)
            / columns as f32)
            .max(CARD_MIN_WIDTH);
        let rows = filtered.len().div_ceil(columns);

        let selected = &mut self.selected;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_rows(ui, CARD_HEIGHT, rows, |ui, row_range| {
                for row in row_range {
                    ui.horizontal(|ui| {
                        for column in 0..columns {
                            let index = row * columns + column;
                            if let Some(entry) = filtered.get(index) {
                                game_card(ui, entry, card_width, selected);
                            } else {
                                ui.allocate_space(Vec2::new(card_width, CARD_HEIGHT - 8.0));
                            }
                        }
                    });
                }
            });

        // The command sender is kept for M4 actions triggered from cards later.
        let _ = commands;
    }

    /// Right-hand detail panel. Returns true when the panel is open.
    pub fn show_detail(&mut self, ctx: &egui::Context, commands: &UnboundedSender<Command>) {
        let Some(slug) = self.selected.clone() else {
            return;
        };
        let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.slug == slug)
            .cloned()
        else {
            self.selected = None;
            return;
        };

        let installed = self.installed(entry.slug.as_str()).cloned();
        let progress = self.installs.get(entry.slug.as_str()).copied();
        let mods_state = self.mods.get(entry.slug.as_str()).cloned();
        let should_request_mods = entry.mods.supported
            && !self.mods_requested.contains(entry.slug.as_str())
            && mods_state.is_none();
        let compatible = entry.supports_os(Os::current());
        let mut request_uninstall = None;
        let mut open = true;
        egui::SidePanel::right("game-detail")
            .resizable(true)
            .default_width(400.0)
            .min_width(320.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.heading(&entry.title);
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("✕").on_hover_text("Fermer").clicked() {
                                open = false;
                            }
                        });
                    });
                    ui.label(RichText::new(entry.slug.as_str()).monospace().weak());
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        badge(ui, entry.kind.label(), Color32::from_rgb(48, 92, 140));
                        badge(
                            ui,
                            entry.status.label(),
                            match entry.status {
                                GameStatus::Playable => Color32::from_rgb(38, 120, 70),
                                GameStatus::Beta => Color32::from_rgb(150, 110, 30),
                                GameStatus::Wip => Color32::from_rgb(140, 55, 55),
                                _ => Color32::from_rgb(90, 90, 100),
                            },
                        );
                        badge(ui, &entry.original_platform, Color32::from_rgb(80, 70, 100));
                    });

                    ui.add_space(8.0);
                    if !entry.description.is_empty() {
                        ui.label(&entry.description);
                    }

                    ui.add_space(8.0);
                    if ui
                        .button("Ouvrir le dépôt officiel")
                        .on_hover_text(&entry.project.repo)
                        .clicked()
                    {
                        let _ = commands.send(Command::OpenUrl(entry.project.repo.clone()));
                    }
                    ui.label(
                        RichText::new(format!(
                            "{} — licence : {}",
                            entry.project.name,
                            entry.project.license.as_deref().unwrap_or("non précisée")
                        ))
                        .small()
                        .weak(),
                    );

                    ui.add_space(10.0);
                    ui.separator();
                    ui.strong("Compatibilité");
                    for os in [Os::Windows, Os::Linux, Os::Macos] {
                        let reason = entry.incompatible_reason(os);
                        match reason {
                            None => ui.label(format!("✓ {}", os.label())),
                            Some(reason) => ui.label(
                                RichText::new(format!("✗ {} — {reason}", os.label()))
                                    .small()
                                    .weak(),
                            ),
                        };
                    }

                    if !entry.releases.assets.is_empty() {
                        ui.add_space(10.0);
                        ui.strong("Binaires publiés");
                        for asset in &entry.releases.assets {
                            ui.label(
                                RichText::new(format!(
                                    "{} · {} · {}",
                                    asset.os.label(),
                                    asset.arch.map_or("toutes arch".to_string(), |arch| {
                                        format!("{arch:?}").to_lowercase()
                                    }),
                                    asset.pattern
                                ))
                                .small(),
                            );
                        }
                    }

                    if let Some(user_files) = &entry.user_files {
                        ui.add_space(10.0);
                        ui.strong("Fichier utilisateur");
                        if user_files.required {
                            ui.label(
                                RichText::new("Requis — vous devez fournir votre propre dump.")
                                    .color(Color32::from_rgb(255, 190, 90)),
                            );
                        } else {
                            ui.label(RichText::new("Optionnel").weak());
                        }
                        if !user_files.description.is_empty() {
                            ui.label(&user_files.description);
                        }
                        if !user_files.accepted_hashes.is_empty() {
                            ui.collapsing(
                                format!("{} hash(es) accepté(s)", user_files.accepted_hashes.len()),
                                |ui| {
                                    for hash in &user_files.accepted_hashes {
                                        ui.label(
                                            RichText::new(format!(
                                                "{} {} {}",
                                                hash.algo,
                                                hash.value.as_str(),
                                                hash.label.as_deref().unwrap_or("")
                                            ))
                                            .small()
                                            .monospace(),
                                        );
                                    }
                                },
                            );
                        }
                        if !user_files.prompt_ext.is_empty() {
                            ui.label(
                                RichText::new(format!(
                                    "Extensions : {}",
                                    user_files.prompt_ext.join(", ")
                                ))
                                .small()
                                .weak(),
                            );
                        }
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.strong("Installation");
                    if let Some(progress) = progress {
                        let fraction = progress.fraction();
                        match fraction {
                            Some(fraction) => {
                                ui.add(
                                    egui::ProgressBar::new(fraction)
                                        .show_percentage()
                                        .desired_width(ui.available_width()),
                                );
                            }
                            None => {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(format!(
                                        "{} téléchargés",
                                        format_bytes(progress.downloaded)
                                    ));
                                });
                            }
                        }
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new("Téléchargement et installation en cours…")
                                    .small()
                                    .weak(),
                            );
                            if ui.button("Annuler").clicked() {
                                let _ = commands.send(Command::CancelInstall {
                                    slug: entry.slug.as_str().to_string(),
                                });
                            }
                        });
                    } else if let Some(game) = &installed {
                        ui.label(
                            RichText::new(format!(
                                "{} · version {}",
                                game.source.label(),
                                game.version.as_deref().unwrap_or("inconnue")
                            ))
                            .color(Color32::from_rgb(120, 200, 140)),
                        );
                        if let Some(path) = &game.install_path {
                            ui.label(
                                RichText::new(path.display().to_string())
                                    .small()
                                    .weak()
                                    .monospace(),
                            );
                        }
                        let user_file = game.user_file.as_ref();
                        if entry.user_files.as_ref().is_some_and(|files| files.required) {
                            match user_file {
                                Some(path) => ui.label(
                                    RichText::new(format!(
                                        "Fichier utilisateur : {}",
                                        path.display()
                                    ))
                                    .small()
                                    .color(Color32::from_rgb(120, 200, 140)),
                                ),
                                None => ui.label(
                                    RichText::new(
                                        "Fichier utilisateur requis : sélectionnez votre dump.",
                                    )
                                    .small()
                                    .color(Color32::from_rgb(255, 190, 90)),
                                ),
                            };
                        }
                        ui.horizontal_wrapped(|ui| {
                            if entry.user_files.is_some()
                                && ui.button("Choisir mon fichier").clicked()
                            {
                                let _ = commands.send(Command::SelectUserFile {
                                    slug: entry.slug.as_str().to_string(),
                                });
                            }
                            if matches!(game.source, InstallSource::Managed)
                                && ui.button("Réparer").clicked()
                            {
                                let _ = commands.send(Command::Repair {
                                    slug: entry.slug.as_str().to_string(),
                                });
                            }
                            if matches!(game.source, InstallSource::Managed)
                                && ui.button("Réinstaller").clicked()
                            {
                                let _ = commands.send(Command::Install {
                                    slug: entry.slug.as_str().to_string(),
                                });
                            }
                            if ui.button("Désinstaller").clicked() {
                                request_uninstall = Some(entry.slug.as_str().to_string());
                            }
                        });
                    } else if compatible {
                        if ui
                            .add(
                                egui::Button::new("Installer")
                                    .min_size(Vec2::new(150.0, 30.0)),
                            )
                            .clicked()
                        {
                            let _ = commands.send(Command::Install {
                                slug: entry.slug.as_str().to_string(),
                            });
                        }
                        ui.label(
                            RichText::new(
                                "Le binaire officiel sera téléchargé depuis GitHub Releases, \
                                 vérifié puis installé dans le dossier de données du launcher.",
                            )
                            .small()
                            .weak(),
                        );
                    } else {
                        ui.label(
                            RichText::new(
                                entry
                                    .incompatible_reason(Os::current())
                                    .unwrap_or("Aucun binaire compatible avec ce système."),
                            )
                            .color(Color32::from_rgb(220, 130, 130)),
                        );
                    }

                    ui.add_space(10.0);
                    ui.strong("Mods");
                    if !entry.mods.supported || entry.mods.strategy == recomp_core::catalog::model::ModStrategy::None {
                        ui.label(RichText::new("Mods non supportés par ce projet.").weak());
                    } else {
                        ui.label(format!(
                            "Dossier « {} » · format {} · stratégie {}",
                            entry.mods.dir_name,
                            entry.mods.format_hint.as_deref().unwrap_or("libre"),
                            entry.mods.strategy.label()
                        ));
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Créer le dossier mods").clicked() {
                                let _ = commands.send(Command::CreateModsDir {
                                    slug: entry.slug.as_str().to_string(),
                                });
                            }
                            if ui.button("Ouvrir").clicked() {
                                let _ = commands.send(Command::OpenModsDir {
                                    slug: entry.slug.as_str().to_string(),
                                });
                            }
                            if ui.button("Actualiser").clicked() {
                                let _ = commands.send(Command::RefreshMods {
                                    slug: entry.slug.as_str().to_string(),
                                });
                            }
                        });

                        if let Some(state) = &mods_state {
                            if state.loading {
                                ui.horizontal(|ui| {
                                    ui.spinner();
                                    ui.label(RichText::new("Scan des mods…").small().weak());
                                });
                            }
                            if let Some(error) = &state.error {
                                ui.label(
                                    RichText::new(error)
                                        .small()
                                        .color(Color32::from_rgb(220, 130, 130)),
                                );
                            }
                            if state.mods.is_empty() {
                                ui.label(
                                    RichText::new(
                                        "Aucun mod détecté. Déposez un fichier mod dans la zone ci-dessous.",
                                    )
                                    .small()
                                    .weak(),
                                );
                            }
                            for (index, record) in state.mods.iter().enumerate() {
                                ui.horizontal(|ui| {
                                    let mut enabled = record.enabled;
                                    if ui.checkbox(&mut enabled, &record.name).changed() {
                                        let _ = commands.send(Command::ToggleMod {
                                            slug: entry.slug.as_str().to_string(),
                                            name: record.name.clone(),
                                            enabled,
                                        });
                                    }
                                    if !record.enabled {
                                        ui.label(
                                            RichText::new("désactivé").small().weak(),
                                        );
                                    }
                                    ui.with_layout(
                                        Layout::right_to_left(Align::Center),
                                        |ui| {
                                            if index + 1 < state.mods.len()
                                                && ui.button("↓").clicked()
                                            {
                                                let mut names: Vec<String> = state
                                                    .mods
                                                    .iter()
                                                    .map(|mod_record| mod_record.name.clone())
                                                    .collect();
                                                names.swap(index, index + 1);
                                                let _ = commands.send(Command::ReorderMods {
                                                    slug: entry.slug.as_str().to_string(),
                                                    names,
                                                });
                                            }
                                            if index > 0 && ui.button("↑").clicked() {
                                                let mut names: Vec<String> = state
                                                    .mods
                                                    .iter()
                                                    .map(|mod_record| mod_record.name.clone())
                                                    .collect();
                                                names.swap(index, index - 1);
                                                let _ = commands.send(Command::ReorderMods {
                                                    slug: entry.slug.as_str().to_string(),
                                                    names,
                                                });
                                            }
                                        },
                                    );
                                });
                            }
                        } else if should_request_mods {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label(RichText::new("Chargement des mods…").small().weak());
                            });
                        }

                        let drop_zone = egui::Frame::group(ui.style())
                            .fill(ui.visuals().faint_bg_color)
                            .inner_margin(8.0)
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(
                                        "Glissez-déposez un fichier ou un dossier de mod ici.",
                                    )
                                    .small()
                                    .weak(),
                                );
                            })
                            .response;
                        let dropped: Vec<std::path::PathBuf> = ui.input(|input| {
                            input
                                .raw
                                .dropped_files
                                .iter()
                                .filter_map(|file| file.path.clone())
                                .collect()
                        });
                        if !dropped.is_empty()
                            && (drop_zone.hovered()
                                || ui.rect_contains_pointer(ui.max_rect()))
                        {
                            let _ = commands.send(Command::ImportMods {
                                slug: entry.slug.as_str().to_string(),
                                paths: dropped,
                            });
                        }
                        ui.label(
                            RichText::new(
                                "Les mods tiers sont fournis par des utilisateurs : vérifiez leur \
                                 provenance. Le launcher ne les exécute jamais lui-même.",
                            )
                            .small()
                            .color(Color32::from_rgb(210, 170, 100)),
                        );
                    }

                    ui.add_space(10.0);
                    ui.strong("En ligne");
                    if entry.online.supported {
                        let modes: Vec<&str> =
                            entry.online.modes.iter().map(|mode| mode.label()).collect();
                        ui.label(modes.join(" · "));
                    } else {
                        ui.label(RichText::new("Aucune option en ligne déclarée.").weak());
                    }

                    ui.add_space(14.0);
                    ui.separator();
                    ui.label(
                        RichText::new(
                            "Aucun contenu de jeu n'est fourni par le launcher. \
                             Les binaires proviennent uniquement des releases officielles du projet.",
                        )
                        .small()
                        .weak(),
                    );
                });
            });

        if let Some(slug) = request_uninstall {
            self.pending_uninstall = Some(slug);
        }
        if should_request_mods {
            let slug = entry.slug.as_str().to_string();
            self.mods_requested.insert(slug.clone());
            let _ = commands.send(Command::RefreshMods { slug });
        }
        self.uninstall_dialog(ctx, commands);
        if !open {
            self.selected = None;
        }
    }

    fn uninstall_dialog(&mut self, ctx: &egui::Context, commands: &UnboundedSender<Command>) {
        let Some(slug) = self.pending_uninstall.clone() else {
            return;
        };
        let title = self
            .library
            .iter()
            .find(|game| game.slug == slug)
            .map(|game| game.title.clone())
            .unwrap_or_else(|| slug.clone());
        let mut open = true;
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new("Confirmer la désinstallation")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(format!("Supprimer « {title} » de la bibliothèque ?"));
                ui.label(
                    RichText::new(
                        "Les fichiers installés par le launcher seront supprimés. \
                         Les dossiers userdata/ et mods/ seront conservés, ainsi que vos dumps personnels.",
                    )
                    .small()
                    .weak(),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Supprimer").clicked() {
                        confirm = true;
                    }
                    if ui.button("Annuler").clicked() {
                        cancel = true;
                    }
                });
            });
        if confirm {
            let _ = commands.send(Command::Uninstall { slug });
            self.pending_uninstall = None;
        } else if cancel || !open {
            self.pending_uninstall = None;
        }
    }
}

fn game_card(
    ui: &mut egui::Ui,
    entry: &GameEntry,
    width: f32,
    selected: &mut Option<recomp_core::catalog::Slug>,
) {
    let compatible = entry.supports_os(Os::current());
    let fill = if selected.as_ref() == Some(&entry.slug) {
        Color32::from_rgb(45, 70, 100)
    } else if compatible {
        ui.visuals().faint_bg_color
    } else {
        Color32::from_rgb(32, 32, 36)
    };

    let inner = egui::Frame::group(ui.style())
        .fill(fill)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.set_width(width - 20.0);
            ui.set_height(CARD_HEIGHT - 28.0);
            ui.horizontal(|ui| {
                thumbnail(ui, entry);
                ui.vertical(|ui| {
                    ui.add(
                        egui::Label::new(RichText::new(&entry.title).strong())
                            .truncate()
                            .sense(Sense::hover()),
                    );
                    ui.label(
                        RichText::new(format!(
                            "{} · {}",
                            entry.original_platform,
                            entry.kind.label()
                        ))
                        .small()
                        .weak(),
                    );
                    ui.label(RichText::new(entry.status.label()).small().color(
                        match entry.status {
                            GameStatus::Playable => Color32::from_rgb(120, 220, 150),
                            GameStatus::Beta => Color32::from_rgb(230, 190, 110),
                            GameStatus::Wip => Color32::from_rgb(230, 130, 130),
                            _ => Color32::GRAY,
                        },
                    ));
                });
            });
            ui.add_space(6.0);
            ui.label(
                RichText::new(supported_oses(entry))
                    .small()
                    .color(if compatible {
                        Color32::from_rgb(140, 190, 230)
                    } else {
                        Color32::from_rgb(180, 110, 110)
                    }),
            );
            if !compatible {
                ui.label(
                    RichText::new(
                        entry
                            .incompatible_reason(Os::current())
                            .unwrap_or("Non compatible avec ce système."),
                    )
                    .small()
                    .color(Color32::from_rgb(190, 130, 130)),
                );
            }
        });

    let response = ui.interact(
        inner.response.rect,
        ui.make_persistent_id(("catalog-card", entry.slug.as_str())),
        Sense::click(),
    );
    if response.clicked() {
        *selected = Some(entry.slug.clone());
    }
    if !compatible {
        response.clone().on_hover_text(
            entry
                .incompatible_reason(Os::current())
                .unwrap_or("Non compatible avec ce système."),
        );
    }
    if compatible {
        response.on_hover_cursor(egui::CursorIcon::PointingHand);
    }
}

fn thumbnail(ui: &mut egui::Ui, entry: &GameEntry) {
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(56.0), Sense::hover());
    let hue = entry
        .slug
        .as_str()
        .bytes()
        .fold(0_u32, |acc, byte| acc.wrapping_add(byte as u32));
    let color = Color32::from_rgb(
        60 + (hue % 100) as u8,
        60 + ((hue / 7) % 100) as u8,
        70 + ((hue / 13) % 100) as u8,
    );
    ui.painter().rect_filled(rect, 7.0, color);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        initials(&entry.title),
        FontId::proportional(20.0),
        Color32::WHITE,
    );
}

fn initials(title: &str) -> String {
    title
        .split_whitespace()
        .filter(|word| !word.is_empty())
        .take(2)
        .filter_map(|word| word.chars().next())
        .collect::<String>()
        .to_uppercase()
}

fn supported_oses(entry: &GameEntry) -> String {
    let mut oses: BTreeSet<&str> = BTreeSet::new();
    for asset in &entry.releases.assets {
        oses.insert(asset.os.label());
    }
    if oses.is_empty() {
        "Aucun binaire publié".to_string()
    } else {
        oses.into_iter().collect::<Vec<_>>().join(" · ")
    }
}

fn badge(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).small().color(Color32::WHITE));
        });
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1024.0;
    const MIB: f64 = KIB * 1024.0;
    const GIB: f64 = MIB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} Gio", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} Mio", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} Kio", bytes / KIB)
    } else {
        format!("{bytes:.0} o")
    }
}

fn format_age(timestamp: Option<i64>) -> String {
    let Some(timestamp) = timestamp else {
        return "jamais".to_string();
    };
    let seconds = now_unix().saturating_sub(timestamp).max(0);
    if seconds < 60 {
        "à l'instant".to_string()
    } else if seconds < 3600 {
        format!("il y a {} min", seconds / 60)
    } else if seconds < 86_400 {
        format!("il y a {} h", seconds / 3600)
    } else {
        format!("il y a {} j", seconds / 86_400)
    }
}
