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
//! wait 30                # run 30 frames
//! wait 0.5s              # run half a second of game time (30 frames)
//! press Right            # hold a key down (names as in `Key`, any case)
//! release Right          # let it go
//! tap Space              # press, run one frame, release
//! hold W 1s              # press, run for 1 second (or a number of frames), release
//! mouse 120 64           # move the cursor to canvas pixel (120, 64)
//! mouse-down Left        # hold a mouse button down (Left, Right, Middle, Back, Forward)
//! mouse-up Left
//! click 120 64           # move the cursor, press Left, run one frame, release
//! click Right            # the same with another button, where the cursor already is
//! resize 640 360         # resize the virtual window, in physical pixels
//! screenshot walk.png    # save the latest frame, relative to the script's directory
//! ```
//!
//! Each saved screenshot is reported on standard output.
//!
//! [`Simulation`]: crate::headless::Simulation
//! [`WindowConfig`]: crate::WindowConfig

use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    Context, Error, Game, Result, WindowConfig,
    headless::Simulation,
    input::{Key, MouseButton},
};

/// The environment variable naming a script for [`run`](crate::run) to run instead of opening a
/// window.
pub const ENV_VAR: &str = "WINGINE_SCRIPT";

/// One script command.
#[derive(Clone, Debug, PartialEq)]
enum Command {
    Wait(u32),
    Press(Key),
    Release(Key),
    Tap(Key),
    Hold(Key, u32),
    Mouse([f32; 2]),
    MouseDown(MouseButton),
    MouseUp(MouseButton),
    Click(MouseButton, Option<[f32; 2]>),
    Resize(u32, u32),
    Screenshot(PathBuf),
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

    let mut simulation = Simulation::new(config, init)?;
    for command in commands {
        match command {
            Command::Wait(frames) => simulation.run_frames(frames),
            Command::Press(key) => simulation.key_down(key),
            Command::Release(key) => simulation.key_up(key),
            Command::Tap(key) => simulation.tap(key),
            Command::Hold(key, frames) => {
                simulation.key_down(key);
                simulation.run_frames(frames);
                simulation.key_up(key);
            }
            Command::Mouse([x, y]) => simulation.move_mouse(x, y),
            Command::MouseDown(button) => simulation.mouse_down(button),
            Command::MouseUp(button) => simulation.mouse_up(button),
            Command::Click(button, position) => {
                if let Some([x, y]) = position {
                    simulation.move_mouse(x, y);
                }
                simulation.mouse_down(button);
                simulation.step();
                simulation.mouse_up(button);
            }
            Command::Resize(width, height) => simulation.resize(width, height),
            Command::Screenshot(file) => {
                // Rebuilt from components so separators match the platform's.
                let file: PathBuf = directory.join(file).components().collect();
                if let Some(parent) = file.parent() {
                    fs::create_dir_all(parent).map_err(|err| {
                        fail(format!("could not create {}: {err}", parent.display()))
                    })?;
                }
                simulation.screenshot()?.save_png(&file)?;
                println!(
                    "frame {}: saved {}",
                    simulation.frame_count(),
                    file.display()
                );
            }
        }
    }
    Ok(())
}

/// Parses a script, or describes the first problem with its line number.
fn parse(source: &str) -> Result<Vec<Command>, String> {
    let mut commands = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let line = line.split('#').next().unwrap_or_default();
        let words: Vec<&str> = line.split_whitespace().collect();
        let Some((&name, args)) = words.split_first() else {
            continue;
        };
        let command =
            parse_command(name, args).map_err(|err| format!("line {}: {err}", index + 1))?;
        commands.push(command);
    }
    Ok(commands)
}

fn parse_command(name: &str, args: &[&str]) -> Result<Command, String> {
    let command = match (name, args) {
        ("wait", [duration]) => Command::Wait(frames(duration)?),
        ("press", [key]) => Command::Press(key.parse().map_err(|err| format!("{err}"))?),
        ("release", [key]) => Command::Release(key.parse().map_err(|err| format!("{err}"))?),
        ("tap", [key]) => Command::Tap(key.parse().map_err(|err| format!("{err}"))?),
        ("hold", [key, duration]) => Command::Hold(
            key.parse().map_err(|err| format!("{err}"))?,
            frames(duration)?,
        ),
        ("mouse", [x, y]) => Command::Mouse([number(x)?, number(y)?]),
        ("mouse-down", [button]) => {
            Command::MouseDown(button.parse().map_err(|err| format!("{err}"))?)
        }
        ("mouse-up", [button]) => Command::MouseUp(button.parse().map_err(|err| format!("{err}"))?),
        ("click", []) => Command::Click(MouseButton::Left, None),
        ("click", [button]) => {
            Command::Click(button.parse().map_err(|err| format!("{err}"))?, None)
        }
        ("click", [x, y]) => Command::Click(MouseButton::Left, Some([number(x)?, number(y)?])),
        ("click", [button, x, y]) => Command::Click(
            button.parse().map_err(|err| format!("{err}"))?,
            Some([number(x)?, number(y)?]),
        ),
        ("resize", [width, height]) => Command::Resize(size(width)?, size(height)?),
        ("screenshot", [file]) => Command::Screenshot(PathBuf::from(file)),
        (
            "wait" | "press" | "release" | "tap" | "hold" | "mouse" | "mouse-down" | "mouse-up"
            | "click" | "resize" | "screenshot",
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
            Ok((seconds / Simulation::<()>::FRAME_TIME).round() as u32)
        }
        None => duration.parse().map_err(|_| invalid()),
    }
}

fn number(text: &str) -> Result<f32, String> {
    text.parse()
        .map_err(|_| format!("`{text}` is not a number"))
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

    #[test]
    fn parses_every_command() {
        let script = "
            # A comment line.
            wait 10
            wait 0.5s      # trailing comment
            press right
            release Right
            tap Space
            hold W 1s
            mouse 12.5 40
            mouse-down left
            mouse-up Left
            click
            click right
            click 3 4
            click middle 3 4
            resize 640 360
            screenshot shots/a.png
        ";
        assert_eq!(
            parse(script),
            Ok(vec![
                Command::Wait(10),
                Command::Wait(30),
                Command::Press(Key::Right),
                Command::Release(Key::Right),
                Command::Tap(Key::Space),
                Command::Hold(Key::W, 60),
                Command::Mouse([12.5, 40.0]),
                Command::MouseDown(MouseButton::Left),
                Command::MouseUp(MouseButton::Left),
                Command::Click(MouseButton::Left, None),
                Command::Click(MouseButton::Right, None),
                Command::Click(MouseButton::Left, Some([3.0, 4.0])),
                Command::Click(MouseButton::Middle, Some([3.0, 4.0])),
                Command::Resize(640, 360),
                Command::Screenshot(PathBuf::from("shots/a.png")),
            ])
        );
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
        assert!(parse("resize 0 10").is_err());
    }
}
