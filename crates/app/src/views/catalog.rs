//! Catalogue: filters, grid/list results and the contextual detail panel.

use std::time::Instant;

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Layout, Margin, Pos2, Rect, ScrollArea, Sense,
    Stroke, StrokeKind, Ui, Vec2,
};
use recomp_core::catalog::filter::CatalogFilter;
use recomp_core::catalog::model::{GameEntry, GameKind, GameStatus, Os};

use crate::ui::components::{self, BadgeKind, Button};
use crate::ui::text::Weight;
use crate::ui::{art, format, icons, text, theme};

use super::{remaining_until, Action, Store, REFRESH_SKELETON};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Grid,
    List,
}

pub struct CatalogUi {
    pub filter: CatalogFilter,
    pub view: ViewMode,
    pub selected: Option<String>,
    pub skeleton_until: Option<Instant>,
}

impl Default for CatalogUi {
    fn default() -> Self {
        Self {
            filter: CatalogFilter::default(),
            view: ViewMode::Grid,
            selected: None,
            skeleton_until: None,
        }
    }
}

impl CatalogUi {
    pub fn begin_refresh(&mut self) {
        self.skeleton_until = Some(Instant::now() + REFRESH_SKELETON);
    }

    pub fn skeleton_active(&self) -> bool {
        remaining_until(self.skeleton_until).is_some()
    }
}

pub fn show(ui: &mut Ui, store: &mut Store, state: &mut CatalogUi, compact: bool) -> Action {
    let tokens = theme::current(ui.ctx());
    let mut action = Action::None;

    if state.skeleton_active() || store.refreshing {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(60));
    }

    components::page_header(
        ui,
        "Catalogue",
        "Découvrez et installez des jeux recompilés, décompilés ou portés nativement.",
        |ui| {
            if Button::new("Actualiser")
                .icon(icons::REFRESH_CW)
                .show(ui)
                .clicked()
            {
                action = Action::RefreshCatalog;
            }
        },
    );

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        let search_width = if compact { 238.0 } else { 320.0 };
        if components::search_field(
            ui,
            &mut state.filter.search,
            "Rechercher un jeu…",
            search_width,
        )
        .changed()
        {
            state.selected = None;
        }

        let platforms = CatalogFilter::platforms(&store.entries);
        let mut platform_options = vec![(None, "Plateforme".to_owned())];
        platform_options.extend(
            platforms
                .into_iter()
                .map(|platform| (Some(platform.clone()), platform)),
        );
        components::select(
            ui,
            "filter-platform",
            if compact { 112.0 } else { 126.0 },
            &mut state.filter.platform,
            &platform_options,
            None,
        );

        let kinds = vec![
            (None, "Type".to_owned()),
            (Some(GameKind::Recomp), "Recomp".to_owned()),
            (Some(GameKind::Decomp), "Décomp".to_owned()),
            (Some(GameKind::NativePort), "Port natif".to_owned()),
        ];
        components::select(
            ui,
            "filter-kind",
            if compact { 100.0 } else { 112.0 },
            &mut state.filter.kind,
            &kinds,
            None,
        );

        let statuses = vec![
            (None, "Statut".to_owned()),
            (Some(GameStatus::Playable), "Jouable".to_owned()),
            (Some(GameStatus::Beta), "Bêta".to_owned()),
            (Some(GameStatus::Wip), "WIP".to_owned()),
        ];
        components::select(
            ui,
            "filter-status",
            if compact { 100.0 } else { 112.0 },
            &mut state.filter.status,
            &statuses,
            None,
        );

        let systems = vec![
            (None, "Système".to_owned()),
            (Some(Os::Windows), "Windows".to_owned()),
            (Some(Os::Linux), "Linux".to_owned()),
            (Some(Os::Macos), "macOS".to_owned()),
        ];
        components::select(
            ui,
            "filter-os",
            if compact { 106.0 } else { 118.0 },
            &mut state.filter.os,
            &systems,
            Some(icons::MONITOR),
        );

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            view_toggle(ui, &mut state.view);
        });
    });

    ui.add_space(14.0);

    let filtered = state.filter.apply(&store.entries);
    if let Some(selected) = &state.selected {
        if !filtered
            .iter()
            .any(|entry| entry.slug.as_str() == selected.as_str())
        {
            state.selected = None;
        }
    }

    ui.horizontal(|ui| {
        let count = filtered.len();
        text::label(
            ui,
            format!("{count} {}", if count == 1 { "jeu" } else { "jeux" }),
            10.0,
            Weight::Medium,
            tokens.text_2,
        );
        if store.offline {
            components::badge(ui, "Hors ligne", BadgeKind::Beta);
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let updated = store
                .last_updated
                .map(format::date)
                .unwrap_or_else(|| "jamais".to_owned());
            text::label(
                ui,
                format!("Dernière mise à jour : {updated}"),
                9.0,
                Weight::Regular,
                tokens.text_3,
            );
        });
    });

    ui.add_space(14.0);

    if state.skeleton_active() || store.refreshing {
        show_skeleton(ui, state, compact);
        return action;
    }

    if filtered.is_empty() {
        empty_state(ui, &tokens);
        return action;
    }

    let has_panel = state.selected.is_some();
    let panel_width = if compact { 310.0 } else { 360.0 };
    let gap = 20.0;
    let available = ui.available_size();

    if has_panel {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            let left_width = (available.x - panel_width - gap).max(320.0);
            let mut left_action = Action::None;
            ui.allocate_ui_with_layout(
                Vec2::new(left_width, available.y),
                Layout::top_down(Align::Min),
                |ui| {
                    left_action = show_results(ui, store, state, &filtered, left_width, compact);
                },
            );
            if left_action != Action::None {
                action = left_action;
            }
            let mut panel_action = Action::None;
            ui.allocate_ui_with_layout(
                Vec2::new(panel_width, available.y),
                Layout::top_down(Align::Min),
                |ui| {
                    if let Some(slug) = state.selected.clone() {
                        panel_action = detail_panel(ui, store, &slug, panel_width);
                    }
                },
            );
            if panel_action != Action::None {
                action = panel_action;
            }
        });
    } else {
        action = show_results(ui, store, state, &filtered, available.x, compact);
    }

    action
}

fn show_results(
    ui: &mut Ui,
    store: &Store,
    state: &mut CatalogUi,
    filtered: &[&GameEntry],
    width: f32,
    compact: bool,
) -> Action {
    let mut action = Action::None;
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(width);
            match state.view {
                ViewMode::Grid => {
                    let columns = if width >= 720.0 {
                        3
                    } else if width >= 430.0 {
                        2
                    } else {
                        1
                    };
                    let gap = 16.0;
                    let card_width =
                        ((width - gap * (columns as f32 - 1.0)) / columns as f32).floor();
                    let card_height = if compact { 224.0 } else { 232.0 };
                    for chunk in filtered.chunks(columns) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = gap;
                            for entry in chunk {
                                if let Some(next) = card(ui, store, entry, card_width, card_height)
                                {
                                    action = next;
                                }
                            }
                        });
                        ui.add_space(gap);
                    }
                }
                ViewMode::List => {
                    for entry in filtered {
                        if let Some(next) = list_row(ui, store, entry) {
                            action = next;
                        }
                        ui.add_space(8.0);
                    }
                }
            }
        });
    if action == Action::None {
        if let Some(clicked) = pending_card_click(ui) {
            state.selected = Some(clicked);
        }
    }
    action
}

thread_local! {
    static CARD_CLICK: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

fn pending_card_click(ui: &Ui) -> Option<String> {
    let clicked = CARD_CLICK.with(|cell| cell.borrow_mut().take());
    if clicked.is_some() {
        ui.ctx().request_repaint();
    }
    clicked
}

fn request_open(slug: &str) {
    CARD_CLICK.with(|cell| *cell.borrow_mut() = Some(slug.to_owned()));
}

fn card(ui: &mut Ui, store: &Store, entry: &GameEntry, width: f32, height: f32) -> Option<Action> {
    let tokens = theme::current(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let unavailable = !entry.supports_os(Os::current());
    let hovered = response.hovered() && !unavailable;
    let installed = store.is_installed(entry.slug.as_str());
    let needs_file = store.needs_user_file(entry.slug.as_str());
    let progress = store.playing(entry.slug.as_str());
    let slug = entry.slug.to_string();

    let action_rect = Rect::from_center_size(
        egui::pos2(rect.right() - 29.0, rect.bottom() - 31.0),
        Vec2::splat(30.0),
    );
    let action_response = ui.interact(
        action_rect,
        ui.id().with(("catalog-card-action", &slug)),
        Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        if hovered {
            ui.painter()
                .add(tokens.card_shadow.as_shape(rect, CornerRadius::same(12)));
        }
        ui.painter()
            .rect_filled(rect, CornerRadius::same(12), tokens.surface);
        let art_height = 132.0;
        let art_rect = Rect::from_min_max(
            rect.min,
            egui::pos2(rect.right(), rect.top() + art_height + 12.0),
        );
        art::paint(ui, art_rect, art::palette_for(&slug), 12);
        let initial = art::initial_for(&slug, &entry.title);
        art::paint_initial(
            ui,
            Rect::from_min_max(rect.min, egui::pos2(rect.right(), rect.top() + art_height)),
            art::Initial {
                text: &initial,
                size: if width < 270.0 { 38.0 } else { 46.0 },
                opacity: 0.88,
                offset: Vec2::ZERO,
            },
        );

        let body = Rect::from_min_max(egui::pos2(rect.left(), rect.top() + art_height), rect.max);
        ui.painter().rect_filled(
            body,
            CornerRadius {
                nw: 0,
                ne: 0,
                sw: 12,
                se: 12,
            },
            tokens.surface,
        );
        if unavailable {
            // Dim the artwork and the card body, but keep labels legible.
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(12),
                tokens.app_bg.linear_multiply(0.42),
            );
        }

        let badge_y = rect.top() + 10.0;
        let mut badge_x = rect.left() + 10.0;
        let status = match entry.status {
            GameStatus::Playable => BadgeKind::Playable,
            GameStatus::Beta => BadgeKind::Beta,
            GameStatus::Wip => BadgeKind::Wip,
            _ => BadgeKind::Neutral,
        };
        let label = entry.status.label();
        let badge_width = badge_width(ui, label);
        paint_badge(ui, egui::pos2(badge_x, badge_y), label, status);
        badge_x += badge_width + 6.0;
        if needs_file {
            paint_badge(
                ui,
                egui::pos2(badge_x, badge_y),
                "Fichier requis",
                BadgeKind::Beta,
            );
        } else if installed {
            paint_badge(
                ui,
                egui::pos2(badge_x, badge_y),
                "Installé",
                BadgeKind::Recomp,
            );
        } else if unavailable {
            paint_badge(
                ui,
                egui::pos2(badge_x, badge_y),
                "Indisponible",
                BadgeKind::Neutral,
            );
        }

        let title_max = width - 28.0;
        let title = ellipsize(
            ui.ctx(),
            &entry.title,
            text::font(12.0, Weight::SemiBold),
            title_max,
        );
        let title_color = if unavailable {
            tokens.text_2.linear_multiply(0.8)
        } else {
            tokens.text
        };
        paint_text(
            ui,
            egui::pos2(rect.left() + 14.0, body.top() + 13.0),
            &title,
            text::font(12.0, Weight::SemiBold),
            title_color,
        );
        let meta = format!("{} · {}", entry.original_platform, entry.kind.label());
        let meta = ellipsize(
            ui.ctx(),
            &meta,
            text::font(10.0, Weight::Regular),
            width - 60.0,
        );
        paint_text(
            ui,
            egui::pos2(rect.left() + 14.0, body.top() + 33.0),
            &meta,
            text::font(10.0, Weight::Regular),
            tokens.text_2,
        );

        let action_center = action_rect.center();
        if let Some(progress) = progress {
            let fraction = progress.fraction().unwrap_or(0.0);
            ui.painter()
                .circle_filled(action_center, 15.0, tokens.accent_soft);
            components::paint_spinner(ui, action_center, 7.0, tokens.accent);
            let bar = Rect::from_min_max(
                egui::pos2(rect.left() + 14.0, rect.bottom() - 16.0),
                egui::pos2(rect.right() - 14.0, rect.bottom() - 11.0),
            );
            ui.painter()
                .rect_filled(bar, CornerRadius::same(3), tokens.surface_3);
            let filled = Rect::from_min_size(
                bar.min,
                Vec2::new((bar.width() * fraction).max(0.0), bar.height()),
            );
            ui.painter()
                .rect_filled(filled, CornerRadius::same(3), tokens.accent);
        } else {
            let (fill, icon_color, glyph) = if unavailable {
                (tokens.surface_3, tokens.text_3, icons::LOCK_KEYHOLE)
            } else if needs_file {
                (tokens.orange_soft, tokens.orange, icons::FILE_QUESTION)
            } else if installed {
                (tokens.green_soft, tokens.green, icons::PLAY)
            } else {
                (tokens.accent, Color32::WHITE, icons::DOWNLOAD)
            };
            ui.painter().circle_filled(action_center, 15.0, fill);
            icons::paint(
                ui.painter(),
                action_center,
                glyph,
                if installed { 14.0 } else { 15.0 },
                icon_color,
            );
        }

        if hovered {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(12),
                Stroke::new(1.0_f32, tokens.border_strong),
                StrokeKind::Inside,
            );
        } else {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(12),
                Stroke::new(1.0_f32, tokens.border),
                StrokeKind::Inside,
            );
        }
    }

    if action_response.clicked() && !unavailable {
        return Some(if progress.is_some() {
            Action::Install(slug)
        } else if needs_file {
            Action::SelectUserFile(slug)
        } else if installed {
            Action::Play(slug)
        } else {
            Action::Install(slug)
        });
    }
    if response.clicked() {
        request_open(&slug);
    }
    None
}

fn list_row(ui: &mut Ui, store: &Store, entry: &GameEntry) -> Option<Action> {
    let tokens = theme::current(ui.ctx());
    let width = ui.available_width();
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 74.0), Sense::click());
    let unavailable = !entry.supports_os(Os::current());
    let hovered = response.hovered() && !unavailable;
    let installed = store.is_installed(entry.slug.as_str());
    let needs_file = store.needs_user_file(entry.slug.as_str());
    let slug = entry.slug.to_string();
    let action_rect = Rect::from_center_size(
        egui::pos2(rect.right() - 34.0, rect.center().y),
        Vec2::splat(30.0),
    );
    let action_response = ui.interact(
        action_rect,
        ui.id().with(("catalog-row-action", &slug)),
        Sense::click(),
    );

    if ui.is_rect_visible(rect) {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(10),
            if hovered {
                tokens.surface_2
            } else {
                tokens.surface
            },
        );
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(10),
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
        if unavailable {
            // Dim the thumbnail and row body; keep the labels legible.
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(10),
                tokens.app_bg.linear_multiply(0.42),
            );
        }
        let thumb = Rect::from_min_size(
            egui::pos2(rect.left() + 10.0, rect.center().y - 24.0),
            Vec2::new(86.0, 48.0),
        );
        art::paint(ui, thumb, art::palette_for(&slug), 8);
        let initial = art::initial_for(&slug, &entry.title);
        art::paint_initial(
            ui,
            thumb,
            art::Initial {
                text: &initial,
                size: 26.0,
                opacity: 0.85,
                offset: Vec2::ZERO,
            },
        );
        let title = ellipsize(
            ui.ctx(),
            &entry.title,
            text::font(12.0, Weight::SemiBold),
            (width - 190.0).max(120.0),
        );
        paint_text(
            ui,
            egui::pos2(rect.left() + 112.0, rect.center().y - 16.0),
            &title,
            text::font(12.0, Weight::SemiBold),
            if unavailable {
                tokens.text_2
            } else {
                tokens.text
            },
        );
        let meta = if let Some(game) = store.library_game(&slug) {
            format!(
                "{} · {} · {}",
                entry.original_platform,
                game.version.as_deref().unwrap_or("version inconnue"),
                game.source.label()
            )
        } else {
            format!("{} · {}", entry.original_platform, entry.kind.label())
        };
        let meta = ellipsize(
            ui.ctx(),
            &meta,
            text::font(10.0, Weight::Regular),
            (width - 190.0).max(120.0),
        );
        paint_text(
            ui,
            egui::pos2(rect.left() + 112.0, rect.center().y + 2.0),
            &meta,
            text::font(10.0, Weight::Regular),
            tokens.text_2,
        );
        let status = match entry.status {
            GameStatus::Playable => BadgeKind::Playable,
            GameStatus::Beta => BadgeKind::Beta,
            GameStatus::Wip => BadgeKind::Wip,
            _ => BadgeKind::Neutral,
        };
        let status_label = entry.status.label();
        let status_width = badge_width(ui, status_label);
        paint_badge(
            ui,
            egui::pos2(rect.left() + 112.0, rect.center().y + 19.0),
            status_label,
            status,
        );
        let secondary_x = rect.left() + 112.0 + status_width + 6.0;
        if needs_file {
            paint_badge(
                ui,
                egui::pos2(secondary_x, rect.center().y + 19.0),
                "Fichier requis",
                BadgeKind::Beta,
            );
        } else if installed {
            paint_badge(
                ui,
                egui::pos2(secondary_x, rect.center().y + 19.0),
                "Installé",
                BadgeKind::Recomp,
            );
        }

        let (fill, glyph, color) = if unavailable {
            (tokens.surface_3, icons::LOCK_KEYHOLE, tokens.text_3)
        } else if needs_file {
            (tokens.orange_soft, icons::FILE_QUESTION, tokens.orange)
        } else if installed {
            (tokens.green_soft, icons::PLAY, tokens.green)
        } else {
            (tokens.accent_soft, icons::DOWNLOAD, tokens.accent)
        };
        ui.painter().circle_filled(action_rect.center(), 15.0, fill);
        icons::paint(ui.painter(), action_rect.center(), glyph, 15.0, color);
    }

    if action_response.clicked() && !unavailable {
        return Some(if needs_file {
            Action::SelectUserFile(slug)
        } else if installed {
            Action::Play(slug)
        } else {
            Action::Install(slug)
        });
    }
    if response.clicked() {
        request_open(&slug);
    }
    None
}

fn detail_panel(ui: &mut Ui, store: &Store, slug: &str, width: f32) -> Action {
    let tokens = theme::current(ui.ctx());
    let Some(entry) = store.entry(slug) else {
        return Action::None;
    };
    let installed = store.is_installed(slug);
    let needs_file = store.needs_user_file(slug);
    let progress = store.playing(slug);
    let unavailable = !entry.supports_os(Os::current());
    let mut action = Action::None;

    ScrollArea::vertical()
        .id_salt("catalog-detail-panel")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_width(width);
            egui::Frame::new()
                .fill(tokens.surface)
                .stroke(Stroke::new(1.0_f32, tokens.border))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    ui.set_width((width - 32.0).max(200.0));
                    let art_size = Vec2::new(ui.available_width(), 152.0);
                    let (art_rect, _) = ui.allocate_exact_size(art_size, Sense::hover());
                    art::paint(ui, art_rect, art::palette_for(slug), 10);
                    let initial = art::initial_for(slug, &entry.title);
                    art::paint_initial(
                        ui,
                        art_rect,
                        art::Initial {
                            text: &initial,
                            size: 86.0,
                            opacity: 0.24,
                            offset: Vec2::ZERO,
                        },
                    );
                    art::bottom_veil(ui.painter(), art_rect, 0.0, 0.42);

                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        components::badge(
                            ui,
                            entry.kind.label(),
                            match entry.kind {
                                GameKind::Recomp => BadgeKind::Recomp,
                                GameKind::Decomp => BadgeKind::Decomp,
                                GameKind::NativePort => BadgeKind::Native,
                                _ => BadgeKind::Accent,
                            },
                        );
                        components::badge(
                            ui,
                            entry.status.label(),
                            match entry.status {
                                GameStatus::Playable => BadgeKind::Playable,
                                GameStatus::Beta => BadgeKind::Beta,
                                GameStatus::Wip => BadgeKind::Wip,
                                _ => BadgeKind::Neutral,
                            },
                        );
                        if installed {
                            components::badge(ui, "Installé", BadgeKind::Recomp);
                        }
                        if needs_file {
                            components::badge(ui, "Fichier requis", BadgeKind::Beta);
                        }
                    });
                    ui.add_space(10.0);
                    let title = ellipsize(
                        ui.ctx(),
                        &entry.title,
                        text::font(20.0, Weight::Bold),
                        (width - 48.0).max(160.0),
                    );
                    text::label(ui, title, 20.0, Weight::Bold, tokens.text);
                    ui.add_space(5.0);
                    let meta = format!("{} · {}", entry.original_platform, entry.project.name);
                    let meta = ellipsize(
                        ui.ctx(),
                        &meta,
                        text::font(10.0, Weight::Regular),
                        (width - 48.0).max(160.0),
                    );
                    text::label(ui, meta, 10.0, Weight::Regular, tokens.text_2);
                    ui.add_space(12.0);

                    let description = if !entry.description.is_empty() {
                        let mut value = entry.description.clone();
                        if value.chars().count() > 220 {
                            value = value.chars().take(220).collect::<String>();
                            value.push('…');
                        }
                        value
                    } else {
                        "Aucune description publiée pour ce projet.".to_owned()
                    };
                    egui::Frame::new()
                        .inner_margin(Margin::ZERO)
                        .show(ui, |ui| {
                            ui.set_max_height(74.0);
                            text::paragraph(ui, description, 10.0, tokens.text_2, 16.0);
                        });

                    ui.add_space(12.0);
                    components::divider(ui);
                    ui.add_space(12.0);
                    info_pair(ui, "Plateforme", &entry.original_platform);
                    info_pair(ui, "Type", entry.kind.label());
                    info_pair(ui, "Statut", entry.status.label());
                    if let Some(game) = store.library_game(slug) {
                        info_pair(ui, "Version", game.version.as_deref().unwrap_or("—"));
                        if let Some(size) = store.stats.per_game.get(slug) {
                            info_pair(ui, "Taille", &format::bytes(*size));
                        }
                    }
                    ui.add_space(14.0);

                    if let Some(progress) = progress {
                        components::progress_bar(ui, progress.fraction().unwrap_or(0.0), 6.0);
                        ui.add_space(7.0);
                        text::label(
                            ui,
                            if let Some(total) = progress.total {
                                format!(
                                    "{} / {} · {}",
                                    format::bytes(progress.downloaded),
                                    format::bytes(total),
                                    format::percent(progress.fraction().unwrap_or(0.0))
                                )
                            } else {
                                format!("{} téléchargés", format::bytes(progress.downloaded))
                            },
                            9.0,
                            Weight::Medium,
                            tokens.text_2,
                        );
                        ui.add_space(10.0);
                        if Button::new("Voir l'installation")
                            .width(ui.available_width())
                            .show(ui)
                            .clicked()
                        {
                            action = Action::Install(slug.to_owned());
                        }
                    } else if unavailable {
                        ui.label(
                            egui::RichText::new(format!(
                                "Aucun build disponible pour {}.",
                                Os::current().label()
                            ))
                            .font(text::font(9.0, Weight::Regular))
                            .color(tokens.orange),
                        );
                        ui.add_space(8.0);
                        Button::new("Indisponible")
                            .icon(icons::LOCK_KEYHOLE)
                            .width(ui.available_width())
                            .enabled(false)
                            .show(ui);
                    } else if installed {
                        if needs_file {
                            if Button::primary("Choisir le fichier")
                                .icon(icons::FILE_QUESTION)
                                .width(ui.available_width())
                                .show(ui)
                                .clicked()
                            {
                                action = Action::SelectUserFile(slug.to_owned());
                            }
                        } else if Button::primary("Jouer")
                            .icon(icons::PLAY)
                            .width(ui.available_width())
                            .show(ui)
                            .clicked()
                        {
                            action = Action::Play(slug.to_owned());
                        }
                    } else if Button::primary("Installer")
                        .icon(icons::DOWNLOAD)
                        .width(ui.available_width())
                        .show(ui)
                        .clicked()
                    {
                        action = Action::Install(slug.to_owned());
                    }

                    ui.add_space(8.0);
                    if Button::new("Voir la fiche")
                        .icon(icons::CHEVRON_RIGHT)
                        .width(ui.available_width())
                        .show(ui)
                        .clicked()
                    {
                        action = Action::OpenGame {
                            slug: slug.to_owned(),
                            from_library: false,
                        };
                    }
                    if installed {
                        ui.add_space(4.0);
                        if Button::ghost("Désinstaller")
                            .icon(icons::TRASH_2)
                            .width(ui.available_width())
                            .show(ui)
                            .clicked()
                        {
                            action = Action::Uninstall(slug.to_owned());
                        }
                    }
                });
        });
    action
}

fn info_pair(ui: &mut Ui, label: &str, value: &str) {
    let tokens = theme::current(ui.ctx());
    ui.horizontal(|ui| {
        ui.set_width(ui.available_width());
        text::label(ui, label, 10.0, Weight::Regular, tokens.text_3);
        let max_width = (ui.available_width() - 8.0).max(40.0);
        let value = text::ellipsize(ui.ctx(), value, text::font(10.0, Weight::Medium), max_width);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            text::label(ui, value, 10.0, Weight::Medium, tokens.text);
        });
    });
    ui.add_space(7.0);
}

fn view_toggle(ui: &mut Ui, view: &mut ViewMode) -> bool {
    let tokens = theme::current(ui.ctx());
    let (rect, allocated) = ui.allocate_exact_size(Vec2::new(68.0, 34.0), Sense::click());
    let response = ui.interact(rect, allocated.id, Sense::click());
    let mut changed = false;
    let half = rect.width() / 2.0;
    let selected = usize::from(*view == ViewMode::List);
    ui.painter()
        .rect_filled(rect, CornerRadius::same(9), tokens.surface_2);
    let active = Rect::from_min_size(
        egui::pos2(
            rect.left() + 2.0 + selected as f32 * (half - 2.0),
            rect.top() + 2.0,
        ),
        Vec2::new(half - 4.0, rect.height() - 4.0),
    );
    ui.painter()
        .rect_filled(active, CornerRadius::same(7), tokens.surface_3);
    for (index, glyph) in [icons::GRID_2X2, icons::LIST].into_iter().enumerate() {
        let center = egui::pos2(rect.left() + half * (index as f32 + 0.5), rect.center().y);
        let color = if index == selected {
            tokens.text
        } else {
            tokens.text_3
        };
        icons::paint(ui.painter(), center, glyph, 16.0, color);
    }
    if response.clicked() {
        if let Some(position) = response.interact_pointer_pos() {
            let next = if position.x > rect.center().x {
                ViewMode::List
            } else {
                ViewMode::Grid
            };
            if next != *view {
                *view = next;
                changed = true;
            }
        }
    }
    changed
}

fn show_skeleton(ui: &mut Ui, state: &CatalogUi, compact: bool) {
    let gap = 16.0;
    let panel_width = if compact { 310.0 } else { 360.0 };
    let has_panel = state.selected.is_some();
    let width = if has_panel {
        (ui.available_width() - panel_width - 20.0).max(320.0)
    } else {
        ui.available_width()
    };
    let columns = if width >= 720.0 {
        3
    } else if width >= 430.0 {
        2
    } else {
        1
    };
    let card_width = ((width - gap * (columns as f32 - 1.0)) / columns as f32).floor();
    let card_height = if compact { 224.0 } else { 232.0 };

    if has_panel {
        let available = ui.available_size();
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 20.0;
            ui.allocate_ui_with_layout(
                Vec2::new(width, available.y),
                Layout::top_down(Align::Min),
                |ui| skeleton_cards(ui, card_width, card_height, columns, gap),
            );
            ui.allocate_ui_with_layout(
                Vec2::new(panel_width, available.y),
                Layout::top_down(Align::Min),
                |ui| skeleton_panel(ui, panel_width),
            );
        });
    } else {
        skeleton_cards(ui, card_width, card_height, columns, gap);
    }
}

fn skeleton_cards(ui: &mut Ui, card_width: f32, card_height: f32, columns: usize, gap: f32) {
    let tokens = theme::current(ui.ctx());
    let pulse = ((ui.ctx().input(|input| input.time) * 2.0).sin() as f32 * 0.5 + 0.5) * 0.35 + 0.35;
    let block = tokens.surface_2.linear_multiply(pulse.min(1.0));
    for _ in 0..2 {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for _ in 0..columns {
                let (rect, _) =
                    ui.allocate_exact_size(Vec2::new(card_width, card_height), Sense::hover());
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(12), tokens.surface);
                let art_rect = Rect::from_min_size(rect.min, Vec2::new(card_width, 132.0));
                ui.painter()
                    .rect_filled(art_rect, CornerRadius::same(12), block);
                let line = Rect::from_min_size(
                    egui::pos2(rect.left() + 14.0, rect.top() + 150.0),
                    Vec2::new(card_width * 0.62, 10.0),
                );
                ui.painter().rect_filled(line, CornerRadius::same(4), block);
                let line = Rect::from_min_size(
                    egui::pos2(rect.left() + 14.0, rect.top() + 170.0),
                    Vec2::new(card_width * 0.4, 8.0),
                );
                ui.painter().rect_filled(line, CornerRadius::same(4), block);
            }
        });
        ui.add_space(gap);
    }
}

fn skeleton_panel(ui: &mut Ui, width: f32) {
    let tokens = theme::current(ui.ctx());
    let pulse = ((ui.ctx().input(|input| input.time) * 2.0).sin() as f32 * 0.5 + 0.5) * 0.35 + 0.35;
    let block = tokens.surface_2.linear_multiply(pulse.min(1.0));
    let height = ui.available_height().clamp(240.0, 560.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::same(12), tokens.surface);
    let art = Rect::from_min_size(
        egui::pos2(rect.left() + 16.0, rect.top() + 16.0),
        Vec2::new(width - 32.0, 152.0),
    );
    ui.painter().rect_filled(art, CornerRadius::same(10), block);
    let mut y = art.bottom() + 22.0;
    for line_width in [0.34_f32, 0.62, 0.92, 0.78, 0.55] {
        let line = Rect::from_min_size(
            egui::pos2(rect.left() + 16.0, y),
            Vec2::new((width - 32.0) * line_width, 9.0),
        );
        ui.painter().rect_filled(line, CornerRadius::same(4), block);
        y += 20.0;
    }
    let button = Rect::from_min_size(
        egui::pos2(rect.left() + 16.0, rect.bottom() - 62.0),
        Vec2::new(width - 32.0, 34.0),
    );
    ui.painter()
        .rect_filled(button, CornerRadius::same(9), block);
}

fn empty_state(ui: &mut Ui, tokens: &theme::Tokens) {
    ui.add_space(40.0);
    ui.vertical_centered(|ui| {
        icons::icon(ui, icons::SEARCH, 28.0, tokens.text_3);
        ui.add_space(10.0);
        text::label(
            ui,
            "Aucun jeu ne correspond à ces filtres.",
            13.0,
            Weight::SemiBold,
            tokens.text,
        );
        ui.add_space(5.0);
        text::label(
            ui,
            "Essayez d'élargir la recherche ou de retirer un filtre.",
            10.0,
            Weight::Regular,
            tokens.text_2,
        );
    });
}

fn paint_text(ui: &Ui, position: Pos2, content: &str, font: FontId, color: Color32) {
    let galley = ui.painter().layout_no_wrap(content.to_owned(), font, color);
    ui.painter().galley(position, galley, color);
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
    let size = Vec2::new(galley.size().x + 14.0, 18.0);
    let rect = Rect::from_min_size(position, size);
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

fn ellipsize(ctx: &egui::Context, text: &str, font: FontId, max_width: f32) -> String {
    text::ellipsize(ctx, text, font, max_width)
}
