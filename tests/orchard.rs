//! Tests for the orchard example, a game written in Luau around a rigged character, in
//! `examples/orchard`. They run it headlessly and check its state, which `main.luau` keeps in the
//! global table `orchard`.

use std::{fs, path::Path};

use wingine::{
    headless::Simulation,
    input::Key,
    runtime::{self, LuauGame},
    scene::SceneFile,
};

const GAME: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/orchard");

/// The game, growing no apples but those a test grows.
fn simulation() -> Simulation<LuauGame> {
    let config = runtime::config(GAME).expect("settings load");
    let mut simulation = Simulation::new(&config.window, |ctx| {
        LuauGame::load_with(ctx, GAME, &config)
    })
    .expect("game starts");
    simulation.exec("orchard.growing = false");
    simulation
}

fn run_for(simulation: &mut Simulation<LuauGame>, seconds: f32) {
    simulation.run_for(seconds);
    if let Some(err) = simulation.game().error() {
        panic!("the game stopped: {err}");
    }
}

/// The rotation of the picker's node at `path`, such as `"hips/leg_f"`.
fn rotation(simulation: &mut Simulation<LuauGame>, path: &str) -> f32 {
    simulation.eval(&format!("orchard.picker:find('{path}'):rotation()"))
}

fn hips(simulation: &mut Simulation<LuauGame>) -> f32 {
    let (_, y): (f32, f32) = simulation.eval("orchard.picker:find('hips'):position()");
    y
}

#[test]
fn the_scene_files_are_in_the_canonical_layout() {
    for name in ["orchard", "picker"] {
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
fn the_pickers_clips_come_from_its_file() {
    let mut simulation = simulation();
    let clips: Vec<String> = simulation.eval(
        "(function() local names = {} for name in orchard.clips do table.insert(names, name) end \
         table.sort(names) return names end)()",
    );
    assert_eq!(clips, ["blink", "catch", "idle", "miss", "slump", "walk"]);
    assert_eq!(
        simulation.eval::<(f32, bool, bool)>(
            "orchard.clips.walk:duration(), orchard.clips.walk:is_looping(), \
             orchard.clips.slump:is_looping()"
        ),
        (0.4, true, false)
    );
}

#[test]
fn the_picker_swings_its_legs_as_it_walks_and_stands_still_once_it_stops() {
    let mut simulation = simulation();
    let (x, _): (f32, f32) = simulation.eval("orchard.picker:position()");
    let mut swings = Vec::new();
    for _ in 0..10 {
        simulation.hold(Key::Right, 0.05);
        swings.push(rotation(&mut simulation, "hips/leg_f"));
    }
    let (walked, _): (f32, f32) = simulation.eval("orchard.picker:position()");
    // 80 pixels a second.
    assert!((walked - x - 40.0).abs() < 2.0, "from {x} to {walked}");
    assert_eq!(simulation.eval::<f32>("orchard.stride"), 1.0);
    let lowest = swings.iter().copied().fold(f32::MAX, f32::min);
    let highest = swings.iter().copied().fold(f32::MIN, f32::max);
    assert!(
        lowest < -0.3 && highest > 0.3,
        "the front leg swings both ways: {swings:?}"
    );

    simulation.hold(Key::Left, 0.1);
    assert_eq!(
        simulation.eval::<(f32, f32)>("orchard.picker:scale()"),
        (-1.0, 1.0),
        "mirrored to face left"
    );

    // Once the walk has faded out, the idle clip has every part back.
    run_for(&mut simulation, 0.3);
    assert_eq!(simulation.eval::<f32>("orchard.stride"), 0.0);
    for leg in [
        "hips/leg_f",
        "hips/leg_f/shin_f",
        "hips/leg_b",
        "hips/leg_b/shin_b",
    ] {
        assert_eq!(rotation(&mut simulation, leg), 0.0, "{leg}");
    }
    let y = hips(&mut simulation);
    assert!((-14.0..=-13.5).contains(&y), "breathing, at {y}");
}

#[test]
fn apples_falling_into_the_basket_are_caught() {
    let mut simulation = simulation();
    // Facing left, so the picker, and the basket's mouth, are mirrored.
    simulation.hold(Key::Left, 0.1);
    run_for(&mut simulation, 0.3);
    let (x, _): (f32, f32) = simulation.eval("orchard.picker:position()");
    simulation.exec(&format!("orchard.grow({x})"));
    run_for(&mut simulation, 1.0);
    assert_eq!(
        simulation.eval::<String>("orchard.apples[1].state"),
        "growing"
    );
    run_for(&mut simulation, 1.0);
    assert_eq!(
        simulation.eval::<String>("orchard.apples[1].state"),
        "falling"
    );
    run_for(&mut simulation, 1.0);
    assert_eq!(simulation.eval::<u32>("orchard.score"), 1);
    assert_eq!(simulation.eval::<u32>("#orchard.apples"), 0);
    assert_eq!(
        simulation.eval::<u32>("#orchard.scene:find('apples'):children()"),
        0,
        "its node is gone"
    );
    assert_eq!(
        simulation.eval::<String>("orchard.scene:find('hud/score/apples'):text()"),
        "Apples 1"
    );
    assert!(!simulation.eval::<bool>("orchard.scene:find('hud/hint'):is_visible()"));
    assert_eq!(simulation.eval::<u32>("orchard.misses"), 0);
}

#[test]
fn the_catch_raises_the_pickers_arms_and_fades_away() {
    let mut simulation = simulation();
    simulation.exec("orchard.grow(160)");
    // Until the apple is caught, then a little more, into the clip.
    while simulation.eval::<u32>("orchard.score") == 0 {
        run_for(&mut simulation, 1.0 / 60.0);
    }
    assert_eq!(
        simulation.eval::<Option<String>>("orchard.reaction"),
        Some("catch".into())
    );
    run_for(&mut simulation, 0.2);
    let arm = rotation(&mut simulation, "hips/body_front/arm_f");
    assert!(arm < -2.0, "raised to the basket: {arm}");
    run_for(&mut simulation, 0.4);
    assert_eq!(simulation.eval::<Option<String>>("orchard.reaction"), None);
    let arm = rotation(&mut simulation, "hips/body_front/arm_f");
    assert!(arm > -0.1, "down again: {arm}");
}

#[test]
fn three_apples_on_the_ground_end_the_day() {
    let mut simulation = simulation();
    simulation.exec("orchard.grow(40) orchard.grow(60) orchard.grow(280)");
    run_for(&mut simulation, 3.0);
    assert_eq!(simulation.eval::<u32>("orchard.misses"), 3);
    assert_eq!(
        simulation.eval::<Vec<String>>(
            "{ orchard.apples[1].state, orchard.apples[2].state, orchard.apples[3].state }"
        ),
        ["fallen", "fallen", "fallen"]
    );
    assert!(simulation.eval::<bool>("orchard.over_at ~= nil"));
    assert!(simulation.eval::<bool>("orchard.scene:find('hud/banner'):is_visible()"));

    // The picker sits down, and stays down: the clip holds its last pose.
    run_for(&mut simulation, 1.5);
    assert_eq!(hips(&mut simulation), -2.5);
    assert_eq!(rotation(&mut simulation, "hips/leg_f"), -1.5);
    assert_eq!(
        simulation.eval::<u32>("#orchard.apples"),
        0,
        "the fallen apples are cleared away"
    );
    let (x, _): (f32, f32) = simulation.eval("orchard.picker:position()");
    simulation.hold(Key::Right, 0.3);
    assert_eq!(
        simulation.eval::<(f32, f32)>("orchard.picker:position()").0,
        x,
        "and doesn't walk"
    );

    simulation.tap(Key::Space);
    run_for(&mut simulation, 0.1);
    assert_eq!(simulation.eval::<u32>("orchard.misses"), 0);
    assert!(simulation.eval::<bool>("orchard.over_at == nil"));
    assert!(!simulation.eval::<bool>("orchard.scene:find('hud/banner'):is_visible()"));
    let y = hips(&mut simulation);
    assert!((-14.0..=-13.5).contains(&y), "standing again, at {y}");
}

/// Where the camera looks, and its zoom.
fn camera(simulation: &mut Simulation<LuauGame>) -> [f32; 3] {
    let (x, y, zoom) = simulation.eval("orchard.camera.x, orchard.camera.y, orchard.camera.zoom");
    [x, y, zoom]
}

#[test]
fn the_camera_closes_in_on_the_picker_at_the_end_of_the_day() {
    let mut simulation = simulation();
    let whole = [160.0, 90.0, 1.0];
    simulation.hold(Key::Left, 0.5);
    assert_eq!(
        camera(&mut simulation),
        whole,
        "the whole orchard, while they play"
    );

    simulation.exec("orchard.grow(200) orchard.grow(240) orchard.grow(280)");
    run_for(&mut simulation, 3.0);
    assert!(simulation.eval::<bool>("orchard.over_at ~= nil"));
    let [_, _, zoom] = camera(&mut simulation);
    assert!(1.0 < zoom && zoom < 2.0, "easing in: {zoom}");
    run_for(&mut simulation, 1.0);
    let (x, y): (f32, f32) = simulation.eval("orchard.picker:position()");
    assert_eq!(camera(&mut simulation), [x, y - 24.0, 2.0], "on the picker");

    simulation.tap(Key::Space);
    run_for(&mut simulation, 0.5);
    let [_, _, zoom] = camera(&mut simulation);
    assert!(1.0 < zoom && zoom < 2.0, "easing out: {zoom}");
    run_for(&mut simulation, 0.5);
    assert_eq!(camera(&mut simulation), whole);
}
