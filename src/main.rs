//! The `wingine` executable, which runs a game written in Luau from its folder:
//!
//! ```text
//! wingine path/to/game
//! ```
//!
//! See the `runtime` module for what the folder holds.

use std::process::ExitCode;

fn main() -> ExitCode {
    static LOGGER: Logger = Logger;
    log::set_logger(&LOGGER).expect("nothing else sets a logger");
    log::set_max_level(log::LevelFilter::Info);

    let mut args = std::env::args_os().skip(1);
    let (Some(folder), None) = (args.next(), args.next()) else {
        eprintln!("usage: wingine <game folder>");
        return ExitCode::FAILURE;
    };
    match wingine::runtime::run(folder) {
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
