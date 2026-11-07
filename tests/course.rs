//! Tests for the course example, a 3D game written entirely in Luau, in `examples/course`. They
//! run it headlessly and check its state, which `main.luau` keeps in the global table `course`.

use std::{fs, path::Path};

use wingine::{
    headless::Simulation,
    input::Key,
    runtime::{self, LuauGame},
    scene::SceneFile,
};

const GAME: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/course");

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

/// Where the player's feet are.
fn feet(simulation: &mut Simulation<LuauGame>) -> [f32; 3] {
    let (x, y, z) = simulation.eval("course.feet().x, course.feet().y, course.feet().z");
    [x, y, z]
}

/// Where the crate `name` is.
fn crate_at(simulation: &mut Simulation<LuauGame>, name: &str) -> [f32; 3] {
    let (x, y, z) = simulation.eval(&format!(
        "(function()
            local at = course.view:find('crates/{name}'):position_3d()
            return at.x, at.y, at.z
        end)()"
    ));
    [x, y, z]
}

/// Puts the player at `x`, `y`, `z`, standing still.
fn place(simulation: &mut Simulation<LuauGame>, [x, y, z]: [f32; 3]) {
    simulation.exec(&format!(
        "course.player:set_position_3d(vector.create({x}, {y}, {z}))
        course.rising = 0"
    ));
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

fn near(actual: f32, expected: f32, tolerance: f32) -> bool {
    (actual - expected).abs() <= tolerance
}

#[test]
fn the_course_is_a_scene_in_the_canonical_layout() {
    let path = Path::new(GAME).join("course.scene.luau");
    // As checked out, which may have turned line endings into CRLF.
    let text = fs::read_to_string(&path).unwrap().replace("\r\n", "\n");
    let scene = SceneFile::parse(&text, &path).unwrap();
    assert!(
        scene.to_luau().unwrap() == text,
        "course.scene.luau isn't in the layout scenes are written in: write it again with \
         `SceneFile::write`"
    );
}

#[test]
fn the_player_starts_on_the_ground_and_the_crates_settle() {
    let mut simulation = simulation();
    simulation.run_for(3.0);
    check(&simulation);
    let [x, y, z] = feet(&mut simulation);
    assert!(x == 0.0 && near(y, 0.0, 0.01) && z == 3.5, "{x}, {y}, {z}");
    assert!(simulation.eval::<bool>("course.grounded"));
    assert_eq!(simulation.eval::<u32>("#course.coins"), 5);
    assert_eq!(simulation.eval::<u32>("course.found"), 0);
    // Where the scene put them, the stack's each half a unit up from the one below.
    for (name, height) in [("ledge_1", 1.0), ("stack_1", 0.0), ("stack_3", 1.0)] {
        let [_, y, _] = crate_at(&mut simulation, name);
        assert!(near(y, height, 0.02), "{name}: {y}");
        let resting = format!("course.world:is_resting(course.view:find('crates/{name}'))");
        assert!(simulation.eval::<bool>(&resting), "{name}");
    }
}

#[test]
fn walls_stop_the_player_without_its_creeping_along_them() {
    let mut simulation = simulation();
    hold(&mut simulation, &[Key::Right], 4.0);
    // Its side, 0.2 from its middle, a hair from the wall's inside, at 5.8.
    let [x, y, z] = feet(&mut simulation);
    assert!(near(x, 5.6, 0.005) && x < 5.6, "{x}");
    assert!(near(y, 0.0, 0.01) && z == 3.5, "{y}, {z}");
}

#[test]
fn the_player_climbs_the_stairs_onto_the_platform() {
    let mut simulation = simulation();
    // In front of the stairs, which rise toward the platform on their left.
    place(&mut simulation, [-1.2, 0.002, -4.0]);
    hold(&mut simulation, &[Key::Left], 1.5);
    let [x, y, _] = feet(&mut simulation);
    assert!(x < -2.8 && near(y, 1.0, 0.01), "{x}, {y}");
    assert!(simulation.eval::<bool>("course.grounded"));
}

#[test]
fn the_player_walks_down_the_slope_picking_up_its_coin() {
    let mut simulation = simulation();
    place(&mut simulation, [-5.0, 1.002, -2.5]);
    simulation.key_down(Key::Down);
    // On the ground all the way down, with nothing to fall from.
    for _ in 0..90 {
        simulation.step();
        assert!(simulation.eval::<bool>("course.grounded"));
    }
    simulation.key_up(Key::Down);
    check(&simulation);
    let [_, y, z] = feet(&mut simulation);
    assert!(z > 0.2 && near(y, 0.0, 0.01), "{y}, {z}");
    assert_eq!(simulation.eval::<u32>("course.found"), 1);
    assert!(simulation.eval::<bool>("course.view:find('coins/slope'):is_visible() == false"));
}

#[test]
fn the_player_jumps_onto_the_block() {
    let mut simulation = simulation();
    // In front of the block, which is too high to step onto.
    place(&mut simulation, [3.5, 0.002, 3.0]);
    hold(&mut simulation, &[Key::Up], 1.0);
    let [_, y, z] = feet(&mut simulation);
    assert!(near(z, 2.2, 0.005) && near(y, 0.0, 0.01), "{y}, {z}");
    // Jumping forward, it picks up the coin over the block, and lands on top.
    simulation.key_down(Key::Up);
    simulation.tap(Key::Space);
    simulation.run_for(0.35);
    simulation.key_up(Key::Up);
    simulation.run_for(0.5);
    check(&simulation);
    let [_, y, z] = feet(&mut simulation);
    assert!(near(y, 0.6, 0.01) && z < 2.0 && z > 1.0, "{y}, {z}");
    assert!(simulation.eval::<bool>("course.grounded"));
    assert_eq!(simulation.eval::<u32>("course.found"), 1);
}

#[test]
fn crates_pushed_off_the_platform_fall_to_the_ground() {
    let mut simulation = simulation();
    // Behind the crate at the platform's front edge, pushing it off, and walking off after it.
    place(&mut simulation, [-3.1, 1.002, -3.0]);
    hold(&mut simulation, &[Key::Down], 1.0);
    simulation.run_for(2.0);
    check(&simulation);
    let [_, y, z] = crate_at(&mut simulation, "ledge_1");
    assert!(z > -1.8 && y < 0.6, "{y}, {z}");
    // The crate beside it is where it was.
    let [x, y, z] = crate_at(&mut simulation, "ledge_2");
    assert!(near(x, -3.7, 0.01) && near(y, 1.0, 0.01) && near(z, -2.1, 0.01));
}

#[test]
fn the_stack_topples_when_pushed() {
    let mut simulation = simulation();
    place(&mut simulation, [3.0, 0.002, -0.5]);
    hold(&mut simulation, &[Key::Up], 1.0);
    simulation.run_for(2.0);
    check(&simulation);
    let highest = ["stack_1", "stack_2", "stack_3"]
        .map(|name| crate_at(&mut simulation, name)[1])
        .into_iter()
        .fold(f32::MIN, f32::max);
    assert!(highest < 0.9, "still stacked: {highest}");
}

#[test]
fn falling_off_the_course_starts_again() {
    let mut simulation = simulation();
    place(&mut simulation, [0.0, 0.002, 4.7]);
    hold(&mut simulation, &[Key::Down], 0.5);
    assert!(!simulation.eval::<bool>("course.grounded"));
    simulation.run_for(1.5);
    check(&simulation);
    let [x, y, z] = feet(&mut simulation);
    assert!(x == 0.0 && near(y, 0.0, 0.01) && z == 3.5, "{x}, {y}, {z}");
}

#[test]
fn crates_that_fall_off_are_gone() {
    let mut simulation = simulation();
    simulation.exec("course.view:find('crates/stack_1'):set_position_3d(vector.create(0, 0, 6))");
    simulation.run_for(2.0);
    check(&simulation);
    assert_eq!(simulation.eval::<u32>("#course.crates"), 4);
    assert!(simulation.eval::<bool>("course.view:find('crates/stack_1') == nil"));
}

#[test]
fn finding_every_coin_finishes_the_course() {
    let mut simulation = simulation();
    // All but the middle one, which the player walks to.
    simulation.exec(
        "for _, coin in course.coins do
            if coin.node:name() ~= 'middle' then
                coin.found = true
                course.found += 1
            end
        end",
    );
    hold(&mut simulation, &[Key::Up], 1.5);
    assert_eq!(simulation.eval::<u32>("course.found"), 5);
    assert!(simulation.eval::<bool>("course.done"));
}

/// A hash of the bits of every crate's position and turn, and the player's feet.
fn hash_of_the_course(simulation: &mut Simulation<LuauGame>) -> u64 {
    let numbers: Vec<f32> = simulation.eval(
        "(function()
            local numbers = {}
            for _, crate in course.crates do
                local at = crate:position_3d()
                local yaw, pitch, roll = crate:orientation()
                for _, n in { at.x, at.y, at.z, yaw, pitch, roll } do
                    table.insert(numbers, n)
                end
            end
            local feet = course.feet()
            for _, n in { feet.x, feet.y, feet.z } do
                table.insert(numbers, n)
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
    // Up the stairs, off the platform pushing its crates, and into the stack.
    place(&mut simulation, [-1.2, 0.002, -4.0]);
    hold(&mut simulation, &[Key::Left], 1.2);
    hold(&mut simulation, &[Key::Left, Key::Down], 0.4);
    hold(&mut simulation, &[Key::Down], 1.0);
    hold(&mut simulation, &[Key::Right], 2.0);
    simulation.tap(Key::Space);
    hold(&mut simulation, &[Key::Right, Key::Up], 2.0);
    simulation.run_for(2.0);
    check(&simulation);
    let hash = hash_of_the_course(&mut simulation);
    // Rapier's enhanced determinism, and `libm` for the rest, make this the same on Windows and
    // Linux, debug and release. A change to how the course plays, or to physics, changes it.
    assert_eq!(hash, 0x7d8a_ea08_8420_3fdd, "{hash:#x}");
}
