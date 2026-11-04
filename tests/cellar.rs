//! Tests for the cellar example, a 2.5D game written entirely in Luau, in `examples/cellar`. They
//! run it headlessly and check its state, which `main.luau` keeps in the global table `cellar`.

use wingine::{
    headless::Simulation,
    input::Key,
    lua::mlua::FromLuaMulti,
    runtime::{self, LuauGame},
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

/// The value of the Luau `expression`, such as `"cellar.lit"`.
fn eval<T: FromLuaMulti>(simulation: &Simulation<LuauGame>, expression: &str) -> T {
    let lua = simulation.game().scripts().lua();
    lua.load(format!("return {expression}"))
        .eval()
        .unwrap_or_else(|err| panic!("`{expression}`: {err}"))
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

fn tile(simulation: &Simulation<LuauGame>) -> (u32, u32) {
    eval(simulation, "cellar.mover:tile()")
}

/// How many of the map's 11x7 tiles are walls.
const WALLS: u32 = 44;

#[test]
fn the_cellar_starts_dark_with_the_hero_at_the_start() {
    let mut simulation = simulation();
    simulation.run_frames(1);
    assert_eq!(tile(&simulation), (5, 5));
    assert_eq!(eval::<u32>(&simulation, "#cellar.candles"), 4);
    assert_eq!(eval::<u32>(&simulation, "cellar.lit"), 0);
    // The floor, a quad for each tile that isn't a wall, the walls and 5 billboards, then the
    // HUD: its shade, then its text.
    let stats = simulation.context().frame_stats();
    let floor_tiles = 7 * 11 - WALLS;
    assert_eq!(stats.draw_calls, 1 + WALLS + 5 + 2);
    assert_eq!(stats.triangles, floor_tiles * 2 + WALLS * 12 + 5 * 2);
}

#[test]
fn walls_stop_the_hero() {
    let mut simulation = simulation();
    walk(&mut simulation, &[Key::Up, Key::Down]);
    assert_eq!(tile(&simulation), (5, 5), "walls above and below");
    walk(&mut simulation, &[Key::A; 6]);
    assert_eq!(tile(&simulation), (1, 5), "the wall at the left");
    assert!(eval::<bool>(&simulation, "cellar.facing_left"));
    walk(&mut simulation, &[Key::D]);
    assert!(!eval::<bool>(&simulation, "cellar.facing_left"));
}

#[test]
fn holding_a_key_keeps_walking() {
    let mut simulation = simulation();
    simulation.hold(Key::Right, 3.0 * STEP_TIME);
    simulation.run_for(STEP_TIME);
    assert_eq!(tile(&simulation), (8, 5));
}

#[test]
fn standing_on_every_candle_lights_them_all() {
    let mut simulation = simulation();
    let [up, down, left, right] = [Key::Up, Key::Down, Key::Left, Key::Right];
    walk(&mut simulation, &[right, right, up]);
    assert_eq!(eval::<u32>(&simulation, "cellar.lit"), 1);
    walk(&mut simulation, &[down, right, right, up, up, up, up]);
    assert_eq!(eval::<u32>(&simulation, "cellar.lit"), 2);
    // Standing on a lit candle again lights nothing more.
    walk(&mut simulation, &[down, up]);
    assert_eq!(eval::<u32>(&simulation, "cellar.lit"), 2);
    walk(&mut simulation, &[left, left, left, down, down, left, left]);
    assert_eq!(eval::<u32>(&simulation, "cellar.lit"), 3);
    assert!(!eval::<bool>(&simulation, "cellar.done"));
    walk(&mut simulation, &[left, left, left, up, up]);
    assert_eq!(tile(&simulation), (1, 1));
    assert_eq!(eval::<u32>(&simulation, "cellar.lit"), 4);
    assert!(eval::<bool>(&simulation, "cellar.done"));

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
    let target = |simulation: &Simulation<LuauGame>| {
        eval::<(f32, f32)>(
            simulation,
            "cellar.view().camera.target.x, cellar.view().camera.target.z",
        )
    };
    assert_eq!(target(&simulation), (5.5, 5.5));
    walk(&mut simulation, &[Key::Right]);
    assert_eq!(target(&simulation), (6.5, 5.5));
    // Where the hero shows: the middle of the canvas, the camera looking at their feet. Outside a
    // frame, the view needs its rect: there's no canvas to fill.
    let shown: (f32, f32) = eval(
        &simulation,
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
