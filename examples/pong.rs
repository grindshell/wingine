//! Pong against a computer opponent, at a fixed 320x180 resolution. Move your paddle, on the left,
//! with W and S or the arrow keys, or with the mouse, and press Space or click to serve. The first
//! to 5 points wins.
//!
//! The opponent is scripted in Luau, in `examples/pong.luau`: a few times a second it looks at the
//! ball and heads for where it will cross. Hits blip, higher as a rally speeds up, and shake the
//! field; walls thud and points chime, each panned to where it happened, and music loops in the
//! background.
//!
//! Run with `cargo run --example pong`. It borrows the sprites example's music and sounds, in
//! `examples/sounds`.

use std::{cell::RefCell, rc::Rc};

use wingine::{
    Context, Game, WindowConfig,
    audio::{Audio, PlaySettings, Sound},
    graphics::{Camera, Canvas, Color, Font, Resolution},
    input::{Input, Key, MouseButton},
    lua::Scripts,
    math::Rect,
};

const SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/pong.luau");
const SOUNDS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/sounds/");
const MUSIC_VOLUME: f32 = 0.4;
/// How far sounds pan toward the side of the field they happen on: 1 is all the way.
const STEREO_WIDTH: f32 = 0.7;

const WIDTH: f32 = 320.0;
const HEIGHT: f32 = 180.0;
/// Scales by whole numbers to fill 720p, 1080p and 1440p screens.
const RESOLUTION: Resolution = Resolution::Fixed {
    width: WIDTH as u32,
    height: HEIGHT as u32,
};

const PADDLE_WIDTH: f32 = 4.0;
const PADDLE_HEIGHT: f32 = 24.0;
/// The gap between each paddle and its end of the field.
const PADDLE_MARGIN: f32 = 8.0;
/// Pixels per second the player's paddle moves with the keys.
const PLAYER_SPEED: f32 = 160.0;
/// Pixels per second the opponent's paddle moves: slower than the player's, to be fair.
const OPPONENT_SPEED: f32 = 110.0;

const BALL_SIZE: f32 = 4.0;
/// Pixels per second the ball moves when it's served.
const SERVE_SPEED: f32 = 120.0;
/// Each hit speeds the ball up by this factor, up to `MAX_SPEED`.
const SPEEDUP: f32 = 1.08;
const MAX_SPEED: f32 = 320.0;
/// The steepest angle a paddle sends the ball back at, when it hits the paddle's very end.
const MAX_BOUNCE_ANGLE: f32 = std::f32::consts::FRAC_PI_3;
/// The angles serves leave at, in turn, in radians from straight across. Positive is downward.
const SERVE_ANGLES: [f32; 5] = [0.35, -0.2, 0.5, -0.45, 0.1];
/// The furthest the ball moves between collision checks, so it can't pass through a paddle.
const MAX_STEP: f32 = 1.0;

const WINNING_SCORE: u32 = 5;
/// Seconds between a point and the next serve.
const SERVE_DELAY: f32 = 1.0;
/// Seconds the field shakes after a hit, and how far it moves at first.
const SHAKE_SECONDS: f32 = 0.15;
const SHAKE_PIXELS: f32 = 2.0;
/// The dashes of the halfway line.
const DASH_LENGTH: f32 = 6.0;
const DASH_SPACING: f32 = 12.0;
const MESSAGE_PADDING: f32 = 8.0;

const BACKGROUND: Color = Color::rgb8(0x14, 0x18, 0x2c);
const LINE: Color = Color::rgb8(0x3a, 0x40, 0x5c);
const PADDLE: Color = Color::rgb8(0xf4, 0xf0, 0xe8);
const BALL: Color = Color::rgb8(0xf2, 0xc1, 0x4e);
const SCORE: Color = Color::rgb8(0x8a, 0x92, 0xb8);
const PANEL: Color = Color::rgba(0.0, 0.0, 0.0, 0.85);
const TEXT: Color = Color::rgb8(0xf4, 0xf0, 0xe8);

struct Pong {
    font: Font,
    headline_font: Font,
    score_font: Font,
    /// The player's paddle.
    left: Paddle,
    /// The opponent's paddle, which the script steers.
    right: Paddle,
    ball: Ball,
    /// Points for the left and right sides.
    score: [u32; 2],
    phase: Phase,
    /// The side the next serve goes toward.
    serve_toward: Side,
    /// Serves so far, which picks the angle of the next.
    serves: usize,
    /// Whether the player's paddle follows the cursor: from when the mouse moves until a key is
    /// held.
    mouse_control: bool,
    /// The cursor's position the frame before, to tell when it moves.
    last_mouse: Option<[f32; 2]>,
    shared: Rc<RefCell<Shared>>,
    scripts: Scripts,
    sounds: Sounds,
    /// Seconds left of the field shaking.
    shake: f32,
    time: f32,
}

struct Sounds {
    hit: Sound,
    wall: Sound,
    point: Sound,
}

/// What the opponent's script sees of the game, and where it wants its paddle, through the
/// functions `register_api` gives it.
struct Shared {
    ball: Ball,
    paddle: Paddle,
    /// Where the script is moving the paddle's center to.
    aim: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    /// Waiting for the player to serve, before the first point.
    Ready,
    /// The ball is in play.
    Rally,
    /// After a point, until the next serve: the seconds left.
    Pause(f32),
    /// The match is over, won by a side. Serving starts the next.
    Over(Side),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

impl Side {
    fn opposite(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
        }
    }

    /// The way along `x` the ball moves to reach this side: -1 or 1.
    fn direction(self) -> f32 {
        match self {
            Self::Left => -1.0,
            Self::Right => 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Paddle {
    /// The left edge.
    x: f32,
    /// The top edge.
    y: f32,
}

impl Paddle {
    /// Halfway down `side` of the field.
    fn new(side: Side) -> Self {
        let x = match side {
            Side::Left => PADDLE_MARGIN,
            Side::Right => WIDTH - PADDLE_MARGIN - PADDLE_WIDTH,
        };
        Self {
            x,
            y: (HEIGHT - PADDLE_HEIGHT) / 2.0,
        }
    }

    fn rect(&self) -> Rect {
        Rect::new(self.x, self.y, PADDLE_WIDTH, PADDLE_HEIGHT)
    }

    fn center(&self) -> f32 {
        self.y + PADDLE_HEIGHT / 2.0
    }

    /// Moves down by `dy`, or up if it's negative, staying on the field.
    fn move_by(&mut self, dy: f32) {
        self.y = (self.y + dy).clamp(0.0, HEIGHT - PADDLE_HEIGHT);
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Ball {
    /// The top-left corner.
    position: [f32; 2],
    /// Pixels per second.
    velocity: [f32; 2],
}

impl Ball {
    /// At rest in the middle of the field.
    fn new() -> Self {
        Self::at([WIDTH / 2.0, HEIGHT / 2.0], [0.0, 0.0])
    }

    /// Centered on `center`, moving at `velocity`.
    fn at(center: [f32; 2], velocity: [f32; 2]) -> Self {
        Self {
            position: center.map(|c| c - BALL_SIZE / 2.0),
            velocity,
        }
    }

    fn center(&self) -> [f32; 2] {
        self.position.map(|p| p + BALL_SIZE / 2.0)
    }

    fn rect(&self) -> Rect {
        let [x, y] = self.position;
        Rect::new(x, y, BALL_SIZE, BALL_SIZE)
    }

    fn speed(&self) -> f32 {
        self.velocity[0].hypot(self.velocity[1])
    }

    /// Whether the ball is hitting the paddle on `side`, moving toward it. A ball that got behind
    /// the paddle is past saving.
    fn hits(&self, paddle: &Paddle, side: Side) -> bool {
        let (rect, paddle_rect) = (self.rect(), paddle.rect());
        let in_front = match side {
            Side::Left => rect.x >= paddle_rect.x,
            Side::Right => rect.right() <= paddle_rect.right(),
        };
        let toward = self.velocity[0] * side.direction() > 0.0;
        toward && in_front && rect.overlaps(&paddle_rect)
    }

    /// Sends the ball back from the paddle on `side`: faster, and steeper the further from the
    /// paddle's middle it hit.
    fn bounce_off(&mut self, paddle: &Paddle, side: Side) {
        let reach = (PADDLE_HEIGHT + BALL_SIZE) / 2.0;
        let offset = ((self.center()[1] - paddle.center()) / reach).clamp(-1.0, 1.0);
        let angle = offset * MAX_BOUNCE_ANGLE;
        let speed = (self.speed() * SPEEDUP).min(MAX_SPEED);
        let away = side.opposite().direction();
        self.velocity = [away * angle.cos(), angle.sin()].map(|v| v * speed);
        // In front of the paddle, so it doesn't hit again.
        self.position[0] = match side {
            Side::Left => paddle.rect().right(),
            Side::Right => paddle.x - BALL_SIZE,
        };
    }
}

impl Pong {
    fn new(ctx: &mut Context) -> wingine::Result<Self> {
        let input = ctx.input_mut();
        input.bind("up", [Key::W, Key::Up]);
        input.bind("down", [Key::S, Key::Down]);
        input.bind("serve", [Key::Space, Key::Enter]);

        let sound = |name: &str| Sound::load(format!("{SOUNDS}{name}.wav"));
        let sounds = Sounds {
            hit: sound("blip")?,
            wall: sound("bump")?,
            point: sound("chime")?,
        };

        let (ball, right) = (Ball::new(), Paddle::new(Side::Right));
        let shared = Rc::new(RefCell::new(Shared {
            ball,
            paddle: right,
            aim: right.center(),
        }));
        let mut scripts = Scripts::new()?;
        register_api(&scripts, &shared)?;
        scripts.load(SCRIPT)?;
        scripts.spawn("opponent", ())?;

        let audio = ctx.audio();
        audio.set_music_volume(MUSIC_VOLUME);
        audio.play_music(format!("{SOUNDS}music.ogg"), 1.0)?;

        let font = ctx.default_font().clone();
        Ok(Self {
            headline_font: font.scaled(2.0),
            score_font: font.scaled(3.0),
            font,
            left: Paddle::new(Side::Left),
            right,
            ball,
            score: [0, 0],
            phase: Phase::Ready,
            // The first serve goes to the computer, so the player can see how it plays.
            serve_toward: Side::Right,
            serves: 0,
            mouse_control: false,
            last_mouse: None,
            shared,
            scripts,
            sounds,
            shake: 0.0,
            time: 0.0,
        })
    }

    /// Puts the ball in play from the middle of the field, toward `serve_toward`.
    fn serve(&mut self) {
        let angle = SERVE_ANGLES[self.serves % SERVE_ANGLES.len()];
        self.serves += 1;
        let direction = self.serve_toward.direction();
        let velocity = [direction * angle.cos(), angle.sin()].map(|v| v * SERVE_SPEED);
        self.ball = Ball::at([WIDTH / 2.0, HEIGHT / 2.0], velocity);
        self.phase = Phase::Rally;
    }

    /// Scores a point for `scorer`. The next serve goes to the side that lost it.
    fn point(&mut self, scorer: Side, audio: &mut Audio) {
        audio.play_with(&self.sounds.point, panned(self.ball.center()[0]));
        self.score[scorer as usize] += 1;
        self.ball = Ball::new();
        self.serve_toward = scorer.opposite();
        self.phase = if self.score[scorer as usize] >= WINNING_SCORE {
            Phase::Over(scorer)
        } else {
            Phase::Pause(SERVE_DELAY)
        };
    }

    /// Moves the player's paddle with the keys, or to the cursor once the mouse moves.
    fn move_player(&mut self, input: &Input, dt: f32) {
        let mut direction = 0.0;
        if input.held("up") {
            direction -= 1.0;
        }
        if input.held("down") {
            direction += 1.0;
        }
        let mouse = input.mouse_position();
        if direction != 0.0 {
            self.mouse_control = false;
        } else if mouse.is_some() && mouse != self.last_mouse {
            self.mouse_control = true;
        }
        self.last_mouse = mouse;
        match mouse {
            Some([_, y]) if self.mouse_control => self.left.move_by(y - self.left.center()),
            _ => self.left.move_by(direction * PLAYER_SPEED * dt),
        }
    }

    /// Lets the opponent's script see the ball, then moves its paddle where the script aims.
    fn run_opponent(&mut self, dt: f32) {
        {
            let mut shared = self.shared.borrow_mut();
            shared.ball = self.ball;
            shared.paddle = self.right;
        }
        if let Err(err) = self.scripts.update(dt) {
            log::error!("{err}");
        }
        let aim = self.shared.borrow().aim;
        let most = OPPONENT_SPEED * dt;
        self.right
            .move_by((aim - self.right.center()).clamp(-most, most));
    }

    /// Moves the ball on by `dt` seconds, bouncing it off the walls and paddles. Returns the side
    /// that scored, if it left the field.
    fn advance_ball(&mut self, audio: &mut Audio, dt: f32) -> Option<Side> {
        let Self {
            ball,
            left,
            right,
            sounds,
            shake,
            ..
        } = self;
        // Short steps, so a fast ball can't pass through a paddle between frames.
        let steps = (ball.speed() * dt / MAX_STEP).ceil().max(1.0);
        let dt = dt / steps;
        for _ in 0..steps as u32 {
            ball.position = [0, 1].map(|i| ball.position[i] + ball.velocity[i] * dt);
            let rect = ball.rect();
            let vy = ball.velocity[1];
            if (rect.y < 0.0 && vy < 0.0) || (rect.bottom() > HEIGHT && vy > 0.0) {
                ball.velocity[1] = -vy;
                audio.play_with(&sounds.wall, panned(rect.x));
            }
            for (side, paddle) in [(Side::Left, &*left), (Side::Right, &*right)] {
                if ball.hits(paddle, side) {
                    ball.bounce_off(paddle, side);
                    *shake = SHAKE_SECONDS;
                    let settings = PlaySettings {
                        // Higher as the rally speeds up.
                        speed: (ball.speed() / SERVE_SPEED).sqrt(),
                        ..panned(paddle.x)
                    };
                    audio.play_with(&sounds.hit, settings);
                }
            }
            if rect.right() < 0.0 {
                return Some(Side::Right);
            }
            if rect.x > WIDTH {
                return Some(Side::Left);
            }
        }
        None
    }

    /// The view of the field, which shakes for a moment after each hit.
    fn camera(&self) -> Camera {
        let strength = SHAKE_PIXELS * self.shake / SHAKE_SECONDS;
        let [dx, dy] = [(self.time * 97.0).sin(), (self.time * 61.0).cos()].map(|v| v * strength);
        Camera::new(WIDTH / 2.0 + dx, HEIGHT / 2.0 + dy)
    }

    /// The headline and prompt to show over the field, if any.
    fn message(&self) -> Option<(&'static str, &'static str)> {
        match self.phase {
            Phase::Ready => Some(("PONG", "Space or click to serve")),
            Phase::Over(Side::Left) => Some(("You win!", "Space or click to play again")),
            Phase::Over(Side::Right) => Some(("The computer wins", "Space or click to play again")),
            Phase::Rally | Phase::Pause(_) => None,
        }
    }
}

impl Game for Pong {
    fn update(&mut self, ctx: &mut Context, dt: f32) {
        self.time += dt;
        self.shake = (self.shake - dt).max(0.0);
        self.run_opponent(dt);

        let input = ctx.input();
        self.move_player(input, dt);
        let serve = input.pressed("serve") || input.mouse_pressed(MouseButton::Left);
        let audio = ctx.audio();
        match self.phase {
            Phase::Ready if serve => self.serve(),
            Phase::Over(_) if serve => {
                self.score = [0, 0];
                self.serve();
            }
            Phase::Pause(time_left) if time_left > dt => self.phase = Phase::Pause(time_left - dt),
            Phase::Pause(_) => self.serve(),
            Phase::Rally => {
                if let Some(scorer) = self.advance_ball(audio, dt) {
                    self.point(scorer, audio);
                }
            }
            Phase::Ready | Phase::Over(_) => {}
        }
    }

    fn draw(&mut self, canvas: &mut Canvas) {
        canvas.clear(BACKGROUND);
        // Hits shake the field, but not the score and messages drawn over it.
        canvas.set_camera(Some(self.camera()));
        let dashes = (HEIGHT / DASH_SPACING).ceil() as u32;
        for i in 0..dashes {
            let y = i as f32 * DASH_SPACING + (DASH_SPACING - DASH_LENGTH) / 2.0;
            canvas.fill_rect(Rect::new(WIDTH / 2.0 - 1.0, y, 2.0, DASH_LENGTH), LINE);
        }
        canvas.fill_rect(self.left.rect(), PADDLE);
        canvas.fill_rect(self.right.rect(), PADDLE);
        // Between matches, the message takes the ball's place.
        if matches!(self.phase, Phase::Rally | Phase::Pause(_)) {
            canvas.fill_rect(self.ball.rect(), BALL);
        }

        canvas.set_camera(None);
        for (side, x) in [(Side::Left, WIDTH / 4.0), (Side::Right, WIDTH * 0.75)] {
            let score = self.score[side as usize].to_string();
            draw_centered(canvas, &self.score_font, &score, [x, 8.0], SCORE);
        }
        if let Some((headline, prompt)) = self.message() {
            let [headline_width, headline_height] = self.headline_font.measure(headline);
            let [prompt_width, prompt_height] = self.font.measure(prompt);
            let width = headline_width.max(prompt_width) + MESSAGE_PADDING * 2.0;
            let height = headline_height + prompt_height + MESSAGE_PADDING * 2.0;
            let [x, y] = [(WIDTH - width) / 2.0, (HEIGHT - height) / 2.0].map(f32::round);
            canvas.fill_rect(Rect::new(x, y, width, height), PANEL);
            let top = y + MESSAGE_PADDING;
            draw_centered(
                canvas,
                &self.headline_font,
                headline,
                [WIDTH / 2.0, top],
                TEXT,
            );
            let below = [WIDTH / 2.0, top + headline_height];
            draw_centered(canvas, &self.font, prompt, below, TEXT);
        }
    }
}

/// Draws `text` centered on `x`, with its top at `y`.
fn draw_centered(canvas: &mut Canvas, font: &Font, text: &str, [x, y]: [f32; 2], color: Color) {
    let [width, _] = font.measure(text);
    canvas.draw_text(font, text, [x - width / 2.0, y], color);
}

/// Plays a sound panned toward `x` on the field.
fn panned(x: f32) -> PlaySettings {
    PlaySettings {
        pan: ((x / WIDTH).clamp(0.0, 1.0) * 2.0 - 1.0) * STEREO_WIDTH,
        ..Default::default()
    }
}

/// Gives the opponent's script the game's functions, which see and steer it through `shared`.
/// They're documented for script authors at the top of `pong.luau`.
fn register_api(scripts: &Scripts, shared: &Rc<RefCell<Shared>>) -> wingine::Result<()> {
    let state = Rc::clone(shared);
    scripts.set_function("ball", move |_, ()| {
        let ball = state.borrow().ball;
        let ([x, y], [vx, vy]) = (ball.center(), ball.velocity);
        Ok((x, y, vx, vy))
    })?;
    let state = Rc::clone(shared);
    scripts.set_function("paddle", move |_, ()| {
        let paddle = state.borrow().paddle;
        Ok((paddle.x, paddle.center()))
    })?;
    scripts.set_function("field", |_, ()| Ok((WIDTH, HEIGHT)))?;
    let state = Rc::clone(shared);
    scripts.set_function("aim", move |_, y: f32| {
        state.borrow_mut().aim = y;
        Ok(())
    })
}

fn config() -> WindowConfig {
    WindowConfig {
        title: "Wingine - pong".into(),
        resolution: RESOLUTION,
        ..Default::default()
    }
}

fn main() -> wingine::Result<()> {
    env_logger::init();
    wingine::run(config(), Pong::new)
}

/// Drives the game with simulated input, without a window.
#[cfg(test)]
mod tests {
    use wingine::headless::Simulation;

    use super::*;

    fn simulation() -> Simulation<Pong> {
        Simulation::new(&config(), Pong::new).expect("game starts")
    }

    /// Runs frames until `done` is true of the game, for up to `seconds`. Returns whether it
    /// became true.
    fn run_until(
        simulation: &mut Simulation<Pong>,
        seconds: f32,
        done: impl Fn(&Pong) -> bool,
    ) -> bool {
        let frames = (seconds / Simulation::<Pong>::FRAME_TIME).round() as u32;
        (0..frames).any(|_| {
            simulation.step();
            done(simulation.game())
        })
    }

    /// Puts the ball in play, centered on `center` and moving at `velocity`.
    fn play_ball(simulation: &mut Simulation<Pong>, center: [f32; 2], velocity: [f32; 2]) {
        let pong = simulation.game_mut();
        pong.ball = Ball::at(center, velocity);
        pong.phase = Phase::Rally;
    }

    fn sounds_playing(simulation: &mut Simulation<Pong>) -> usize {
        simulation.context().audio().sounds_playing()
    }

    #[test]
    fn the_ball_waits_for_a_serve() {
        let mut simulation = simulation();
        simulation.run_for(1.0);
        assert_eq!(simulation.game().phase, Phase::Ready);
        assert_eq!(simulation.game().ball, Ball::new());

        simulation.tap(Key::Space);
        assert_eq!(simulation.game().phase, Phase::Rally);
        let [vx, _] = simulation.game().ball.velocity;
        assert!(vx > 0.0, "the first serve goes to the computer");
    }

    #[test]
    fn clicking_serves() {
        let mut simulation = simulation();
        simulation.click(MouseButton::Left, 100.0, 90.0);
        assert_eq!(simulation.game().phase, Phase::Rally);
    }

    #[test]
    fn the_keys_move_the_paddle_until_it_reaches_a_wall() {
        let mut simulation = simulation();
        simulation.hold(Key::W, 2.0);
        assert_eq!(simulation.game().left.y, 0.0);
        simulation.hold(Key::Down, 0.5);
        let y = simulation.game().left.y;
        assert!((y - PLAYER_SPEED * 0.5).abs() < 1e-3, "at {y}");
    }

    #[test]
    fn the_paddle_follows_the_mouse_until_a_key_takes_over() {
        let mut simulation = simulation();
        simulation.move_mouse(100.0, 40.0);
        simulation.step();
        assert_eq!(simulation.game().left.center(), 40.0);

        // The cursor stays where it was, so it doesn't take the paddle back.
        simulation.hold(Key::S, 0.25);
        simulation.step();
        let center = simulation.game().left.center();
        assert!(
            (center - (40.0 + PLAYER_SPEED * 0.25)).abs() < 1e-3,
            "at {center}"
        );
    }

    #[test]
    fn the_ball_bounces_off_the_walls_with_a_thud() {
        let mut simulation = simulation();
        play_ball(&mut simulation, [160.0, 10.0], [60.0, -120.0]);
        assert!(run_until(&mut simulation, 1.0, |pong| pong.ball.velocity
            [1]
            > 0.0));
        assert_eq!(sounds_playing(&mut simulation), 1);
        assert_eq!(simulation.game().ball.velocity[0], 60.0);
    }

    #[test]
    fn paddles_send_the_ball_back_faster() {
        let mut simulation = simulation();
        let middle = simulation.game().left.center();
        play_ball(&mut simulation, [60.0, middle], [-SERVE_SPEED, 0.0]);
        assert!(run_until(&mut simulation, 1.0, |pong| pong.ball.velocity
            [0]
            > 0.0));
        let pong = simulation.game();
        assert!((pong.ball.speed() - SERVE_SPEED * SPEEDUP).abs() < 1e-3);
        assert!(
            pong.ball.velocity[1].abs() < 1e-3,
            "straight back from the middle"
        );
        assert!(pong.shake > 0.0);
        assert_eq!(sounds_playing(&mut simulation), 1);
    }

    #[test]
    fn hits_further_from_the_middle_go_back_steeper() {
        let mut simulation = simulation();
        let middle = simulation.game().left.center();
        play_ball(&mut simulation, [60.0, middle + 10.0], [-SERVE_SPEED, 0.0]);
        assert!(run_until(&mut simulation, 1.0, |pong| pong.ball.velocity
            [0]
            > 0.0));
        let [vx, vy] = simulation.game().ball.velocity;
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
        assert!(run_until(&mut simulation, 1.0, |pong| pong.phase != Phase::Rally));
        assert_eq!(simulation.game().score, [0, 1]);
        assert_eq!(simulation.game().ball, Ball::new());
        assert_eq!(sounds_playing(&mut simulation), 1, "the chime");

        // The next serve goes to the player, who lost the point.
        simulation.run_for(SERVE_DELAY - 0.1);
        assert!(matches!(simulation.game().phase, Phase::Pause(_)));
        simulation.run_for(0.2);
        assert_eq!(simulation.game().phase, Phase::Rally);
        assert!(simulation.game().ball.velocity[0] < 0.0);
    }

    #[test]
    fn the_opponent_returns_a_ball_it_can_reach() {
        let mut simulation = simulation();
        play_ball(&mut simulation, [200.0, 40.0], [140.0, 50.0]);
        assert!(run_until(&mut simulation, 2.0, |pong| pong.ball.velocity
            [0]
            < 0.0));
        assert_eq!(simulation.game().score, [0, 0]);
    }

    #[test]
    fn a_fast_shot_into_a_corner_beats_the_opponent() {
        let mut simulation = simulation();
        play_ball(&mut simulation, [170.0, 100.0], [280.0, 155.0]);
        assert!(run_until(&mut simulation, 2.0, |pong| pong.phase != Phase::Rally));
        assert_eq!(simulation.game().score, [1, 0]);
    }

    #[test]
    fn the_first_to_five_wins_and_serving_starts_again() {
        let mut simulation = simulation();
        simulation.game_mut().score = [2, WINNING_SCORE - 1];
        play_ball(&mut simulation, [100.0, 20.0], [-200.0, 0.0]);
        assert!(run_until(&mut simulation, 1.0, |pong| pong.phase != Phase::Rally));
        assert_eq!(simulation.game().phase, Phase::Over(Side::Right));

        // The match stays over until the player serves.
        simulation.run_for(SERVE_DELAY * 2.0);
        assert_eq!(simulation.game().phase, Phase::Over(Side::Right));
        simulation.tap(Key::Enter);
        assert_eq!(simulation.game().phase, Phase::Rally);
        assert_eq!(simulation.game().score, [0, 0]);
    }

    #[test]
    fn the_computer_beats_a_player_who_stands_still() {
        let mut simulation = simulation();
        simulation.tap(Key::Space);
        let over = |pong: &Pong| matches!(pong.phase, Phase::Over(_));
        assert!(run_until(&mut simulation, 180.0, over), "the match ends");
        assert_eq!(simulation.game().phase, Phase::Over(Side::Right));
    }

    #[test]
    fn music_plays_from_the_start() {
        let mut simulation = simulation();
        simulation.run_for(0.5);
        assert!(simulation.context().audio().music_playing());
    }
}
