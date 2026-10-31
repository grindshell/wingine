//! Tests for the shmup example, a bullet hell written in Luau, in `examples/shmup`. They run it
//! headlessly and check its state, which `main.luau` keeps in the global table `shmup`, and drive
//! its bullet system, `shmup.bullets`, directly. Tests of the bullet systems run each of them:
//! `bullets.luau`, and the `danmaku` native library, which they build from
//! `examples/native-danmaku`.

use std::{
    env::consts::{DLL_PREFIX, DLL_SUFFIX},
    f64::consts::{FRAC_PI_2, PI},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

use wingine::{
    headless::{Image, Simulation},
    input::Key,
    lua::mlua::FromLuaMulti,
    runtime::{self, LuauGame},
};

const GAME: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/shmup");
/// The bullet systems, by the names the game gives them.
const SYSTEMS: [&str; 2] = ["Luau", "native"];

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

/// The shmup, copied to a folder of its own with the `danmaku` library in its `native` folder, or
/// without it. Tests run copies so that a library someone put in `examples/shmup/native` doesn't
/// change what they test.
fn game(with_danmaku: bool) -> &'static Path {
    static WITH: OnceLock<PathBuf> = OnceLock::new();
    static WITHOUT: OnceLock<PathBuf> = OnceLock::new();
    let (copy, name) = if with_danmaku {
        (&WITH, "shmup-danmaku")
    } else {
        (&WITHOUT, "shmup")
    };
    copy.get_or_init(|| {
        let folder = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        if folder.exists() {
            fs::remove_dir_all(&folder).unwrap();
        }
        copy_files(Path::new(GAME), &folder);
        if with_danmaku {
            let library = build_danmaku();
            let native = folder.join("native");
            fs::create_dir_all(&native).unwrap();
            fs::copy(&library, native.join(library.file_name().unwrap())).unwrap();
        }
        folder
    })
}

/// Copies the files in `from` to `to`, and those in its folders but `native`.
fn copy_files(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let (path, name) = (entry.path(), entry.file_name());
        if path.is_dir() {
            if name != "native" {
                copy_files(&path, &to.join(name));
            }
        } else {
            fs::copy(&path, to.join(name)).unwrap();
        }
    }
}

/// Builds the `danmaku` library, returning its file. It builds in a target folder of its own,
/// since the one running the tests is busy.
fn build_danmaku() -> PathBuf {
    let target = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-build");
    let output = Command::new(env!("CARGO"))
        .args(["build", "--package", "native-danmaku", "--target-dir"])
        .arg(&target)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    target
        .join("debug")
        .join(format!("{DLL_PREFIX}danmaku{DLL_SUFFIX}"))
}

fn load(folder: &Path) -> Simulation<LuauGame> {
    let config = runtime::config(folder).expect("settings load");
    Simulation::new(&config.window, |ctx| LuauGame::load(ctx, folder)).expect("game starts")
}

/// The game, with both bullet systems, running bullets in Luau.
fn simulation() -> Simulation<LuauGame> {
    load(game(true))
}

/// The game, running its bullets in `system`.
fn simulation_with(system: &str) -> Simulation<LuauGame> {
    let simulation = simulation();
    use_system(&simulation, system);
    simulation
}

/// Switches the game's bullets to `system`, clearing those there are.
fn use_system(simulation: &Simulation<LuauGame>, system: &str) {
    for _ in SYSTEMS {
        if eval::<String>(simulation, "shmup.system") != system {
            exec(simulation, "shmup.switch_system()");
        }
    }
    assert_eq!(eval::<String>(simulation, "shmup.system"), system);
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

/// The error running the Luau `code` fails with.
fn exec_error(simulation: &Simulation<LuauGame>, code: &str) -> String {
    let scripts = simulation.game().scripts();
    match scripts.load_source("test", code) {
        Ok(_) => panic!("`{code}` runs"),
        Err(err) => err.to_string(),
    }
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

/// A game in play with no enemies coming, so tests choose the bullets, which run in `system`.
fn quiet_game(system: &str) -> Simulation<LuauGame> {
    let simulation = simulation_with(system);
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
    let mut simulation = quiet_game("Luau");
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
    let mut simulation = quiet_game("Luau");
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
    for system in SYSTEMS {
        let mut simulation = simulation_with(system);
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
            panic!("{system}: one bullet");
        };
        assert!(
            close(bx, x) && close(by, y),
            "{system}: at {bx}, {by}, not {x}, {y}"
        );
        assert_eq!(speed, 90.0, "the limit was reached");

        // The title screen doesn't move bullets.
        step(&mut simulation);
        assert_eq!(positions(&simulation), [[bx, by]], "{system}");
    }
}

#[test]
fn bullets_spawn_spaced_around_their_angle() {
    for system in SYSTEMS {
        let simulation = simulation_with(system);
        spawn(
            &simulation,
            &format!("{PELLET}, 100, 100, {FRAC_PI_2}, 60, 3, 0.5"),
        );
        exec(&simulation, "shmup.bullets.step(1)");
        let positions = positions(&simulation);
        assert_eq!(positions.len(), 3, "{system}");
        for (position, angle) in positions
            .iter()
            .zip([FRAC_PI_2 - 0.5, FRAC_PI_2, FRAC_PI_2 + 0.5])
        {
            let [x, y] = [100.0 + angle.cos() * 60.0, 100.0 + angle.sin() * 60.0];
            assert!(
                close(position[0], x) && close(position[1], y),
                "{system}: {positions:?}"
            );
        }
    }
}

#[test]
fn bullets_leaving_the_field_are_removed() {
    for system in SYSTEMS {
        let simulation = simulation_with(system);
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
        assert_eq!(positions(&simulation), [[100.0, 120.0]], "{system}");
        assert_eq!(eval::<u32>(&simulation, "shmup.bullets.clear()"), 1);
        assert_eq!(bullets(&simulation), 0, "{system}");
    }
}

#[test]
fn bullets_that_cannot_be_are_errors() {
    for system in SYSTEMS {
        let simulation = simulation_with(system);
        for (call, expected) in [
            (
                "spawn(99, 0, 0, 0, 0)".to_owned(),
                "there's no bullet kind 99",
            ),
            (
                format!("spawn({PELLET}, 0, 0, 0, 10, 1, 0, 0, 0, 50, 20)"),
                "min_speed is more than max_speed",
            ),
            (
                "add(99, 0, 0, 0, 0, 0, 0, 0, 0, false)".to_owned(),
                "there's no bullet kind 99",
            ),
            (
                format!("add({PELLET}, 0, 0, 0, 10, 0, 0, 50, 20, false)"),
                "min_speed is more than max_speed",
            ),
            ("bullet(1)".to_owned(), "there's no bullet 1"),
        ] {
            let err = exec_error(&simulation, &format!("shmup.bullets.{call}"));
            assert!(err.contains(expected), "{system}: {err}");
        }
        assert_eq!(bullets(&simulation), 0, "{system}");
    }
}

#[test]
fn bullets_graze_the_player_once_and_hits_cost_a_life() {
    for system in SYSTEMS {
        graze_and_hit(system);
    }
}

fn graze_and_hit(system: &str) {
    let mut simulation = quiet_game(system);
    let [x, y] = player(&simulation);
    // Straight down past the player, 8 pixels to their right.
    spawn(
        &simulation,
        &format!("{PELLET}, {}, {}, {FRAC_PI_2}, 240", x + 8.0, y - 60.0),
    );
    run_for(&mut simulation, 1.0);
    assert_eq!(eval::<u32>(&simulation, "shmup.graze"), 1, "{system}");
    assert_eq!(eval::<u64>(&simulation, "shmup.score"), GRAZE_POINTS);
    assert_eq!(eval::<u32>(&simulation, "shmup.player.lives"), LIVES);

    exec(&simulation, "shmup.player.bombs = 1");
    spawn(
        &simulation,
        &format!("{PELLET}, {x}, {}, {FRAC_PI_2}, 240", y - 60.0),
    );
    spawn(&simulation, &format!("{PELLET}, 20, 20, 0, 0"));
    assert!(
        run_until(&mut simulation, 1.0, "shmup.player.dead > 0"),
        "{system}: hit"
    );
    assert_eq!(eval::<u32>(&simulation, "shmup.player.lives"), LIVES - 1);
    assert_eq!(
        bullets(&simulation),
        0,
        "{system}: a hit clears the bullets"
    );
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
    let mut simulation = quiet_game("Luau");
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
    let mut simulation = quiet_game("Luau");
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

/// What a game with its bullets in `system` does from the `practice` given to `shmup.start`,
/// shooting with `damage` and weaving from side to side, with a player who can't be hit, and
/// pressing Tab at the start of each of the seconds in `switches`: every bullet's position each
/// second, and the score and graze at the end.
fn play(
    system: &str,
    practice: &str,
    damage: u32,
    seconds: u32,
    switches: &[u32],
) -> (Vec<Vec<[f64; 2]>>, u64, u32) {
    let mut simulation = simulation_with(system);
    exec(
        &simulation,
        &format!(
            "shmup.start({practice})
            shmup.cheats.damage = {damage}
            shmup.cheats.invincible = true"
        ),
    );
    simulation.key_down(Key::Z);
    let mut seconds_of_bullets = Vec::new();
    for second in 0..seconds {
        let (from, to) = if second % 2 == 0 {
            (Key::Right, Key::Left)
        } else {
            (Key::Left, Key::Right)
        };
        simulation.key_up(from);
        simulation.key_down(to);
        let mut frames = 60;
        if switches.contains(&second) {
            simulation.tap(Key::Tab);
            frames -= 1;
        }
        for _ in 0..frames {
            step(&mut simulation);
        }
        seconds_of_bullets.push(positions(&simulation));
    }
    let score = eval(&simulation, "shmup.score");
    let graze = eval(&simulation, "shmup.graze");
    (seconds_of_bullets, score, graze)
}

#[test]
fn both_systems_fire_the_same_bullets() {
    // The stage's fairies, and the boss's attacks.
    for (practice, damage, seconds) in [("nil", 1, 30), ("'boss'", 10, 40)] {
        let (luau, luau_score, luau_graze) = play("Luau", practice, damage, seconds, &[]);
        let (native, native_score, native_graze) = play("native", practice, damage, seconds, &[]);
        for (second, (luau, native)) in luau.iter().zip(&native).enumerate() {
            // Exactly the same, since their arithmetic is.
            assert!(
                luau == native,
                "{practice}: after {} seconds, {} bullets in Luau and {} native, \
                 the first different at {:?}",
                second + 1,
                luau.len(),
                native.len(),
                luau.iter().zip(native).find(|(a, b)| a != b),
            );
        }
        let most = luau.iter().map(Vec::len).max().unwrap();
        assert!(most > 100, "{practice}: at most {most} bullets");
        assert!(luau_graze > 0, "{practice}: no grazes");
        assert_eq!(luau_graze, native_graze, "{practice}");
        assert_eq!(luau_score, native_score, "{practice}");
    }
}

/// The bullets' field in `image`, a screenshot, without the panel beside it, which names the
/// bullet system.
fn field_pixels(image: &Image) -> Vec<u8> {
    // Screenshots are twice the canvas's size, and the field is 16 canvas pixels from the left
    // and 12 from the top.
    let scale = 2;
    let [x, y] = [16 * scale, 12 * scale];
    let [width, height] = [FIELD[0] as usize * scale, FIELD[1] as usize * scale];
    let row = image.width() as usize * 4;
    image
        .pixels()
        .chunks(row)
        .skip(y)
        .take(height)
        .flat_map(|pixels| &pixels[x * 4..(x + width) * 4])
        .copied()
        .collect()
}

#[test]
fn both_systems_draw_bullets_alike() {
    let draw = |system: &str| {
        let mut simulation = quiet_game(system);
        // A ring of each kind, overlapping and turning, so that the order they're drawn in shows.
        exec(
            &simulation,
            "for kind = 1, 24 do
                shmup.bullets.spawn(kind, 144, 150, kind * 0.3, 40, 8, 0.8, 0, 1)
            end",
        );
        run_for(&mut simulation, 0.5);
        assert_eq!(bullets(&simulation), 24 * 8, "{system}");
        field_pixels(&simulation.screenshot().unwrap())
    };
    let (luau, native) = (draw("Luau"), draw("native"));
    let different = luau
        .chunks(4)
        .zip(native.chunks(4))
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(different, 0, "pixels differ");
}

/// Everything about a bullet, as `system.bullet` returns it.
type Snapshot = (u32, f64, f64, f64, f64, f64, f64, f64, f64, bool);

/// Everything about every bullet, in the system's order.
fn snapshots(simulation: &Simulation<LuauGame>) -> Vec<Snapshot> {
    (1..=bullets(simulation))
        .map(|i| eval(simulation, &format!("shmup.bullets.bullet({i})")))
        .collect()
}

#[test]
fn tab_switches_systems_and_they_take_over_the_bullets() {
    let mut simulation = quiet_game("Luau");
    // A fan of bullets, speeding up and turning, which graze a point below them.
    spawn(
        &simulation,
        &format!("{PELLET}, 100, 100, 1, 10, 5, 0.1, 5, 0.5, 0, 50"),
    );
    let graze = "select(2, shmup.bullets.collide(100, 110, 2, 16))";
    assert_eq!(eval::<u32>(&simulation, graze), 5);
    let before = snapshots(&simulation);
    assert!(before.iter().all(|bullet| bullet.9), "grazed");

    // Paused, so that bullets don't move as the key is pressed.
    exec(&simulation, "shmup.paused = true");
    simulation.tap(Key::Tab);
    assert_eq!(eval::<String>(&simulation, "shmup.system"), "native");
    assert!(eval::<bool>(&simulation, "shmup.bullets == danmaku"));
    assert_eq!(snapshots(&simulation), before);
    assert_eq!(eval::<u32>(&simulation, graze), 0, "they grazed already");

    spawn(&simulation, &format!("{PELLET}, 100, 100, 0, 0, 3, 0.1"));
    simulation.tap(Key::Tab);
    assert_eq!(eval::<String>(&simulation, "shmup.system"), "Luau");
    assert_eq!(bullets(&simulation), 8);
    assert_eq!(snapshots(&simulation)[..5], before);
}

#[test]
fn switching_systems_carries_on_as_though_nothing_changed() {
    // The boss's attacks, switching to Rust and back partway, against staying in Luau.
    let (stay, stay_score, stay_graze) = play("Luau", "'boss'", 10, 20, &[]);
    let (switch, switch_score, switch_graze) = play("Luau", "'boss'", 10, 20, &[6, 13]);
    assert!(stay == switch, "the bullets differ");
    assert!(stay[5].len() > 20, "{} bullets switched", stay[5].len());
    assert_eq!(stay_graze, switch_graze);
    assert_eq!(stay_score, switch_score);
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
    run_for(&mut simulation, 0.5);

    // The other system starts from nothing, and both are plotted.
    simulation.tap(Key::Tab);
    assert_eq!(eval::<String>(&simulation, "shmup.system"), "native");
    run_for(&mut simulation, 2.0);
    let count = bullets(&simulation);
    assert!(count >= 7000, "{count} native bullets");
    assert!(eval::<bool>(
        &simulation,
        "next(shmup.stress.curves.native.bins) ~= nil
            and next(shmup.stress.curves.Luau.bins) ~= nil"
    ));

    simulation.tap(Key::Escape);
    assert_eq!(mode(&simulation), "title");
    assert_eq!(bullets(&simulation), 0);
}

#[test]
fn without_the_library_bullets_stay_in_luau() {
    let mut simulation = load(game(false));
    exec(&simulation, "shmup.start() shmup.stage = nil");
    assert!(eval::<bool>(&simulation, "danmaku == nil"));
    simulation.tap(Key::Tab);
    assert_eq!(eval::<String>(&simulation, "shmup.system"), "Luau");
    assert_eq!(
        eval::<String>(&simulation, "shmup.message.headline"),
        "No danmaku library"
    );
}
