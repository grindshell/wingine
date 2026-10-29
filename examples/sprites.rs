//! A small top-down map built from a 16x16 tileset, drawn at a fixed 256x144 resolution and scaled
//! up to the window. Walk the hero around with the arrow keys or WASD (holding a direction keeps
//! walking), or click a tile to walk toward it. The camera follows, and trees and the house block
//! the way.
//!
//! Run with `cargo run --example sprites`. The tileset is Kenney's 1-bit Pack
//! (<https://kenney.nl/assets/1-bit-pack>, CC0). Its sprites are white on transparent, so they are
//! tinted when drawn.

use wingine::{
    Context, Game, WindowConfig,
    graphics::{Camera, Canvas, Color, Resolution, Sprite, SpriteSheet},
    grid::{Direction, GridMover},
    input::{Key, MouseButton},
    math::Rect,
    tilemap::{Tile, Tilemap, Tileset},
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

const MAP_COLUMNS: u32 = 40;
const MAP_ROWS: u32 = 24;
const HOUSE_TILE: [i32; 2] = [17, 9];
const START_TILE: [i32; 2] = [MAP_COLUMNS as i32 / 2, MAP_ROWS as i32 / 2];
/// Seconds the hero takes to walk one tile.
const STEP_SECONDS: f32 = 0.16;
/// Movement actions and the direction each walks in, checked in this order.
const DIRECTIONS: [(&str, Direction); 4] = [
    ("up", Direction::Up),
    ("down", Direction::Down),
    ("left", Direction::Left),
    ("right", Direction::Right),
];

const BACKGROUND: Color = Color::rgb8(0x1b, 0x26, 0x1e);
const GRASS: Color = Color::rgb8(0x3e, 0x6b, 0x3f);
const TREE: Color = Color::rgb8(0x6a, 0xa8, 0x4f);
const HOUSE: Color = Color::rgb8(0xc8, 0x8b, 0x5a);
const HERO: Color = Color::rgb8(0xf2, 0xd3, 0xab);
const BAT: Color = Color::rgb8(0xb0, 0x7c, 0xd6);
const HEART: Color = Color::rgb8(0xe0, 0x4a, 0x4a);
const HUD_PANEL: Color = Color::rgba(0.0, 0.0, 0.0, 0.6);
/// Blending is in linear space, where a little white brightens dark colors a lot.
const HOVER: Color = Color::rgba(1.0, 1.0, 1.0, 0.06);

struct Scene {
    map: Tilemap,
    hero_right: Sprite,
    hero_left: Sprite,
    bat: Sprite,
    heart_full: Sprite,
    heart_empty: Sprite,
    hero: Hero,
    /// The map tile under the cursor.
    hovered: Option<[i32; 2]>,
    time: f32,
}

struct Hero {
    mover: GridMover,
    /// A clicked tile the hero is walking toward.
    target: Option<[i32; 2]>,
    /// A direction tapped during a step, to walk once the step ends.
    queued: Option<Direction>,
    facing_left: bool,
}

impl Hero {
    /// The next direction to walk toward `target`: along the longer axis if that way is clear,
    /// otherwise the shorter. Without pathfinding, the hero gives up when both are blocked.
    fn direction_to_target(&mut self, map: &Tilemap) -> Option<Direction> {
        let target = self.target?;
        let tile = self.mover.tile();
        let [dx, dy] = [target[0] - tile[0], target[1] - tile[1]];
        let horizontal = match dx.signum() {
            1 => Some(Direction::Right),
            -1 => Some(Direction::Left),
            _ => None,
        };
        let vertical = match dy.signum() {
            1 => Some(Direction::Down),
            -1 => Some(Direction::Up),
            _ => None,
        };
        let candidates = if dx.abs() >= dy.abs() {
            [horizontal, vertical]
        } else {
            [vertical, horizontal]
        };
        let direction = candidates
            .into_iter()
            .flatten()
            .find(|direction| !map.is_solid(direction.neighbor(tile)));
        if direction.is_none() {
            self.target = None;
        }
        direction
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
            map: build_map(&sheet),
            hero_left: hero.clone().flipped_x(),
            hero_right: hero,
            bat: sheet.sprite(26, 8),
            heart_full: sheet.sprite(39, 10),
            heart_empty: sheet.sprite(40, 10),
            hero: Hero {
                mover: GridMover::new(START_TILE, STEP_SECONDS),
                target: None,
                queued: None,
                facing_left: false,
            },
            hovered: None,
            time: 0.0,
        })
    }

    /// The hero's position in world pixels, snapped to whole pixels so it doesn't jitter against
    /// the map.
    fn hero_position(&self) -> [f32; 2] {
        self.map
            .tile_to_world(self.hero.mover.position())
            .map(f32::round)
    }

    /// Follows the hero, inside the map.
    fn camera(&self) -> Camera {
        let [x, y] = self.hero_position();
        Camera {
            bounds: Some(self.map.bounds()),
            ..Camera::new(x + TILE_SIZE / 2.0, y + TILE_SIZE / 2.0)
        }
    }
}

impl Game for Scene {
    fn update(&mut self, ctx: &mut Context, dt: f32) {
        self.time += dt;

        let input = ctx.input();
        self.hovered = input.mouse_position().and_then(|mouse| {
            let [width, height] = ctx.canvas_size();
            let world = self.camera().canvas_to_world(mouse, width, height);
            let tile = self.map.world_to_tile(world);
            self.map.contains(tile).then_some(tile)
        });
        if input.mouse_pressed(MouseButton::Left) {
            self.hero.target = self.hovered;
        }

        let direction_where = |test: &dyn Fn(&str) -> bool| {
            DIRECTIONS
                .iter()
                .find(|(action, _)| test(action))
                .map(|&(_, direction)| direction)
        };
        let held = direction_where(&|action| input.held(action));
        // A tap shorter than a frame is pressed but never held, so presses count too.
        let pressed = direction_where(&|action| input.pressed(action));

        let (hero, map) = (&mut self.hero, &self.map);
        // The keyboard takes over from a clicked target.
        if held.is_some() || pressed.is_some() {
            hero.target = None;
        }
        hero.mover.update(dt);
        if hero.mover.is_moving() {
            // Presses last one frame, so remember a tap made mid-step for when the step ends.
            if pressed.is_some() {
                hero.queued = pressed;
            }
            return;
        }

        // Start the next step, so holding a direction keeps walking tile by tile.
        let queued = hero.queued.take();
        let Some(direction) = held
            .or(pressed)
            .or(queued)
            .or_else(|| hero.direction_to_target(map))
        else {
            return;
        };
        match direction {
            Direction::Left => hero.facing_left = true,
            Direction::Right => hero.facing_left = false,
            Direction::Up | Direction::Down => {}
        }
        hero.mover.step(direction, |tile| map.is_solid(tile));
        if hero.target == Some(hero.mover.tile()) {
            hero.target = None;
        }
    }

    fn draw(&mut self, canvas: &mut Canvas) {
        canvas.clear(BACKGROUND);
        canvas.set_camera(Some(self.camera()));
        // Only the tiles the camera can see are drawn.
        self.map.draw(canvas);

        if let Some(tile) = self.hovered {
            canvas.fill_rect(self.map.tile_rect(tile), HOVER);
        }

        let hero = if self.hero.facing_left {
            &self.hero_left
        } else {
            &self.hero_right
        };
        let [hero_x, hero_y] = self.hero_position();
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

/// Scattered grass on the ground layer, and trees and the house, which block the way, on the
/// layer above.
fn build_map(sheet: &SpriteSheet) -> Tilemap {
    let mut tileset = Tileset::new(TILE, TILE);
    let grass = [sheet.sprite(1, 0), sheet.sprite(5, 0), sheet.sprite(7, 0)].map(|sprite| {
        tileset.add(Tile {
            tint: GRASS,
            ..Tile::new(sprite)
        })
    });
    let trees = [sheet.sprite(0, 1), sheet.sprite(4, 1)].map(|sprite| {
        tileset.add(Tile {
            tint: TREE,
            solid: true,
            ..Tile::new(sprite)
        })
    });
    let house = tileset.add(Tile {
        tint: HOUSE,
        solid: true,
        ..Tile::new(sheet.sprite(19, 10))
    });

    let mut map = Tilemap::new(tileset, MAP_COLUMNS, MAP_ROWS);
    let ground = map.add_layer();
    let objects = map.add_layer();
    for row in 0..MAP_ROWS as i32 {
        for column in 0..MAP_COLUMNS as i32 {
            let tile = [column, row];
            let h = hash(tile);
            if tile == HOUSE_TILE {
                map.set(objects, tile, Some(house));
            } else if is_tree(tile) {
                map.set(objects, tile, Some(trees[h as usize % trees.len()]));
            } else if h.is_multiple_of(3) {
                map.set(ground, tile, Some(grass[(h >> 8) as usize % grass.len()]));
            }
        }
    }
    map
}

/// Trees ring the map and are scattered inside it.
fn is_tree(tile: [i32; 2]) -> bool {
    let [column, row] = tile;
    let [last_column, last_row] = [MAP_COLUMNS as i32 - 1, MAP_ROWS as i32 - 1];
    let border = column == 0 || row == 0 || column == last_column || row == last_row;
    border || (tile != START_TILE && tile != HOUSE_TILE && (hash(tile) >> 4).is_multiple_of(13))
}

/// Cheap deterministic noise, so the map is the same every run.
fn hash([x, y]: [i32; 2]) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9e37_79b1) ^ (y as u32).wrapping_mul(0x85eb_ca77);
    h ^= h >> 15;
    h.wrapping_mul(0x2c1b_3c6d)
}

fn config() -> WindowConfig {
    WindowConfig {
        title: "Wingine - sprites".into(),
        resolution: RESOLUTION,
        ..Default::default()
    }
}

fn main() -> wingine::Result<()> {
    env_logger::init();
    wingine::run(config(), Scene::new)
}

/// Drives the scene with simulated input, without a window. Expected positions come from the
/// map's own solid tiles, so the tests don't depend on where trees happen to be.
#[cfg(test)]
mod tests {
    use wingine::headless::Simulation;

    use super::*;

    fn simulation() -> Simulation<Scene> {
        Simulation::new(&config(), Scene::new).expect("scene starts")
    }

    /// Where walking from `from` in `direction` ends after at most `steps` tiles.
    fn walk(map: &Tilemap, from: [i32; 2], direction: Direction, steps: usize) -> [i32; 2] {
        let mut tile = from;
        for _ in 0..steps {
            let next = direction.neighbor(tile);
            if map.is_solid(next) {
                break;
            }
            tile = next;
        }
        tile
    }

    /// The canvas position of the middle of `tile`, for clicking on it.
    fn canvas_point(scene: &Scene, tile: [i32; 2]) -> [f32; 2] {
        let area = scene.camera().visible_area(256.0, 144.0);
        let rect = scene.map.tile_rect(tile);
        [
            rect.x + TILE_SIZE / 2.0 - area.x,
            rect.y + TILE_SIZE / 2.0 - area.y,
        ]
    }

    #[test]
    fn trees_and_the_house_block_the_way() {
        let simulation = simulation();
        let map = &simulation.game().map;
        assert!(map.is_solid(HOUSE_TILE));
        assert!(map.is_solid([0, 5]), "the border");
        assert!(!map.is_solid(START_TILE));
    }

    #[test]
    fn a_tap_walks_one_tile() {
        let mut simulation = simulation();
        simulation.tap(Key::Left);
        simulation.run_for(0.5);
        let scene = simulation.game();
        let expected = walk(&scene.map, START_TILE, Direction::Left, 1);
        assert_eq!(scene.hero.mover.tile(), expected);
        assert!(!scene.hero.mover.is_moving(), "the step finished");
    }

    #[test]
    fn holding_a_direction_keeps_walking_until_blocked() {
        let mut simulation = simulation();
        // Long enough to start 12 steps, but not a 13th.
        simulation.hold(Key::D, 11.5 * STEP_SECONDS);
        simulation.run_for(0.5);
        let scene = simulation.game();
        let expected = walk(&scene.map, START_TILE, Direction::Right, 12);
        assert_eq!(scene.hero.mover.tile(), expected);
        assert!(!scene.hero.facing_left);
    }

    /// Walking on from tile to tile keeps to the step time, though steps end between frames.
    #[test]
    fn walking_keeps_an_even_pace() {
        let mut simulation = simulation();
        // A way with room for 7 steps, so nothing cuts the walk short.
        let keys = [Key::W, Key::S, Key::A, Key::D];
        let (direction, key) = Direction::ALL
            .into_iter()
            .zip(keys)
            .find(|&(direction, _)| {
                let [dx, dy] = direction.offset();
                let end = [START_TILE[0] + 7 * dx, START_TILE[1] + 7 * dy];
                walk(&simulation.game().map, START_TILE, direction, 7) == end
            })
            .expect("a clear way from the start");

        simulation.key_down(key);
        simulation.run_frames(58);
        // The first frame starts walking, and each of the other 57 moves the hero on.
        let walked = 57.0 * Simulation::<Scene>::FRAME_TIME / STEP_SECONDS;
        let offset = direction.offset();
        let expected = [0, 1].map(|i| START_TILE[i] as f32 + offset[i] as f32 * walked);
        let position = simulation.game().hero.mover.position();
        assert!(
            (0..2).all(|i| (position[i] - expected[i]).abs() < 1e-3),
            "at {position:?}, expected {expected:?}"
        );
    }

    #[test]
    fn walking_left_faces_left() {
        let mut simulation = simulation();
        simulation.tap(Key::A);
        assert!(simulation.game().hero.facing_left);
    }

    #[test]
    fn hovering_finds_the_tile_under_the_cursor() {
        let mut simulation = simulation();
        let tile = [START_TILE[0] + 3, START_TILE[1] - 2];
        let [x, y] = canvas_point(simulation.game(), tile);
        simulation.move_mouse(x, y);
        simulation.step();
        assert_eq!(simulation.game().hovered, Some(tile));
    }

    #[test]
    fn clicking_a_tile_walks_toward_it() {
        let mut simulation = simulation();
        // A tile in a straight, clear line from the start, so walking there needs no pathfinding.
        let target = walk(&simulation.game().map, START_TILE, Direction::Down, 3);
        assert_ne!(target, START_TILE, "the test needs room to walk");
        let [x, y] = canvas_point(simulation.game(), target);
        simulation.click(MouseButton::Left, x, y);
        simulation.run_for(1.0);
        assert_eq!(simulation.game().hero.mover.tile(), target);
        assert_eq!(simulation.game().hero.target, None, "arrived");
    }

    #[test]
    fn the_keyboard_overrides_a_click() {
        let mut simulation = simulation();
        let target = walk(&simulation.game().map, START_TILE, Direction::Down, 3);
        let [x, y] = canvas_point(simulation.game(), target);
        simulation.click(MouseButton::Left, x, y);
        simulation.tap(Key::Up);
        simulation.run_for(1.0);
        assert_eq!(simulation.game().hero.target, None);
        assert_ne!(simulation.game().hero.mover.tile(), target);
    }
}
