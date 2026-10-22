//! Typography helpers.
//!
//! Inter is loaded as four static weights. These helpers keep tracking and
//! fallback behaviour consistent across custom widgets.

use eframe::egui::{self, Color32, Context, FontId, Response, RichText, Ui};

use super::fonts;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    Regular,
    Medium,
    SemiBold,
    Bold,
}

impl Weight {
    fn family(self) -> &'static str {
        match self {
            Self::Regular => fonts::REGULAR,
            Self::Medium => fonts::MEDIUM,
            Self::SemiBold => fonts::SEMIBOLD,
            Self::Bold => fonts::BOLD,
        }
    }
}

pub fn font(size: f32, weight: Weight) -> FontId {
    FontId::new(size, fonts::family(weight.family()))
}

/// Shortens a single line with an ellipsis when it exceeds `max_width`.
pub fn ellipsize(ctx: &Context, text: &str, font: FontId, max_width: f32) -> String {
    let width = |value: &str| {
        ctx.fonts(|fonts| {
            fonts
                .layout_no_wrap(value.to_owned(), font.clone(), Color32::WHITE)
                .size()
                .x
        })
    };
    if width(text) <= max_width {
        return text.to_owned();
    }
    let mut output = String::new();
    for ch in text.chars() {
        let mut candidate = output.clone();
        candidate.push(ch);
        candidate.push('…');
        if width(&candidate) > max_width {
            break;
        }
        output.push(ch);
    }
    output.push('…');
    output
}

pub fn tracking(size: f32) -> f32 {
    if size >= 20.0 {
        -0.045 * size
    } else {
        -0.01 * size
    }
}

pub fn rich(text: impl Into<String>, size: f32, weight: Weight, color: Color32) -> RichText {
    RichText::new(text)
        .font(font(size, weight))
        .color(color)
        .extra_letter_spacing(tracking(size))
}

pub fn label(
    ui: &mut Ui,
    text: impl Into<String>,
    size: f32,
    weight: Weight,
    color: Color32,
) -> Response {
    ui.add(egui::Label::new(rich(text, size, weight, color)))
}

pub fn page_title(ui: &mut Ui, text: impl Into<String>, color: Color32) -> Response {
    ui.add(egui::Label::new(rich(text, 25.0, Weight::Bold, color)))
}

pub fn page_description(ui: &mut Ui, text: impl Into<String>, color: Color32) -> Response {
    ui.add(egui::Label::new(rich(text, 11.0, Weight::Regular, color)))
}

/// Word-wrapped paragraph; returns the allocated height.
pub fn paragraph(
    ui: &mut Ui,
    text: impl Into<String>,
    size: f32,
    color: Color32,
    line_height: f32,
) -> f32 {
    let response = ui.add(
        egui::Label::new(rich(text, size, Weight::Regular, color).line_height(Some(line_height)))
            .wrap(),
    );
    response.rect.height()
}
