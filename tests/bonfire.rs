//! Tests for the bonfire example, a night by a fire written in Luau, in `examples/bonfire`, which
//! shows blend modes and particles: a scene's particles nodes, a night multiplied over it and
//! glows screened over that, and fireworks the script makes. They run it headlessly and check its
//! state, which `main.luau` keeps in the global table `bonfire`, and run its own tests, in
//! `bonfire.test.luau`.

use std::{fs, path::Path};

use wingine::{
    headless::Simulation,
    input::{Key, MouseButton},
    runtime::{self, LuauGame, testing},
    scene::SceneFile,
};

const GAME: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/bonfire");

/// How many of a screenshot's pixels each of the canvas's is: the window, 640x360, shows the
/// canvas, 320x180.
const SCALE: f32 = 2.0;

fn simulation() -> Simulation<LuauGame> {
    let config = runtime::config(GAME).expect("settings load");
    let mut simulation = Simulation::new(&config.window, |ctx| {
        LuauGame::load_with(ctx, GAME, &config)
    })
    .expect("game starts");
    simulation.step();
    simulation
}

fn check(simulation: &Simulation<LuauGame>) {
    if let Some(err) = simulation.game().error() {
        panic!("the game stopped: {err}");
    }
}

#[test]
fn the_bonfire_is_a_scene_in_the_canonical_layout() {
    let path = Path::new(GAME).join("bonfire.scene.luau");
    // As checked out, which may have turned line endings into CRLF.
    let text = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    let scene = SceneFile::parse(&text, &path).unwrap();
    assert!(
        scene.to_luau().unwrap() == text,
        "bonfire.scene.luau isn't in the layout scenes are written in: write it again with \
         `SceneFile::write`"
    );
}

/// Its own tests, in `bonfire.test.luau`, as `wingine test examples/bonfire` runs them. With
/// `UPDATE_SCREENSHOTS=1`, their reference is saved again.
#[test]
fn the_luau_tests_pass() {
    let options = testing::Options {
        update_screenshots: std::env::var_os("UPDATE_SCREENSHOTS").is_some(),
        ..Default::default()
    };
    let report = testing::run(GAME, &options, |_| {}).expect("the tests run");
    assert!(report.outcomes.len() >= 4, "{report}");
    assert!(report.passed(), "{report}");
}

/// How bright a pixel of the window is, its channels together.
fn brightness(simulation: &mut Simulation<LuauGame>, [x, y]: [f32; 2]) -> u32 {
    let image = simulation.screenshot().unwrap();
    let pixel = image.pixel((x * SCALE) as u32, (y * SCALE) as u32);
    pixel[..3].iter().map(|&channel| u32::from(channel)).sum()
}

#[test]
fn the_fire_lights_the_camp_as_brightly_as_it_burns() {
    let mut simulation = simulation();
    // The meadow beside the fire, out of reach of its flames and sparks, and in the night far
    // from it.
    let near = [126.0, 166.0];
    let far = [40.0, 166.0];
    let lit = brightness(&mut simulation, near);
    assert!(lit > brightness(&mut simulation, far) + 60, "the glow");

    simulation.tap(Key::Space);
    simulation.run_for(2.0);
    check(&simulation);
    let out = brightness(&mut simulation, near);
    assert!(out + 60 < lit, "dark once the fire's out: {out}, lit {lit}");
    assert!(
        out.abs_diff(brightness(&mut simulation, far)) < 12,
        "as dark as the rest of the night"
    );
}

#[test]
fn fireworks_take_each_color_in_turn() {
    let mut simulation = simulation();
    for i in 0..5 {
        let x = 40.0 + 50.0 * i as f32;
        simulation.click(MouseButton::Left, x, 30.0);
    }
    check(&simulation);
    let colors: Vec<String> = simulation.eval(
        "(function()
            local colors = {}
            for _, rocket in bonfire.rockets do
                table.insert(colors, rocket.burst:settings().colors[2])
            end
            return colors
        end)()",
    );
    assert_eq!(
        colors,
        [
            "#ff6080ff",
            "#60c0ffff",
            "#ffd040ff",
            "#80ff90ff",
            "#ff6080ff"
        ]
    );
}
