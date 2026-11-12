//! The `wingine` executable, which runs a game written in Luau from its folder, its tests, or
//! exports it to ship:
//!
//! ```text
//! wingine [--watch] path/to/game
//! wingine test [--update] [--ignored] path/to/game [filter]
//! wingine export path/to/game path/to/out
//! wingine
//! ```
//!
//! With `--watch`, the game reloads whenever a file in its folder changes. `test` runs the tests
//! in its `*.test.luau` files, or those whose files and names contain `filter`; with `--update`,
//! it saves screenshots that don't match as their references, and with `--ignored`, it runs only
//! the tests declared `ignored`, which it otherwise leaves out. `export` makes a folder to
//! ship, whose executable, a copy of this one, runs the game beside it when it's started with
//! nothing, as by double-clicking. See the `runtime` module for what the folder holds,
//! `runtime::testing` for tests, and `runtime::export` for exports.

use std::{ffi::OsString, path::Path, process::ExitCode};

use wingine::runtime::{self, testing};

const USAGE: &str = "usage: wingine [--watch] <game folder>
       wingine test [--update] [--ignored] <game folder> [filter]
       wingine export <game folder> <out folder>";

fn main() -> ExitCode {
    static LOGGER: Logger = Logger;
    log::set_logger(&LOGGER).expect("nothing else sets a logger");
    log::set_max_level(log::LevelFilter::Info);

    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let result = match args.as_slice() {
        // As a game shipped to players starts, by double-clicking it.
        [] => match runtime::shipped_game() {
            Some(game) => runtime::run_shipped(game),
            None => {
                eprintln!("{USAGE}");
                return ExitCode::FAILURE;
            }
        },
        // A game in a folder called `test` or `export` runs as `wingine ./test`.
        [command, rest @ ..] if command == "test" && !rest.is_empty() => return test(rest),
        [command, game, out] if command == "export" => return export(game, out),
        [flag, folder] if flag == "--watch" => runtime::run_watching(folder),
        [folder] if folder != "--watch" => runtime::run(folder),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Exports the game in `game` to the folder `out` with this executable, as `wingine export`
/// does, saying what it made and what might be wrong with it.
fn export(game: &OsString, out: &OsString) -> ExitCode {
    let exported = std::env::current_exe()
        .map_err(|err| format!("can't find this executable to copy: {err}"))
        .and_then(|executable| {
            runtime::export::export(game, out, executable).map_err(|err| err.to_string())
        });
    match exported {
        Ok(exported) => {
            for warning in &exported.warnings {
                eprintln!("warning: {warning}");
            }
            println!(
                "exported {} to {}, {} files of the game: players start {}",
                Path::new(game).display(),
                exported.folder.display(),
                exported.files,
                exported.executable.display()
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Runs a game's tests, as `wingine test` does with `args`, printing each as it finishes, then
/// the failures and a summary, as `cargo test` does.
fn test(mut args: &[OsString]) -> ExitCode {
    let (mut update_screenshots, mut ignored) = (false, false);
    while let [flag, rest @ ..] = args {
        if flag == "--update" {
            update_screenshots = true;
        } else if flag == "--ignored" {
            ignored = true;
        } else {
            break;
        }
        args = rest;
    }
    let (folder, filter) = match args {
        [folder] => (folder, None),
        [folder, filter] => (folder, Some(filter.to_string_lossy().into_owned())),
        _ => {
            eprintln!("{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    let options = testing::Options {
        filter,
        update_screenshots,
        ignored,
    };
    println!("running the tests of {}", Path::new(folder).display());
    let report = testing::run(folder, &options, |outcome| {
        let result = if outcome.passed() { "ok" } else { "FAILED" };
        println!("test {} ... {result}", outcome.title());
        for path in &outcome.written {
            println!("    saved {path}");
        }
    });
    match report {
        Ok(report) => {
            if report.outcomes.is_empty() && report.filtered_out == 0 && report.ignored == 0 {
                println!("no tests: they go in files named *.test.luau in the game's folder");
            }
            println!("\n{report}");
            if report.passed() {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

/// Logs to standard error: whatever scripts and native libraries print, the engine's warnings,
/// and errors from anywhere.
struct Logger;

impl log::Log for Logger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        let target = metadata.target();
        let level = if target == "lua" || target == "native" {
            log::Level::Info
        } else if target.starts_with("wingine") {
            log::Level::Warn
        } else {
            log::Level::Error
        };
        metadata.level() <= level
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{} {}] {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {}
}
