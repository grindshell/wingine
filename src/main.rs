//! The `wingine` executable, which runs a game written in Luau from its folder, or its tests:
//!
//! ```text
//! wingine [--watch] path/to/game
//! wingine test [--update] path/to/game [filter]
//! ```
//!
//! With `--watch`, the game reloads whenever a file in its folder changes. `test` runs the tests
//! in its `*.test.luau` files, or those whose files and names contain `filter`, and with
//! `--update`, saves screenshots that don't match as their references. See the `runtime` module
//! for what the folder holds, and `runtime::testing` for tests.

use std::{ffi::OsString, path::Path, process::ExitCode};

use wingine::runtime::testing;

const USAGE: &str =
    "usage: wingine [--watch] <game folder>\n       wingine test [--update] <game folder> [filter]";

fn main() -> ExitCode {
    static LOGGER: Logger = Logger;
    log::set_logger(&LOGGER).expect("nothing else sets a logger");
    log::set_max_level(log::LevelFilter::Info);

    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let result = match args.as_slice() {
        // A game in a folder called `test` runs as `wingine ./test`.
        [command, rest @ ..] if command == "test" && !rest.is_empty() => return test(rest),
        [flag, folder] if flag == "--watch" => wingine::runtime::run_watching(folder),
        [folder] if folder != "--watch" => wingine::runtime::run(folder),
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

/// Runs a game's tests, as `wingine test` does with `args`, printing each as it finishes, then
/// the failures and a summary, as `cargo test` does.
fn test(args: &[OsString]) -> ExitCode {
    let (update_screenshots, args) = match args {
        [flag, rest @ ..] if flag == "--update" => (true, rest),
        rest => (false, rest),
    };
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
            if report.outcomes.is_empty() && report.filtered_out == 0 {
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
