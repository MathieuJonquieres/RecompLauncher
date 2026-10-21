//! Paramètres: theme, diagnostics, paths, network, storage and licences.
//! Every control writes through the real settings backend.

use std::path::PathBuf;

use eframe::egui::{self, Align, CornerRadius, Layout, Margin, Sense, Stroke, Ui, Vec2};
use tokio::sync::mpsc::UnboundedSender;

use crate::backend_bridge::{Command, SettingsSnapshot};
use crate::ui::components::{self, BadgeKind, Button};
use crate::ui::text::Weight;
use crate::ui::{format, icons, text, theme};

use super::Store;

pub struct SettingsUi {
    pub loaded: bool,
    pub language: String,
    pub diagnostics: bool,
    pub proxy: String,
    pub manifest_url: String,
    pub servers_url: String,
    pub install_dir: String,
    pub data_dir: String,
    pub portable: bool,
}

impl Default for SettingsUi {
    fn default() -> Self {
        Self {
            loaded: false,
            language: "fr-FR".to_owned(),
            diagnostics: false,
            proxy: String::new(),
            manifest_url: String::new(),
            servers_url: String::new(),
            install_dir: String::new(),
            data_dir: String::new(),
            portable: false,
        }
    }
}

impl SettingsUi {
    pub fn apply(&mut self, snapshot: SettingsSnapshot) {
        self.language = snapshot.language;
        self.diagnostics = snapshot.diagnostics;
        self.proxy = snapshot.proxy;
        self.manifest_url = snapshot.manifest_url;
        self.servers_url = snapshot.servers_url;
        self.install_dir = snapshot.install_dir;
        self.data_dir = snapshot.data_dir;
        self.portable = snapshot.portable;
        self.loaded = true;
    }

    pub fn save(&self, dark_theme: bool, sender: &UnboundedSender<Command>) {
        let _ = sender.send(Command::SaveSettings {
            language: self.language.clone(),
            dark_theme,
            diagnostics: self.diagnostics,
            proxy: self.proxy.clone(),
            manifest_url: self.manifest_url.clone(),
            servers_url: self.servers_url.clone(),
        });
    }
}

pub fn show(
    ui: &mut Ui,
    store: &Store,
    state: &mut SettingsUi,
    dark_theme: &mut bool,
    sender: &UnboundedSender<Command>,
) {
    let tokens = theme::current(ui.ctx());
    components::page_header(
        ui,
        "Paramètres",
        "Configurez l'apparence, le réseau et le stockage de votre launcher.",
        |_ui| {},
    );

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_max_width(860.0);
            ui.set_width(ui.available_width());

            section(ui, "Apparence", |ui| {
                row(ui, "Thème", "Choisissez l'apparence de l'interface.", |ui| {
                    let before = *dark_theme;
                    components::segmented(ui, dark_theme);
                    if *dark_theme != before {
                        state.save(*dark_theme, sender);
                    }
                });
                divider(ui);
                row(
                    ui,
                    "Langue de l'interface",
                    "L'interface de cette version est en français.",
                    |ui| {
                        egui::Frame::new()
                            .fill(tokens.surface_2)
                            .stroke(Stroke::new(1.0_f32, tokens.border))
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    icons::icon(ui, icons::GLOBE_2, 14.0, tokens.text_3);
                                    text::label(ui, "Français", 10.0, Weight::Medium, tokens.text);
                                });
                            });
                    },
                );
                divider(ui);
                row(
                    ui,
                    "Diagnostics",
                    "Afficher les détails techniques dans les messages d'erreur.",
                    |ui| {
                        let before = state.diagnostics;
                        components::toggle(ui, &mut state.diagnostics);
                        if state.diagnostics != before {
                            state.save(*dark_theme, sender);
                        }
                    },
                );
            });

            ui.add_space(16.0);
            section(ui, "Installation", |ui| {
                path_row(
                    ui,
                    "Dossier d'installation",
                    "Les jeux gérés par le launcher sont installés ici.",
                    &state.install_dir,
                    |ui| {
                        if Button::new("Ouvrir")
                            .icon(icons::FOLDER_OPEN)
                            .show(ui)
                            .clicked()
                        {
                            let _ = sender.send(Command::OpenPath(PathBuf::from(
                                state.install_dir.clone(),
                            )));
                        }
                    },
                );
                divider(ui);
                path_row(
                    ui,
                    "Dossier de données",
                    "Cache, journaux et base de données locale.",
                    &state.data_dir,
                    |ui| {
                        if Button::new("Ouvrir")
                            .icon(icons::FOLDER_OPEN)
                            .show(ui)
                            .clicked()
                        {
                            let _ = sender.send(Command::OpenDataDir);
                        }
                    },
                );
                divider(ui);
                row(
                    ui,
                    "Mode portable",
                    "Les données sont stockées à côté de l'exécutable.",
                    |ui| {
                        components::badge(
                            ui,
                            if state.portable { "Portable" } else { "Standard" },
                            BadgeKind::Neutral,
                        );
                    },
                );
            });

            ui.add_space(16.0);
            let mut network_dirty = false;
            section(ui, "Réseau", |ui| {
                network_dirty |= field_row(
                    ui,
                    "Proxy",
                    "Laissez vide pour utiliser la configuration système.",
                    &mut state.proxy,
                    "http://127.0.0.1:8080",
                    false,
                );
                divider(ui);
                network_dirty |= field_row(
                    ui,
                    "URL du catalogue",
                    "Manifeste JSON utilisé pour découvrir les jeux.",
                    &mut state.manifest_url,
                    "https://…/catalog.json",
                    false,
                );
                divider(ui);
                network_dirty |= field_row(
                    ui,
                    "URL des serveurs",
                    "Liste publique de serveurs proposée par les projets.",
                    &mut state.servers_url,
                    "https://…/servers.json",
                    false,
                );
                ui.add_space(10.0);
                text::label(
                    ui,
                    "Les modifications sont enregistrées automatiquement.",
                    8.0,
                    Weight::Regular,
                    tokens.text_3,
                );
            });
            if network_dirty {
                state.save(*dark_theme, sender);
            }

            ui.add_space(16.0);
            section(ui, "Stockage", |ui| {
                ui.horizontal(|ui| {
                    storage_metric(
                        ui,
                        icons::HARD_DRIVE,
                        &format::bytes(store.stats.games),
                        "Jeux installés",
                    );
                    ui.add_space(18.0);
                    storage_metric(
                        ui,
                        icons::ARCHIVE,
                        &format::bytes(store.stats.cache),
                        "Cache local",
                    );
                });
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if Button::new("Vider le cache")
                        .icon(icons::TRASH_2)
                        .show(ui)
                        .clicked()
                    {
                        let _ = sender.send(Command::ClearCache);
                    }
                    if Button::new("Exporter les journaux")
                        .icon(icons::DOWNLOAD)
                        .show(ui)
                        .clicked()
                    {
                        let _ = sender.send(Command::ExportLogs);
                    }
                });
            });

            ui.add_space(16.0);
            section(ui, "À propos", |ui| {
                ui.horizontal(|ui| {
                    let (logo, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
                    crate::ui::art::brand(ui, logo);
                    icons::paint(
                        ui.painter(),
                        logo.center(),
                        icons::GAMEPAD_2,
                        20.0,
                        eframe::egui::Color32::WHITE,
                    );
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        text::label(ui, "RecompLauncher", 13.0, Weight::SemiBold, tokens.text);
                        text::label(
                            ui,
                            format!("Version {}", env!("CARGO_PKG_VERSION")),
                            9.0,
                            Weight::Regular,
                            tokens.text_2,
                        );
                    });
                });
                ui.add_space(12.0);
                text::paragraph(
                    ui,
                    "Launcher de jeux recompilés, décompilés et portés nativement. Les fichiers utilisateur restent sur votre machine et ne sont jamais téléversés.",
                    10.0,
                    tokens.text_2,
                    17.0,
                );
                ui.add_space(10.0);
                text::label(
                    ui,
                    "Polices : Inter (SIL Open Font License 1.1) · Icônes : Lucide (ISC).",
                    8.0,
                    Weight::Regular,
                    tokens.text_3,
                );
            });

            ui.add_space(30.0);
        });
}

fn section(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    let tokens = theme::current(ui.ctx());
    egui::Frame::new()
        .fill(tokens.surface)
        .stroke(Stroke::new(1.0_f32, tokens.border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(18, 16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            text::label(ui, title, 12.0, Weight::SemiBold, tokens.text);
            ui.add_space(12.0);
            body(ui);
        });
}

fn row(ui: &mut Ui, label: &str, description: &str, right: impl FnOnce(&mut Ui)) {
    let tokens = theme::current(ui.ctx());
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            text::label(ui, label, 10.0, Weight::Medium, tokens.text);
            ui.add_space(2.0);
            text::label(ui, description, 8.0, Weight::Regular, tokens.text_3);
        });
        ui.with_layout(Layout::right_to_left(Align::Center), right);
    });
}

fn divider(ui: &mut Ui) {
    ui.add_space(12.0);
    components::divider(ui);
    ui.add_space(12.0);
}

fn path_row(ui: &mut Ui, label: &str, description: &str, path: &str, right: impl FnOnce(&mut Ui)) {
    let tokens = theme::current(ui.ctx());
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            text::label(ui, label, 10.0, Weight::Medium, tokens.text);
            ui.add_space(2.0);
            text::label(ui, description, 8.0, Weight::Regular, tokens.text_3);
            ui.add_space(4.0);
            ui.label(components::mono(
                format::short_path(std::path::Path::new(path), 58),
                8.0,
                tokens.text_2,
            ));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), right);
    });
}

fn field_row(
    ui: &mut Ui,
    label: &str,
    description: &str,
    value: &mut String,
    hint: &str,
    mono: bool,
) -> bool {
    let tokens = theme::current(ui.ctx());
    ui.vertical(|ui| {
        text::label(ui, label, 10.0, Weight::Medium, tokens.text);
        ui.add_space(2.0);
        text::label(ui, description, 8.0, Weight::Regular, tokens.text_3);
        ui.add_space(6.0);
        let width = ui.available_width().min(520.0);
        let response = if mono {
            components::text_field_mono(ui, value, hint, width)
        } else {
            components::text_field(ui, value, hint, width)
        };
        response.lost_focus()
    })
    .inner
}

fn storage_metric(ui: &mut Ui, glyph: char, value: &str, label: &str) {
    let tokens = theme::current(ui.ctx());
    ui.horizontal(|ui| {
        icons::icon(ui, glyph, 16.0, tokens.text_3);
        ui.vertical(|ui| {
            text::label(ui, value, 13.0, Weight::SemiBold, tokens.text);
            text::label(ui, label, 8.0, Weight::Regular, tokens.text_3);
        });
    });
}
