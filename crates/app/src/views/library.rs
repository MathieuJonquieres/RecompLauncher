//! Bibliothèque: installed games, real playtime and disk usage.

use eframe::egui::{
    self, Align, Color32, CornerRadius, Layout, Margin, Pos2, Rect, ScrollArea, Sense, Stroke,
    StrokeKind, Ui, Vec2,
};
use recomp_core::library::LibraryGame;

use crate::ui::components::{self, BadgeKind, Button};
use crate::ui::text::Weight;
use crate::ui::{art, format, icons, text, theme};

use super::{Action, InstallProgress, Store};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LibrarySort {
    Recent,
    Title,
    Playtime,
}

pub struct LibraryUi {
    pub sort: LibrarySort,
}

impl Default for LibraryUi {
    fn default() -> Self {
        Self {
            sort: LibrarySort::Recent,
        }
    }
}

pub fn show(ui: &mut Ui, store: &mut Store, state: &mut LibraryUi) -> Action {
    let tokens = theme::current(ui.ctx());
    let mut action = Action::None;

    components::page_header(
        ui,
        "Bibliothèque",
        "Retrouvez vos jeux installés, vos temps de jeu et vos sauvegardes.",
        |ui| {
            if Button::new("Parcourir le catalogue")
                .icon(icons::GRID_2X2)
                .show(ui)
                .clicked()
            {
                action = Action::BrowseCatalog;
            }
        },
    );

    stats_row(ui, store);
    ui.add_space(22.0);

    let mut games: Vec<&LibraryGame> = store.library.iter().collect();
    match state.sort {
        LibrarySort::Recent => games.sort_by(|a, b| {
            b.last_played
                .unwrap_or(b.installed_at)
                .cmp(&a.last_played.unwrap_or(a.installed_at))
                .then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase()))
        }),
        LibrarySort::Title => games.sort_by_key(|a| a.title.to_lowercase()),
        LibrarySort::Playtime => games.sort_by_key(|a| std::cmp::Reverse(a.playtime_s)),
    }

    ui.horizontal(|ui| {
        text::label(
            ui,
            format!(
                "{} {}",
                games.len(),
                if games.len() == 1 {
                    "jeu installé"
                } else {
                    "jeux installés"
                }
            ),
            10.0,
            Weight::Medium,
            tokens.text_2,
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let options = vec![
                (LibrarySort::Recent, "Récents".to_owned()),
                (LibrarySort::Title, "Titre".to_owned()),
                (LibrarySort::Playtime, "Temps de jeu".to_owned()),
            ];
            components::select(ui, "library-sort", 120.0, &mut state.sort, &options, None);
            text::label(ui, "Trier par", 9.0, Weight::Regular, tokens.text_3);
        });
    });
    ui.add_space(12.0);

    if games.is_empty() {
        empty_library(ui, &tokens, &mut action);
        return action;
    }

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for game in games {
                if let Some(next) = row(ui, store, game) {
                    action = next;
                }
                ui.add_space(8.0);
            }
        });

    action
}

fn stats_row(ui: &mut Ui, store: &Store) {
    let gap = 16.0;
    let width = ((ui.available_width() - gap * 2.0) / 3.0).floor();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        stat_card(
            ui,
            width,
            icons::LIBRARY,
            &store.library.len().to_string(),
            "Jeux installés",
            "Dans votre bibliothèque",
        );
        stat_card(
            ui,
            width,
            icons::CLOCK_3,
            &format::duration(store.total_playtime()),
            "Temps de jeu",
            "Temps total enregistré",
        );
        stat_card(
            ui,
            width,
            icons::HARD_DRIVE,
            &format::bytes(store.stats.games),
            "Espace disque",
            "Installations gérées",
        );
    });
}

fn stat_card(ui: &mut Ui, width: f32, glyph: char, value: &str, label: &str, hint: &str) {
    let tokens = theme::current(ui.ctx());
    egui::Frame::new()
        .fill(tokens.surface)
        .stroke(Stroke::new(1.0_f32, tokens.border))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(Margin::symmetric(16, 14))
        .show(ui, |ui| {
            ui.set_width((width - 32.0).max(120.0));
            ui.horizontal(|ui| {
                let (icon_rect, _) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::hover());
                ui.painter()
                    .rect_filled(icon_rect, CornerRadius::same(9), tokens.accent_soft);
                icons::paint(ui.painter(), icon_rect.center(), glyph, 17.0, tokens.accent);
                ui.add_space(2.0);
                ui.vertical(|ui| {
                    text::label(ui, value, 18.0, Weight::Bold, tokens.text);
                    text::label(ui, label, 10.0, Weight::Medium, tokens.text_2);
                });
            });
            ui.add_space(8.0);
            text::label(ui, hint, 9.0, Weight::Regular, tokens.text_3);
        });
}

fn row(ui: &mut Ui, store: &Store, game: &LibraryGame) -> Option<Action> {
    let tokens = theme::current(ui.ctx());
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 94.0), Sense::click());
    let hovered = response.hovered();
    let entry = store.entry(&game.slug);
    let progress = store.playing(&game.slug);
    let playing = progress.is_some();
    let needs_file = entry
        .and_then(|entry| entry.user_files.as_ref())
        .is_some_and(|user_files| user_files.required)
        && game.user_file.is_none();

    let play_width = 92.0;
    let play_rect = Rect::from_center_size(
        egui::pos2(rect.right() - 62.0, rect.center().y),
        Vec2::new(play_width, 34.0),
    );
    let trash_rect = Rect::from_center_size(
        egui::pos2(rect.right() - 128.0, rect.center().y),
        Vec2::splat(30.0),
    );
    let play_response = ui.interact(
        play_rect,
        ui.id().with(("library-play", &game.slug)),
        Sense::click(),
    );
    let trash_response = ui.interact(
        trash_rect,
        ui.id().with(("library-trash", &game.slug)),
        Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(12),
            if hovered {
                tokens.surface_2
            } else {
                tokens.surface
            },
        );
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(12),
            Stroke::new(
                1.0_f32,
                if hovered {
                    tokens.border_strong
                } else {
                    tokens.border
                },
            ),
            StrokeKind::Inside,
        );
        let thumb = Rect::from_min_size(
            egui::pos2(rect.left() + 14.0, rect.top() + 14.0),
            Vec2::new(118.0, 66.0),
        );
        art::paint(ui, thumb, art::palette_for(&game.slug), 9);
        let initial = art::initial_for(&game.slug, &game.title);
        art::paint_initial(
            ui,
            thumb,
            art::Initial {
                text: &initial,
                size: 34.0,
                opacity: 0.86,
                offset: Vec2::ZERO,
            },
        );

        let text_left = rect.left() + 146.0;
        let text_max = (rect.right() - 292.0 - text_left).max(140.0);
        let title = text::ellipsize(
            ui.ctx(),
            &game.title,
            text::font(14.0, Weight::SemiBold),
            text_max,
        );
        paint(
            ui,
            egui::pos2(text_left, rect.top() + 20.0),
            &title,
            text::font(14.0, Weight::SemiBold),
            tokens.text,
        );
        let version = game.version.as_deref().unwrap_or("version inconnue");
        let meta = format!("{version} · {}", game.source.label());
        let meta = text::ellipsize(ui.ctx(), &meta, text::font(10.0, Weight::Regular), text_max);
        paint(
            ui,
            egui::pos2(text_left, rect.top() + 41.0),
            &meta,
            text::font(10.0, Weight::Regular),
            tokens.text_2,
        );

        let mut badge_x = text_left;
        if needs_file {
            paint_badge(
                ui,
                egui::pos2(badge_x, rect.top() + 59.0),
                "Fichier requis",
                BadgeKind::Beta,
            );
            badge_x += 92.0;
        } else {
            paint_badge(
                ui,
                egui::pos2(badge_x, rect.top() + 59.0),
                "Prêt",
                BadgeKind::Playable,
            );
            badge_x += 52.0;
        }
        if let Some(size) = store.stats.per_game.get(&game.slug) {
            paint(
                ui,
                egui::pos2(badge_x, rect.top() + 63.0),
                &format::bytes(*size),
                text::font(9.0, Weight::Medium),
                tokens.text_3,
            );
        }

        let right_text_x = rect.right() - 280.0;
        paint(
            ui,
            egui::pos2(right_text_x, rect.top() + 25.0),
            &format::duration(game.playtime_s),
            text::font(12.0, Weight::SemiBold),
            tokens.text,
        );
        paint(
            ui,
            egui::pos2(right_text_x, rect.top() + 42.0),
            &match game.last_played {
                Some(timestamp) => format!(
                    "Joué {}",
                    format::relative_day(timestamp, format::now()).to_lowercase()
                ),
                None => "Jamais lancé".to_owned(),
            },
            text::font(9.0, Weight::Regular),
            tokens.text_3,
        );

        if playing {
            components::paint_spinner(ui, play_rect.center(), 9.0, tokens.accent);
        } else {
            let (fill, stroke, color, label) = if needs_file {
                (
                    tokens.orange_soft,
                    Some(tokens.orange),
                    tokens.orange,
                    "Configurer",
                )
            } else {
                (tokens.accent, None, Color32::WHITE, "Jouer")
            };
            ui.painter()
                .rect_filled(play_rect, CornerRadius::same(9), fill);
            if let Some(stroke) = stroke {
                ui.painter().rect_stroke(
                    play_rect,
                    CornerRadius::same(9),
                    Stroke::new(1.0_f32, stroke),
                    StrokeKind::Inside,
                );
            }
            let galley = ui.painter().layout_no_wrap(
                label.to_owned(),
                text::font(10.0, Weight::SemiBold),
                color,
            );
            ui.painter().galley(
                egui::pos2(
                    play_rect.center().x - galley.size().x / 2.0,
                    play_rect.center().y - galley.size().y / 2.0,
                ),
                galley,
                color,
            );
        }

        let trash_color = if trash_response.hovered() {
            tokens.red
        } else {
            tokens.text_3
        };
        if trash_response.hovered() {
            ui.painter()
                .rect_filled(trash_rect, CornerRadius::same(8), tokens.red_soft);
        }
        icons::paint(
            ui.painter(),
            trash_rect.center(),
            icons::TRASH_2,
            16.0,
            trash_color,
        );

        if let Some(progress) = progress {
            let bar = Rect::from_min_max(
                egui::pos2(text_left, rect.bottom() - 9.0),
                egui::pos2(rect.right() - 152.0, rect.bottom() - 5.0),
            );
            ui.painter()
                .rect_filled(bar, CornerRadius::same(2), tokens.surface_3);
            let fraction = progress_fraction(progress);
            let filled =
                Rect::from_min_size(bar.min, Vec2::new(bar.width() * fraction, bar.height()));
            ui.painter()
                .rect_filled(filled, CornerRadius::same(2), tokens.accent);
        }
    }

    if trash_response.clicked() {
        return Some(Action::Uninstall(game.slug.clone()));
    }
    if play_response.clicked() {
        return Some(if playing {
            Action::Install(game.slug.clone())
        } else if needs_file {
            Action::SelectUserFile(game.slug.clone())
        } else {
            Action::Play(game.slug.clone())
        });
    }
    if response.clicked() {
        return Some(Action::OpenGame {
            slug: game.slug.clone(),
            from_library: true,
        });
    }
    None
}

fn progress_fraction(progress: InstallProgress) -> f32 {
    progress.fraction().unwrap_or(0.0)
}

fn empty_library(ui: &mut Ui, tokens: &theme::Tokens, action: &mut Action) {
    ui.add_space(60.0);
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(52.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(14), tokens.accent_soft);
        icons::paint(
            ui.painter(),
            rect.center(),
            icons::LIBRARY,
            24.0,
            tokens.accent,
        );
        ui.add_space(14.0);
        text::label(
            ui,
            "Votre bibliothèque est vide.",
            15.0,
            Weight::SemiBold,
            tokens.text,
        );
        ui.add_space(6.0);
        text::label(
            ui,
            "Installez un jeu depuis le catalogue pour le retrouver ici.",
            10.0,
            Weight::Regular,
            tokens.text_2,
        );
        ui.add_space(16.0);
        if Button::primary("Parcourir le catalogue")
            .icon(icons::GRID_2X2)
            .show(ui)
            .clicked()
        {
            *action = Action::BrowseCatalog;
        }
    });
}

fn paint(ui: &Ui, position: Pos2, value: &str, font: egui::FontId, color: Color32) {
    let galley = ui.painter().layout_no_wrap(value.to_owned(), font, color);
    ui.painter().galley(position, galley, color);
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
