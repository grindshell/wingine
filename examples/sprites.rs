//! A small top-down scene from a 16x16 tileset. Walk the hero around with the arrow keys or WASD;
//! holding a direction keeps walking. Trees and the house block the way.
//!
//! Run with `cargo run --example sprites`. The tileset is Kenney's 1-bit Pack
//! (<https://kenney.nl/assets/1-bit-pack>, CC0). Its sprites are white on transparent, so they are
//! tinted when drawn.

use wingine::{
    Context, Game, WindowConfig,
    graphics::{Canvas, Color, Sprite, SpriteSheet},
    input::Key,
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

const MAP_COLUMNS: i32 = 20;
const MAP_ROWS: i32 = 12;
const HOUSE_TILE: [i32; 2] = [3, 2];
/// Seconds the hero takes to walk one tile.
const STEP_SECONDS: f32 = 0.16;
/// Movement actions and the direction each walks in, checked in this order.
const DIRECTIONS: [(&str, [i32; 2]); 4] = [
    ("up", [0, -1]),
    ("down", [0, 1]),
    ("left", [-1, 0]),
    ("right", [1, 0]),
];

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
    hero: Hero,
    time: f32,
}

struct Hero {
    tile: [i32; 2],
    /// The walk to `tile` currently under way, if any.
    step: Option<Step>,
    facing_left: bool,
}

struct Step {
    from: [i32; 2],
    /// From 0 at `from` to 1 at the destination.
    progress: f32,
}

impl Hero {
    /// Position in tiles, between two tiles while walking.
    fn position(&self) -> [f32; 2] {
        let to = self.tile.map(|v| v as f32);
        match &self.step {
            Some(Step { from, progress }) => {
                let from = from.map(|v| v as f32);
                [0, 1].map(|i| from[i] + (to[i] - from[i]) * progress)
            }
            None => to,
        }
    }
}

impl Scene {
    fn new(ctx: &mut Context) -> wingine::Result<Self> {
        let input = ctx.input_mut();
        input.bind("up", [Key::Up, Key::W]);
        input.bind("down", [Key::Down, Key::S]);
        input.bind("left", [Key::Left, Key::A]);
        input.bind("right", [Key::Right, Key::D]);

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
            hero: Hero {
                tile: [MAP_COLUMNS / 2, MAP_ROWS / 2],
                step: None,
                facing_left: false,
            },
            time: 0.0,
        })
    }
}

impl Game for Scene {
    fn update(&mut self, ctx: &mut Context, dt: f32) {
        self.time += dt;

        let hero = &mut self.hero;
        if let Some(step) = &mut hero.step {
            step.progress += dt / STEP_SECONDS;
            if step.progress < 1.0 {
                return;
            }
            hero.step = None;
        }

        // Start the next step, so holding a direction keeps walking tile by tile. A tap shorter
        // than a frame is pressed but never held, so presses count too.
        let input = ctx.input();
        let Some(&(_, [dx, dy])) = DIRECTIONS
            .iter()
            .find(|(action, _)| input.held(action) || input.pressed(action))
        else {
            return;
        };
        if dx != 0 {
            hero.facing_left = dx < 0;
        }
        let target = [hero.tile[0] + dx, hero.tile[1] + dy];
        if walkable(target) {
            hero.step = Some(Step {
                from: hero.tile,
                progress: 0.0,
            });
            hero.tile = target;
        }
    }

    fn draw(&mut self, canvas: &mut Canvas) {
        canvas.clear(BACKGROUND);

        for row in 0..MAP_ROWS {
            for column in 0..MAP_COLUMNS {
                let h = hash(column as u32, row as u32);
                let dest = cell(column as f32, row as f32);
                if is_border([column, row]) {
                    let tree = &self.trees[h as usize % self.trees.len()];
                    canvas.draw_sprite(tree, dest, TREE);
                } else if h.is_multiple_of(3) {
                    let grass = &self.grass[(h >> 8) as usize % self.grass.len()];
                    canvas.draw_sprite(grass, dest, GRASS);
                }
            }
        }

        let [house_x, house_y] = HOUSE_TILE.map(|v| v as f32);
        canvas.draw_sprite(&self.house, cell(house_x, house_y), HOUSE);

        let hero = if self.hero.facing_left {
            &self.hero_left
        } else {
            &self.hero_right
        };
        let [x, y] = self.hero.position();
        canvas.draw_sprite(hero, cell(x, y), HERO);

        let bat_y = 2.0 + (self.time * 3.0).sin() * 0.25;
        canvas.draw_sprite(&self.bat, cell(MAP_COLUMNS as f32 - 4.0, bat_y), BAT);

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

fn is_border([column, row]: [i32; 2]) -> bool {
    column == 0 || row == 0 || column == MAP_COLUMNS - 1 || row == MAP_ROWS - 1
}

/// Whether the hero can stand on `tile`: inside the map, and not a tree or the house.
fn walkable(tile: [i32; 2]) -> bool {
    let [column, row] = tile;
    let inside = (0..MAP_COLUMNS).contains(&column) && (0..MAP_ROWS).contains(&row);
    inside && !is_border(tile) && tile != HOUSE_TILE
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
            width: (MAP_COLUMNS as f32 * CELL) as u32,
            height: (MAP_ROWS as f32 * CELL) as u32,
            resizable: false,
        },
        Scene::new,
    )
}
