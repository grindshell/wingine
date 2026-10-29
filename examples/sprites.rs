//! A small top-down map from a 16x16 tileset, drawn at a fixed 256x144 resolution and scaled up to
//! the window. Walk the hero around with the arrow keys or WASD; holding a direction keeps
//! walking, and the camera follows. Trees and the house block the way.
//!
//! Run with `cargo run --example sprites`. The tileset is Kenney's 1-bit Pack
//! (<https://kenney.nl/assets/1-bit-pack>, CC0). Its sprites are white on transparent, so they are
//! tinted when drawn.

use wingine::{
    Context, Game, WindowConfig,
    graphics::{Camera, Canvas, Color, Resolution, Sprite, SpriteSheet},
    input::Key,
    math::Rect,
};

const TILESET: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/kenney_1-bit-pack_transparent-packed.png"
);
const TILE: u32 = 16;
const TILE_SIZE: f32 = TILE as f32;
/// 16 by 9 tiles, which scales by whole numbers to fill 720p, 1080p and 1440p screens.
const RESOLUTION: Resolution = Resolution::Fixed {
    width: 256,
    height: 144,
};

const MAP_COLUMNS: i32 = 40;
const MAP_ROWS: i32 = 24;
const HOUSE_TILE: [i32; 2] = [17, 9];
const START_TILE: [i32; 2] = [MAP_COLUMNS / 2, MAP_ROWS / 2];
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

        let sheet = SpriteSheet::new(ctx.load_texture(TILESET)?, TILE, TILE);
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
                tile: START_TILE,
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

        // Follow the hero, snapped to whole pixels so it doesn't jitter against the map.
        let [hero_x, hero_y] = self.hero.position().map(|v| (v * TILE_SIZE).round());
        let camera = Camera {
            bounds: Some(Rect::new(
                0.0,
                0.0,
                MAP_COLUMNS as f32 * TILE_SIZE,
                MAP_ROWS as f32 * TILE_SIZE,
            )),
            ..Camera::new(hero_x + TILE_SIZE / 2.0, hero_y + TILE_SIZE / 2.0)
        };
        canvas.set_camera(Some(camera));

        // Only draw the tiles the camera can see.
        let visible = camera.visible_area(canvas.width(), canvas.height());
        let columns = tile_range(visible.x, visible.right(), MAP_COLUMNS);
        let rows = tile_range(visible.y, visible.bottom(), MAP_ROWS);
        for row in rows {
            for column in columns.clone() {
                let tile = [column, row];
                let h = hash(tile);
                if tile == HOUSE_TILE {
                    canvas.draw_sprite(&self.house, tile_rect(tile), HOUSE);
                } else if is_tree(tile) {
                    let tree = &self.trees[h as usize % self.trees.len()];
                    canvas.draw_sprite(tree, tile_rect(tile), TREE);
                } else if h.is_multiple_of(3) {
                    let grass = &self.grass[(h >> 8) as usize % self.grass.len()];
                    canvas.draw_sprite(grass, tile_rect(tile), GRASS);
                }
            }
        }

        let hero = if self.hero.facing_left {
            &self.hero_left
        } else {
            &self.hero_right
        };
        canvas.draw_sprite(hero, Rect::new(hero_x, hero_y, TILE_SIZE, TILE_SIZE), HERO);

        let bat_y = (6.0 + (self.time * 3.0).sin() * 0.25) * TILE_SIZE;
        let bat = Rect::new(24.0 * TILE_SIZE, bat_y.round(), TILE_SIZE, TILE_SIZE);
        canvas.draw_sprite(&self.bat, bat, BAT);

        // The HUD is drawn in canvas pixels, so it stays put as the camera moves.
        canvas.set_camera(None);
        canvas.fill_rect(
            Rect::new(2.0, 2.0, TILE_SIZE * 3.0 + 4.0, TILE_SIZE + 4.0),
            HUD_PANEL,
        );
        for i in 0..3 {
            let heart = if i < 2 {
                &self.heart_full
            } else {
                &self.heart_empty
            };
            let dest = Rect::new(4.0 + i as f32 * TILE_SIZE, 4.0, TILE_SIZE, TILE_SIZE);
            canvas.draw_sprite(heart, dest, HEART);
        }
    }
}

/// The tiles along one axis that overlap world coordinates `start..end`.
fn tile_range(start: f32, end: f32, tiles: i32) -> std::ops::Range<i32> {
    let first = (start / TILE_SIZE).floor() as i32;
    let last = (end / TILE_SIZE).ceil() as i32;
    first.max(0)..last.min(tiles)
}

fn tile_rect([column, row]: [i32; 2]) -> Rect {
    Rect::new(
        column as f32 * TILE_SIZE,
        row as f32 * TILE_SIZE,
        TILE_SIZE,
        TILE_SIZE,
    )
}

/// Trees ring the map and are scattered inside it.
fn is_tree(tile: [i32; 2]) -> bool {
    let [column, row] = tile;
    let border = column == 0 || row == 0 || column == MAP_COLUMNS - 1 || row == MAP_ROWS - 1;
    border || (tile != START_TILE && tile != HOUSE_TILE && (hash(tile) >> 4).is_multiple_of(13))
}

/// Whether the hero can stand on `tile`: inside the map, and not a tree or the house.
fn walkable(tile: [i32; 2]) -> bool {
    let [column, row] = tile;
    let inside = (0..MAP_COLUMNS).contains(&column) && (0..MAP_ROWS).contains(&row);
    inside && !is_tree(tile) && tile != HOUSE_TILE
}

/// Cheap deterministic noise, so the scattered tiles don't change between frames.
fn hash([x, y]: [i32; 2]) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9e37_79b1) ^ (y as u32).wrapping_mul(0x85eb_ca77);
    h ^= h >> 15;
    h.wrapping_mul(0x2c1b_3c6d)
}

fn main() -> wingine::Result<()> {
    env_logger::init();
    wingine::run(
        WindowConfig {
            title: "Wingine - sprites".into(),
            resolution: RESOLUTION,
            ..Default::default()
        },
        Scene::new,
    )
}
