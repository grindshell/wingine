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

use std::{env, fs, path::Path};

use image::{Rgba, RgbaImage};
use wingine::{
    graphics::{Camera, Color, Resolution, SpriteSheet},
    headless::{Headless, Image},
    math::Rect,
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
