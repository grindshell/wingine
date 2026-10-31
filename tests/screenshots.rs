//! Screenshot tests: each test renders a scene headlessly and compares it with a reference PNG in
//! `tests/screenshots/`.
//!
//! - `UPDATE_SCREENSHOTS=1 cargo test --test screenshots` writes new references for scenes that
//!   are missing one or no longer match. Review the changed PNGs before committing them.
//! - On a mismatch, the rendered frame and a diff image (mismatched pixels in magenta) are written
//!   to `target/tmp/screenshots/`.
//!
//! Matching allows a small per-channel difference, so references rendered on one GPU pass on
//! others, including the software rasterizers used in CI.

use std::{env, fs, path::Path, rc::Rc};

use image::{Rgba, RgbaImage};
use wingine::{
    Context,
    graphics::{Camera, Color, Resolution, SpriteSheet, Texture},
    headless::{Headless, Image},
    math::{Affine, Rect},
    scene::{Assets, Scene, SceneFile, TilesetFile},
    tilemap::{Tile, Tilemap, Tileset},
};

/// Largest per-channel difference for a pixel to still match. GPUs and software rasterizers round
/// sRGB conversion and blending slightly differently.
const CHANNEL_TOLERANCE: u8 = 2;

const TILESET: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/kenney_1-bit-pack_transparent-packed.png"
);

#[test]
fn clear() {
    let mut headless = headless();
    let frame = headless
        .render(16, 16, |canvas| canvas.clear(Color::rgb8(0x33, 0x66, 0x99)))
        .unwrap();
    assert_screenshot(&headless, "clear", frame);
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
    assert_screenshot(&headless, "fill_rects", frame);
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
    assert_screenshot(&headless, "sprites", frame);
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
    assert_screenshot(&headless, "rotated_sprites", frame);
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
    assert_screenshot(&headless, "transformed_sprites", frame);
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
    assert_screenshot(&headless, "scene", frame);
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
    assert_screenshot(&headless, "scene_tilemap", frame);
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
        assert_screenshot(&headless, name, frame);
    }
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
    }
    let ctx = headless.context();
    let font = ctx.default_font().clone();
    let path = files.last().unwrap().0;
    let mut assets = Files { ctx, files };
    let file = assets.scene(path).unwrap();
    Scene::new(&file, path, &font, &mut assets).unwrap()
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
    assert_screenshot(&headless, "batch_order", frame);
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
    assert_screenshot(&headless, "many_quads", frame);
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
    assert_screenshot(&headless, "fixed_resolution", frame);
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
    assert_screenshot(&headless, "camera", frame);
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
    assert_screenshot(&headless, "tilemap", frame);
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
    assert_screenshot(&headless, "text", frame);
}

fn headless() -> Headless {
    Headless::new().expect(
        "screenshot tests need a GPU adapter. Without a GPU, install a software one: \
         lavapipe (mesa-vulkan-drivers) on Linux; WARP ships with Windows",
    )
}

/// Compares `frame` with the reference screenshot `name`, panicking on a mismatch, or with
/// `UPDATE_SCREENSHOTS` set, replacing the reference.
fn assert_screenshot(headless: &Headless, name: &str, frame: Image) {
    let actual = RgbaImage::from_raw(frame.width(), frame.height(), frame.into_pixels())
        .expect("frame dimensions match its pixel data");
    let reference_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/screenshots")
        .join(format!("{name}.png"));
    let output_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("screenshots");

    let mismatch = match image::open(&reference_path) {
        Ok(reference) => compare(&actual, &reference.into_rgba8()),
        Err(err) => Some(Mismatch::Missing(err.to_string())),
    };
    let Some(mismatch) = mismatch else {
        return;
    };

    if env::var_os("UPDATE_SCREENSHOTS").is_some() {
        save(&actual, &reference_path);
        return;
    }

    let actual_path = output_dir.join(format!("{name}.actual.png"));
    save(&actual, &actual_path);
    let details = match mismatch {
        Mismatch::Missing(err) => format!("could not read the reference: {err}"),
        Mismatch::Size(reference) => format!(
            "size {}x{} does not match the reference's {}x{}",
            actual.width(),
            actual.height(),
            reference.0,
            reference.1
        ),
        Mismatch::Pixels {
            count,
            largest_difference,
            diff,
        } => {
            let diff_path = output_dir.join(format!("{name}.diff.png"));
            save(&diff, &diff_path);
            format!(
                "{count} pixels differ by more than {CHANNEL_TOLERANCE} \
                 (largest channel difference: {largest_difference})\n\
                 diff:      {}",
                diff_path.display()
            )
        }
    };
    panic!(
        "screenshot `{name}` does not match its reference: {details}\n\
         reference: {}\n\
         actual:    {}\n\
         adapter:   {}\n\
         If the change is intended, rerun with UPDATE_SCREENSHOTS=1 and review the new reference.",
        reference_path.display(),
        actual_path.display(),
        headless.adapter_name(),
    );
}

enum Mismatch {
    Missing(String),
    Size((u32, u32)),
    Pixels {
        count: usize,
        largest_difference: u8,
        /// The frame dimmed, with mismatched pixels in magenta.
        diff: RgbaImage,
    },
}

fn compare(actual: &RgbaImage, reference: &RgbaImage) -> Option<Mismatch> {
    if actual.dimensions() != reference.dimensions() {
        return Some(Mismatch::Size(reference.dimensions()));
    }
    let mut diff = RgbaImage::new(actual.width(), actual.height());
    let mut count = 0;
    let mut largest_difference = 0;
    for ((x, y, a), r) in actual.enumerate_pixels().zip(reference.pixels()) {
        let difference =
            a.0.iter()
                .zip(r.0)
                .map(|(a, r)| a.abs_diff(r))
                .max()
                .unwrap();
        largest_difference = largest_difference.max(difference);
        let pixel = if difference > CHANNEL_TOLERANCE {
            count += 1;
            Rgba([255, 0, 255, 255])
        } else {
            let [r, g, b, _] = a.0;
            Rgba([r / 3, g / 3, b / 3, 255])
        };
        diff.put_pixel(x, y, pixel);
    }
    (count > 0).then_some(Mismatch::Pixels {
        count,
        largest_difference,
        diff,
    })
}

fn save(image: &RgbaImage, path: &Path) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    image
        .save(path)
        .unwrap_or_else(|err| panic!("failed to write {}: {err}", path.display()));
}
