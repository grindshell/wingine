//! Draws a small top-down scene from a 16x16 tileset, with a hero walking back and forth.
//!
//! Run with `cargo run --example sprites`. The tileset is Kenney's 1-bit Pack
//! (<https://kenney.nl/assets/1-bit-pack>, CC0). Its sprites are white on transparent, so they are
//! tinted when drawn.

use wingine::{
    Context, Game, WindowConfig,
    graphics::{Canvas, Color, Sprite, SpriteSheet},
    math::Rect,
};

const TILESET: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/kenney_1-bit-pack_transparent-packed.png"
);
const TILE_SIZE: u32 = 16;
/// Tiles are drawn at a whole-number scale so pixels stay square.
const SCALE: f32 = 3.0;
const CELL: f32 = TILE_SIZE as f32 * SCALE;

const BACKGROUND: Color = Color::rgb8(0x1b, 0x26, 0x1e);
const GRASS: Color = Color::rgb8(0x3e, 0x6b, 0x3f);
const TREE: Color = Color::rgb8(0x6a, 0xa8, 0x4f);
const HOUSE: Color = Color::rgb8(0xc8, 0x8b, 0x5a);
const HERO: Color = Color::rgb8(0xf2, 0xd3, 0xab);
const BAT: Color = Color::rgb8(0xb0, 0x7c, 0xd6);
const HEART: Color = Color::rgb8(0xe0, 0x4a, 0x4a);
const HUD_PANEL: Color = Color::rgba(0.0, 0.0, 0.0, 0.6);

struct Scene {
    grass: [Sprite; 3],
    trees: [Sprite; 2],
    house: Sprite,
    hero_right: Sprite,
    hero_left: Sprite,
    bat: Sprite,
    heart_full: Sprite,
    heart_empty: Sprite,
    time: f32,
}

impl Scene {
    fn new(ctx: &mut Context) -> wingine::Result<Self> {
        let sheet = SpriteSheet::new(ctx.load_texture(TILESET)?, TILE_SIZE, TILE_SIZE);
        let hero = sheet.sprite(27, 0);
        Ok(Self {
            grass: [sheet.sprite(1, 0), sheet.sprite(5, 0), sheet.sprite(7, 0)],
            trees: [sheet.sprite(0, 1), sheet.sprite(4, 1)],
            house: sheet.sprite(19, 10),
            hero_left: hero.clone().flipped_x(),
            hero_right: hero,
            bat: sheet.sprite(26, 8),
            heart_full: sheet.sprite(39, 10),
            heart_empty: sheet.sprite(40, 10),
            time: 0.0,
        })
    }
}

impl Game for Scene {
    fn update(&mut self, dt: f32) {
        self.time += dt;
    }

    fn draw(&mut self, canvas: &mut Canvas) {
        canvas.clear(BACKGROUND);

        let columns = (canvas.width() / CELL).ceil() as u32;
        let rows = (canvas.height() / CELL).ceil() as u32;
        for row in 0..rows {
            for column in 0..columns {
                let h = hash(column, row);
                let on_border = column == 0 || row == 0 || column + 1 == columns || row + 1 == rows;
                if on_border {
                    let tree = &self.trees[h as usize % self.trees.len()];
                    canvas.draw_sprite(tree, cell(column as f32, row as f32), TREE);
                } else if h.is_multiple_of(3) {
                    let grass = &self.grass[(h >> 8) as usize % self.grass.len()];
                    canvas.draw_sprite(grass, cell(column as f32, row as f32), GRASS);
                }
            }
        }

        canvas.draw_sprite(&self.house, cell(3.0, 2.0), HOUSE);

        // Walk back and forth along the middle row, facing the direction of travel.
        let middle = (rows / 2) as f32;
        let walk = self.time * 0.6;
        let span = columns.saturating_sub(4) as f32 / 2.0;
        let x = columns as f32 / 2.0 - 0.5 + walk.sin() * span;
        let hero = if walk.cos() >= 0.0 {
            &self.hero_right
        } else {
            &self.hero_left
        };
        canvas.draw_sprite(hero, cell(x, middle), HERO);

        let bat_y = 2.0 + (self.time * 3.0).sin() * 0.25;
        canvas.draw_sprite(&self.bat, cell(columns as f32 - 4.0, bat_y), BAT);

        // HUD: a translucent solid panel with sprites on top, which switches textures mid-frame.
        canvas.fill_rect(
            Rect::new(8.0, 8.0, CELL * 3.0 + 16.0, CELL + 16.0),
            HUD_PANEL,
        );
        for i in 0..3 {
            let heart = if i < 2 {
                &self.heart_full
            } else {
                &self.heart_empty
            };
            let dest = Rect::new(16.0 + i as f32 * CELL, 16.0, CELL, CELL);
            canvas.draw_sprite(heart, dest, HEART);
        }
    }
}

/// The screen rectangle of the grid cell at `column`, `row`, snapped to whole screen pixels.
fn cell(column: f32, row: f32) -> Rect {
    Rect::new((column * CELL).round(), (row * CELL).round(), CELL, CELL)
}

/// Cheap deterministic noise, so the scattered tiles don't change between frames.
fn hash(x: u32, y: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9e37_79b1) ^ y.wrapping_mul(0x85eb_ca77);
    h ^= h >> 15;
    h.wrapping_mul(0x2c1b_3c6d)
}

fn main() -> wingine::Result<()> {
    env_logger::init();
    wingine::run(
        WindowConfig {
            title: "Wingine - sprites".into(),
            ..Default::default()
        },
        Scene::new,
    )
}
