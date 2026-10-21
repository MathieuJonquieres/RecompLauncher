//! Semantic design tokens and egui style application.
//!
//! The values mirror the reproduction brief exactly (dark and light). Views
//! read tokens through [`current`], so switching the theme never changes a
//! single dimension or layout decision.

use eframe::egui::{
    self, Color32, Context, CornerRadius, FontFamily, FontId, Id, Margin, Stroke, Style, TextStyle,
    Visuals,
};

use super::fonts;

const TOKENS_ID: &str = "recomp.tokens";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub dark: bool,
    pub app_bg: Color32,
    pub surface: Color32,
    pub surface_2: Color32,
    pub surface_3: Color32,
    pub elevated: Color32,
    pub text: Color32,
    pub text_2: Color32,
    pub text_3: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub accent_soft: Color32,
    pub green: Color32,
    pub green_soft: Color32,
    pub orange: Color32,
    pub orange_soft: Color32,
    pub red: Color32,
    pub red_soft: Color32,
    pub blue: Color32,
    pub blue_soft: Color32,
    pub modal_shadow: egui::epaint::Shadow,
    pub overlay: Color32,
    pub card_shadow: egui::epaint::Shadow,
    pub accent_shadow: egui::epaint::Shadow,
}

impl Tokens {
    pub fn dark() -> Self {
        let rgba = Color32::from_rgba_unmultiplied;
        Self {
            dark: true,
            app_bg: Color32::from_rgb(0x0F, 0x11, 0x15),
            surface: Color32::from_rgb(0x17, 0x1A, 0x21),
            surface_2: Color32::from_rgb(0x1C, 0x20, 0x28),
            surface_3: Color32::from_rgb(0x23, 0x28, 0x33),
            elevated: Color32::from_rgb(0x20, 0x24, 0x2D),
            text: Color32::from_rgb(0xE6, 0xE8, 0xEC),
            text_2: Color32::from_rgb(0x92, 0x99, 0xA7),
            text_3: Color32::from_rgb(0x64, 0x6C, 0x7A),
            border: Color32::from_rgb(0x28, 0x2D, 0x37),
            border_strong: Color32::from_rgb(0x35, 0x3B, 0x47),
            accent: Color32::from_rgb(0x6C, 0x7C, 0xFF),
            accent_hover: Color32::from_rgb(0x7D, 0x8A, 0xFF),
            accent_soft: rgba(108, 124, 255, 36),
            green: Color32::from_rgb(0x57, 0xC7, 0x85),
            green_soft: rgba(87, 199, 133, 31),
            orange: Color32::from_rgb(0xF2, 0xA5, 0x52),
            orange_soft: rgba(242, 165, 82, 33),
            red: Color32::from_rgb(0xEF, 0x6B, 0x73),
            red_soft: rgba(239, 107, 115, 33),
            blue: Color32::from_rgb(0x63, 0xA9, 0xEE),
            blue_soft: rgba(99, 169, 238, 41),
            modal_shadow: egui::epaint::Shadow {
                offset: [0, 24],
                blur: 70,
                spread: 0,
                color: rgba(0, 0, 0, 102),
            },
            overlay: rgba(5, 6, 9, 184),
            card_shadow: egui::epaint::Shadow {
                offset: [0, 8],
                blur: 24,
                spread: 0,
                color: rgba(0, 0, 0, 80),
            },
            accent_shadow: egui::epaint::Shadow {
                offset: [0, 4],
                blur: 14,
                spread: 0,
                color: rgba(108, 124, 255, 72),
            },
        }
    }

    pub fn light() -> Self {
        let rgba = Color32::from_rgba_unmultiplied;
        Self {
            dark: false,
            app_bg: Color32::from_rgb(0xF2, 0xF3, 0xF7),
            surface: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            surface_2: Color32::from_rgb(0xF7, 0xF8, 0xFA),
            surface_3: Color32::from_rgb(0xEC, 0xEE, 0xF3),
            elevated: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            text: Color32::from_rgb(0x17, 0x1A, 0x21),
            text_2: Color32::from_rgb(0x66, 0x70, 0x80),
            text_3: Color32::from_rgb(0x8B, 0x93, 0xA0),
            border: Color32::from_rgb(0xE1, 0xE4, 0xEA),
            border_strong: Color32::from_rgb(0xCF, 0xD4, 0xDD),
            accent: Color32::from_rgb(0x59, 0x69, 0xEE),
            accent_hover: Color32::from_rgb(0x4B, 0x5C, 0xE1),
            accent_soft: rgba(89, 105, 238, 28),
            green: Color32::from_rgb(0x25, 0x87, 0x53),
            green_soft: rgba(37, 135, 83, 26),
            orange: Color32::from_rgb(0xB8, 0x67, 0x11),
            orange_soft: rgba(184, 103, 17, 26),
            red: Color32::from_rgb(0xC9, 0x3E, 0x49),
            red_soft: rgba(201, 62, 73, 26),
            blue: Color32::from_rgb(0x2A, 0x6F, 0xB8),
            blue_soft: rgba(42, 111, 184, 30),
            modal_shadow: egui::epaint::Shadow {
                offset: [0, 24],
                blur: 70,
                spread: 0,
                color: rgba(24, 32, 50, 51),
            },
            overlay: rgba(24, 28, 36, 107),
            card_shadow: egui::epaint::Shadow {
                offset: [0, 8],
                blur: 22,
                spread: 0,
                color: rgba(24, 32, 50, 30),
            },
            accent_shadow: egui::epaint::Shadow {
                offset: [0, 4],
                blur: 14,
                spread: 0,
                color: rgba(89, 105, 238, 55),
            },
        }
    }

    pub fn for_mode(dark: bool) -> Self {
        if dark {
            Self::dark()
        } else {
            Self::light()
        }
    }
}

/// Returns the tokens installed by the latest [`apply`] call.
pub fn current(ctx: &Context) -> Tokens {
    ctx.data(|data| data.get_temp::<Tokens>(Id::new(TOKENS_ID)))
        .unwrap_or_else(Tokens::dark)
}

/// Applies the theme globally and caches its tokens for this frame.
pub fn apply(ctx: &Context, dark: bool) {
    let tokens = Tokens::for_mode(dark);
    ctx.data_mut(|data| data.insert_temp(Id::new(TOKENS_ID), tokens));

    let mut visuals = if dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    visuals.dark_mode = dark;
    visuals.panel_fill = tokens.app_bg;
    visuals.window_fill = tokens.elevated;
    visuals.window_stroke = Stroke::new(1.0_f32, tokens.border_strong);
    visuals.window_corner_radius = CornerRadius::same(14);
    visuals.window_shadow = tokens.modal_shadow;
    visuals.menu_corner_radius = CornerRadius::same(9);
    visuals.popup_shadow = tokens.card_shadow;
    visuals.faint_bg_color = tokens.surface_2;
    visuals.extreme_bg_color = tokens.surface_2;
    visuals.code_bg_color = tokens.surface_3;
    visuals.warn_fg_color = tokens.orange;
    visuals.error_fg_color = tokens.red;
    visuals.hyperlink_color = tokens.accent;
    visuals.override_text_color = Some(tokens.text_2);
    visuals.selection.bg_fill = tokens.accent_soft;
    visuals.selection.stroke = Stroke::new(1.0_f32, tokens.accent);
    visuals.text_cursor.stroke = Stroke::new(1.5_f32, tokens.accent);
    visuals.striped = false;
    visuals.button_frame = true;
    visuals.collapsing_header_frame = false;
    visuals.indent_has_left_vline = false;
    visuals.interact_cursor = Some(egui::CursorIcon::PointingHand);

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = tokens.surface;
    widgets.noninteractive.weak_bg_fill = tokens.surface;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, tokens.border);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0_f32, tokens.text_2);
    widgets.noninteractive.corner_radius = CornerRadius::same(8);

    widgets.inactive.bg_fill = tokens.surface_3;
    widgets.inactive.weak_bg_fill = tokens.surface_2;
    widgets.inactive.bg_stroke = Stroke::new(1.0_f32, tokens.border_strong);
    widgets.inactive.fg_stroke = Stroke::new(1.0_f32, tokens.text_2);
    widgets.inactive.corner_radius = CornerRadius::same(8);

    widgets.hovered.bg_fill = tokens.surface_3;
    widgets.hovered.weak_bg_fill = tokens.surface_3;
    widgets.hovered.bg_stroke = Stroke::new(1.0_f32, tokens.border_strong);
    widgets.hovered.fg_stroke = Stroke::new(1.0_f32, tokens.text);
    widgets.hovered.corner_radius = CornerRadius::same(8);

    widgets.active.bg_fill = tokens.accent_soft;
    widgets.active.weak_bg_fill = tokens.accent_soft;
    widgets.active.bg_stroke = Stroke::new(1.0_f32, tokens.accent);
    widgets.active.fg_stroke = Stroke::new(1.0_f32, tokens.text);
    widgets.active.corner_radius = CornerRadius::same(8);

    widgets.open.bg_fill = tokens.surface_3;
    widgets.open.weak_bg_fill = tokens.surface_3;
    widgets.open.bg_stroke = Stroke::new(1.0_f32, tokens.border_strong);
    widgets.open.fg_stroke = Stroke::new(1.0_f32, tokens.text);
    widgets.open.corner_radius = CornerRadius::same(8);

    visuals.image_loading_spinners = false;
    ctx.set_visuals(visuals);

    let mut style = Style {
        animation_time: 0.14,
        ..(*ctx.style()).clone()
    };
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 7.0);
    style.spacing.interact_size = egui::vec2(34.0, 34.0);
    style.spacing.indent = 16.0;
    style.spacing.menu_margin = Margin::same(5);
    style.spacing.window_margin = Margin::same(0);
    style.spacing.scroll.bar_width = 8.0;
    style.spacing.scroll.floating = true;
    style.spacing.scroll.bar_inner_margin = 4.0;
    style.spacing.scroll.bar_outer_margin = 2.0;
    style.spacing.scroll.floating_width = 5.0;
    style.spacing.scroll.dormant_background_opacity = 0.0;
    style.spacing.scroll.active_background_opacity = 0.18;
    style.spacing.scroll.interact_background_opacity = 0.32;

    style.text_styles = [
        (
            TextStyle::Heading,
            FontId::new(25.0, fonts::family(fonts::BOLD)),
        ),
        (
            TextStyle::Body,
            FontId::new(13.0, fonts::family(fonts::REGULAR)),
        ),
        (
            TextStyle::Button,
            FontId::new(11.0, fonts::family(fonts::SEMIBOLD)),
        ),
        (
            TextStyle::Small,
            FontId::new(9.0, fonts::family(fonts::REGULAR)),
        ),
        (
            TextStyle::Monospace,
            FontId::new(11.0, FontFamily::Monospace),
        ),
    ]
    .into();

    ctx.set_style(style);
}
