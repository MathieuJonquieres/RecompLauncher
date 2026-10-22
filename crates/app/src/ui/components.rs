//! Reusable widgets painted from the design tokens.
//!
//! These replace egui's default look so spacing, radii and heights stay
//! identical across pages and themes.

use eframe::egui::{
    self, Align, Color32, CornerRadius, FontId, Layout, Margin, Rect, Response, RichText, Sense,
    Stroke, StrokeKind, TextEdit, Ui, Vec2, WidgetText,
};

use super::icons;
use super::text::{self, Weight};
use super::theme::{self, Tokens};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    Secondary,
    Primary,
    Ghost,
    Danger,
}

pub struct Button {
    label: String,
    variant: ButtonVariant,
    icon: Option<char>,
    width: Option<f32>,
    enabled: bool,
}

impl Button {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            variant: ButtonVariant::Secondary,
            icon: None,
            width: None,
            enabled: true,
        }
    }

    pub fn primary(label: impl Into<String>) -> Self {
        Self {
            variant: ButtonVariant::Primary,
            ..Self::new(label)
        }
    }

    pub fn ghost(label: impl Into<String>) -> Self {
        Self {
            variant: ButtonVariant::Ghost,
            ..Self::new(label)
        }
    }

    pub fn danger(label: impl Into<String>) -> Self {
        Self {
            variant: ButtonVariant::Danger,
            ..Self::new(label)
        }
    }

    pub fn icon(mut self, glyph: char) -> Self {
        self.icon = Some(glyph);
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let tokens = theme::current(ui.ctx());
        let font = text::font(11.0, Weight::SemiBold);
        let text_color = match self.variant {
            ButtonVariant::Primary | ButtonVariant::Danger => Color32::WHITE,
            ButtonVariant::Ghost => tokens.text_2,
            ButtonVariant::Secondary => tokens.text,
        };
        let galley = ui
            .painter()
            .layout_no_wrap(self.label.clone(), font, text_color);
        let icon_w = if self.icon.is_some() { 16.0 } else { 0.0 };
        let gap = if self.icon.is_some() { 7.0 } else { 0.0 };
        let width = self.width.unwrap_or(24.0 + icon_w + gap + galley.size().x);
        let size = Vec2::new(width, 34.0);
        let sense = if self.enabled {
            Sense::click()
        } else {
            Sense::hover()
        };
        let (rect, allocated) = ui.allocate_exact_size(size, sense);
        let response = ui.interact(rect, allocated.id, sense);

        if !ui.is_rect_visible(rect) {
            return response;
        }

        let hovered = response.hovered() && self.enabled;
        let alpha = if self.enabled { 1.0 } else { 0.42 };
        let (fill, stroke, color) = match self.variant {
            ButtonVariant::Secondary => (
                if hovered {
                    tokens.surface_3
                } else {
                    tokens.surface_2
                },
                Some(tokens.border_strong),
                tokens.text,
            ),
            ButtonVariant::Primary => (
                if hovered {
                    tokens.accent_hover
                } else {
                    tokens.accent
                },
                None,
                Color32::WHITE,
            ),
            ButtonVariant::Ghost => (
                if hovered {
                    tokens.surface_2
                } else {
                    Color32::TRANSPARENT
                },
                None,
                if hovered { tokens.text } else { tokens.text_2 },
            ),
            ButtonVariant::Danger => (
                if hovered {
                    tokens.red.linear_multiply(0.88)
                } else {
                    tokens.red
                },
                None,
                Color32::WHITE,
            ),
        };

        if self.variant == ButtonVariant::Primary {
            let shadow = if self.enabled {
                tokens.accent_shadow
            } else {
                egui::epaint::Shadow::NONE
            };
            ui.painter()
                .add(shadow.as_shape(rect, CornerRadius::same(9)));
        }

        let fill = fill.linear_multiply(alpha);
        let color = color.linear_multiply(alpha);
        ui.painter().rect_filled(rect, CornerRadius::same(9), fill);
        if let Some(stroke) = stroke {
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(9),
                Stroke::new(1.0_f32, stroke.linear_multiply(alpha)),
                StrokeKind::Inside,
            );
        }
        if response.has_focus() {
            ui.painter().rect_stroke(
                rect.expand(2.0),
                CornerRadius::same(11),
                Stroke::new(2.0_f32, tokens.accent_soft),
                StrokeKind::Outside,
            );
        }

        let content_w = icon_w + gap + galley.size().x;
        let start_x = rect.center().x - content_w / 2.0;
        if let Some(glyph) = self.icon {
            icons::paint(
                ui.painter(),
                egui::pos2(start_x + 8.0, rect.center().y),
                glyph,
                16.0,
                color,
            );
        }
        ui.painter().galley(
            egui::pos2(
                start_x + icon_w + gap,
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            color,
        );

        if self.enabled {
            response.on_hover_cursor(egui::CursorIcon::PointingHand)
        } else {
            response
        }
    }
}

pub fn icon_button(ui: &mut Ui, glyph: char, active: bool) -> Response {
    let tokens = theme::current(ui.ctx());
    let (rect, allocated) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    let response = ui.interact(rect, allocated.id, Sense::click());
    let hovered = response.hovered();

    let fill = if active {
        tokens.accent_soft
    } else if hovered {
        tokens.surface_3
    } else {
        Color32::TRANSPARENT
    };
    let color = if active {
        tokens.accent
    } else if hovered {
        tokens.text
    } else {
        tokens.text_2
    };
    ui.painter().rect_filled(rect, CornerRadius::same(8), fill);
    icons::paint(ui.painter(), rect.center(), glyph, 18.0, color);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn icon_button_sized(ui: &mut Ui, glyph: char, size: f32, button: f32) -> Response {
    let tokens = theme::current(ui.ctx());
    let (rect, allocated) = ui.allocate_exact_size(Vec2::splat(button), Sense::click());
    let response = ui.interact(rect, allocated.id, Sense::click());
    let color = if response.hovered() {
        tokens.text
    } else {
        tokens.text_2
    };
    if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(8), tokens.surface_3);
    }
    icons::paint(ui.painter(), rect.center(), glyph, size, color);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BadgeKind {
    Playable,
    Beta,
    Wip,
    Recomp,
    Decomp,
    Native,
    Last,
    Accent,
    Neutral,
}

pub fn badge(ui: &mut Ui, label: impl Into<String>, kind: BadgeKind) -> Response {
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
    let label = label.into();
    let galley = ui
        .painter()
        .layout_no_wrap(label, text::font(9.0, Weight::SemiBold), fg);
    let size = Vec2::new(galley.size().x + 14.0, 18.0);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(6), bg);
    ui.painter().galley(
        egui::pos2(
            rect.center().x - galley.size().x / 2.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        fg,
    );
    response
}

pub fn dot(ui: &mut Ui, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(3.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 1.5, color);
    response
}

pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let tokens = theme::current(ui.ctx());
    let (rect, allocated) = ui.allocate_exact_size(Vec2::new(36.0, 20.0), Sense::click());
    let response = ui.interact(rect, allocated.id, Sense::click());
    if response.clicked() {
        *on = !*on;
    }
    let t = ui.ctx().animate_bool_with_time(response.id, *on, 0.12);
    let rail = if *on { tokens.accent } else { tokens.surface_3 };
    ui.painter().rect_filled(rect, CornerRadius::same(10), rail);
    let knob_x = egui::lerp((rect.left() + 3.0)..=(rect.right() - 17.0), t);
    ui.painter().circle_filled(
        egui::pos2(knob_x + 7.0, rect.center().y),
        7.0,
        if *on { Color32::WHITE } else { tokens.text_2 },
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn segmented(ui: &mut Ui, dark: &mut bool) -> Response {
    let tokens = theme::current(ui.ctx());
    let (rect, allocated) = ui.allocate_exact_size(Vec2::new(132.0, 32.0), Sense::click());
    let response = ui.interact(rect, allocated.id, Sense::click());
    let half = rect.width() / 2.0;
    let selected = if *dark { 0 } else { 1 };
    if response.clicked() {
        if let Some(pos) = response.interact_pointer_pos() {
            *dark = pos.x <= rect.center().x;
        }
    }
    ui.painter()
        .rect_filled(rect, CornerRadius::same(8), tokens.surface_2);
    let active_rect = Rect::from_min_size(
        egui::pos2(
            rect.left() + 2.0 + selected as f32 * (half - 2.0),
            rect.top() + 2.0,
        ),
        Vec2::new(half - 4.0, rect.height() - 4.0),
    );
    ui.painter()
        .rect_filled(active_rect, CornerRadius::same(7), tokens.surface_3);

    for (index, (glyph, label)) in [(icons::MOON, "Sombre"), (icons::SUN, "Clair")]
        .into_iter()
        .enumerate()
    {
        let cell = Rect::from_min_size(
            egui::pos2(rect.left() + index as f32 * half, rect.top()),
            Vec2::new(half, rect.height()),
        );
        let color = if index == selected {
            tokens.text
        } else {
            tokens.text_2
        };
        let galley = ui.painter().layout_no_wrap(
            label.to_owned(),
            text::font(10.0, Weight::SemiBold),
            color,
        );
        let total = 15.0 + 6.0 + galley.size().x;
        let start = cell.center().x - total / 2.0;
        icons::paint(
            ui.painter(),
            egui::pos2(start + 7.5, cell.center().y),
            glyph,
            15.0,
            color,
        );
        ui.painter().galley(
            egui::pos2(start + 21.0, cell.center().y - galley.size().y / 2.0),
            galley,
            color,
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn search_field(ui: &mut Ui, text_value: &mut String, hint: &str, width: f32) -> Response {
    let tokens = theme::current(ui.ctx());
    let frame = egui::Frame::new()
        .fill(tokens.surface)
        .stroke(Stroke::new(1.0_f32, tokens.border))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(11, 0));
    let inner = frame.show(ui, |ui| {
        ui.set_width((width - 22.0).max(80.0));
        ui.set_height(34.0);
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            icons::icon(ui, icons::SEARCH, 17.0, tokens.text_3);
            ui.add_space(2.0);
            let edit = TextEdit::singleline(text_value)
                .frame(false)
                .margin(Vec2::ZERO)
                .desired_width(f32::INFINITY)
                .hint_text(RichText::new(hint).color(tokens.text_3).size(11.0))
                .font(text::font(11.0, Weight::Regular))
                .text_color(tokens.text);
            ui.add(edit)
        })
        .inner
    });
    let response = inner.inner;
    if response.has_focus() {
        ui.painter().rect_stroke(
            inner.response.rect.expand(3.0),
            CornerRadius::same(12),
            Stroke::new(3.0_f32, tokens.accent_soft),
            StrokeKind::Outside,
        );
    }
    response
}

pub fn text_field(ui: &mut Ui, value: &mut String, hint: &str, width: f32) -> Response {
    text_field_with(ui, value, hint, width, false)
}

pub fn text_field_mono(ui: &mut Ui, value: &mut String, hint: &str, width: f32) -> Response {
    text_field_with(ui, value, hint, width, true)
}

fn text_field_with(
    ui: &mut Ui,
    value: &mut String,
    hint: &str,
    width: f32,
    mono: bool,
) -> Response {
    let tokens = theme::current(ui.ctx());
    let frame = egui::Frame::new()
        .fill(tokens.surface)
        .stroke(Stroke::new(1.0_f32, tokens.border))
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(11, 0));
    let inner = frame.show(ui, |ui| {
        ui.set_width((width - 22.0).max(60.0));
        ui.set_height(34.0);
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            let edit = TextEdit::singleline(value)
                .frame(false)
                .margin(Vec2::ZERO)
                .desired_width(f32::INFINITY)
                .hint_text(RichText::new(hint).color(tokens.text_3).size(10.0))
                .font(if mono {
                    FontId::monospace(10.0)
                } else {
                    text::font(10.0, Weight::Regular)
                })
                .text_color(tokens.text);
            ui.add(edit)
        })
        .inner
    });
    let response = inner.inner;
    if response.has_focus() {
        ui.painter().rect_stroke(
            inner.response.rect.expand(3.0),
            CornerRadius::same(12),
            Stroke::new(3.0_f32, tokens.accent_soft),
            StrokeKind::Outside,
        );
    }
    response
}

pub fn select<T: PartialEq + Clone>(
    ui: &mut Ui,
    id: &str,
    width: f32,
    selected: &mut T,
    options: &[(T, String)],
    icon: Option<char>,
) -> bool {
    let tokens = theme::current(ui.ctx());
    let mut changed = false;
    let label = options
        .iter()
        .find(|(value, _)| value == selected)
        .map(|(_, label)| label.clone())
        .unwrap_or_default();

    let selected_text: WidgetText = match icon {
        Some(glyph) => {
            let mut job = egui::text::LayoutJob::default();
            job.append(
                &format!("{glyph}  "),
                0.0,
                egui::TextFormat {
                    font_id: icons::font(14.0),
                    color: tokens.text_2,
                    ..Default::default()
                },
            );
            job.append(
                &label,
                0.0,
                egui::TextFormat {
                    font_id: text::font(10.0, Weight::Medium),
                    color: tokens.text,
                    ..Default::default()
                },
            );
            job.into()
        }
        None => text::rich(label, 10.0, Weight::Medium, tokens.text).into(),
    };

    let combo = egui::ComboBox::from_id_salt(id)
        .width(width)
        .selected_text(selected_text)
        .icon(move |ui, rect, visuals, _open, _above| {
            icons::paint(
                ui.painter(),
                rect.center(),
                icons::CHEVRON_DOWN,
                14.0,
                visuals.fg_stroke.color,
            );
        });
    combo.show_ui(ui, |ui| {
        ui.set_min_width(width.max(140.0));
        ui.spacing_mut().interact_size.y = 28.0;
        for (value, label) in options {
            if ui
                .selectable_label(
                    value == selected,
                    text::rich(label.clone(), 10.0, Weight::Regular, tokens.text),
                )
                .clicked()
            {
                *selected = value.clone();
                changed = true;
            }
        }
    });
    changed
}

pub fn page_header(ui: &mut Ui, title: &str, description: &str, actions: impl FnOnce(&mut Ui)) {
    let tokens = theme::current(ui.ctx());
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            text::page_title(ui, title, tokens.text);
            ui.add_space(7.0);
            text::page_description(ui, description, tokens.text_2);
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            actions(ui);
        });
    });
    ui.add_space(27.0);
}

pub fn divider(ui: &mut Ui) {
    let tokens = theme::current(ui.ctx());
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 1.0), Sense::hover());
    ui.painter()
        .rect_filled(rect, CornerRadius::ZERO, tokens.border);
}

pub fn progress_bar(ui: &mut Ui, fraction: f32, height: f32) {
    let tokens = theme::current(ui.ctx());
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
    let radius = CornerRadius::same((height / 2.0) as u8);
    ui.painter().rect_filled(rect, radius, tokens.surface_3);
    let filled_width = rect.width() * fraction.clamp(0.0, 1.0);
    if filled_width > 0.0 {
        let filled =
            Rect::from_min_size(rect.min, Vec2::new(filled_width.max(height), rect.height()));
        ui.painter().rect_filled(filled, radius, tokens.accent);
    }
}

/// Paints a rotating 270° arc, the visual equivalent of Lucide's
/// `loader-circle` without rasterising a rotation.
pub fn spinner(ui: &mut Ui, size: f32, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_spinner(ui, rect.center(), size / 2.0, color);
    response
}

pub fn paint_spinner(ui: &Ui, center: egui::Pos2, radius: f32, color: Color32) {
    let time = ui.ctx().input(|input| input.time) as f32;
    let start = time * 5.0;
    let segments = 36;
    let points: Vec<egui::Pos2> = (0..=segments)
        .map(|index| {
            let angle = start + index as f32 / segments as f32 * std::f32::consts::TAU * 0.75;
            center + Vec2::new(angle.cos(), angle.sin()) * radius
        })
        .collect();
    ui.painter()
        .add(egui::Shape::line(points, Stroke::new(1.8_f32, color)));
    ui.ctx().request_repaint();
}

pub fn selectable_row(
    ui: &mut Ui,
    label: &str,
    glyph: char,
    active: bool,
    height: f32,
) -> Response {
    let tokens = theme::current(ui.ctx());
    let width = ui.available_width();
    let (rect, allocated) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());
    let response = ui.interact(rect, allocated.id, Sense::click());
    let fill = if active {
        tokens.accent_soft
    } else if response.hovered() {
        tokens.surface_2
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, CornerRadius::same(9), fill);
    let color = if active || response.hovered() {
        tokens.text
    } else {
        tokens.text_2
    };
    let icon_color = if active { tokens.accent } else { color };
    icons::paint_left(
        ui.painter(),
        Rect::from_min_max(
            egui::pos2(rect.left() + 11.0, rect.top()),
            egui::pos2(rect.left() + 29.0, rect.bottom()),
        ),
        glyph,
        18.0,
        icon_color,
    );
    let galley =
        ui.painter()
            .layout_no_wrap(label.to_owned(), text::font(13.0, Weight::Medium), color);
    ui.painter().galley(
        egui::pos2(rect.left() + 40.0, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Square checkbox used by the uninstall confirmation.
pub fn checkbox_square(painter: &egui::Painter, rect: Rect, checked: bool, tokens: &Tokens) {
    let (fill, stroke) = if checked {
        (tokens.accent, tokens.accent)
    } else {
        (Color32::TRANSPARENT, tokens.border_strong)
    };
    painter.rect_filled(rect, CornerRadius::same(5), fill);
    painter.rect_stroke(
        rect,
        CornerRadius::same(5),
        Stroke::new(1.0_f32, stroke),
        StrokeKind::Inside,
    );
    if checked {
        icons::paint(painter, rect.center(), icons::CHECK, 12.0, Color32::WHITE);
    }
}

pub fn mono(text: impl Into<String>, size: f32, color: Color32) -> RichText {
    RichText::new(text)
        .font(FontId::monospace(size))
        .color(color)
}
