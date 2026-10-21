//! Settings page: locale, theme, network endpoints, cache and log export.
//!
//! The view only edits a local copy and sends one command; persistence and
//! cache/disk work happen in the backend.

use eframe::egui::{self, RichText};
use recomp_core::i18n::I18n;
use tokio::sync::mpsc::UnboundedSender;

use crate::backend_bridge::{Command, SettingsSnapshot};

#[derive(Default)]
pub struct SettingsView {
    pub snapshot: Option<SettingsSnapshot>,
    loaded: bool,
    language: String,
    dark_theme: bool,
    proxy: String,
    manifest_url: String,
    servers_url: String,
}

impl SettingsView {
    pub fn apply(&mut self, snapshot: SettingsSnapshot) {
        self.language = snapshot.language.clone();
        self.dark_theme = snapshot.dark_theme;
        self.proxy = snapshot.proxy.clone();
        self.manifest_url = snapshot.manifest_url.clone();
        self.servers_url = snapshot.servers_url.clone();
        self.snapshot = Some(snapshot);
        self.loaded = true;
    }

    pub fn show_central(
        &mut self,
        ui: &mut egui::Ui,
        commands: &UnboundedSender<Command>,
        i18n: &I18n,
    ) {
        ui.heading(i18n.text("settings-title"));
        ui.add_space(4.0);
        ui.label(RichText::new(i18n.text("nav-settings-description")).weak());
        ui.add_space(12.0);
        if !self.loaded {
            if ui.button(i18n.text("common-refresh")).clicked() {
                let _ = commands.send(Command::LoadSettings);
            }
            return;
        }

        egui::Grid::new("settings-grid")
            .num_columns(2)
            .spacing([18.0, 10.0])
            .show(ui, |ui| {
                ui.label(i18n.text("settings-language"));
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.language, "fr-FR".to_string(), "Français");
                    ui.radio_value(&mut self.language, "en-US".to_string(), "English");
                });
                ui.end_row();

                ui.label(i18n.text("settings-theme"));
                ui.checkbox(&mut self.dark_theme, "");
                ui.end_row();

                ui.label(i18n.text("settings-proxy"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.proxy)
                        .hint_text("http://127.0.0.1:8080")
                        .desired_width(360.0),
                );
                ui.end_row();

                ui.label(i18n.text("settings-manifest_url"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.manifest_url)
                        .hint_text("https://…/catalog.json")
                        .desired_width(360.0),
                );
                ui.end_row();

                ui.label(i18n.text("settings-servers_url"));
                ui.add(
                    egui::TextEdit::singleline(&mut self.servers_url)
                        .hint_text("https://…/servers.json")
                        .desired_width(360.0),
                );
                ui.end_row();
            });

        ui.add_space(10.0);
        if ui.button(i18n.text("common-save")).clicked() {
            let _ = commands.send(Command::SaveSettings {
                language: self.language.clone(),
                dark_theme: self.dark_theme,
                proxy: self.proxy.clone(),
                manifest_url: self.manifest_url.clone(),
                servers_url: self.servers_url.clone(),
            });
        }

        ui.add_space(18.0);
        ui.separator();
        if let Some(snapshot) = &self.snapshot {
            ui.label(format!(
                "{} : {}",
                i18n.text("settings-data_dir"),
                snapshot.data_dir
            ));
            ui.label(format!(
                "{} : {}",
                i18n.text("settings-install_dir"),
                snapshot.install_dir
            ));
            if snapshot.portable {
                ui.label(i18n.text("settings-portable"));
            }
        }
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            if ui.button(i18n.text("settings-clear_cache")).clicked() {
                let _ = commands.send(Command::ClearCache);
            }
            if ui.button(i18n.text("settings-export_logs")).clicked() {
                let _ = commands.send(Command::ExportLogs);
            }
            if ui.button(i18n.text("settings-open_data")).clicked() {
                let _ = commands.send(Command::OpenDataDir);
            }
        });
        ui.add_space(12.0);
        ui.label(RichText::new(i18n.text("settings-about")).small().weak());
    }
}
