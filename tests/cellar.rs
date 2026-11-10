//! Tests for the cellar example, a 2.5D game written entirely in Luau, in `examples/cellar`. They
//! run it headlessly and check its state, which `main.luau` keeps in the global table `cellar`.

use std::{fs, path::Path};

use wingine::{
    headless::Simulation,
    input::Key,
    runtime::{self, LuauGame},
    scene::SceneFile,
};

const GAME: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/cellar");
/// Seconds a step takes, as in `main.luau`.
const STEP_TIME: f32 = 0.18;

fn simulation() -> Simulation<LuauGame> {
    let config = runtime::config(GAME).expect("settings load");
    Simulation::new(&config.window, |ctx| {
        LuauGame::load_with(ctx, GAME, &config)
    })
    .expect("game starts")
}

/// Taps each of `keys` in turn, letting the step it starts finish before the next.
fn walk(simulation: &mut Simulation<LuauGame>, keys: &[Key]) {
    for &key in keys {
        simulation.tap(key);
        simulation.run_for(STEP_TIME + 0.05);
        if let Some(err) = simulation.game().error() {
            panic!("the game stopped: {err}");
        }
    }
}

fn tile(simulation: &mut Simulation<LuauGame>) -> (u32, u32) {
    simulation.eval("cellar.mover:tile()")
}

/// How many of the map's 11x7 tiles are walls.
const WALLS: u32 = 44;

#[test]
fn the_cellar_starts_dark_with_the_hero_at_the_start() {
    let mut simulation = simulation();
    simulation.run_frames(1);
    assert_eq!(tile(&mut simulation), (5, 5));
    assert_eq!(simulation.eval::<u32>("#cellar.candles"), 4);
    assert_eq!(simulation.eval::<u32>("cellar.lit"), 0);
    // The floor, a plane under the whole map, the walls, the props, and 5 billboards, then the
    // HUD: its shade, then its text. The walls are one mesh, so they're one draw call, as are the
    // billboards, of one sheet. The props are a crate, a barrel, a table, a crate and a barrel: a
    // draw call for the crates and one for the barrels, wherever they are, and two for the
    // table's top and its four legs, one mesh.
    let stats = simulation.context().frame_stats();
    assert_eq!(simulation.eval::<u32>("#cellar.props"), 5);
    assert_eq!(
        simulation.eval::<u32>("#cellar.world:tagged('solid')"),
        WALLS + 5
    );
    let kinds: Vec<String> = simulation.eval(
        "(function()
            local kinds = {}
            for _, prop in cellar.props do
                table.insert(kinds, prop.kind)
            end
            return kinds
        end)()",
    );
    let prop_triangles: u32 = kinds
        .iter()
        .map(|kind| {
            let path = Path::new(GAME).join(format!("models/{kind}.glb"));
            let model = wingine::scene::load_model(simulation.context(), &path).unwrap();
            model.triangle_count()
        })
        .sum();
    assert_eq!(stats.draw_calls, 1 + 1 + (1 + 1 + 2) + 1 + 2);
    assert_eq!(stats.triangles, 2 + WALLS * 12 + prop_triangles + 5 * 2);
}

#[test]
fn the_level_is_a_scene_in_the_canonical_layout() {
    let path = Path::new(GAME).join("cellar.scene.luau");
    // As checked out, which may have turned line endings into CRLF.
    let text = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    let scene = SceneFile::parse(&text, &path).unwrap();
    assert!(
        scene.to_luau().unwrap() == text,
        "cellar.scene.luau isn't in the layout scenes are written in: write it again with \
         `SceneFile::write`"
    );
}

#[test]
fn the_hero_is_drawn_where_they_walk() {
    let mut simulation = simulation();
    walk(&mut simulation, &[Key::Left]);
    let (x, z): (f32, f32) =
        simulation.eval("cellar.hero:position_3d().x, cellar.hero:position_3d().z");
    assert_eq!((x, z), (4.5, 5.5));
    // Facing left, they're mirrored.
    let (width, _): (f32, f32) = simulation.eval("cellar.hero:size()");
    assert_eq!(width, -1.0);
}

#[test]
fn props_stop_the_hero() {
    let mut simulation = simulation();
    // Up the right-hand side to the crate's row, then left into it.
    walk(&mut simulation, &[Key::Right; 4]);
    walk(&mut simulation, &[Key::Up, Key::Up, Key::Left]);
    assert_eq!(tile(&mut simulation), (9, 3), "the crate");
    walk(&mut simulation, &[Key::Up, Key::Left]);
    assert_eq!(tile(&mut simulation), (9, 2), "the table");
}

#[test]
fn the_mouse_points_at_props() {
    let mut simulation = simulation();
    simulation.run_frames(1);
    assert!(simulation.eval::<bool>("cellar.pointed == nil"));
    // Where the middle of the crate east of the start shows.
    let (x, y): (f32, f32) = simulation.eval(
        "graphics.project(
            { camera = cellar.view().camera, rect = { x = 0, y = 0, width = 320, height = 180 } },
            vector.create(8.5, 0.375, 3.5)
        )",
    );
    simulation.move_mouse(x, y);
    simulation.run_frames(2);
    assert_eq!(simulation.eval::<String>("cellar.pointed.kind"), "crate");
    // Its name is drawn over it: a quad for each letter, after the HUD's.
    let glyphs = |text: &str| text.chars().filter(|c| *c != ' ').count() as u32;
    let hud = 1 + glyphs("Candles lit: 0/4");
    let quads = simulation.context().frame_stats().quads;
    assert_eq!(quads, hud + glyphs("crate"));

    // Off every prop, it points at none.
    simulation.move_mouse(160.0, 90.0);
    simulation.run_frames(1);
    assert!(simulation.eval::<bool>("cellar.pointed == nil"));
}

#[test]
fn walls_stop_the_hero() {
    let mut simulation = simulation();
    walk(&mut simulation, &[Key::Up, Key::Down]);
    assert_eq!(tile(&mut simulation), (5, 5), "walls above and below");
    walk(&mut simulation, &[Key::A; 6]);
    assert_eq!(tile(&mut simulation), (1, 5), "the wall at the left");
    assert!(simulation.eval::<bool>("cellar.facing_left"));
    walk(&mut simulation, &[Key::D]);
    assert!(!simulation.eval::<bool>("cellar.facing_left"));
}

#[test]
fn holding_a_key_keeps_walking() {
    let mut simulation = simulation();
    simulation.hold(Key::Right, 3.0 * STEP_TIME);
    simulation.run_for(STEP_TIME);
    assert_eq!(tile(&mut simulation), (8, 5));
}

#[test]
fn standing_on_every_candle_lights_them_all() {
    let mut simulation = simulation();
    let [up, down, left, right] = [Key::Up, Key::Down, Key::Left, Key::Right];
    walk(&mut simulation, &[right, right, up]);
    assert_eq!(simulation.eval::<u32>("cellar.lit"), 1);
    walk(&mut simulation, &[down, right, right, up, up, up, up]);
    assert_eq!(simulation.eval::<u32>("cellar.lit"), 2);
    // Standing on a lit candle again lights nothing more.
    walk(&mut simulation, &[down, up]);
    assert_eq!(simulation.eval::<u32>("cellar.lit"), 2);
    walk(&mut simulation, &[left, left, left, down, down, left, left]);
    assert_eq!(simulation.eval::<u32>("cellar.lit"), 3);
    assert!(!simulation.eval::<bool>("cellar.done"));
    walk(&mut simulation, &[left, left, left, up, up]);
    assert_eq!(tile(&mut simulation), (1, 1));
    assert_eq!(simulation.eval::<u32>("cellar.lit"), 4);
    assert!(simulation.eval::<bool>("cellar.done"));

    // The message over the hero's head is drawn: a quad for each glyph of it, and the HUD's.
    simulation.run_frames(1);
    let glyphs = |text: &str| text.chars().filter(|c| *c != ' ').count() as u32;
    let hud = 1 + glyphs("Candles lit: 4/4");
    let quads = simulation.context().frame_stats().quads;
    assert_eq!(quads, hud + glyphs("Every candle is lit!"));
}

#[test]
fn the_camera_follows_the_hero() {
    let mut simulation = simulation();
    let target = |simulation: &mut Simulation<LuauGame>| {
        simulation
            .eval::<(f32, f32)>("cellar.view().camera.target.x, cellar.view().camera.target.z")
    };
    assert_eq!(target(&mut simulation), (5.5, 5.5));
    walk(&mut simulation, &[Key::Right]);
    assert_eq!(target(&mut simulation), (6.5, 5.5));
    // Where the hero shows: the middle of the canvas, the camera looking at their feet. Outside a
    // frame, the view needs its rect: there's no canvas to fill.
    let shown: (f32, f32) = simulation.eval(
        "graphics.project(
            { camera = cellar.view().camera, rect = { x = 0, y = 0, width = 320, height = 180 } },
            cellar.hero_position()
        )",
    );
    assert!(
        (shown.0 - 160.0).abs() < 0.01 && (shown.1 - 90.0).abs() < 0.01,
        "{shown:?}"
    );
}
