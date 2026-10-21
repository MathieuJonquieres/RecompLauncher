//! Library page: installed, manually added and external games.
//!
//! The view never touches the filesystem or network. Every action is a
//! `Command` handled by the backend, and state arrives through `Event::Library`.

use std::collections::{HashMap, HashSet};

use eframe::egui::{self, Color32, RichText};
use recomp_core::catalog::model::{GameEntry, ModStrategy};
use recomp_core::db::now_unix;
use recomp_core::library::{InstallSource, LibraryGame};
use tokio::sync::mpsc::UnboundedSender;

use crate::backend_bridge::Command;
use crate::views::{InstallProgress, ModsUiState};

#[derive(Default)]
pub struct LibraryView {
    pub games: Vec<LibraryGame>,
    pub installs: HashMap<String, InstallProgress>,
    pub mods: HashMap<String, ModsUiState>,
    pub mods_supported: HashSet<String>,
    pub search: String,
    pending_uninstall: Option<String>,
}

impl LibraryView {
    pub fn apply_library(&mut self, games: Vec<LibraryGame>) {
        self.games = games;
    }

    pub fn apply_catalog(&mut self, entries: &[GameEntry]) {
        self.mods_supported.clear();
        for entry in entries {
            if entry.mods.supported && entry.mods.strategy != ModStrategy::None {
                self.mods_supported.insert(entry.slug.as_str().to_string());
            }
        }
    }

    pub fn apply_mods(&mut self, slug: &str, state: ModsUiState) {
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

    pub fn show_central(&mut self, ui: &mut egui::Ui, commands: &UnboundedSender<Command>) {
        ui.horizontal(|ui| {
            ui.heading("Bibliothèque");
            ui.label(
                RichText::new(format!("{} jeu(x)", self.games.len()))
                    .small()
                    .weak(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Actualiser").clicked() {
                    let _ = commands.send(Command::RefreshLibrary);
                }
            });
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Filtrer par titre ou slug…")
                    .desired_width(260.0),
            );
        });
        ui.separator();

        if self.games.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(
                        "Votre bibliothèque est vide. Installez un jeu depuis le catalogue \
                         ou ajoutez un jeu externe.",
                    )
                    .weak(),
                );
            });
            self.uninstall_dialog(ui.ctx(), commands);
            return;
        }

        let query = self.search.trim().to_lowercase();
        let filtered: Vec<&LibraryGame> = self
            .games
            .iter()
            .filter(|game| {
                query.is_empty()
                    || game.title.to_lowercase().contains(&query)
                    || game.slug.to_lowercase().contains(&query)
            })
            .collect();

        if filtered.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("Aucun jeu ne correspond à ce filtre.").weak());
            });
            self.uninstall_dialog(ui.ctx(), commands);
            return;
        }

        let mut request_uninstall = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for game in filtered {
                    self.game_row(ui, game, commands, &mut request_uninstall);
                    ui.add_space(6.0);
                }
            });
        if request_uninstall.is_some() {
            self.pending_uninstall = request_uninstall;
        }
        self.uninstall_dialog(ui.ctx(), commands);
    }

    fn game_row(
        &self,
        ui: &mut egui::Ui,
        game: &LibraryGame,
        commands: &UnboundedSender<Command>,
        request_uninstall: &mut Option<String>,
    ) {
        egui::Frame::group(ui.style())
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.strong(&game.title);
                            source_badge(ui, game.source);
                        });
                        ui.label(
                            RichText::new(format!(
                                "{} · version {}",
                                game.slug,
                                game.version.as_deref().unwrap_or("inconnue")
                            ))
                            .small()
                            .weak()
                            .monospace(),
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        if ui.button(RichText::new("▶ Jouer").strong()).clicked() {
                            let _ = commands.send(Command::Play {
                                slug: game.slug.clone(),
                            });
                        }
                        if ui.button("Retirer").clicked() {
                            *request_uninstall = Some(game.slug.clone());
                        }
                        if game.source == InstallSource::Managed {
                            if ui.button("Réparer").clicked() {
                                let _ = commands.send(Command::Repair {
                                    slug: game.slug.clone(),
                                });
                            }
                            if ui.button("Réinstaller").clicked() {
                                let _ = commands.send(Command::Install {
                                    slug: game.slug.clone(),
                                });
                            }
                            if ui.button("Choisir mon fichier").clicked() {
                                let _ = commands.send(Command::SelectUserFile {
                                    slug: game.slug.clone(),
                                });
                            }
                        }
                        if let Some(path) =
                            game.install_path.as_ref().or(game.external_exe.as_ref())
                        {
                            if ui.button("Ouvrir le dossier").clicked() {
                                let target = if path.is_dir() {
                                    path.clone()
                                } else {
                                    path.parent()
                                        .map(std::path::Path::to_path_buf)
                                        .unwrap_or_else(|| path.clone())
                                };
                                let _ = commands.send(Command::OpenPath(target));
                            }
                        }
                        if self.mods_supported.contains(&game.slug) {
                            if ui.button("Créer le dossier mods").clicked() {
                                let _ = commands.send(Command::CreateModsDir {
                                    slug: game.slug.clone(),
                                });
                            }
                            if ui.button("Ouvrir les mods").clicked() {
                                let _ = commands.send(Command::OpenModsDir {
                                    slug: game.slug.clone(),
                                });
                            }
                            if ui.button("Actualiser les mods").clicked() {
                                let _ = commands.send(Command::RefreshMods {
                                    slug: game.slug.clone(),
                                });
                            }
                        }
                    });
                });

                ui.add_space(4.0);
                if let Some(path) = &game.install_path {
                    ui.label(
                        RichText::new(path.display().to_string())
                            .small()
                            .monospace()
                            .weak(),
                    );
                }
                if let Some(path) = &game.external_exe {
                    ui.label(
                        RichText::new(format!("Exécutable : {}", path.display()))
                            .small()
                            .monospace()
                            .weak(),
                    );
                }
                if let Some(path) = &game.user_file {
                    ui.label(
                        RichText::new(format!("Fichier utilisateur : {}", path.display()))
                            .small()
                            .color(Color32::from_rgb(150, 200, 170)),
                    );
                } else if game.source == InstallSource::Managed {
                    ui.label(
                        RichText::new("Fichier utilisateur : non sélectionné")
                            .small()
                            .color(Color32::from_rgb(210, 170, 100)),
                    );
                }
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!(
                            "Temps de jeu : {} · Dernier lancement : {}",
                            format_playtime(game.playtime_s),
                            format_timestamp(game.last_played)
                        ))
                        .small()
                        .weak(),
                    );
                });

                if let Some(state) = self.mods.get(&game.slug) {
                    let enabled = state.mods.iter().filter(|record| record.enabled).count();
                    ui.label(
                        RichText::new(format!(
                            "Mods : {} détecté(s), {} activé(s)",
                            state.mods.len(),
                            enabled
                        ))
                        .small()
                        .weak(),
                    );
                    if let Some(error) = &state.error {
                        ui.label(
                            RichText::new(error)
                                .small()
                                .color(Color32::from_rgb(220, 130, 130)),
                        );
                    }
                }

                if let Some(progress) = self.installs.get(&game.slug).copied() {
                    ui.add_space(4.0);
                    match progress.fraction() {
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
                                ui.label(
                                    RichText::new(format!(
                                        "{} téléchargés",
                                        format_bytes(progress.downloaded)
                                    ))
                                    .small(),
                                );
                            });
                        }
                    }
                    if ui.button("Annuler l'installation").clicked() {
                        let _ = commands.send(Command::CancelInstall {
                            slug: game.slug.clone(),
                        });
                    }
                }
            });
    }

    fn uninstall_dialog(&mut self, ctx: &egui::Context, commands: &UnboundedSender<Command>) {
        let Some(slug) = self.pending_uninstall.clone() else {
            return;
        };
        let game = self.games.iter().find(|game| game.slug == slug);
        let title = game
            .map(|game| game.title.clone())
            .unwrap_or_else(|| slug.clone());
        let managed = game.is_some_and(|game| game.source == InstallSource::Managed);
        let mut open = true;
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new("Confirmer le retrait")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(format!("Retirer « {title} » de la bibliothèque ?"));
                if managed {
                    ui.label(
                        RichText::new(
                            "Les fichiers installés par le launcher seront supprimés. \
                             Les dossiers userdata/ et mods/ seront conservés, ainsi que vos dumps personnels.",
                        )
                        .small()
                        .weak(),
                    );
                } else {
                    ui.label(
                        RichText::new(
                            "Seule la fiche sera supprimée. Aucun fichier ne sera effacé.",
                        )
                        .small()
                        .weak(),
                    );
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Retirer").clicked() {
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

fn source_badge(ui: &mut egui::Ui, source: InstallSource) {
    let color = match source {
        InstallSource::Managed => Color32::from_rgb(40, 110, 75),
        InstallSource::Manual => Color32::from_rgb(55, 85, 125),
        InstallSource::External => Color32::from_rgb(105, 75, 120),
        _ => Color32::GRAY,
    };
    egui::Frame::new()
        .fill(color)
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(6, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(source.label()).small().color(Color32::WHITE));
        });
}

fn format_playtime(seconds: u64) -> String {
    if seconds < 60 {
        format!("{seconds} s")
    } else if seconds < 3600 {
        format!("{} min", seconds / 60)
    } else {
        format!("{} h {} min", seconds / 3600, (seconds % 3600) / 60)
    }
}

fn format_timestamp(timestamp: Option<i64>) -> String {
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
