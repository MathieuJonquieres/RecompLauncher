//! Uninstallation confirmation with an explicit save-data choice.

use eframe::egui::{self, CornerRadius, Margin, Sense, Stroke, Ui, Vec2};
use tokio::sync::mpsc::UnboundedSender;

use crate::backend_bridge::Command;
use crate::ui::components::{self, Button};
use crate::ui::text::Weight;
use crate::ui::{format, icons, modal, text, theme};

use super::Store;

pub struct UninstallModal {
    pub slug: String,
    pub step: usize,
    pub delete_saves: bool,
    pub error: Option<String>,
}

impl UninstallModal {
    pub fn new(slug: String) -> Self {
        Self {
            slug,
            step: 0,
            delete_saves: false,
            error: None,
        }
    }

    pub fn on_uninstalled(&mut self) {
        self.step = 2;
    }

    pub fn on_failed(&mut self, message: String) {
        self.error = Some(message);
        self.step = 0;
    }
}

pub fn show(
    ctx: &egui::Context,
    state: &mut UninstallModal,
    store: &Store,
    sender: &UnboundedSender<Command>,
) -> bool {
    let entry = store.entry(&state.slug).cloned();
    let game = store.library_game(&state.slug).cloned();
    let title = entry
        .as_ref()
        .map(|entry| entry.title.clone())
        .or_else(|| game.as_ref().map(|game| game.title.clone()))
        .unwrap_or_else(|| state.slug.clone());
    let width = 520.0;
    let close = std::cell::Cell::new(false);

    let response = modal::show(
        ctx,
        "uninstall-modal",
        width,
        state,
        |ui, state| match state.step {
            0 => {
                if modal::header(ui, "DÉSINSTALLATION", &format!("Désinstaller {title}")) {
                    close.set(true);
                }
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(20, 18))
                    .show(ui, |ui| {
                        ui.set_width(width - 40.0);
                        step_confirm(ui, state, entry.as_ref(), game.as_ref());
                    });
            }
            1 => {
                modal::header_locked(ui, "DÉSINSTALLATION", "Suppression en cours");
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(20, 28))
                    .show(ui, |ui| {
                        ui.set_width(width - 40.0);
                        ui.vertical_centered(|ui| {
                            components::spinner(ui, 30.0, theme::current(ui.ctx()).accent);
                            ui.add_space(12.0);
                            text::label(
                                ui,
                                "Suppression des fichiers du jeu…",
                                12.0,
                                Weight::SemiBold,
                                theme::current(ui.ctx()).text,
                            );
                            ui.add_space(4.0);
                            text::label(
                                ui,
                                "La bibliothèque sera actualisée dès que l'opération sera terminée.",
                                9.0,
                                Weight::Regular,
                                theme::current(ui.ctx()).text_2,
                            );
                        });
                    });
            }
            _ => {
                if modal::header(ui, "DÉSINSTALLATION", "Jeu désinstallé") {
                    close.set(true);
                }
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(20, 24))
                    .show(ui, |ui| {
                        ui.set_width(width - 40.0);
                        ui.vertical_centered(|ui| {
                            let tokens = theme::current(ui.ctx());
                            let (circle, _) =
                                ui.allocate_exact_size(Vec2::splat(52.0), Sense::hover());
                            ui.painter()
                                .circle_filled(circle.center(), 26.0, tokens.green_soft);
                            icons::paint(
                                ui.painter(),
                                circle.center(),
                                icons::CHECK,
                                26.0,
                                tokens.green,
                            );
                            ui.add_space(12.0);
                            text::label(
                                ui,
                                format!("{title} a été retiré de la bibliothèque."),
                                13.0,
                                Weight::SemiBold,
                                tokens.text,
                            );
                            ui.add_space(5.0);
                            text::label(
                                ui,
                                if state.delete_saves {
                                    "Les sauvegardes et fichiers de configuration ont été supprimés."
                                } else {
                                    "Les sauvegardes et fichiers de configuration ont été conservés."
                                },
                                9.0,
                                Weight::Regular,
                                tokens.text_2,
                            );
                        });
                    });
            }
        },
        |ui, state| {
            let step = state.step;
            if step != 1 {
                modal::footer(ui, |ui| match step {
                    0 => footer_confirm(ui, state, sender),
                    _ => footer_done(ui, &close),
                });
            }
        },
    );

    if response.escape && state.step != 1 {
        close.set(true);
    }
    if response.backdrop_clicked && state.step != 1 {
        close.set(true);
    }
    close.get() || state.step == 4
}

fn footer_confirm(ui: &mut Ui, state: &mut UninstallModal, sender: &UnboundedSender<Command>) {
    if Button::danger("Désinstaller")
        .icon(icons::TRASH_2)
        .show(ui)
        .clicked()
    {
        let _ = sender.send(Command::Uninstall {
            slug: state.slug.clone(),
            delete_saves: state.delete_saves,
        });
        state.step = 1;
        state.error = None;
    }
    if Button::new("Annuler").show(ui).clicked() {
        state.step = 4;
    }
}

fn footer_done(ui: &mut Ui, close: &std::cell::Cell<bool>) {
    if Button::primary("Fermer").show(ui).clicked() {
        close.set(true);
    }
}

fn step_confirm(
    ui: &mut Ui,
    state: &mut UninstallModal,
    entry: Option<&recomp_core::catalog::model::GameEntry>,
    game: Option<&recomp_core::library::LibraryGame>,
) {
    let tokens = theme::current(ui.ctx());
    text::paragraph(
        ui,
        "Cette action supprime les fichiers installés par le launcher. Elle est immédiate et ne peut pas être annulée.",
        10.0,
        tokens.text_2,
        17.0,
    );
    ui.add_space(12.0);

    ui.horizontal(|ui| {
        let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::hover());
        ui.painter()
            .rect_filled(icon_rect, CornerRadius::same(9), tokens.orange_soft);
        icons::paint(
            ui.painter(),
            icon_rect.center(),
            icons::TRIANGLE_ALERT,
            16.0,
            tokens.orange,
        );
        ui.vertical(|ui| {
            text::label(
                ui,
                "Fichiers concernés",
                10.0,
                Weight::SemiBold,
                tokens.text,
            );
            let paths = if let Some(game) = game {
                game.install_path
                    .as_ref()
                    .map(|path| format::short_path(path, 56))
                    .unwrap_or_else(|| "Aucun dossier géré".to_owned())
            } else {
                "Aucun dossier géré".to_owned()
            };
            text::label(ui, paths, 9.0, Weight::Regular, tokens.text_2);
        });
    });

    ui.add_space(12.0);
    let saves = game
        .and_then(|game| game.install_path.as_ref())
        .and_then(|path| path.parent())
        .map(|parent| parent.join("userdata"));
    let saves_label = saves
        .as_ref()
        .map(|path| format::short_path(path, 56))
        .unwrap_or_else(|| "Dossier de sauvegardes inconnu".to_owned());

    let row = egui::Frame::new()
        .fill(tokens.surface_2)
        .stroke(Stroke::new(1.0_f32, tokens.border))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (square, _) = ui.allocate_exact_size(Vec2::splat(17.0), Sense::hover());
                components::checkbox_square(ui.painter(), square, state.delete_saves, &tokens);
                ui.vertical(|ui| {
                    text::label(
                        ui,
                        "Supprimer aussi les sauvegardes",
                        10.0,
                        Weight::SemiBold,
                        tokens.text,
                    );
                    text::label(ui, &saves_label, 8.0, Weight::Regular, tokens.text_3);
                });
            });
        });
    let response = row.response.interact(Sense::click());
    if response.clicked() {
        state.delete_saves = !state.delete_saves;
    }

    if let Some(entry) = entry {
        if entry.user_files.is_some() {
            ui.add_space(10.0);
            text::label(
                ui,
                "Le fichier utilisateur (dump) n'est jamais copié ni supprimé.",
                8.0,
                Weight::Regular,
                tokens.text_3,
            );
        }
    }

    if let Some(error) = &state.error {
        ui.add_space(12.0);
        modal::info_box(
            ui,
            icons::CIRCLE_ALERT,
            modal::Tone::Red,
            "La désinstallation a échoué",
            error,
        );
    }

    ui.add_space(4.0);
}
