//! Tests for the runtime that runs games written in Luau, with small games written to temporary
//! folders.

use std::{
    fs,
    path::{Path, PathBuf},
};

use wingine::{
    Error,
    graphics::Resolution,
    headless::{Image, Simulation},
    input::Key,
    lua::mlua::FromLuaMulti,
    runtime::{self, LuauGame},
};

/// Writes a game made of `files`, each a path in the game's folder and its contents, to a fresh
/// folder named after `test`.
fn game(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let folder = std::env::temp_dir()
        .join("wingine-runtime-tests")
        .join(test);
    if folder.exists() {
        fs::remove_dir_all(&folder).unwrap();
    }
    for (path, contents) in files {
        let path = folder.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
    folder
}

/// A 20x20 canvas in a 20x20 window, so screenshots are canvas pixels.
const SMALL_WINDOW: &str =
    "return { width = 20, height = 20, resolution = { width = 20, height = 20 } }";

fn simulation(folder: &Path) -> Simulation<LuauGame> {
    let config = runtime::config(folder).expect("window settings load");
    Simulation::new(&config, |ctx| LuauGame::load(ctx, folder)).expect("game starts")
}

/// The value of the Luau `expression` in the game.
fn eval<T: FromLuaMulti>(simulation: &Simulation<LuauGame>, expression: &str) -> T {
    let lua = simulation.game().scripts().lua();
    lua.load(format!("return {expression}"))
        .eval()
        .unwrap_or_else(|err| panic!("`{expression}`: {err}"))
}

/// The error that stopped the game, which a test expects there to be.
fn stopped_with(simulation: &Simulation<LuauGame>) -> String {
    let err = simulation.game().error().expect("the game stopped");
    err.to_string()
}

fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
    let offset = ((y * image.width() + x) * 4) as usize;
    image.pixels()[offset..offset + 4].try_into().unwrap()
}

#[test]
fn window_settings_come_from_window_luau() {
    let folder = game(
        "window_settings",
        &[(
            "window.luau",
            "return {
                title = 'Settings',
                width = 640,
                height = 480,
                resizable = false,
                resolution = { width = 160, height = 120 },
            }",
        )],
    );
    let config = runtime::config(&folder).unwrap();
    assert_eq!(config.title, "Settings");
    assert_eq!([config.width, config.height], [640, 480]);
    assert!(!config.resizable);
    assert_eq!(
        config.resolution,
        Resolution::Fixed {
            width: 160,
            height: 120
        }
    );
}

#[test]
fn without_window_settings_the_title_is_the_folders_name() {
    let folder = game("untitled", &[("main.luau", "")]);
    let config = runtime::config(&folder).unwrap();
    assert_eq!(config.title, "untitled");
    assert_eq!(config.resolution, Resolution::Window);
}

#[test]
fn unknown_or_invalid_window_settings_are_errors() {
    let folder = game(
        "misspelled_setting",
        &[("window.luau", "return { titel = 'x' }")],
    );
    let err = runtime::config(&folder).unwrap_err().to_string();
    assert!(
        err.contains("`titel`: runtime error: unknown setting"),
        "{err}"
    );

    let folder = game(
        "empty_resolution",
        &[(
            "window.luau",
            "return { resolution = { width = 0, height = 10 } }",
        )],
    );
    let err = runtime::config(&folder).unwrap_err().to_string();
    assert!(err.contains("must be at least 1"), "{err}");
}

#[test]
fn update_and_tasks_run_each_frame_and_draw_draws() {
    let folder = game(
        "frames",
        &[
            ("window.luau", SMALL_WINDOW),
            (
                "main.luau",
                "frames = 0
                ticks = 0
                jumped = false
                input.bind('jump', { 'Space' })
                spawn(function()
                    while true do
                        ticks += 1
                        wait()
                    end
                end)

                function update(dt)
                    frames += 1
                    if input.pressed('jump') then
                        jumped = true
                    end
                end

                function draw()
                    graphics.clear('#ff0000')
                    graphics.fill_rect(0, 0, 10, 10, { r = 0, g = 0, b = 1 })
                end",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    simulation.run_frames(3);
    assert!(!eval::<bool>(&simulation, "jumped"));
    simulation.tap(Key::Space);
    assert!(eval::<bool>(&simulation, "jumped"));
    assert_eq!(eval::<u32>(&simulation, "frames"), 4);
    assert_eq!(
        eval::<u32>(&simulation, "ticks"),
        5,
        "once when spawned, then after each update"
    );

    let image = simulation.screenshot().unwrap();
    assert_eq!(pixel(&image, 5, 5), [0, 0, 255, 255]);
    assert_eq!(pixel(&image, 15, 15), [255, 0, 0, 255]);
}

#[test]
fn an_error_stops_the_game_and_shows_it() {
    let folder = game(
        "failing_update",
        &[
            ("window.luau", SMALL_WINDOW),
            (
                "main.luau",
                "frames = 0
                function update()
                    frames += 1
                    if frames == 2 then
                        error('out of cheese')
                    end
                end",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    simulation.run_frames(5);
    assert_eq!(
        eval::<u32>(&simulation, "frames"),
        2,
        "stopped at the error"
    );
    let err = stopped_with(&simulation);
    assert!(err.contains("main.luau:5: out of cheese"), "{err}");

    let image = simulation.screenshot().unwrap();
    assert_eq!(
        pixel(&image, 19, 19),
        [0x30, 0x10, 0x18, 255],
        "the error's background"
    );
}

#[test]
fn drawing_only_works_in_draw() {
    let folder = game(
        "drawing_in_update",
        &[(
            "main.luau",
            "function update() graphics.fill_rect(0, 0, 1, 1, '#ffffff') end",
        )],
    );
    let mut simulation = simulation(&folder);
    simulation.step();
    let err = stopped_with(&simulation);
    assert!(err.contains("drawing only works in `draw`"), "{err}");
}

#[test]
fn input_and_audio_do_not_work_in_draw() {
    let folder = game(
        "input_in_draw",
        &[("main.luau", "function draw() input.held('jump') end")],
    );
    let mut simulation = simulation(&folder);
    simulation.step();
    let err = stopped_with(&simulation);
    assert!(err.contains("not in `draw`"), "{err}");
}

#[test]
fn scripts_require_modules_relative_to_themselves() {
    let folder = game(
        "modules",
        &[
            (
                "main.luau",
                "local numbers = require('./lib/numbers')
                doubled = numbers.double(21)",
            ),
            (
                "lib/numbers.luau",
                "-- Modules see the engine's tables too.
                input.bind('confirm', { 'Enter' })
                return { double = function(n) return n * 2 end }",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    assert_eq!(eval::<u32>(&simulation, "doubled"), 42);
    simulation.tap(Key::Enter);
    assert!(simulation.context().input().released("confirm"));
}

#[test]
fn sounds_load_from_the_game_folder() {
    let folder = game(
        "sounds",
        &[(
            "main.luau",
            "local blip = audio.load('sounds/blip.wav')
            length = blip:duration()
            function update()
                if input.key_pressed('Space') then
                    audio.play(blip, { volume = 0.5 })
                end
            end",
        )],
    );
    let sounds = folder.join("sounds");
    fs::create_dir_all(&sounds).unwrap();
    let blip = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/sounds/blip.wav");
    fs::copy(blip, sounds.join("blip.wav")).unwrap();

    let mut simulation = simulation(&folder);
    assert!(eval::<f32>(&simulation, "length") > 0.0);
    simulation.tap(Key::Space);
    assert_eq!(simulation.context().audio().sounds_playing(), 1);
}

#[test]
fn fonts_measure_and_scale() {
    let folder = game(
        "fonts",
        &[(
            "main.luau",
            "local font = graphics.default_font()
            width, height = font:measure('Hi')
            big_width = font:scaled(2):measure('Hi')
            line_height = font:line_height()
            zero_scale_fails = not pcall(function() font:scaled(0) end)",
        )],
    );
    let simulation = simulation(&folder);
    let (width, height): (f32, f32) = eval(&simulation, "width, height");
    assert!(width > 0.0);
    assert_eq!(height, eval::<f32>(&simulation, "line_height"));
    assert_eq!(eval::<f32>(&simulation, "big_width"), width * 2.0);
    assert!(eval::<bool>(&simulation, "zero_scale_fails"));
}

#[test]
fn games_that_fail_to_load_are_errors() {
    let config = wingine::WindowConfig::default();
    let missing = game("no_main", &[]);
    let result = Simulation::new(&config, |ctx| LuauGame::load(ctx, &missing));
    assert!(matches!(result, Err(Error::ReadFile { .. })));

    let failing = game("failing_main", &[("main.luau", "audio.load('nope.wav')")]);
    let result = Simulation::new(&config, |ctx| LuauGame::load(ctx, &failing));
    let err = result.err().expect("loading fails").to_string();
    assert!(err.contains("nope.wav"), "{err}");
}
