//! Real installation flow: version choice, backend download, archive hash
//! verification and user-file check.

use std::time::{Duration, Instant};

use eframe::egui::{self, Align, CornerRadius, Layout, Margin, Sense, Stroke, Ui, Vec2};
use recomp_core::paths::AppPaths;
use tokio::sync::mpsc::UnboundedSender;

use crate::backend_bridge::{Command, UserFileCheckStatus};
use crate::ui::components::{self, BadgeKind, Button};
use crate::ui::text::Weight;
use crate::ui::{format, icons, modal, text, theme};

use super::{InstallProgress, Store, FILE_CHECK_MINIMUM};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileCheckUi {
    Idle,
    Verified { label: String, at: Instant },
    Rejected(String),
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct InstallOutcome {
    pub version: String,
    pub user_file_required: bool,
    pub repaired: bool,
}

pub struct InstallModal {
    pub slug: String,
    pub step: usize,
    pub started: bool,
    pub progress: InstallProgress,
    pub outcome: Option<InstallOutcome>,
    pub error: Option<String>,
    pub file_check: FileCheckUi,
    pub speed_bps: f64,
    pub min_step_until: Option<Instant>,
    pub last_sample: Option<(Instant, u64)>,
}

impl InstallModal {
    pub fn new(slug: String) -> Self {
        Self {
            slug,
            step: 0,
            started: false,
            progress: InstallProgress::default(),
            outcome: None,
            error: None,
            file_check: FileCheckUi::Idle,
            speed_bps: 0.0,
            min_step_until: None,
            last_sample: None,
        }
    }

    pub fn on_started(&mut self, repaired: bool) {
        self.step = 1;
        self.started = true;
        self.error = None;
        self.progress.repairing = repaired;
        self.last_sample = Some((Instant::now(), 0));
    }

    pub fn on_progress(&mut self, downloaded: u64, total: Option<u64>) {
        if let Some((at, previous)) = self.last_sample {
            let elapsed = at.elapsed().as_secs_f64();
            if elapsed >= 0.15 && downloaded >= previous {
                self.speed_bps = (downloaded - previous) as f64 / elapsed;
                self.last_sample = Some((Instant::now(), downloaded));
            }
        } else {
            self.last_sample = Some((Instant::now(), downloaded));
        }
        self.progress.downloaded = downloaded;
        self.progress.total = total;
        self.step = 1;
    }

    pub fn on_finished(&mut self, version: String, user_file_required: bool, repaired: bool) {
        self.outcome = Some(InstallOutcome {
            version,
            user_file_required,
            repaired,
        });
        self.step = 1;
        self.min_step_until = Some(Instant::now() + FILE_CHECK_MINIMUM);
        if !user_file_required {
            self.file_check = FileCheckUi::Verified {
                label: "Fichiers vérifiés".to_owned(),
                at: Instant::now(),
            };
        }
    }

    pub fn on_failed(&mut self, message: String) {
        self.error = Some(message);
        self.step = 1;
    }

    pub fn on_file_check(&mut self, status: UserFileCheckStatus) {
        match status {
            UserFileCheckStatus::Verified(label) => {
                self.file_check = FileCheckUi::Verified {
                    label,
                    at: Instant::now(),
                };
                self.min_step_until = Some(Instant::now() + FILE_CHECK_MINIMUM);
            }
            UserFileCheckStatus::Rejected(message) => {
                self.file_check = FileCheckUi::Rejected(message);
            }
            UserFileCheckStatus::Cancelled => {
                self.file_check = FileCheckUi::Cancelled;
            }
        }
    }

    fn ready_for_configuration(&self) -> bool {
        let Some(outcome) = &self.outcome else {
            return false;
        };
        if self
            .min_step_until
            .is_some_and(|deadline| Instant::now() < deadline)
        {
            return false;
        }
        if !outcome.user_file_required {
            return true;
        }
        matches!(self.file_check, FileCheckUi::Verified { .. })
    }
}

pub fn show(
    ctx: &egui::Context,
    modal_state: &mut InstallModal,
    store: &Store,
    paths: &AppPaths,
    sender: &UnboundedSender<Command>,
) -> bool {
    if modal_state.step == 1 && modal_state.ready_for_configuration() {
        modal_state.step = 2;
    }
    if modal_state.step > 0 && modal_state.step < 2 {
        if let Some(remaining) = modal_state
            .min_step_until
            .and_then(|deadline| deadline.checked_duration_since(Instant::now()))
        {
            ctx.request_repaint_after(remaining + Duration::from_millis(30));
        }
    }

    let entry = store.entry(&modal_state.slug).cloned();
    let title = entry
        .as_ref()
        .map(|entry| entry.title.clone())
        .unwrap_or_else(|| modal_state.slug.clone());
    let width = 620.0;
    let close = std::cell::Cell::new(false);

    let response = modal::show(
        ctx,
        "install-modal",
        width,
        modal_state,
        |ui, modal_state| {
            let header_title = match modal_state.step {
                0 => format!("Installer {title}"),
                1 => "Vérification des fichiers".to_owned(),
                2 => "Configuration".to_owned(),
                _ => "Installation terminée".to_owned(),
            };
            if modal::header(ui, "INSTALLATION", &header_title) {
                close.set(true);
            }
            modal::stepper(
                ui,
                modal_state.step.min(3),
                &["Version", "Vérification", "Configuration", "Terminé"],
            );

            egui::Frame::new()
                .inner_margin(Margin::symmetric(20, 18))
                .show(ui, |ui| {
                    ui.set_width(width - 40.0);
                    match modal_state.step {
                        0 => step_version(ui, modal_state, entry.as_ref(), paths),
                        1 => step_verification(ui, modal_state, entry.as_ref(), sender),
                        3 => step_done(ui, modal_state, entry.as_ref()),
                        _ => step_configuration(ui, modal_state, entry.as_ref(), paths),
                    }
                });
        },
        |ui, modal_state| {
            modal::footer(ui, |ui| match modal_state.step {
                0 => footer_version(ui, modal_state, sender),
                1 => footer_verification(ui, modal_state, sender),
                3 => footer_done(ui, modal_state, sender, &close),
                _ => footer_configuration(ui, modal_state),
            });
        },
    );

    if response.backdrop_clicked || response.escape {
        close.set(true);
    }
    let close = close.get();
    if close && modal_state.step == 1 && modal_state.error.is_none() {
        let _ = sender.send(Command::CancelInstall {
            slug: modal_state.slug.clone(),
        });
    }
    close || modal_state.step == 4
}

fn footer_version(ui: &mut Ui, modal_state: &mut InstallModal, sender: &UnboundedSender<Command>) {
    if Button::primary("Commencer l'installation")
        .icon(icons::DOWNLOAD)
        .show(ui)
        .clicked()
    {
        let _ = sender.send(Command::Install {
            slug: modal_state.slug.clone(),
        });
        modal_state.started = true;
        modal_state.step = 1;
        modal_state.error = None;
        modal_state.outcome = None;
        modal_state.file_check = FileCheckUi::Idle;
        modal_state.progress = InstallProgress::default();
        modal_state.min_step_until = None;
    }
    if Button::new("Annuler").show(ui).clicked() {
        let _ = sender.send(Command::CancelInstall {
            slug: modal_state.slug.clone(),
        });
        modal_state.step = 4;
    }
}

fn footer_verification(
    ui: &mut Ui,
    modal_state: &mut InstallModal,
    sender: &UnboundedSender<Command>,
) {
    if modal_state.error.is_some() {
        if Button::primary("Réessayer")
            .icon(icons::ROTATE_CW)
            .show(ui)
            .clicked()
        {
            modal_state.error = None;
            modal_state.outcome = None;
            modal_state.progress = InstallProgress::default();
            modal_state.file_check = FileCheckUi::Idle;
            modal_state.min_step_until = None;
            modal_state.step = 0;
        }
    } else if modal_state.outcome.is_none()
        && Button::new("Annuler").icon(icons::X).show(ui).clicked()
    {
        let _ = sender.send(Command::CancelInstall {
            slug: modal_state.slug.clone(),
        });
        modal_state.step = 4;
    }
}

fn footer_configuration(ui: &mut Ui, modal_state: &mut InstallModal) {
    if Button::primary("Terminer")
        .icon(icons::CHECK)
        .show(ui)
        .clicked()
    {
        modal_state.step = 3;
    }
}

fn footer_done(
    ui: &mut Ui,
    modal_state: &mut InstallModal,
    sender: &UnboundedSender<Command>,
    close: &std::cell::Cell<bool>,
) {
    if Button::primary("Jouer")
        .icon(icons::PLAY)
        .show(ui)
        .clicked()
    {
        let _ = sender.send(Command::Play {
            slug: modal_state.slug.clone(),
        });
        close.set(true);
    }
    if Button::new("Fermer").show(ui).clicked() {
        close.set(true);
    }
}

fn step_version(
    ui: &mut Ui,
    modal_state: &mut InstallModal,
    entry: Option<&recomp_core::catalog::model::GameEntry>,
    paths: &AppPaths,
) {
    let tokens = theme::current(ui.ctx());
    text::label(
        ui,
        "Choisir une version",
        13.0,
        Weight::SemiBold,
        tokens.text,
    );
    ui.add_space(4.0);
    text::paragraph(
        ui,
        "La dernière release publiée par le projet sera téléchargée et vérifiée.",
        10.0,
        tokens.text_2,
        17.0,
    );
    ui.add_space(14.0);

    let option = egui::Frame::new()
        .fill(tokens.surface_2)
        .stroke(Stroke::new(1.0_f32, tokens.accent))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(14, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (circle, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                ui.painter().circle_stroke(
                    circle.center(),
                    8.0,
                    Stroke::new(2.0_f32, tokens.accent),
                );
                ui.painter()
                    .circle_filled(circle.center(), 4.0, tokens.accent);
                ui.vertical(|ui| {
                    text::label(ui, "Dernière release", 11.0, Weight::SemiBold, tokens.text);
                    text::label(
                        ui,
                        entry
                            .map(|entry| {
                                format!("{} · {}", entry.project.name, entry.original_platform)
                            })
                            .unwrap_or_else(|| "Version publiée par le projet".to_owned()),
                        9.0,
                        Weight::Regular,
                        tokens.text_2,
                    );
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    components::badge(ui, "Recommandée", BadgeKind::Last);
                });
            });
        });
    let _ = option;

    ui.add_space(14.0);
    text::label(ui, "Emplacement", 9.0, Weight::Bold, tokens.text_3);
    ui.add_space(5.0);
    let install_root = paths.games_dir();
    let preview = install_root.join(&modal_state.slug);
    egui::Frame::new()
        .fill(tokens.surface)
        .stroke(Stroke::new(1.0_f32, tokens.border))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                icons::icon(ui, icons::FOLDER, 15.0, tokens.text_3);
                ui.label(components::mono(
                    format::short_path(&preview, 72),
                    9.0,
                    tokens.text_2,
                ));
            });
        });

    ui.add_space(12.0);
    modal::info_box(
        ui,
        icons::SHIELD_CHECK,
        modal::Tone::Green,
        "Téléchargement officiel",
        "Les fichiers proviennent des releases GitHub du projet et leur hash SHA-256 est vérifié.",
    );

    ui.add_space(4.0);
}

fn step_verification(
    ui: &mut Ui,
    modal_state: &mut InstallModal,
    entry: Option<&recomp_core::catalog::model::GameEntry>,
    sender: &UnboundedSender<Command>,
) {
    let tokens = theme::current(ui.ctx());
    text::label(
        ui,
        "Vérification des fichiers",
        13.0,
        Weight::SemiBold,
        tokens.text,
    );
    ui.add_space(4.0);
    text::paragraph(
        ui,
        "Le launcher télécharge l'archive, vérifie son intégrité puis contrôle le fichier utilisateur si le jeu en exige un.",
        10.0,
        tokens.text_2,
        17.0,
    );
    ui.add_space(14.0);

    let archive_done = modal_state.outcome.is_some();
    verify_row(
        ui,
        icons::ARCHIVE,
        "Archive et binaires",
        if archive_done {
            "Vérifié".to_owned()
        } else if let Some(total) = modal_state.progress.total {
            format!(
                "{} / {} · {}",
                format::bytes(modal_state.progress.downloaded),
                format::bytes(total),
                format::speed(modal_state.speed_bps)
            )
        } else if modal_state.progress.downloaded > 0 {
            format!(
                "{} · {}",
                format::bytes(modal_state.progress.downloaded),
                format::speed(modal_state.speed_bps)
            )
        } else {
            "En attente du téléchargement…".to_owned()
        },
        if archive_done {
            RowState::Done
        } else {
            RowState::Active
        },
    );
    if let Some(fraction) = modal_state.progress.fraction() {
        ui.add_space(6.0);
        components::progress_bar(ui, fraction, 6.0);
    } else if !archive_done && modal_state.progress.downloaded > 0 {
        ui.add_space(6.0);
        components::progress_bar(ui, 0.0, 6.0);
    }

    if let Some(entry) = entry {
        if entry.user_files.is_some() {
            ui.add_space(10.0);
            let (description, state) = match &modal_state.file_check {
                FileCheckUi::Idle => (
                    "Sélectionnez votre dump pour terminer l'installation.".to_owned(),
                    RowState::Pending,
                ),
                FileCheckUi::Verified { label, .. } => (label.clone(), RowState::Done),
                FileCheckUi::Rejected(message) => (message.clone(), RowState::Error),
                FileCheckUi::Cancelled => ("Sélection annulée.".to_owned(), RowState::Pending),
            };
            verify_row(
                ui,
                icons::FILE_SEARCH,
                "Fichier utilisateur",
                description,
                state,
            );
            ui.add_space(8.0);
            let verified = matches!(modal_state.file_check, FileCheckUi::Verified { .. });
            if !verified
                && modal_state.outcome.is_some()
                && Button::new("Sélectionner le fichier")
                    .icon(icons::FILE_QUESTION)
                    .show(ui)
                    .clicked()
            {
                let _ = sender.send(Command::SelectUserFile {
                    slug: modal_state.slug.clone(),
                });
            }
            if let FileCheckUi::Verified { at, .. } = modal_state.file_check {
                let remaining = FILE_CHECK_MINIMUM
                    .checked_sub(at.elapsed())
                    .unwrap_or(Duration::ZERO);
                if !remaining.is_zero() {
                    ui.ctx().request_repaint_after(remaining);
                }
            }
        }
    }

    if let Some(error) = &modal_state.error {
        ui.add_space(12.0);
        modal::info_box(
            ui,
            icons::CIRCLE_ALERT,
            modal::Tone::Red,
            "L'installation a échoué",
            error,
        );
    }

    ui.add_space(4.0);
}

#[derive(Clone, Copy)]
enum RowState {
    Pending,
    Active,
    Done,
    Error,
}

fn verify_row(ui: &mut Ui, glyph: char, label: &str, description: String, state: RowState) {
    let tokens = theme::current(ui.ctx());
    egui::Frame::new()
        .fill(tokens.surface_2)
        .stroke(Stroke::new(1.0_f32, tokens.border))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                match state {
                    RowState::Active => {
                        components::spinner(ui, 16.0, tokens.accent);
                    }
                    RowState::Done => {
                        icons::icon(ui, icons::CHECK, 16.0, tokens.green);
                    }
                    RowState::Error => {
                        icons::icon(ui, icons::X_CIRCLE, 16.0, tokens.red);
                    }
                    RowState::Pending => {
                        icons::icon(ui, glyph, 16.0, tokens.text_3);
                    }
                }
                ui.vertical(|ui| {
                    text::label(ui, label, 10.0, Weight::SemiBold, tokens.text);
                    text::paragraph(
                        ui,
                        &description,
                        9.0,
                        match state {
                            RowState::Error => tokens.red,
                            RowState::Done => tokens.green,
                            _ => tokens.text_2,
                        },
                        13.0,
                    );
                });
            });
        });
}

fn step_configuration(
    ui: &mut Ui,
    modal_state: &mut InstallModal,
    entry: Option<&recomp_core::catalog::model::GameEntry>,
    paths: &AppPaths,
) {
    let tokens = theme::current(ui.ctx());
    text::label(ui, "Configuration", 13.0, Weight::SemiBold, tokens.text);
    ui.add_space(4.0);
    text::paragraph(
        ui,
        "Votre installation est prête. Voici les paramètres réels enregistrés par le launcher.",
        10.0,
        tokens.text_2,
        17.0,
    );
    ui.add_space(14.0);

    if let Some(outcome) = &modal_state.outcome {
        configuration_row(ui, "Version", &outcome.version);
    }
    configuration_row(
        ui,
        "Jeu",
        entry.map(|entry| entry.title.as_str()).unwrap_or("—"),
    );
    let saves = paths.userdata_dir(&modal_state.slug);
    configuration_row(ui, "Sauvegardes", &format::short_path(&saves, 56));
    let mods = entry
        .map(|entry| {
            if entry.mods.supported {
                format!("Dossier {}", entry.mods.dir_name)
            } else {
                "Non pris en charge".to_owned()
            }
        })
        .unwrap_or_else(|| "—".to_owned());
    configuration_row(ui, "Mods", &mods);
    if let Some(entry) = entry {
        configuration_row(
            ui,
            "Exécutable",
            entry
                .launch
                .exe_for(recomp_core::catalog::model::Os::current())
                .unwrap_or("—"),
        );
    }

    ui.add_space(12.0);
    modal::info_box(
        ui,
        icons::SHIELD_CHECK,
        modal::Tone::Green,
        "Installation vérifiée",
        "L'archive téléchargée et les fichiers contrôlés correspondent aux empreintes publiées.",
    );

    ui.add_space(4.0);
}

fn step_done(
    ui: &mut Ui,
    modal_state: &mut InstallModal,
    entry: Option<&recomp_core::catalog::model::GameEntry>,
) {
    let tokens = theme::current(ui.ctx());
    ui.add_space(6.0);
    ui.vertical_centered(|ui| {
        let (circle, _) = ui.allocate_exact_size(Vec2::splat(56.0), Sense::hover());
        ui.painter()
            .circle_filled(circle.center(), 28.0, tokens.green_soft);
        icons::paint(
            ui.painter(),
            circle.center(),
            icons::CHECK,
            28.0,
            tokens.green,
        );
        ui.add_space(12.0);
        text::label(
            ui,
            if modal_state
                .outcome
                .as_ref()
                .is_some_and(|outcome| outcome.repaired)
            {
                "Réparation terminée"
            } else {
                "Installation terminée"
            },
            16.0,
            Weight::Bold,
            tokens.text,
        );
        ui.add_space(5.0);
        text::label(
            ui,
            format!(
                "{} est prêt à être lancé.",
                entry.map(|entry| entry.title.as_str()).unwrap_or("Le jeu")
            ),
            10.0,
            Weight::Regular,
            tokens.text_2,
        );
    });
    ui.add_space(16.0);
    if let Some(outcome) = &modal_state.outcome {
        configuration_row(ui, "Version installée", &outcome.version);
    }
    if let Some(entry) = entry {
        if entry.user_files.is_some() {
            configuration_row(
                ui,
                "Fichier utilisateur",
                match &modal_state.file_check {
                    FileCheckUi::Verified { label, .. } => label,
                    _ => "Non requis",
                },
            );
        }
    }

    ui.add_space(6.0);
}

fn configuration_row(ui: &mut Ui, label: &str, value: &str) {
    let tokens = theme::current(ui.ctx());
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        text::label(ui, label, 9.0, Weight::Regular, tokens.text_3);
        let max_width = (ui.available_width() - 8.0).max(80.0);
        let value = text::ellipsize(ui.ctx(), value, text::font(9.0, Weight::Medium), max_width);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            text::label(ui, value, 9.0, Weight::Medium, tokens.text);
        });
    });
    ui.add_space(7.0);
}
