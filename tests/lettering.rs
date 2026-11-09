//! Tests for the lettering example, a type specimen written in Luau, in `examples/lettering`: its
//! own tests, in `lettering.test.luau`, and the fonts its scene names.

use std::{fs, path::Path};

use wingine::{
    headless::Simulation,
    runtime::{self, LuauGame, testing},
    scene::SceneFile,
};

const GAME: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/lettering");

fn simulation() -> Simulation<LuauGame> {
    let config = runtime::config(GAME).expect("settings load");
    Simulation::new(&config.window, |ctx| {
        LuauGame::load_with(ctx, GAME, &config)
    })
    .expect("game starts")
}

/// Its own tests, in `lettering.test.luau`, as `wingine test examples/lettering` runs them. With
/// `UPDATE_SCREENSHOTS=1`, their reference is saved again.
#[test]
fn the_luau_tests_pass() {
    let options = testing::Options {
        update_screenshots: std::env::var_os("UPDATE_SCREENSHOTS").is_some(),
        ..Default::default()
    };
    let report = testing::run(GAME, &options, |_| {}).expect("the tests run");
    assert!(report.outcomes.len() >= 3, "{report}");
    assert!(report.passed(), "{report}");
}

#[test]
fn the_specimens_are_in_the_fonts_their_nodes_name() {
    let mut simulation = simulation();
    simulation.step();
    // Each line's height, as its font lays it out: the built-in font's lines are 10 apart.
    simulation.exec(
        "heights = {}
        for _, name in { 'built_in', 'built_in_large', 'bitmap', 'smooth', 'crisp', 'scripts' } do
            local _, height = lettering.specimen:find('specimens/' .. name):size()
            table.insert(heights, height)
        end",
    );
    let heights: Vec<f32> = simulation.eval("heights");
    assert_eq!(heights[0], 10.0, "the built-in font");
    assert_eq!(heights[1], 20.0, "twice its size");
    assert_eq!(heights[2], 15.0, "the title's font, its cells 14 tall");
    assert_eq!(heights[3], heights[4], "Hack, smooth and crisp");
    assert_eq!(heights[3], heights[5]);
    assert!((14.0..=16.0).contains(&heights[3]), "{heights:?}");
}

#[test]
fn the_specimen_is_a_scene_in_the_canonical_layout() {
    let path = Path::new(GAME).join("lettering.scene.luau");
    // As checked out, which may have turned line endings into CRLF.
    let text = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    let scene = SceneFile::parse(&text, &path).unwrap();
    assert!(
        scene.to_luau().unwrap() == text,
        "lettering.scene.luau isn't in the layout scenes are written in: write it again with \
         `SceneFile::write`"
    );
}
