//! Tests for running games headlessly with simulated input: `Simulation` and scripted runs.

use std::{cell::RefCell, fs, path::Path, rc::Rc};

use wingine::{
    Context, Error, FrameStats, Game, Redraw, WindowConfig,
    graphics::{Canvas, Color, Resolution},
    headless::{KeyRepeat, Simulation},
    input::{Control, GamepadButton, Key, MouseButton},
    math::Rect,
    save::Saves,
    scene::Value,
};

/// Records what the game sees of its input each frame.
#[derive(Default)]
struct Probe {
    updates: u32,
    space_presses: u32,
    space_held_frames: u32,
    /// The updates, counting from 0, in which Space or L repeated, and which.
    repeats: Vec<(u32, Key)>,
    /// What was typed, each update's apart.
    typed: Vec<String>,
    clicks: Vec<[f32; 2]>,
    canvas_size: [f32; 2],
}

impl Game for Probe {
    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        let input = ctx.input();
        if !input.text().is_empty() {
            self.typed.push(input.text().to_owned());
        }
        for key in [Key::Space, Key::L] {
            if input.key_repeated(key) {
                self.repeats.push((self.updates, key));
            }
        }
        self.updates += 1;
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
fn a_fixed_resolution_larger_than_the_gpu_renders_is_an_error() {
    let config = WindowConfig {
        resolution: Resolution::Fixed {
            width: 100_000,
            height: 16,
        },
        ..config()
    };
    let err = Simulation::new(&config, |_| Ok(Probe::default()))
        .err()
        .expect("it's too wide");
    assert!(matches!(err, Error::WindowConfig(_)), "{err}");
    assert!(err.to_string().ends_with("not 100000x16"), "{err}");
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

/// Counts the frames its actions are held, and binds what a test asks it to in its next update.
#[derive(Default)]
struct Actions {
    jump_held: u32,
    fire_held: u32,
    /// What to bind `jump` to in the next update.
    rebind: Option<Key>,
}

impl Game for Actions {
    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        if let Some(key) = self.rebind.take() {
            ctx.input_mut().bind("jump", [key]);
        }
        let input = ctx.input();
        self.jump_held += u32::from(input.held("jump"));
        self.fire_held += u32::from(input.held("fire"));
    }

    fn draw(&mut self, _canvas: &mut Canvas) {}
}

/// A game with `jump` on W and any gamepad's South, `fire` on gamepad 2's right trigger, and
/// `pause` on any gamepad's Start.
fn actions() -> Simulation<Actions> {
    Simulation::new(&config(), |ctx| {
        let input = ctx.input_mut();
        input.bind(
            "jump",
            [Control::Key(Key::W), Control::Button(GamepadButton::South)],
        );
        input.bind_gamepad("fire", 2, [GamepadButton::RightTrigger]);
        input.bind("pause", [GamepadButton::Start]);
        Ok(Actions::default())
    })
    .unwrap()
}

#[test]
fn actions_are_pressed_by_what_they_are_bound_to() {
    let mut simulation = actions();

    // An action with keys presses its first key.
    simulation.hold_action("jump", 0.5);
    assert_eq!(simulation.game().jump_held, 30);
    simulation.action_down("jump");
    simulation.step();
    assert!(simulation.context().input().key_held(Key::W));

    // Releasing it lets go of what was pressed, even after the game binds it elsewhere.
    simulation.game_mut().rebind = Some(Key::Up);
    simulation.step();
    simulation.action_up("jump");
    simulation.step();
    assert!(!simulation.context().input().key_held(Key::W));

    // One with only buttons presses its first, on its gamepad, or with any, the first connected.
    simulation.connect_gamepad("first");
    simulation.connect_gamepad("second");
    simulation.tap_action("fire");
    assert_eq!(simulation.game().fire_held, 1);
    simulation.action_down("pause");
    simulation.step();
    let input = simulation.context().input();
    assert!(input.gamepad(1).unwrap().held(GamepadButton::Start));
    assert!(!input.gamepad(2).unwrap().held(GamepadButton::Start));
    simulation.action_up("pause");
    simulation.step();
    let input = simulation.context().input();
    assert!(!input.gamepad(1).unwrap().held(GamepadButton::Start));
}

#[test]
#[should_panic(expected = "`dash` isn't bound to anything")]
fn pressing_an_unbound_action_panics() {
    actions().action_down("dash");
}

#[test]
#[should_panic(expected = "`pause` is bound only to gamepads' buttons, and none is connected")]
fn pressing_a_gamepads_action_without_one_panics() {
    actions().action_down("pause");
}

#[test]
#[should_panic(expected = "`fire` is bound only to gamepad 2's buttons, and it isn't connected")]
fn pressing_an_action_of_a_disconnected_gamepad_panics() {
    let mut simulation = actions();
    simulation.connect_gamepad("first");
    simulation.action_down("fire");
}

/// The updates from `first` to `last` that come `every` updates apart, each with `key`.
fn every(first: u32, last: u32, every: usize, key: Key) -> Vec<(u32, Key)> {
    (first..=last).step_by(every).map(|n| (n, key)).collect()
}

#[test]
fn held_keys_repeat_after_a_delay_then_at_a_rate() {
    let mut simulation = simulation();
    simulation.hold(Key::Space, 1.0);
    simulation.run_frames(30);
    let probe = simulation.game();
    assert_eq!(probe.space_presses, 1, "repeats aren't presses");
    // Half a second after the press, then every other frame.
    assert_eq!(probe.repeats, every(30, 58, 2, Key::Space));
}

#[test]
fn only_the_last_key_pressed_repeats() {
    let mut simulation = simulation();
    simulation.key_down(Key::Space);
    simulation.run_frames(40);
    simulation.key_down(Key::L);
    simulation.run_frames(40);
    // Letting go of L doesn't start Space repeating again.
    simulation.key_up(Key::L);
    simulation.run_frames(40);
    simulation.key_up(Key::Space);
    let mut expected = every(30, 38, 2, Key::Space);
    expected.extend(every(70, 78, 2, Key::L));
    assert_eq!(simulation.game().repeats, expected);
}

#[test]
fn key_repeat_can_be_set_or_turned_off() {
    let mut simulation = simulation();
    simulation.set_key_repeat(Some(KeyRepeat {
        delay: 0.25,
        rate: 20.0,
    }));
    simulation.hold(Key::Space, 0.5);
    assert_eq!(simulation.game().repeats, every(15, 27, 3, Key::Space));

    // A key held when repeats change waits the new delay from then.
    simulation.key_down(Key::L);
    simulation.run_frames(10);
    simulation.set_key_repeat(Some(KeyRepeat {
        delay: 0.1,
        rate: 60.0,
    }));
    simulation.run_frames(8);
    simulation.set_key_repeat(None);
    simulation.run_frames(60);
    simulation.key_up(Key::L);
    let mut expected = every(15, 27, 3, Key::Space);
    expected.extend(every(46, 47, 1, Key::L));
    assert_eq!(simulation.game().repeats, expected);
}

#[test]
fn keys_type_as_on_a_us_keyboard_and_text_types_as_any() {
    let mut simulation = simulation();
    simulation.tap(Key::L);
    simulation.key_down(Key::RightShift);
    simulation.tap(Key::Period);
    simulation.key_up(Key::RightShift);
    // Shortcuts and keys that type control characters aren't text.
    simulation.key_down(Key::LeftControl);
    simulation.tap(Key::S);
    simulation.key_up(Key::LeftControl);
    simulation.tap(Key::Enter);
    simulation.tap(Key::Backspace);
    // Text from other layouts or an input method, in order within an update.
    simulation.type_text("é");
    simulation.type_text("日本\r");
    simulation.step();
    assert_eq!(simulation.game().typed, ["l", ">", "é日本"]);
}

#[test]
fn held_keys_type_again_as_they_repeat() {
    let mut simulation = simulation();
    simulation.hold(Key::L, 1.0);
    assert_eq!(simulation.game().typed, ["l"; 16]);
}

#[test]
fn compositions_reach_a_game_that_takes_an_input_method() {
    let mut simulation = simulation();
    simulation.compose("に");
    simulation.step();
    assert_eq!(simulation.context().input().composition(), "");

    let area = Rect::new(4.0, 4.0, 20.0, 8.0);
    simulation
        .context()
        .input_mut()
        .set_input_method(Some(area));
    simulation.compose("にほ");
    simulation.run_frames(2);
    assert_eq!(simulation.context().input().composition(), "にほ");
    simulation.type_text("日本");
    simulation.step();
    assert_eq!(simulation.context().input().composition(), "");
    assert_eq!(simulation.game().typed, ["日本"]);
}

#[test]
fn a_script_types() {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("simulation-type");
    fs::create_dir_all(&directory).unwrap();
    let script = directory.join("type.txt");
    fs::write(&script, "type > ?  # the stairs\ntap Q\nwait 1\n").unwrap();

    let typed = Rc::new(RefCell::new(String::new()));
    let probe = TextProbe(typed.clone());
    wingine::script::run(&config(), |_| Ok(probe), &script).unwrap();
    assert_eq!(*typed.borrow(), "> ?q");
}

/// Records what was typed, where a test can read it after a scripted run has ended the game.
struct TextProbe(Rc<RefCell<String>>);

impl Game for TextProbe {
    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        self.0.borrow_mut().push_str(ctx.input().text());
    }

    fn draw(&mut self, _canvas: &mut Canvas) {}
}

/// Writes the save `progress` each update.
struct Saver;

impl Game for Saver {
    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        ctx.saves()
            .write("progress", &Value::Number(1.0))
            .expect("the save is written");
    }

    fn draw(&mut self, _canvas: &mut Canvas) {}
}

#[test]
fn a_script_starts_from_copies_of_saves() {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("simulation-saves");
    let _ = fs::remove_dir_all(&directory);
    let start = directory.join("deep career");
    Saves::in_folder(&start)
        .write("career", &Value::Number(7.0))
        .unwrap();
    let script = directory.join("deep.txt");
    fs::write(
        &script,
        "# From a career deep in.\nsaves deep career\nwait 2\n",
    )
    .unwrap();

    // What the game read as it started, and where its saves were.
    let seen = RefCell::new(None);
    wingine::script::run(
        &config(),
        |ctx| {
            let saves = ctx.saves();
            let career = saves.read("career")?;
            *seen.borrow_mut() = Some((career, saves.folder().unwrap().to_owned()));
            Ok(Saver)
        },
        &script,
    )
    .unwrap();
    let (career, folder) = seen.take().unwrap();
    assert_eq!(career, Some(Value::Number(7.0)));
    assert_ne!(folder, start);
    assert!(!folder.exists(), "the copy goes when the run ends");
    assert_eq!(
        Saves::in_folder(&start).list().unwrap(),
        ["career"],
        "the run never writes the saves it started from"
    );

    // Without `saves`, a run starts with none, even for a game whose window keeps them in a
    // folder of its own, which it leaves as it was.
    fs::write(&script, "wait 2\n").unwrap();
    let own = WindowConfig {
        save_folder: Some(start.clone()),
        ..config()
    };
    let seen = RefCell::new(None);
    wingine::script::run(
        &own,
        |ctx| {
            *seen.borrow_mut() = Some(ctx.saves().list()?);
            Ok(Saver)
        },
        &script,
    )
    .unwrap();
    assert_eq!(seen.take(), Some(Vec::new()));
    assert_eq!(Saves::in_folder(&start).list().unwrap(), ["career"]);

    fs::write(&script, "saves nowhere\n").unwrap();
    let err = wingine::script::run(&config(), |_| Ok(Saver), &script).unwrap_err();
    assert!(
        matches!(&err, Error::Script { message, .. } if message.starts_with("line 1: could not read the saves in")),
        "unexpected error: {err}"
    );
}

#[test]
fn a_script_sets_key_repeat() {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("simulation-key-repeat");
    fs::create_dir_all(&directory).unwrap();
    let script = directory.join("repeat.txt");
    fs::write(
        &script,
        "key-repeat 10 15\nhold L 20\nkey-repeat off\nhold L 20\n",
    )
    .unwrap();

    let seen = Rc::new(RefCell::new(Vec::new()));
    let probe = RepeatProbe {
        updates: 0,
        seen: seen.clone(),
    };
    wingine::script::run(&config(), |_| Ok(probe), &script).unwrap();
    // Ten frames after the press, then every four, and then not at all.
    assert_eq!(*seen.borrow(), [10, 14, 18]);
}

/// Records the updates, counting from 0, in which L repeated, where a test can read them after a
/// scripted run has ended the game.
struct RepeatProbe {
    updates: u32,
    seen: Rc<RefCell<Vec<u32>>>,
}

impl Game for RepeatProbe {
    fn update(&mut self, ctx: &mut Context, _dt: f32) {
        if ctx.input().key_repeated(Key::L) {
            self.seen.borrow_mut().push(self.updates);
        }
        self.updates += 1;
    }

    fn draw(&mut self, _canvas: &mut Canvas) {}
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

/// With an update rate and frames on demand, a frame too soon after the last for an update, as
/// one for input may be, asks for another when an update falls due, so the game sees the input
/// without waiting for more.
#[test]
fn frames_without_an_update_want_one_when_it_falls_due() {
    let config = WindowConfig {
        redraw: Redraw::OnDemand,
        update_rate: Some(30.0),
        ..config()
    };
    let mut simulation = Simulation::new(&config, |_| Ok(Probe::default())).unwrap();
    for _ in 0..4 {
        simulation.step();
    }
    let updates = simulation.game().updates;
    simulation.key_down(Key::Space);
    simulation.step();
    assert_eq!(simulation.game().updates, updates, "too soon for an update");
    let wait = simulation.next_frame_in().expect("a frame is wanted");
    assert!(wait <= 1.0 / 30.0, "{wait}");
    simulation.key_up(Key::Space);
    simulation.step();
    assert_eq!(simulation.game().updates, updates + 1);
    assert_eq!(simulation.game().space_presses, 1);
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
