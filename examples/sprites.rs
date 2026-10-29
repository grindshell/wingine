//! A small top-down map built from a 16x16 tileset, drawn at a fixed 256x144 resolution and scaled
//! up to the window. Walk the hero around with the arrow keys or WASD (holding a direction keeps
//! walking), or click a tile to walk toward it. The camera follows, and trees and the house block
//! the way. Face the house and press Space, Enter or E to read the note on its door.
//!
//! The note and the bat circling the house are scripted in Luau, in `examples/sprites.luau`.
//! Music loops in the background, text blips as it appears, and walking into things thuds.
//!
//! Run with `cargo run --example sprites`. The tileset is Kenney's 1-bit Pack
//! (<https://kenney.nl/assets/1-bit-pack>, CC0). Its sprites are white on transparent, so they are
//! tinted when drawn. The music and sounds in `examples/sounds` were synthesized for this example.

use std::{cell::RefCell, collections::HashMap, rc::Rc};

use wingine::{
    Context, Game, WindowConfig,
    audio::{Audio, Sound},
    graphics::{Camera, Canvas, Color, Font, Resolution, Sprite, SpriteSheet, TextLayout},
    grid::{Direction, GridMover},
    input::{Key, MouseButton},
    lua::{Scripts, TaskId, mlua},
    math::Rect,
    tilemap::{Tile, Tilemap, Tileset},
};

const TILESET: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/kenney_1-bit-pack_transparent-packed.png"
);
const SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/sprites.luau");
const SOUNDS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/sounds/");
const MUSIC_VOLUME: f32 = 0.5;
/// Text blips every this many glyphs as it's revealed.
const GLYPHS_PER_BLIP: usize = 3;
/// Seconds between thuds while walking into something.
const BUMP_SECONDS: f32 = 0.3;
/// Lets scripts make the scene's functions wait, in Luau.
const SCRIPT_API: &str = "
function say(text: string)
    show_text(text)
    wait_until(function() return not text_open() end)
end
";
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

/// Where the bat starts circling the house, from its top-left corner.
const BAT_START: [i32; 2] = [HOUSE_TILE[0] - 1, HOUSE_TILE[1] - 1];
const BAT_STEP_SECONDS: f32 = 0.3;
/// Glyphs of dialogue revealed per second.
const REVEAL_RATE: f32 = 45.0;
/// The dialogue box spans the canvas, 4 pixels in from each side.
const DIALOGUE_WIDTH: f32 = 248.0;
const DIALOGUE_PADDING: f32 = 6.0;

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
const DIALOGUE_BORDER: Color = Color::rgb8(0xf2, 0xd3, 0xab);
const DIALOGUE_PANEL: Color = Color::rgb8(0x14, 0x18, 0x2c);
const TEXT: Color = Color::rgb8(0xf4, 0xf0, 0xe8);

struct Scene {
    map: Tilemap,
    font: Font,
    hero_right: Sprite,
    hero_left: Sprite,
    bat: Sprite,
    heart_full: Sprite,
    heart_empty: Sprite,
    hero: Hero,
    /// The map tile under the cursor.
    hovered: Option<[i32; 2]>,
    shared: Rc<RefCell<Shared>>,
    scripts: Scripts,
    /// The script the hero set off, such as reading the note. The hero waits while it runs.
    event: Option<TaskId>,
    sounds: Sounds,
    /// Seconds until walking into something can thud again.
    bump_cooldown: f32,
    time: f32,
}

struct Sounds {
    blip: Sound,
    bump: Sound,
}

/// The parts of the scene that scripts change, through the functions `register_api` gives them.
struct Shared {
    /// Text being read, which stops the hero until it's closed.
    dialogue: Option<Dialogue>,
    bat: GridMover,
    /// Sounds scripts have asked for, to play on the scene's next update.
    sounds: Vec<Sound>,
}

struct Dialogue {
    layout: TextLayout,
    /// Glyphs shown so far, counting up as the text is revealed.
    shown: f32,
}

impl Dialogue {
    fn is_complete(&self) -> bool {
        self.shown >= self.layout.glyph_count() as f32
    }
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
        input.bind("interact", [Key::Space, Key::Enter, Key::E]);

        let sheet = SpriteSheet::new(ctx.load_texture(TILESET)?, TILE, TILE);
        let hero = sheet.sprite(27, 0);
        let font = ctx.default_font().clone();
        let sound = |name: &str| Sound::load(format!("{SOUNDS}{name}.wav"));
        let sounds = Sounds {
            blip: sound("blip")?,
            bump: sound("bump")?,
        };
        let script_sounds = HashMap::from([("chime".to_owned(), sound("chime")?)]);

        let shared = Rc::new(RefCell::new(Shared {
            dialogue: None,
            bat: GridMover::new(BAT_START, BAT_STEP_SECONDS),
            sounds: Vec::new(),
        }));
        let mut scripts = Scripts::new()?;
        register_api(&scripts, &shared, &font, script_sounds)?;
        scripts.load(SCRIPT)?;
        scripts.spawn("bat", ())?;

        let audio = ctx.audio();
        audio.set_music_volume(MUSIC_VOLUME);
        audio.play_music(format!("{SOUNDS}music.ogg"), 1.0)?;

        Ok(Self {
            map: build_map(&sheet),
            font,
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
            shared,
            scripts,
            event: None,
            sounds,
            bump_cooldown: 0.0,
            time: 0.0,
        })
    }

    /// Plays the sounds scripts have asked for since this was last called.
    fn play_script_sounds(&self, audio: &mut Audio) {
        for sound in self.shared.borrow_mut().sounds.drain(..) {
            audio.play(&sound);
        }
    }

    /// Whether the hero stands still, facing the house.
    fn facing_the_house(&self) -> bool {
        let mover = &self.hero.mover;
        !mover.is_moving() && mover.facing().neighbor(mover.tile()) == HOUSE_TILE
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
        self.bump_cooldown = (self.bump_cooldown - dt).max(0.0);
        // Scripts run first, so what they ask for this frame shows this frame.
        if let Err(err) = self.scripts.update(dt) {
            log::error!("{err}");
        }
        self.play_script_sounds(ctx.audio());
        self.shared.borrow_mut().bat.update(dt);

        let input = ctx.input();
        self.hovered = input.mouse_position().and_then(|mouse| {
            let [width, height] = ctx.canvas_size();
            let world = self.camera().canvas_to_world(mouse, width, height);
            let tile = self.map.world_to_tile(world);
            self.map.contains(tile).then_some(tile)
        });
        let interact = input.pressed("interact");
        let click = input.mouse_pressed(MouseButton::Left);
        let direction_where = |test: &dyn Fn(&str) -> bool| {
            DIRECTIONS
                .iter()
                .find(|(action, _)| test(action))
                .map(|&(_, direction)| direction)
        };
        let held = direction_where(&|action| input.held(action));
        // A tap shorter than a frame is pressed but never held, so presses count too.
        let pressed = direction_where(&|action| input.pressed(action));
        let audio = ctx.audio();

        // While reading, presses reveal the rest of the text, then close it.
        let mut shared = self.shared.borrow_mut();
        if let Some(dialogue) = &mut shared.dialogue {
            let total = dialogue.layout.glyph_count() as f32;
            if !(interact || click) {
                let blips = dialogue.shown as usize / GLYPHS_PER_BLIP;
                dialogue.shown = (dialogue.shown + dt * REVEAL_RATE).min(total);
                if dialogue.shown as usize / GLYPHS_PER_BLIP > blips {
                    audio.play(&self.sounds.blip);
                }
            } else if !dialogue.is_complete() {
                dialogue.shown = total;
            } else {
                shared.dialogue = None;
            }
            return;
        }
        // Scripts borrow the shared state while they run.
        drop(shared);

        // The hero waits for the script it set off, such as between pages of dialogue.
        if self
            .event
            .is_some_and(|event| self.scripts.is_running(event))
        {
            return;
        }
        if interact && self.facing_the_house() {
            self.hero.target = None;
            match self.scripts.spawn("read_note", ()) {
                Ok(event) => self.event = Some(event),
                Err(err) => log::error!("{err}"),
            }
            self.play_script_sounds(audio);
            return;
        }

        if click {
            self.hero.target = self.hovered;
        }

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
        let stepped = hero.mover.step(direction, |tile| map.is_solid(tile));
        // Walking into something thuds, but only now and then while the key is held.
        if !stepped && held.or(pressed).is_some() && self.bump_cooldown == 0.0 {
            audio.play(&self.sounds.bump);
            self.bump_cooldown = BUMP_SECONDS;
        }
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

        let shared = self.shared.borrow();
        // The bat bobs as it flies.
        let [bat_x, bat_y] = self.map.tile_to_world(shared.bat.position());
        let bat_y = bat_y + (self.time * 3.0).sin() * 4.0;
        let bat = Rect::new(bat_x.round(), bat_y.round(), TILE_SIZE, TILE_SIZE);
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

        if let Some(dialogue) = &shared.dialogue {
            let height = dialogue.layout.size()[1] + DIALOGUE_PADDING + 2.0;
            let x = (canvas.width() - DIALOGUE_WIDTH) / 2.0;
            let panel = Rect::new(x, canvas.height() - 4.0 - height, DIALOGUE_WIDTH, height);
            canvas.fill_rect(panel, DIALOGUE_BORDER);
            let inside = Rect::new(panel.x + 1.0, panel.y + 1.0, panel.w - 2.0, panel.h - 2.0);
            canvas.fill_rect(inside, DIALOGUE_PANEL);
            let text = [panel.x + DIALOGUE_PADDING, panel.y + DIALOGUE_PADDING - 1.0];
            let shown = dialogue.shown as usize;
            canvas.draw_text_layout(&dialogue.layout, ..shown, text, TEXT);
        } else if self.facing_the_house() {
            let prompt = "Space: read the note";
            let [width, height] = self.font.measure(prompt);
            let position = [
                ((canvas.width() - width) / 2.0).round(),
                canvas.height() - height - 4.0,
            ];
            let backing = Rect::new(
                position[0] - 3.0,
                position[1] - 2.0,
                width + 6.0,
                height + 2.0,
            );
            canvas.fill_rect(backing, HUD_PANEL);
            canvas.draw_text(&self.font, prompt, position, TEXT);
        }
    }
}

/// Gives scripts the scene's functions, which change it through `shared`. They're documented for
/// script authors at the top of `sprites.luau`. `sounds` are the sounds scripts can play, by
/// name.
fn register_api(
    scripts: &Scripts,
    shared: &Rc<RefCell<Shared>>,
    font: &Font,
    sounds: HashMap<String, Sound>,
) -> wingine::Result<()> {
    let (state, font) = (Rc::clone(shared), font.clone());
    scripts.set_function("show_text", move |_, text: String| {
        let width = DIALOGUE_WIDTH - DIALOGUE_PADDING * 2.0;
        state.borrow_mut().dialogue = Some(Dialogue {
            layout: font.layout(&text, Some(width)),
            shown: 0.0,
        });
        Ok(())
    })?;
    let state = Rc::clone(shared);
    scripts.set_function("text_open", move |_, ()| {
        Ok(state.borrow().dialogue.is_some())
    })?;
    let state = Rc::clone(shared);
    scripts.set_function("bat_fly", move |_, direction: Direction| {
        // Bats fly over trees and houses alike.
        state.borrow_mut().bat.step(direction, |_| false);
        Ok(())
    })?;
    let state = Rc::clone(shared);
    scripts.set_function("play_sound", move |_, name: String| {
        let sound = sounds
            .get(&name)
            .ok_or_else(|| mlua::Error::runtime(format!("there's no sound called `{name}`")))?;
        state.borrow_mut().sounds.push(sound.clone());
        Ok(())
    })?;
    scripts.load_source("sprites API", SCRIPT_API)
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

    /// Stands the hero still beside the house, facing it.
    fn face_the_house(simulation: &mut Simulation<Scene>) {
        let scene = simulation.game_mut();
        let side = Direction::ALL
            .into_iter()
            .find(|side| !scene.map.is_solid(side.neighbor(HOUSE_TILE)))
            .expect("the house can be reached");
        scene.hero.mover.place(side.neighbor(HOUSE_TILE));
        scene.hero.mover.face(side.opposite());
    }

    /// How much of the open dialogue is shown, out of how many glyphs, if any is open.
    fn shown(simulation: &Simulation<Scene>) -> Option<(f32, usize)> {
        let shared = simulation.game().shared.borrow();
        let dialogue = shared.dialogue.as_ref()?;
        Some((dialogue.shown, dialogue.layout.glyph_count()))
    }

    fn event_running(simulation: &Simulation<Scene>) -> bool {
        let scene = simulation.game();
        scene
            .event
            .is_some_and(|event| scene.scripts.is_running(event))
    }

    #[test]
    fn reading_the_note_shows_its_pages_in_turn() {
        let mut simulation = simulation();
        face_the_house(&mut simulation);
        simulation.tap(Key::Space);
        let (shown_so_far, first_page) = shown(&simulation).expect("the note opens");
        assert_eq!(shown_so_far, 0.0);

        simulation.run_for(0.5);
        let (shown_so_far, _) = shown(&simulation).unwrap();
        assert!(
            shown_so_far > 0.0 && shown_so_far < first_page as f32,
            "partly revealed: {shown_so_far} of {first_page}"
        );

        // The next press shows the rest, and the one after closes it.
        simulation.tap(Key::Enter);
        assert_eq!(shown(&simulation), Some((first_page as f32, first_page)));
        simulation.tap(Key::E);
        assert_eq!(shown(&simulation), None);

        // The script goes on to the second page on the next frame.
        simulation.step();
        let (_, second_page) = shown(&simulation).expect("the second page opens");
        assert_ne!(second_page, first_page);
        simulation.tap(Key::Space);
        simulation.tap(Key::Space);
        simulation.step();
        assert_eq!(shown(&simulation), None);
        assert!(!event_running(&simulation), "the note is read");
    }

    #[test]
    fn the_hero_stays_put_while_reading() {
        let mut simulation = simulation();
        face_the_house(&mut simulation);
        let tile = simulation.game().hero.mover.tile();
        simulation.tap(Key::Space);
        for key in [Key::Up, Key::Down, Key::Left, Key::Right] {
            simulation.hold(key, 0.3);
        }
        assert_eq!(simulation.game().hero.mover.tile(), tile);
        assert!(shown(&simulation).is_some());
    }

    #[test]
    fn theres_nothing_to_read_facing_away() {
        let mut simulation = simulation();
        face_the_house(&mut simulation);
        let mover = &mut simulation.game_mut().hero.mover;
        mover.face(mover.facing().opposite());
        simulation.tap(Key::Space);
        assert!(shown(&simulation).is_none());
        assert!(!event_running(&simulation));
    }

    #[test]
    fn music_plays_from_the_start() {
        let mut simulation = simulation();
        simulation.run_for(0.5);
        assert!(simulation.context().audio().music_playing());
    }

    #[test]
    fn walking_into_the_house_thuds() {
        let mut simulation = simulation();
        face_the_house(&mut simulation);
        let key = match simulation.game().hero.mover.facing() {
            Direction::Up => Key::W,
            Direction::Down => Key::S,
            Direction::Left => Key::A,
            Direction::Right => Key::D,
        };
        assert_eq!(simulation.context().audio().sounds_playing(), 0);
        simulation.tap(key);
        assert_eq!(simulation.context().audio().sounds_playing(), 1);
    }

    #[test]
    fn reading_the_note_chimes_and_text_blips() {
        let mut simulation = simulation();
        face_the_house(&mut simulation);
        simulation.tap(Key::Space);
        assert_eq!(
            simulation.context().audio().sounds_playing(),
            1,
            "the script's chime"
        );
        // Wait for the chime to end, then the text blips as it appears.
        simulation.run_for(0.5);
        let blips = (0..10)
            .map(|_| {
                simulation.step();
                simulation.context().audio().sounds_playing()
            })
            .max();
        assert_eq!(blips, Some(1));
    }

    #[test]
    fn the_bat_circles_the_house() {
        let mut simulation = simulation();
        let mut visited = std::collections::HashSet::new();
        // Long enough to go all the way round.
        for _ in 0..270 {
            simulation.step();
            visited.insert(simulation.game().shared.borrow().bat.tile());
        }
        let ring: std::collections::HashSet<[i32; 2]> = [[-1, -1], [0, -1], [1, -1], [1, 0]]
            .into_iter()
            .chain([[1, 1], [0, 1], [-1, 1], [-1, 0]])
            .map(|[dx, dy]| [HOUSE_TILE[0] + dx, HOUSE_TILE[1] + dy])
            .collect();
        assert_eq!(visited, ring);
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
