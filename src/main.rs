//! The `wingine` executable, which runs a game written in Luau from its folder:
//!
//! ```text
//! wingine [--watch] path/to/game
//! ```
//!
//! With `--watch`, the game reloads whenever a file in its folder changes. See the `runtime`
//! module for what the folder holds.

use std::process::ExitCode;

fn main() -> ExitCode {
    static LOGGER: Logger = Logger;
    log::set_logger(&LOGGER).expect("nothing else sets a logger");
    log::set_max_level(log::LevelFilter::Info);

    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let result = match args.as_slice() {
        [flag, folder] if flag == "--watch" => wingine::runtime::run_watching(folder),
        [folder] if folder != "--watch" => wingine::runtime::run(folder),
        _ => {
            eprintln!("usage: wingine [--watch] <game folder>");
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
