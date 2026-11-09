//! Tests for the quarry example, a level seen from the side written in Luau, in `examples/quarry`,
//! which shows 2D physics: a tile map solid where its tiles are, crates and a boulder that fall
//! and roll, a lift that carries what's on it, and a character walked among them. They run it
//! headlessly and check its state, which `main.luau` keeps in the global table `quarry`, and run
//! its own tests, in `quarry.test.luau`.

use std::{fs, path::Path};

use wingine::{
    headless::Simulation,
    input::Key,
    runtime::{self, LuauGame, testing},
    scene::SceneFile,
};

const GAME: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/quarry");

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

/// Holds `keys` together for `seconds`.
fn hold(simulation: &mut Simulation<LuauGame>, keys: &[Key], seconds: f32) {
    for &key in keys {
        simulation.key_down(key);
    }
    simulation.run_for(seconds);
    for &key in keys {
        simulation.key_up(key);
    }
    check(simulation);
}

/// Where the node at `path` is.
fn position(simulation: &mut Simulation<LuauGame>, path: &str) -> (f32, f32) {
    simulation.eval(&format!("quarry.scene:find('{path}'):position()"))
}

/// Puts the player at `x`, `y`, standing still.
fn place(simulation: &mut Simulation<LuauGame>, x: f32, y: f32) {
    simulation.exec(&format!(
        "quarry.player:set_position({x}, {y})
        quarry.falling = 0"
    ));
}

fn near(actual: f32, expected: f32, tolerance: f32) -> bool {
    (actual - expected).abs() <= tolerance
}

#[test]
fn the_quarry_is_a_scene_in_the_canonical_layout() {
    let path = Path::new(GAME).join("quarry.scene.luau");
    // As checked out, which may have turned line endings into CRLF.
    let text = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    let scene = SceneFile::parse(&text, &path).unwrap();
    assert!(
        scene.to_luau().unwrap() == text,
        "quarry.scene.luau isn't in the layout scenes are written in: write it again with \
         `SceneFile::write`"
    );
}

/// Its own tests, in `quarry.test.luau`, as `wingine test examples/quarry` runs them. With
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

#[test]
fn the_player_stops_at_walls_and_falls_into_the_pit() {
    let mut simulation = simulation();
    // The brick wall down the level's left, 16 across: the player's box is 8 wide.
    hold(&mut simulation, &[Key::Left], 1.0);
    let (x, y) = position(&mut simulation, "player");
    assert!(near(x, 20.01, 0.01) && near(y, 239.99, 0.01), "{x}, {y}");
    // Into the pit, 16 deep, and out again with a jump.
    place(&mut simulation, 110.0, 240.0);
    simulation.run_for(0.5);
    let (_, y) = position(&mut simulation, "player");
    assert!(near(y, 255.99, 0.01), "{y}");
    simulation.key_down(Key::Right);
    simulation.tap(Key::Space);
    simulation.run_for(0.8);
    simulation.key_up(Key::Right);
    check(&simulation);
    let (x, y) = position(&mut simulation, "player");
    assert!(x > 130.0 && near(y, 239.99, 0.01), "{x}, {y}");
}

#[test]
fn a_crate_pushed_to_the_ledge_lets_the_player_climb_it() {
    let mut simulation = simulation();
    // Push the far crate against the ledge, 48 high, which the player can't jump up to alone.
    place(&mut simulation, 150.0, 240.0);
    hold(&mut simulation, &[Key::Right], 1.5);
    let (x, y) = position(&mut simulation, "crates/far");
    assert!(near(x, 200.0, 0.5) && near(y, 240.0, 0.2), "{x}, {y}");
    // Onto the crate, then onto the ledge.
    simulation.key_down(Key::Right);
    simulation.tap(Key::Space);
    simulation.run_for(0.8);
    simulation.tap(Key::Space);
    simulation.run_for(0.8);
    simulation.key_up(Key::Right);
    check(&simulation);
    let (x, y) = position(&mut simulation, "player");
    assert!(x > 208.0 && near(y, 191.99, 0.01), "on the ledge: {x}, {y}");
}

#[test]
fn the_boulder_rolls_off_the_ledge_and_down_the_slope() {
    let mut simulation = simulation();
    place(&mut simulation, 230.0, 192.0);
    hold(&mut simulation, &[Key::Right], 1.0);
    simulation.run_for(4.0);
    check(&simulation);
    // On the floor, its middle 8 above it, past the slope, and turned as it rolled.
    let (x, y) = position(&mut simulation, "boulder");
    assert!(x > 330.0 && near(y, 232.0, 0.3), "{x}, {y}");
    let turned: f32 = simulation.eval("quarry.boulder:rotation()");
    assert!(turned != 0.0, "{turned}");
}

#[test]
fn the_lift_carries_crates_too() {
    let mut simulation = simulation();
    // A crate put on the lift as it waits at the bottom, carried up with it.
    simulation.exec("quarry.scene:find('crates/near'):set_position(336, 180)");
    simulation.run_for(3.6);
    check(&simulation);
    let (x, y) = position(&mut simulation, "crates/near");
    assert!(near(x, 336.0, 1.0) && near(y, 96.0, 0.5), "{x}, {y}");
}

/// A hash of the bits of where the crates, the boulder and the player are, and how the crates
/// and the boulder are turned.
fn hash_of_the_quarry(simulation: &mut Simulation<LuauGame>) -> u64 {
    let numbers: Vec<f32> = simulation.eval(
        "(function()
            local numbers = {}
            for _, node in { quarry.crates[1], quarry.crates[2], quarry.boulder, quarry.player } do
                local x, y = node:position()
                table.insert(numbers, x)
                table.insert(numbers, y)
                table.insert(numbers, node:rotation())
            end
            return numbers
        end)()",
    );
    // FNV-1a.
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in numbers.iter().flat_map(|n| n.to_bits().to_le_bytes()) {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

#[test]
fn the_same_run_ends_the_same_to_the_bit_on_every_platform() {
    let mut simulation = simulation();
    // The near crate into the pit, over it, the far crate to the ledge, up onto the ledge, the
    // boulder down the slope, and onto the lift.
    hold(&mut simulation, &[Key::Right], 1.2);
    simulation.key_down(Key::Right);
    simulation.tap(Key::Space);
    simulation.run_for(2.0);
    simulation.tap(Key::Space);
    simulation.run_for(0.8);
    simulation.tap(Key::Space);
    simulation.run_for(2.0);
    simulation.key_up(Key::Right);
    simulation.run_for(3.0);
    check(&simulation);
    let hash = hash_of_the_quarry(&mut simulation);
    // Rapier's enhanced determinism, and `libm` for the rest, make this the same on Windows and
    // Linux, debug and release. A change to how the quarry plays, or to physics, changes it.
    assert_eq!(hash, 0x8b7c_e10f_339d_daa1, "{hash:#x}");
}
