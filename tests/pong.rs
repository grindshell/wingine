//! Tests for the Pong example, a game written entirely in Luau, in `examples/pong`. They run it
//! headlessly and check its state, which `main.luau` keeps in the global table `pong`.

use wingine::{
    headless::Simulation,
    input::{Key, MouseButton},
    lua::mlua::FromLuaMulti,
    runtime::{self, LuauGame},
};

const GAME: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/pong");

// The game's tuning, as in `main.luau`.
const PADDLE_HEIGHT: f64 = 24.0;
const PLAYER_SPEED: f64 = 160.0;
const BALL_SIZE: f64 = 4.0;
const SERVE_SPEED: f64 = 120.0;
const SPEEDUP: f64 = 1.08;
const MAX_BOUNCE_ANGLE: f64 = std::f64::consts::FRAC_PI_3;
const WINNING_SCORE: u32 = 5;
const SERVE_DELAY: f32 = 1.0;
/// The ball's center and velocity before a serve.
const RESTING_BALL: [f64; 4] = [160.0, 90.0, 0.0, 0.0];

fn simulation() -> Simulation<LuauGame> {
    let config = runtime::config(GAME).expect("window settings load");
    Simulation::new(&config, |ctx| LuauGame::load(ctx, GAME)).expect("game starts")
}

/// The value of the Luau `expression`, such as `"pong.ball.vx"`.
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

/// Runs frames until the Luau `condition` is true, for up to `seconds`. Returns whether it
/// became true.
fn run_until(simulation: &mut Simulation<LuauGame>, seconds: f32, condition: &str) -> bool {
    let frames = (seconds / Simulation::<LuauGame>::FRAME_TIME).round() as u32;
    (0..frames).any(|_| {
        simulation.step();
        if let Some(err) = simulation.game().error() {
            panic!("the game stopped: {err}");
        }
        eval(simulation, condition)
    })
}

/// Puts the ball in play, centered on `x`, `y` and moving at `vx`, `vy`.
fn play_ball(simulation: &Simulation<LuauGame>, [x, y]: [f64; 2], [vx, vy]: [f64; 2]) {
    exec(
        simulation,
        &format!("pong.ball = {{ x = {x}, y = {y}, vx = {vx}, vy = {vy} }} pong.phase = 'rally'"),
    );
}

/// The ball's center and velocity.
fn ball(simulation: &Simulation<LuauGame>) -> [f64; 4] {
    let (x, y, vx, vy) = eval(
        simulation,
        "pong.ball.x, pong.ball.y, pong.ball.vx, pong.ball.vy",
    );
    [x, y, vx, vy]
}

fn phase(simulation: &Simulation<LuauGame>) -> String {
    eval(simulation, "pong.phase")
}

/// The left and right sides' points.
fn score(simulation: &Simulation<LuauGame>) -> [u32; 2] {
    let (left, right) = eval(simulation, "pong.score.left, pong.score.right");
    [left, right]
}

fn player_center(simulation: &Simulation<LuauGame>) -> f64 {
    eval::<f64>(simulation, "pong.left.y") + PADDLE_HEIGHT / 2.0
}

fn sounds_playing(simulation: &mut Simulation<LuauGame>) -> usize {
    simulation.context().audio().sounds_playing()
}

#[test]
fn the_ball_waits_for_a_serve() {
    let mut simulation = simulation();
    simulation.run_for(1.0);
    assert_eq!(phase(&simulation), "ready");
    assert_eq!(ball(&simulation), RESTING_BALL);

    simulation.tap(Key::Space);
    assert_eq!(phase(&simulation), "rally");
    let [_, _, vx, _] = ball(&simulation);
    assert!(vx > 0.0, "the first serve goes to the computer");
}

#[test]
fn clicking_serves() {
    let mut simulation = simulation();
    simulation.click(MouseButton::Left, 100.0, 90.0);
    assert_eq!(phase(&simulation), "rally");
}

#[test]
fn the_keys_move_the_paddle_until_it_reaches_a_wall() {
    let mut simulation = simulation();
    simulation.hold(Key::W, 2.0);
    assert_eq!(eval::<f64>(&simulation, "pong.left.y"), 0.0);
    simulation.hold(Key::Down, 0.5);
    let y: f64 = eval(&simulation, "pong.left.y");
    assert!((y - PLAYER_SPEED * 0.5).abs() < 1e-3, "at {y}");
}

#[test]
fn the_paddle_follows_the_mouse_until_a_key_takes_over() {
    let mut simulation = simulation();
    simulation.move_mouse(100.0, 40.0);
    simulation.step();
    assert_eq!(player_center(&simulation), 40.0);

    // The cursor stays where it was, so it doesn't take the paddle back.
    simulation.hold(Key::S, 0.25);
    simulation.step();
    let center = player_center(&simulation);
    assert!(
        (center - (40.0 + PLAYER_SPEED * 0.25)).abs() < 1e-3,
        "at {center}"
    );
}

#[test]
fn the_ball_bounces_off_the_walls_with_a_thud() {
    let mut simulation = simulation();
    play_ball(&simulation, [160.0, 10.0], [60.0, -120.0]);
    assert!(run_until(&mut simulation, 1.0, "pong.ball.vy > 0"));
    assert_eq!(sounds_playing(&mut simulation), 1);
    let [_, _, vx, _] = ball(&simulation);
    assert_eq!(vx, 60.0);
}

#[test]
fn paddles_send_the_ball_back_faster() {
    let mut simulation = simulation();
    let middle = player_center(&simulation);
    play_ball(&simulation, [60.0, middle], [-SERVE_SPEED, 0.0]);
    assert!(run_until(&mut simulation, 1.0, "pong.ball.vx > 0"));
    let [_, _, vx, vy] = ball(&simulation);
    assert!((vx.hypot(vy) - SERVE_SPEED * SPEEDUP).abs() < 1e-3);
    assert!(vy.abs() < 1e-3, "straight back from the middle");
    assert!(eval::<f64>(&simulation, "pong.shake") > 0.0);
    assert_eq!(sounds_playing(&mut simulation), 1);
}

#[test]
fn hits_further_from_the_middle_go_back_steeper() {
    let mut simulation = simulation();
    let middle = player_center(&simulation);
    play_ball(&simulation, [60.0, middle + 10.0], [-SERVE_SPEED, 0.0]);
    assert!(run_until(&mut simulation, 1.0, "pong.ball.vx > 0"));
    let [_, _, vx, vy] = ball(&simulation);
    let expected = 10.0 / ((PADDLE_HEIGHT + BALL_SIZE) / 2.0) * MAX_BOUNCE_ANGLE;
    assert!(
        (vy.atan2(vx) - expected).abs() < 1e-3,
        "downward, the way it hit"
    );
}

#[test]
fn missing_the_ball_scores_for_the_other_side() {
    let mut simulation = simulation();
    // Above the player's paddle, which waits in the middle.
    play_ball(&simulation, [100.0, 20.0], [-200.0, 0.0]);
    assert!(run_until(&mut simulation, 1.0, "pong.phase ~= 'rally'"));
    assert_eq!(score(&simulation), [0, 1]);
    assert_eq!(ball(&simulation), RESTING_BALL);
    assert_eq!(sounds_playing(&mut simulation), 1, "the chime");

    // The next serve goes to the player, who lost the point.
    simulation.run_for(SERVE_DELAY - 0.1);
    assert_eq!(phase(&simulation), "pause");
    simulation.run_for(0.2);
    assert_eq!(phase(&simulation), "rally");
    let [_, _, vx, _] = ball(&simulation);
    assert!(vx < 0.0);
}

#[test]
fn the_opponent_returns_a_ball_it_can_reach() {
    let mut simulation = simulation();
    play_ball(&simulation, [200.0, 40.0], [140.0, 50.0]);
    assert!(run_until(&mut simulation, 2.0, "pong.ball.vx < 0"));
    assert_eq!(score(&simulation), [0, 0]);
}

#[test]
fn a_fast_shot_into_a_corner_beats_the_opponent() {
    let mut simulation = simulation();
    play_ball(&simulation, [170.0, 100.0], [280.0, 155.0]);
    assert!(run_until(&mut simulation, 2.0, "pong.phase ~= 'rally'"));
    assert_eq!(score(&simulation), [1, 0]);
}

#[test]
fn the_first_to_five_wins_and_serving_starts_again() {
    let mut simulation = simulation();
    let right = WINNING_SCORE - 1;
    exec(
        &simulation,
        &format!("pong.score = {{ left = 2, right = {right} }}"),
    );
    play_ball(&simulation, [100.0, 20.0], [-200.0, 0.0]);
    assert!(run_until(&mut simulation, 1.0, "pong.phase ~= 'rally'"));
    assert_eq!(phase(&simulation), "over");
    assert_eq!(eval::<String>(&simulation, "pong.winner"), "right");

    // The match stays over until the player serves.
    simulation.run_for(SERVE_DELAY * 2.0);
    assert_eq!(phase(&simulation), "over");
    simulation.tap(Key::Enter);
    assert_eq!(phase(&simulation), "rally");
    assert_eq!(score(&simulation), [0, 0]);
}

#[test]
fn the_computer_beats_a_player_who_stands_still() {
    let mut simulation = simulation();
    simulation.tap(Key::Space);
    let over = run_until(&mut simulation, 180.0, "pong.phase == 'over'");
    assert!(over, "the match ends");
    assert_eq!(eval::<String>(&simulation, "pong.winner"), "right");
}

#[test]
fn music_plays_from_the_start() {
    let mut simulation = simulation();
    simulation.run_for(0.5);
    assert!(simulation.context().audio().music_playing());
}
