//! Abstract game art.
//!
//! No cover art is used. Each game gets a deterministic gradient, an initial,
//! two decorative circles and a dot grid, generated once into a texture and
//! cached by size/palette/scale.

use std::collections::HashMap;

use eframe::egui::{
    self, Color32, Context, Id, Mesh, Pos2, Rect, TextureHandle, TextureOptions, Vec2,
};

use super::text::{self, Weight};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Palette {
    Violet,
    Orange,
    Blue,
    Red,
    Mint,
    Gold,
    Brand,
}

impl Palette {
    pub fn colors(self) -> [Color32; 3] {
        match self {
            Self::Violet => [
                Color32::from_rgb(0x22, 0x29, 0x47),
                Color32::from_rgb(0x67, 0x61, 0xA8),
                Color32::from_rgb(0x25, 0x28, 0x44),
            ],
            Self::Orange => [
                Color32::from_rgb(0x52, 0x33, 0x26),
                Color32::from_rgb(0xC0, 0x75, 0x3F),
                Color32::from_rgb(0x53, 0x3C, 0x2F),
            ],
            Self::Blue => [
                Color32::from_rgb(0x17, 0x36, 0x53),
                Color32::from_rgb(0x34, 0x81, 0xBC),
                Color32::from_rgb(0x17, 0x2D, 0x47),
            ],
            Self::Red => [
                Color32::from_rgb(0x5B, 0x26, 0x30),
                Color32::from_rgb(0xBD, 0x4C, 0x52),
                Color32::from_rgb(0x57, 0x25, 0x2D),
            ],
            Self::Mint => [
                Color32::from_rgb(0x17, 0x3B, 0x3D),
                Color32::from_rgb(0x40, 0x83, 0x7F),
                Color32::from_rgb(0x1B, 0x41, 0x42),
            ],
            Self::Gold => [
                Color32::from_rgb(0x49, 0x3B, 0x1F),
                Color32::from_rgb(0xAA, 0x86, 0x40),
                Color32::from_rgb(0x4A, 0x3D, 0x22),
            ],
            Self::Brand => [
                Color32::from_rgb(0x83, 0x90, 0xFF),
                Color32::from_rgb(0x6C, 0x7C, 0xFF),
                Color32::from_rgb(0x52, 0x62, 0xEA),
            ],
        }
    }
}

pub fn palette_for(slug: &str) -> Palette {
    match slug {
        "zelda64-recomp" | "2ship2harkinian" => Palette::Violet,
        "banjo-recomp" => Palette::Orange,
        "ship-of-harkinian" => Palette::Gold,
        "devilutionx" => Palette::Red,
        "openrct2" => Palette::Mint,
        _ => {
            let mut hash = 0u32;
            for byte in slug.bytes() {
                hash = hash.wrapping_mul(31).wrapping_add(u32::from(byte));
            }
            match hash % 6 {
                0 => Palette::Violet,
                1 => Palette::Orange,
                2 => Palette::Blue,
                3 => Palette::Red,
                4 => Palette::Mint,
                _ => Palette::Gold,
            }
        }
    }
}

pub fn initial_for(slug: &str, title: &str) -> String {
    match slug {
        "zelda64-recomp" | "2ship2harkinian" => return "M".to_owned(),
        "ship-of-harkinian" => return "O".to_owned(),
        "banjo-recomp" => return "B".to_owned(),
        "devilutionx" => return "D".to_owned(),
        "openrct2" => return "R".to_owned(),
        _ => {}
    }
    if let Some((_, after)) = title.rsplit_once(':') {
        if let Some(ch) = after.chars().find(|ch| ch.is_alphanumeric()) {
            return ch.to_uppercase().to_string();
        }
    }
    title
        .chars()
        .find(|ch| ch.is_alphanumeric())
        .map(|ch| ch.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_owned())
}

#[derive(Default, Clone)]
struct ArtCache {
    textures: HashMap<ArtKey, TextureHandle>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ArtKey {
    palette: Palette,
    width: u32,
    height: u32,
    radius: u8,
    scale: u32,
}

fn texture(ctx: &Context, key: ArtKey) -> TextureHandle {
    // Never call `load_texture` (which reads the context lock) from inside
    // `data_mut` (which holds the same write lock). That deadlocks the UI.
    let cached = ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<ArtCache>(Id::new("recomp.art.cache"))
            .textures
            .get(&key)
            .cloned()
    });
    if let Some(handle) = cached {
        return handle;
    }

    let image = generate(key);
    let handle = ctx.load_texture(
        format!("art-{:?}-{}x{}", key.palette, key.width, key.height),
        image,
        TextureOptions::LINEAR,
    );
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<ArtCache>(Id::new("recomp.art.cache"))
            .textures
            .insert(key, handle.clone());
    });
    handle
}

fn generate(key: ArtKey) -> egui::ColorImage {
    let scale = key.scale as f32 / 100.0;
    let width = ((key.width as f32) * scale).ceil().max(1.0) as usize;
    let height = ((key.height as f32) * scale).ceil().max(1.0) as usize;
    let radius = key.radius as f32 * scale;
    let [start, middle, end] = key.palette.colors();
    let decorated = key.palette != Palette::Brand;

    let mut image = egui::ColorImage::new([width, height], Color32::TRANSPARENT);
    let dot_step = 7.0 * scale;
    let dot_radius = 0.6 * scale;
    let stroke = 1.4 * scale;

    let top_circle = (
        Pos2::new(width as f32 * 1.02, height as f32 * -0.12),
        (width.max(height) as f32) * 0.58,
    );
    let bottom_circle = (
        Pos2::new(width as f32 * 0.04, height as f32 * 1.08),
        (width.max(height) as f32) * 0.48,
    );

    for y in 0..height {
        for x in 0..width {
            let u = (x as f32 + 0.5) / width as f32;
            let v = (y as f32 + 0.5) / height as f32;
            let t = (u * 0.22 + v * 0.78).clamp(0.0, 1.0);
            let mut color = if t <= 0.6 {
                mix(start, middle, t / 0.6)
            } else {
                mix(middle, end, (t - 0.6) / 0.4)
            };

            if decorated {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                for (center, circle_radius) in [top_circle, bottom_circle] {
                    let distance = center.distance(Pos2::new(px, py));
                    if (distance - circle_radius).abs() <= stroke {
                        color = blend(color, Color32::WHITE, 0.15);
                    }
                }
                let dx = px.rem_euclid(dot_step) - dot_step / 2.0;
                let dy = py.rem_euclid(dot_step) - dot_step / 2.0;
                if (dx * dx + dy * dy).sqrt() <= dot_radius {
                    color = blend(color, Color32::WHITE, 0.12);
                }
            }

            let mask = rounded_mask(
                x as f32 + 0.5,
                y as f32 + 0.5,
                width as f32,
                height as f32,
                radius,
            );
            image.pixels[y * width + x] = color.linear_multiply(mask);
        }
    }
    image
}

fn rounded_mask(x: f32, y: f32, width: f32, height: f32, radius: f32) -> f32 {
    if radius <= 0.5 {
        return 1.0;
    }
    let cx = x.clamp(radius, width - radius);
    let cy = y.clamp(radius, height - radius);
    let dx = x - cx;
    let dy = y - cy;
    let distance = (dx * dx + dy * dy).sqrt() - radius;
    (0.5 - distance).clamp(0.0, 1.0)
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let lerp = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * t) as u8;
    Color32::from_rgb(lerp(a.r(), b.r()), lerp(a.g(), b.g()), lerp(a.b(), b.b()))
}

fn blend(base: Color32, overlay: Color32, alpha: f32) -> Color32 {
    let alpha = alpha.clamp(0.0, 1.0);
    let lerp = |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * alpha) as u8;
    Color32::from_rgb(
        lerp(base.r(), overlay.r()),
        lerp(base.g(), overlay.g()),
        lerp(base.b(), overlay.b()),
    )
}

/// Paints the generated gradient inside `rect`.
pub fn paint(ui: &egui::Ui, rect: Rect, palette: Palette, radius: u8) {
    let scale = (ui.ctx().pixels_per_point() * 100.0).round() as u32;
    let key = ArtKey {
        palette,
        width: rect.width().round().max(1.0) as u32,
        height: rect.height().round().max(1.0) as u32,
        radius,
        scale,
    };
    let handle = texture(ui.ctx(), key);
    ui.painter().add(egui::Shape::image(
        handle.id(),
        rect,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    ));
}

pub struct Initial<'a> {
    pub text: &'a str,
    pub size: f32,
    pub opacity: f32,
    pub offset: Vec2,
}

pub fn paint_initial(ui: &egui::Ui, rect: Rect, initial: Initial<'_>) {
    let font = text::font(initial.size, Weight::Bold);
    let color = Color32::WHITE.linear_multiply(initial.opacity);
    let shadow = Color32::BLACK.linear_multiply(initial.opacity * 0.28);
    let galley = ui
        .painter()
        .layout_no_wrap(initial.text.to_owned(), font, color);
    let position = rect.center() - galley.size() / 2.0 + initial.offset;
    let shadow_galley = ui.painter().layout_no_wrap(
        initial.text.to_owned(),
        text::font(initial.size, Weight::Bold),
        shadow,
    );
    ui.painter()
        .galley(position + Vec2::new(0.0, 1.5), shadow_galley, shadow);
    ui.painter().galley(position, galley, color);
}

/// Paints the brand gradient.
pub fn brand(ui: &egui::Ui, rect: Rect) {
    paint(ui, rect, Palette::Brand, 10);
}

/// Vertical black gradient, used at the bottom of heroes.
pub fn bottom_veil(painter: &egui::Painter, rect: Rect, top_alpha: f32, bottom_alpha: f32) {
    let mut mesh = Mesh::default();
    let top = Color32::from_black_alpha((top_alpha * 255.0) as u8);
    let bottom = Color32::from_black_alpha((bottom_alpha * 255.0) as u8);
    let index = mesh.vertices.len() as u32;
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(index, index + 1, index + 2);
    mesh.add_triangle(index, index + 2, index + 3);
    painter.add(egui::Shape::mesh(mesh));
}

/// Horizontal black gradient, used on the game detail hero.
pub fn left_veil(painter: &egui::Painter, rect: Rect, left_alpha: f32, right_alpha: f32) {
    let mut mesh = Mesh::default();
    let left = Color32::from_black_alpha((left_alpha * 255.0) as u8);
    let right = Color32::from_black_alpha((right_alpha * 255.0) as u8);
    let index = mesh.vertices.len() as u32;
    mesh.colored_vertex(rect.left_top(), left);
    mesh.colored_vertex(rect.right_top(), right);
    mesh.colored_vertex(rect.right_bottom(), right);
    mesh.colored_vertex(rect.left_bottom(), left);
    mesh.add_triangle(index, index + 1, index + 2);
    mesh.add_triangle(index, index + 2, index + 3);
    painter.add(egui::Shape::mesh(mesh));
}
