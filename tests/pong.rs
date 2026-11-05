//! Tests for the Pong example, a game written entirely in Luau, in `examples/pong`. They run it
//! headlessly and check its state, which `main.luau` keeps in the global table `pong`.

use wingine::{
    headless::Simulation,
    input::{GamepadButton, Key, MouseButton, Stick},
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
    let config = runtime::config(GAME).expect("settings load");
    Simulation::new(&config.window, |ctx| {
        LuauGame::load_with(ctx, GAME, &config)
    })
    .expect("game starts")
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
        simulation.eval(condition)
    })
}

/// Puts the ball in play, centered on `x`, `y` and moving at `vx`, `vy`.
fn play_ball(simulation: &mut Simulation<LuauGame>, [x, y]: [f64; 2], [vx, vy]: [f64; 2]) {
    simulation.exec(&format!(
        "pong.ball = {{ x = {x}, y = {y}, vx = {vx}, vy = {vy} }} pong.phase = 'rally'"
    ));
}

/// The ball's center and velocity.
fn ball(simulation: &mut Simulation<LuauGame>) -> [f64; 4] {
    let (x, y, vx, vy) = simulation.eval("pong.ball.x, pong.ball.y, pong.ball.vx, pong.ball.vy");
    [x, y, vx, vy]
}

fn phase(simulation: &mut Simulation<LuauGame>) -> String {
    simulation.eval("pong.phase")
}

/// The left and right sides' points.
fn score(simulation: &mut Simulation<LuauGame>) -> [u32; 2] {
    let (left, right) = simulation.eval("pong.score.left, pong.score.right");
    [left, right]
}

fn player_center(simulation: &mut Simulation<LuauGame>) -> f64 {
    simulation.eval::<f64>("pong.left.y") + PADDLE_HEIGHT / 2.0
}

fn sounds_playing(simulation: &mut Simulation<LuauGame>) -> usize {
    simulation.context().audio().sounds_playing()
}

#[test]
fn the_ball_waits_for_a_serve() {
    let mut simulation = simulation();
    simulation.run_for(1.0);
    assert_eq!(phase(&mut simulation), "ready");
    assert_eq!(ball(&mut simulation), RESTING_BALL);

    simulation.tap(Key::Space);
    assert_eq!(phase(&mut simulation), "rally");
    let [_, _, vx, _] = ball(&mut simulation);
    assert!(vx > 0.0, "the first serve goes to the computer");
}

#[test]
fn clicking_serves() {
    let mut simulation = simulation();
    simulation.click(MouseButton::Left, 100.0, 90.0);
    assert_eq!(phase(&mut simulation), "rally");
}

#[test]
fn the_keys_move_the_paddle_until_it_reaches_a_wall() {
    let mut simulation = simulation();
    simulation.hold(Key::W, 2.0);
    assert_eq!(simulation.eval::<f64>("pong.left.y"), 0.0);
    simulation.hold(Key::Down, 0.5);
    let y: f64 = simulation.eval("pong.left.y");
    assert!((y - PLAYER_SPEED * 0.5).abs() < 1e-3, "at {y}");
}

#[test]
fn a_gamepad_moves_the_paddle_as_far_as_its_stick_is_pushed() {
    let mut simulation = simulation();
    let gamepad = simulation.connect_gamepad("Gamepad");
    simulation.move_stick(gamepad, Stick::Left, 0.0, -1.0);
    simulation.run_for(2.0);
    assert_eq!(simulation.eval::<f64>("pong.left.y"), 0.0);

    // Past the dead zone, halfway to where it counts as pushed all the way: half speed.
    simulation.move_stick(gamepad, Stick::Left, 0.0, 0.55);
    simulation.run_for(0.5);
    let y: f64 = simulation.eval("pong.left.y");
    assert!((y - PLAYER_SPEED * 0.5 * 0.5).abs() < 1e-3, "at {y}");

    simulation.move_stick(gamepad, Stick::Left, 0.0, 0.0);
    simulation.hold_button(gamepad, GamepadButton::DPadDown, 0.5);
    let moved = simulation.eval::<f64>("pong.left.y") - y;
    assert!(
        (moved - PLAYER_SPEED * 0.5).abs() < 1e-3,
        "the D-pad moves it at full speed"
    );

    simulation.tap_button(gamepad, GamepadButton::South);
    assert_eq!(phase(&mut simulation), "rally");
}

#[test]
fn the_paddle_follows_the_mouse_until_a_key_takes_over() {
    let mut simulation = simulation();
    simulation.move_mouse(100.0, 40.0);
    simulation.step();
    assert_eq!(player_center(&mut simulation), 40.0);

    // The cursor stays where it was, so it doesn't take the paddle back.
    simulation.hold(Key::S, 0.25);
    simulation.step();
    let center = player_center(&mut simulation);
    assert!(
        (center - (40.0 + PLAYER_SPEED * 0.25)).abs() < 1e-3,
        "at {center}"
    );
}

#[test]
fn the_ball_bounces_off_the_walls_with_a_thud() {
    let mut simulation = simulation();
    play_ball(&mut simulation, [160.0, 10.0], [60.0, -120.0]);
    assert!(run_until(&mut simulation, 1.0, "pong.ball.vy > 0"));
    assert_eq!(sounds_playing(&mut simulation), 1);
    let [_, _, vx, _] = ball(&mut simulation);
    assert_eq!(vx, 60.0);
}

#[test]
fn paddles_send_the_ball_back_faster() {
    let mut simulation = simulation();
    let middle = player_center(&mut simulation);
    play_ball(&mut simulation, [60.0, middle], [-SERVE_SPEED, 0.0]);
    assert!(run_until(&mut simulation, 1.0, "pong.ball.vx > 0"));
    let [_, _, vx, vy] = ball(&mut simulation);
    assert!((vx.hypot(vy) - SERVE_SPEED * SPEEDUP).abs() < 1e-3);
    assert!(vy.abs() < 1e-3, "straight back from the middle");
    assert!(simulation.eval::<f64>("pong.shake") > 0.0);
    assert_eq!(sounds_playing(&mut simulation), 1);
}

#[test]
fn hits_further_from_the_middle_go_back_steeper() {
    let mut simulation = simulation();
    let middle = player_center(&mut simulation);
    play_ball(&mut simulation, [60.0, middle + 10.0], [-SERVE_SPEED, 0.0]);
    assert!(run_until(&mut simulation, 1.0, "pong.ball.vx > 0"));
    let [_, _, vx, vy] = ball(&mut simulation);
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
    play_ball(&mut simulation, [100.0, 20.0], [-200.0, 0.0]);
    assert!(run_until(&mut simulation, 1.0, "pong.phase ~= 'rally'"));
    assert_eq!(score(&mut simulation), [0, 1]);
    assert_eq!(ball(&mut simulation), RESTING_BALL);
    assert_eq!(sounds_playing(&mut simulation), 1, "the chime");

    // The next serve goes to the player, who lost the point.
    simulation.run_for(SERVE_DELAY - 0.1);
    assert_eq!(phase(&mut simulation), "pause");
    simulation.run_for(0.2);
    assert_eq!(phase(&mut simulation), "rally");
    let [_, _, vx, _] = ball(&mut simulation);
    assert!(vx < 0.0);
}

#[test]
fn the_opponent_returns_a_ball_it_can_reach() {
    let mut simulation = simulation();
    play_ball(&mut simulation, [200.0, 40.0], [140.0, 50.0]);
    assert!(run_until(&mut simulation, 2.0, "pong.ball.vx < 0"));
    assert_eq!(score(&mut simulation), [0, 0]);
}

#[test]
fn a_fast_shot_into_a_corner_beats_the_opponent() {
    let mut simulation = simulation();
    play_ball(&mut simulation, [170.0, 100.0], [280.0, 155.0]);
    assert!(run_until(&mut simulation, 2.0, "pong.phase ~= 'rally'"));
    assert_eq!(score(&mut simulation), [1, 0]);
}

#[test]
fn the_first_to_five_wins_and_serving_starts_again() {
    let mut simulation = simulation();
    let right = WINNING_SCORE - 1;
    simulation.exec(&format!("pong.score = {{ left = 2, right = {right} }}"));
    play_ball(&mut simulation, [100.0, 20.0], [-200.0, 0.0]);
    assert!(run_until(&mut simulation, 1.0, "pong.phase ~= 'rally'"));
    assert_eq!(phase(&mut simulation), "over");
    assert_eq!(simulation.eval::<String>("pong.winner"), "right");

    // The match stays over until the player serves.
    simulation.run_for(SERVE_DELAY * 2.0);
    assert_eq!(phase(&mut simulation), "over");
    simulation.tap(Key::Enter);
    assert_eq!(phase(&mut simulation), "rally");
    assert_eq!(score(&mut simulation), [0, 0]);
}

#[test]
fn the_computer_beats_a_player_who_stands_still() {
    let mut simulation = simulation();
    simulation.tap(Key::Space);
    let over = run_until(&mut simulation, 180.0, "pong.phase == 'over'");
    assert!(over, "the match ends");
    assert_eq!(simulation.eval::<String>("pong.winner"), "right");
}

/// A whole match, with the player's paddle chasing the ball with the keys, plays out the same to
/// the bit on every platform: the ball's angles come from `exact`, and the rest is arithmetic
/// that IEEE 754 makes exact. The match is hashed frame by frame, and CI checks the hash on
/// Windows and Linux.
#[test]
fn a_match_plays_out_the_same_on_every_platform() {
    let mut simulation = simulation();
    simulation.tap(Key::Space);
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut held = [false, false];
    let mut frames = 0;
    while phase(&mut simulation) != "over" {
        assert!(frames < 60 * 600, "the match ends");
        let (ball_y, vx, paddle_y): (f64, f64, f64) =
            simulation.eval("pong.ball.y, pong.ball.vx, pong.left.y");
        // Chasing the ball as it comes, but lazily, so both sides win points.
        let center = paddle_y + PADDLE_HEIGHT / 2.0;
        let chase = [
            vx < 0.0 && ball_y < center - 10.0,
            vx < 0.0 && ball_y > center + 10.0,
        ];
        for ((key, held), chase) in [Key::Up, Key::Down].into_iter().zip(&mut held).zip(chase) {
            if chase != *held {
                if chase {
                    simulation.key_down(key);
                } else {
                    simulation.key_up(key);
                }
                *held = chase;
            }
        }
        simulation.step();
        frames += 1;
        let state: [f64; 6] = simulation
            .eval::<(f64, f64, f64, f64, f64, f64)>(
                "pong.ball.x, pong.ball.y, pong.ball.vx, pong.ball.vy, pong.left.y, pong.right.y",
            )
            .into();
        for value in state {
            hash = (hash ^ value.to_bits()).wrapping_mul(0x100_0000_01b3);
        }
    }
    // With `math.cos` and `math.sin`, the match comes out the same here, but not to the bit: the
    // hash differs between Windows and Linux. A change to how Pong plays changes these: pin
    // the new ones once CI shows them the same on both.
    assert_eq!(
        (frames, score(&mut simulation), hash),
        (15452, [5, 2], 0x1049_db71_e255_e7d3),
        "the match played differently"
    );
}

#[test]
fn music_plays_from_the_start() {
    let mut simulation = simulation();
    simulation.run_for(0.5);
    assert!(simulation.context().audio().music_playing());
}
