//! Tests for the shmup example, a bullet hell written in Luau, in `examples/shmup`. They run it
//! headlessly and check its state, which `main.luau` keeps in the global table `shmup`, and drive
//! its bullet system, `shmup.bullets`, directly.

use std::f64::consts::{FRAC_PI_2, PI};

use wingine::{
    headless::Simulation,
    input::Key,
    lua::mlua::FromLuaMulti,
    runtime::{self, LuauGame},
};

const GAME: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/shmup");

// The game's tuning, as in its scripts.
const DT: f64 = 1.0 / 60.0;
const FIELD: [f64; 2] = [288.0, 336.0];
const BULLET_MARGIN: f64 = 32.0;
const START: [f64; 2] = [144.0, 288.0];
const SPEED: f64 = 170.0;
const FOCUS_SPEED: f64 = 70.0;
const MARGIN: f64 = 10.0;
const LIVES: u32 = 2;
const BOMBS: u32 = 3;
const CLEAR_POINTS: u64 = 10;
const GRAZE_POINTS: u64 = 100;
const SMALL_POINTS: u64 = 1000;
const LIFE_BONUS: u64 = 2_000_000;
const BOMB_BONUS: u64 = 500_000;
/// The first kind of bullet: a red pellet.
const PELLET: u32 = 1;

fn simulation() -> Simulation<LuauGame> {
    let config = runtime::config(GAME).expect("settings load");
    Simulation::new(&config.window, |ctx| LuauGame::load(ctx, GAME)).expect("game starts")
}

/// The value of the Luau `expression`, such as `"shmup.score"`.
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

fn step(simulation: &mut Simulation<LuauGame>) {
    simulation.step();
    if let Some(err) = simulation.game().error() {
        panic!("the game stopped: {err}");
    }
}

/// Runs frames until the Luau `condition` is true, for up to `seconds`. Returns whether it
/// became true.
fn run_until(simulation: &mut Simulation<LuauGame>, seconds: f32, condition: &str) -> bool {
    let frames = (seconds / Simulation::<LuauGame>::FRAME_TIME).round() as u32;
    (0..frames).any(|_| {
        step(simulation);
        eval(simulation, condition)
    })
}

fn run_for(simulation: &mut Simulation<LuauGame>, seconds: f32) {
    let frames = (seconds / Simulation::<LuauGame>::FRAME_TIME).round() as u32;
    for _ in 0..frames {
        step(simulation);
    }
}

/// A game in play with no enemies coming, so tests choose the bullets.
fn quiet_game() -> Simulation<LuauGame> {
    let simulation = simulation();
    exec(&simulation, "shmup.start() shmup.stage = nil");
    simulation
}

fn mode(simulation: &Simulation<LuauGame>) -> String {
    eval(simulation, "shmup.mode")
}

fn player(simulation: &Simulation<LuauGame>) -> [f64; 2] {
    let (x, y) = eval(simulation, "shmup.player.x, shmup.player.y");
    [x, y]
}

fn bullets(simulation: &Simulation<LuauGame>) -> usize {
    eval(simulation, "shmup.bullets.count()")
}

/// Every bullet's position, in the system's order.
fn positions(simulation: &Simulation<LuauGame>) -> Vec<[f64; 2]> {
    let flat: Vec<f64> = eval(simulation, "shmup.bullets.positions()");
    flat.chunks(2).map(|pair| [pair[0], pair[1]]).collect()
}

/// Spawns a bullet as `system.spawn` takes them, from `kind` on.
fn spawn(simulation: &Simulation<LuauGame>, args: &str) {
    exec(simulation, &format!("shmup.bullets.spawn({args})"));
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// Close enough for movement over frames, whose `dt` reaches scripts rounded to an `f32`.
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn the_title_menu_starts_each_mode() {
    let mut simulation = simulation();
    assert_eq!(mode(&simulation), "title");
    simulation.tap(Key::Up);
    assert_eq!(eval::<u32>(&simulation, "shmup.selected"), 3, "it wraps");
    simulation.tap(Key::Z);
    assert_eq!(mode(&simulation), "stress");
    simulation.tap(Key::Escape);
    assert_eq!(mode(&simulation), "title");

    simulation.tap(Key::Up);
    simulation.tap(Key::Enter);
    assert_eq!(mode(&simulation), "play");
    assert!(
        run_until(&mut simulation, 4.0, "shmup.boss ~= nil"),
        "boss practice"
    );

    let mut simulation = self::simulation();
    simulation.tap(Key::Z);
    assert_eq!(mode(&simulation), "play");
    run_for(&mut simulation, 3.0);
    assert!(
        eval::<bool>(&simulation, "shmup.boss == nil"),
        "fairies first"
    );
}

#[test]
fn the_player_moves_and_focusing_slows_them() {
    let mut simulation = quiet_game();
    simulation.hold(Key::Right, 0.5);
    let [x, y] = player(&simulation);
    assert!(near(x, START[0] + SPEED * 0.5), "at {x}");
    assert_eq!(y, START[1]);

    simulation.key_down(Key::LeftShift);
    simulation.hold(Key::Up, 0.5);
    assert!(eval::<bool>(&simulation, "shmup.player.focused"));
    simulation.key_up(Key::LeftShift);
    let [_, y] = player(&simulation);
    assert!(near(y, START[1] - FOCUS_SPEED * 0.5), "at {y}");

    simulation.hold(Key::Left, 3.0);
    assert_eq!(player(&simulation)[0], MARGIN, "stops at the edge");
}

#[test]
fn moving_diagonally_is_no_faster() {
    let mut simulation = quiet_game();
    simulation.key_down(Key::Up);
    simulation.hold(Key::Left, 0.5);
    simulation.key_up(Key::Up);
    let [x, y] = player(&simulation);
    let moved = (START[0] - x).hypot(START[1] - y);
    assert!(near(moved, SPEED * 0.5), "moved {moved}");
}

#[test]
fn shots_shoot_down_fairies_for_points() {
    let mut simulation = simulation();
    exec(&simulation, "shmup.start() shmup.player.x = 40");
    // The first ghost comes down the left side.
    simulation.key_down(Key::Z);
    assert!(run_until(&mut simulation, 4.0, "shmup.score >= 1000"));
    simulation.key_up(Key::Z);
    let score: u64 = eval(&simulation, "shmup.score");
    // Each hit scores 10 too.
    assert_eq!((score - SMALL_POINTS) % 10, 0);
    assert!(eval::<u32>(&simulation, "#shmup.shots") > 0);
}

#[test]
fn bullets_move_by_their_parameters() {
    let mut simulation = simulation();
    // Speeding up by 60 a second to at most 90, turning by half a radian a second.
    spawn(
        &simulation,
        &format!("{PELLET}, 100, 100, 0, 60, 1, 0, 60, 0.5, 0, 90"),
    );
    exec(
        &simulation,
        "for _ = 1, 60 do shmup.bullets.step(1 / 60) end",
    );
    let [mut x, mut y, mut angle, mut speed] = [100.0, 100.0, 0.0, 60.0_f64];
    for _ in 0..60 {
        speed = (speed + 60.0 * DT).clamp(0.0, 90.0);
        angle += 0.5 * DT;
        x += angle.cos() * speed * DT;
        y += angle.sin() * speed * DT;
    }
    let [[bx, by]] = positions(&simulation)[..] else {
        panic!("one bullet");
    };
    assert!(close(bx, x) && close(by, y), "at {bx}, {by}, not {x}, {y}");
    assert_eq!(speed, 90.0, "the limit was reached");

    // The title screen doesn't move bullets, but play does.
    step(&mut simulation);
    assert_eq!(positions(&simulation), [[bx, by]]);
}

#[test]
fn bullets_spawn_spaced_around_their_angle() {
    let simulation = simulation();
    spawn(
        &simulation,
        &format!("{PELLET}, 100, 100, {FRAC_PI_2}, 60, 3, 0.5"),
    );
    exec(&simulation, "shmup.bullets.step(1)");
    let positions = positions(&simulation);
    for (position, angle) in positions
        .iter()
        .zip([FRAC_PI_2 - 0.5, FRAC_PI_2, FRAC_PI_2 + 0.5])
    {
        let [x, y] = [100.0 + angle.cos() * 60.0, 100.0 + angle.sin() * 60.0];
        assert!(
            close(position[0], x) && close(position[1], y),
            "{positions:?}"
        );
    }
}

#[test]
fn bullets_leaving_the_field_are_removed() {
    let simulation = simulation();
    let edge = FIELD[0] + BULLET_MARGIN;
    // One heading off the right, one staying, one heading off the left.
    spawn(
        &simulation,
        &format!("{PELLET}, {}, 100, 0, 60", edge - 0.5),
    );
    spawn(&simulation, &format!("{PELLET}, 100, 120, 0, 0"));
    spawn(
        &simulation,
        &format!("{PELLET}, {}, 140, {PI}, 60", -BULLET_MARGIN + 0.5),
    );
    exec(&simulation, "shmup.bullets.step(1 / 60)");
    assert_eq!(positions(&simulation), [[100.0, 120.0]]);
    assert_eq!(eval::<u32>(&simulation, "shmup.bullets.clear()"), 1);
    assert_eq!(bullets(&simulation), 0);
}

#[test]
fn bullets_graze_the_player_once_and_hits_cost_a_life() {
    let mut simulation = quiet_game();
    let [x, y] = player(&simulation);
    // Straight down past the player, 8 pixels to their right.
    spawn(
        &simulation,
        &format!("{PELLET}, {}, {}, {FRAC_PI_2}, 240", x + 8.0, y - 60.0),
    );
    run_for(&mut simulation, 1.0);
    assert_eq!(eval::<u32>(&simulation, "shmup.graze"), 1);
    assert_eq!(eval::<u64>(&simulation, "shmup.score"), GRAZE_POINTS);
    assert_eq!(eval::<u32>(&simulation, "shmup.player.lives"), LIVES);

    exec(&simulation, "shmup.player.bombs = 1");
    spawn(
        &simulation,
        &format!("{PELLET}, {x}, {}, {FRAC_PI_2}, 240", y - 60.0),
    );
    spawn(&simulation, &format!("{PELLET}, 20, 20, 0, 0"));
    assert!(run_until(&mut simulation, 1.0, "shmup.player.dead > 0"));
    assert_eq!(eval::<u32>(&simulation, "shmup.player.lives"), LIVES - 1);
    assert_eq!(bullets(&simulation), 0, "a hit clears the bullets");
    assert_eq!(eval::<u32>(&simulation, "shmup.player.bombs"), BOMBS);

    // Back after a second, unhittable for a while.
    run_for(&mut simulation, 1.1);
    assert_eq!(player(&simulation), START);
    spawn(
        &simulation,
        &format!("{PELLET}, {}, {}, 0, 0", START[0], START[1]),
    );
    run_for(&mut simulation, 0.5);
    assert_eq!(eval::<u32>(&simulation, "shmup.player.lives"), LIVES - 1);
}

#[test]
fn a_bomb_clears_bullets_and_keeps_the_player_safe() {
    let mut simulation = quiet_game();
    exec(
        &simulation,
        &format!("shmup.bullets.spawn({PELLET}, 144, 100, 0, 30, 20, 0.3)"),
    );
    simulation.tap(Key::X);
    assert_eq!(eval::<u32>(&simulation, "shmup.player.bombs"), BOMBS - 1);
    assert_eq!(bullets(&simulation), 0);
    assert_eq!(eval::<u64>(&simulation, "shmup.score"), 20 * CLEAR_POINTS);

    // Bullets that come during it are cleared too, and none hits.
    let [x, y] = player(&simulation);
    spawn(&simulation, &format!("{PELLET}, {x}, {y}, 0, 0"));
    run_for(&mut simulation, 0.5);
    assert_eq!(bullets(&simulation), 0);
    assert_eq!(eval::<u32>(&simulation, "shmup.player.lives"), LIVES);
    simulation.tap(Key::X);
    assert_eq!(
        eval::<u32>(&simulation, "shmup.player.bombs"),
        BOMBS - 1,
        "one at a time"
    );
}

#[test]
fn losing_the_last_life_ends_the_game() {
    let mut simulation = quiet_game();
    exec(&simulation, "shmup.player.lives = 0");
    let [x, y] = player(&simulation);
    spawn(&simulation, &format!("{PELLET}, {x}, {y}, 0, 0"));
    assert!(run_until(&mut simulation, 2.0, "shmup.mode == 'over'"));
    simulation.tap(Key::Z);
    assert_eq!(mode(&simulation), "title");
}

/// Boss practice that shoots the boss down fast, with a player who can't be hit, standing under
/// it and shooting.
fn boss_fight() -> Simulation<LuauGame> {
    let mut simulation = simulation();
    exec(
        &simulation,
        "shmup.start('boss') shmup.cheats.damage = 30 shmup.cheats.invincible = true",
    );
    simulation.key_down(Key::Z);
    assert!(run_until(&mut simulation, 6.0, "shmup.card ~= nil"));
    simulation
}

#[test]
fn spell_cards_shot_down_cleanly_pay_their_bonus() {
    let mut simulation = boss_fight();
    assert_eq!(eval::<u32>(&simulation, "shmup.card.number"), 1);
    assert!(
        eval::<bool>(&simulation, "shmup.card.name == nil"),
        "an opening first"
    );
    assert!(run_until(&mut simulation, 10.0, "shmup.card == nil"));
    assert_eq!(
        bullets(&simulation),
        0,
        "the end of an attack clears the field"
    );

    assert!(run_until(&mut simulation, 5.0, "shmup.card ~= nil"));
    assert_eq!(eval::<u32>(&simulation, "shmup.card.number"), 2);
    let before: u64 = eval(&simulation, "shmup.score");
    assert!(run_until(&mut simulation, 10.0, "shmup.card == nil"));
    let gained = eval::<u64>(&simulation, "shmup.score") - before;
    assert!(gained > 500_000, "the bonus is paid: gained {gained}");
    assert_eq!(
        eval::<String>(&simulation, "shmup.message.headline"),
        "Spell Card Bonus!"
    );
}

#[test]
fn bombing_during_a_spell_card_forfeits_its_bonus() {
    let mut simulation = boss_fight();
    assert!(run_until(
        &mut simulation,
        15.0,
        "shmup.card ~= nil and shmup.card.number == 2"
    ));
    simulation.tap(Key::X);
    assert!(eval::<bool>(&simulation, "shmup.card.failed"));
    let before: u64 = eval(&simulation, "shmup.score");
    assert!(run_until(&mut simulation, 10.0, "shmup.card == nil"));
    let gained = eval::<u64>(&simulation, "shmup.score") - before;
    assert!(gained < 100_000, "no bonus: gained {gained}");
}

#[test]
fn a_spell_card_that_times_out_pays_nothing_and_the_next_begins() {
    let mut simulation = boss_fight();
    assert!(run_until(
        &mut simulation,
        15.0,
        "shmup.card ~= nil and shmup.card.number == 2"
    ));
    exec(
        &simulation,
        "shmup.cheats.damage = 0 shmup.card.time_left = 0.1",
    );
    assert!(run_until(
        &mut simulation,
        5.0,
        "shmup.card ~= nil and shmup.card.number == 3"
    ));
    assert!(eval::<bool>(
        &simulation,
        "shmup.message == nil or shmup.message.headline ~= 'Spell Card Bonus!'"
    ));
}

#[test]
fn the_stage_can_be_cleared() {
    let mut simulation = simulation();
    exec(
        &simulation,
        "shmup.start() shmup.cheats.damage = 10 shmup.cheats.invincible = true",
    );
    simulation.key_down(Key::Z);
    assert!(run_until(&mut simulation, 150.0, "shmup.mode == 'clear'"));
    assert_eq!(
        eval::<u64>(&simulation, "shmup.clear_bonus"),
        u64::from(LIVES) * LIFE_BONUS + u64::from(BOMBS) * BOMB_BONUS
    );
    simulation.key_up(Key::Z);
    simulation.tap(Key::Z);
    assert_eq!(mode(&simulation), "title");
}

#[test]
fn the_same_seed_fires_the_same_bullets() {
    let run = |seed: u32| {
        let mut simulation = simulation();
        exec(
            &simulation,
            &format!("shmup.seed = {seed} shmup.start('boss')"),
        );
        simulation.key_down(Key::LeftShift);
        run_for(&mut simulation, 12.0);
        positions(&simulation)
    };
    let first = run(1);
    assert!(first.len() > 20, "{} bullets", first.len());
    assert_eq!(first, run(1));
    assert_ne!(first, run(2));
}

#[test]
fn the_stress_test_pours_out_bullets_and_plots_them() {
    let mut simulation = simulation();
    exec(&simulation, "shmup.start_stress()");
    run_for(&mut simulation, 2.0);
    let count = bullets(&simulation);
    assert!(count >= 7000, "{count} bullets");
    assert!(eval::<bool>(
        &simulation,
        "next(shmup.stress.curves.Luau.bins) ~= nil"
    ));

    simulation.tap(Key::R);
    assert!(bullets(&simulation) < count, "running again starts over");
    simulation.tap(Key::Escape);
    assert_eq!(mode(&simulation), "title");
    assert_eq!(bullets(&simulation), 0);
}

#[test]
fn without_the_library_bullets_stay_in_luau() {
    let mut simulation = quiet_game();
    simulation.tap(Key::Tab);
    assert_eq!(eval::<String>(&simulation, "shmup.system"), "Luau");
    assert_eq!(
        eval::<String>(&simulation, "shmup.message.headline"),
        "No danmaku library"
    );
}
