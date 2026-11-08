//! The `wingine-editor` executable, which edits the game in a folder:
//!
//! ```text
//! wingine-editor [path/to/game]
//! ```
//!
//! With `--run path/to/game`, it runs the game instead, as `wingine --watch` does, reloading it
//! whenever a file in its folder changes: that's how the editor's Play runs a game, in a process
//! of its own, so saving in the editor reloads it.

use std::{path::PathBuf, process::ExitCode};

use wingine::{Redraw, WindowConfig};
use wingine_editor::{Editor, docks::output::Logger};

fn main() -> ExitCode {
    static LOGGER: Logger = Logger;
    log::set_logger(&LOGGER).expect("nothing else sets a logger");
    log::set_max_level(log::LevelFilter::Info);

    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let result = match args.as_slice() {
        [flag, folder] if flag == "--run" => wingine::runtime::run_watching(folder),
        [] => run(None),
        [folder] => run(Some(PathBuf::from(folder))),
        _ => {
            eprintln!("usage: wingine-editor [game folder]");
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

fn run(folder: Option<PathBuf>) -> wingine::Result<()> {
    let config = WindowConfig {
        title: "Wingine Editor".into(),
        width: 1600,
        height: 900,
        redraw: Redraw::OnDemand,
        ..Default::default()
    };
    wingine::run(config, |ctx| Ok(Editor::new(ctx, folder)))
}
