//! Tests for running games headlessly with simulated input: `Simulation` and scripted runs.

use std::{cell::RefCell, fs, path::Path, rc::Rc};

use wingine::{
    Context, Error, FrameStats, Game, Redraw, WindowConfig,
    graphics::{Canvas, Color, Resolution},
    headless::Simulation,
    input::{GamepadButton, Key, MouseButton},
    math::Rect,
};

/// Records what the game sees of its input each frame.
#[derive(Default)]
struct Probe {
    updates: u32,
    space_presses: u32,
    space_held_frames: u32,
    clicks: Vec<[f32; 2]>,
    canvas_size: [f32; 2],
}

impl Game for Probe {
    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        self.updates += 1;
        let input = ctx.input();
        self.space_presses += u32::from(input.key_pressed(Key::Space));
        self.space_held_frames += u32::from(input.key_held(Key::Space));
        if input.mouse_pressed(MouseButton::Left) {
            self.clicks.push(
                input
                    .mouse_position()
                    .expect("the cursor is over the window"),
            );
        }
        self.canvas_size = ctx.canvas_size();
    }

    fn draw(&mut self, canvas: &mut Canvas) {
        canvas.clear(Color::rgb8(0x20, 0x20, 0x30));
        for [x, y] in &self.clicks {
            canvas.fill_rect(Rect::new(*x, *y, 2.0, 2.0), Color::WHITE);
        }
    }
}

/// A 32x16 canvas in a 100x60 window: 3x, with bars.
fn config() -> WindowConfig {
    WindowConfig {
        width: 100,
        height: 60,
        resolution: Resolution::Fixed {
            width: 32,
            height: 16,
        },
        ..Default::default()
    }
}

fn simulation() -> Simulation<Probe> {
    Simulation::new(&config(), |_| Ok(Probe::default())).expect("a GPU adapter is available")
}

#[test]
fn a_tap_is_pressed_once_and_held_for_one_frame() {
    let mut simulation = simulation();
    simulation.tap(Key::Space);
    simulation.run_frames(3);
    let probe = simulation.game();
    assert_eq!(probe.updates, 4);
    assert_eq!(simulation.frame_count(), 4);
    assert_eq!(probe.space_presses, 1);
    assert_eq!(probe.space_held_frames, 1);
}

/// A game that asks for a frame when told to, as one that redraws on demand does.
#[derive(Default)]
struct Asking {
    /// Seconds from now to ask for a frame at, in the next update.
    ask: Option<f32>,
}

impl Game for Asking {
    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        match self.ask.take() {
            Some(0.0) => ctx.request_redraw(),
            Some(seconds) => ctx.request_redraw_after(seconds),
            None => {}
        }
    }

    fn draw(&mut self, _canvas: &mut Canvas) {}
}

#[test]
fn frames_on_demand_are_wanted_after_input_and_when_asked_for() {
    let config = WindowConfig {
        redraw: Redraw::OnDemand,
        ..config()
    };
    let mut simulation = Simulation::new(&config, |_| Ok(Asking::default())).unwrap();
    // The first frames start egui, which asks for a few more.
    for _ in 0..3 {
        simulation.step();
    }
    assert_eq!(simulation.next_frame_in(), None);

    // Input wants a frame, which the frame it gets satisfies.
    simulation.key_down(Key::Space);
    assert_eq!(simulation.next_frame_in(), Some(0.0));
    simulation.step();
    assert_eq!(simulation.next_frame_in(), None);
    simulation.key_up(Key::Space);
    simulation.move_mouse(4.0, 4.0);
    assert_eq!(simulation.next_frame_in(), Some(0.0));
    simulation.step();

    // The game asks for the next frame, or one later.
    simulation.game_mut().ask = Some(0.0);
    simulation.step();
    assert_eq!(simulation.next_frame_in(), Some(0.0));
    simulation.step();
    assert_eq!(simulation.next_frame_in(), None);
    simulation.game_mut().ask = Some(1.0);
    simulation.step();
    let wait = simulation.next_frame_in().expect("a frame is wanted");
    assert!(0.5 < wait && wait <= 1.0, "{wait}");

    // Another thread asks with a waker.
    simulation.step();
    let waker = simulation.context().waker();
    std::thread::spawn(move || waker.wake()).join().unwrap();
    assert_eq!(simulation.next_frame_in(), Some(0.0));
    simulation.step();
    assert_eq!(simulation.next_frame_in(), None);
}

#[test]
fn a_hold_lasts_its_duration() {
    let mut simulation = simulation();
    simulation.hold(Key::Space, 0.5);
    simulation.run_frames(2);
    assert_eq!(simulation.game().space_presses, 1);
    assert_eq!(simulation.game().space_held_frames, 30);
}

#[test]
fn an_update_rate_sets_how_often_frames_update() {
    let config = WindowConfig {
        update_rate: Some(30.0),
        ..config()
    };
    let mut simulation = Simulation::new(&config, |_| Ok(Probe::default())).unwrap();
    // The tap's frame has no update, so the next update sees it.
    simulation.tap(Key::Space);
    simulation.run_frames(5);
    assert_eq!(simulation.game().updates, 3);
    assert_eq!(simulation.game().space_presses, 1);

    let config = WindowConfig {
        update_rate: Some(120.0),
        ..config
    };
    let mut simulation = Simulation::new(&config, |_| Ok(Probe::default())).unwrap();
    simulation.hold_frames(Key::Space, 3);
    assert_eq!(simulation.game().updates, 6);
    assert_eq!(simulation.game().space_presses, 1);
    assert_eq!(simulation.game().space_held_frames, 6);
}

#[test]
fn clicks_are_in_canvas_pixels() {
    let mut simulation = simulation();
    simulation.click(MouseButton::Left, 4.0, 5.0);
    assert_eq!(simulation.game().clicks, [[4.0, 5.0]]);
    assert_eq!(simulation.game().canvas_size, [32.0, 16.0]);
}

#[test]
fn screenshots_are_window_sized() {
    let mut simulation = simulation();
    let frame = simulation.screenshot().unwrap();
    assert_eq!((frame.width(), frame.height()), (100, 60));
    simulation.resize(64, 32);
    let frame = simulation.screenshot().unwrap();
    assert_eq!((frame.width(), frame.height()), (64, 32));
}

/// Headless GPU use is limited to one thread at a time, but one thread can hold several.
#[test]
fn one_thread_can_run_two_simulations_at_once() {
    let mut first = simulation();
    let mut second = simulation();
    first.tap(Key::Space);
    second.run_frames(2);
    assert!(first.screenshot().is_ok() && second.screenshot().is_ok());
    assert_eq!(first.game().space_presses, 1);
    assert_eq!(second.game().space_presses, 0);
}

#[test]
fn a_script_drives_the_game_and_saves_screenshots() {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("simulation-script");
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    let script = directory.join("run.txt");
    fs::write(
        &script,
        "# Click, then capture at two window sizes.\n\
         tap Space\n\
         wait 2\n\
         click 4 5\n\
         screenshot shots/start.png\n\
         resize 64 32\n\
         screenshot shots/small.png\n",
    )
    .unwrap();

    wingine::script::run(&config(), |_| Ok(Probe::default()), &script).unwrap();

    let dimensions = |name: &str| image::image_dimensions(directory.join("shots").join(name));
    assert_eq!(dimensions("start.png").unwrap(), (100, 60));
    assert_eq!(dimensions("small.png").unwrap(), (64, 32));
}

/// What a game sees of the gamepads in an update: those connected, whether gamepad 2's South
/// was pressed, and how far gamepad 1's left stick is pushed right.
type PadUpdate = (Vec<u32>, bool, f32);

/// Records what it sees of the gamepads each update, where a test can read it after a scripted
/// run has ended the game.
struct PadProbe(Rc<RefCell<Vec<PadUpdate>>>);

impl Game for PadProbe {
    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        let input = ctx.input();
        let connected = input.gamepads().map(|(number, _)| number).collect();
        let south = input
            .gamepad(2)
            .is_some_and(|gamepad| gamepad.pressed(GamepadButton::South));
        let right = input.gamepad(1).map_or(0.0, |gamepad| {
            gamepad.strength(GamepadButton::LeftStickRight)
        });
        self.0.borrow_mut().push((connected, south, right));
    }

    fn draw(&mut self, _canvas: &mut Canvas) {}
}

#[test]
fn a_script_connects_the_gamepads_it_names() {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("simulation-gamepads");
    fs::create_dir_all(&directory).unwrap();
    let script = directory.join("pads.txt");
    fs::write(&script, "pad-tap 2 South\nstick 1 left 0.55 0\nwait 1\n").unwrap();

    let seen = Rc::new(RefCell::new(Vec::new()));
    let probe = PadProbe(seen.clone());
    wingine::script::run(&config(), |_| Ok(probe), &script).unwrap();

    let seen = seen.borrow();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0], (vec![1, 2], true, 0.0));
    let (_, south, right) = &seen[1];
    assert!(!south && (right - 0.5).abs() < 1e-6, "{right}");
}

/// Draws five rectangles a frame, and records the frame stats it sees.
#[derive(Default)]
struct Counter {
    seen: Vec<FrameStats>,
}

impl Game for Counter {
    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        self.seen.push(ctx.frame_stats());
    }

    fn draw(&mut self, canvas: &mut Canvas) {
        for x in 0..5 {
            canvas.fill_rect(Rect::new(x as f32, 0.0, 1.0, 1.0), Color::WHITE);
        }
    }
}

#[test]
fn frame_stats_describe_the_frame_before() {
    let mut simulation = Simulation::new(&config(), |_| Ok(Counter::default())).unwrap();
    simulation.run_frames(2);
    let seen = &simulation.game().seen;
    assert_eq!(seen[0], FrameStats::default());
    assert_eq!((seen[1].quads, seen[1].draw_calls), (5, 1));
    let stats = simulation.context().frame_stats();
    assert!(stats.frame > 0.0);
    assert_eq!(stats.render, 0.0, "frames render only when asked to");

    simulation.set_render_every_frame(true);
    simulation.step();
    let stats = simulation.context().frame_stats();
    assert!(stats.render > 0.0);
    assert!(stats.frame >= stats.render);
}

#[test]
fn a_script_benchmarks_frames() {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("simulation-benchmark");
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    let script = directory.join("bench.txt");
    fs::write(&script, "wait 2\nbenchmark 3 stats/run.csv\n").unwrap();

    wingine::script::run(&config(), |_| Ok(Counter::default()), &script).unwrap();

    let csv = fs::read_to_string(directory.join("stats").join("run.csv")).unwrap();
    let lines: Vec<&str> = csv.lines().collect();
    assert_eq!(
        lines[0],
        "frame,frame_ms,update_ms,draw_ms,render_ms,quads,draw_calls"
    );
    assert_eq!(lines.len(), 4);
    let first: Vec<&str> = lines[1].split(',').collect();
    assert_eq!(first[0], "3");
    assert!(first[4].parse::<f32>().unwrap() > 0.0, "frames render");
    assert_eq!(first[5..], ["5", "1"]);
}

#[test]
fn a_bad_script_reports_its_line() {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("simulation-bad-script");
    fs::create_dir_all(&directory).unwrap();
    let script = directory.join("bad.txt");
    fs::write(&script, "wait 1\ntap NotAKey\n").unwrap();

    let err = wingine::script::run(&config(), |_| Ok(Probe::default()), &script).unwrap_err();
    assert!(
        matches!(&err, Error::Script { message, .. } if message == "line 2: unknown key `NotAKey`"),
        "unexpected error: {err}"
    );
}
