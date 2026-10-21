//! Full game page: overview, real mods, online servers and configuration.

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Layout, Margin, Pos2, Rect, ScrollArea, Sense,
    Stroke, Ui, Vec2,
};
use recomp_core::catalog::model::{GameStatus, Os};
use tokio::sync::mpsc::UnboundedSender;

use crate::backend_bridge::Command;
use crate::ui::components::{self, BadgeKind, Button};
use crate::ui::text::Weight;
use crate::ui::{art, format, icons, text, theme};

use super::{Action, Store};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailTab {
    Overview,
    Mods,
    Online,
    Configuration,
}

pub struct DetailUi {
    pub tab: DetailTab,
    pub server_name: String,
    pub server_address: String,
    pub server_port: String,
}

impl Default for DetailUi {
    fn default() -> Self {
        Self {
            tab: DetailTab::Overview,
            server_name: String::new(),
            server_address: String::new(),
            server_port: "7777".to_owned(),
        }
    }
}

pub fn show(
    ui: &mut Ui,
    store: &mut Store,
    state: &mut DetailUi,
    slug: &str,
    from_library: bool,
    sender: &UnboundedSender<Command>,
) -> Action {
    let tokens = theme::current(ui.ctx());
    let Some(entry) = store.entry(slug).cloned() else {
        let mut back = false;
        ui.vertical_centered(|ui| {
            ui.add_space(60.0);
            text::label(
                ui,
                "Jeu introuvable dans le catalogue.",
                15.0,
                Weight::SemiBold,
                tokens.text,
            );
            ui.add_space(10.0);
            back = Button::new("Retour")
                .icon(icons::ARROW_LEFT)
                .show(ui)
                .clicked();
        });
        return if back {
            Action::BackToLibrary
        } else {
            Action::None
        };
    };
    let mut action = Action::None;
    let installed = store.is_installed(slug);
    let progress = store.playing(slug);
    let unavailable = !entry.supports_os(Os::current());
    let needs_file = store.needs_user_file(slug);

    ui.horizontal(|ui| {
        if Button::ghost("Retour")
            .icon(icons::ARROW_LEFT)
            .show(ui)
            .clicked()
        {
            action = Action::BackToLibrary;
        }
        ui.add_space(2.0);
        text::label(
            ui,
            if from_library {
                "Bibliothèque"
            } else {
                "Catalogue"
            },
            9.0,
            Weight::Regular,
            tokens.text_3,
        );
        icons::icon(ui, icons::CHEVRON_RIGHT, 10.0, tokens.text_3);
        let crumb_width = (ui.available_width() - 8.0).max(120.0);
        let crumb = text::ellipsize(
            ui.ctx(),
            &entry.title,
            text::font(9.0, Weight::Medium),
            crumb_width,
        );
        text::label(ui, crumb, 9.0, Weight::Medium, tokens.text_2);
    });
    ui.add_space(14.0);
    let hero_width = ui.available_width();
    let hero_height = 238.0;
    let (hero, _) = ui.allocate_exact_size(Vec2::new(hero_width, hero_height), Sense::hover());
    art::paint(ui, hero, art::palette_for(slug), 14);
    let initial = art::initial_for(slug, &entry.title);
    art::paint_initial(
        ui,
        hero,
        art::Initial {
            text: &initial,
            size: 150.0,
            opacity: 0.12,
            offset: Vec2::new(hero_width * 0.23, -6.0),
        },
    );
    art::left_veil(ui.painter(), hero, 0.86, 0.0);
    art::bottom_veil(ui.painter(), hero, 0.0, 0.58);

    let content_left = hero.left() + 24.0;
    let mut badge_x = content_left;
    for (label, kind) in [
        (entry.original_platform.clone(), BadgeKind::Neutral),
        (
            entry.kind.label().to_owned(),
            match entry.kind {
                recomp_core::catalog::model::GameKind::Recomp => BadgeKind::Recomp,
                recomp_core::catalog::model::GameKind::Decomp => BadgeKind::Decomp,
                recomp_core::catalog::model::GameKind::NativePort => BadgeKind::Native,
                _ => BadgeKind::Accent,
            },
        ),
        (
            entry.status.label().to_owned(),
            match entry.status {
                GameStatus::Playable => BadgeKind::Playable,
                GameStatus::Beta => BadgeKind::Beta,
                GameStatus::Wip => BadgeKind::Wip,
                _ => BadgeKind::Neutral,
            },
        ),
    ] {
        let width = badge_width(ui, &label);
        paint_badge(ui, egui::pos2(badge_x, hero.top() + 22.0), &label, kind);
        badge_x += width + 6.0;
    }

    let hero_title = text::ellipsize(
        ui.ctx(),
        &entry.title,
        text::font(28.0, Weight::Bold),
        (hero_width - 380.0).max(200.0),
    );
    paint_text(
        ui,
        egui::pos2(content_left, hero.bottom() - 78.0),
        &hero_title,
        text::font(28.0, Weight::Bold),
        Color32::WHITE,
    );
    let meta = text::ellipsize(
        ui.ctx(),
        &format!(
            "{} · {} · {}",
            entry.original_platform,
            entry.kind.label(),
            entry.project.name
        ),
        text::font(10.0, Weight::Regular),
        (hero_width - 48.0).max(160.0),
    );
    paint_text(
        ui,
        egui::pos2(content_left, hero.bottom() - 45.0),
        &meta,
        text::font(10.0, Weight::Regular),
        Color32::from_rgb(0xDC, 0xE0, 0xE8),
    );
    ui.scope_builder(
        egui::UiBuilder::new()
            .max_rect(hero.shrink(20.0))
            .layout(Layout::right_to_left(Align::TOP)),
        |ui| {
            if needs_file {
                if Button::primary("Choisir le fichier")
                    .icon(icons::FILE_QUESTION)
                    .show(ui)
                    .clicked()
                {
                    let _ = sender.send(Command::SelectUserFile {
                        slug: slug.to_owned(),
                    });
                }
            } else if let Some(progress) = progress {
                let label = if progress.repairing {
                    "Réparation…"
                } else {
                    "Installation…"
                };
                if Button::new(label)
                    .icon(icons::LOADER_CIRCLE)
                    .show(ui)
                    .clicked()
                {
                    action = Action::Install(slug.to_owned());
                }
            } else if unavailable {
                Button::new("Indisponible")
                    .icon(icons::LOCK_KEYHOLE)
                    .enabled(false)
                    .show(ui);
            } else if installed {
                if Button::primary("Jouer")
                    .icon(icons::PLAY)
                    .width(110.0)
                    .show(ui)
                    .clicked()
                {
                    let _ = sender.send(Command::Play {
                        slug: slug.to_owned(),
                    });
                }
            } else if Button::primary("Installer")
                .icon(icons::DOWNLOAD)
                .width(118.0)
                .show(ui)
                .clicked()
            {
                action = Action::Install(slug.to_owned());
            }
            if installed
                && Button::new("Désinstaller")
                    .icon(icons::TRASH_2)
                    .show(ui)
                    .clicked()
            {
                action = Action::Uninstall(slug.to_owned());
            }
        },
    );
    // `scope_builder` places its content at the hero's top, but its usual
    // cursor advance would then move the parent cursor *backwards* to the
    // overlay's bottom. Restore the cursor to the end of the hero instead.
    ui.advance_cursor_after_rect(hero);

    ui.add_space(6.0);
    tab_bar(ui, &mut state.tab, entry.online.supported);
    ui.add_space(18.0);

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            match state.tab {
                DetailTab::Overview => overview(ui, store, &entry, sender, &mut action, installed),
                DetailTab::Mods => mods_tab(ui, store, slug, sender),
                DetailTab::Online => online_tab(ui, store, state, slug, sender),
                DetailTab::Configuration => {
                    configuration(ui, store, &entry, sender, &mut action, installed)
                }
            }
        });

    action
}

fn overview(
    ui: &mut Ui,
    store: &Store,
    entry: &recomp_core::catalog::model::GameEntry,
    sender: &UnboundedSender<Command>,
    action: &mut Action,
    installed: bool,
) {
    let metadata = store.metadata(entry.slug.as_str()).cloned();
    let description = metadata
        .as_ref()
        .and_then(|state| state.metadata.as_ref())
        .and_then(|metadata| {
            metadata
                .description
                .clone()
                .or_else(|| metadata.summary.clone())
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| entry.description.clone());
    let stack = ui.available_width() < 880.0;
    let width = ui.available_width();

    if stack {
        about_section(ui, &description, metadata.as_ref());
        ui.add_space(16.0);
        details_section(ui, entry);
        if entry.user_files.is_some() {
            ui.add_space(16.0);
            user_file_section(ui, store, entry, sender);
        }
        ui.add_space(16.0);
        install_section(ui, store, entry, sender, action, installed);
        ui.add_space(16.0);
        links_section(ui, store, entry, sender);
    } else {
        let gap = 20.0;
        let left_width = (width * 0.64).floor();
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            ui.allocate_ui_with_layout(
                Vec2::new(left_width, ui.available_height()),
                Layout::top_down(Align::Min),
                |ui| {
                    about_section(ui, &description, metadata.as_ref());
                    ui.add_space(16.0);
                    details_section(ui, entry);
                    if entry.user_files.is_some() {
                        ui.add_space(16.0);
                        user_file_section(ui, store, entry, sender);
                    }
                },
            );
            ui.allocate_ui_with_layout(
                Vec2::new(width - left_width - gap, ui.available_height()),
                Layout::top_down(Align::Min),
                |ui| {
                    install_section(ui, store, entry, sender, action, installed);
                    ui.add_space(16.0);
                    links_section(ui, store, entry, sender);
                },
            );
        });
    }
}

fn about_section(ui: &mut Ui, description: &str, metadata: Option<&super::MetadataUiState>) {
    section(ui, "À propos", |ui| {
        if let Some(state) = metadata {
            if state.loading {
                components::spinner(ui, 16.0, theme::current(ui.ctx()).text_3);
            }
            if let Some(error) = &state.error {
                ui.add_space(4.0);
                text::label(
                    ui,
                    error,
                    8.0,
                    Weight::Regular,
                    theme::current(ui.ctx()).orange,
                );
            }
        }
        let body = if description.is_empty() {
            "Aucune description n'est publiée pour ce projet."
        } else {
            description
        };
        text::paragraph(ui, body, 11.0, theme::current(ui.ctx()).text_2, 19.0);
        if let Some(metadata) = metadata.and_then(|state| state.metadata.as_ref()) {
            ui.add_space(10.0);
            text::label(
                ui,
                format!(
                    "Source : {} · {}",
                    metadata.attribution,
                    format::date(metadata.fetched_at)
                ),
                8.0,
                Weight::Regular,
                theme::current(ui.ctx()).text_3,
            );
        }
    });
}

fn details_section(ui: &mut Ui, entry: &recomp_core::catalog::model::GameEntry) {
    let tokens = theme::current(ui.ctx());
    section(ui, "Détails du jeu", |ui| {
        let rows = [
            ("Plateforme d'origine", entry.original_platform.clone()),
            ("Type de projet", entry.kind.label().to_owned()),
            ("Statut", entry.status.label().to_owned()),
            ("Projet", entry.project.name.clone()),
            (
                "Licence",
                entry
                    .project
                    .license
                    .clone()
                    .unwrap_or_else(|| "Non précisée".to_owned()),
            ),
            (
                "Modes en ligne",
                if entry.online.supported {
                    format!("{} mode(s)", entry.online.modes.len())
                } else {
                    "Non pris en charge".to_owned()
                },
            ),
        ];
        for row in rows.chunks(2) {
            ui.horizontal(|ui| {
                let width = (ui.available_width() - 16.0) / 2.0;
                for (label, value) in row {
                    ui.allocate_ui_with_layout(
                        Vec2::new(width, 36.0),
                        Layout::top_down(Align::Min),
                        |ui| {
                            text::label(ui, *label, 9.0, Weight::Regular, tokens.text_3);
                            ui.add_space(3.0);
                            text::label(ui, value, 10.0, Weight::Medium, tokens.text);
                        },
                    );
                }
            });
            ui.add_space(8.0);
        }
    });
}

fn user_file_section(
    ui: &mut Ui,
    store: &Store,
    entry: &recomp_core::catalog::model::GameEntry,
    sender: &UnboundedSender<Command>,
) {
    let tokens = theme::current(ui.ctx());
    let user_files = entry.user_files.as_ref().expect("checked by caller");
    section(ui, "Fichier utilisateur", |ui| {
        text::paragraph(
            ui,
            user_files.description.clone(),
            10.0,
            tokens.text_2,
            17.0,
        );
        ui.add_space(10.0);
        let current = store
            .library_game(entry.slug.as_str())
            .and_then(|game| game.user_file.as_ref());
        if let Some(path) = current {
            ui.label(components::mono(
                format::short_path(path, 68),
                9.0,
                tokens.green,
            ));
        } else {
            text::label(
                ui,
                "Aucun fichier sélectionné.",
                9.0,
                Weight::Regular,
                tokens.orange,
            );
        }
        ui.add_space(10.0);
        if Button::new("Sélectionner un fichier")
            .icon(icons::FILE_SEARCH)
            .show(ui)
            .clicked()
        {
            let _ = sender.send(Command::SelectUserFile {
                slug: entry.slug.to_string(),
            });
        }
    });
}

fn install_section(
    ui: &mut Ui,
    store: &Store,
    entry: &recomp_core::catalog::model::GameEntry,
    sender: &UnboundedSender<Command>,
    action: &mut Action,
    installed: bool,
) {
    let tokens = theme::current(ui.ctx());
    let slug = entry.slug.as_str();
    let game = store.library_game(slug);
    let progress = store.playing(slug);
    let size = store.stats.per_game.get(slug).copied();

    section(ui, "Installation", |ui| {
        ui.horizontal(|ui| {
            if progress.is_some() {
                components::spinner(ui, 16.0, tokens.accent);
                text::label(
                    ui,
                    "Installation en cours",
                    11.0,
                    Weight::SemiBold,
                    tokens.accent,
                );
            } else if store.needs_user_file(slug) {
                icons::icon(ui, icons::FILE_QUESTION, 16.0, tokens.orange);
                text::label(
                    ui,
                    "Fichier utilisateur requis",
                    11.0,
                    Weight::SemiBold,
                    tokens.orange,
                );
            } else if installed {
                icons::icon(ui, icons::CIRCLE_CHECK, 16.0, tokens.green);
                text::label(ui, "Installé et prêt", 11.0, Weight::SemiBold, tokens.green);
            } else {
                icons::icon(ui, icons::CIRCLE_ALERT, 16.0, tokens.orange);
                text::label(ui, "Non installé", 11.0, Weight::SemiBold, tokens.orange);
            }
        });
        ui.add_space(12.0);
        if let Some(game) = game {
            pair(ui, "Version", game.version.as_deref().unwrap_or("—"));
            if let Some(size) = size {
                pair(ui, "Taille", &format::bytes(size));
            }
            pair(ui, "Installé le", &format::date(game.installed_at));
            pair(
                ui,
                "Dernière session",
                &game
                    .last_played
                    .map(|timestamp| format::relative_day(timestamp, format::now()))
                    .unwrap_or_else(|| "Jamais".to_owned()),
            );
            if let Some(path) = &game.install_path {
                ui.add_space(2.0);
                text::label(ui, "Dossier", 9.0, Weight::Regular, tokens.text_3);
                ui.add_space(3.0);
                ui.label(components::mono(
                    format::short_path(path, 44),
                    9.0,
                    tokens.text_2,
                ));
            }
            ui.add_space(14.0);
            if progress.is_some() {
                components::progress_bar(
                    ui,
                    progress
                        .and_then(|progress| progress.fraction())
                        .unwrap_or(0.0),
                    6.0,
                );
            } else {
                if store.needs_user_file(slug) {
                    if Button::primary("Choisir le fichier utilisateur")
                        .icon(icons::FILE_QUESTION)
                        .width(ui.available_width())
                        .show(ui)
                        .clicked()
                    {
                        let _ = sender.send(Command::SelectUserFile {
                            slug: slug.to_owned(),
                        });
                    }
                    ui.add_space(6.0);
                }
                if Button::new("Vérifier les fichiers")
                    .icon(icons::SHIELD_CHECK)
                    .width(ui.available_width())
                    .show(ui)
                    .clicked()
                {
                    let _ = sender.send(Command::Repair {
                        slug: slug.to_owned(),
                    });
                }
                ui.add_space(6.0);
                if Button::ghost("Désinstaller")
                    .icon(icons::TRASH_2)
                    .width(ui.available_width())
                    .show(ui)
                    .clicked()
                {
                    *action = Action::Uninstall(slug.to_owned());
                }
            }
        } else if progress.is_some() {
            components::progress_bar(
                ui,
                progress
                    .and_then(|progress| progress.fraction())
                    .unwrap_or(0.0),
                6.0,
            );
        } else {
            text::paragraph(
                ui,
                "Ce jeu n'est pas encore installé. Utilisez le bouton Installer pour lancer le téléchargement.",
                10.0,
                tokens.text_2,
                17.0,
            );
            ui.add_space(12.0);
            if !entry.supports_os(Os::current()) {
                Button::new("Indisponible")
                    .icon(icons::LOCK_KEYHOLE)
                    .width(ui.available_width())
                    .enabled(false)
                    .show(ui);
            } else if Button::primary("Installer")
                .icon(icons::DOWNLOAD)
                .width(ui.available_width())
                .show(ui)
                .clicked()
            {
                *action = Action::Install(slug.to_owned());
            }
        }
    });
}

fn links_section(
    ui: &mut Ui,
    store: &Store,
    entry: &recomp_core::catalog::model::GameEntry,
    sender: &UnboundedSender<Command>,
) {
    section(ui, "Liens", |ui| {
        if Button::new("Dépôt GitHub")
            .icon(icons::EXTERNAL_LINK)
            .width(ui.available_width())
            .show(ui)
            .clicked()
        {
            let _ = sender.send(Command::OpenUrl(entry.project.repo.clone()));
        }
        let wikipedia = store
            .metadata(entry.slug.as_str())
            .and_then(|state| state.metadata.as_ref())
            .and_then(|metadata| metadata.source_url.clone());
        if let Some(url) = wikipedia {
            ui.add_space(6.0);
            if Button::new("Page Wikipédia")
                .icon(icons::GLOBE_2)
                .width(ui.available_width())
                .show(ui)
                .clicked()
            {
                let _ = sender.send(Command::OpenUrl(url));
            }
        }
        if let Some(docs) = &entry.mods.docs_url {
            if entry.mods.supported {
                ui.add_space(6.0);
                if Button::new("Documentation des mods")
                    .icon(icons::BOOK_OPEN)
                    .width(ui.available_width())
                    .show(ui)
                    .clicked()
                {
                    let _ = sender.send(Command::OpenUrl(docs.clone()));
                }
            }
        }
    });
}

fn mods_tab(ui: &mut Ui, store: &Store, slug: &str, sender: &UnboundedSender<Command>) {
    let tokens = theme::current(ui.ctx());
    let Some(entry) = store.entry(slug) else {
        return;
    };
    if !entry.mods.supported {
        empty_block(
            ui,
            icons::PACKAGE_OPEN,
            "Mods non pris en charge",
            "Ce projet ne déclare pas de prise en charge des mods.",
        );
        return;
    }
    let state = store.mods(slug);
    let count = state.map(|state| state.mods.len()).unwrap_or(0);
    section_with_actions(
        ui,
        "Mods",
        &format!("{count} mod(s) détecté(s)"),
        |ui| {
            if Button::primary("Importer")
                .icon(icons::UPLOAD)
                .show(ui)
                .clicked()
            {
                let _ = sender.send(Command::PickMods {
                    slug: slug.to_owned(),
                });
            }
            if Button::new("Ouvrir le dossier")
                .icon(icons::FOLDER_OPEN)
                .show(ui)
                .clicked()
            {
                let _ = sender.send(Command::OpenModsDir {
                    slug: slug.to_owned(),
                });
            }
            if components::icon_button(ui, icons::REFRESH_CW, false).clicked() {
                let _ = sender.send(Command::RefreshMods {
                    slug: slug.to_owned(),
                });
            }
        },
        |ui| match state {
            None => {
                components::spinner(ui, 18.0, tokens.text_3);
                ui.add_space(8.0);
                text::label(
                    ui,
                    "Analyse du dossier de mods…",
                    10.0,
                    Weight::Regular,
                    tokens.text_2,
                );
            }
            Some(state) if state.loading => {
                components::spinner(ui, 18.0, tokens.text_3);
            }
            Some(state) if state.error.is_some() => {
                let message = state.error.clone().unwrap_or_default();
                text::paragraph(ui, message, 10.0, tokens.red, 17.0);
                if Button::new("Créer le dossier de mods")
                    .icon(icons::FOLDER)
                    .show(ui)
                    .clicked()
                {
                    let _ = sender.send(Command::CreateModsDir {
                        slug: slug.to_owned(),
                    });
                }
            }
            Some(state) if state.mods.is_empty() => {
                text::paragraph(
                    ui,
                    "Aucun mod installé. Importez une archive ou déposez vos fichiers dans le dossier de mods.",
                    10.0,
                    tokens.text_2,
                    17.0,
                );
                ui.add_space(10.0);
                if Button::new("Créer le dossier")
                    .icon(icons::FOLDER)
                    .show(ui)
                    .clicked()
                {
                    let _ = sender.send(Command::CreateModsDir {
                        slug: slug.to_owned(),
                    });
                }
            }
            Some(state) => {
                for index in 0..state.mods.len() {
                    mod_row(ui, state, index, sender, slug);
                    ui.add_space(6.0);
                }
            }
        },
    );
}

fn mod_row(
    ui: &mut Ui,
    state: &super::ModsUiState,
    index: usize,
    sender: &UnboundedSender<Command>,
    slug: &str,
) {
    let tokens = theme::current(ui.ctx());
    let module = &state.mods[index];
    egui::Frame::new()
        .fill(tokens.surface_2)
        .stroke(Stroke::new(1.0_f32, tokens.border))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                icons::icon(ui, icons::GRIP_VERTICAL, 15.0, tokens.text_3);
                let mut enabled = module.enabled;
                components::toggle(ui, &mut enabled);
                if enabled != module.enabled {
                    let _ = sender.send(Command::ToggleMod {
                        slug: slug.to_owned(),
                        name: module.name.clone(),
                        enabled,
                    });
                }
                ui.add_space(4.0);
                let name_width = (ui.available_width() - 110.0).max(120.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(name_width, 32.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        let name = text::ellipsize(
                            ui.ctx(),
                            &module.name,
                            text::font(11.0, Weight::Medium),
                            name_width,
                        );
                        text::label(ui, name, 11.0, Weight::Medium, tokens.text);
                        let path = format::short_path(&module.path, 60);
                        text::label(ui, path, 8.0, Weight::Regular, tokens.text_3);
                    },
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if index + 1 < state.mods.len()
                        && components::icon_button_sized(ui, icons::CHEVRON_DOWN, 14.0, 24.0)
                            .clicked()
                    {
                        let mut names: Vec<String> = state
                            .mods
                            .iter()
                            .map(|module| module.name.clone())
                            .collect();
                        names.swap(index, index + 1);
                        let _ = sender.send(Command::ReorderMods {
                            slug: slug.to_owned(),
                            names,
                        });
                    }
                    if index > 0
                        && components::icon_button_sized(ui, icons::CHEVRON_UP, 14.0, 24.0)
                            .clicked()
                    {
                        let mut names: Vec<String> = state
                            .mods
                            .iter()
                            .map(|module| module.name.clone())
                            .collect();
                        names.swap(index, index - 1);
                        let _ = sender.send(Command::ReorderMods {
                            slug: slug.to_owned(),
                            names,
                        });
                    }
                    components::badge(ui, format!("{}", index + 1), BadgeKind::Neutral);
                });
            });
        });
}

fn online_tab(
    ui: &mut Ui,
    store: &Store,
    state: &mut DetailUi,
    slug: &str,
    sender: &UnboundedSender<Command>,
) {
    let tokens = theme::current(ui.ctx());
    let Some(entry) = store.entry(slug) else {
        return;
    };
    if !entry.online.supported {
        empty_block(
            ui,
            icons::WIFI_OFF,
            "Multijoueur non pris en charge",
            "Ce projet ne déclare pas de mode en ligne.",
        );
        return;
    }
    let servers = store.servers(slug);
    let count = servers.map(|state| state.list.servers.len()).unwrap_or(0);
    section_with_actions(
        ui,
        "Serveurs",
        &format!("{count} serveur(s)"),
        |ui| {
            if Button::new("Découvrir en LAN")
                .icon(icons::RADAR)
                .show(ui)
                .clicked()
            {
                let _ = sender.send(Command::DiscoverLan {
                    slug: slug.to_owned(),
                });
            }
            if components::icon_button(ui, icons::REFRESH_CW, false).clicked() {
                let _ = sender.send(Command::PingServers {
                    slug: slug.to_owned(),
                });
            }
        },
        |ui| {
            text::paragraph(
                ui,
                "Les serveurs publics sont fournis par le projet. Les entrées manuelles restent locales.",
                10.0,
                tokens.text_2,
                17.0,
            );
            ui.add_space(10.0);
            if let Some(servers) = servers {
                if servers.loading {
                    components::spinner(ui, 18.0, tokens.text_3);
                }
                for error in &servers.list.errors {
                    text::label(ui, error, 9.0, Weight::Regular, tokens.orange);
                }
                for server in &servers.list.servers {
                    server_row(ui, server, slug, sender);
                    ui.add_space(6.0);
                }
                if servers.list.servers.is_empty() && !servers.loading {
                    text::paragraph(
                        ui,
                        "Aucun serveur connu pour ce jeu. Ajoutez une adresse ou lancez une découverte LAN.",
                        10.0,
                        tokens.text_2,
                        17.0,
                    );
                }
            }
            ui.add_space(12.0);
            components::divider(ui);
            ui.add_space(12.0);
            text::label(
                ui,
                "Ajouter un serveur",
                11.0,
                Weight::SemiBold,
                tokens.text,
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                components::text_field(ui, &mut state.server_name, "Nom", 140.0);
                components::text_field(ui, &mut state.server_address, "hôte ou IP", 170.0);
                components::text_field(ui, &mut state.server_port, "Port", 64.0);
                let valid = !state.server_address.trim().is_empty()
                    && state.server_port.parse::<u16>().is_ok();
                if Button::primary("Ajouter").enabled(valid).show(ui).clicked() {
                    if let Ok(port) = state.server_port.parse::<u16>() {
                        let _ = sender.send(Command::AddServer {
                            slug: slug.to_owned(),
                            name: if state.server_name.trim().is_empty() {
                                state.server_address.clone()
                            } else {
                                state.server_name.clone()
                            },
                            address: state.server_address.trim().to_owned(),
                            port,
                        });
                        state.server_name.clear();
                        state.server_address.clear();
                    }
                }
            });
        },
    );
}

fn server_row(
    ui: &mut Ui,
    server: &recomp_core::online::ServerInfo,
    slug: &str,
    sender: &UnboundedSender<Command>,
) {
    let tokens = theme::current(ui.ctx());
    egui::Frame::new()
        .fill(tokens.surface_2)
        .stroke(Stroke::new(1.0_f32, tokens.border))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(12, 9))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                icons::icon(
                    ui,
                    if server.last_ping_ms.is_some() {
                        icons::WIFI
                    } else {
                        icons::NETWORK
                    },
                    15.0,
                    if server.last_ping_ms.is_some() {
                        tokens.green
                    } else {
                        tokens.text_3
                    },
                );
                let name_width = (ui.available_width() - 220.0).max(120.0);
                ui.allocate_ui_with_layout(
                    Vec2::new(name_width, 34.0),
                    Layout::top_down(Align::Min),
                    |ui| {
                        let name = text::ellipsize(
                            ui.ctx(),
                            &server.name,
                            text::font(11.0, Weight::Medium),
                            name_width,
                        );
                        text::label(ui, name, 11.0, Weight::Medium, tokens.text);
                        let meta = format!("{} · {}", server.endpoint(), server.source.label());
                        let meta = text::ellipsize(
                            ui.ctx(),
                            &meta,
                            text::font(9.0, Weight::Regular),
                            name_width,
                        );
                        text::label(ui, meta, 9.0, Weight::Regular, tokens.text_3);
                    },
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(id) = server.id {
                        if components::icon_button_sized(ui, icons::TRASH_2, 14.0, 24.0).clicked() {
                            let _ = sender.send(Command::RemoveServer {
                                slug: slug.to_owned(),
                                id,
                            });
                        }
                    }
                    if Button::new("Rejoindre").show(ui).clicked() {
                        let _ = sender.send(Command::JoinServer {
                            slug: slug.to_owned(),
                            address: server.address.clone(),
                            port: server.port,
                        });
                    }
                    if let Some(ping) = server.last_ping_ms {
                        text::label(ui, format!("{ping} ms"), 9.0, Weight::Medium, tokens.green);
                    }
                    if let Some(players) = server.players {
                        text::label(
                            ui,
                            format!(
                                "{players}{}",
                                server
                                    .max_players
                                    .map(|max| format!("/{max}"))
                                    .unwrap_or_default()
                            ),
                            9.0,
                            Weight::Regular,
                            tokens.text_2,
                        );
                    }
                });
            });
        });
}

fn configuration(
    ui: &mut Ui,
    store: &Store,
    entry: &recomp_core::catalog::model::GameEntry,
    sender: &UnboundedSender<Command>,
    action: &mut Action,
    installed: bool,
) {
    let tokens = theme::current(ui.ctx());
    let slug = entry.slug.as_str();
    if let Some(user_files) = &entry.user_files {
        section(ui, "Fichier utilisateur", |ui| {
            text::paragraph(
                ui,
                user_files.description.clone(),
                10.0,
                tokens.text_2,
                17.0,
            );
            ui.add_space(10.0);
            if let Some(game) = store.library_game(slug) {
                if let Some(path) = &game.user_file {
                    ui.label(components::mono(
                        format::short_path(path, 70),
                        9.0,
                        tokens.green,
                    ));
                } else {
                    text::label(
                        ui,
                        "Aucun fichier sélectionné.",
                        9.0,
                        Weight::Regular,
                        tokens.orange,
                    );
                }
            }
            ui.add_space(10.0);
            if Button::new("Sélectionner un fichier")
                .icon(icons::FILE_SEARCH)
                .show(ui)
                .clicked()
            {
                let _ = sender.send(Command::SelectUserFile {
                    slug: slug.to_owned(),
                });
            }
        });
        ui.add_space(16.0);
    }

    section(ui, "Chemins et lancement", |ui| {
        if let Some(game) = store.library_game(slug) {
            if let Some(path) = &game.install_path {
                pair(ui, "Dossier d'installation", &format::short_path(path, 52));
                if Button::new("Ouvrir le dossier")
                    .icon(icons::FOLDER_OPEN)
                    .show(ui)
                    .clicked()
                {
                    let _ = sender.send(Command::OpenPath(path.clone()));
                }
                ui.add_space(10.0);
                let saves = path.parent().map(|parent| parent.join("userdata"));
                if let Some(saves) = saves {
                    pair(ui, "Sauvegardes", &format::short_path(&saves, 52));
                    if Button::new("Ouvrir les sauvegardes")
                        .icon(icons::ARCHIVE)
                        .show(ui)
                        .clicked()
                    {
                        let _ = sender.send(Command::OpenPath(saves));
                    }
                }
            }
            if let Some(version) = &game.version {
                ui.add_space(10.0);
                pair(ui, "Version installée", version);
            }
        } else {
            text::paragraph(
                ui,
                "Le jeu doit être installé pour afficher ses chemins.",
                10.0,
                tokens.text_2,
                17.0,
            );
        }
        ui.add_space(12.0);
        let args = entry.launch.args.join(" ");
        pair(
            ui,
            "Arguments de lancement",
            if args.is_empty() { "Aucun" } else { &args },
        );
        pair(
            ui,
            "Exécutable",
            entry
                .launch
                .exe_for(Os::current())
                .unwrap_or("non déclaré pour ce système"),
        );
    });

    if installed {
        ui.add_space(16.0);
        section(ui, "Maintenance", |ui| {
            if Button::new("Vérifier l'installation")
                .icon(icons::SHIELD_CHECK)
                .width(ui.available_width())
                .show(ui)
                .clicked()
            {
                let _ = sender.send(Command::Repair {
                    slug: slug.to_owned(),
                });
            }
            ui.add_space(6.0);
            if Button::ghost("Désinstaller le jeu")
                .icon(icons::TRASH_2)
                .width(ui.available_width())
                .show(ui)
                .clicked()
            {
                *action = Action::Uninstall(slug.to_owned());
            }
        });
    }
}

fn tab_bar(ui: &mut Ui, tab: &mut DetailTab, online: bool) {
    let tokens = theme::current(ui.ctx());
    let mut tabs = vec![
        ("Vue d'ensemble", DetailTab::Overview),
        ("Mods", DetailTab::Mods),
    ];
    if online {
        tabs.push(("En ligne", DetailTab::Online));
    }
    tabs.push(("Configuration", DetailTab::Configuration));

    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 42.0), Sense::hover());
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0_f32, tokens.border),
    );
    let mut x = rect.left();
    for (label, value) in tabs {
        let galley = ui.painter().layout_no_wrap(
            label.to_owned(),
            text::font(11.0, Weight::SemiBold),
            tokens.text,
        );
        let width = galley.size().x + 28.0;
        let tab_rect =
            Rect::from_min_size(egui::pos2(x, rect.top()), Vec2::new(width, rect.height()));
        let response = ui.interact(
            tab_rect,
            ui.id().with(("detail-tab", label)),
            Sense::click(),
        );
        let active = *tab == value;
        let color = if active || response.hovered() {
            tokens.text
        } else {
            tokens.text_2
        };
        let galley = ui.painter().layout_no_wrap(
            label.to_owned(),
            text::font(11.0, Weight::SemiBold),
            color,
        );
        ui.painter().galley(
            egui::pos2(
                tab_rect.center().x - galley.size().x / 2.0,
                tab_rect.center().y - 9.0,
            ),
            galley,
            color,
        );
        if active {
            ui.painter().rect_filled(
                Rect::from_min_size(
                    egui::pos2(tab_rect.left() + 10.0, rect.bottom() - 2.0),
                    Vec2::new(tab_rect.width() - 20.0, 2.0),
                ),
                CornerRadius::same(1),
                tokens.accent,
            );
        }
        if response.clicked() {
            *tab = value;
        }
        x += width;
    }
}

fn section(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    section_with_actions(ui, title, "", |_| {}, body);
}

fn section_with_actions(
    ui: &mut Ui,
    title: &str,
    subtitle: &str,
    actions: impl FnOnce(&mut Ui),
    body: impl FnOnce(&mut Ui),
) {
    let tokens = theme::current(ui.ctx());
    egui::Frame::new()
        .fill(tokens.surface)
        .stroke(Stroke::new(1.0_f32, tokens.border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(18, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                text::label(ui, title, 12.0, Weight::SemiBold, tokens.text);
                if !subtitle.is_empty() {
                    ui.add_space(6.0);
                    text::label(ui, subtitle, 9.0, Weight::Regular, tokens.text_3);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), actions);
            });
            ui.add_space(12.0);
            body(ui);
        });
}

fn pair(ui: &mut Ui, label: &str, value: &str) {
    let tokens = theme::current(ui.ctx());
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        text::label(ui, label, 9.0, Weight::Regular, tokens.text_3);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            text::label(ui, value, 9.0, Weight::Medium, tokens.text);
        });
    });
    ui.add_space(5.0);
}

fn empty_block(ui: &mut Ui, glyph: char, title: &str, description: &str) {
    let tokens = theme::current(ui.ctx());
    ui.add_space(30.0);
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(48.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(12), tokens.surface_2);
        icons::paint(ui.painter(), rect.center(), glyph, 22.0, tokens.text_3);
        ui.add_space(12.0);
        text::label(ui, title, 13.0, Weight::SemiBold, tokens.text);
        ui.add_space(5.0);
        text::label(ui, description, 10.0, Weight::Regular, tokens.text_2);
    });
}

fn badge_width(ui: &Ui, label: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(
            label.to_owned(),
            text::font(9.0, Weight::SemiBold),
            Color32::WHITE,
        )
        .size()
        .x
        + 14.0
}

fn paint_badge(ui: &Ui, position: Pos2, label: &str, kind: BadgeKind) {
    let tokens = theme::current(ui.ctx());
    let (fg, bg) = match kind {
        BadgeKind::Playable => (tokens.green, tokens.green_soft),
        BadgeKind::Beta => (tokens.orange, tokens.orange_soft),
        BadgeKind::Wip => (tokens.text_2, tokens.surface_3),
        BadgeKind::Recomp | BadgeKind::Decomp | BadgeKind::Native | BadgeKind::Accent => {
            (tokens.accent, tokens.accent_soft)
        }
        BadgeKind::Last => (tokens.blue, tokens.blue_soft),
        BadgeKind::Neutral => (tokens.text_2, tokens.surface_3),
    };
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_owned(), text::font(9.0, Weight::SemiBold), fg);
    let rect = Rect::from_min_size(position, Vec2::new(galley.size().x + 14.0, 18.0));
    ui.painter().rect_filled(rect, CornerRadius::same(6), bg);
    ui.painter().galley(
        egui::pos2(
            rect.center().x - galley.size().x / 2.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        fg,
    );
}

fn paint_text(ui: &Ui, position: Pos2, value: &str, font: FontId, color: Color32) {
    let galley = ui.painter().layout_no_wrap(value.to_owned(), font, color);
    ui.painter().galley(position, galley, color);
}
