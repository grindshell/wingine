//! Tests for the garden example, a game written in Luau around a scene file, in
//! `examples/garden`. They run it headlessly and check its state, which `main.luau` keeps in the
//! global table `garden`.

use std::{fs, path::Path};

use wingine::{
    headless::Simulation,
    input::Key,
    lua::mlua::FromLuaMulti,
    runtime::{self, LuauGame},
    scene::SceneFile,
};

const GAME: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/garden");

fn simulation() -> Simulation<LuauGame> {
    let config = runtime::config(GAME).expect("settings load");
    Simulation::new(&config.window, |ctx| LuauGame::load(ctx, GAME)).expect("game starts")
}

/// The value of the Luau `expression`, such as `"garden.walk_time"`.
fn eval<T: FromLuaMulti>(simulation: &Simulation<LuauGame>, expression: &str) -> T {
    let lua = simulation.game().scripts().lua();
    lua.load(format!("return {expression}"))
        .eval()
        .unwrap_or_else(|err| panic!("`{expression}`: {err}"))
}

/// Runs the Luau `code`, such as to change the game's state.
fn exec(simulation: &Simulation<LuauGame>, code: &str) {
    let scripts = simulation.game().scripts();
    scripts
        .load_source("test", code)
        .unwrap_or_else(|err| panic!("{err}"));
}

fn run_for(simulation: &mut Simulation<LuauGame>, seconds: f32) {
    simulation.run_for(seconds);
    if let Some(err) = simulation.game().error() {
        panic!("the game stopped: {err}");
    }
}

/// Where the middle of the player's feet is.
fn player(simulation: &Simulation<LuauGame>) -> [f32; 2] {
    let (x, y) = eval(simulation, "garden.player:position()");
    [x, y]
}

#[test]
fn the_scene_file_is_in_the_canonical_layout() {
    let path = Path::new(GAME).join("garden.scene.luau");
    // As checked out, which may have turned line endings into CRLF.
    let text = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    let scene = SceneFile::parse(&text, &path).unwrap();
    assert!(
        scene.to_luau().unwrap() == text,
        "garden.scene.luau isn't in the layout scenes are written in: write it again with \
         `SceneFile::write`"
    );
}

#[test]
fn the_player_walks_and_faces_the_way_it_goes() {
    let mut simulation = simulation();
    let [x, y] = player(&simulation);
    simulation.hold(Key::Left, 0.5);
    let [left, same] = player(&simulation);
    // 64 pixels a second.
    assert!((x - left - 32.0).abs() < 2.0, "from {x} to {left}");
    assert_eq!(same, y);
    assert_eq!(
        eval::<(f32, f32)>(&simulation, "garden.player:scale()"),
        (-1.0, 1.0),
        "mirrored to face left"
    );
    assert!(eval::<f32>(&simulation, "garden.walk_time") > 0.0);

    run_for(&mut simulation, 0.1);
    assert_eq!(player(&simulation), [left, same], "stopped");
    assert_eq!(eval::<f32>(&simulation, "garden.player:rotation()"), 0.0);
    simulation.hold(Key::D, 0.1);
    assert_eq!(
        eval::<(f32, f32)>(&simulation, "garden.player:scale()"),
        (1.0, 1.0)
    );
}

#[test]
fn solid_things_are_in_the_way() {
    let mut simulation = simulation();
    // Below the bench, which covers 168 to 184 across and 80 to 96 down.
    exec(&simulation, "garden.player:set_position(176, 120)");
    simulation.hold(Key::Up, 1.0);
    let [x, y] = player(&simulation);
    assert_eq!(x, 176.0);
    assert!((96.0..98.0).contains(&y), "stopped below the bench at {y}");

    // Sliding along it to the right, until it's clear, then up past it.
    simulation.hold(Key::Right, 0.5);
    simulation.hold(Key::Up, 0.5);
    let [x, y] = player(&simulation);
    assert!(x > 188.0 && y < 80.0, "went around the bench to {x}, {y}");
}

#[test]
fn standing_beside_things_shows_their_notes() {
    let mut simulation = simulation();
    assert!(!eval::<bool>(&simulation, "garden.note:is_visible()"));
    // Just below the well.
    exec(&simulation, "garden.player:set_position(112, 94)");
    run_for(&mut simulation, 0.05);
    assert!(eval::<bool>(&simulation, "garden.note:is_visible()"));
    let text: String = eval(&simulation, "garden.note_text:text()");
    let expected: String = eval(&simulation, "garden.scene:find('well/rim'):props().text");
    assert_eq!(text, expected);
    assert!(text.contains("well"), "{text}");

    exec(&simulation, "garden.player:set_position(160, 140)");
    run_for(&mut simulation, 0.05);
    assert!(!eval::<bool>(&simulation, "garden.note:is_visible()"));
    assert!(eval::<bool>(&simulation, "garden.showing == nil"));
}

#[test]
fn the_sails_and_the_flock_turn_at_their_speeds() {
    let mut simulation = simulation();
    let bird = "garden.scene:find('flock/1'):world_position()";
    let start: (f32, f32) = eval(&simulation, bird);
    run_for(&mut simulation, 1.0);
    let sails: f32 = eval(&simulation, "garden.scene:find('mill/sails'):rotation()");
    assert!((sails - 0.8).abs() < 0.05, "{sails}");
    let flock: f32 = eval(&simulation, "garden.scene:find('flock'):rotation()");
    assert!((flock - 0.6).abs() < 0.05, "{flock}");
    let moved: (f32, f32) = eval(&simulation, bird);
    assert!(
        moved.1 > start.1 + 20.0,
        "circled clockwise, from {start:?} to {moved:?}"
    );
}
