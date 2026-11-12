//! Screenshot tests: each test renders a scene headlessly and compares it with a reference PNG in
//! `tests/screenshots/`.
//!
//! - `UPDATE_SCREENSHOTS=1 cargo test --test screenshots` writes new references for scenes that
//!   are missing one or no longer match. Review the changed PNGs before committing them.
//! - On a mismatch, the rendered frame and a diff image (mismatched pixels in magenta) are written
//!   to `target/tmp/screenshots/`.
//!
//! Matching, by `Image::compare`, allows a small per-channel difference, so references rendered on
//! one GPU pass on others, including the software rasterizers used in CI.

use std::{env, fs, path::Path, rc::Rc};

use image::{RgbaImage, imageops};
use wingine::{
    Context,
    graphics::{
        Align, Blend, Camera, Camera3d, Canvas, Color, Fog, Font, Light, Lighting, Mesh, MeshData,
        MeshVertex, Model, Resolution, Scaling, SpriteSheet, Texture, View3d,
    },
    headless::{Comparison, Headless, Image},
    math::{Affine, Affine3, Rect},
    particles::{ParticleSettings, Particles},
    scene::{Assets, Scene, SceneFile, Shape, TilesetFile},
    tilemap::{Tile, Tilemap, Tileset},
};

/// Largest per-channel difference for a pixel to still match. GPUs and software rasterizers round
/// sRGB conversion and blending slightly differently.
const CHANNEL_TOLERANCE: u8 = 2;

const TILESET: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/kenney_1-bit-pack_transparent-packed.png"
);

/// The examples' folder, which scenes' model and font files are in.
const EXAMPLES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples");

/// The RPG's frame for its panels: 5 texels square, a border a texel wide, its corners cut.
const FRAME: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/rpg/frame.png");

#[test]
fn clear() {
    let mut headless = headless();
    let frame = headless
        .render(16, 16, |canvas| canvas.clear(Color::rgb8(0x33, 0x66, 0x99)))
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "clear", frame);
}

#[test]
fn fill_rects() {
    let mut headless = headless();
    let frame = headless
        .render(64, 64, |canvas| {
            canvas.clear(Color::rgb8(0x20, 0x20, 0x28));
            // Later shapes draw on top.
            canvas.fill_rect(
                Rect::new(8.0, 8.0, 32.0, 32.0),
                Color::rgb8(0xd0, 0x40, 0x40),
            );
            canvas.fill_rect(
                Rect::new(24.0, 24.0, 32.0, 32.0),
                Color::rgb8(0x40, 0xb0, 0x50),
            );
            // Translucent, blended over the background and both squares.
            canvas.fill_rect(
                Rect::new(0.0, 36.0, 64.0, 12.0),
                Color::rgba(1.0, 1.0, 1.0, 0.5),
            );
            // One-pixel lines along the edges: the canvas must cover every pixel exactly.
            canvas.fill_rect(
                Rect::new(0.0, 0.0, 64.0, 1.0),
                Color::rgb8(0xf0, 0xc0, 0x30),
            );
            canvas.fill_rect(
                Rect::new(63.0, 0.0, 1.0, 64.0),
                Color::rgb8(0x30, 0xa0, 0xf0),
            );
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "fill_rects", frame);
}

/// Outlines, lines and circles: a thin outline, and a thick translucent one over a fill, which
/// must be even at its corners; a line along a row of pixels, a thin diagonal one and a thick
/// translucent one; a circle inside a translucent ring; and a circle in an outline through a
/// zoomed camera, both twice the size.
///
/// The diagonal lines and the circles are off the pixel grid by fractions, so no pixel's center
/// lands exactly on an edge, where GPUs may differ.
#[test]
fn outlines_lines_and_circles() {
    let mut headless = headless();
    let frame = headless
        .render(96, 64, |canvas| {
            canvas.clear(Color::rgb8(0x20, 0x20, 0x28));
            let yellow = Color::rgb8(0xf0, 0xc0, 0x30);
            let half_white = Color::rgba(1.0, 1.0, 1.0, 0.5);
            canvas.outline_rect(Rect::new(4.0, 4.0, 24.0, 16.0), 1.0, yellow);
            canvas.fill_rect(
                Rect::new(32.0, 4.0, 24.0, 16.0),
                Color::rgb8(0xd0, 0x40, 0x40),
            );
            canvas.outline_rect(Rect::new(32.0, 4.0, 24.0, 16.0), 3.0, half_white);

            let cyan = Color::rgb8(0x30, 0xa0, 0xf0);
            canvas.draw_line([4.0, 26.5], [60.0, 26.5], 1.0, cyan);
            canvas.draw_line([4.3, 31.2], [40.1, 58.7], 1.0, Color::WHITE);
            let amber = Color::rgba(1.0, 0.8, 0.2, 0.6);
            canvas.draw_line([44.2, 36.1], [60.3, 58.4], 5.0, amber);

            let green = Color::rgb8(0x40, 0xb0, 0x50);
            canvas.fill_circle([78.3, 16.2], 9.0, green);
            canvas.outline_circle([78.3, 16.2], 13.0, 2.0, half_white);

            // World units are two canvas pixels, with the world's origin at the canvas's.
            canvas.set_camera(Some(Camera {
                zoom: 2.0,
                ..Camera::new(24.0, 16.0)
            }));
            canvas.outline_rect(Rect::new(34.0, 19.0, 11.0, 11.0), 1.0, yellow);
            canvas.fill_circle([39.6, 24.6], 4.1, cyan);
        })
        .unwrap();
    assert_screenshot(
        &headless.adapter_name(),
        "outlines_lines_and_circles",
        frame,
    );
}

#[test]
fn sprites() {
    let mut headless = headless();
    let sheet = SpriteSheet::new(headless.context().load_texture(TILESET).unwrap(), 16, 16);
    let hero = sheet.sprite(27, 0);
    let tree = sheet.sprite(0, 1);
    let heart = sheet.sprite_at(10 * sheet.columns() + 39);
    let frame = headless
        .render(96, 48, |canvas| {
            canvas.clear(Color::rgb8(0x1b, 0x26, 0x1e));
            let row = [
                hero.clone(),
                hero.clone().flipped_x(),
                hero.clone().flipped_y(),
                hero.clone().flipped_x().flipped_y(),
            ];
            for (i, sprite) in row.iter().enumerate() {
                let dest = Rect::new(i as f32 * 16.0, 0.0, 16.0, 16.0);
                canvas.draw_sprite(sprite, dest, Color::WHITE);
            }
            canvas.draw_sprite(&hero, Rect::new(64.0, 0.0, 16.0, 16.0), Color::RED);
            canvas.draw_sprite(&heart, Rect::new(80.0, 0.0, 16.0, 16.0), Color::RED);
            // Scaled up, which must stay pixel-crisp.
            canvas.draw_sprite(&hero, Rect::new(0.0, 16.0, 32.0, 32.0), Color::WHITE);
            let tree_tint = Color::rgb8(0x6a, 0xa8, 0x4f);
            canvas.draw_sprite(&tree, Rect::new(32.0, 16.0, 32.0, 32.0), tree_tint);
            // Stretched to a non-square size.
            canvas.draw_sprite(&tree, Rect::new(64.0, 16.0, 32.0, 16.0), tree_tint);
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "sprites", frame);
}

/// Sprites rotated about their centers: unturned, a quarter turn, a half turn and an eighth back,
/// then one stretched before turning, and one through a zoomed camera.
///
/// Where a pixel's center lands exactly on the edge between two texels, GPUs pick either, so the
/// scene avoids that: the eighth turn is off the pixel grid by a fraction, and nothing shrinks.
#[test]
fn rotated_sprites() {
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, FRAC_PI_6, PI};

    let mut headless = headless();
    let sheet = SpriteSheet::new(headless.context().load_texture(TILESET).unwrap(), 16, 16);
    let hero = sheet.sprite(27, 0);
    let frame = headless
        .render(96, 64, |canvas| {
            canvas.clear(Color::rgb8(0x1b, 0x26, 0x1e));
            for (x, angle) in [
                (4.0, 0.0),
                (24.0, FRAC_PI_2),
                (44.0, PI),
                (66.25, -FRAC_PI_4),
            ] {
                let dest = Rect::new(x, 4.0, 16.0, 16.0);
                canvas.draw_sprite_rotated(&hero, dest, angle, Color::WHITE);
            }
            // 32x16, standing up as 16x32 about its center.
            let dest = Rect::new(4.0, 32.0, 32.0, 16.0);
            canvas.draw_sprite_rotated(&hero, dest, FRAC_PI_2, Color::RED);
            // World units are two canvas pixels, with the world's origin at the canvas's.
            canvas.set_camera(Some(Camera {
                zoom: 2.0,
                ..Camera::new(24.0, 16.0)
            }));
            let dest = Rect::new(26.0, 16.0, 12.0, 12.0);
            canvas.draw_sprite_rotated(&hero, dest, FRAC_PI_6, Color::GREEN);
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "rotated_sprites", frame);
}

/// Sprites and a fill placed by transforms: a sprite standing on its feet and tipped over about
/// them, one mirrored and one turned upside down about their middles, one doubled inside a parent
/// turned a quarter, a translucent fill turned off the pixel grid, and a sprite through a zoomed
/// camera.
///
/// Every sprite is drawn at one texel per pixel or larger, on whole pixels, so no pixel's center
/// lands on the edge between two texels.
#[test]
fn transformed_sprites() {
    use std::f32::consts::{FRAC_PI_2, PI};

    let mut headless = headless();
    let sheet = SpriteSheet::new(headless.context().load_texture(TILESET).unwrap(), 16, 16);
    let hero = sheet.sprite(27, 0);
    let frame = headless
        .render(128, 64, |canvas| {
            canvas.clear(Color::rgb8(0x1b, 0x26, 0x1e));
            // The middle of its feet at the origin.
            let feet = Rect::new(-8.0, -16.0, 16.0, 16.0);
            let standing = Affine::from_translation([12.0, 20.0]);
            canvas.draw_sprite_transformed(&hero, feet, standing, Color::WHITE);
            let fallen = Affine::from_parts([12.0, 44.0], FRAC_PI_2, [1.0, 1.0]);
            canvas.draw_sprite_transformed(&hero, feet, fallen, Color::WHITE);

            let middle = Rect::new(-8.0, -8.0, 16.0, 16.0);
            let mirrored = Affine::from_parts([40.0, 12.0], 0.0, [-1.0, 1.0]);
            canvas.draw_sprite_transformed(&hero, middle, mirrored, Color::RED);
            let upside_down = Affine::from_parts([40.0, 44.0], PI, [1.0, 1.0]);
            canvas.draw_sprite_transformed(&hero, middle, upside_down, Color::GREEN);

            // The parent's x axis points down, so the child stands on its side, 32x32.
            let parent = Affine::from_parts([96.0, 32.0], FRAC_PI_2, [1.0, 1.0]);
            let child = Affine::from_parts([0.0, 0.0], 0.0, [2.0, 2.0]);
            let tint = Color::rgb8(0x80, 0xd0, 0xff);
            canvas.draw_sprite_transformed(&hero, middle, parent * child, tint);

            let turned = Affine::from_parts([66.3, 32.2], 0.5, [1.0, 1.0]);
            let fill = Color::rgba(1.0, 0.8, 0.2, 0.75);
            canvas.fill_rect_transformed(Rect::new(-6.0, -3.0, 12.0, 6.0), turned, fill);

            // World units are two canvas pixels, with the world's origin at the canvas's.
            canvas.set_camera(Some(Camera {
                zoom: 2.0,
                ..Camera::new(32.0, 16.0)
            }));
            let small = Rect::new(0.0, 0.0, 8.0, 8.0);
            let corner = Affine::from_translation([56.0, 24.0]);
            canvas.draw_sprite_transformed(&hero, small, corner, Color::WHITE);
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "transformed_sprites", frame);
}

/// A scene read from a scene file: a windmill whose sails, rects, turn about a hub off the pixel
/// grid, a sprite mirrored about its feet, text whose parent turns but which only moves, a hidden
/// node whose child doesn't draw, and an area, which never draws.
#[test]
fn scene() {
    let source = r##"return {
        format = 1,
        sheets = { tiles = { path = "tiles.png", tile_width = 16, tile_height = 16 } },
        children = {
            { name = "ground", kind = "rect", y = 48, width = 128, height = 16, color = "#2a3d2c" },
            {
                name = "mill", kind = "sprite", x = 24, y = 48, sheet = "tiles", cell = { 0, 1 },
                pivot = { 8, 16 }, tint = "#d8b070",
                children = {
                    {
                        name = "sails", kind = "node", y = -12, rotation = 0.3,
                        children = {
                            { name = "1", kind = "rect", width = 2, height = 10, pivot = { 1, 10 } },
                            { name = "2", kind = "rect", width = 2, height = 10, pivot = { 1, 10 }, rotation = 1.5707964 },
                            { name = "3", kind = "rect", width = 2, height = 10, pivot = { 1, 10 }, rotation = 3.1415927 },
                            { name = "4", kind = "rect", width = 2, height = 10, pivot = { 1, 10 }, rotation = -1.5707964 },
                        },
                    },
                },
            },
            {
                name = "hero", kind = "sprite", x = 56, y = 48, scale_x = -1, sheet = "tiles",
                cell = { 27, 0 }, pivot = { 8, 16 }, tint = "#f2d3ab",
            },
            {
                name = "labels", kind = "node", x = 72, y = 8, rotation = 0.5,
                children = {
                    { name = "title", kind = "text", text = "Mill", x = 4, pivot = { 0, 4 }, color = "#f2c14e" },
                },
            },
            {
                name = "hidden", kind = "node", visible = false,
                children = {
                    { name = "red", kind = "rect", x = 96, y = 32, width = 16, height = 16, color = "#ff0000" },
                },
            },
            { name = "zone", kind = "area", x = 96, y = 32, width = 16, height = 16 },
        },
    }"##;
    let mut headless = headless();
    let mut scene = load_scene(&mut headless, &[("scene.scene.luau", source)]);
    let frame = headless
        .render(128, 64, |canvas| {
            canvas.clear(Color::rgb8(0x1b, 0x26, 0x1e));
            scene.draw(canvas);
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "scene", frame);
}

/// A scene with a tilemap placed away from the origin, seen through a camera that cuts its edge
/// cells: grass with a gap, a hero between the ground and the trees, and a tree over the hero.
#[test]
fn scene_tilemap() {
    let tileset = r##"return {
        format = 1, tile_width = 16, tile_height = 16,
        sheets = { tiles = { path = "tiles.png", tile_width = 16, tile_height = 16 } },
        tiles = {
            grass = { sheet = "tiles", cell = { 5, 0 }, tint = "#3e6b3f" },
            tree = { sheet = "tiles", cell = { 0, 1 }, tint = "#6aa84f", solid = true },
        },
    }"##;
    let level = r##"return {
        format = 1,
        sheets = { tiles = { path = "tiles.png", tile_width = 16, tile_height = 16 } },
        children = {
            {
                name = "map", kind = "tilemap", x = 24, y = 8, tileset = "world.tileset.luau",
                columns = 6, rows = 4,
                children = {
                    {
                        name = "ground", kind = "tile_layer", legend = { ["."] = "grass" },
                        tiles = { "......", "......", ".. ...", "......" },
                    },
                    {
                        name = "hero", kind = "sprite", x = 40, y = 32, sheet = "tiles",
                        cell = { 27, 0 }, tint = "#f2d3ab",
                    },
                    {
                        name = "trees", kind = "tile_layer", legend = { T = "tree" },
                        tiles = { "      ", " T    ", "   T  ", "      " },
                    },
                },
            },
        },
    }"##;
    let mut headless = headless();
    let mut scene = load_scene(
        &mut headless,
        &[("world.tileset.luau", tileset), ("level.scene.luau", level)],
    );
    let frame = headless
        .render(64, 48, |canvas| {
            canvas.clear(Color::rgb8(0x10, 0x10, 0x18));
            // Shows x 32..96 and y 14..62 of the world, where the map covers x 24..120 and y
            // 8..72.
            canvas.set_camera(Some(Camera::new(64.0, 38.0)));
            scene.draw(canvas);
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "scene_tilemap", frame);
}

/// A screen laid out by anchors, drawn on a wide canvas and a narrow one: a bar stretched along
/// the bottom, inset by 4, with text at each end, the right-hand one aligned right; a panel in
/// the middle, with two lines of centered text; a marker in the top-right corner, inside a group
/// at the canvas's origin; and a shade over the left quarter of the canvas.
#[test]
fn scene_anchors() {
    let screen = r##"return {
        format = 1,
        children = {
            {
                name = "bar", kind = "rect", x = 4, y = -4, anchor = { 0, 1 }, width = -8,
                height = 12, stretch = { 1, 0 }, pivot = { 0, 12 }, color = "#3a405c",
                children = {
                    -- Without an anchor, it would be from the bar's origin, its bottom-left.
                    { name = "lives", kind = "text", x = 2, y = 1, anchor = { 0, 0 }, text = "HP 3" },
                    {
                        name = "score", kind = "text", x = -2, y = 1, anchor = { 1, 0 },
                        text = "1,250", align = "right", color = "#f2c14e",
                    },
                },
            },
            {
                name = "panel", kind = "rect", y = -6, anchor = { 0.5, 0.5 }, width = 72,
                height = 26, pivot = { 36, 13 }, color = "#14182c",
                children = {
                    {
                        name = "title", kind = "text", y = 3, anchor = { 0.5, 0 },
                        text = "Paused\nPress a key", align = "center",
                    },
                },
            },
            {
                name = "corner", kind = "node",
                children = {
                    {
                        name = "marker", kind = "rect", x = -2, y = 2, anchor = { 1, 0 }, width = 6,
                        height = 6, pivot = { 6, 0 }, color = "#ff7b9c",
                    },
                },
            },
            {
                name = "shade", kind = "rect", width = 0, height = 0, stretch = { 0.25, 1 },
                color = "#00000080",
            },
        },
    }"##;
    let mut headless = headless();
    let mut scene = load_scene(&mut headless, &[("screen.scene.luau", screen)]);
    for (name, [width, height]) in [
        ("scene_anchors_wide", [128, 64]),
        ("scene_anchors_narrow", [96, 80]),
    ] {
        let frame = headless
            .render(width, height, |canvas| {
                canvas.clear(Color::rgb8(0x2a, 0x3d, 0x2c));
                scene.draw(canvas);
            })
            .unwrap();
        assert_screenshot(&headless.adapter_name(), name, frame);
    }

    // Laid out for the narrow canvas, but drawn on a larger one, through a camera that shows it
    // whole, as an editor shows a screen: the same layout as the narrow screenshot, with the
    // canvas around it.
    let frame = headless
        .render(128, 96, |canvas| {
            canvas.clear(Color::rgb8(0x10, 0x10, 0x18));
            canvas.set_camera(Some(Camera::new(48.0, 40.0)));
            canvas.fill_rect(
                Rect::new(0.0, 0.0, 96.0, 80.0),
                Color::rgb8(0x2a, 0x3d, 0x2c),
            );
            scene.draw_with_size(canvas, [96.0, 80.0]);
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "scene_anchors_sized", frame);
}

/// Panels. The RPG's frame, sliced so its cut corners and one-texel border keep their size: wide,
/// tall, exactly as tall as its edges (so with no middle), turned a quarter about its corner, and
/// tinted. Then a scene's: a panel in the bottom-right corner fitting two lines of text inside its
/// padding and border, its fill translucent so the background shows through it but not its
/// border, the second line lined up at the right by its anchor; and a row of rects stacked across
/// with a gap, lined up down a stack 16 tall by their anchors, the last stretching down it.
#[test]
fn panels() {
    use std::f32::consts::FRAC_PI_2;

    let screen = r##"return {
        format = 3,
        children = {
            {
                name = "menu", kind = "panel", x = -4, y = -4, anchor = { 1, 1 },
                padding = { 6, 5, 6, 5 }, color = "#14182cc0", border = 1,
                border_color = "#f2d3ab",
                children = {
                    { name = "first", kind = "text", text = "Continue", color = "#f2c14e" },
                    { name = "second", kind = "text", anchor = { 1, 0 }, text = "New" },
                },
            },
            {
                name = "swatches", kind = "stack", x = 4, y = 30, height = 16,
                direction = "row", gap = 2,
                children = {
                    { name = "a", kind = "rect", width = 8, height = 8, color = "#ff7b9c" },
                    {
                        name = "b", kind = "rect", anchor = { 0, 0.5 }, width = 8, height = 12,
                        color = "#4de0e0",
                    },
                    {
                        name = "c", kind = "rect", anchor = { 0, 1 }, width = 8, height = 4,
                        color = "#f2c14e",
                    },
                    {
                        name = "d", kind = "rect", width = 8, height = 0, stretch = { 0, 1 },
                        color = "#8fd16a",
                    },
                },
            },
        },
    }"##;
    let mut headless = headless();
    let frame = headless.context().load_texture(FRAME).unwrap().sprite();
    let mut scene = load_scene(&mut headless, &[("panels.scene.luau", screen)]);
    let image = headless
        .render(128, 80, |canvas| {
            canvas.clear(Color::rgb8(0x2a, 0x3d, 0x2c));
            let slice = [2.0; 4];
            canvas.draw_sprite_sliced(&frame, slice, Rect::new(4.0, 4.0, 24.0, 12.0), Color::WHITE);
            canvas.draw_sprite_sliced(&frame, slice, Rect::new(32.0, 4.0, 6.0, 20.0), Color::WHITE);
            canvas.draw_sprite_sliced(&frame, slice, Rect::new(42.0, 4.0, 16.0, 4.0), Color::WHITE);
            let turned = Affine::from_parts([76.0, 4.0], FRAC_PI_2, [1.0, 1.0]);
            canvas.draw_sprite_sliced_transformed(
                &frame,
                slice,
                Rect::new(0.0, 0.0, 16.0, 8.0),
                turned,
                Color::WHITE,
            );
            let tint = Color::rgb8(0x8f, 0xd1, 0x6a);
            canvas.draw_sprite_sliced(&frame, slice, Rect::new(84.0, 4.0, 12.0, 12.0), tint);
            scene.draw(canvas);
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "panels", image);
}

/// Four instances of a puppet, off the pixel grid, posed by a clip that raises its arm, the
/// forearm turning inside it, and leans its body: at the clip's start, halfway, at its end, and at
/// its end weighted by a quarter. Halfway, its head steps to another sprite and its leg to half its
/// length; at the end, its hat is hidden. A quarter of the way, numbers go a quarter of the way
/// there, and the rest doesn't change. The head doesn't turn: GPUs sample a turned sprite's texels
/// differently where its edges cross pixels' middles.
#[test]
fn scene_clips() {
    let puppet = r##"return {
        format = 1,
        sheets = { tiles = { path = "tiles.png", tile_width = 16, tile_height = 16 } },
        children = {
            {
                name = "body", kind = "rect", y = -8, width = 4, height = 12, pivot = { 2, 12 },
                color = "#4f8fd0",
                children = {
                    {
                        name = "arm", kind = "rect", y = -11, width = 2, height = 6, pivot = { 1, 1 },
                        color = "#f2c28c",
                        children = {
                            {
                                name = "forearm", kind = "rect", y = 5, width = 2, height = 6,
                                pivot = { 1, 1 }, color = "#d49464",
                            },
                        },
                    },
                    {
                        name = "hat", kind = "rect", y = -26, width = 8, height = 3, pivot = { 4, 3 },
                        color = "#c04040",
                    },
                },
            },
            {
                name = "head", kind = "sprite", y = -20, sheet = "tiles", cell = { 27, 0 },
                pivot = { 8, 14 }, tint = "#f2d3ab",
            },
            {
                name = "leg", kind = "rect", y = -8, width = 2, height = 8, pivot = { 1, 0 },
                color = "#6b5844",
            },
        },
        clips = {
            wave = {
                duration = 1,
                tracks = {
                    { node = "body/arm", property = "rotation", keys = { { 0, 0 }, { 1, -2.5 } } },
                    { node = "body/arm/forearm", property = "rotation", keys = { { 0, 0 }, { 1, -1 } } },
                    { node = "body", property = "rotation", keys = { { 0, 0 }, { 1, 0.3 } } },
                    { node = "head", property = "cell", keys = { { 0, { 27, 0 } }, { 0.5, { 28, 0 } } } },
                    { node = "body/hat", property = "visible", keys = { { 0, true }, { 0.75, false } } },
                    {
                        node = "leg", property = "scale_y", interpolation = "step",
                        keys = { { 0, 1 }, { 0.5, 0.5 } },
                    },
                },
            },
        },
    }"##;
    let stage = r##"return {
        format = 1,
        children = {
            { name = "1", kind = "instance", x = 16.3, y = 43.7, scene = "puppet.scene.luau" },
            { name = "2", kind = "instance", x = 48.3, y = 43.7, scene = "puppet.scene.luau" },
            { name = "3", kind = "instance", x = 80.3, y = 43.7, scene = "puppet.scene.luau" },
            { name = "4", kind = "instance", x = 112.3, y = 43.7, scene = "puppet.scene.luau" },
        },
    }"##;
    let mut headless = headless();
    let mut scene = load_scene(
        &mut headless,
        &[("puppet.scene.luau", puppet), ("stage.scene.luau", stage)],
    );
    let wave = &SceneFile::parse(puppet, "puppet.scene.luau").unwrap().clips["wave"];
    let root = scene.root();
    for (name, time, weight) in [
        ("1", 0.0, 1.0),
        ("2", 0.5, 1.0),
        ("3", 1.0, 1.0),
        ("4", 1.0, 0.25),
    ] {
        let puppet = scene.find(root, name).unwrap();
        scene.pose(puppet, wave, time, weight).unwrap();
    }
    let frame = headless
        .render(128, 48, |canvas| {
            canvas.clear(Color::rgb8(0x1b, 0x26, 0x1e));
            scene.draw(canvas);
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "scene_clips", frame);
}

/// A scene with a 3D view between a rect under it and a bar over it: a floor, a box turned in a
/// stack, a ball on it, and a prop placed by an instance, a post with a lamp, lit by a sun from
/// the left. Nothing is textured, and nothing flat overlaps, so GPUs agree.
#[test]
fn scene_views() {
    let lamp = r##"return {
        format = 2,
        children = {
            { name = "post", kind = "mesh", shape = "cuboid", y = 0.75, size = { 0.2, 1.5, 0.2 }, tint = "#6b5844" },
            { name = "light", kind = "mesh", shape = "sphere", y = 1.6, radius = 0.25, tint = "#f2d06b" },
        },
    }"##;
    let room = r##"return {
        format = 2,
        children = {
            { name = "back", kind = "rect", x = 8, y = 4, width = 112, height = 40, color = "#30304a" },
            {
                name = "view", kind = "view3d", x = 16, y = 8, width = 96, height = 32,
                camera = { position = { 0, 3, 5 }, target = { 0, 0.5, 0 }, fov = 0.7 },
                lighting = { direction = { 1, -1, -0.5 }, color = "#e0e0e0", ambient = "#404040" },
                children = {
                    { name = "floor", kind = "mesh", shape = "plane", size = { 6, 3 }, tint = "#5a7a4a" },
                    {
                        name = "stack", kind = "node", x = -1, yaw = 0.5,
                        children = {
                            { name = "box", kind = "mesh", shape = "cuboid", y = 0.5, tint = "#c08040" },
                            { name = "ball", kind = "mesh", shape = "sphere", y = 1.4, radius = 0.4, tint = "#4f8fd0" },
                        },
                    },
                    { name = "lamp", kind = "instance", scene = "lamp.scene.luau", x = 1.5, z = -0.5, scale_y = 0.8 },
                },
            },
            { name = "bar", kind = "rect", x = 16, y = 30, width = 40, height = 6, color = "#c04040" },
        },
    }"##;
    let mut headless = headless();
    let mut scene = load_scene(
        &mut headless,
        &[("lamp.scene.luau", lamp), ("room.scene.luau", room)],
    );
    let frame = headless
        .render(128, 48, |canvas| {
            canvas.clear(Color::rgb8(0x1b, 0x26, 0x1e));
            scene.draw(canvas);
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "scene_views", frame);
}

/// The scene at the end of `files`, a path and the file's contents for each scene and tileset
/// file, with every image the tileset's.
fn load_scene(headless: &mut Headless, files: &[(&'static str, &'static str)]) -> Scene {
    struct Files<'a> {
        ctx: &'a mut Context,
        files: &'a [(&'static str, &'static str)],
    }
    impl Files<'_> {
        fn source(&self, path: &str) -> &'static str {
            let found = self.files.iter().find(|(name, _)| *name == path);
            found.unwrap_or_else(|| panic!("no {path}")).1
        }
    }
    impl Assets for Files<'_> {
        fn texture(&mut self, _: &str) -> wingine::Result<Texture> {
            self.ctx.load_texture(TILESET)
        }
        fn scene(&mut self, path: &str) -> wingine::Result<Rc<SceneFile>> {
            SceneFile::parse(self.source(path), path).map(Rc::new)
        }
        fn tileset(&mut self, path: &str) -> wingine::Result<Rc<TilesetFile>> {
            TilesetFile::parse(self.source(path), path).map(Rc::new)
        }
        fn font(&mut self, path: &str) -> wingine::Result<Font> {
            self.ctx.load_font(Path::new(EXAMPLES).join(path))
        }
        fn model(&mut self, path: &str) -> wingine::Result<Model> {
            wingine::scene::load_model(self.ctx, &Path::new(EXAMPLES).join(path))
        }
        fn mesh(&mut self, shape: &Shape) -> wingine::Result<Mesh> {
            self.ctx.create_mesh(&shape.mesh_data())
        }
    }
    let ctx = headless.context();
    let font = ctx.default_font().clone();
    let path = files.last().unwrap().0;
    let mut assets = Files { ctx, files };
    let file = assets.scene(path).unwrap();
    Scene::new(&file, path, &font, &mut assets).unwrap()
}

/// Blend modes, a column each in the order of `Blend::ALL`, over bands of gray from black at the
/// top to white at the bottom: a red bar, then a translucent green one overlapping it. Normal
/// covers; add brightens, and red and green add to yellow; multiply darkens, keeping black black
/// and taking each color where it's white; screen brightens, never past white. Then a translucent
/// white bar across the bottom, in normal blending again.
#[test]
fn blend_modes() {
    let mut headless = headless();
    let frame = headless
        .render(64, 48, |canvas| {
            canvas.clear(Color::BLACK);
            for band in 0..12 {
                let gray = band as f32 / 11.0;
                let rect = Rect::new(0.0, band as f32 * 4.0, 64.0, 4.0);
                canvas.fill_rect(rect, Color::rgb(gray, gray, gray));
            }
            for (i, blend) in Blend::ALL.into_iter().enumerate() {
                let x = i as f32 * 16.0;
                canvas.set_blend(blend);
                let red = Color::rgb8(0xd0, 0x40, 0x40);
                canvas.fill_rect(Rect::new(x + 1.0, 0.0, 9.0, 44.0), red);
                let green = Color::rgba8(0x40, 0xb0, 0x50, 0xc0);
                canvas.fill_rect(Rect::new(x + 6.0, 0.0, 9.0, 44.0), green);
            }
            canvas.set_blend(Blend::Normal);
            canvas.fill_rect(
                Rect::new(0.0, 44.0, 64.0, 4.0),
                Color::rgba(1.0, 1.0, 1.0, 0.5),
            );
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "blend_modes", frame);
}

/// Particles. On the left, squares from a fountain on an emitter moved and turned as it goes,
/// slowed by drag and pulled down, turning as they fly, shrinking and fading from white through
/// orange to clear red. On the right, sprites added over a dark band, their colors by their ages:
/// three still bursts a quarter of a second apart, each at a whole pixel.
#[test]
fn particles() {
    let mut headless = headless();
    let sheet = SpriteSheet::new(headless.context().load_texture(TILESET).unwrap(), 16, 16);
    let mut fountain = Particles::new(
        ParticleSettings {
            rate: 60.0,
            lifetime: [0.6, 1.2],
            speed: [20.0, 40.0],
            direction: -1.4,
            spread: 0.8,
            gravity: [0.0, 40.0],
            drag: 0.5,
            angle: [0.0, 1.5],
            spin: [-3.0, 3.0],
            scales: vec![3.0, 1.0],
            colors: vec![
                Color::WHITE,
                Color::rgb8(0xf2, 0xa0, 0x30),
                Color::rgba8(0xd0, 0x30, 0x30, 0),
            ],
            ..ParticleSettings::default()
        },
        None,
        7,
    );
    for frame in 0..60 {
        let t = frame as f32 / 60.0;
        let origin = Affine::from_parts_exact([10.0 + 20.0 * t, 44.0], 0.4 * t, [1.0, 1.0]);
        fountain.set_origin(origin, Rect::new(-2.0, 0.0, 4.0, 1.0));
        fountain.advance(1.0 / 60.0);
    }
    let mut sparks = Particles::new(
        ParticleSettings {
            rate: 0.0,
            lifetime: [1.0, 1.0],
            speed: [0.0, 0.0],
            colors: vec![Color::rgb8(0x40, 0x80, 0xff), Color::rgb8(0xff, 0x80, 0x20)],
            blend: Blend::Add,
            ..ParticleSettings::default()
        },
        Some(sheet.sprite(5, 0)),
        1,
    );
    for x in [64.0, 80.0, 96.0] {
        sparks.set_origin(Affine::from_translation([x, 36.0]), Rect::default());
        sparks.emit(1);
        sparks.advance(0.25);
    }
    let frame = headless
        .render(112, 48, |canvas| {
            canvas.clear(Color::rgb8(0x20, 0x20, 0x28));
            canvas.draw_particles(&fountain);
            canvas.fill_rect(
                Rect::new(56.0, 24.0, 56.0, 24.0),
                Color::rgb8(0x30, 0x10, 0x10),
            );
            canvas.draw_particles(&sparks);
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "particles", frame);
}

/// Blend modes and particles in a scene: a sprite added over a field, a night multiplying the
/// field's right half, and a fire of particles, run for a second, rising from a box and added to
/// what's under it, over the night and the sky. The canvas's own blend mode, set to screen before
/// the scene draws, doesn't change how the scene draws, and is screen again after.
#[test]
fn scene_blend() {
    let source = r##"return {
        format = 4,
        sheets = { tiles = { path = "tiles.png", tile_width = 16, tile_height = 16 } },
        children = {
            { name = "sky", kind = "rect", width = 64, height = 24, color = "#203060" },
            { name = "field", kind = "rect", y = 24, width = 64, height = 24, color = "#3e6b3f" },
            {
                name = "tree", kind = "sprite", x = 8, y = 20, sheet = "tiles", cell = { 0, 1 },
                tint = "#a0d070", blend = "add",
            },
            {
                name = "night", kind = "rect", x = 32, width = 32, height = 48,
                color = "#4050a0", blend = "multiply",
            },
            {
                name = "fire", kind = "particles", x = 44, y = 40, width = 8, height = 2,
                rate = 40, seed = 3, lifetime = { 0.5, 1 }, speed = { 10, 20 }, direction = -1.5707964,
                spread = 0.4, scales = { 2, 1 }, colors = { "#ffe080", "#ff602000" }, blend = "add",
            },
        },
    }"##;
    let mut headless = headless();
    let mut scene = load_scene(&mut headless, &[("fire.scene.luau", source)]);
    let root = scene.root();
    for _ in 0..60 {
        scene.advance_particles(root, 1.0 / 60.0);
    }
    let frame = headless
        .render(64, 48, |canvas| {
            canvas.set_blend(Blend::Screen);
            scene.draw(canvas);
            assert_eq!(canvas.blend(), Blend::Screen);
            canvas.set_blend(Blend::Normal);
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "scene_blend", frame);
}

/// A panel draws normally after a node that draws added, even one off the canvas.
#[test]
fn panels_draw_normally_after_other_blend_modes() {
    let alone = r##"return {
        format = 4,
        children = {
            {
                name = "panel", kind = "panel", x = 4, y = 4, width = 24, height = 16,
                color = "#4050a0c0", border = 1, border_color = "#f2d3ab",
            },
        },
    }"##;
    let after_glow = r##"return {
        format = 4,
        children = {
            { name = "glow", kind = "rect", x = -40, width = 8, height = 8, blend = "add" },
            {
                name = "panel", kind = "panel", x = 4, y = 4, width = 24, height = 16,
                color = "#4050a0c0", border = 1, border_color = "#f2d3ab",
            },
        },
    }"##;
    let mut headless = headless();
    let [alone, after_glow] = [alone, after_glow].map(|source| {
        let mut scene = load_scene(&mut headless, &[("panel.scene.luau", source)]);
        headless
            .render(32, 24, |canvas| {
                canvas.clear(Color::rgb8(0x3e, 0x6b, 0x3f));
                scene.draw(canvas);
            })
            .unwrap()
    });
    assert!(alone == after_glow);
}

/// Solid fills and sprites interleaved, so batches switch texture several times. Draw order must
/// hold across the switches.
#[test]
fn batch_order() {
    let mut headless = headless();
    let sheet = SpriteSheet::new(headless.context().load_texture(TILESET).unwrap(), 16, 16);
    let hero = sheet.sprite(27, 0);
    let frame = headless
        .render(48, 16, |canvas| {
            canvas.clear(Color::BLACK);
            canvas.fill_rect(
                Rect::new(0.0, 0.0, 32.0, 16.0),
                Color::rgb8(0x30, 0x30, 0x80),
            );
            canvas.draw_sprite(&hero, Rect::new(0.0, 0.0, 16.0, 16.0), Color::WHITE);
            canvas.fill_rect(
                Rect::new(8.0, 4.0, 16.0, 8.0),
                Color::rgba(1.0, 0.5, 0.0, 0.75),
            );
            canvas.draw_sprite(&hero, Rect::new(16.0, 0.0, 16.0, 16.0), Color::GREEN);
            canvas.draw_sprite(&hero, Rect::new(32.0, 0.0, 16.0, 16.0), Color::WHITE);
            canvas.fill_rect(
                Rect::new(40.0, 0.0, 8.0, 16.0),
                Color::rgb8(0x80, 0x20, 0x20),
            );
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "batch_order", frame);
}

/// More quads than the renderer's initial buffers hold, so they have to grow.
#[test]
fn many_quads() {
    let mut headless = headless();
    let frame = headless
        .render(64, 64, |canvas| {
            for y in 0..64u8 {
                for x in 0..64u8 {
                    let color = Color::rgb8(x * 4, y * 4, 0x80);
                    canvas.fill_rect(Rect::new(x.into(), y.into(), 1.0, 1.0), color);
                }
            }
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "many_quads", frame);
}

/// A fixed resolution scales up by the largest whole number that fits, without smoothing, and is
/// centered between black bars.
#[test]
fn fixed_resolution() {
    let mut headless = headless();
    headless.set_resolution(Resolution::Fixed {
        width: 32,
        height: 16,
    });
    // 3x fits a 100x60 frame, leaving bars on every side.
    let frame = headless
        .render(100, 60, |canvas| {
            assert_eq!((canvas.width(), canvas.height()), (32.0, 16.0));
            canvas.clear(Color::rgb8(0x20, 0x30, 0x60));
            // Single-pixel details, which any smoothing would blur.
            for x in (0..32).step_by(2) {
                canvas.fill_rect(Rect::new(x as f32, 0.0, 1.0, 1.0), Color::WHITE);
            }
            canvas.fill_rect(
                Rect::new(0.0, 15.0, 32.0, 1.0),
                Color::rgb8(0xf0, 0xc0, 0x30),
            );
            canvas.fill_rect(
                Rect::new(31.0, 0.0, 1.0, 16.0),
                Color::rgb8(0x30, 0xa0, 0xf0),
            );
            canvas.fill_rect(Rect::new(8.0, 4.0, 8.0, 8.0), Color::RED);
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "fixed_resolution", frame);
}

/// A fixed resolution shows a translucent clear, and what's drawn translucent over it, as the
/// window's resolution does: the window shows colors, not how opaque they are.
#[test]
fn fixed_resolution_shows_translucent_clears_as_the_window_does() {
    let draw = |canvas: &mut Canvas| {
        canvas.clear(Color::rgba8(0x40, 0x80, 0xc0, 0x80));
        canvas.fill_rect(
            Rect::new(4.0, 2.0, 8.0, 4.0),
            Color::rgba8(0xf0, 0xc0, 0x30, 0xa0),
        );
    };
    let mut headless = headless();
    let window = headless.render(16, 8, draw).unwrap();
    headless.set_resolution(Resolution::Fixed {
        width: 16,
        height: 8,
    });
    let fixed = headless.render(16, 8, draw).unwrap();
    // A window shows no alpha, which only the frame of its own resolution keeps.
    for (fixed, window) in fixed.pixels().chunks(4).zip(window.pixels().chunks(4)) {
        let differs = (0..3).any(|i| fixed[i].abs_diff(window[i]) > 2);
        assert!(
            !differs,
            "{fixed:?} at a fixed resolution, {window:?} at the window's"
        );
    }
}

/// What the scaled fixed resolutions draw on their 30x15 canvas: single texels, which show how
/// each is scaled, along its edges and in a checkerboard.
fn draw_scaled(canvas: &mut Canvas) {
    assert_eq!((canvas.width(), canvas.height()), (30.0, 15.0));
    canvas.clear(Color::rgb8(0x20, 0x30, 0x60));
    for x in (0..30).step_by(2) {
        canvas.fill_rect(Rect::new(x as f32, 0.0, 1.0, 1.0), Color::WHITE);
    }
    canvas.fill_rect(
        Rect::new(0.0, 14.0, 30.0, 1.0),
        Color::rgb8(0xf0, 0xc0, 0x30),
    );
    canvas.fill_rect(
        Rect::new(29.0, 0.0, 1.0, 15.0),
        Color::rgb8(0x30, 0xa0, 0xf0),
    );
    for y in 4..10 {
        for x in (6 + y % 2..14).step_by(2) {
            canvas.fill_rect(Rect::new(x as f32, y as f32, 1.0, 1.0), Color::RED);
        }
    }
}

/// A fixed resolution fitted to the frame keeping its shape, by any scale: 7/3 fills a 70x60
/// frame across, between bars above and below, its texels 2 and 3 pixels square. Scales whose
/// fractions have odd denominators put no pixel's middle on the edge between texels, where GPUs
/// would pick either.
#[test]
fn fixed_resolution_fit() {
    let mut headless = headless();
    headless.set_resolution(Resolution::Fixed {
        width: 30,
        height: 15,
    });
    headless.context().set_scaling(Scaling::Fit);
    let frame = headless.render(70, 60, draw_scaled).unwrap();
    assert_screenshot(&headless.adapter_name(), "fixed_resolution_fit", frame);
}

/// A fixed resolution stretched to fill the frame both ways: 7/3 across and 8/3 down a 70x40
/// frame, with no bars.
#[test]
fn fixed_resolution_stretched() {
    let mut headless = headless();
    headless.set_resolution(Resolution::Fixed {
        width: 30,
        height: 15,
    });
    headless.context().set_scaling(Scaling::Stretch);
    let frame = headless.render(70, 40, draw_scaled).unwrap();
    assert_screenshot(
        &headless.adapter_name(),
        "fixed_resolution_stretched",
        frame,
    );
}

/// World drawing through a zoomed camera held inside its bounds, then a HUD in canvas pixels.
#[test]
fn camera() {
    let mut headless = headless();
    let frame = headless
        .render(64, 48, |canvas| {
            canvas.clear(Color::rgb8(0x10, 0x10, 0x18));
            // Asks to center on (40, 4), but the 32x24 view must stay inside the 48x48 world,
            // so it shows x 16..48, y 0..24.
            canvas.set_camera(Some(Camera {
                zoom: 2.0,
                bounds: Some(Rect::new(0.0, 0.0, 48.0, 48.0)),
                ..Camera::new(40.0, 4.0)
            }));
            for row in 0..6u8 {
                for column in 0..6u8 {
                    if (row + column).is_multiple_of(2) {
                        let tile =
                            Rect::new(f32::from(column) * 8.0, f32::from(row) * 8.0, 8.0, 8.0);
                        canvas.fill_rect(tile, Color::rgb8(0x30, 0x60, 0x40));
                    }
                }
            }
            canvas.fill_rect(Rect::new(44.0, 2.0, 2.0, 2.0), Color::RED);

            canvas.set_camera(None);
            canvas.fill_rect(
                Rect::new(0.0, 40.0, 64.0, 8.0),
                Color::rgba(0.0, 0.0, 0.0, 0.6),
            );
            canvas.fill_rect(Rect::new(2.0, 42.0, 4.0, 4.0), Color::GREEN);
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "camera", frame);
}

/// A two-layer tilemap through a camera that cuts its edge cells in half: tints apply, an empty
/// cell shows the clear color, and a sprite drawn between the layers goes over the ground but
/// under the tree on the layer above.
#[test]
fn tilemap() {
    let mut headless = headless();
    let sheet = SpriteSheet::new(headless.context().load_texture(TILESET).unwrap(), 16, 16);
    let mut tileset = Tileset::new(16, 16);
    let grass = tileset.add(Tile {
        tint: Color::rgb8(0x3e, 0x6b, 0x3f),
        ..Tile::new(sheet.sprite(5, 0))
    });
    let tree = tileset.add(Tile {
        tint: Color::rgb8(0x6a, 0xa8, 0x4f),
        solid: true,
        ..Tile::new(sheet.sprite(0, 1))
    });
    let mut map = Tilemap::new(tileset, 6, 4);
    let ground = map.add_layer();
    let above = map.add_layer();
    for row in 0..4 {
        for column in 0..6 {
            if [column, row] != [2, 2] {
                map.set(ground, [column, row], Some(grass));
            }
        }
    }
    map.set(above, [1, 1], Some(tree));
    map.set(above, [3, 2], Some(tree));
    let hero = sheet.sprite(27, 0);

    let frame = headless
        .render(64, 48, |canvas| {
            canvas.clear(Color::rgb8(0x10, 0x10, 0x18));
            // Shows x 8..72 and y 6..54 of the 96x64 map.
            canvas.set_camera(Some(Camera {
                bounds: Some(map.bounds()),
                ..Camera::new(40.0, 30.0)
            }));
            map.draw_layer(canvas, ground);
            // Half over the empty cell, half under the tree.
            let hero_tint = Color::rgb8(0xf2, 0xd3, 0xab);
            canvas.draw_sprite(&hero, Rect::new(40.0, 32.0, 16.0, 16.0), hero_tint);
            map.draw_layer(canvas, above);
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "tilemap", frame);
}

/// The built-in font: every glyph, a scaled title, text wrapped to fit a box, and a layout only
/// partly revealed.
#[test]
fn text() {
    let mut headless = headless();
    let font = headless.context().default_font().clone();
    let wrapped = font.layout(
        "Wrapped to 90 pixels… with “quotes” — and an unbreakably-long-word.",
        Some(90.0),
    );
    let revealed = font.layout("Half of this\nis hidden.", None);
    let frame = headless
        .render(200, 128, |canvas| {
            canvas.clear(Color::rgb8(0x20, 0x20, 0x30));
            let title = Color::rgb8(0xf0, 0xc0, 0x30);
            canvas.draw_text(&font.scaled(2.0), "Wingine", [4.0, 2.0], title);
            canvas.draw_text(
                &font,
                " !\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~\n\
                 0123456789 …–—‘’“”\n\
                 ABCDEFGHIJKLMNOPQRSTUVWXYZ\n\
                 abcdefghijklmnopqrstuvwxyz",
                [4.0, 22.0],
                Color::WHITE,
            );

            let [width, height] = wrapped.size();
            let panel = Rect::new(4.0, 66.0, width + 8.0, height + 6.0);
            canvas.fill_rect(panel, Color::rgb8(0x10, 0x10, 0x60));
            canvas.draw_text_layout(&wrapped, .., [8.0, 69.0], Color::WHITE);

            let shown = revealed.glyph_count() / 2;
            let blue = Color::rgb8(0x80, 0xd0, 0xff);
            // Fractional positions round to whole pixels.
            canvas.draw_text_layout(&revealed, ..shown, [120.4, 69.6], blue);
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "text", frame);
}

/// Fonts loaded from files: the RPG title's bitmap font, its glyphs as wide as their ink, and
/// Hack, a TrueType font, smooth, crisp, scaled (rasterized again at the larger size), and laid
/// out centered and wrapped. Hack's glyphs are rasterized as each is first drawn, into the same
/// frame.
#[test]
fn text_in_loaded_fonts() {
    let mut headless = headless();
    let ctx = headless.context();
    let title = ctx
        .load_font(Path::new(EXAMPLES).join("rpg/title.font.luau"))
        .unwrap();
    let hack = ctx
        .load_font(Path::new(EXAMPLES).join("lettering/fonts/hack.font.luau"))
        .unwrap();
    let crisp = ctx
        .load_font(Path::new(EXAMPLES).join("lettering/fonts/hack-crisp.font.luau"))
        .unwrap();
    let wrapped = hack.layout_aligned(
        "Centered, and wrapped between words.",
        Some(150.0),
        Align::Center,
    );
    let frame = headless
        .render(260, 140, |canvas| {
            canvas.clear(Color::rgb8(0x14, 0x18, 0x2c));
            let gold = Color::rgb8(0xf2, 0xc1, 0x4e);
            canvas.draw_text(&title, "The Stone Shrine", [4.0, 2.0], gold);
            canvas.draw_text(&title, "0123456789 &?! éöñ", [4.0, 18.0], Color::WHITE);
            canvas.draw_text(&hack, "Smooth: AVA Ωж To?", [4.0, 38.0], Color::WHITE);
            canvas.draw_text(&crisp, "Crisp: AVA Ωж To?", [4.0, 54.0], Color::WHITE);
            let large = Color::rgb8(0x9f, 0xd8, 0xa0);
            canvas.draw_text(&hack.scaled(1.5), "Scaled 1.5", [4.0, 70.0], large);
            canvas.draw_text_layout(&wrapped, .., [55.0, 96.0], Color::WHITE);
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "text_in_loaded_fonts", frame);
}

/// A cube turned to show three sides and a sphere, lit by the default sun: tops brightest, then
/// fronts, then right-hand sides, with the sphere shading smoothly between.
#[test]
fn meshes_lit() {
    let mut headless = headless();
    let ctx = headless.context();
    let cube = ctx.create_mesh(&MeshData::cube(1.0)).unwrap();
    let sphere = ctx.create_mesh(&MeshData::sphere(0.6, 16, 8)).unwrap();
    let frame = headless
        .render(96, 64, |canvas| {
            canvas.clear(Color::rgb8(0x1b, 0x26, 0x1e));
            let camera = Camera3d::perspective([0.03, 1.61, 4.02], [0.0; 3], 0.8);
            let mut view = canvas.begin_3d(View3d::new(camera, Rect::new(0.0, 0.0, 96.0, 64.0)));
            let turned =
                Affine3::from_translation([-0.9, 0.0, 0.0]) * Affine3::from_rotation_y(0.6);
            view.draw_mesh(&cube, turned, None, Color::rgb8(0xd0, 0x60, 0x40));
            let right = Affine3::from_translation([0.9, 0.0, 0.0]);
            view.draw_mesh(&sphere, right, None, Color::rgb8(0x50, 0x90, 0xd0));
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "meshes_lit", frame);
}

/// A tall box pushed through a cube at a slant, and a cube in front of both: each hides what's
/// behind it, pixel by pixel, whichever order they're drawn in.
#[test]
fn meshes_depth() {
    let mut headless = headless();
    let ctx = headless.context();
    let cube = ctx.create_mesh(&MeshData::cube(1.0)).unwrap();
    let post = ctx.create_mesh(&MeshData::cuboid([0.4, 2.0, 0.4])).unwrap();
    let draws = [
        (
            &cube,
            Affine3::from_translation([0.0, 0.0, -1.0]) * Affine3::from_rotation_y(0.3),
            Color::rgb8(0x50, 0x90, 0xd0),
        ),
        (
            &post,
            Affine3::from_translation([0.2, 0.0, -1.1]) * Affine3::from_rotation_z(0.4),
            Color::rgb8(0xf0, 0xc0, 0x30),
        ),
        (
            &cube,
            Affine3::from_translation([-0.6, -0.4, 0.6]) * Affine3::from_scale([0.5; 3]),
            Color::rgb8(0xd0, 0x60, 0x40),
        ),
    ];
    let draw = |canvas: &mut Canvas, near_first: bool| {
        canvas.clear(Color::rgb8(0x10, 0x10, 0x18));
        let camera = Camera3d::perspective([0.02, 0.51, 3.03], [0.0, 0.0, -0.5], 0.9);
        let mut view = canvas.begin_3d(View3d::new(camera, Rect::new(0.0, 0.0, 64.0, 48.0)));
        let mut order: Vec<_> = draws.iter().collect();
        if near_first {
            order.reverse();
        }
        for (mesh, transform, color) in order {
            view.draw_mesh(mesh, *transform, None, *color);
        }
    };
    let far_first = headless
        .render(64, 48, |canvas| draw(canvas, false))
        .unwrap();
    let near_first = headless
        .render(64, 48, |canvas| draw(canvas, true))
        .unwrap();
    assert!(
        far_first.pixels() == near_first.pixels(),
        "the order meshes are drawn in changed what shows"
    );
    assert_screenshot(&headless.adapter_name(), "meshes_depth", far_first);
}

/// 3D views among 2D drawing: stripes under a view, a translucent bar over it, a second view over
/// part of the first and the bar, whose depth doesn't hide it behind the first's meshes, and a
/// marker over that. Views don't clear what's under them.
#[test]
fn meshes_layered() {
    let mut headless = headless();
    let ctx = headless.context();
    let cube = ctx.create_mesh(&MeshData::cube(1.0)).unwrap();
    let sphere = ctx.create_mesh(&MeshData::sphere(0.8, 12, 6)).unwrap();
    let frame = headless
        .render(96, 64, |canvas| {
            canvas.clear(Color::rgb8(0x10, 0x10, 0x18));
            for x in (0..96).step_by(8) {
                let stripe = Rect::new(x as f32, 0.0, 4.0, 64.0);
                canvas.fill_rect(stripe, Color::rgb8(0x24, 0x2c, 0x38));
            }

            let camera = Camera3d::perspective([0.02, 1.21, 3.01], [0.0; 3], 0.9);
            canvas
                .begin_3d(View3d::new(camera, Rect::new(4.0, 8.0, 48.0, 40.0)))
                .draw_mesh(
                    &cube,
                    Affine3::from_translation([0.6, 0.0, 0.0]) * Affine3::from_rotation_y(0.5),
                    None,
                    Color::rgb8(0xd0, 0x60, 0x40),
                );
            canvas.fill_rect(
                Rect::new(0.0, 26.0, 96.0, 8.0),
                Color::rgba(1.0, 1.0, 1.0, 0.5),
            );

            // Farther from its camera than the cube is from the first's, but drawn after, so over
            // the cube's right-hand side.
            let camera = Camera3d::perspective([0.01, 0.02, 6.03], [0.0; 3], 0.5);
            canvas
                .begin_3d(View3d::new(camera, Rect::new(24.0, 12.0, 56.0, 44.0)))
                .draw_mesh(
                    &sphere,
                    Affine3::IDENTITY,
                    None,
                    Color::rgb8(0x50, 0x90, 0xd0),
                );
            canvas.fill_rect(
                Rect::new(56.0, 38.0, 6.0, 6.0),
                Color::rgb8(0xf0, 0xc0, 0x30),
            );
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "meshes_layered", frame);
}

/// 3D views at a fixed resolution of 32x24, scaled 3x onto the frame, so meshes render at the
/// canvas's own pixels: a view hanging off the canvas's left edge, which keeps its projection,
/// showing a cube mirrored left to right, which isn't turned inside out; and a view through a 2D
/// camera zoomed 2x, unlit.
#[test]
fn meshes_fixed_resolution() {
    let mut headless = headless();
    headless.set_resolution(Resolution::Fixed {
        width: 32,
        height: 24,
    });
    let cube = headless
        .context()
        .create_mesh(&MeshData::cuboid([1.6, 1.0, 1.0]))
        .unwrap();
    let frame = headless
        .render(100, 80, |canvas| {
            canvas.clear(Color::rgb8(0x20, 0x30, 0x60));
            let camera = Camera3d::perspective([1.21, 1.02, 3.03], [0.0; 3], 0.9);
            let rect = Rect::new(-8.0, 0.0, 32.0, 16.0);
            let mirrored = Affine3::from_scale([-1.0, 1.0, 1.0]) * Affine3::from_rotation_y(0.4);
            canvas.begin_3d(View3d::new(camera, rect)).draw_mesh(
                &cube,
                mirrored,
                None,
                Color::rgb8(0xd0, 0x60, 0x40),
            );

            // World units are two canvas pixels, so the view's 8x4 rect covers 16x8 pixels.
            canvas.set_camera(Some(Camera {
                zoom: 2.0,
                ..Camera::new(8.0, 6.0)
            }));
            let camera = Camera3d::orthographic([0.01, 0.02, 3.0], [0.0; 3], 1.5);
            let view = View3d {
                lighting: Lighting::UNLIT,
                ..View3d::new(camera, Rect::new(8.0, 8.0, 8.0, 4.0))
            };
            canvas.begin_3d(view).draw_mesh(
                &cube,
                Affine3::IDENTITY,
                None,
                Color::rgb8(0x40, 0xb0, 0x50),
            );
        })
        .unwrap();
    assert_screenshot(&headless.adapter_name(), "meshes_fixed_resolution", frame);
}

/// Meshes showing sprites, seen from above at an angle: cubes whose sides show a sprite with a
/// transparent background, which shows what's behind through it, on a plane showing a tree,
/// flipped, so its top is toward the camera.
///
/// Where a pixel's middle lands almost exactly between two texels, GPUs pick either, so a few
/// pixels may differ.
#[test]
fn meshes_textured() {
    let mut headless = headless();
    let sheet = SpriteSheet::new(headless.context().load_texture(TILESET).unwrap(), 16, 16);
    let ctx = headless.context();
    let cube = ctx.create_mesh(&MeshData::cube(1.0)).unwrap();
    let plane = ctx.create_mesh(&MeshData::plane([3.0, 3.0])).unwrap();
    let hero = sheet.sprite(27, 0);
    let tree = sheet.sprite(0, 1).flipped_y();
    let frame = headless
        .render(128, 96, |canvas| {
            canvas.clear(Color::rgb8(0x1b, 0x26, 0x1e));
            let camera = Camera3d::perspective([1.83, 2.01, 2.52], [-0.2, -0.2, 0.2], 0.82);
            let rect = Rect::new(0.0, 0.0, 128.0, 96.0);
            let mut view = canvas.begin_3d(View3d::new(camera, rect));
            let below = Affine3::from_translation([0.0, -0.5, 0.0]);
            view.draw_mesh(&plane, below, Some(&tree), Color::rgb8(0x6a, 0xa8, 0x4f));
            view.draw_mesh(&cube, Affine3::IDENTITY, Some(&hero), Color::WHITE);
            let small =
                Affine3::from_translation([-1.0, -0.25, 0.9]) * Affine3::from_scale([0.5; 3]);
            view.draw_mesh(&cube, small, Some(&hero), Color::rgb8(0xf2, 0xd3, 0xab));
        })
        .unwrap();
    assert_screenshot_allowing(&headless.adapter_name(), "meshes_textured", frame, 16);
}

/// The example props, models made in Blender: a barrel colored by its vertices, a crate showing
/// its texture, and a table whose legs, one mesh placed four times by the file's nodes, hold up
/// its top, turned and tinted as a whole. A few pixels may differ, as for `meshes_textured`.
#[test]
#[cfg(feature = "models")]
fn models_draw_their_parts() {
    let models = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/models");
    let mut headless = headless();
    let ctx = headless.context();
    let barrel = ctx.load_model(format!("{models}/barrel.glb")).unwrap();
    let crate_ = ctx.load_model(format!("{models}/crate.glb")).unwrap();
    let table = ctx.load_model(format!("{models}/table.glb")).unwrap();
    let ground = ctx.create_mesh(&MeshData::plane([5.0, 3.0])).unwrap();
    let frame = headless
        .render(128, 96, |canvas| {
            canvas.clear(Color::rgb8(0x10, 0x10, 0x18));
            let camera = Camera3d::perspective([0.52, 2.13, 3.07], [0.0, 0.3, 0.0], 0.8);
            let rect = Rect::new(0.0, 0.0, 128.0, 96.0);
            let mut view = canvas.begin_3d(View3d::new(camera, rect));
            view.draw_mesh(
                &ground,
                Affine3::IDENTITY,
                None,
                Color::rgb8(0x4a, 0x40, 0x38),
            );
            let at = |x, z, turn| {
                Affine3::from_translation([x, 0.0, z]) * Affine3::from_rotation_y(turn)
            };
            view.draw_model(&barrel, at(-1.2, 0.1, 0.3), Color::WHITE);
            view.draw_model(&crate_, at(-0.1, -0.3, 0.2), Color::WHITE);
            let pale = Color::rgb8(0xe0, 0xd0, 0xc0);
            view.draw_model(&table, at(1.15, 0.2, -0.5), pale);
        })
        .unwrap();
    assert_screenshot_allowing(
        &headless.adapter_name(),
        "models_draw_their_parts",
        frame,
        16,
    );
}

/// Writes a `.gltf` model to `folder`: a square 2 across facing `+z`, centered on its origin,
/// showing `texture.png`, red then blue, twice across it with the sampler's default repeating, and
/// its material double-sided if `double_sided`. Its buffer and image are files beside it.
#[cfg(feature = "models")]
fn write_square_model(folder: &Path, double_sided: bool) -> std::path::PathBuf {
    fs::create_dir_all(folder).unwrap();
    let mut bytes = Vec::new();
    let positions = [
        -1.0, -1.0, 0.0, 1.0, -1.0, 0.0, 1.0, 1.0, 0.0, -1.0, 1.0, 0.0f32,
    ];
    let uvs = [0.0, 1.0, 2.0, 1.0, 2.0, 0.0, 0.0, 0.0f32];
    for value in positions.iter().chain(&uvs) {
        bytes.extend(value.to_le_bytes());
    }
    for index in [0u16, 1, 2, 0, 2, 3] {
        bytes.extend(index.to_le_bytes());
    }
    fs::write(folder.join("square.bin"), &bytes).unwrap();
    let mut texture = RgbaImage::new(2, 1);
    texture.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
    texture.put_pixel(1, 0, image::Rgba([0, 0, 255, 255]));
    texture.save(folder.join("texture.png")).unwrap();
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},
        "buffers":[{{"byteLength":{},"uri":"square.bin"}}],
        "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":48}},{{"buffer":0,"byteOffset":48,"byteLength":32}},{{"buffer":0,"byteOffset":80,"byteLength":12}}],
        "accessors":[
            {{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3","min":[-1,-1,0],"max":[1,1,0]}},
            {{"bufferView":1,"componentType":5126,"count":4,"type":"VEC2"}},
            {{"bufferView":2,"componentType":5123,"count":6,"type":"SCALAR"}}
        ],
        "images":[{{"uri":"texture.png"}}],"textures":[{{"source":0}}],
        "materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}},"doubleSided":{double_sided}}}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":1}},"indices":2,"material":0}}]}}],
        "nodes":[{{"mesh":0}}]}}"#,
        bytes.len()
    );
    let path = folder.join("square.gltf");
    fs::write(&path, json).unwrap();
    path
}

/// A model's texture repeats where its sampler says so, and a double-sided material shows from
/// behind, lit as though it faced that way, which a one-sided material doesn't.
#[test]
#[cfg(feature = "models")]
fn models_repeat_textures_and_show_double_sided_backs() {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("square_models");
    let mut headless = headless();
    let ctx = headless.context();
    let one_sided = ctx
        .load_model(write_square_model(&folder.join("one"), false))
        .unwrap();
    let both_sided = ctx
        .load_model(write_square_model(&folder.join("both"), true))
        .unwrap();
    let background = [0x20, 0x30, 0x60, 255];
    let mut render = |model: &Model, from: f32| {
        let frame = headless
            .render(64, 64, |canvas| {
                canvas.clear(Color::rgb8(0x20, 0x30, 0x60));
                let camera = Camera3d::orthographic([0.0, 0.0, from], [0.0; 3], 2.0);
                // Lit from the camera, so only what faces it is lit: a back only if it's turned.
                let view = View3d {
                    lighting: Lighting {
                        direction: [0.0, 0.0, -from.signum()],
                        color: Color::WHITE,
                        ambient: Color::BLACK,
                    },
                    ..View3d::new(camera, Rect::new(0.0, 0.0, 64.0, 64.0))
                };
                canvas
                    .begin_3d(view)
                    .draw_model(model, Affine3::IDENTITY, Color::WHITE);
            })
            .unwrap();
        // A pixel in each quarter of a row across the middle.
        [8, 24, 40, 56].map(|x| {
            let i = ((32 * 64 + x) * 4) as usize;
            <[u8; 4]>::try_from(&frame.pixels()[i..i + 4]).unwrap()
        })
    };
    let [red, blue] = [[255, 0, 0, 255], [0, 0, 255, 255]];
    // From the front, the texture twice across, as its coordinates run from 0 to 2.
    assert_eq!(render(&one_sided, 5.0), [red, blue, red, blue]);
    assert_eq!(render(&both_sided, 5.0), [red, blue, red, blue]);
    // From behind, mirrored, and lit as the front is from the front.
    assert_eq!(render(&one_sided, -5.0), [background; 4]);
    assert_eq!(render(&both_sided, -5.0), [blue, red, blue, red]);
}

/// Sprites standing upright on the ground of a 3D view seen from above at an angle, each facing
/// the camera: a hero in front of a crate, a tree behind it and partly hidden, and the hero
/// mirrored by a negative width. They're lit as the ground is, so the white hero shows as bright
/// as white ground would. A few pixels may differ, as for `meshes_textured`.
#[test]
fn meshes_billboards() {
    let mut headless = headless();
    let sheet = SpriteSheet::new(headless.context().load_texture(TILESET).unwrap(), 16, 16);
    let ctx = headless.context();
    let cube = ctx.create_mesh(&MeshData::cube(1.0)).unwrap();
    let ground = ctx.create_mesh(&MeshData::plane([5.0, 5.0])).unwrap();
    let hero = sheet.sprite(27, 0);
    let tree = sheet.sprite(0, 1);
    let frame = headless
        .render(128, 96, |canvas| {
            canvas.clear(Color::rgb8(0x10, 0x10, 0x18));
            let camera = Camera3d::perspective([1.03, 3.52, 4.01], [0.0, 0.3, 0.0], 0.8);
            let rect = Rect::new(0.0, 0.0, 128.0, 96.0);
            let mut view = canvas.begin_3d(View3d::new(camera, rect));
            view.draw_mesh(
                &ground,
                Affine3::IDENTITY,
                None,
                Color::rgb8(0x3e, 0x6b, 0x3f),
            );
            let crate_ = Affine3::from_translation([0.0, 0.5, 0.0]);
            view.draw_mesh(&cube, crate_, None, Color::rgb8(0xb0, 0x7a, 0x48));
            view.draw_billboard(&hero, [0.3, 0.0, 1.0], [1.0, 1.0], Color::WHITE);
            let tree_tint = Color::rgb8(0x6a, 0xa8, 0x4f);
            view.draw_billboard(&tree, [-0.4, 0.0, -1.2], [1.5, 1.5], tree_tint);
            let skin = Color::rgb8(0xf2, 0xd3, 0xab);
            view.draw_billboard(&hero, [1.6, 0.0, 0.2], [-1.0, 1.0], skin);
        })
        .unwrap();
    assert_screenshot_allowing(&headless.adapter_name(), "meshes_billboards", frame, 16);
}

/// A billboard below a camera in perspective looking steeply down, but behind where it looks
/// across the ground, turns to face it rather than showing its back, which isn't drawn.
#[test]
fn meshes_billboards_below_the_camera() {
    let mut headless = headless();
    let sheet = SpriteSheet::new(headless.context().load_texture(TILESET).unwrap(), 16, 16);
    let tree = sheet.sprite(0, 1);
    let background = Color::rgb8(0x10, 0x10, 0x18);
    let frame = headless
        .render(64, 64, |canvas| {
            canvas.clear(background);
            let camera = Camera3d::perspective([0.0, 2.0, 0.0], [0.0, 0.0, -0.5], 1.5);
            let view = View3d {
                lighting: Lighting::UNLIT,
                ..View3d::new(camera, Rect::new(0.0, 0.0, 64.0, 64.0))
            };
            canvas
                .begin_3d(view)
                .draw_billboard(&tree, [0.0, 0.0, 0.8], [1.0, 1.0], Color::WHITE);
        })
        .unwrap();
    let corner = frame.pixel(0, 0);
    let shown = frame.pixels().chunks(4).filter(|p| *p != corner).count();
    assert!(shown > 30, "the billboard shows only {shown} pixels");
}

/// Billboards face the camera however it's rolled about the way it looks, standing upright in
/// the world, so the view turns with the camera: rolled a half turn, it shows them upside down,
/// and a quarter turn, on their sides. A negative width still mirrors them.
#[test]
fn meshes_billboards_rolled() {
    let mut headless = headless();
    let sheet = SpriteSheet::new(headless.context().load_texture(TILESET).unwrap(), 16, 16);
    let hero = sheet.sprite(27, 0);
    let background = Color::rgb8(0x10, 0x10, 0x18);
    let mut render = |up: [f32; 3], width: f32| {
        headless
            .render(64, 64, |canvas| {
                canvas.clear(background);
                let camera = Camera3d {
                    up,
                    ..Camera3d::orthographic([0.0, 1.0, 5.0], [0.0, 0.5, 0.0], 2.0)
                };
                let view = View3d {
                    lighting: Lighting::UNLIT,
                    ..View3d::new(camera, Rect::new(0.0, 0.0, 64.0, 64.0))
                };
                canvas
                    .begin_3d(view)
                    .draw_billboard(&hero, [0.0; 3], [width, 1.0], Color::WHITE);
            })
            .unwrap()
    };
    let upright = render([0.0, 1.0, 0.0], 1.0);
    let corner = upright.pixel(0, 0);
    let shown = upright.pixels().chunks(4).filter(|p| *p != corner).count();
    assert!(shown > 100, "the billboard shows only {shown} pixels");

    // Up the view is the world's `+x`, so its up is to the left of the view.
    let on_its_side = transformed(&upright, imageops::rotate270);
    assert_alike(
        &on_its_side,
        &render([1.0, 0.0, 0.0], 1.0),
        16,
        "a quarter turn",
    );
    let upside_down = transformed(&upright, imageops::rotate180);
    assert_alike(
        &upside_down,
        &render([0.0, -1.0, 0.0], 1.0),
        16,
        "a half turn",
    );
    let on_its_other_side = transformed(&upright, imageops::rotate90);
    assert_alike(
        &on_its_other_side,
        &render([-1.0, 0.0, 0.0], 1.0),
        16,
        "three quarters of a turn",
    );

    let mirrored = transformed(&upright, imageops::flip_horizontal);
    assert!(
        !mirrored
            .compare(&upright, CHANNEL_TOLERANCE)
            .differs_by_at_most(0),
        "the sprite is lopsided"
    );
    assert_alike(&mirrored, &render([0.0, 1.0, 0.0], -1.0), 16, "mirrored");
    assert_alike(
        &transformed(&mirrored, imageops::rotate180),
        &render([0.0, -1.0, 0.0], -1.0),
        16,
        "mirrored, a half turn",
    );
}

/// A triangle whose normal points out of it at its bottom-left corner and up at its others, lit
/// from straight above, so it brightens away from that corner. How long its normals are doesn't
/// change its shading, nor do transforms that stretch or mirror it but don't turn its normals.
#[test]
fn meshes_normals() {
    let mut headless = headless();
    let triangle = |out_of_it: f32| MeshData {
        vertices: vec![
            MeshVertex::new([-1.0, -1.0, 0.0], [0.0, 0.0, out_of_it], [0.0; 2]),
            MeshVertex::new([1.0, -1.0, 0.0], [0.0, 1.0, 0.0], [0.0; 2]),
            MeshVertex::new([0.0, 1.0, 0.0], [0.0, 1.0, 0.0], [0.0; 2]),
        ],
        indices: vec![0, 1, 2],
    };
    let ctx = headless.context();
    let unit = ctx.create_mesh(&triangle(1.0)).unwrap();
    let long = ctx.create_mesh(&triangle(10.0)).unwrap();
    let mut render = |mesh: &Mesh, transform: Affine3| {
        headless
            .render(64, 64, |canvas| {
                canvas.clear(Color::rgb8(0x20, 0x30, 0x60));
                let camera = Camera3d::orthographic([0.0, 0.0, 5.0], [0.0; 3], 2.0);
                let view = View3d {
                    lighting: Lighting {
                        direction: [0.0, -1.0, 0.0],
                        color: Color::WHITE,
                        ambient: Color::BLACK,
                    },
                    ..View3d::new(camera, Rect::new(0.0, 0.0, 64.0, 64.0))
                };
                canvas
                    .begin_3d(view)
                    .draw_mesh(mesh, transform, None, Color::WHITE);
            })
            .unwrap()
    };
    let frame = render(&unit, Affine3::IDENTITY);
    let longer = render(&long, Affine3::IDENTITY);
    assert_alike(&frame, &longer, 0, "a longer normal");
    // Stretching it along `z` makes the normals pointing up three times as long as the one out of
    // it, but leaves their directions as they were, mirrored or not.
    let stretched = render(&unit, Affine3::from_scale([1.0, 1.0, 3.0]));
    assert_alike(&frame, &stretched, 0, "stretched");
    let mirrored = render(&unit, Affine3::from_scale([-1.0, 1.0, 3.0]));
    let flipped = transformed(&frame, imageops::flip_horizontal);
    assert_alike(&flipped, &mirrored, 0, "mirrored and stretched");

    assert_screenshot(&headless.adapter_name(), "meshes_normals", frame);
}

/// A floor, a cube and a ball in a view with no sun and little ambient light, lit by a warm light
/// over the cube and a brighter, cooler one beside the ball: each lights what faces it, less the
/// farther off it is, to nothing at its range, and where both reach, their light adds. A light
/// added after the meshes lights them as one added before does.
#[test]
fn lights_at_places() {
    let mut headless = headless();
    let ctx = headless.context();
    let ground = ctx.create_mesh(&MeshData::plane([8.0, 8.0])).unwrap();
    let cube = ctx.create_mesh(&MeshData::cube(1.0)).unwrap();
    let sphere = ctx.create_mesh(&MeshData::sphere(0.6, 16, 8)).unwrap();
    let camera = Camera3d::perspective([0.03, 3.51, 5.02], [0.0, 0.3, 0.0], 0.8);
    let view = View3d {
        lighting: Lighting {
            direction: [0.0, -1.0, 0.0],
            color: Color::BLACK,
            ambient: Color::rgb8(0x18, 0x18, 0x20),
        },
        ..View3d::new(camera, Rect::new(0.0, 0.0, 128.0, 96.0))
    };
    let warm = Light::point([-1.5, 1.6, 0.5], Color::rgb8(0xff, 0xd0, 0x90), 4.0);
    let cool = Light {
        energy: 1.5,
        ..Light::point([1.6, 1.2, 0.8], Color::rgb8(0x60, 0xa0, 0xff), 3.0)
    };
    let mut render = |lights: &[Light]| {
        headless
            .render(128, 96, |canvas| {
                canvas.clear(Color::rgb8(0x08, 0x08, 0x10));
                let mut view = canvas.begin_3d(view);
                if let Some(first) = lights.first() {
                    view.add_light(*first);
                }
                let grey = Color::rgb8(0xa0, 0xa0, 0xa0);
                view.draw_mesh(&ground, Affine3::IDENTITY, None, grey);
                let turned =
                    Affine3::from_translation([-1.2, 0.5, -0.4]) * Affine3::from_rotation_y(0.5);
                view.draw_mesh(&cube, turned, None, Color::rgb8(0xd0, 0x60, 0x40));
                let right = Affine3::from_translation([1.2, 0.6, 0.0]);
                view.draw_mesh(&sphere, right, None, Color::WHITE);
                for light in lights.iter().skip(1) {
                    view.add_light(*light);
                }
            })
            .unwrap()
    };
    let unlit = render(&[]);
    let frame = render(&[warm, cool]);
    let swapped = render(&[cool, warm]);
    assert_alike(&frame, &swapped, 0, "lights added in the other order");

    let pixel = |image: &Image, point: [f32; 3]| {
        let [x, y] = view.project(point).expect("it's in view");
        image.pixel(x as u32, y as u32)
    };
    // The floor's far corner is out of both lights' reach, so shows by the ambient light alone.
    let corner = [3.5, 0.0, -3.5];
    assert_eq!(warm.level_at(corner) + cool.level_at(corner), 0.0);
    assert_eq!(pixel(&frame, corner), pixel(&unlit, corner));
    // Under the warm light the floor is lit warm, and between the two, by both.
    let under = pixel(&frame, [-1.5, 0.0, 1.4]);
    assert!(under[0] > 100 && under[0] > under[2] + 20, "{under:?}");
    let between = [0.4, 0.0, 1.6];
    let both = pixel(&frame, between);
    let warm_only = pixel(&render(&[warm]), between);
    let cool_only = pixel(&render(&[cool]), between);
    for channel in 0..3 {
        assert!(
            both[channel] >= warm_only[channel].max(cool_only[channel]),
            "{both:?} isn't {warm_only:?} and {cool_only:?} together"
        );
    }
    assert!(both[0] > cool_only[0] && both[2] > warm_only[2], "{both:?}");

    assert_screenshot(&headless.adapter_name(), "lights_at_places", frame);
}

/// Two spot lights shining straight down on a floor seen from above: one with a hard rim, which
/// lights a disc and nothing outside it, and one fading across its whole cone. A third, turned
/// away from the floor, lights nothing.
#[test]
fn lights_spots() {
    let mut headless = headless();
    let ground = headless
        .context()
        .create_mesh(&MeshData::plane([12.0, 6.0]))
        .unwrap();
    let camera = Camera3d::orthographic([0.0, 10.0, 0.0], [0.0; 3], 6.0);
    let view = View3d {
        lighting: Lighting {
            direction: [0.0, -1.0, 0.0],
            color: Color::BLACK,
            ambient: Color::BLACK,
        },
        ..View3d::new(camera, Rect::new(0.0, 0.0, 128.0, 64.0))
    };
    let down = [0.0, -1.0, 0.0];
    let hard = Light::spot([-3.0, 3.0, 0.0], down, 0.5, Color::WHITE, 6.0);
    let soft = Light {
        edge: 1.0,
        ..Light::spot(
            [3.0, 3.0, 0.0],
            down,
            0.5,
            Color::rgb8(0xff, 0xd0, 0x90),
            6.0,
        )
    };
    let away = Light::spot([0.0, 3.0, 0.0], [0.0, 1.0, 0.0], 0.5, Color::WHITE, 6.0);
    let frame = headless
        .render(128, 64, |canvas| {
            canvas.clear(Color::rgb8(0x20, 0x30, 0x60));
            let mut view = canvas.begin_3d(view);
            view.draw_mesh(&ground, Affine3::IDENTITY, None, Color::WHITE);
            for light in [hard, soft, away] {
                view.add_light(light);
            }
        })
        .unwrap();
    let pixel = |point: [f32; 3]| {
        let [x, y] = view.project(point).expect("it's in view");
        frame.pixel(x as u32, y as u32)
    };
    // The hard cone's rim on the floor is 3 tan 0.5 from under it, about 1.64.
    let black = [0, 0, 0, 255];
    let [middle, inside, outside] = [0.05, 1.4, 1.9].map(|x| pixel([-3.0 + x, 0.0, 0.1]));
    assert!(middle[0] > 180, "{middle:?}");
    assert!(inside[0] > 120 && inside[0] < middle[0], "{inside:?}");
    assert_eq!(outside, black);
    // The soft one fades all the way from its middle.
    let [middle, halfway, outside] = [0.05, 1.3, 1.9].map(|x| pixel([3.0 + x, 0.0, 0.1]));
    assert!(middle[0] > 180, "{middle:?}");
    assert!(
        halfway[0] > 40 && halfway[0] < middle[0] - 40,
        "{halfway:?}"
    );
    assert_eq!(outside, black);
    assert_eq!(pixel([0.05, 0.0, 0.1]), black, "under the one turned away");

    assert_screenshot(&headless.adapter_name(), "lights_spots", frame);
}

/// A billboard takes a light's whole level wherever the light is from it, having no side to turn
/// away, where a wall facing the camera, lit from its side, is lit by how squarely it faces the
/// light.
#[test]
fn lights_on_billboards() {
    let mut headless = headless();
    let sheet = SpriteSheet::new(headless.context().load_texture(TILESET).unwrap(), 16, 16);
    let sprite = sheet.sprite(27, 0);
    let wall = headless
        .context()
        .create_mesh(&MeshData::cuboid([1.0, 1.0, 0.1]))
        .unwrap();
    let camera = Camera3d::orthographic([0.0, 0.5, 5.0], [0.0, 0.5, 0.0], 2.0);
    let dark = View3d {
        lighting: Lighting {
            direction: [0.0, -1.0, 0.0],
            color: Color::BLACK,
            ambient: Color::BLACK,
        },
        ..View3d::new(camera, Rect::new(0.0, 0.0, 64.0, 32.0))
    };
    let unlit = View3d {
        lighting: Lighting::UNLIT,
        ..dark
    };
    // Level with the middle of both, a little in front of them, between the two.
    let lamp = Light::point([0.0, 0.5, 0.2], Color::WHITE, 4.0);
    let mut render = |view: View3d, lights: &[Light]| {
        headless
            .render(64, 32, |canvas| {
                canvas.clear(Color::BLACK);
                let mut view = canvas.begin_3d(view);
                for light in lights {
                    view.add_light(*light);
                }
                view.draw_billboard(&sprite, [-1.0, 0.0, 0.0], [1.0, 1.0], Color::WHITE);
                let right = Affine3::from_translation([1.0, 0.5, -0.05]);
                view.draw_mesh(&wall, right, Some(&sprite), Color::WHITE);
            })
            .unwrap()
    };
    let lit = render(dark, &[lamp]);
    let shown = render(unlit, &[]);
    // How bright each is by the lamp, of how bright it is shown as it is: the left half of the
    // frame is the billboard's, and the right the wall's.
    let part = |columns: std::ops::Range<u32>| {
        let sum = |image: &Image| -> f32 {
            let pixels = columns.clone().flat_map(|x| (0..32).map(move |y| (x, y)));
            pixels
                .map(|(x, y)| {
                    let [r, g, b, _] = image.pixel(x, y);
                    f32::from(r) + f32::from(g) + f32::from(b)
                })
                .sum()
        };
        assert!(sum(&shown) > 1000.0, "nothing shows in {columns:?}");
        sum(&lit) / sum(&shown)
    };
    let [billboard, mesh] = [part(0..32), part(32..64)];
    // Both are about 1 from the lamp, of its 4, so three quarters lit but for facing it: the
    // wall faces a fifth toward it.
    assert!(billboard > 0.75 && billboard < 0.97, "{billboard}");
    assert!(mesh > 0.2 && mesh < 0.6, "{mesh}");
}

/// A floor running away from the camera, cubes along it and a billboard, in fog the color the
/// canvas is cleared to: what's nearer than the fog shows as it does without, what's past it is
/// the fog's color and nothing else, so the floor's far end can't be told from what's behind it,
/// and between the two things fade, lit and shaded or not.
#[test]
fn fog_over_distance() {
    let mut headless = headless();
    let ctx = headless.context();
    let ground = ctx.create_mesh(&MeshData::plane([6.0, 40.0])).unwrap();
    let cube = ctx.create_mesh(&MeshData::cube(1.0)).unwrap();
    let sheet = SpriteSheet::new(ctx.load_texture(TILESET).unwrap(), 16, 16);
    let sprite = sheet.sprite(27, 0);
    let camera = Camera3d::perspective([0.0, 2.0, 2.0], [0.0, 0.5, -10.0], 0.8);
    let sky = Color::rgb8(0x60, 0x80, 0xa0);
    let fog = Fog::new(sky, 6.0, 18.0);
    let clear = View3d::new(camera, Rect::new(0.0, 0.0, 128.0, 96.0));
    let foggy = View3d {
        fog: Some(fog),
        ..clear
    };
    let lamp = Light {
        shadows: true,
        ..Light::point([0.5, 2.5, -2.0], Color::rgb8(0xff, 0xd0, 0x90), 8.0)
    };
    let mut render = |view: View3d| {
        headless
            .render(128, 96, |canvas| {
                canvas.clear(sky);
                let mut view = canvas.begin_3d(view);
                view.add_light(lamp);
                let floor = Affine3::from_translation([0.0, 0.0, -18.0]);
                view.draw_mesh(&ground, floor, None, Color::rgb8(0xa0, 0xa0, 0xa0));
                for [x, z] in [[-1.5, -3.0], [1.5, -13.0], [-1.5, -22.0]] {
                    let place = Affine3::from_translation([x, 0.5, z]);
                    view.draw_mesh(&cube, place, None, Color::rgb8(0xd0, 0x60, 0x40));
                }
                view.draw_billboard(&sprite, [-1.0, 0.0, -8.0], [1.0, 1.5], Color::WHITE);
            })
            .unwrap()
    };
    let shown = render(clear);
    let frame = render(foggy);
    let pixel = |image: &Image, point: [f32; 3]| {
        let [x, y] = clear.project(point).expect("it's in view");
        image.pixel(x as u32, y as u32)
    };
    let amount = |point: [f32; 3]| fog.amount_at(foggy.distance(point));

    // The floor before the fog begins is as it is without.
    let near = [0.0, 0.0, -3.0];
    assert_eq!(amount(near), 0.0);
    assert_eq!(pixel(&frame, near), pixel(&shown, near));
    // Past where it's whole, the floor is the color behind it, which without fog it isn't.
    let far = [0.0, 0.0, -30.0];
    assert_eq!(amount(far), 1.0);
    let behind = frame.pixel(64, 2);
    assert_eq!(pixel(&frame, far), behind);
    assert_ne!(pixel(&shown, far), behind);
    // And between, it's between the two.
    let middle = [2.5, 0.0, -9.0];
    assert!(amount(middle) > 0.4 && amount(middle) < 0.5);
    let [with, without] = [pixel(&frame, middle), pixel(&shown, middle)];
    for channel in 0..3 {
        let [low, high] = if without[channel] < behind[channel] {
            [without[channel], behind[channel]]
        } else {
            [behind[channel], without[channel]]
        };
        assert!(
            with[channel] > low && with[channel] < high,
            "{with:?} isn't between {without:?} and {behind:?}"
        );
    }

    assert_screenshot(&headless.adapter_name(), "fog_over_distance", frame);
}

/// A view with no sun and no ambient light, so what a light doesn't reach is black.
fn dark_view(camera: Camera3d, [width, height]: [f32; 2]) -> View3d {
    View3d {
        lighting: Lighting {
            direction: [0.0, -1.0, 0.0],
            color: Color::BLACK,
            ambient: Color::BLACK,
        },
        ..View3d::new(camera, Rect::new(0.0, 0.0, width, height))
    }
}

/// A floor, a cube and a ball lit by one light with shadows: the cube and the ball shade the
/// floor behind them, with hard edges, and nothing the light reaches is shaded by itself,
/// however steeply the light meets it.
#[test]
fn shadows_from_lights() {
    let mut headless = headless();
    let ctx = headless.context();
    let ground = ctx.create_mesh(&MeshData::plane([8.0, 8.0])).unwrap();
    let cube = ctx.create_mesh(&MeshData::cube(1.0)).unwrap();
    let sphere = ctx.create_mesh(&MeshData::sphere(0.6, 16, 8)).unwrap();
    let camera = Camera3d::perspective([0.03, 3.51, 5.02], [0.0, 0.3, 0.0], 0.8);
    let view = dark_view(camera, [128.0, 96.0]);
    let lamp = Light {
        energy: 1.5,
        ..Light::point([0.3, 1.5, 0.2], Color::rgb8(0xff, 0xd0, 0x90), 7.0)
    };
    let shading = Light {
        shadows: true,
        ..lamp
    };
    let mut render = |light: Light, things: bool| {
        headless
            .render(128, 96, |canvas| {
                canvas.clear(Color::rgb8(0x08, 0x08, 0x10));
                let mut view = canvas.begin_3d(view);
                view.add_light(light);
                let grey = Color::rgb8(0xa0, 0xa0, 0xa0);
                view.draw_mesh(&ground, Affine3::IDENTITY, None, grey);
                if things {
                    let turned = Affine3::from_translation([-1.2, 0.5, -0.4])
                        * Affine3::from_rotation_y(0.5);
                    view.draw_mesh(&cube, turned, None, Color::rgb8(0xd0, 0x60, 0x40));
                    let right = Affine3::from_translation([1.2, 0.6, 0.0]);
                    view.draw_mesh(&sphere, right, None, Color::WHITE);
                }
            })
            .unwrap()
    };
    // A floor alone shades nothing, though it casts shadows, out to where the light all but
    // runs along it.
    let floor = render(lamp, false);
    assert_alike(&floor, &render(shading, false), 0, "a floor with shadows");

    let plain = render(lamp, true);
    let frame = render(shading, true);
    let pixel = |image: &Image, point: [f32; 3]| {
        let [x, y] = view.project(point).expect("it's in view");
        image.pixel(x as u32, y as u32)
    };
    let black = [0, 0, 0, 255];
    // Past the cube from the light, and past the ball, the floor is in their shadows.
    for behind in [[-2.9, 0.0, -1.1], [2.6, 0.0, -0.3]] {
        assert_ne!(pixel(&plain, behind), black, "{behind:?} without shadows");
        assert_eq!(pixel(&frame, behind), black, "{behind:?}");
    }
    // What the light reaches is lit as it is without shadows: the floor before them, the cube's
    // top and the side of it the light is on, and the ball's.
    for reached in [
        [0.5, 0.0, 2.0],
        [-0.3, 0.0, -2.5],
        [-1.2, 1.0, -0.4],
        [-0.72, 0.6, -0.1],
        [0.75, 0.9, 0.1],
    ] {
        let lit = pixel(&plain, reached);
        assert_ne!(lit, black, "{reached:?} without shadows");
        assert_eq!(pixel(&frame, reached), lit, "{reached:?}");
    }
    // Shadows only take light away.
    let pairs = frame.pixels().iter().zip(plain.pixels());
    assert!(pairs.clone().all(|(shaded, lit)| shaded <= lit));
    assert!(pairs.filter(|(shaded, lit)| shaded < lit).count() > 600);

    assert_screenshot(&headless.adapter_name(), "shadows_from_lights", frame);
}

/// A floor seen from above, and over it two lights with a slab under each: a spot light, whose
/// shadows are in one map, and a point light, whose are in six. Each slab's shadow is twice its
/// size, the light being twice as far from the floor as from the slab, and its edge is hard.
#[test]
fn shadows_from_spots_and_points() {
    let mut headless = headless();
    let ctx = headless.context();
    let ground = ctx.create_mesh(&MeshData::plane([12.0, 6.0])).unwrap();
    let slab = ctx.create_mesh(&MeshData::cuboid([0.6, 0.1, 0.6])).unwrap();
    let camera = Camera3d::orthographic([0.0, 10.0, 0.0], [0.0; 3], 6.0);
    let view = dark_view(camera, [128.0, 64.0]);
    let spot = Light {
        shadows: true,
        ..Light::spot([-3.0, 3.0, 0.0], [0.0, -1.0, 0.0], 0.5, Color::WHITE, 6.0)
    };
    let point = Light {
        shadows: true,
        ..Light::point([3.0, 3.0, 0.0], Color::rgb8(0xff, 0xd0, 0x90), 6.0)
    };
    assert_eq!([spot.shadow_maps(), point.shadow_maps()], [1, 6]);
    let mut render = |slabs: bool| {
        headless
            .render(128, 64, |canvas| {
                canvas.clear(Color::rgb8(0x20, 0x30, 0x60));
                let mut view = canvas.begin_3d(view);
                view.draw_mesh(&ground, Affine3::IDENTITY, None, Color::WHITE);
                for light in [spot, point] {
                    view.add_light(light);
                    let [x, _, z] = light.position;
                    if slabs {
                        let under = Affine3::from_translation([x, 1.5, z]);
                        view.draw_mesh(&slab, under, None, Color::rgb8(0x60, 0xa0, 0xff));
                    }
                }
            })
            .unwrap()
    };
    let open = render(false);
    let frame = render(true);
    let pixel = |image: &Image, point: [f32; 3]| {
        let [x, y] = view.project(point).expect("it's in view");
        image.pixel(x as u32, y as u32)
    };
    let black = [0, 0, 0, 255];
    for light in [spot, point] {
        let [x, _, z] = light.position;
        // The slab reaches 0.3 from under the light, and its shadow 0.6: between them the floor
        // is dark, across and along, and past them it's lit as it is without the slab.
        for [dx, dz] in [[0.45, 0.0], [-0.45, 0.0], [0.0, 0.45], [0.0, -0.45]] {
            let shaded = [x + dx, 0.0, z + dz];
            assert_ne!(pixel(&open, shaded), black, "{shaded:?} without the slab");
            assert_eq!(pixel(&frame, shaded), black, "{shaded:?}");
            let past = [x + dx * 1.7, 0.0, z + dz * 1.7];
            assert_ne!(pixel(&open, past), black, "{past:?} without the slab");
            assert_eq!(pixel(&frame, past), pixel(&open, past), "{past:?}");
        }
    }

    assert_screenshot(
        &headless.adapter_name(),
        "shadows_from_spots_and_points",
        frame,
    );
}

/// What shades a wall from a light before it: a mesh, but not one drawn after the view is told
/// meshes cast no shadows, nor a billboard, nor the pixels of a mesh's sprite that don't show.
#[test]
fn shadows_are_cast_by_meshes_that_show() {
    let mut headless = headless();
    let sheet = SpriteSheet::new(headless.context().load_texture(TILESET).unwrap(), 16, 16);
    let sprite = sheet.sprite(27, 0);
    let ctx = headless.context();
    let wall = ctx
        .create_mesh(&MeshData::cuboid([10.0, 4.0, 0.2]))
        .unwrap();
    let card = ctx
        .create_mesh(&MeshData::cuboid([1.0, 1.0, 0.02]))
        .unwrap();
    let camera = Camera3d::orthographic([0.0, 0.5, 5.0], [0.0, 0.5, 0.0], 3.0);
    let view = dark_view(camera, [128.0, 48.0]);
    // On the camera's line, twice as far from the wall as what's drawn before it, a little to
    // the right: so each thing's shadow is twice its size, and falls to its right.
    let lamp = Light {
        shadows: true,
        ..Light::point([0.0, 0.5, 3.0], Color::WHITE, 12.0)
    };
    #[derive(Clone, Copy, PartialEq)]
    enum Thing {
        Nothing,
        Card,
        CardWithoutShadow,
        Billboard,
        Sprite,
    }
    let mut render = |thing: Thing| {
        headless
            .render(128, 48, |canvas| {
                canvas.clear(Color::BLACK);
                let mut view = canvas.begin_3d(view);
                view.add_light(lamp);
                let back = Affine3::from_translation([0.0, 0.5, -0.1]);
                view.draw_mesh(&wall, back, None, Color::WHITE);
                let before = Affine3::from_translation([1.5, 0.5, 1.5]);
                match thing {
                    Thing::Nothing => {}
                    Thing::Card => view.draw_mesh(&card, before, None, Color::WHITE),
                    Thing::CardWithoutShadow => {
                        view.set_casts_shadow(false);
                        view.draw_mesh(&card, before, None, Color::WHITE);
                    }
                    Thing::Billboard => {
                        view.draw_billboard(&sprite, [1.5, 0.0, 1.5], [1.0, 1.0], Color::WHITE);
                    }
                    Thing::Sprite => view.draw_mesh(&card, before, Some(&sprite), Color::WHITE),
                }
            })
            .unwrap()
    };
    // How many of the wall's pixels are dark where the card's shadow falls: from 2 to 4 across,
    // and from 0 to 1 up, level with the light.
    let [left, top] = view.project([2.1, 0.9, 0.0]).unwrap().map(|v| v as u32);
    let [right, bottom] = view.project([3.9, 0.1, 0.0]).unwrap().map(|v| v as u32);
    let mut dark = |thing: Thing| {
        let image = render(thing);
        let pixels = (left..right).flat_map(|x| (top..bottom).map(move |y| (x, y)));
        pixels
            .filter(|&(x, y)| image.pixel(x, y) == [0, 0, 0, 255])
            .count()
    };
    let all = ((right - left) * (bottom - top)) as usize;
    assert!(all > 300, "{all}");
    assert_eq!(dark(Thing::Nothing), 0);
    assert_eq!(dark(Thing::Card), all);
    assert_eq!(dark(Thing::CardWithoutShadow), 0);
    assert_eq!(dark(Thing::Billboard), 0);
    // The sprite is a figure with clear pixels around it, and so is its shadow.
    let figure = dark(Thing::Sprite);
    assert!(
        figure > all / 5 && figure < all * 4 / 5,
        "{figure} of {all}"
    );

    let frame = render(Thing::Sprite);
    assert_screenshot(&headless.adapter_name(), "shadows_cut_out", frame);
}

/// Five point lights with shadows over a floor, a slab under each, want thirty shadow maps of a
/// view's twenty-four: the four nearest the camera have theirs, and the farthest lights the
/// floor under its slab as though the slab weren't there.
#[test]
fn shadows_run_out_farthest_from_the_camera_first() {
    let mut headless = headless();
    let ctx = headless.context();
    let ground = ctx.create_mesh(&MeshData::plane([12.0, 6.0])).unwrap();
    let slab = ctx
        .create_mesh(&MeshData::cuboid([0.4, 0.05, 0.4]))
        .unwrap();
    // A little left of the middle, so the light farthest right is the farthest from it.
    let camera = Camera3d::orthographic([-0.5, 10.0, 0.0], [-0.5, 0.0, 0.0], 6.0);
    let view = dark_view(camera, [128.0, 64.0]);
    let lights = [-4.0, -2.0, 0.0, 2.0, 4.0].map(|x| Light {
        shadows: true,
        ..Light::point([x, 2.0, 0.0], Color::WHITE, 2.5)
    });
    let mut render = |slabs: bool| {
        headless
            .render(128, 64, |canvas| {
                canvas.clear(Color::BLACK);
                let mut view = canvas.begin_3d(view);
                view.draw_mesh(&ground, Affine3::IDENTITY, None, Color::WHITE);
                // The farthest first, so it isn't the order they're added in that decides.
                for light in lights.iter().rev() {
                    view.add_light(*light);
                    if slabs {
                        let under = Affine3::from_translation([light.position[0], 1.0, 0.0]);
                        view.draw_mesh(&slab, under, None, Color::WHITE);
                    }
                }
            })
            .unwrap()
    };
    let open = render(false);
    let frame = render(true);
    let pixel = |image: &Image, point: [f32; 3]| {
        let [x, y] = view.project(point).expect("it's in view");
        image.pixel(x as u32, y as u32)
    };
    for light in lights {
        // Between the slab's edge, 0.2 from under the light, and its shadow's, 0.4 from it.
        let x = light.position[0];
        let beside = [x, 0.0, 0.3];
        let lit = pixel(&open, beside);
        assert_ne!(lit, [0, 0, 0, 255], "{beside:?} without the slab");
        if x == 4.0 {
            assert_eq!(pixel(&frame, beside), lit, "the farthest has no shadows");
        } else {
            assert_eq!(pixel(&frame, beside), [0, 0, 0, 255], "{beside:?}");
        }
    }
}

/// A 3D view covers the pixels a 2D fill of its rect does, whichever pixels' middles its edges
/// fall on, on the target or hanging off it: a plane bigger than the view, seen face on, fills
/// it.
#[test]
fn meshes_cover_their_views_rect() {
    let mut headless = headless();
    let plane = headless
        .context()
        .create_mesh(&MeshData::plane([4.0, 4.0]))
        .unwrap();
    let lit = |frame: &Image| {
        let pixels = frame.pixels().chunks(4);
        pixels.filter(|p| *p != [0, 0, 0, 255]).count()
    };
    for offset in [0.49, 0.5, 0.51] {
        for [x, y] in [[0.0, 0.0], [3.0, 0.0], [-3.0, -2.0], [8.0, 9.0]] {
            let rect = Rect::new(x + offset, y + offset, 10.0, 10.0);
            let fill = headless
                .render(16, 16, |canvas| {
                    canvas.clear(Color::BLACK);
                    canvas.fill_rect(rect, Color::WHITE);
                })
                .unwrap();
            let view = headless
                .render(16, 16, |canvas| {
                    canvas.clear(Color::BLACK);
                    let camera = Camera3d::orthographic([0.0, 5.0, 0.0], [0.0; 3], 2.0);
                    let view = View3d {
                        lighting: Lighting::UNLIT,
                        ..View3d::new(camera, rect)
                    };
                    canvas
                        .begin_3d(view)
                        .draw_mesh(&plane, Affine3::IDENTITY, None, Color::WHITE);
                })
                .unwrap();
            assert!(
                fill == view,
                "{rect:?}: the view covers {} pixels, and a fill of its rect {}",
                lit(&view),
                lit(&fill)
            );
        }
    }
}

/// An egui window over a game with a 40x30 canvas drawn 4x, at the window's own resolution, so its
/// text is crisp over the canvas's big pixels. In it, a canvas the engine rendered, shown 2x, and
/// one cell of a texture, shown 2x, both sampled nearest-neighbor.
#[cfg(feature = "egui")]
#[test]
fn gui() {
    use wingine::{
        Game, WindowConfig,
        graphics::Canvas,
        gui::{GuiCanvas, egui},
        headless::Simulation,
    };

    struct Tools {
        view: GuiCanvas,
        sheet: Texture,
    }
    impl Game for Tools {
        fn gui(&mut self, ctx: &mut Context, ui: &mut egui::Ui) {
            let canvas = self.view.begin(24, 16);
            canvas.clear(Color::rgb8(0x14, 0x18, 0x2c));
            canvas.fill_rect(Rect::new(2.0, 2.0, 8.0, 8.0), Color::RED);
            canvas.fill_rect(
                Rect::new(12.0, 6.0, 10.0, 8.0),
                Color::rgb8(0xf2, 0xc1, 0x4e),
            );
            let view = self.view.render(ctx);
            let sheet = ctx.gui_texture(&self.sheet);
            // The cell at column 27, row 0.
            let cell = egui::Rect::from_min_size(
                egui::pos2(27.0 * 16.0 / sheet.size.x, 0.0),
                egui::vec2(16.0, 16.0) / sheet.size,
            );
            egui::Window::new("Tools")
                .fixed_pos([8.0, 8.0])
                .resizable(false)
                .show(ui.ctx(), |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::Image::new(view).fit_to_exact_size(view.size * 2.0));
                        // Sized as the cell is shown, not as the whole sheet is.
                        let shown = egui::load::SizedTexture::new(sheet.id, [32.0, 32.0]);
                        ui.add(egui::Image::new(shown).uv(cell));
                    });
                    let _ = ui.button("Apply");
                });
        }

        fn draw(&mut self, canvas: &mut Canvas) {
            canvas.clear(Color::rgb8(0x2a, 0x3d, 0x2c));
            for x in (0..40).step_by(8) {
                canvas.fill_rect(
                    Rect::new(x as f32, 0.0, 4.0, 30.0),
                    Color::rgb8(0x3e, 0x6b, 0x3f),
                );
            }
        }
    }

    let config = WindowConfig {
        width: 160,
        height: 120,
        resolution: Resolution::Fixed {
            width: 40,
            height: 30,
        },
        ..Default::default()
    };
    let mut simulation = Simulation::new(&config, |ctx| {
        Ok(Tools {
            view: GuiCanvas::new(),
            sheet: ctx.load_texture(TILESET)?,
        })
    })
    .expect("a GPU adapter is available");
    // egui sizes a new window in its first frame, then fades it in.
    simulation.run_for(0.5);
    let frame = simulation.screenshot().unwrap();
    assert_screenshot(&simulation.adapter_name(), "gui", frame);
}

/// egui shows an engine texture's colors multiplied by their alpha, as it blends them: a clear
/// texel that holds a color shows nothing, and a half-opaque one about half its color.
#[cfg(feature = "egui")]
#[test]
fn gui_textures_blend_as_egui_does() {
    use wingine::{Game, WindowConfig, graphics::Canvas, gui::egui, headless::Simulation};

    struct Shown {
        texture: Texture,
    }
    impl Game for Shown {
        fn gui(&mut self, ctx: &mut Context, ui: &mut egui::Ui) {
            let shown = ctx.gui_texture(&self.texture);
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(32.0, 16.0));
            let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0));
            ui.painter().image(shown.id, rect, uv, egui::Color32::WHITE);
        }

        fn draw(&mut self, canvas: &mut Canvas) {
            canvas.clear(Color::BLACK);
        }
    }

    let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join("gui_textures");
    fs::create_dir_all(&folder).unwrap();
    let path = folder.join("texels.png");
    let mut texels = RgbaImage::new(2, 1);
    texels.put_pixel(0, 0, image::Rgba([255, 255, 255, 0]));
    texels.put_pixel(1, 0, image::Rgba([255, 0, 0, 128]));
    texels.save(&path).unwrap();
    let config = WindowConfig {
        width: 32,
        height: 16,
        ..Default::default()
    };
    let mut simulation = Simulation::new(&config, |ctx| {
        Ok(Shown {
            texture: ctx.load_texture(&path)?,
        })
    })
    .expect("a GPU adapter is available");
    simulation.run_frames(1);
    let frame = simulation.screenshot().unwrap();
    assert_eq!(frame.pixel(8, 8)[..3], [0, 0, 0], "the clear texel");
    // Half of red's light, which is about three quarters of its sRGB.
    let [r, g, b, _] = frame.pixel(24, 8);
    assert!(
        (180..=196).contains(&r) && g == 0 && b == 0,
        "{r}, {g}, {b}"
    );
}

fn headless() -> Headless {
    Headless::new().expect(
        "screenshot tests need a GPU adapter. Without a GPU, install a software one: \
         lavapipe (mesa-vulkan-drivers) on Linux; WARP ships with Windows",
    )
}

/// Compares `frame` with the reference screenshot `name`, panicking on a mismatch, or with
/// `UPDATE_SCREENSHOTS` set, replacing the reference.
fn assert_screenshot(adapter: &str, name: &str, frame: Image) {
    assert_screenshot_allowing(adapter, name, frame, 0);
}

/// [`assert_screenshot`], letting up to `allowed` pixels differ by any amount: for textures in
/// perspective, where some pixels' middles land so near the edge between two texels that GPUs
/// pick either.
fn assert_screenshot_allowing(adapter: &str, name: &str, frame: Image, allowed: usize) {
    let reference_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/screenshots")
        .join(format!("{name}.png"));
    let output_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("screenshots");

    let reference = Image::load_png(&reference_path);
    let matches = reference.as_ref().is_ok_and(|reference| {
        let comparison = frame.compare(reference, CHANNEL_TOLERANCE);
        comparison.differs_by_at_most(allowed)
    });
    if matches {
        return;
    }

    if env::var_os("UPDATE_SCREENSHOTS").is_some() {
        save(&frame, &reference_path);
        return;
    }

    let actual_path = output_dir.join(format!("{name}.actual.png"));
    save(&frame, &actual_path);
    let details = match &reference {
        Err(err) => format!("could not read the reference: {err}"),
        Ok(reference) => match frame.compare(reference, CHANNEL_TOLERANCE) {
            Comparison::DifferentSizes => format!(
                "size {}x{} does not match the reference's {}x{}",
                frame.width(),
                frame.height(),
                reference.width(),
                reference.height()
            ),
            Comparison::SameSize { differing, largest } => {
                let diff_path = output_dir.join(format!("{name}.diff.png"));
                save(
                    &frame.diff(reference, CHANNEL_TOLERANCE).unwrap(),
                    &diff_path,
                );
                format!(
                    "{differing} pixels differ by more than {CHANNEL_TOLERANCE} \
                     (largest channel difference: {largest})\n\
                     diff:      {}",
                    diff_path.display()
                )
            }
        },
    };
    panic!(
        "screenshot `{name}` does not match its reference: {details}\n\
         reference: {}\n\
         actual:    {}\n\
         adapter:   {}\n\
         If the change is intended, rerun with UPDATE_SCREENSHOTS=1 and review the new reference.",
        reference_path.display(),
        actual_path.display(),
        adapter,
    );
}

/// Asserts that `actual` matches `expected`, rendered the same scene another way, but for at most
/// `allowed` pixels.
fn assert_alike(expected: &Image, actual: &Image, allowed: usize, what: &str) {
    match actual.compare(expected, CHANNEL_TOLERANCE) {
        comparison if comparison.differs_by_at_most(allowed) => {}
        Comparison::SameSize { differing, largest } => panic!(
            "{what}: {differing} pixels differ by more than {CHANNEL_TOLERANCE} \
             (largest channel difference: {largest})"
        ),
        Comparison::DifferentSizes => panic!("{what}: the frames' sizes differ"),
    }
}

/// `frame` turned or flipped by `transform`, one of `image`'s operations.
fn transformed(frame: &Image, transform: impl Fn(&RgbaImage) -> RgbaImage) -> Image {
    let rgba = RgbaImage::from_raw(frame.width(), frame.height(), frame.pixels().to_vec())
        .expect("frame dimensions match its pixel data");
    let done = transform(&rgba);
    Image::from_pixels(done.width(), done.height(), done.into_raw()).unwrap()
}

fn save(image: &Image, path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    image.save_png(path).unwrap_or_else(|err| panic!("{err}"));
}
