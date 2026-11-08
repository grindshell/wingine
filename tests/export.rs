//! Tests for exporting games to ship, end to end: `wingine export`, then the exported executable
//! started with nothing, as a player double-clicking it starts it, run headlessly by a script.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

use wingine::headless::Image;

/// A fresh folder for `test`, holding a game of `files`, by their paths in its `game` folder.
fn folder(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("export-tests")
        .join(test);
    if folder.exists() {
        fs::remove_dir_all(&folder).unwrap();
    }
    for (path, contents) in files {
        let path = folder.join("source").join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    folder
}

/// A game called `tiny`, its 20x20 canvas the window, so screenshots are canvas pixels.
const CONF: &str = "return {
    identity = 'tiny',
    width = 20,
    height = 20,
    resolution = { width = 20, height = 20 },
}";

/// Exports the game in `folder`'s `source` to its `shipped`, with the `wingine` built for tests.
fn export(folder: &Path) -> Output {
    let output = Command::new(env!("CARGO_BIN_EXE_wingine"))
        .arg("export")
        .arg(folder.join("source"))
        .arg(folder.join("shipped"))
        .output()
        .unwrap();
    let printed = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{printed}");
    output
}

/// Starts the exported executable in `folder` with no arguments, running a script that takes a
/// screenshot after a few frames, and returns the screenshot.
fn play(folder: &Path) -> Image {
    let script = folder.join("check/play.txt");
    fs::create_dir_all(script.parent().unwrap()).unwrap();
    fs::write(&script, "wait 3\nscreenshot shot.png\n").unwrap();
    let executable = folder
        .join("shipped")
        .join(format!("tiny{}", std::env::consts::EXE_SUFFIX));
    // From somewhere else, as a double-click starts it, so it finds the game beside itself.
    let output = Command::new(&executable)
        .current_dir(folder)
        .env("WINGINE_SCRIPT", &script)
        .output()
        .unwrap();
    let printed = String::from_utf8_lossy(&output.stdout).into_owned()
        + &String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{printed}");
    Image::load_png(folder.join("check/shot.png")).unwrap()
}

#[test]
fn exported_games_start_with_nothing_but_a_double_click() {
    let folder = folder(
        "plays",
        &[
            ("conf.luau", CONF),
            (
                "main.luau",
                "function draw() graphics.clear(require('./look').background) end",
            ),
            ("look.luau", "return { background = '#336699' }"),
            ("look.test.luau", "test('left out', function() end)"),
        ],
    );
    let output = export(&folder);
    let printed = String::from_utf8_lossy(&output.stdout);
    assert!(printed.contains("3 files of the game"), "{printed}");
    let game = folder.join("shipped/game");
    assert!(game.join("look.luau").is_file());
    assert!(!game.join("look.test.luau").exists());

    let shot = play(&folder);
    assert_eq!(shot.width(), 20);
    assert_eq!(shot.pixel(10, 10), [0x33, 0x66, 0x99, 0xff]);
}

#[test]
fn exported_games_show_errors_loading_them_in_their_windows() {
    // The color the engine shows errors on.
    const ERROR: [u8; 4] = [0x30, 0x10, 0x18, 0xff];
    let folder = folder(
        "broken",
        &[("conf.luau", CONF), ("main.luau", "error('no game today')")],
    );
    export(&folder);
    assert_eq!(play(&folder).pixel(1, 1), ERROR);

    // Settings that don't read show in a window with the default settings.
    let game = folder.join("shipped/game");
    fs::write(game.join("conf.luau"), "return { colour = 'red' }").unwrap();
    let shot = play(&folder);
    assert_eq!((shot.width(), shot.height()), (1280, 720));
    assert_eq!(shot.pixel(1, 1), ERROR);
}

#[test]
fn exports_refuse_folders_in_use() {
    let folder = folder("refused", &[("conf.luau", CONF), ("main.luau", "")]);
    fs::create_dir_all(folder.join("shipped")).unwrap();
    fs::write(folder.join("shipped/notes.txt"), "mine").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_wingine"))
        .arg("export")
        .arg(folder.join("source"))
        .arg(folder.join("shipped"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    let printed = String::from_utf8_lossy(&output.stderr);
    assert!(printed.contains("isn't an empty folder"), "{printed}");
    assert_eq!(
        fs::read_to_string(folder.join("shipped/notes.txt")).unwrap(),
        "mine"
    );
}
