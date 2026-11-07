//! The editor's look, after Godot 4's default editor theme: dark slate docks over a darker
//! background, with a light blue accent.

use wingine::{
    graphics::Color,
    gui::egui::{self, Color32, CornerRadius, Stroke},
};

/// Godot's accent: selections, the active tool, focused fields.
pub const ACCENT: Color32 = Color32::from_rgb(0x70, 0xba, 0xfa);
/// Behind the docks, between them.
pub const BACKGROUND: Color32 = Color32::from_rgb(0x1d, 0x22, 0x29);
/// The docks themselves.
pub const PANEL: Color32 = Color32::from_rgb(0x26, 0x2c, 0x36);
/// Buttons and fields at rest.
pub const WIDGET: Color32 = Color32::from_rgb(0x36, 0x3d, 0x4a);
/// The viewport around the scene.
pub const VIEWPORT: Color = Color::rgb8(0x40, 0x40, 0x46);
/// Lines of the viewport's grid.
pub const GRID: Color = Color::rgba8(0xff, 0xff, 0xff, 0x14);
/// The outline of the game's canvas in the viewport, Godot's for the viewport's rectangle.
pub const CANVAS: Color32 = Color32::from_rgb(0xb2, 0x8c, 0xf0);
/// Keys on the timeline, and the fields a clip keys in the Inspector.
pub const KEY: Color32 = Color32::from_rgb(0xf2, 0xc1, 0x4e);
/// The scene's x axis and y axis through its origin.
pub const X_AXIS: Color = Color::rgba8(0xf0, 0x60, 0x70, 0x80);
pub const Y_AXIS: Color = Color::rgba8(0x80, 0xe0, 0x70, 0x80);

/// Behind scripts.
pub const CODE_BACKGROUND: Color32 = Color32::from_rgb(0x1d, 0x22, 0x29);
/// Plain text in scripts, and names.
pub const CODE_TEXT: Color32 = Color32::from_rgb(0xcd, 0xcf, 0xd2);
/// Line numbers, and the line the cursor is on, behind it.
pub const LINE_NUMBER: Color32 = Color32::from_rgb(0x76, 0x79, 0x7e);
pub const CURRENT_LINE: Color32 = Color32::from_rgba_premultiplied(8, 8, 8, 8);
/// Problems in scripts: errors, and warnings and the rest.
pub const ERROR: Color32 = Color32::from_rgb(0xff, 0x78, 0x6b);
pub const WARNING: Color32 = Color32::from_rgb(0xff, 0xde, 0x66);

/// Godot's color for a kind of word in scripts.
pub fn code_color(kind: crate::luau::Kind) -> Color32 {
    use crate::luau::Kind;
    match kind {
        Kind::Keyword | Kind::Literal => Color32::from_rgb(0xff, 0x70, 0x85),
        Kind::Control => Color32::from_rgb(0xff, 0x8c, 0xcc),
        Kind::Number => Color32::from_rgb(0xa1, 0xff, 0xe0),
        Kind::String => Color32::from_rgb(0xff, 0xed, 0xa1),
        Kind::Comment => Color32::from_rgb(0x80, 0x84, 0x8a),
        Kind::Global => Color32::from_rgb(0x8f, 0xff, 0xdb),
        Kind::Function => Color32::from_rgb(0x57, 0xb3, 0xff),
        Kind::Name => CODE_TEXT,
        Kind::Symbol => Color32::from_rgb(0xab, 0xc9, 0xff),
    }
}

pub fn apply(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL;
    visuals.extreme_bg_color = BACKGROUND;
    visuals.faint_bg_color = Color32::from_rgb(0x2b, 0x32, 0x3d);
    visuals.code_bg_color = BACKGROUND;
    visuals.window_stroke = Stroke::new(1.0, Color32::from_rgb(0x3d, 0x45, 0x52));
    visuals.window_corner_radius = CornerRadius::same(4);
    visuals.menu_corner_radius = CornerRadius::same(4);
    visuals.hyperlink_color = ACCENT;
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.35);
    visuals.selection.stroke = Stroke::new(1.0, ACCENT);
    visuals.text_cursor.stroke.color = ACCENT;

    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = PANEL;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0x34, 0x3b, 0x47));
    for (visuals, fill) in [
        (&mut widgets.inactive, WIDGET),
        (&mut widgets.hovered, Color32::from_rgb(0x41, 0x4a, 0x59)),
        (&mut widgets.active, Color32::from_rgb(0x4b, 0x56, 0x68)),
        (&mut widgets.open, Color32::from_rgb(0x41, 0x4a, 0x59)),
    ] {
        visuals.bg_fill = fill;
        visuals.weak_bg_fill = fill;
        visuals.corner_radius = CornerRadius::same(3);
    }
    widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
    widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);

    ctx.set_visuals_of(egui::Theme::Dark, visuals);
    ctx.set_theme(egui::Theme::Dark);
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(6.0, 4.0);
        style.spacing.button_padding = egui::vec2(6.0, 3.0);
    });
}

/// The glyph for a kind of node, from egui's built-in fonts, as Godot shows an icon.
pub fn kind_icon(kind: &str) -> &'static str {
    match kind {
        "node" => "○",
        "sprite" => "🖼",
        "rect" => "■",
        "text" => "🗛",
        "area" => "⛶",
        "stack" => "▤",
        "panel" => "▣",
        "tilemap" => "🗺",
        "tile_layer" => "☰",
        "instance" => "⛓",
        "view3d" => "📷",
        "mesh" => "⬛",
        "model" => "🗿",
        "billboard" => "👤",
        "scene" => "🎬",
        _ => "?",
    }
}

/// The glyph for a file in the FileSystem dock, by its name.
pub fn file_icon(name: &str) -> &'static str {
    if name.ends_with(".scene.luau") {
        "🎬"
    } else if name.ends_with(".tileset.luau") {
        "🗺"
    } else if name.ends_with(".luau") {
        "📜"
    } else if name.ends_with(".glb") || name.ends_with(".gltf") {
        "🗿"
    } else if name.ends_with(".png") {
        "🖼"
    } else if name.ends_with(".ogg") || name.ends_with(".wav") {
        "🔊"
    } else {
        "📄"
    }
}
