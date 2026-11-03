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
fn f1_outlines_what_is_solid() {
    let mut simulation = simulation();
    let (x, y, width): (f32, f32, f32) = eval(
        &simulation,
        "(function() local b = garden.solids[1]:bounds() return b.x, b.y, b.width end)()",
    );
    // The canvas pixel in the middle of the outline's top edge, in the screenshot, which is
    // scaled up from the canvas.
    let middle_of_top = |simulation: &mut Simulation<LuauGame>| {
        let image = simulation.screenshot().unwrap();
        let scale = image.width() / 320;
        let [column, row] =
            [x + width / 2.0, y + 0.5].map(|at| at.floor() as u32 * scale + scale / 2);
        let offset = ((row * image.width() + column) * 4) as usize;
        <[u8; 4]>::try_from(&image.pixels()[offset..offset + 4]).unwrap()
    };
    let before = middle_of_top(&mut simulation);

    simulation.tap(Key::F1);
    assert!(eval::<bool>(&simulation, "garden.debug"));
    let [red, green, blue, _] = middle_of_top(&mut simulation);
    assert!(
        red > green + 60 && red > blue + 60,
        "{:?}",
        [red, green, blue]
    );

    simulation.tap(Key::F1);
    assert!(!eval::<bool>(&simulation, "garden.debug"));
    assert_eq!(middle_of_top(&mut simulation), before);
}

#[test]
fn the_scene_files_are_in_the_canonical_layout() {
    for name in ["garden", "tree", "flower"] {
        let path = Path::new(GAME).join(format!("{name}.scene.luau"));
        // As checked out, which may have turned line endings into CRLF.
        let text = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
        let scene = SceneFile::parse(&text, &path).unwrap();
        assert!(
            scene.to_luau().unwrap() == text,
            "{name}.scene.luau isn't in the layout scenes are written in: write it again with \
             `SceneFile::write`"
        );
    }
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

    // A tree, from `tree.scene.luau`, covering 50 to 66 across and 38 to 54 down.
    exec(&simulation, "garden.player:set_position(58, 70)");
    simulation.hold(Key::Up, 1.0);
    let [_, y] = player(&simulation);
    assert!((54.0..56.0).contains(&y), "stopped below the tree at {y}");
}

#[test]
fn space_plants_and_picks_flowers() {
    let mut simulation = simulation();
    let flowers = |simulation: &Simulation<LuauGame>| -> u32 {
        eval(simulation, "#garden.flowers:children()")
    };
    assert_eq!(flowers(&simulation), 5);

    exec(&simulation, "garden.player:set_position(160, 120)");
    simulation.tap(Key::Space);
    run_for(&mut simulation, 0.05);
    assert_eq!(flowers(&simulation), 6);
    assert_eq!(
        eval::<(f32, f32)>(&simulation, "garden.flowers:find('planted_1'):position()"),
        (160.0, 120.0),
        "at the player's feet"
    );
    assert_eq!(
        eval::<(f32, f32)>(
            &simulation,
            "garden.flowers:find('planted_1/flower'):size()"
        ),
        (16.0, 16.0)
    );

    // Standing on it, Space picks it.
    simulation.tap(Key::Space);
    run_for(&mut simulation, 0.05);
    assert_eq!(flowers(&simulation), 5);
    assert!(eval::<bool>(
        &simulation,
        "garden.flowers:find('planted_1') == nil"
    ));

    // So it does a flower the scene placed, which covers 196 to 212 across and 150 to 166 down.
    exec(&simulation, "garden.player:set_position(204, 162)");
    simulation.tap(Key::Space);
    run_for(&mut simulation, 0.05);
    assert_eq!(flowers(&simulation), 4);
    assert!(eval::<bool>(&simulation, "garden.flowers:find('3') == nil"));
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
