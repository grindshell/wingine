//! Scripted runs: drive any game with simulated input and save screenshots, without a window.
//!
//! Set the `WINGINE_SCRIPT` environment variable to a script file and start the game as usual
//! (for example `cargo run --example sprites`). Instead of opening a window, [`run`](crate::run)
//! starts the game in a [`Simulation`], runs the script, and exits. The virtual window is the
//! size set in the game's [`WindowConfig`], and frames advance at a fixed 60 per second.
//!
//! A script has one command per line; `#` starts a comment:
//!
//! ```text
//! saves deep-career      # start from copies of the saves in a folder: first, if at all
//! wait 30                # run 30 frames
//! wait 0.5s              # run half a second of game time (30 frames)
//! press Right            # hold a key down (names as in `Key`, any case)
//! release Right          # let it go
//! tap Space              # press, run one frame, release
//! hold W 1s              # press, run for 1 second (or a number of frames), release
//! type Ada > ?           # type the rest of the line, without pressing keys
//! mouse 120 64           # move the cursor to canvas pixel (120, 64)
//! mouse-by 40 -10        # move the mouse 40 right and 10 away, for a game that has captured it
//! mouse-down Left        # hold a mouse button down (Left, Right, Middle, Back, Forward)
//! mouse-up Left
//! click 120 64           # move the cursor, press Left, run one frame, release
//! click Right            # the same with another button, where the cursor already is
//! scroll 0 -2            # turn the mouse wheel 2 notches toward you; x tilts it right
//! click-gui Go to town   # click the interface's first widget labelled the rest of the line
//! pad-down 1 South       # hold gamepad 1's button down (names as in `GamepadButton`)
//! pad-up 1 South         # let it go
//! pad-tap 2 Start        # press gamepad 2's button, run one frame, release
//! pad-hold 1 DPadLeft 1s # press, run for 1 second (or a number of frames), release
//! stick 1 Left 0.5 -1    # move gamepad 1's left stick to x, y, each from -1 to 1 (y down)
//! key-repeat 0.25s 20    # repeat held keys after 0.25 seconds (or frames), 20 times a second
//! key-repeat off         # don't repeat held keys
//! resize 640 360         # resize the virtual window, in physical pixels
//! screenshot walk.png    # save the latest frame, relative to the script's directory
//! benchmark 10s          # run 10 seconds, rendering every frame, and report how long they took
//! benchmark 600 run.csv  # the same, also saving each frame's stats to a CSV file
//! ```
//!
//! Gamepads are numbered from 1, and connect when a command first names them, along with any
//! numbered before them.
//!
//! A run's [saves](crate::save) are in a temporary folder, which starts empty, unless the script
//! starts with `saves` and a folder, relative to the script's (the folder a window keeps them in,
//! or one a run before kept): then it starts with copies of the files in that folder, so the run
//! never changes them. Either way, the folder goes when the run ends.
//!
//! `mouse-by` moves the mouse as [`Simulation::move_mouse_by`] does, in the units of
//! [`Input::mouse_motion`]: the game sees it only while it has captured the mouse, as a game that
//! looks around with it does, and otherwise it does nothing.
//!
//! `click-gui` clicks a widget of the game's [interface](crate::gui) as
//! [`Simulation::click_gui`] does, by its label: a button's text, or what a field is labelled by.
//! The interface must have laid it out in the frame before, and a window shows from the frame
//! after it opens, so `wait` a frame or two for one. It needs the `egui` feature.
//!
//! A key held down repeats as a window's system would repeat it, the last key pressed only: by
//! default, after half a second, 30 times a second (see [`KeyRepeat`]). Keys pressed, and their
//! repeats, type what they would on a US keyboard, with Shift if it's held; `type` types any
//! text, as other layouts and input methods do, but not `#`, which starts a comment.
//!
//! Each saved screenshot is reported on standard output, as is each benchmark: the mean, median,
//! 99th percentile and longest of each [`FrameStats`] time, as a window would see them (with
//! vsync off), except that each frame waits for the GPU to finish it before the next starts.
//! Benchmark a release build: `cargo run --release`.
//!
//! [`Simulation`]: crate::headless::Simulation
//! [`Input::mouse_motion`]: crate::input::Input::mouse_motion
//! [`KeyRepeat`]: crate::headless::KeyRepeat
//! [`WindowConfig`]: crate::WindowConfig

use std::{
    fmt,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use crate::{
    Context, Error, FrameStats, Game, Result, WindowConfig,
    headless::{KeyRepeat, Simulation},
    input::{GamepadButton, Key, MouseButton, Stick},
    save::TemporaryFolder,
};

/// The environment variable naming a script for [`run`](crate::run) to run instead of opening a
/// window.
pub const ENV_VAR: &str = "WINGINE_SCRIPT";

/// One script command.
#[derive(Clone, Debug, PartialEq)]
enum Command {
    Saves(PathBuf),
    Wait(u32),
    Press(Key),
    Release(Key),
    Tap(Key),
    Hold(Key, u32),
    Type(String),
    Mouse([f32; 2]),
    MouseBy([f32; 2]),
    MouseDown(MouseButton),
    MouseUp(MouseButton),
    Click(MouseButton, Option<[f32; 2]>),
    Scroll([f32; 2]),
    #[cfg(feature = "egui")]
    ClickGui(String),
    PadDown(u32, GamepadButton),
    PadUp(u32, GamepadButton),
    PadTap(u32, GamepadButton),
    PadHold(u32, GamepadButton, u32),
    Stick(u32, Stick, [f32; 2]),
    KeyRepeat(Option<KeyRepeat>),
    Resize(u32, u32),
    Screenshot(PathBuf),
    Benchmark(u32, Option<PathBuf>),
}

/// Runs the script at `path` against the game built by `init`, as [`run`](crate::run) does when
/// `WINGINE_SCRIPT` is set.
pub fn run<G: Game>(
    config: &WindowConfig,
    init: impl FnOnce(&mut Context) -> Result<G>,
    path: &Path,
) -> Result<()> {
    let fail = |message: String| Error::Script {
        path: path.to_owned(),
        message,
    };
    let source =
        fs::read_to_string(path).map_err(|err| fail(format!("could not read it: {err}")))?;
    let commands = parse(&source).map_err(fail)?;
    let directory = path.parent().unwrap_or(Path::new("."));

    // Declared before the simulation, so its game has finished with the saves first.
    let saves = match commands.first() {
        Some((line, Command::Saves(folder))) => {
            let copy = TemporaryFolder::new();
            copy_files(&directory.join(folder), copy.path())
                .map_err(|err| fail(format!("line {line}: {err}")))?;
            Some(copy)
        }
        _ => None,
    };
    // Never the game's own folder of saves: without copies, the simulation keeps a temporary
    // folder of its own.
    let config = WindowConfig {
        save_folder: saves.as_ref().map(|saves| saves.path().to_owned()),
        ..config.clone()
    };
    let mut simulation = Simulation::new(&config, init)?;
    // Gamepads connected so far.
    let mut gamepads = 0;
    for (line, command) in commands {
        // Only `click-gui` names its line when it fails.
        #[cfg(not(feature = "egui"))]
        let _ = line;
        if let Command::PadDown(gamepad, _)
        | Command::PadUp(gamepad, _)
        | Command::PadTap(gamepad, _)
        | Command::PadHold(gamepad, ..)
        | Command::Stick(gamepad, ..) = command
        {
            while gamepads < gamepad {
                gamepads = simulation.connect_gamepad(&format!("Gamepad {}", gamepads + 1));
            }
        }
        match command {
            // Copied before the game started.
            Command::Saves(_) => {}
            Command::Wait(frames) => simulation.run_frames(frames),
            Command::Press(key) => simulation.key_down(key),
            Command::Release(key) => simulation.key_up(key),
            Command::Tap(key) => simulation.tap(key),
            Command::Hold(key, frames) => simulation.hold_frames(key, frames),
            Command::Type(text) => simulation.type_text(&text),
            Command::Mouse([x, y]) => simulation.move_mouse(x, y),
            Command::MouseBy([x, y]) => simulation.move_mouse_by(x, y),
            Command::MouseDown(button) => simulation.mouse_down(button),
            Command::MouseUp(button) => simulation.mouse_up(button),
            Command::Click(button, position) => {
                if let Some([x, y]) = position {
                    simulation.move_mouse(x, y);
                }
                simulation.tap_mouse(button);
            }
            Command::Scroll([x, y]) => simulation.scroll(x, y),
            #[cfg(feature = "egui")]
            Command::ClickGui(label) => {
                if simulation.gui_rect(&label).is_none() {
                    return Err(fail(format!(
                        "line {line}: the interface had no widget labelled `{label}` in the \
                         frame before (a window shows from the frame after it opens, so `wait` \
                         for one)"
                    )));
                }
                simulation.click_gui(&label);
            }
            Command::PadDown(gamepad, button) => simulation.button_down(gamepad, button),
            Command::PadUp(gamepad, button) => simulation.button_up(gamepad, button),
            Command::PadTap(gamepad, button) => simulation.tap_button(gamepad, button),
            Command::PadHold(gamepad, button, frames) => {
                simulation.hold_button_frames(gamepad, button, frames);
            }
            Command::Stick(gamepad, stick, [x, y]) => simulation.move_stick(gamepad, stick, x, y),
            Command::KeyRepeat(repeat) => simulation.set_key_repeat(repeat),
            Command::Resize(width, height) => simulation.resize(width, height),
            Command::Screenshot(file) => {
                let file = output_file(directory, &file).map_err(fail)?;
                simulation.screenshot()?.save_png(&file)?;
                println!(
                    "frame {}: saved {}",
                    simulation.frame_count(),
                    file.display()
                );
            }
            Command::Benchmark(frames, file) => {
                let first = simulation.frame_count() + 1;
                simulation.set_render_every_frame(true);
                let stats: Vec<FrameStats> = (0..frames)
                    .map(|_| {
                        simulation.step();
                        simulation.context().frame_stats()
                    })
                    .collect();
                simulation.set_render_every_frame(false);
                println!(
                    "frames {first} to {}, rendered on {}:",
                    simulation.frame_count(),
                    simulation.adapter_name()
                );
                print!("{}", summary(&stats));
                if cfg!(debug_assertions) {
                    println!("  (a debug build: benchmark a release build instead)");
                }
                if let Some(file) = file {
                    let file = output_file(directory, &file).map_err(fail)?;
                    fs::write(&file, csv(first, &stats)).map_err(|err| {
                        fail(format!("could not write {}: {err}", file.display()))
                    })?;
                    println!("  saved {}", file.display());
                }
            }
        }
    }
    Ok(())
}

/// Copies the files in folder `from` into folder `to`, which it makes.
fn copy_files(from: &Path, to: &Path) -> Result<(), String> {
    let entries = fs::read_dir(from)
        .map_err(|err| format!("could not read the saves in {}: {err}", from.display()))?;
    fs::create_dir_all(to).map_err(|err| format!("could not create {}: {err}", to.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("could not read {}: {err}", from.display()))?;
        let path = entry.path();
        if path.is_file() {
            fs::copy(&path, to.join(entry.file_name()))
                .map_err(|err| format!("could not copy {}: {err}", path.display()))?;
        }
    }
    Ok(())
}

/// The path of `file`, relative to the script's `directory`, with its folder created.
fn output_file(directory: &Path, file: &Path) -> Result<PathBuf, String> {
    // Rebuilt from components so separators match the platform's.
    let file: PathBuf = directory.join(file).components().collect();
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("could not create {}: {err}", parent.display()))?;
    }
    Ok(file)
}

/// Reads one of a frame's stats.
type Stat<T> = fn(&FrameStats) -> T;

/// A table of the frames' times in milliseconds, and what they drew.
fn summary(stats: &[FrameStats]) -> String {
    let mut text = String::from("              mean   median      99%      max  (ms)\n");
    let times: [(&str, Stat<f32>); 4] = [
        ("frame", |stats| stats.frame),
        ("update", |stats| stats.update),
        ("draw", |stats| stats.draw),
        ("render", |stats| stats.render),
    ];
    for (name, time) in times {
        let mut ms: Vec<f32> = stats.iter().map(|stats| time(stats) * 1000.0).collect();
        ms.sort_by(f32::total_cmp);
        let mean = ms.iter().sum::<f32>() / ms.len() as f32;
        let _ = writeln!(
            text,
            "  {name:<8}{mean:>8.3}{:>9.3}{:>9.3}{:>9.3}",
            percentile(&ms, 0.5),
            percentile(&ms, 0.99),
            percentile(&ms, 1.0),
        );
    }
    let counts: [(&str, Stat<u32>); 2] = [
        ("quads", |stats| stats.quads),
        ("draw calls", |stats| stats.draw_calls),
    ];
    for (name, count) in counts {
        let total: u64 = stats.iter().map(|stats| u64::from(count(stats))).sum();
        let most = stats.iter().map(count).max().unwrap_or(0);
        let mean = total as f64 / stats.len() as f64;
        let _ = writeln!(text, "  {name}: {mean:.0} on average, {most} at most");
    }
    text
}

/// The value at `fraction` of the way through `sorted`, by the nearest-rank method.
fn percentile(sorted: &[f32], fraction: f32) -> f32 {
    let rank = (fraction * sorted.len() as f32).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

/// The frames' stats as CSV, numbering them from `first`, with times in milliseconds.
fn csv(first: u64, stats: &[FrameStats]) -> String {
    let mut text = String::from("frame,frame_ms,update_ms,draw_ms,render_ms,quads,draw_calls\n");
    for (number, stats) in (first..).zip(stats) {
        let _ = writeln!(
            text,
            "{number},{},{},{},{},{},{}",
            stats.frame * 1000.0,
            stats.update * 1000.0,
            stats.draw * 1000.0,
            stats.render * 1000.0,
            stats.quads,
            stats.draw_calls
        );
    }
    text
}

/// Parses a script into its commands and their line numbers, or describes the first problem
/// with its line number.
fn parse(source: &str) -> Result<Vec<(usize, Command)>, String> {
    let mut commands = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let number = index + 1;
        let line = line.split('#').next().unwrap_or_default();
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some((&name, args)) = words.split_first() else {
            continue;
        };
        // What these commands take is the rest of the line, spaces and all.
        let rest = line.trim().strip_prefix(name).unwrap_or_default().trim();
        let command = match (name, rest) {
            ("saves" | "type" | "click-gui", "") => Err(format!("wrong arguments for `{name}`")),
            ("saves", _) if !commands.is_empty() => {
                Err("`saves` must come before every other command".to_owned())
            }
            ("saves", folder) => Ok(Command::Saves(PathBuf::from(folder))),
            ("type", text) => Ok(Command::Type(text.to_owned())),
            ("click-gui", label) => click_gui(label),
            _ => parse_command(name, args),
        }
        .map_err(|err| format!("line {number}: {err}"))?;
        commands.push((number, command));
    }
    Ok(commands)
}

/// Parses `click-gui`, which needs the `egui` feature.
fn click_gui(label: &str) -> Result<Command, String> {
    #[cfg(feature = "egui")]
    return Ok(Command::ClickGui(label.to_owned()));
    #[cfg(not(feature = "egui"))]
    {
        let _ = label;
        Err("`click-gui` needs the engine's `egui` feature".to_owned())
    }
}

fn parse_command(name: &str, args: &[&str]) -> Result<Command, String> {
    let command = match (name, args) {
        ("wait", [duration]) => Command::Wait(frames(duration)?),
        ("press", [key]) => Command::Press(by_name(key)?),
        ("release", [key]) => Command::Release(by_name(key)?),
        ("tap", [key]) => Command::Tap(by_name(key)?),
        ("hold", [key, duration]) => Command::Hold(by_name(key)?, frames(duration)?),
        ("mouse", [x, y]) => Command::Mouse([number(x)?, number(y)?]),
        ("mouse-by", [x, y]) => Command::MouseBy([number(x)?, number(y)?]),
        ("mouse-down", [button]) => Command::MouseDown(by_name(button)?),
        ("mouse-up", [button]) => Command::MouseUp(by_name(button)?),
        ("click", []) => Command::Click(MouseButton::Left, None),
        ("click", [button]) => Command::Click(by_name(button)?, None),
        ("click", [x, y]) => Command::Click(MouseButton::Left, Some([number(x)?, number(y)?])),
        ("click", [button, x, y]) => {
            Command::Click(by_name(button)?, Some([number(x)?, number(y)?]))
        }
        ("scroll", [x, y]) => Command::Scroll([number(x)?, number(y)?]),
        ("pad-down", [gamepad, button]) => Command::PadDown(size(gamepad)?, by_name(button)?),
        ("pad-up", [gamepad, button]) => Command::PadUp(size(gamepad)?, by_name(button)?),
        ("pad-tap", [gamepad, button]) => Command::PadTap(size(gamepad)?, by_name(button)?),
        ("pad-hold", [gamepad, button, duration]) => {
            Command::PadHold(size(gamepad)?, by_name(button)?, frames(duration)?)
        }
        ("stick", [gamepad, stick, x, y]) => {
            Command::Stick(size(gamepad)?, by_name(stick)?, [number(x)?, number(y)?])
        }
        ("key-repeat", ["off"]) => Command::KeyRepeat(None),
        ("key-repeat", [delay, rate]) => Command::KeyRepeat(Some(KeyRepeat {
            delay: frames(delay)? as f32 / 60.0,
            rate: repeat_rate(rate)?,
        })),
        ("resize", [width, height]) => Command::Resize(size(width)?, size(height)?),
        ("screenshot", [file]) => Command::Screenshot(PathBuf::from(file)),
        ("benchmark", [duration]) => Command::Benchmark(some_frames(duration)?, None),
        ("benchmark", [duration, file]) => {
            Command::Benchmark(some_frames(duration)?, Some(PathBuf::from(file)))
        }
        (
            "wait" | "press" | "release" | "tap" | "hold" | "mouse" | "mouse-by" | "mouse-down"
            | "mouse-up" | "click" | "scroll" | "pad-down" | "pad-up" | "pad-tap" | "pad-hold"
            | "stick" | "key-repeat" | "resize" | "screenshot" | "benchmark",
            _,
        ) => return Err(format!("wrong arguments for `{name}`")),
        _ => return Err(format!("unknown command `{name}`")),
    };
    Ok(command)
}

/// Parses a duration: a number of frames, or seconds with an `s` suffix.
fn frames(duration: &str) -> Result<u32, String> {
    let invalid =
        || format!("`{duration}` is not a number of frames or seconds (like `30` or `0.5s`)");
    match duration.strip_suffix('s') {
        Some(seconds) => {
            let seconds: f32 = seconds.parse().map_err(|_| invalid())?;
            if !seconds.is_finite() || seconds < 0.0 {
                return Err(invalid());
            }
            Ok(Simulation::<()>::frames_in(seconds))
        }
        None => duration.parse().map_err(|_| invalid()),
    }
}

/// Parses a duration of at least one frame.
fn some_frames(duration: &str) -> Result<u32, String> {
    match frames(duration)? {
        0 => Err(format!("`{duration}` is less than a frame")),
        frames => Ok(frames),
    }
}

/// Parses the name of a key or mouse button.
fn by_name<T>(text: &str) -> Result<T, String>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    text.parse().map_err(|err: T::Err| err.to_string())
}

fn number(text: &str) -> Result<f32, String> {
    text.parse()
        .map_err(|_| format!("`{text}` is not a number"))
}

/// Parses how many times a second keys repeat.
fn repeat_rate(text: &str) -> Result<f32, String> {
    match text.parse::<f32>() {
        Ok(rate) if rate > 0.0 && rate.is_finite() => Ok(rate),
        _ => Err(format!("`{text}` is not a number of repeats a second")),
    }
}

fn size(text: &str) -> Result<u32, String> {
    match text.parse() {
        Ok(size) if size > 0 => Ok(size),
        _ => Err(format!("`{text}` is not a positive whole number")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The commands of `source`, without their line numbers.
    fn commands(source: &str) -> Result<Vec<Command>, String> {
        let commands = parse(source)?;
        Ok(commands.into_iter().map(|(_, command)| command).collect())
    }

    #[test]
    fn parses_every_command() {
        let script = "
            # A comment line.
            saves deep career
            wait 10
            wait 0.5s      # trailing comment
            press right
            release Right
            tap Space
            hold W 1s
            type >
            type Ada  Lovelace? # a comment
            mouse 12.5 40
            mouse-by 40 -10.5
            mouse-down left
            mouse-up Left
            click
            click right
            click 3 4
            click middle 3 4
            scroll 0.5 -2
            pad-down 1 south
            pad-up 1 South
            pad-tap 2 Start
            pad-hold 1 DPadLeft 0.5s
            stick 1 left 0.5 -1
            key-repeat 0.25s 20
            key-repeat 15 2.5
            key-repeat off
            resize 640 360
            screenshot shots/a.png
            benchmark 2s
            benchmark 10 stats/run.csv
        ";
        assert_eq!(
            commands(script),
            Ok(vec![
                Command::Saves(PathBuf::from("deep career")),
                Command::Wait(10),
                Command::Wait(30),
                Command::Press(Key::Right),
                Command::Release(Key::Right),
                Command::Tap(Key::Space),
                Command::Hold(Key::W, 60),
                Command::Type(">".into()),
                Command::Type("Ada  Lovelace?".into()),
                Command::Mouse([12.5, 40.0]),
                Command::MouseBy([40.0, -10.5]),
                Command::MouseDown(MouseButton::Left),
                Command::MouseUp(MouseButton::Left),
                Command::Click(MouseButton::Left, None),
                Command::Click(MouseButton::Right, None),
                Command::Click(MouseButton::Left, Some([3.0, 4.0])),
                Command::Click(MouseButton::Middle, Some([3.0, 4.0])),
                Command::Scroll([0.5, -2.0]),
                Command::PadDown(1, GamepadButton::South),
                Command::PadUp(1, GamepadButton::South),
                Command::PadTap(2, GamepadButton::Start),
                Command::PadHold(1, GamepadButton::DPadLeft, 30),
                Command::Stick(1, Stick::Left, [0.5, -1.0]),
                Command::KeyRepeat(Some(KeyRepeat {
                    delay: 0.25,
                    rate: 20.0
                })),
                Command::KeyRepeat(Some(KeyRepeat {
                    delay: 0.25,
                    rate: 2.5
                })),
                Command::KeyRepeat(None),
                Command::Resize(640, 360),
                Command::Screenshot(PathBuf::from("shots/a.png")),
                Command::Benchmark(120, None),
                Command::Benchmark(10, Some(PathBuf::from("stats/run.csv"))),
            ])
        );
        assert_eq!(parse("\n\nwait 1").unwrap()[0].0, 3, "lines count from 1");
    }

    #[test]
    fn click_gui_takes_the_rest_of_the_line() {
        let parsed = commands("click-gui Go to town  # by the stairs");
        #[cfg(feature = "egui")]
        assert_eq!(parsed, Ok(vec![Command::ClickGui("Go to town".into())]));
        #[cfg(not(feature = "egui"))]
        assert_eq!(
            parsed,
            Err("line 1: `click-gui` needs the engine's `egui` feature".into())
        );
    }

    #[test]
    fn percentiles_take_the_nearest_rank() {
        let sorted = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(percentile(&sorted, 0.5), 2.0);
        assert_eq!(percentile(&sorted, 0.99), 4.0);
        assert_eq!(percentile(&sorted, 1.0), 4.0);
        assert_eq!(percentile(&sorted, 0.0), 1.0);
    }

    #[test]
    fn errors_name_the_line() {
        assert_eq!(
            parse("wait 1\npress Shift"),
            Err("line 2: unknown key `Shift`".into())
        );
        assert_eq!(parse("jump"), Err("line 1: unknown command `jump`".into()));
        assert_eq!(
            parse("resize 640"),
            Err("line 1: wrong arguments for `resize`".into())
        );
        assert!(
            parse("wait soon")
                .unwrap_err()
                .starts_with("line 1: `soon`")
        );
        assert!(parse("wait -1s").is_err());
        assert!(parse("benchmark 0").is_err());
        assert!(parse("resize 0 10").is_err());
        assert!(parse("pad-down 0 South").is_err());
        assert_eq!(
            parse("key-repeat 0.5s 0"),
            Err("line 1: `0` is not a number of repeats a second".into())
        );
        assert!(parse("key-repeat on").is_err());
        assert_eq!(
            parse("type # nothing"),
            Err("line 1: wrong arguments for `type`".into())
        );
        assert_eq!(
            parse("wait 1\nsaves deep"),
            Err("line 2: `saves` must come before every other command".into())
        );
        assert!(parse("click-gui").is_err());
        assert!(parse("key-repeat -1 30").is_err());
        assert_eq!(
            parse("pad-tap 1 A"),
            Err("line 1: unknown gamepad button `A`".into())
        );
    }
}
