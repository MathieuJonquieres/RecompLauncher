//! Bundled fonts: Inter (OFL) and the Lucide icon font (ISC).
//!
//! egui selects a font by family, not by weight, so each Inter weight gets its
//! own family. [`text`](super::text) exposes the matching helpers.

use std::sync::Arc;

use eframe::egui::{Context, FontData, FontDefinitions, FontFamily};

pub const REGULAR: &str = "inter-regular";
pub const MEDIUM: &str = "inter-medium";
pub const SEMIBOLD: &str = "inter-semibold";
pub const BOLD: &str = "inter-bold";
pub const ICONS: &str = "lucide";

const INTER_REGULAR: &[u8] = include_bytes!("../../../../resources/fonts/Inter-Regular.ttf");
const INTER_MEDIUM: &[u8] = include_bytes!("../../../../resources/fonts/Inter-Medium.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../../../../resources/fonts/Inter-SemiBold.ttf");
const INTER_BOLD: &[u8] = include_bytes!("../../../../resources/fonts/Inter-Bold.ttf");
const LUCIDE: &[u8] = include_bytes!("../../../../resources/fonts/lucide.ttf");

/// Installs the bundled fonts. Safe to call more than once.
pub fn install(ctx: &Context) {
    let mut definitions = FontDefinitions::default();

    for (name, bytes) in [
        (REGULAR, INTER_REGULAR),
        (MEDIUM, INTER_MEDIUM),
        (SEMIBOLD, INTER_SEMIBOLD),
        (BOLD, INTER_BOLD),
        (ICONS, LUCIDE),
    ] {
        definitions
            .font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    }

    let fallback = definitions
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();

    let mut proportional = vec![REGULAR.to_owned()];
    proportional.extend(fallback);
    definitions
        .families
        .insert(FontFamily::Proportional, proportional);

    for name in [REGULAR, MEDIUM, SEMIBOLD, BOLD] {
        definitions
            .families
            .insert(FontFamily::Name(Arc::from(name)), vec![name.to_owned()]);
    }
    definitions
        .families
        .insert(FontFamily::Name(Arc::from(ICONS)), vec![ICONS.to_owned()]);

    ctx.set_fonts(definitions);
}

pub fn family(name: &'static str) -> FontFamily {
    FontFamily::Name(Arc::from(name))
}
