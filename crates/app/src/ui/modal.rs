//! Modal scaffolding shared by installation and uninstallation.

use eframe::egui::{
    self, Align, Color32, Context, CornerRadius, Id, Layout, Margin, Response, Stroke, Ui, Vec2,
};

use super::components;
use super::icons;
use super::text::{self, Weight};
use super::theme;

pub struct ModalResponse {
    pub backdrop_clicked: bool,
    pub escape: bool,
}

pub fn show<S>(
    ctx: &Context,
    id: &str,
    width: f32,
    state: &mut S,
    content: impl FnOnce(&mut Ui, &mut S),
    footer: impl FnOnce(&mut Ui, &mut S),
) -> ModalResponse {
    let tokens = theme::current(ctx);
    let frame = egui::Frame::new()
        .fill(tokens.elevated)
        .stroke(Stroke::new(1.0_f32, tokens.border_strong))
        .corner_radius(CornerRadius::same(14))
        .shadow(tokens.modal_shadow);
    let escape = ctx.input(|input| input.key_pressed(egui::Key::Escape));
    let response = egui::Modal::new(Id::new(id))
        .backdrop_color(tokens.overlay)
        .frame(frame)
        .show(ctx, |ui| {
            ui.set_width(width);
            content(ui, state);
            footer(ui, state);
        });
    ModalResponse {
        backdrop_clicked: response.backdrop_response.clicked(),
        escape,
    }
}

/// Header with a close button.
pub fn header(ui: &mut Ui, eyebrow: &str, title: &str) -> bool {
    header_impl(ui, eyebrow, title, true)
}

/// Header used while an irreversible operation is running.
pub fn header_locked(ui: &mut Ui, eyebrow: &str, title: &str) {
    let _ = header_impl(ui, eyebrow, title, false);
}

fn header_impl(ui: &mut Ui, eyebrow: &str, title: &str, closable: bool) -> bool {
    let tokens = theme::current(ui.ctx());
    let mut close = false;
    egui::Frame::new()
        .inner_margin(Margin {
            left: 20,
            right: 14,
            top: 14,
            bottom: 14,
        })
        .show(ui, |ui| {
            ui.set_height(43.0);
            ui.horizontal(|ui| {
                let title_width = (ui.available_width() - 52.0).max(120.0);
                let title =
                    text::ellipsize(ui.ctx(), title, text::font(17.0, Weight::Bold), title_width);
                ui.vertical(|ui| {
                    text::label(ui, eyebrow, 9.0, Weight::Bold, tokens.accent);
                    ui.add_space(4.0);
                    text::label(ui, title, 17.0, Weight::Bold, tokens.text);
                });
                if closable {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if components::icon_button(ui, icons::X, false)
                            .on_hover_text("Fermer")
                            .clicked()
                        {
                            close = true;
                        }
                    });
                }
            });
        });
    close
}

pub fn footer(ui: &mut Ui, content: impl FnOnce(&mut Ui)) {
    let tokens = theme::current(ui.ctx());
    egui::Frame::new()
        .fill(tokens.surface_2)
        .inner_margin(Margin {
            left: 20,
            right: 20,
            top: 13,
            bottom: 14,
        })
        .show(ui, |ui| {
            ui.set_min_height(34.0);
            ui.with_layout(Layout::right_to_left(Align::Center), content);
        });
}

pub fn stepper(ui: &mut Ui, active: usize, labels: &[&str]) {
    let tokens = theme::current(ui.ctx());
    egui::Frame::new()
        .fill(tokens.surface_2)
        .inner_margin(Margin::symmetric(20, 12))
        .show(ui, |ui| {
            ui.set_height(38.0);
            ui.horizontal_centered(|ui| {
                let total = labels.len();
                for (index, label) in labels.iter().enumerate() {
                    let done = index < active;
                    let current = index == active;
                    let (circle_fill, circle_text) = if done {
                        (tokens.green_soft, tokens.green)
                    } else if current {
                        (tokens.accent, Color32::WHITE)
                    } else {
                        (tokens.surface_3, tokens.text_3)
                    };
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(23.0), egui::Sense::hover());
                    ui.painter().circle_filled(rect.center(), 11.5, circle_fill);
                    if done {
                        icons::paint(ui.painter(), rect.center(), icons::CHECK, 13.0, circle_text);
                    } else {
                        let galley = ui.painter().layout_no_wrap(
                            (index + 1).to_string(),
                            text::font(10.0, Weight::SemiBold),
                            circle_text,
                        );
                        ui.painter().galley(
                            rect.center() - galley.size() / 2.0,
                            galley,
                            circle_text,
                        );
                    }
                    ui.add_space(7.0);
                    text::label(
                        ui,
                        *label,
                        10.0,
                        Weight::SemiBold,
                        if current || done {
                            tokens.text
                        } else {
                            tokens.text_3
                        },
                    );
                    if index + 1 < total {
                        ui.add_space(14.0);
                        let (line, _) =
                            ui.allocate_exact_size(Vec2::new(27.0, 1.0), egui::Sense::hover());
                        ui.painter().rect_filled(
                            line,
                            CornerRadius::ZERO,
                            if done {
                                tokens.green
                            } else {
                                tokens.border_strong
                            },
                        );
                        ui.add_space(14.0);
                    }
                }
            });
        });
}

pub fn info_box(ui: &mut Ui, glyph: char, tone: Tone, title: &str, description: &str) -> Response {
    let tokens = theme::current(ui.ctx());
    let (fg, bg) = match tone {
        Tone::Green => (tokens.green, tokens.green_soft),
        Tone::Red => (tokens.red, tokens.red_soft),
    };
    egui::Frame::new()
        .fill(bg)
        .corner_radius(CornerRadius::same(9))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                icons::icon(ui, glyph, 16.0, fg);
                ui.vertical(|ui| {
                    text::label(ui, title, 10.0, Weight::SemiBold, tokens.text);
                    ui.add_space(2.0);
                    text::paragraph(ui, description, 9.0, tokens.text_2, 13.0);
                });
            });
        })
        .response
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Green,
    Red,
}
