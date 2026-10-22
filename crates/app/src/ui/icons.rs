//! Lucide glyphs vendored as a font, so icons tint and scale like text.
//!
//! Codepoints come from `lucide-static` 1.61.0 (ISC).

use eframe::egui::{self, Color32, FontId, Painter, Pos2, Response, RichText, Ui};

use super::fonts;

pub const GAMEPAD_2: char = '\u{e0df}';
pub const BOOK_OPEN: char = '\u{e05f}';
pub const LIBRARY: char = '\u{e100}';
pub const SETTINGS: char = '\u{e154}';
pub const PACKAGE_OPEN: char = '\u{e2cc}';
pub const WIFI_OFF: char = '\u{e1af}';
pub const SUN: char = '\u{e178}';
pub const MOON: char = '\u{e11e}';
pub const MINUS: char = '\u{e11c}';
pub const X: char = '\u{e1b2}';
pub const SEARCH: char = '\u{e151}';
pub const REFRESH_CW: char = '\u{e145}';
pub const GRID_2X2: char = '\u{e4ff}';
pub const LIST: char = '\u{e106}';
pub const MONITOR: char = '\u{e11d}';
pub const LOCK_KEYHOLE: char = '\u{e531}';
pub const ARCHIVE: char = '\u{e041}';
pub const FILE_QUESTION: char = '\u{e322}';
pub const SHIELD_CHECK: char = '\u{e1ff}';
pub const EXTERNAL_LINK: char = '\u{e0b9}';
pub const DOWNLOAD: char = '\u{e0b2}';
pub const CLOCK_3: char = '\u{e250}';
pub const HARD_DRIVE: char = '\u{e0ed}';
pub const PLAY: char = '\u{e13c}';
pub const TRASH_2: char = '\u{e18e}';
pub const CHEVRON_RIGHT: char = '\u{e06f}';
pub const FOLDER: char = '\u{e0d7}';
pub const UPLOAD: char = '\u{e19e}';
pub const WIFI: char = '\u{e1ae}';
pub const GLOBE_2: char = '\u{e1f3}';
pub const NETWORK: char = '\u{e125}';
pub const RADAR: char = '\u{e497}';
pub const FILE_SEARCH: char = '\u{e0cb}';
pub const CHECK: char = '\u{e06c}';
pub const LOADER_CIRCLE: char = '\u{e10a}';
pub const X_CIRCLE: char = '\u{e084}';
pub const GRIP_VERTICAL: char = '\u{e0eb}';
pub const CHEVRON_DOWN: char = '\u{e06d}';
pub const CHEVRON_UP: char = '\u{e070}';
pub const SQUARE: char = '\u{e167}';
pub const ARROW_LEFT: char = '\u{e048}';
pub const CIRCLE_ALERT: char = '\u{e077}';
pub const CIRCLE_CHECK: char = '\u{e226}';
pub const TRIANGLE_ALERT: char = '\u{e193}';
pub const FOLDER_OPEN: char = '\u{e247}';
pub const ROTATE_CW: char = '\u{e149}';

pub fn font(size: f32) -> FontId {
    FontId::new(size, fonts::family(fonts::ICONS))
}

pub fn rich(glyph: char, size: f32, color: Color32) -> RichText {
    RichText::new(glyph).font(font(size)).color(color)
}

pub fn icon(ui: &mut Ui, glyph: char, size: f32, color: Color32) -> Response {
    ui.add(egui::Label::new(rich(glyph, size, color)))
}

/// Paints a glyph centred on `center`.
pub fn paint(painter: &Painter, center: Pos2, glyph: char, size: f32, color: Color32) {
    let galley = painter.layout_no_wrap(glyph.to_string(), font(size), color);
    let position = center - galley.size() / 2.0;
    painter.galley(position, galley, color);
}

/// Paints a glyph left-aligned in `rect`, vertically centred.
pub fn paint_left(painter: &Painter, rect: egui::Rect, glyph: char, size: f32, color: Color32) {
    let galley = painter.layout_no_wrap(glyph.to_string(), font(size), color);
    let position = egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0);
    painter.galley(position, galley, color);
}
