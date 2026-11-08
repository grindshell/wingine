//! Tests for the runtime that runs games written in Luau, with small games written to temporary
//! folders.

use std::{
    fs,
    path::{Path, PathBuf},
};

use wingine::{
    Error,
    graphics::Resolution,
    headless::Simulation,
    input::{GamepadButton, Key, MouseButton, Stick},
    runtime::{self, LibraryConfig, LuauGame},
};

/// Writes a game made of `files`, each a path in the game's folder and its contents, to a fresh
/// folder named after `test`.
fn game(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("runtime-tests")
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
    let config = runtime::config(folder).expect("settings load");
    Simulation::new(&config.window, |ctx| {
        LuauGame::load_with(ctx, folder, &config)
    })
    .expect("game starts")
}

/// The error that stopped the game, which a test expects there to be.
fn stopped_with(simulation: &Simulation<LuauGame>) -> String {
    let err = simulation.game().error().expect("the game stopped");
    err.to_string()
}

#[test]
fn settings_come_from_conf_luau() {
    let folder = game(
        "settings",
        &[(
            "conf.luau",
            "return {
                title = 'Settings',
                width = 640,
                height = 480,
                resizable = false,
                resolution = { width = 160, height = 120 },
                vsync = false,
                update_rate = 60,
                debug_key = 'F3',
                identity = 'settings-test',
                libraries = { 'physics', { name = 'particle_fx', optional = true } },
            }",
        )],
    );
    let config = runtime::config(&folder).unwrap();
    let window = &config.window;
    assert_eq!(window.title, "Settings");
    assert_eq!([window.width, window.height], [640, 480]);
    assert!(!window.resizable);
    assert!(!window.vsync);
    assert_eq!(window.update_rate, Some(60.0));
    assert_eq!(window.debug_key, Some(Key::F3));
    assert_eq!(window.identity, "settings-test");
    assert_eq!(
        window.resolution,
        Resolution::Fixed {
            width: 160,
            height: 120
        }
    );
    let library = |name: &str, optional| LibraryConfig {
        name: name.into(),
        optional,
    };
    assert_eq!(
        config.libraries,
        [library("physics", false), library("particle_fx", true)]
    );
}

#[test]
fn without_settings_the_title_is_the_folders_name() {
    let folder = game("untitled", &[("main.luau", "")]);
    let config = runtime::config(&folder).unwrap();
    assert_eq!(config.window.title, "untitled");
    assert_eq!(config.window.identity, "untitled");
    assert_eq!(config.window.resolution, Resolution::Window);
    assert!(config.window.vsync);
    assert_eq!(config.window.update_rate, None);
    assert!(config.libraries.is_empty());

    // An identity names a folder, so it keeps to characters any system takes.
    let folder = game("Café Quest 2!", &[("main.luau", "")]);
    let config = runtime::config(&folder).unwrap();
    assert_eq!(config.window.title, "Café Quest 2!");
    assert_eq!(config.window.identity, "Caf__Quest_2_");
}

#[test]
fn scripts_see_the_stats_of_the_frame_before() {
    let folder = game(
        "frame_stats",
        &[(
            "main.luau",
            "loading = graphics.frame_stats()
            seen = {}
            function update()
                table.insert(seen, graphics.frame_stats())
            end
            function draw()
                drawing = graphics.frame_stats()
                graphics.fill_rect(0, 0, 1, 1, '#ffffff')
                graphics.fill_rect(1, 1, 1, 1, '#ffffff')
            end",
        )],
    );
    let mut simulation = simulation(&folder);
    simulation.run_frames(2);
    assert_eq!(simulation.eval::<u32>("loading.quads"), 0);
    assert_eq!(simulation.eval::<f32>("seen[1].frame"), 0.0);
    assert_eq!(
        simulation.eval::<(u32, u32)>("seen[2].quads, seen[2].draw_calls"),
        (2, 1)
    );
    assert!(simulation.eval::<f32>("seen[2].frame") > 0.0);
    assert_eq!(simulation.eval::<u32>("drawing.quads"), 2);
}

#[test]
fn frames_without_updates_draw_with_the_stats_of_the_frame_before() {
    let folder = game(
        "frame_stats_between_updates",
        &[
            // Half as many updates as frames, so every other frame has none.
            ("conf.luau", "return { update_rate = 30 }"),
            (
                "main.luau",
                "seen = {}
                function draw()
                    table.insert(seen, graphics.frame_stats().quads)
                    for _ = 1, #seen do
                        graphics.fill_rect(0, 0, 1, 1, '#ffffff')
                    end
                end",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    simulation.run_frames(4);
    assert_eq!(simulation.eval::<Vec<u32>>("seen"), [0, 1, 2, 3]);
}

#[test]
fn unknown_or_invalid_settings_are_errors() {
    let folder = game(
        "misspelled_setting",
        &[("conf.luau", "return { titel = 'x' }")],
    );
    let err = runtime::config(&folder).unwrap_err().to_string();
    assert!(
        err.contains("`titel`: runtime error: unknown setting"),
        "{err}"
    );

    let folder = game(
        "empty_resolution",
        &[(
            "conf.luau",
            "return { resolution = { width = 0, height = 10 } }",
        )],
    );
    let err = runtime::config(&folder).unwrap_err().to_string();
    assert!(err.contains("must be at least 1"), "{err}");

    for (test, conf, message) in [
        (
            "library_path",
            "return { libraries = { '../physics' } }",
            "`../physics` is not a library name",
        ),
        (
            "library_setting",
            "return { libraries = { { name = 'physics', optinal = true } } }",
            "not `optinal`",
        ),
        (
            "zero_update_rate",
            "return { update_rate = 0 }",
            "`update_rate`: runtime error: must be above 0",
        ),
        (
            "debug_key",
            "return { debug_key = 'Hyper' }",
            "unknown key `Hyper`",
        ),
        (
            "identity",
            "return { identity = '../elsewhere' }",
            "`identity`: runtime error: use only letters, digits",
        ),
    ] {
        let folder = game(test, &[("conf.luau", conf)]);
        let err = runtime::config(&folder).unwrap_err().to_string();
        assert!(err.contains(message), "{err}");
    }
}

#[test]
fn an_update_rate_updates_by_the_same_dt() {
    let folder = game(
        "update_rate",
        &[
            ("conf.luau", "return { update_rate = 30 }"),
            (
                "main.luau",
                "updates, draws, dts = 0, 0, {}
                function update(dt)
                    updates += 1
                    dts[dt] = true
                end
                function draw() draws += 1 end",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    simulation.run_frames(4);
    assert_eq!(simulation.eval::<(u32, u32)>("updates, draws"), (2, 4));
    let dt: f32 = simulation.eval("next(dts)");
    assert!((dt - 1.0 / 30.0).abs() < 1e-6, "{dt}");
    assert!(simulation.eval::<bool>("next(dts, next(dts)) == nil"));
}

#[test]
fn update_and_tasks_run_each_frame_and_draw_draws() {
    let folder = game(
        "frames",
        &[
            ("conf.luau", SMALL_WINDOW),
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
    assert!(!simulation.eval::<bool>("jumped"));
    simulation.tap(Key::Space);
    assert!(simulation.eval::<bool>("jumped"));
    assert_eq!(simulation.eval::<u32>("frames"), 4);
    assert_eq!(
        simulation.eval::<u32>("ticks"),
        5,
        "once when spawned, then after each update"
    );

    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(5, 5), [0, 0, 255, 255]);
    assert_eq!(image.pixel(15, 15), [255, 0, 0, 255]);
}

#[test]
fn tasks_started_in_update_resume_on_the_next_frame() {
    let folder = game(
        "spawn_in_update",
        &[(
            "main.luau",
            "frames = 0
            resumed_on = nil
            function update()
                frames += 1
                if frames == 2 then
                    spawn(function()
                        wait()
                        resumed_on = frames
                    end)
                end
            end",
        )],
    );
    let mut simulation = simulation(&folder);
    simulation.run_frames(2);
    assert_eq!(simulation.eval::<Option<u32>>("resumed_on"), None);
    simulation.step();
    assert_eq!(simulation.eval::<Option<u32>>("resumed_on"), Some(3));
}

#[test]
fn a_task_failing_as_it_starts_does_not_stop_the_game() {
    let folder = game(
        "spawn_failing",
        &[(
            "main.luau",
            "frames = 0
            function update()
                frames += 1
                if frames == 1 then
                    spawn(function() error('lost the map') end)
                end
            end",
        )],
    );
    let mut simulation = simulation(&folder);
    simulation.run_frames(3);
    assert!(simulation.game().error().is_none());
    assert_eq!(simulation.eval::<u32>("frames"), 3);
}

#[test]
fn an_error_stops_the_game_and_shows_it() {
    let folder = game(
        "failing_update",
        &[
            ("conf.luau", SMALL_WINDOW),
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
    assert_eq!(simulation.eval::<u32>("frames"), 2, "stopped at the error");
    let err = stopped_with(&simulation);
    assert!(err.contains("main.luau:5: out of cheese"), "{err}");

    let image = simulation.screenshot().unwrap();
    assert_eq!(
        image.pixel(19, 19),
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
fn scripts_quit_the_game() {
    let folder = game(
        "quitting",
        &[(
            "main.luau",
            "input.bind('quit', { 'Escape' })
            function update()
                if input.pressed('quit') then
                    window.quit()
                end
            end",
        )],
    );
    let mut quitting = simulation(&folder);
    quitting.step();
    assert!(!quitting.has_quit());
    quitting.tap(Key::Escape);
    assert!(quitting.has_quit());

    // The window closes after the frame has drawn, so quitting while drawing would be too late.
    let folder = game(
        "quitting_in_draw",
        &[("main.luau", "function draw() window.quit() end")],
    );
    let mut drawing = simulation(&folder);
    drawing.step();
    let err = stopped_with(&drawing);
    assert!(err.contains("not in `draw`"), "{err}");
    assert!(!drawing.has_quit());
}

#[test]
fn close_requested_decides_whether_the_window_closes() {
    let folder = game("closing_unasked", &[("main.luau", "")]);
    assert!(simulation(&folder).request_close(), "without asking");

    let folder = game(
        "closing_asked",
        &[(
            "main.luau",
            "asked = 0
            willing = false
            input.bind('yes', { 'Y' })
            function close_requested()
                asked += 1
                -- What's lent to `update` is lent here too.
                input.held('yes')
                if willing then
                    return nil
                end
                return false
            end
            function update()
                if asked > 0 and input.pressed('yes') then
                    window.quit()
                end
            end",
        )],
    );
    let mut asking = simulation(&folder);
    assert!(!asking.request_close(), "false keeps it open");
    assert_eq!(asking.eval::<u32>("asked"), 1);
    asking.step();
    assert!(!asking.has_quit());
    asking.tap(Key::Y);
    assert!(asking.has_quit(), "the game closes it once answered");
    asking.exec("(function() willing = true end)()");
    assert!(asking.request_close(), "anything but false closes it");

    // A game that can't answer can't keep its window open.
    let folder = game(
        "closing_failing",
        &[("main.luau", "function close_requested() error('no') end")],
    );
    let mut failing = simulation(&folder);
    assert!(failing.request_close());
    let err = stopped_with(&failing);
    assert!(err.contains("main.luau:1: no"), "{err}");
    assert!(failing.request_close(), "nor once it has stopped");
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
    assert_eq!(simulation.eval::<u32>("doubled"), 42);
    simulation.tap(Key::Enter);
    assert!(simulation.context().input().released("confirm"));
}

#[test]
fn tests_run_luau_in_the_game_between_frames() {
    let folder = game(
        "test_chunks",
        &[
            (
                "main.luau",
                "local counter = require('./lib/counter')
                function update()
                    counter.add(1)
                end",
            ),
            (
                "lib/counter.luau",
                "local counter = { count = 0 }
                function counter.add(n)
                    counter.count += n
                end
                return counter",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    simulation.run_frames(2);

    // A test's chunk requires from the game's folder, getting the module the game has.
    simulation.exec("require('./lib/counter').add(10)");
    assert_eq!(simulation.eval::<u32>("require('./lib/counter').count"), 12);
    simulation.step();
    assert_eq!(simulation.eval::<u32>("require('./lib/counter').count"), 13);

    // It's lent the context, so the engine's functions work in it.
    simulation.exec("input.bind('jump', { 'Space' })");
    simulation.key_down(Key::Space);
    assert!(simulation.eval::<bool>("input.held('jump')"));
    assert_eq!(
        simulation.eval::<(f32, f32)>("graphics.size()"),
        (1280.0, 720.0)
    );

    // Its errors say where they are and what failed, without stopping the game.
    let err = simulation
        .try_exec("local t = nil\nreturn t.x")
        .unwrap_err();
    assert!(err.to_string().contains("test:2:"), "{err}");
    let err = simulation
        .try_exec("graphics.clear('#000000')")
        .unwrap_err();
    assert!(err.to_string().contains("only works in `draw`"), "{err}");
    simulation.step();
    assert!(simulation.game().error().is_none());
}

#[test]
fn held_keys_repeat_for_scripts() {
    let folder = game(
        "key_repeat",
        &[(
            "main.luau",
            "input.bind('east', { 'L', 'DPadRight' })
            steps = 0
            repeats = 0
            function update()
                if input.pressed('east') or input.repeated('east') then
                    steps += 1
                end
                if input.key_repeated('L') then
                    repeats += 1
                end
            end",
        )],
    );
    let mut simulation = simulation(&folder);
    // Pressed, then repeated after half a second, 30 times a second.
    simulation.hold(Key::L, 1.0);
    assert_eq!(simulation.eval::<u32>("steps"), 16);
    assert_eq!(simulation.eval::<u32>("repeats"), 15);

    // The D-pad doesn't repeat.
    simulation.connect_gamepad("Pad");
    simulation.hold_button(1, GamepadButton::DPadRight, 1.0);
    assert_eq!(simulation.eval::<u32>("steps"), 17);
}

#[test]
fn scripts_read_what_is_typed() {
    let folder = game(
        "typed",
        &[(
            "main.luau",
            "typed = ''
            composing = ''
            function update()
                typed ..= input.text()
                composing = input.composition()
                if input.key_pressed('Tab') then
                    input.set_input_method({ x = 2, y = 2, width = 40, height = 10 })
                end
            end",
        )],
    );
    let mut simulation = simulation(&folder);
    simulation.key_down(Key::LeftShift);
    simulation.tap(Key::Period);
    simulation.key_up(Key::LeftShift);
    simulation.type_text("ß");
    simulation.step();
    assert_eq!(simulation.eval::<String>("typed"), ">ß");

    simulation.tap(Key::Tab);
    simulation.compose("にほ");
    simulation.step();
    assert_eq!(simulation.eval::<String>("composing"), "にほ");
    simulation.type_text("日本");
    simulation.step();
    assert_eq!(simulation.eval::<String>("typed"), ">ß日本");
    assert_eq!(simulation.eval::<String>("composing"), "");
}

#[test]
fn gamepads_are_read_by_number_and_through_actions() {
    let folder = game(
        "gamepads",
        &[(
            "main.luau",
            "input.bind('jump', { 'Space', 'South' })
            input.bind('p2_jump', { 'South' }, 2)
            input.bind('left', { 'A', 'LeftStickLeft' })
            input.bind('right', { 'D', 'LeftStickRight' })
            input.bind('shield', { 'RightTrigger' })
            local _, err = pcall(input.bind, 'dodge', { 'Shift' })
            bind_error = tostring(err)
            seen = nil
            function update()
                seen = {
                    gamepads = input.gamepads(),
                    name = input.gamepad_name(1),
                    unknown = input.gamepad_name(3),
                    jump = input.pressed('jump'),
                    p2_jump = input.held('p2_jump'),
                    axis = input.axis('left', 'right'),
                    shield = input.strength('shield'),
                    shield_held = input.held('shield'),
                    south_on_1 = input.button_pressed('South', 1),
                    south_on_2 = input.button_held('South', 2),
                    south_released = input.button_released('South'),
                    trigger = input.button_strength('rightTrigger'),
                }
            end",
        )],
    );
    let mut simulation = simulation(&folder);
    assert!(
        simulation
            .eval::<String>("bind_error")
            .contains("unknown key or gamepad button `Shift`")
    );
    let one = simulation.connect_gamepad("Left Hand");
    let two = simulation.connect_gamepad("Right Hand");
    assert_eq!((one, two), (1, 2));

    simulation.button_down(1, GamepadButton::South);
    simulation.push_button(1, GamepadButton::RightTrigger, 0.3);
    simulation.move_stick(1, Stick::Left, -0.55, 0.0);
    simulation.step();
    assert_eq!(simulation.eval::<Vec<u32>>("seen.gamepads"), [1, 2]);
    assert_eq!(simulation.eval::<String>("seen.name"), "Left Hand");
    assert!(simulation.eval::<Option<String>>("seen.unknown").is_none());
    assert!(simulation.eval::<bool>("seen.jump"));
    assert!(simulation.eval::<bool>("seen.south_on_1"));
    assert!(
        !simulation.eval::<bool>("seen.p2_jump"),
        "gamepad 1 isn't 2"
    );
    assert!(!simulation.eval::<bool>("seen.south_on_2"));
    let axis: f64 = simulation.eval("seen.axis");
    assert!((axis + 0.5).abs() < 1e-6, "{axis}");
    assert!((simulation.eval::<f64>("seen.shield") - 0.3).abs() < 1e-6);
    assert!(!simulation.eval::<bool>("seen.shield_held"));
    assert!((simulation.eval::<f64>("seen.trigger") - 0.3).abs() < 1e-6);

    simulation.button_down(2, GamepadButton::South);
    simulation.disconnect_gamepad(1);
    simulation.step();
    assert_eq!(simulation.eval::<Vec<u32>>("seen.gamepads"), [2]);
    assert!(simulation.eval::<bool>("seen.p2_jump"));
    assert!(
        simulation.eval::<bool>("seen.south_released"),
        "disconnecting releases it"
    );
    assert_eq!(simulation.eval::<f64>("seen.axis"), 0.0);
}

/// The fields of the table `name` declares in the definitions file, a line each.
fn declared_fields(name: &str) -> Vec<String> {
    let start = format!("\ndeclare {name}: {{\n");
    // As checked out, which may have turned line endings into CRLF.
    let definitions = runtime::DEFINITIONS.replace("\r\n", "\n");
    let block = definitions
        .split_once(&start)
        .and_then(|(_, rest)| rest.split_once("\n}"))
        .unwrap_or_else(|| panic!("the definitions declare `{name}`"))
        .0;
    let mut fields: Vec<String> = block
        .lines()
        .filter_map(|line| line.strip_prefix("    "))
        .filter(|line| !line.starts_with("--"))
        .filter_map(|line| line.split_once(": ").map(|(name, _)| name))
        .map(str::to_owned)
        .collect();
    fields.sort();
    fields
}

#[test]
fn the_definitions_declare_every_function_of_the_engine() {
    let folder = game(
        "definitions",
        &[(
            "main.luau",
            "local tables = { graphics = graphics, input = input, audio = audio,
                tilemap = tilemap, grid = grid, physics = physics, scene = scene, save = save,
                exact = exact, rect = rect, window = window, ui = ui }
            names = {}
            for table_name, functions in tables do
                local found = {}
                for name in functions do
                    table.insert(found, name)
                end
                table.sort(found)
                names[table_name] = found
            end",
        )],
    );
    let mut simulation = simulation(&folder);
    let tables = [
        "graphics", "input", "audio", "tilemap", "grid", "physics", "scene", "save", "exact",
        "rect", "window", "ui",
    ];
    for table in tables {
        let functions: Vec<String> = simulation.eval(&format!("names.{table}"));
        assert_eq!(declared_fields(table), functions, "the fields of `{table}`");
    }
    for built_in in ["spawn", "wait", "wait_until", "approach"] {
        assert_eq!(
            simulation.eval::<String>(&format!("type({built_in})")),
            "function"
        );
        let declared = format!("\ndeclare function {built_in}");
        assert!(runtime::DEFINITIONS.contains(&declared), "{built_in}");
    }
}

/// What `library`, `exact` or `math`, gives for a spread of arguments to each of its functions,
/// hashed together bit for bit.
fn math_fingerprint(simulation: &mut Simulation<LuauGame>, library: &str) -> u64 {
    let results: Vec<f64> = simulation.eval(&format!(
        "(function()
                local m = {library}
                local results = {{}}
                for i = -500, 500 do
                    local x = i * 0.0137 + 0.001
                    local unit = i / 500.5
                    local positive = (i + 501) * 0.731
                    for _, value in {{
                        m.sin(x), m.cos(x), m.tan(x), m.asin(unit), m.acos(unit), m.atan(x),
                        m.atan2(x, unit), m.sinh(x), m.cosh(x), m.tanh(x), m.exp(x),
                        m.log(positive), m.log(positive, 3), m.log(positive, 2),
                        m.log10(positive), m.pow(positive, x),
                    }} do
                        table.insert(results, value)
                    end
                end
                return results
            end)()"
    ));
    assert_eq!(results.len(), 1001 * 16);
    // FNV-1a over each result's bits.
    results.iter().fold(0xcbf2_9ce4_8422_2325, |hash, value| {
        (hash ^ value.to_bits()).wrapping_mul(0x100_0000_01b3)
    })
}

#[test]
fn exact_math_gives_the_same_bits_on_every_platform() {
    let folder = game("exact_math", &[("main.luau", "")]);
    let mut simulation = simulation(&folder);
    // CI checks it on Windows and Linux. `math`'s comes out differently on each, from their C
    // libraries.
    assert_eq!(
        math_fingerprint(&mut simulation, "exact"),
        0x0c4f_400f_fe58_5058
    );
    assert_eq!(
        simulation.eval::<(f64, f64, f64)>("exact.sin(0), exact.cos(0), exact.log(8, 2)"),
        (0.0, 1.0, 3.0)
    );
}

#[test]
fn saves_last_after_the_game_ends() {
    let folder = game(
        "saves",
        &[("main.luau", "progress = save.read('progress')")],
    );
    let mut config = runtime::config(&folder).unwrap();
    let saves = folder.join("saves");
    config.window.save_folder = Some(saves.clone());
    let start = || {
        Simulation::new(&config.window, |ctx| {
            LuauGame::load_with(ctx, &folder, &config)
        })
        .expect("game starts")
    };

    let mut first = start();
    assert!(first.eval::<bool>("progress == nil"));
    assert!(first.eval::<Vec<String>>("save.list()").is_empty());
    first.exec(
        "save.write('progress', {
            stage = 'hunt',
            coins = 12.5,
            items = { 'herb', 'tonic' },
            gear = { weapon = 'staff' },
            done = false,
            ['found places'] = {},
        })",
    );
    first.exec("save.write('best', 900)");
    assert_eq!(
        first.eval::<Vec<String>>("save.list()"),
        ["best", "progress"]
    );
    drop(first);

    // Written as scene files are, a field to a line.
    let text = fs::read_to_string(saves.join("progress.luau")).unwrap();
    assert!(text.starts_with("return {\n"), "{text}");
    assert!(text.contains("\n    stage = \"hunt\",\n"), "{text}");
    assert!(text.contains("\n    [\"found places\"] = {},\n"), "{text}");

    let mut second = start();
    assert_eq!(
        second.eval::<(String, f64, String, String, bool)>(
            "progress.stage, progress.coins, progress.items[2], progress.gear.weapon, \
             progress.done"
        ),
        ("hunt".into(), 12.5, "tonic".into(), "staff".into(), false)
    );
    assert!(second.eval::<bool>("next(progress['found places']) == nil"));
    assert_eq!(second.eval::<u32>("save.read('best')"), 900);
    second.exec("save.remove('best')");
    second.exec("save.remove('never_saved')");
    assert_eq!(second.eval::<Vec<String>>("save.list()"), ["progress"]);
    assert!(second.eval::<bool>("save.read('best') == nil"));
}

#[test]
fn only_plain_data_can_be_saved() {
    let folder = game("unplain_saves", &[("main.luau", "")]);
    let mut simulation = simulation(&folder);
    for (code, message) in [
        (
            "save.write('slot', function() end)",
            "save `slot`: the data is a function",
        ),
        (
            "save.write('slot', { levels = { village = { f = print } } })",
            "`levels.village.f` is a function",
        ),
        ("save.write('slot', nil)", "the data is nil"),
        ("save.write('slot', 0 / 0)", "must be a finite number"),
        ("save.write('slot', { 1, nil, 3 })", "but not 1, 2, 3"),
        (
            "save.write('slot', { 1, a = 2 })",
            "both strings and numbers",
        ),
        ("local t = {} t.t = t save.write('slot', t)", "too deep"),
        (
            "save.write('../slot', 1)",
            "save `../slot`: a save's name may hold only letters",
        ),
        ("save.read('slot 2')", "may hold only letters"),
    ] {
        let err = simulation.try_exec(code).unwrap_err().to_string();
        assert!(err.contains(message), "`{code}`: {err}");
    }
    assert!(simulation.eval::<Vec<String>>("save.list()").is_empty());
}

#[test]
fn simulations_keep_saves_of_their_own() {
    let folder = game("own_saves", &[("main.luau", "")]);
    let mut first = simulation(&folder);
    let mut second = simulation(&folder);
    first.exec("save.write('slot', 1)");
    assert!(second.eval::<bool>("save.read('slot') == nil"));
    let saves = first.context().saves().folder().unwrap().to_owned();
    assert!(saves.join("slot.luau").is_file());
    drop(first);
    assert!(!saves.exists());
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
    assert!(simulation.eval::<f32>("length") > 0.0);
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
    let mut simulation = simulation(&folder);
    let (width, height): (f32, f32) = simulation.eval("width, height");
    assert!(width > 0.0);
    assert_eq!(height, simulation.eval::<f32>("line_height"));
    assert_eq!(simulation.eval::<f32>("big_width"), width * 2.0);
    assert!(simulation.eval::<bool>("zero_scale_fails"));
}

/// Writes a PNG `width` by `height` pixels to `path`, with `pixels` row by row.
fn write_png(path: &Path, width: u32, height: u32, pixels: &[[u8; 4]]) {
    let image = image::RgbaImage::from_raw(width, height, pixels.concat()).unwrap();
    image.save(path).unwrap();
}

const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];

#[test]
fn sprites_load_from_sheets_and_draw() {
    let folder = game(
        "sprites",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                "local sheet = graphics.load_sprite_sheet('art/sheet.png', 1, 1)
                columns, rows = sheet:size()
                local red, blue = sheet:sprite(0, 0), sheet:sprite(1, 0)
                width, height = blue:size()
                outside_fails = not pcall(function() sheet:sprite(2, 0) end)
                empty_tiles_fail = not pcall(function()
                    graphics.load_sprite_sheet('art/sheet.png', 0, 1)
                end)

                function draw()
                    graphics.clear('#000000')
                    -- Stretched, then at its own size, mirrored and tinted.
                    graphics.draw_sprite(blue, 0, 0, nil, 10, 20)
                    graphics.draw_sprite(red:flipped_x(), 12, 0, '#ffffff')
                end",
            ),
        ],
    );
    fs::create_dir_all(folder.join("art")).unwrap();
    write_png(&folder.join("art/sheet.png"), 2, 1, &[RED, BLUE]);

    let mut simulation = simulation(&folder);
    assert_eq!(simulation.eval::<(u32, u32)>("columns, rows"), (2, 1));
    assert_eq!(simulation.eval::<(f32, f32)>("width, height"), (1.0, 1.0));
    assert!(simulation.eval::<bool>("outside_fails"));
    assert!(simulation.eval::<bool>("empty_tiles_fail"));

    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(9, 19), BLUE);
    assert_eq!(image.pixel(10, 19), BLACK);
    assert_eq!(image.pixel(12, 0), RED);
    assert_eq!(image.pixel(13, 0), BLACK);
}

#[test]
fn sprites_draw_rotated_about_their_centers() {
    let folder = game(
        "rotated",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                "local sheet = graphics.load_sprite_sheet('sheet.png', 1, 1)
                local red, blue = sheet:sprite(0, 0), sheet:sprite(1, 0)
                function draw()
                    graphics.clear('#000000')
                    -- 8x2, standing up as 2x8 about its center, (4, 5).
                    graphics.draw_sprite_rotated(blue, 0, 4, math.pi / 2, nil, 8, 2)
                    graphics.draw_sprite_rotated(red, 12, 0, 0, '#ffffff')
                end",
            ),
        ],
    );
    write_png(&folder.join("sheet.png"), 2, 1, &[RED, BLUE]);

    let mut simulation = simulation(&folder);
    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(3, 1), BLUE);
    assert_eq!(image.pixel(4, 8), BLUE);
    assert_eq!(image.pixel(0, 4), BLACK);
    assert_eq!(image.pixel(5, 5), BLACK);
    assert_eq!(image.pixel(12, 0), RED);
    assert_eq!(image.pixel(13, 0), BLACK);
}

#[test]
fn colors_can_be_made_numbers() {
    let folder = game(
        "color_numbers",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                "hex = graphics.color('#ff804080')
                table = graphics.color({ r = 0, g = 0, b = 1 })
                function draw()
                    graphics.clear(0x000000ff)
                    graphics.fill_rect(0, 0, 1, 1, table)
                end",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    assert_eq!(
        simulation.eval::<(u32, u32)>("hex, table"),
        (0xff804080, 0x0000ffff)
    );
    // An opacity multiplies the alpha, keeping it from 0 to 1.
    let faded = [
        ("0xf2c14eff, 0.5", 0xf2c14e80),
        ("'#00000080', 0.5", 0x00000040),
        ("{ r = 1, g = 1, b = 1, a = 0.25 }, 2", 0xffffff80),
        ("'#ffffff', 2", 0xffffffff),
        ("'#ffffff', -1", 0xffffff00),
        ("0x12345678, 1", 0x12345678),
    ];
    for (arguments, color) in faded {
        let found = simulation.eval::<u32>(&format!("graphics.color({arguments})"));
        assert_eq!(found, color, "{arguments}: {found:08x}");
    }
    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(0, 0), BLUE);
    assert_eq!(image.pixel(1, 0), BLACK);
}

#[test]
fn animations_show_their_frames_in_turn() {
    let folder = game(
        "animations",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                "local sheet = graphics.load_sprite_sheet('sheet.png', 1, 1)
                local red, blue = sheet:sprite(0, 0), sheet:sprite(1, 0)
                blink = graphics.new_animation({ red, blue, red }, 0.25)
                once = blink:once()
                mirrored = blink:flipped_x():flipped_y()
                no_frames_fail = not pcall(graphics.new_animation, {}, 0.25)
                no_time_fails = not pcall(graphics.new_animation, { red }, 0)
                local time = 0

                function update(dt)
                    time += dt
                end

                function draw()
                    graphics.draw_sprite(blink:frame(time), 0, 0)
                end",
            ),
        ],
    );
    write_png(&folder.join("sheet.png"), 2, 1, &[RED, BLUE]);

    let mut simulation = simulation(&folder);
    let indices = "blink:frame_index(0), blink:frame_index(0.3), blink:frame_index(0.6), \
                   blink:frame_index(0.8)";
    assert_eq!(
        simulation.eval::<(u32, u32, u32, u32)>(indices),
        (1, 2, 3, 1)
    );
    assert_eq!(simulation.eval::<u32>("once:frame_index(10)"), 3);
    assert_eq!(
        simulation.eval::<(u32, f32)>("blink:frame_count(), blink:duration()"),
        (3, 0.75)
    );
    assert_eq!(simulation.eval::<u32>("mirrored:frame_count()"), 3);
    assert!(simulation.eval::<bool>(
        "once:is_finished(0.75) and not once:is_finished(0.7) and not blink:is_finished(100)"
    ));
    assert!(simulation.eval::<bool>("no_frames_fail and no_time_fails"));

    simulation.step();
    assert_eq!(simulation.screenshot().unwrap().pixel(0, 0), RED);
    simulation.run_for(0.3);
    assert_eq!(simulation.screenshot().unwrap().pixel(0, 0), BLUE);
}

#[test]
fn sprite_sheets_only_load_while_the_game_loads_or_updates() {
    let folder = game(
        "sprites_in_draw",
        &[(
            "main.luau",
            "function draw() graphics.load_sprite_sheet('sheet.png', 1, 1) end",
        )],
    );
    let mut simulation = simulation(&folder);
    simulation.step();
    assert!(stopped_with(&simulation).contains("not in `draw`"));
}

/// A room: a floor, an arm turned a quarter holding a blue hand, and a door.
const ROOM: &str = r##"return {
    format = 1,
    sheets = { colors = { path = "sheet.png", tile_width = 1, tile_height = 1 } },
    props = { title = "Room" },
    children = {
        { name = "floor", kind = "rect", y = 16, width = 20, height = 4, color = "#00ff00" },
        {
            name = "arm", kind = "node", x = 10, y = 10, rotation = 1.5707964,
            children = {
                {
                    name = "hand", kind = "sprite", x = 4, sheet = "colors", cell = { 1, 0 },
                    width = 2, height = 2, pivot = { 1, 1 }, tags = { "held" },
                    props = { weight = 3, names = { "a", "b" } },
                },
            },
        },
        { name = "door", kind = "area", width = 4, height = 4, tags = { "held" } },
    },
}"##;

#[test]
fn scenes_load_and_their_nodes_are_found() {
    let folder = game(
        "scenes",
        &[
            ("conf.luau", SMALL_WINDOW),
            ("room.scene.luau", ROOM),
            (
                "main.luau",
                "room = scene.load('room.scene.luau')
                again = scene.load('room.scene.luau')
                hand = room:find('arm/hand')",
            ),
        ],
    );
    write_png(&folder.join("sheet.png"), 2, 1, &[RED, BLUE]);
    let mut simulation = simulation(&folder);
    assert_eq!(simulation.eval::<String>("room:props().title"), "Room");
    assert_eq!(
        simulation.eval::<(String, String, String)>("hand:name(), hand:kind(), hand:path()"),
        ("hand".into(), "sprite".into(), "arm/hand".into())
    );
    assert_eq!(
        simulation.eval::<(f64, String)>("hand:props().weight, hand:props().names[2]"),
        (3.0, "b".into())
    );
    assert_eq!(
        simulation.eval::<(f32, f32)>("hand:world_position()"),
        (10.0, 14.0),
        "4 along the arm, which points down"
    );
    assert_eq!(simulation.eval::<(f32, f32)>("hand:size()"), (2.0, 2.0));
    assert!(simulation.eval::<bool>(
        "room:find('arm/hand') == hand and room:tagged('held')[1] == hand
         and #room:tagged('held') == 2 and again:find('arm/hand') ~= hand
         and room:find('nothing') == nil and hand:parent():name() == 'arm'
         and #room:children() == 3 and hand:has_tag('held') and hand:tags()[1] == 'held'"
    ));
    assert!(
        simulation
            .eval::<String>("tostring(hand)")
            .contains("arm/hand")
    );
    assert!(simulation.eval::<bool>(
        "room:find('door'):contains(1, 1) and not room:find('door'):contains(5, 5)
         and room:find('door'):bounds().width == 4 and room:bounds() == nil"
    ));
}

#[test]
fn scenes_draw_and_change() {
    let folder = game(
        "scenes_draw",
        &[
            ("conf.luau", SMALL_WINDOW),
            ("room.scene.luau", ROOM),
            (
                "main.luau",
                "room = scene.load('room.scene.luau')
                red = graphics.load_sprite_sheet('sheet.png', 1, 1):sprite(0, 0)
                function draw()
                    graphics.clear('#000000')
                    graphics.draw_scene(room)
                end",
            ),
        ],
    );
    write_png(&folder.join("sheet.png"), 2, 1, &[RED, BLUE]);
    let mut simulation = simulation(&folder);
    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(0, 19), [0, 255, 0, 255]);
    // The hand covers (9, 13) to (11, 15).
    assert_eq!(image.pixel(9, 13), BLUE);
    assert_eq!(image.pixel(10, 14), BLUE);
    assert_eq!(image.pixel(8, 13), BLACK);

    simulation.exec(
        "room:find('arm'):set_rotation(0)
         room:find('arm/hand'):set_sprite(red)
         room:find('floor'):set_color('#0000ff')
         room:find('floor'):set_size(10, 4)",
    );
    simulation.step();
    let image = simulation.screenshot().unwrap();
    // 4 along the arm, which now points right: (13, 9) to (15, 11).
    assert_eq!(image.pixel(13, 9), RED);
    assert_eq!(image.pixel(9, 13), BLACK);
    assert_eq!(image.pixel(0, 19), BLUE);
    assert_eq!(image.pixel(10, 19), BLACK);

    simulation.exec("room:find('arm'):set_visible(false)");
    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(13, 9), BLACK);
}

#[test]
fn scenes_lay_out_on_the_canvas() {
    let folder = game(
        "scenes_lay_out",
        &[
            // The canvas is the window's size.
            ("conf.luau", "return { width = 40, height = 20 }"),
            (
                "screen.scene.luau",
                r##"return {
                    format = 1,
                    children = {
                        {
                            name = "corner", kind = "rect", anchor = { 1, 1 }, width = 2,
                            height = 2, pivot = { 2, 2 }, color = "#ff0000",
                        },
                        { name = "label", kind = "text", text = "Hi", align = "right" },
                    },
                }"##,
            ),
            (
                "main.luau",
                "screen = scene.load('screen.scene.luau')
                corner = screen:find('corner')
                label = screen:find('label')
                function draw()
                    graphics.clear('#000000')
                    graphics.draw_scene(screen)
                end",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    let bounds = "corner:bounds().x, corner:bounds().y";
    assert_eq!(
        simulation.eval::<(f32, f32)>(bounds),
        (38.0, 18.0),
        "laid out on the canvas once loaded"
    );
    assert_eq!(simulation.eval::<(f32, f32)>("corner:anchor()"), (1.0, 1.0));
    simulation.resize(60, 30);
    simulation.step();
    assert_eq!(simulation.eval::<(f32, f32)>(bounds), (58.0, 28.0));
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(59, 29), [255, 0, 0, 255]);

    simulation.exec("corner:set_anchor(0.5, 0) label:set_anchor(nil)");
    assert_eq!(simulation.eval::<(f32, f32)>(bounds), (28.0, -2.0));
    assert!(
        simulation
            .eval::<bool>("label:anchor() == nil and label:bounds().x + label:bounds().width == 0")
    );
    let err = simulation
        .try_exec("corner:set_anchor(1)")
        .unwrap_err()
        .to_string();
    assert!(err.contains("set_anchor takes fractions"), "{err}");
}

#[test]
fn menus_pick_among_the_nodes_shown() {
    let folder = game(
        "menus",
        &[
            ("conf.luau", "return { width = 100, height = 60 }"),
            (
                "screen.scene.luau",
                r##"return {
                    format = 3,
                    children = {
                        {
                            name = "list", kind = "stack", x = 10, y = 10, gap = 2,
                            children = {
                                { name = "a", kind = "area", width = 40, height = 10 },
                                { name = "b", kind = "area", width = 40, height = 10 },
                                {
                                    name = "hidden", kind = "area", visible = false, width = 40,
                                    height = 10,
                                },
                                { name = "c", kind = "area", width = 40, height = 10 },
                            },
                        },
                        {
                            name = "row", kind = "node", y = 50,
                            children = {
                                {
                                    name = "one", kind = "area", tags = { "cell" }, width = 20,
                                    height = 10,
                                },
                                { name = "label", kind = "text", text = "x" },
                                {
                                    name = "two", kind = "area", tags = { "cell" }, x = 30,
                                    width = 20, height = 10,
                                },
                            },
                        },
                    },
                }"##,
            ),
            (
                "main.luau",
                "screen = scene.load('screen.scene.luau')
                list = ui.menu(screen:find('list'))
                cells = ui.menu(screen:find('row'), { tag = 'cell', wrap = false, confirm = 'pick' })
                input.bind('up', { 'Up' })
                input.bind('down', { 'Down' })
                input.bind('right', { 'Right' })
                input.bind('confirm', { 'Z' })
                input.bind('cancel', { 'X' })
                input.bind('pick', { 'Space' })
                -- What each menu said last, as `what item`.
                said = {}
                local function hear(name, happened, item)
                    if happened then
                        said[name] = `{happened} {item or ''}`
                    end
                end
                function update()
                    hear('list', list:update())
                    hear('cells', cells:update())
                end
                function names(menu)
                    local found = {}
                    for _, item in menu:items() do
                        table.insert(found, item:name())
                    end
                    return found
                end",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    let said = |simulation: &mut Simulation<LuauGame>, menu: &str| -> Option<String> {
        let said = simulation.eval(&format!("said.{menu}"));
        simulation.exec(&format!("said.{menu} = nil"));
        said
    };
    assert_eq!(
        simulation.eval::<Vec<String>>("names(list)"),
        ["a", "b", "c"],
        "those shown"
    );
    assert_eq!(simulation.eval::<u32>("list:selected()"), 1);
    simulation.tap(Key::Down);
    assert_eq!(said(&mut simulation, "list").as_deref(), Some("moved 2"));
    simulation.tap(Key::Down);
    simulation.tap(Key::Down);
    assert_eq!(
        said(&mut simulation, "list").as_deref(),
        Some("moved 1"),
        "it wraps"
    );

    simulation.exec("list:set_enabled(2, false)");
    assert!(!simulation.eval::<bool>("list:is_enabled(2)"));
    simulation.tap(Key::Down);
    assert_eq!(said(&mut simulation, "list").as_deref(), Some("moved 3"));
    // The selection stays on its item as others come and go.
    simulation.exec("screen:find('list/hidden'):set_visible(true)");
    assert_eq!(simulation.eval::<u32>("list:selected()"), 4);
    simulation.tap(Key::Z);
    assert_eq!(said(&mut simulation, "list").as_deref(), Some("picked 4"));
    simulation.tap(Key::X);
    assert_eq!(said(&mut simulation, "list").as_deref(), Some("cancelled "));
    simulation.exec("list:select(1)");
    assert_eq!(simulation.eval::<u32>("list:selected()"), 1);

    // The mouse moving onto an item selects it, and clicking one picks it.
    simulation.move_mouse(20.0, 25.0);
    simulation.step();
    assert_eq!(
        said(&mut simulation, "list"),
        None,
        "the second can't be picked"
    );
    simulation.exec("list:set_enabled(2, true)");
    simulation.move_mouse(21.0, 25.0);
    simulation.step();
    assert_eq!(said(&mut simulation, "list").as_deref(), Some("moved 2"));
    simulation.click(MouseButton::Left, 20.0, 15.0);
    assert_eq!(said(&mut simulation, "list").as_deref(), Some("picked 1"));

    // Items by tag, which needn't be a stack's, with actions of the game's.
    assert_eq!(
        simulation.eval::<Vec<String>>("names(cells)"),
        ["one", "two"]
    );
    simulation.tap(Key::Right);
    assert_eq!(said(&mut simulation, "cells").as_deref(), Some("moved 2"));
    simulation.tap(Key::Right);
    assert_eq!(said(&mut simulation, "cells"), None, "it doesn't wrap");
    simulation.tap(Key::Z);
    assert_eq!(said(&mut simulation, "cells"), None);
    simulation.tap(Key::Space);
    assert_eq!(said(&mut simulation, "cells").as_deref(), Some("picked 2"));

    let err = simulation
        .try_exec("ui.menu(screen, { colour = 'red' })")
        .unwrap_err()
        .to_string();
    assert!(err.contains("not `colour`"), "{err}");
    let err = simulation
        .try_exec("list:select(5)")
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("the menu has no item 5: it has 4 shown"),
        "{err}"
    );
}

#[test]
fn scene_nodes_refuse_what_they_do_not_have() {
    let folder = game(
        "scene_errors",
        &[
            ("room.scene.luau", ROOM),
            ("main.luau", "room = scene.load('room.scene.luau')"),
        ],
    );
    write_png(&folder.join("sheet.png"), 2, 1, &[RED, BLUE]);
    let mut simulation = simulation(&folder);
    let mut error = |code: &str| -> String {
        simulation.eval(&format!(
            "tostring(select(2, pcall(function() {code} end)))"
        ))
    };
    let err = error("room:find('floor'):set_tint('#ff0000')");
    assert!(
        err.contains("node `floor` is a rect node, which has no tint"),
        "{err}"
    );
    let err = error("room:size()");
    assert!(
        err.contains("the scene's root is a plain node, which has no size"),
        "{err}"
    );
    let err = error("room:find('door'):set_position(0/0, 0)");
    assert!(err.contains("must be finite"), "{err}");
    let err = error("room:find('door'):text()");
    assert!(
        err.contains("node `door` is an area node, which has no text"),
        "{err}"
    );
}

#[test]
fn clips_pose_scene_nodes() {
    let folder = game(
        "clips",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "puppet.scene.luau",
                r##"return {
                    format = 1,
                    sheets = { colors = { path = "sheet.png", tile_width = 1, tile_height = 1 } },
                    children = {
                        {
                            name = "arm", kind = "node", x = 10,
                            children = {
                                { name = "hand", kind = "sprite", x = 4, sheet = "colors", cell = { 0, 0 } },
                            },
                        },
                    },
                    clips = {
                        wave = {
                            duration = 1, loop = true,
                            tracks = {
                                { node = "arm", property = "rotation", keys = { { 0, 0 }, { 0.5, 1 }, { 1, 0 } } },
                                { node = "arm/hand", property = "cell", keys = { { 0, { 0, 0 } }, { 0.5, { 1, 0 } } } },
                            },
                        },
                        drop = {
                            duration = 0.5,
                            tracks = { { node = "arm", property = "y", keys = { { 0, 0 }, { 0.5, 8 } } } },
                        },
                        lost = { duration = 1, tracks = { { node = "leg", property = "x", keys = { { 0, 1 } } } } },
                    },
                }"##,
            ),
            (
                "main.luau",
                "puppet = scene.load('puppet.scene.luau')
                clips = scene.clips('puppet.scene.luau')
                arm = puppet:find('arm')
                hand = puppet:find('arm/hand')
                function draw()
                    graphics.clear('#000000')
                    graphics.draw_scene(puppet)
                end",
            ),
        ],
    );
    write_png(&folder.join("sheet.png"), 2, 1, &[RED, BLUE]);
    let mut simulation = simulation(&folder);
    assert_eq!(
        simulation.eval::<(f32, bool, bool, bool)>(
            "clips.wave:duration(), clips.wave:is_looping(), clips.drop:is_finished(0.4), \
             clips.drop:is_finished(0.5)"
        ),
        (1.0, true, false, true)
    );

    simulation.exec("puppet:pose(clips.wave, 1.25)");
    assert_eq!(simulation.eval::<f32>("arm:rotation()"), 0.5, "looped");
    // Halfway there from 2, at 4 for the clip.
    simulation.exec("arm:set_position(10, 2) puppet:pose(clips.drop, 0.25, 0.5)");
    assert_eq!(simulation.eval::<(f32, f32)>("arm:position()"), (10.0, 3.0));

    // The hand shows the clip's cell, the blue one.
    simulation.exec("puppet:pose(clips.wave, 0.5) arm:set_rotation(0)");
    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(14, 3), BLUE);
    simulation.exec("hand:set_cell(0, 0)");
    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(14, 3), RED);

    let mut error = |code: &str| -> String {
        simulation.eval(&format!(
            "tostring(select(2, pcall(function() {code} end)))"
        ))
    };
    let err = error("puppet:pose(clips.lost, 0)");
    assert!(
        err.contains("there's no node `leg` in the scene's root for the clip to pose"),
        "{err}"
    );
    let err = error("puppet:pose(clips.wave, 0/0)");
    assert!(err.contains("a time must be finite"), "{err}");
    let err = error("puppet:pose('wave', 0)");
    assert!(err.contains("Clip"), "{err}");
    let err = error("hand:set_cell(2, 0)");
    assert!(
        err.contains("sprite (2, 0) is outside a 2x1 sprite sheet"),
        "{err}"
    );
    let err = error("arm:set_cell(0, 0)");
    assert!(
        err.contains("node `arm` is a plain node, which has no cell"),
        "{err}"
    );
    let err = error("scene.clips('missing.scene.luau')");
    assert!(err.contains("missing.scene.luau"), "{err}");
}

/// A tileset of red and blue tiles, the blue ones solid.
const COLORS_TILESET: &str = r##"return {
    format = 1, tile_width = 2, tile_height = 2,
    sheets = { colors = { path = "sheet.png", tile_width = 1, tile_height = 1 } },
    tiles = {
        red = { sheet = "colors", cell = { 0, 0 } },
        blue = { sheet = "colors", cell = { 1, 0 }, solid = true, props = { cold = true } },
    },
}"##;

/// A 4x3 map of 2x2 tiles at (4, 2), red with a blue wall on a layer above, and a villager from
/// another file.
const LEVEL: &str = r##"return {
    format = 1,
    children = {
        {
            name = "map", kind = "tilemap", x = 4, y = 2, tileset = "colors.tileset.luau",
            columns = 4, rows = 3,
            children = {
                { name = "floor", kind = "tile_layer", legend = { r = "red" }, tiles = { "rrrr", "rrrr", "rrrr" } },
                { name = "walls", kind = "tile_layer", legend = { b = "blue" }, tiles = { "    ", " b  ", "    " } },
            },
        },
        {
            name = "guide", kind = "instance", x = 1, scene = "people/villager.scene.luau",
            overrides = { body = { color = "#00ff00" } },
        },
    },
}"##;

const VILLAGER: &str = r##"return {
    format = 1,
    props = { talk = "hello" },
    children = { { name = "body", kind = "rect", width = 1, height = 1 } },
}"##;

#[test]
fn scene_tilemaps_and_instances_work_from_luau() {
    let folder = game(
        "scene_tilemaps",
        &[
            ("conf.luau", SMALL_WINDOW),
            ("colors.tileset.luau", COLORS_TILESET),
            ("level.scene.luau", LEVEL),
            ("people/villager.scene.luau", VILLAGER),
            (
                "main.luau",
                "level = scene.load('level.scene.luau')
                map = level:find('map')
                walls = level:find('map/walls')
                function draw()
                    graphics.clear('#000000')
                    graphics.draw_scene(level)
                end",
            ),
        ],
    );
    write_png(&folder.join("sheet.png"), 2, 1, &[RED, BLUE]);
    let mut simulation = simulation(&folder);
    assert!(simulation.eval::<bool>(
        "map:is_solid(1, 1) and not map:is_solid(0, 0) and map:is_solid(-1, 0)
         and walls:is_solid(1, 1)"
    ));
    assert_eq!(
        simulation.eval::<(f32, f32)>("map:tile_to_world(1, 1)"),
        (6.0, 4.0)
    );
    assert_eq!(
        simulation.eval::<(i32, i32)>("map:world_to_tile(7, 5)"),
        (1, 1)
    );
    assert_eq!(simulation.eval::<(u32, u32)>("map:grid_size()"), (4, 3));
    assert_eq!(simulation.eval::<(u32, u32)>("map:tile_size()"), (2, 2));
    assert!(simulation.eval::<bool>("map:tile_props('blue').cold"));
    assert_eq!(
        simulation.eval::<(Option<String>, Option<String>)>("walls:tile(1, 1), walls:tile(0, 0)"),
        (Some("blue".into()), None)
    );
    assert_eq!(
        simulation.eval::<String>("level:find('guide'):props().talk"),
        "hello"
    );

    simulation.step();
    let image = simulation.screenshot().unwrap();
    // The map covers (4, 2) to (12, 8), its wall (6, 4) to (8, 6).
    assert_eq!(image.pixel(4, 2), RED);
    assert_eq!(image.pixel(6, 4), BLUE);
    assert_eq!(image.pixel(3, 2), BLACK);
    assert_eq!(
        image.pixel(1, 0),
        [0, 255, 0, 255],
        "the guide's body, overridden"
    );

    simulation.exec("walls:set_tile(1, 1, nil) walls:set_tile(0, 0, 'blue')");
    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(6, 4), RED);
    assert_eq!(image.pixel(4, 2), BLUE);

    let mut error = |code: &str| -> String {
        simulation.eval(&format!(
            "tostring(select(2, pcall(function() {code} end)))"
        ))
    };
    let err = error("walls:set_tile(0, 0, 'green')");
    assert!(err.contains("there's no tile `green`"), "{err}");
    let err = error("map:set_tile(0, 0, 'red')");
    assert!(
        err.contains("node `map` is a tilemap node, which has no tiles"),
        "{err}"
    );
    let err = error("level:is_solid(0, 0)");
    assert!(err.contains("which has no tilemap"), "{err}");
}

#[test]
fn scene_nodes_are_added_and_removed_from_luau() {
    let folder = game(
        "scene_add_remove",
        &[
            ("conf.luau", SMALL_WINDOW),
            ("room.scene.luau", ROOM),
            (
                "main.luau",
                "room = scene.load('room.scene.luau')
                spare = room:find('floor'):add(scene.load('room.scene.luau'), 'spare')
                hand = room:find('arm/hand')
                twin = room:add(hand, 'twin')
                function draw()
                    graphics.clear('#000000')
                    graphics.draw_scene(room)
                end",
            ),
        ],
    );
    write_png(&folder.join("sheet.png"), 2, 1, &[RED, BLUE]);
    let mut simulation = simulation(&folder);
    assert!(simulation.eval::<bool>(
        "room:find('floor/spare/arm/hand') ~= nil and twin:path() == 'twin'
         and twin ~= hand and spare:props().title == 'Room'"
    ));
    let error = |simulation: &mut Simulation<LuauGame>, code: &str| -> String {
        simulation.eval(&format!(
            "tostring(select(2, pcall(function() {code} end)))"
        ))
    };
    let err = error(&mut simulation, "room:add(twin)");
    assert!(err.contains("a node named `twin` there already"), "{err}");
    let err = error(&mut simulation, "room:add(room)");
    assert!(err.contains("takes a name"), "{err}");

    simulation.step();
    // The twin, at the hand's position in the arm but in the room, covers (3, -1) to (5, 1).
    assert_eq!(simulation.screenshot().unwrap().pixel(4, 0), BLUE);
    assert_eq!(simulation.screenshot().unwrap().pixel(2, 0), BLACK);

    simulation.exec("twin:remove()");
    simulation.step();
    assert_eq!(simulation.screenshot().unwrap().pixel(4, 0), BLACK);
    assert!(simulation.eval::<bool>("twin:is_removed() and not hand:is_removed()"));
    let err = error(&mut simulation, "twin:position()");
    assert!(err.contains("has been removed"), "{err}");
    assert_eq!(
        simulation.eval::<String>("tostring(twin)"),
        "SceneNode(removed)"
    );
    let err = error(&mut simulation, "room:remove()");
    assert!(err.contains("root can't be removed"), "{err}");
}

#[test]
fn scenes_only_load_while_the_game_loads_or_updates() {
    let folder = game(
        "scenes_in_draw",
        &[
            ("room.scene.luau", ROOM),
            (
                "main.luau",
                "function draw() scene.load('room.scene.luau') end",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    simulation.step();
    assert!(stopped_with(&simulation).contains("not in `draw`"));
}

#[test]
fn scene_errors_name_the_file_and_node() {
    let folder = game(
        "scene_broken",
        &[
            (
                "broken.scene.luau",
                &ROOM.replace("cell = { 1, 0 }", "cell = { 1, 0 }, colour = 'red'"),
            ),
            ("main.luau", "scene.load('broken.scene.luau')"),
        ],
    );
    write_png(&folder.join("sheet.png"), 2, 1, &[RED, BLUE]);
    let config = runtime::config(&folder).unwrap();
    let err = Simulation::new(&config.window, |ctx| {
        LuauGame::load_with(ctx, &folder, &config)
    })
    .err()
    .expect("the scene fails to load");
    let message = err.to_string();
    assert!(message.contains("broken.scene.luau"), "{message}");
    assert!(
        message.contains("node `arm/hand`: a sprite node has no field `colour`"),
        "{message}"
    );
}

#[test]
fn tilemaps_hold_tiles_in_layers_and_draw_them() {
    let folder = game(
        "tilemaps",
        &[
            (
                "conf.luau",
                "return { width = 30, height = 20, resolution = { width = 30, height = 20 } }",
            ),
            (
                "main.luau",
                "local white = graphics.load_sprite_sheet('white.png', 1, 1):sprite(0, 0)
                local tileset = tilemap.new_tileset(10, 10)
                local grass = tileset:add({ sprite = white, tint = '#ff0000' })
                local wall = tileset:add({ sprite = white, tint = '#0000ff', solid = true })
                map = tilemap.new(tileset, 3, 2)
                ground = map:add_layer()
                walls = map:add_layer()
                for row = 0, 1 do
                    for column = 0, 2 do
                        map:set(ground, column, row, grass)
                    end
                end
                map:set(walls, 1, 0, wall)
                map:set(walls, 2, 1, wall)
                map:set(walls, 2, 1, nil)

                function fails(f)
                    local ok, err = pcall(f)
                    return if ok then nil else tostring(err)
                end
                outside = fails(function() map:set(ground, 3, 0, grass) end)
                no_tile = fails(function() map:set(ground, 0, 0, 7) end)
                no_layer = fails(function() map:get(2, 0, 0) end)

                only_ground = false
                function draw()
                    graphics.clear('#000000')
                    graphics.draw_tilemap(map, if only_ground then ground else nil)
                end",
            ),
        ],
    );
    write_png(&folder.join("white.png"), 1, 1, &[WHITE]);

    let mut simulation = simulation(&folder);
    let mut check = |expression: &str| simulation.eval::<bool>(expression);
    assert!(check(
        "map:get(walls, 1, 0) ~= nil and map:get(walls, 2, 1) == nil"
    ));
    assert!(check("map:is_solid(1, 0) and not map:is_solid(2, 1)"));
    assert!(check("map:is_solid(-1, 0) and not map:contains(-1, 0)"));
    assert_eq!(simulation.eval::<(u32, u32)>("map:size()"), (3, 2));
    assert_eq!(simulation.eval::<usize>("map:layer_count()"), 2);
    let bounds: (f32, f32, f32, f32) =
        simulation.eval("map:bounds().x, map:bounds().y, map:bounds().width, map:bounds().height");
    assert_eq!(bounds, (0.0, 0.0, 30.0, 20.0));
    assert_eq!(
        simulation.eval::<(i32, i32)>("map:world_to_tile(15, 19.5)"),
        (1, 1)
    );
    assert_eq!(
        simulation.eval::<(f32, f32)>("map:tile_to_world(1.5, 1)"),
        (15.0, 10.0)
    );
    for (name, message) in [
        ("outside", "tile (3, 0) is outside a 3x2 map"),
        ("no_tile", "the map's tileset has no tile 7"),
        ("no_layer", "there's no layer 2 in a map of 2 layers"),
    ] {
        let err: String = simulation.eval(name);
        assert!(err.contains(message), "{name}: {err}");
    }

    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(15, 5), BLUE, "the wall, over the ground");
    assert_eq!(image.pixel(25, 15), RED);
    simulation.exec("only_ground = true");
    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(15, 5), RED);
}

#[test]
fn grid_movers_walk_from_tile_to_tile() {
    let folder = game(
        "grid_movers",
        &[(
            "main.luau",
            "local mover = grid.new_mover(2, 2, 0.5)
            started = mover:step('right')
            column, row = mover:tile()
            mover:update(0.25)
            x, y = mover:position()
            moving = mover:is_moving()
            mover:update(0.25)
            arrived = not mover:is_moving()

            -- The blocked function sees where the mover would go, and may look at the mover.
            blocked_step = mover:step('up', function(c, r)
                checked = { c, r }
                local mc, mr = mover:tile()
                from = { mc, mr }
                return true
            end)
            facing = mover:facing()
            neighbor = { grid.neighbor('left', 0, 0) }
            opposite = grid.opposite('up')
            zero_step_time_fails = not pcall(function() grid.new_mover(0, 0, 0) end)
            bad_frame_times_fail = not pcall(function() mover:update(0 / 0) end)
                and not pcall(function() mover:update(-0.1) end)",
        )],
    );
    let mut simulation = simulation(&folder);
    assert!(simulation.eval::<bool>("started"));
    assert_eq!(simulation.eval::<(i32, i32)>("column, row"), (3, 2));
    assert_eq!(simulation.eval::<(f32, f32)>("x, y"), (2.5, 2.0));
    assert!(simulation.eval::<bool>("moving and arrived"));
    assert!(!simulation.eval::<bool>("blocked_step"));
    assert_eq!(
        simulation.eval::<(i32, i32, i32, i32)>("checked[1], checked[2], from[1], from[2]"),
        (3, 1, 3, 2)
    );
    assert_eq!(simulation.eval::<String>("facing"), "up", "turned");
    assert_eq!(
        simulation.eval::<(i32, i32)>("neighbor[1], neighbor[2]"),
        (-1, 0)
    );
    assert_eq!(simulation.eval::<String>("opposite"), "down");
    assert!(simulation.eval::<bool>("zero_step_time_fails"));
    assert!(simulation.eval::<bool>("bad_frame_times_fail"));
}

#[test]
fn tilemaps_too_big_to_hold_are_errors() {
    let folder = game(
        "huge_tilemaps",
        &[(
            "main.luau",
            "local tileset = tilemap.new_tileset(1, 1)
            local ok, err = pcall(function() tilemap.new(tileset, 100000, 100000) end)
            too_big = tostring(err)
            -- One layer of this fits, but not two.
            local map = tilemap.new(tileset, 4096, 2049)
            map:add_layer()
            ok, err = pcall(function() map:add_layer() end)
            second_layer = tostring(err)
            layers = map:layer_count()",
        )],
    );
    let mut simulation = simulation(&folder);
    let too_big: String = simulation.eval("too_big");
    assert!(too_big.contains("more than the 16777216"), "{too_big}");
    let second_layer: String = simulation.eval("second_layer");
    assert!(second_layer.contains("with 2 layers"), "{second_layer}");
    assert_eq!(simulation.eval::<usize>("layers"), 1);
}

#[test]
fn each_image_is_loaded_once() {
    let folder = game(
        "cached_images",
        &[(
            "main.luau",
            "graphics.load_sprite_sheet('sheet.png', 1, 1)
            function update()
                if reload then
                    reload = false
                    -- The same image, divided differently.
                    again = graphics.load_sprite_sheet('sheet.png', 2, 1)
                end
            end",
        )],
    );
    let sheet = folder.join("sheet.png");
    write_png(&sheet, 2, 1, &[RED, BLUE]);
    let mut simulation = simulation(&folder);
    // Gone from disk, it loads all the same.
    fs::remove_file(&sheet).unwrap();
    simulation.exec("reload = true");
    simulation.step();
    assert!(simulation.game().error().is_none());
    assert_eq!(simulation.eval::<(u32, u32)>("again:size()"), (1, 1));
}

#[test]
fn text_layouts_reveal_their_glyphs() {
    let folder = game(
        "text_layouts",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                "local font = graphics.default_font()
                -- Just too narrow for the text on one line.
                max_width = font:measure('Hi there') - 1
                layout = font:layout('Hi there', max_width)
                lines = layout:line_count()
                glyphs = layout:glyph_count()
                width, height = layout:size()
                line_height = font:line_height()
                shown = 0

                function draw()
                    graphics.clear('#000000')
                    graphics.draw_text_layout(layout, 0, 0, '#ffffff', shown)
                end",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    assert_eq!(simulation.eval::<usize>("lines"), 2, "wrapped");
    assert_eq!(simulation.eval::<usize>("glyphs"), 7, "spaces don't count");
    let (width, height): (f32, f32) = simulation.eval("width, height");
    assert!(width <= simulation.eval::<f32>("max_width"));
    assert_eq!(height, 2.0 * simulation.eval::<f32>("line_height"));

    let lit = |simulation: &mut Simulation<LuauGame>| {
        simulation.step();
        let image = simulation.screenshot().unwrap();
        image.pixels().chunks(4).filter(|p| p[0] > 0).count()
    };
    assert_eq!(lit(&mut simulation), 0, "none shown");
    simulation.exec("shown = 1.9");
    let one = lit(&mut simulation);
    simulation.exec("shown = nil");
    let all = lit(&mut simulation);
    assert!(0 < one && one < all, "{one} of {all}");
}

#[test]
fn the_camera_stays_inside_its_bounds() {
    let folder = game(
        "camera_bounds",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                "function draw()
                    graphics.clear('#000000')
                    -- Centered on the world's corner, the view would show half off the world.
                    graphics.set_camera(0, 0, { x = 0, y = 0, width = 40, height = 40 })
                    graphics.fill_rect(0, 0, 2, 2, '#ff0000')
                end",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(1, 1), RED);
    assert_eq!(image.pixel(11, 11), BLACK);
}

#[test]
fn outlines_lines_and_circles_draw_where_they_are_given() {
    let folder = game(
        "shapes",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                "function draw()
                    graphics.clear('#000000')
                    graphics.outline_rect(0, 0, 10, 10, '#ff0000')
                    graphics.fill_circle(5, 5, 2, '#0000ff')
                    graphics.outline_rect(12, 0, 8, 8, '#ff0000', 2)
                    graphics.outline_circle(16, 4, 2, '#0000ff')
                    graphics.draw_line(0, 12.5, 20, 12.5, '#0000ff')
                    graphics.draw_line(0, 16, 20, 16, '#ff0000', 2)
                end",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    simulation.step();
    let image = simulation.screenshot().unwrap();
    // A thin outline, with a circle inside it.
    assert_eq!(image.pixel(0, 0), RED);
    assert_eq!(image.pixel(9, 9), RED);
    assert_eq!(image.pixel(1, 1), BLACK);
    assert_eq!(image.pixel(4, 4), BLUE);
    assert_eq!(image.pixel(3, 5), BLUE);
    assert_eq!(image.pixel(2, 5), BLACK);
    // A thick outline, with a ring inside it.
    assert_eq!(image.pixel(13, 1), RED);
    assert_eq!(image.pixel(14, 3), BLUE);
    assert_eq!(image.pixel(15, 3), BLACK);
    // A thin line along the middles of row 12, and a thick one over rows 15 and 16.
    assert_eq!(image.pixel(10, 11), BLACK);
    assert_eq!(image.pixel(10, 12), BLUE);
    assert_eq!(image.pixel(10, 13), BLACK);
    assert_eq!(image.pixel(3, 14), BLACK);
    assert_eq!(image.pixel(3, 15), RED);
    assert_eq!(image.pixel(3, 16), RED);
    assert_eq!(image.pixel(3, 17), BLACK);
}

#[test]
fn the_camera_zooms_about_the_middle_of_the_view() {
    let folder = game(
        "camera_zoom",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                "zoom = 2
                function draw()
                    graphics.clear('#000000')
                    graphics.set_camera(10, 10, nil, zoom)
                    graphics.fill_rect(10, 10, 2, 2, '#ff0000')
                end",
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    simulation.step();
    let image = simulation.screenshot().unwrap();
    // From the middle, (10, 10), twice the size.
    assert_eq!(image.pixel(10, 10), RED);
    assert_eq!(image.pixel(13, 13), RED);
    assert_eq!(image.pixel(14, 14), BLACK);
    assert_eq!(image.pixel(9, 9), BLACK);

    simulation.exec("zoom = 0.5");
    simulation.step();
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(10, 10), RED);
    assert_eq!(image.pixel(11, 11), BLACK);

    for zoom in ["0", "-1", "0 / 0", "math.huge"] {
        let code = format!("graphics.set_camera(0, 0, nil, {zoom})");
        let err = simulation.try_exec(&code).unwrap_err().to_string();
        assert!(err.contains("zoom must be above 0"), "{zoom}: {err}");
    }
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

/// A view of the middle of the world, 2 units across, looking along -z without perspective or
/// shading, for 3D tests on the 20x20 canvas of `SMALL_WINDOW`.
const FLAT_VIEW: &str = "view = {
    camera = { position = vector.create(0, 0, 5), target = vector.zero, height = 2 },
    lighting = { color = '#000000', ambient = '#ffffff' },
}";

#[test]
fn luau_games_draw_meshes_and_billboards_in_3d_views() {
    let folder = game(
        "meshes",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                &format!(
                    "{FLAT_VIEW}
                    cube = graphics.new_cube(1)
                    hero = graphics.load_sprite_sheet('tiles.png', 16, 16):sprite(27, 0)
                    seen = {{}}
                    function update()
                        table.insert(seen, graphics.frame_stats())
                    end
                    function draw()
                        graphics.begin_3d(view)
                        -- Half the view across: the middle 10 pixels.
                        graphics.draw_mesh(cube, vector.zero, {{ tint = '#ff0000' }})
                        -- A quarter of it across, nearer the camera, its bottom at the middle.
                        graphics.draw_billboard(hero, vector.create(-0.75, 0, 1), '#00ff00', 0.5)
                        -- Over the view, across the cube.
                        graphics.fill_rect(0, 12, 20, 2, '#0000ff')
                    end"
                ),
            ),
        ],
    );
    fs::copy(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/examples/kenney_1-bit-pack_transparent-packed.png"
        ),
        folder.join("tiles.png"),
    )
    .unwrap();
    let mut simulation = simulation(&folder);
    simulation.run_frames(2);
    assert_eq!(simulation.game().error().map(ToString::to_string), None);
    let stats = "seen[2].triangles, seen[2].draw_calls";
    // The cube's 12 and the billboard's 2; the two meshes, then the rect.
    assert_eq!(simulation.eval::<(u32, u32)>(stats), (14, 3));

    let frame = simulation.screenshot().unwrap();
    assert_eq!(frame.pixel(10, 7), [255, 0, 0, 255], "the cube");
    assert_eq!(frame.pixel(17, 7), [0, 0, 0, 255], "beside it");
    assert_eq!(
        frame.pixel(10, 13),
        [0, 0, 255, 255],
        "the rect over the view"
    );
    // The hero's sprite is mostly transparent, so the canvas shows through it, apart from its
    // green pixels, all left of the cube, above the middle.
    let green = (0..20)
        .flat_map(|x| (0..20).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            let [r, g, b, _] = frame.pixel(x, y);
            g > 128 && r < 32 && b < 32
        })
        .collect::<Vec<_>>();
    assert!(!green.is_empty());
    assert!(green.iter().all(|&(x, y)| x < 5 && y < 10), "{green:?}");
}

#[test]
fn meshes_draw_only_in_an_open_3d_view() {
    let draw = |test: &str, body: &str| {
        let main = format!(
            "{FLAT_VIEW}
            cube = graphics.new_cube(1)
            function draw()
                {body}
            end"
        );
        let mut simulation = simulation(&game(test, &[("main.luau", &main)]));
        simulation.run_frames(1);
        simulation.game().error().map(ToString::to_string)
    };
    let without = draw("mesh_without_view", "graphics.draw_mesh(cube, vector.zero)").unwrap();
    assert!(
        without.contains("call graphics.begin_3d first"),
        "{without}"
    );
    let after_2d = draw(
        "mesh_after_2d",
        "graphics.begin_3d(view)
        graphics.fill_rect(0, 0, 1, 1, '#ffffff')
        graphics.draw_mesh(cube, vector.zero)",
    );
    assert!(after_2d.unwrap().contains("begin_3d"));
    let in_view = draw(
        "mesh_in_view",
        "graphics.begin_3d(view)
        graphics.set_camera(5, 5)
        graphics.draw_mesh(cube, vector.zero)
        graphics.draw_mesh(cube, vector.zero, { yaw = 1, scale = vector.create(1, 2, 1) })",
    );
    assert_eq!(in_view, None);
}

#[test]
fn luau_games_make_meshes() {
    let folder = game(
        "making_meshes",
        &[(
            "main.luau",
            "counts = {
                graphics.new_cube(1):triangle_count(),
                graphics.new_cuboid(1, 2, 3):triangle_count(),
                graphics.new_plane(2, 2):triangle_count(),
                graphics.new_sphere(1):triangle_count(),
                graphics.new_sphere(1, 4, 2):triangle_count(),
                graphics.new_mesh({
                    vertices = {
                        { position = vector.create(0, 0, 0) },
                        { position = vector.create(1, 0, 0), u = 1, color = '#ff0000' },
                        { position = vector.create(1, 0, -1), normal = vector.create(0, 1, 0) },
                    },
                    indices = { 1, 2, 3 },
                }):triangle_count(),
            }
            from_zero = pcall(graphics.new_mesh, {
                vertices = { { position = vector.zero } },
                indices = { 0, 1, 1 },
            })
            _, too_few = pcall(graphics.new_mesh, {
                vertices = { { position = vector.zero } },
                indices = { 1, 1 },
            })
            _, infinite = pcall(graphics.new_cube, math.huge)
            _, negative = pcall(graphics.new_cuboid, -1, 1, 1)
            _, flat = pcall(graphics.new_plane, 2, 0)
            _, no_radius = pcall(graphics.new_sphere, 0)
            -- Far more slices and rings than a sphere needs get the most it has, not a crash.
            most = graphics.new_sphere(1, 4000000000, 4000000000):triangle_count()",
        )],
    );
    let mut simulation = simulation(&folder);
    assert_eq!(
        simulation.eval::<Vec<u32>>("counts"),
        [12, 12, 2, 16 * 2 * 6 + 16 * 2, 4 * 2, 1]
    );
    assert!(!simulation.eval::<bool>("from_zero"));
    let too_few: String = simulation.eval("tostring(too_few)");
    assert!(too_few.contains("2 indices"), "{too_few}");
    for name in ["infinite", "negative", "flat", "no_radius"] {
        let err: String = simulation.eval(&format!("tostring({name})"));
        assert!(err.contains("must be above 0 and finite"), "{name}: {err}");
    }
    // The poles' rings are a triangle a slice, and the rest two.
    let most = 512;
    assert_eq!(
        simulation.eval::<u32>("most"),
        most * 2 + most * (most - 2) * 2
    );
}

#[test]
fn cameras_place_points_on_the_canvas_and_rays_into_the_world() {
    let folder = game(
        "projecting",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                &format!(
                    "{FLAT_VIEW}
                    -- The middle, and the top-right corner of the view, which a rect moves.
                    middle = {{ graphics.project(view, vector.zero) }}
                    corner = {{ graphics.project({{ camera = view.camera, rect = {{ x = 4, y = 6, width = 10, height = 10 }} }}, vector.create(1, 1, 0)) }}
                    behind = {{ graphics.project(view, vector.create(0, 0, 10)) }}
                    origin, direction = graphics.ray(view, 15, 10)
                    _, both = pcall(graphics.project, {{ camera = {{ position = vector.one, target = vector.zero, fov = 1, height = 2 }} }}, vector.zero)"
                ),
            ),
        ],
    );
    let mut simulation = simulation(&folder);
    assert_eq!(
        simulation.eval::<(f32, f32)>("middle[1], middle[2]"),
        (10.0, 10.0)
    );
    assert_eq!(
        simulation.eval::<(f32, f32)>("corner[1], corner[2]"),
        (14.0, 6.0)
    );
    assert_eq!(simulation.eval::<usize>("#behind"), 0);
    // Half way from the middle to the right edge, a unit to the right; without perspective, rays
    // all go the way the camera looks.
    let ray: (f32, f32, f32, f32) = simulation.eval("origin.x, origin.z, direction.x, direction.z");
    assert_eq!(ray, (0.5, 4.9, 0.0, -1.0));
    let both: String = simulation.eval("tostring(both)");
    assert!(both.contains("not both"), "{both}");
}

/// A 3D view filling the canvas, seen from the front through an orthographic camera 2 units high,
/// unlit, so a unit is 10 pixels with the world's origin in the middle: a red box to the left in
/// front of a blue wall, and a red billboard to the right, inside a plain node.
const STAGE: &str = r##"return {
    format = 2,
    sheets = { colors = { path = "sheet.png", tile_width = 1, tile_height = 1 } },
    children = {
        {
            name = "view", kind = "view3d", width = 0, height = 0, stretch = { 1, 1 },
            camera = { position = { 0, 0, 5 }, target = { 0, 0, 0 }, height = 2 },
            lighting = { color = "#000000", ambient = "#ffffff" },
            children = {
                { name = "wall", kind = "mesh", shape = "cuboid", z = -1, size = { 2, 2, 0.1 }, tint = "#0000ff" },
                { name = "box", kind = "mesh", shape = "cuboid", x = -0.5, z = 1, size = { 0.4, 0.4, 0.4 }, tint = "#ff0000" },
                {
                    name = "group", kind = "node", x = 0.5, z = 1,
                    children = {
                        { name = "hero", kind = "billboard", y = -0.2, sheet = "colors", cell = { 0, 0 }, width = 0.4, height = 0.4 },
                    },
                },
            },
        },
        { name = "label", kind = "rect", width = 1, height = 1, color = "#00000000" },
    },
}"##;

#[test]
fn scene_views_work_from_luau() {
    let folder = game(
        "scene_views",
        &[
            ("conf.luau", SMALL_WINDOW),
            ("stage.scene.luau", STAGE),
            (
                "main.luau",
                "stage = scene.load('stage.scene.luau')
                view = stage:find('view')
                box = view:find('box')
                hero = view:find('group/hero')
                function draw()
                    graphics.clear('#000000')
                    graphics.draw_scene(stage)
                end",
            ),
        ],
    );
    write_png(&folder.join("sheet.png"), 2, 1, &[RED, BLUE]);
    let mut simulation = simulation(&folder);
    simulation.step();
    assert_eq!(simulation.game().error().map(ToString::to_string), None);
    let image = simulation.screenshot().unwrap();
    assert_eq!(image.pixel(5, 10), RED, "the box");
    assert_eq!(image.pixel(15, 10), RED, "the hero");
    assert_eq!(image.pixel(10, 2), BLUE, "the wall");

    assert_eq!(
        simulation.eval::<(String, String, String)>("view:kind(), box:kind(), hero:kind()"),
        ("view3d".into(), "mesh".into(), "billboard".into())
    );
    assert_eq!(
        simulation.eval::<(f32, f32, f32)>(
            "box:position_3d().x, box:position_3d().y, box:position_3d().z"
        ),
        (-0.5, 0.0, 1.0)
    );
    assert_eq!(
        simulation.eval::<(f32, f32, f32)>(
            "hero:world_position_3d().x, hero:world_position_3d().y, hero:world_position_3d().z"
        ),
        (0.5, -0.2, 1.0)
    );
    let bounds: [f32; 6] = simulation.eval::<(f32, f32, f32, f32, f32, f32)>("(function() local low, high = box:bounds_3d() return low.x, low.y, low.z, high.x, high.y, high.z end)()",
    )
    .into();
    let expected = [-0.7, -0.2, 0.8, -0.3, 0.2, 1.2];
    assert!(
        bounds
            .iter()
            .zip(expected)
            .all(|(a, b)| (a - b).abs() < 1e-6),
        "{bounds:?}"
    );
    // Picked nearest first, and placed on the canvas.
    let mut picked = |x: f32, y: f32| {
        simulation.eval::<Vec<String>>(&format!(
            "(function() local names = {{}} for _, node in view:pick({x}, {y}) do \
                 table.insert(names, node:name()) end return names end)()"
        ))
    };
    assert_eq!(picked(5.0, 10.0), ["box", "wall"]);
    assert_eq!(picked(15.0, 10.0), ["hero", "wall"]);
    assert_eq!(picked(10.0, 2.0), ["wall"]);
    assert_eq!(
        simulation.eval::<(f32, f32)>("view:project(vector.create(-0.5, 0, 1))"),
        (5.0, 10.0)
    );
    assert_eq!(
        simulation.eval::<(f32, f32, String)>("view:camera().height, view:camera().target.z, string.format('%08x', view:lighting().ambient)"
        ),
        (2.0, 0.0, "ffffffff".into())
    );

    // Moved, turned, stretched and recolored, as the next frame shows.
    simulation.exec(
        "box:set_position_3d(vector.create(0, 0.5, 1))
         box:set_orientation(0.25)
         box:set_scale_3d(0.5)
         hero:set_tint('#0000ff')
         hero:set_size(-0.4, 0.4)
         view:set_lighting({ ambient = '#808080', color = '#000000' })
         view:set_camera({ position = vector.create(0, 0, 5), target = vector.zero, height = 4 })",
    );
    assert_eq!(
        simulation.eval::<(f32, f32, f32)>("box:orientation()"),
        (0.25, 0.0, 0.0)
    );
    assert_eq!(
        simulation.eval::<(f32, f32)>("box:scale_3d().x, box:scale_3d().z"),
        (0.5, 0.5)
    );
    assert_eq!(simulation.eval::<(f32, f32)>("hero:size()"), (-0.4, 0.4));
    assert_eq!(simulation.eval::<f32>("view:camera().height"), 4.0);

    // What nodes in 3D views don't have, and what other nodes don't.
    let mut error = |code: &str| {
        simulation.eval::<String>(&format!(
            "tostring(select(2, pcall(function() return {code} end)))"
        ))
    };
    assert!(
        error("box:rotation()").contains("is placed in 3D, in a 3D view, so it has no rotation"),
        "{}",
        error("box:rotation()")
    );
    assert!(error("box:set_anchor(0, 0)").contains("has no anchor"));
    assert!(
        error("box:world_position()").contains("has no world position in the scene"),
        "{}",
        error("box:world_position()")
    );
    assert!(
        error("stage:find('label'):position_3d()")
            .contains("node `label` isn't in a 3D view, so it isn't placed in 3D")
    );
    assert!(error("box:camera()").contains("node `view/box` is a mesh node, which has no camera"));
    assert!(error("box:pick(1, 1)").contains("which has no nodes to pick"));
}

/// Copies the example props, `table.glb` and `crate.glb` and the crate's texture, into `models`
/// in `folder`.
#[cfg(feature = "models")]
fn copy_props(folder: &Path) {
    let models = folder.join("models");
    fs::create_dir_all(&models).unwrap();
    for file in ["table.glb", "crate.glb", "crate.png"] {
        let from = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/models");
        fs::copy(from.join(file), models.join(file)).unwrap();
    }
}

#[test]
#[cfg(feature = "models")]
fn luau_games_load_and_draw_models() {
    let folder = game(
        "models",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                &format!(
                    "{FLAT_VIEW}
                    desk = graphics.load_model('models/table.glb')
                    box = graphics.load_model('models/crate.glb')
                    low, high = desk:bounds()
                    _, missing = pcall(graphics.load_model, 'models/none.glb')
                    local texture = graphics.load_sprite_sheet('models/crate.png', 12, 12):sprite(0, 0)
                    _, with_sprite = pcall(graphics.draw_model, box, vector.zero, {{ sprite = texture }})
                    seen = {{}}
                    function update()
                        table.insert(seen, graphics.frame_stats())
                        if reload then
                            reload = false
                            again = graphics.load_model('models/crate.glb')
                        end
                    end
                    function draw()
                        graphics.begin_3d(view)
                        graphics.draw_model(desk, vector.zero, {{ tint = '#ff0000' }})
                        graphics.draw_model(box, vector.create(0, -1, -2), {{ yaw = 0.5, scale = 0.5 }})
                    end"
                ),
            ),
        ],
    );
    copy_props(&folder);
    let mut simulation = simulation(&folder);
    simulation.run_frames(2);
    assert_eq!(simulation.game().error().map(ToString::to_string), None);

    // A top and four legs, each a box.
    assert_eq!(simulation.eval::<u32>("desk:triangle_count()"), 60);
    let bounds: [f32; 6] = simulation
        .eval::<(f32, f32, f32, f32, f32, f32)>("low.x, low.y, low.z, high.x, high.y, high.z")
        .into();
    let expected = [-0.5, 0.0, -0.3, 0.5, 0.5, 0.3];
    assert!(
        bounds
            .iter()
            .zip(expected)
            .all(|(a, b)| (a - b).abs() < 1e-6),
        "{bounds:?}"
    );
    let stats = "seen[2].triangles, seen[2].draw_calls";
    // The table's top, its four legs, one mesh, in a draw call, and the crate.
    assert_eq!(simulation.eval::<(u32, u32)>(stats), (72, 3));
    let frame = simulation.screenshot().unwrap();
    // Seen from the front, the table's top is a row of pixels from 0.4 to 0.5 up, tinted red.
    let [r, g, b, _] = frame.pixel(10, 5);
    assert!(r > 100 && g < 20 && b < 20, "{:?}", [r, g, b]);

    let missing: String = simulation.eval("tostring(missing)");
    assert!(missing.contains("none.glb"), "{missing}");
    let with_sprite: String = simulation.eval("tostring(with_sprite)");
    assert!(with_sprite.contains("no sprite"), "{with_sprite}");

    // Gone from disk, it loads all the same.
    fs::remove_file(folder.join("models/crate.glb")).unwrap();
    simulation.exec("reload = true");
    simulation.step();
    assert!(simulation.game().error().is_none());
    assert_eq!(simulation.eval::<u32>("again:triangle_count()"), 12);
}

#[test]
fn rays_hit_boxes_from_luau() {
    let folder = game(
        "hitting_boxes",
        &[(
            "main.luau",
            "local from, ahead = vector.create(0, 0, 5), vector.create(0, 0, -1)
            low, high = graphics.new_cuboid(2, 4, 2):bounds()
            hits = {
                graphics.hit_box(from, ahead, low, high),
                -- The corners either way round.
                graphics.hit_box(from, ahead, high, low),
                -- From inside.
                graphics.hit_box(vector.zero, ahead, low, high),
                -- Twice as far, in lengths of a direction half as long.
                graphics.hit_box(from, ahead / 2, low, high),
            }
            miss = graphics.hit_box(vector.create(3, 0, 5), ahead, low, high)
            empty = { graphics.new_mesh({ vertices = {}, indices = {} }):bounds() }",
        )],
    );
    let mut simulation = simulation(&folder);
    assert_eq!(
        simulation
            .eval::<(f32, f32, f32, f32, f32, f32)>("low.x, low.y, low.z, high.x, high.y, high.z"),
        (-1.0, -2.0, -1.0, 1.0, 2.0, 1.0)
    );
    assert_eq!(simulation.eval::<Vec<f32>>("hits"), [4.0, 4.0, 0.0, 8.0]);
    assert!(simulation.eval::<bool>("miss == nil"));
    assert_eq!(simulation.eval::<usize>("#empty"), 0);
}

/// Physics worlds of a scene's nodes.
#[cfg(feature = "physics")]
mod physics {
    use super::*;

    /// A yard fenced in 100 by 60, a wall from 10 to 20 across and 0 to 40 down, a cart scaled 2
    /// with a rider, and a walker.
    const YARD: &str = r##"return {
    format = 2,
    children = {
        { name = "fence", kind = "area", width = 100, height = 60 },
        { name = "wall", kind = "area", x = 10, width = 10, height = 40 },
        {
            name = "cart", kind = "node", x = 60, y = 20, scale_x = 2, scale_y = 2,
            children = { { name = "rider", kind = "node" } },
        },
        { name = "walker", kind = "node", x = 2, y = 10 },
    },
}"##;

    /// The yard, loaded as `yard`, with its fence's edges and its wall in `world`, and a 2 by 2
    /// box around a node's position, `BOX`.
    fn yard(test: &str) -> Simulation<LuauGame> {
        let main = "yard = scene.load('yard.scene.luau')
            walker = yard:find('walker')
            BOX = { x = -1, y = -1, width = 2, height = 2 }
            world = physics.world()
            world:add(yard:find('fence'), { edges = true })
            world:add(yard:find('wall'))";
        let folder = game(test, &[("yard.scene.luau", YARD), ("main.luau", main)]);
        simulation(&folder)
    }

    fn near(actual: f32, expected: f32) -> bool {
        (actual - expected).abs() < 0.02
    }

    #[test]
    fn nodes_stop_at_colliders_and_slide_along_them() {
        let mut simulation = yard("physics_stop");
        // The walker's box's right edge, at 3, reaches the wall at 10.
        let (dx, dy): (f32, f32) = simulation.eval("world:move(walker, 20, 0, BOX)");
        assert!(near(dx, 7.0) && dy == 0.0, "{dx}, {dy}");
        assert_eq!(
            simulation.eval::<(f32, f32)>("walker:position()"),
            (2.0 + dx, 10.0)
        );

        // Down and into the wall, it slides down beside it.
        let (dx, dy): (f32, f32) = simulation.eval("world:move(walker, 5, 20, BOX)");
        assert!(near(dx, 0.0) && near(dy, 20.0), "{dx}, {dy}");

        // The fence keeps it in.
        simulation.exec("world:move(walker, -20, 0, BOX)");
        let (x, y): (f32, f32) = simulation.eval("walker:position()");
        assert!(near(x, 1.0) && near(y, 30.0), "{x}, {y}");
    }

    #[test]
    fn colliders_follow_their_nodes() {
        let mut simulation = yard("physics_follow");
        simulation.exec("yard:find('wall'):set_position(30, 0)");
        let (dx, _): (f32, f32) = simulation.eval("world:move(walker, 50, 0, BOX)");
        assert!(near(dx, 27.0), "stopped at the wall where it went: {dx}");

        // Once it's gone, nothing but the fence is in the way.
        simulation.exec("yard:find('wall'):remove()");
        simulation.exec("world:move(walker, 100, 0, BOX)");
        let (x, _): (f32, f32) = simulation.eval("walker:position()");
        assert!(near(x, 99.0), "stopped at the fence's far side: {x}");

        // Nor the fence, once the world has no collider of it.
        simulation.exec("world:remove(yard:find('fence'))");
        assert_eq!(
            simulation.eval::<(f32, f32)>("world:move(walker, 10, 0, BOX)"),
            (10.0, 0.0)
        );
    }

    #[test]
    fn adding_a_node_again_changes_how_solid_it_is() {
        let mut simulation = yard("physics_again");
        // Inside the wall, only its edges are in the way.
        simulation.exec(
            "world:add(yard:find('wall'), { edges = true })
            walker:set_position(15, 10)",
        );
        assert_eq!(
            simulation.eval::<(f32, f32)>("world:move(walker, 0, 5, BOX)"),
            (0.0, 5.0)
        );
        let (dx, _): (f32, f32) = simulation.eval("world:move(walker, 10, 0, BOX)");
        assert!(near(dx, 4.0), "stopped at the wall's right edge: {dx}");
    }

    #[test]
    fn a_solid_node_is_not_in_its_own_way() {
        let mut simulation = yard("physics_own_way");
        // A box inside the wall, which is solid, moves it as freely as anything.
        let moved: (f32, f32) = simulation
            .eval("world:move(yard:find('wall'), 0, 15, { x = 1, y = 1, width = 8, height = 8 })");
        assert_eq!(moved, (0.0, 15.0));
        // And it's solid where it's gone, from 15 to 55 down.
        simulation.exec("walker:set_position(2, 30)");
        let (_, dy): (f32, f32) = simulation.eval("world:move(walker, 10, 0, BOX)");
        assert_eq!(dy, 0.0);
        let (x, _): (f32, f32) = simulation.eval("walker:position()");
        assert!(near(x, 9.0), "stopped at the wall: {x}");
    }

    #[test]
    fn nodes_move_in_the_scene_however_their_parents_stretch_them() {
        let mut simulation = yard("physics_parents");
        assert_eq!(
            simulation.eval::<(f32, f32)>("world:move(yard:find('cart/rider'), 10, -4, BOX)"),
            (10.0, -4.0)
        );
        assert_eq!(
            simulation.eval::<(f32, f32)>("yard:find('cart/rider'):world_position()"),
            (70.0, 16.0)
        );
        assert_eq!(
            simulation.eval::<(f32, f32)>("yard:find('cart/rider'):position()"),
            (5.0, -2.0),
            "half as far in the cart, which doubles what's in it"
        );
    }

    #[test]
    fn a_world_is_of_one_scene() {
        let mut simulation = yard("physics_one_scene");
        let err = simulation
            .try_exec("world:add(scene.load('yard.scene.luau'):find('wall'))")
            .unwrap_err()
            .to_string();
        assert!(err.contains("another scene"), "{err}");
        let err = simulation
            .try_exec("world:move(walker, 1, 0, { x = 0, y = 0, width = 0, height = 2 })")
            .unwrap_err()
            .to_string();
        assert!(err.contains("wider and taller than 0"), "{err}");
        // Removing a node of another scene, or one with no collider, does nothing.
        simulation.exec(
            "world:remove(scene.load('yard.scene.luau'):find('wall'))
            world:remove(walker)",
        );
        let (dx, _): (f32, f32) = simulation.eval("world:move(walker, 20, 0, BOX)");
        assert!(near(dx, 7.0), "{dx}");
    }

    /// A 3D view of a pen: a floor whose top is at `y` 0, a wall across `x` from 2 to 2.5, a crate
    /// half a unit across held up at 2, a turned group with a box in it, a group at -4 with a
    /// block half a unit across standing in it, and a walker.
    const PEN: &str = r##"return {
    format = 2,
    children = {
        {
            name = "view", kind = "view3d", width = 0, height = 0, stretch = { 1, 1 },
            camera = { position = { 0, 4, 8 }, target = { 0, 0, 0 } },
            children = {
                { name = "floor", kind = "mesh", shape = "cuboid", y = -0.5, size = { 20, 1, 20 } },
                { name = "wall", kind = "mesh", shape = "cuboid", x = 2.25, y = 1, size = { 0.5, 2, 20 } },
                { name = "crate", kind = "mesh", shape = "cuboid", x = -2, y = 2, size = { 0.5, 0.5, 0.5 } },
                {
                    name = "turned", kind = "node", z = -3, yaw = 0.5,
                    children = {
                        { name = "box", kind = "mesh", shape = "cuboid", y = 0.25, size = { 0.5, 0.5, 0.5 } },
                    },
                },
                {
                    name = "group", kind = "node", x = -4,
                    children = {
                        { name = "block", kind = "mesh", shape = "cuboid", y = 0.25, size = { 0.5, 0.5, 0.5 } },
                    },
                },
                { name = "walker", kind = "node", y = 0.002 },
            },
        },
        { name = "flat", kind = "rect", width = 1, height = 1 },
    },
}"##;

    /// The pen, loaded as `pen`, its view `view`, with the floor and the wall fixed and the crate
    /// dynamic in `world`, and the walker a character 0.4 across and 1 tall.
    fn pen(test: &str) -> Simulation<LuauGame> {
        let main = "pen = scene.load('pen.scene.luau')
            view = pen:find('view')
            walker = view:find('walker')
            crate = view:find('crate')
            world = physics.world_3d()
            world:add(view:find('floor'))
            world:add(view:find('wall'))
            world:add(crate, { body = 'dynamic', shape = 'box' })
            world:add(walker, { character = { radius = 0.2, height = 1 } })";
        let folder = game(test, &[("pen.scene.luau", PEN), ("main.luau", main)]);
        let mut simulation = simulation(&folder);
        simulation.step();
        simulation
    }

    /// Walks the walker by `motion` once a frame for `frames`, stepping the world, and returns
    /// where it ends up and whether it's on the ground.
    fn walk(simulation: &mut Simulation<LuauGame>, motion: &str, frames: u32) -> ([f32; 3], bool) {
        simulation.exec(&format!(
            "on_ground = false
            for _ = 1, {frames} do
                local _, grounded = world:walk(walker, {motion})
                on_ground = grounded
                world:step(1 / 60)
            end"
        ));
        let (x, y, z): (f32, f32, f32) = simulation
            .eval("walker:position_3d().x, walker:position_3d().y, walker:position_3d().z");
        ([x, y, z], simulation.eval("on_ground"))
    }

    #[test]
    fn dynamic_bodies_fall_and_move_their_nodes() {
        let mut simulation = pen("physics_3d_fall");
        assert!(!simulation.eval::<bool>("world:is_resting(crate)"));
        simulation.exec("for _ = 1, 240 do world:step(1 / 60) end");
        // Its middle half its height up, resting on the floor where it fell.
        let (x, y, z): (f32, f32, f32) =
            simulation.eval("crate:position_3d().x, crate:position_3d().y, crate:position_3d().z");
        assert!(
            near(x, -2.0) && near(y, 0.25) && near(z, 0.0),
            "{x}, {y}, {z}"
        );
        assert!(simulation.eval::<bool>("world:is_resting(crate)"));
        assert!(simulation.eval::<bool>("world:is_resting(view:find('floor'))"));

        // Pushed, it slides along the floor, slowing.
        simulation.exec("world:push(crate, vector.create(0.25, 0, 0))");
        let vx: f32 = simulation.eval("world:velocity(crate).x");
        assert!(vx > 1.0, "{vx}");
        simulation.exec("for _ = 1, 30 do world:step(1 / 60) end");
        let (x, slower): (f32, f32) =
            simulation.eval("crate:position_3d().x, world:velocity(crate).x");
        assert!(x > -1.9 && slower < vx, "{x}, {slower}");
        simulation.exec("world:set_velocity(crate, vector.create(0, 5, 0))");
        assert_eq!(simulation.eval::<f32>("world:velocity(crate).y"), 5.0);
    }

    #[test]
    fn moving_a_dynamic_body_s_node_moves_the_body() {
        let mut simulation = pen("physics_3d_teleport");
        simulation.exec(
            "crate:set_position_3d(vector.create(0, 5, 1))
            world:step(1 / 60)",
        );
        let (x, y, z): (f32, f32, f32) =
            simulation.eval("crate:position_3d().x, crate:position_3d().y, crate:position_3d().z");
        assert!(x == 0.0 && y < 5.0 && y > 4.9 && z == 1.0, "{x}, {y}, {z}");
    }

    #[test]
    fn bodies_that_move_follow_the_nodes_they_are_in() {
        let mut simulation = pen("physics_3d_carried");
        // A kinematic block, carried in front of the walker by the group it's in.
        simulation.exec(
            "world:add(view:find('group/block'), { body = 'kinematic' })
            view:find('group'):set_position_3d(vector.create(1, 0, 0))",
        );
        let (feet, _) = walk(&mut simulation, "vector.create(0.05, -0.01, 0)", 60);
        // Its side, at 0.2 from its middle, a hair from the block's at 0.75.
        assert!(near(feet[0], 0.55) && feet[0] < 0.55, "{feet:?}");

        // A dynamic one goes with its group too, staying where it is in it.
        simulation.exec(
            "world:add(view:find('group/block'), { body = 'dynamic', shape = 'box' })
            for _ = 1, 10 do world:step(1 / 60) end
            view:find('group'):set_position_3d(vector.create(-4, 0, 2))
            world:step(1 / 60)",
        );
        let (x, z): (f32, f32) = simulation.eval(
            "view:find('group/block'):position_3d().x, view:find('group/block'):position_3d().z",
        );
        assert!(near(x, 0.0) && near(z, 0.0), "{x}, {z}");
    }

    #[test]
    fn characters_walk_and_stop_at_what_is_solid() {
        let mut simulation = pen("physics_3d_walk");
        let (feet, grounded) = walk(&mut simulation, "vector.create(0.05, -0.01, 0)", 60);
        // Its side, at 0.2 from its middle, a hair from the wall's at 2.
        assert!(grounded);
        assert!(near(feet[0], 1.8) && feet[0] < 1.8, "{feet:?}");
        assert!(near(feet[1], 0.0) && feet[2] == 0.0, "{feet:?}");
        let (moved, grounded): (f32, bool) =
            simulation.eval("(function() local moved, grounded = world:walk(walker, vector.create(1, 0, 0)) return moved.x, grounded end)()");
        assert!(moved.abs() < 1.0e-3 && grounded, "{moved}");

        // Off the ground, it falls as it's told to.
        simulation.exec("walker:set_position_3d(vector.create(0, 1, 0))");
        let (feet, grounded) = walk(&mut simulation, "vector.create(0, -0.1, 0)", 5);
        assert!(!grounded && near(feet[1], 0.5), "{feet:?}");
    }

    #[test]
    fn characters_go_where_they_walk_and_are_put_where_scripts_put_them() {
        let mut simulation = pen("physics_3d_put");
        // Walked, its body goes there over the step, as fast as it walked, pushing what's in
        // its way.
        walk(&mut simulation, "vector.create(0.05, -0.01, 0)", 1);
        let vx: f32 = simulation.eval("world:velocity(walker).x");
        assert!(near(vx, 3.0), "{vx}");
        // Put somewhere by a script, as it starts over, it's there at once, sweeping through
        // nothing between.
        simulation.exec(
            "walker:set_position_3d(vector.create(-6, 0.002, 0))
            world:step(1 / 60)",
        );
        let vx: f32 = simulation.eval("world:velocity(walker).x");
        assert_eq!(vx, 0.0);
        let (feet, grounded) = walk(&mut simulation, "vector.create(0, -0.01, 0)", 1);
        assert!(grounded && near(feet[0], -6.0), "{feet:?}");
    }

    #[test]
    fn fixed_bodies_follow_their_nodes_and_go_with_them() {
        let mut simulation = pen("physics_3d_follow");
        simulation.exec("view:find('wall'):set_position_3d(vector.create(1.25, 1, 0))");
        let (feet, _) = walk(&mut simulation, "vector.create(0.05, -0.01, 0)", 60);
        assert!(
            near(feet[0], 0.8),
            "stopped at the wall where it went: {feet:?}"
        );

        // Once it's gone, nothing's in the way.
        simulation.exec("view:find('wall'):remove()");
        let (feet, _) = walk(&mut simulation, "vector.create(0.05, -0.01, 0)", 20);
        assert!(near(feet[0], 1.8), "{feet:?}");
        // Nor once the world has no body of it.
        simulation.exec("world:remove(view:find('floor'))");
        let (feet, grounded) = walk(&mut simulation, "vector.create(0, -0.1, 0)", 5);
        assert!(!grounded && near(feet[1], -0.5), "{feet:?}");
    }

    #[test]
    fn fixed_bodies_are_placed_however_their_parents_turn_them() {
        let mut simulation = pen("physics_3d_turned");
        // Two units out from the box's left side, turned as it is, walking straight at it.
        simulation.exec(
            "world:add(view:find('turned/box'), { shape = 'box' })
            walker:set_position_3d(vector.create(-2 * math.cos(0.5), 0.002, -3 + 2 * math.sin(0.5)))",
        );
        let (feet, _) = walk(
            &mut simulation,
            "vector.create(0.05 * math.cos(0.5), -0.01, -0.05 * math.sin(0.5))",
            60,
        );
        // Stopped its radius from the side, a quarter from the box's middle, and no farther along.
        let (sin, cos) = 0.5_f32.sin_cos();
        let (out, along) = (
            feet[0] * cos - (feet[2] + 3.0) * sin,
            feet[0] * sin + (feet[2] + 3.0) * cos,
        );
        assert!(
            near(out, -0.45) && along.abs() < 1.0e-3,
            "{feet:?}: {out}, {along}"
        );
    }

    #[test]
    fn worlds_are_of_one_view_and_say_what_is_wrong() {
        let mut simulation = pen("physics_3d_errors");
        let error = |simulation: &mut Simulation<LuauGame>, code: &str| {
            simulation.try_exec(code).unwrap_err().to_string()
        };
        let err = error(&mut simulation, "world:add(pen:find('flat'))");
        assert!(err.contains("isn't in a 3D view"), "{err}");
        let err = error(&mut simulation, "physics.world():add(walker)");
        assert!(err.contains("physics.world_3d()"), "{err}");
        let err = error(
            &mut simulation,
            "world:add(scene.load('pen.scene.luau'):find('view/wall'))",
        );
        assert!(err.contains("another 3D view"), "{err}");
        let err = error(
            &mut simulation,
            "world:add(view:find('wall'), { body = 'dynamic', shape = 'triangles' })",
        );
        assert!(err.contains("only fixed bodies"), "{err}");
        let err = error(
            &mut simulation,
            "world:add(view:find('turned/box'), { body = 'dynamic' })",
        );
        assert!(err.contains("turns or stretches"), "{err}");
        let err = error(&mut simulation, "world:add(view:find('turned'))");
        assert!(err.contains("shows nothing"), "{err}");
        let err = error(
            &mut simulation,
            "world:add(view:find('turned'), { character = {} })",
        );
        assert!(err.contains("give it a radius"), "{err}");
        let err = error(&mut simulation, "world:add(crate, { body = 'floating' })");
        assert!(err.contains("not \"floating\""), "{err}");
        let err = error(&mut simulation, "world:add(crate, { shape = 'cone' })");
        assert!(err.contains("not \"cone\""), "{err}");
        let err = error(
            &mut simulation,
            "world:add(crate, { body = 'dynamic', density = 0 })",
        );
        assert!(err.contains("density must be more than 0"), "{err}");
        let err = error(&mut simulation, "world:add(crate, { friction = -1 })");
        assert!(err.contains("friction must be 0 or more"), "{err}");
        // A character whose group is turned once it's been added can't walk in it.
        simulation.exec(
            "world:add(view:find('group/block'), { character = {} })
            view:find('group'):set_orientation(0.5)",
        );
        let err = error(
            &mut simulation,
            "world:walk(view:find('group/block'), vector.create(0.1, 0, 0))",
        );
        assert!(err.contains("turns or stretches"), "{err}");
        let err = error(&mut simulation, "world:walk(crate, vector.create(1, 0, 0))");
        assert!(err.contains("isn't a character"), "{err}");
        let err = error(&mut simulation, "world:velocity(view:find('turned'))");
        assert!(err.contains("has no body"), "{err}");
        // Removing a node with no body does nothing.
        simulation.exec("world:remove(view:find('turned'))");
    }

    #[test]
    #[cfg(feature = "models")]
    fn models_are_solid_where_their_triangles_are() {
        let main = "pen = scene.load('pen.scene.luau')
            view = pen:find('view')
            walker = view:find('walker')
            world = physics.world_3d()
            world:add(view:find('floor'))
            world:add(walker, { character = { radius = 0.2, height = 1 } })";
        let pen = PEN.replace(
            r#"{ name = "walker", kind = "node", y = 0.002 },"#,
            r#"{ name = "walker", kind = "node", y = 0.002 },
                { name = "crate_model", kind = "model", x = 1.5, model = "models/crate.glb" },"#,
        );
        let folder = game(
            "physics_3d_models",
            &[("pen.scene.luau", &pen), ("main.luau", main)],
        );
        copy_props(&folder);
        let mut simulation = simulation(&folder);
        simulation.exec("world:add(view:find('crate_model'))");
        let (feet, _) = walk(&mut simulation, "vector.create(0.05, -0.01, 0)", 60);
        let near_side: f32 = simulation.eval("({view:find('crate_model'):bounds_3d()})[1].x");
        assert!(
            feet[0] < near_side - 0.2 && feet[0] > near_side - 0.21,
            "{feet:?}, {near_side}"
        );
    }
}

/// Interfaces Luau games lay out in `gui(ui)`, driven by clicking their widgets by label.
#[cfg(feature = "egui")]
mod gui {
    use super::*;
    use wingine::input::MouseButton;

    /// A 400x300 window whose canvas is the window, so canvas pixels are points.
    const WINDOW: &str = "return { width = 400, height = 300 }";

    /// A game whose `gui` is `gui`, with its globals `state` set up first, run for two frames so
    /// its windows show.
    fn interface(test: &str, state: &str, gui: &str) -> Simulation<LuauGame> {
        let main = format!("{state}\nfunction gui(ui: Ui)\n{gui}\nend");
        let folder = game(test, &[("conf.luau", WINDOW), ("main.luau", &main)]);
        let mut simulation = simulation(&folder);
        simulation.run_frames(2);
        if let Some(err) = simulation.game().error() {
            panic!("the game stopped: {err}");
        }
        simulation
    }

    #[test]
    fn widgets_return_what_they_were_changed_to() {
        let mut simulation = interface(
            "gui_widgets",
            "clicks = 0
            checked = false
            mode = 'easy'
            name = ''
            submitted = nil",
            "ui:window({ title = 'Tools', x = 10, y = 10 }, function(ui)
                if ui:button('Press me') then
                    clicks += 1
                end
                checked = ui:checkbox('Checked', checked)
                ui:horizontal(function(ui)
                    mode = ui:radio(mode, 'easy', 'Easy')
                    mode = ui:radio(mode, 'hard', 'Hard')
                end)
                local entered
                name, entered = ui:text_input(name, 'Your name')
                if entered then
                    submitted = name
                end
            end)",
        );
        simulation.click_gui("Press me");
        assert_eq!(simulation.eval::<u32>("clicks"), 1);
        simulation.click_gui("Checked");
        assert!(simulation.eval::<bool>("checked"));
        simulation.click_gui("Hard");
        assert_eq!(simulation.eval::<String>("mode"), "hard");
        simulation.click_gui("Easy");
        assert_eq!(simulation.eval::<String>("mode"), "easy");

        simulation.click_gui("Your name");
        simulation.type_text("Ada");
        simulation.step();
        assert_eq!(simulation.eval::<String>("name"), "Ada");
        assert_eq!(simulation.eval::<Option<String>>("submitted"), None);
        simulation.tap(Key::Enter);
        simulation.step();
        assert_eq!(
            simulation.eval::<Option<String>>("submitted").as_deref(),
            Some("Ada")
        );
    }

    #[test]
    fn a_gui_defined_while_playing_is_laid_out() {
        let folder = game(
            "gui_later",
            &[
                ("conf.luau", WINDOW),
                (
                    "main.luau",
                    "function update(dt: number)
                        if input.key_pressed('G') then
                            function gui(ui: Ui)
                                ui:window('Later', function(ui)
                                    ui:label('Defined later')
                                end)
                            end
                        end
                    end",
                ),
            ],
        );
        let mut simulation = simulation(&folder);
        // Long enough for frames to stop running an interface the game has none of.
        simulation.run_frames(5);
        simulation.tap(Key::G);
        simulation.run_frames(3);
        assert!(simulation.game().error().is_none());
        assert!(simulation.gui_rect("Defined later").is_some());
    }

    #[test]
    fn a_combo_returns_the_choice_picked() {
        let mut simulation = interface(
            "gui_combo",
            "stage = 'start'",
            "ui:window('Quest', function(ui)
                stage = ui:combo('Stage', stage, { 'start', 'middle', 'end' })
            end)",
        );
        simulation.click_gui("Stage");
        simulation.run_frames(2);
        simulation.click_gui("middle");
        simulation.step();
        assert_eq!(simulation.eval::<String>("stage"), "middle");
    }

    #[test]
    fn sliders_and_number_fields_change_numbers() {
        let mut simulation = interface(
            "gui_numbers",
            "speed = 0
            coins = 5",
            "ui:panel({ side = 'left', size = 250 }, function(ui)
                speed = ui:slider('Speed', speed, 0, 10, 1)
                coins = ui:drag('Coins', coins, 1, 0, 9)
                ui:progress(speed / 10, 'How fast')
            end)",
        );
        // Dragged from its track's start to past its end. It shows its value after its track.
        let speed = simulation.gui_rect("Speed").unwrap();
        let y = speed.y + speed.h / 2.0;
        simulation.move_mouse(speed.x + 2.0, y);
        simulation.mouse_down(MouseButton::Left);
        simulation.step();
        simulation.move_mouse(speed.x + 300.0, y);
        simulation.step();
        simulation.mouse_up(MouseButton::Left);
        simulation.step();
        assert_eq!(simulation.eval::<f64>("speed"), 10.0);

        // Typed into, and kept within its range.
        let coins = *simulation.gui_rects("Coins").first().unwrap();
        simulation.click(
            MouseButton::Left,
            coins.x + coins.w / 2.0,
            coins.y + coins.h / 2.0,
        );
        simulation.step();
        simulation.key_down(Key::LeftControl);
        simulation.tap(Key::A);
        simulation.key_up(Key::LeftControl);
        simulation.type_text("42");
        simulation.tap(Key::Enter);
        simulation.run_frames(2);
        assert_eq!(simulation.eval::<f64>("coins"), 9.0);
    }

    #[test]
    fn sprites_show_in_the_interface() {
        let folder = game(
            "gui_sprite",
            &[
                ("conf.luau", WINDOW),
                (
                    "main.luau",
                    "local red = graphics.load_sprite_sheet('sheet.png', 1, 1):sprite(0, 0)
                    function draw()
                        graphics.clear('#000000')
                    end
                    function gui(ui: Ui)
                        ui:panel('left', function(ui)
                            ui:horizontal(function(ui)
                                ui:sprite(red, nil, 32, 32)
                                ui:label('Beside')
                            end)
                        end)
                    end",
                ),
            ],
        );
        write_png(&folder.join("sheet.png"), 2, 1, &[RED, BLUE]);
        let mut simulation = simulation(&folder);
        simulation.run_frames(2);
        // Left of the label beside it, in the middle of its row.
        let beside = simulation.gui_rect("Beside").unwrap();
        let [x, y] = [beside.x - 20.0, beside.y + beside.h / 2.0];
        let image = simulation.screenshot().unwrap();
        assert_eq!(image.pixel(x as u32, y as u32), RED);
    }

    #[test]
    fn containers_lay_out_what_their_functions_add() {
        let simulation = interface(
            "gui_containers",
            "",
            "ui:window('Outer', function(ui)
                ui:heading('Heading')
                ui:collapsing('Folded', function(ui)
                    ui:label('Hidden until opened')
                end)
                ui:grid('table', function(ui)
                    ui:label('Name')
                    ui:monospace('Value')
                    ui:end_row()
                end)
                ui:enabled(false, function(ui)
                    ui:button('Disabled')
                end)
                ui:group(function(ui)
                    ui:vertical(function(ui)
                        ui:small('Small')
                        ui:separator()
                        ui:space(4)
                        ui:label('Colored', '#ff0000')
                    end)
                end)
                ui:scroll(function(ui)
                    ui:label('Scrolled')
                end)
            end)",
        );
        for label in [
            "Heading", "Folded", "Name", "Value", "Disabled", "Small", "Colored", "Scrolled",
        ] {
            assert!(simulation.gui_rect(label).is_some(), "{label}");
        }
        assert!(simulation.gui_rect("Hidden until opened").is_none());
        let name = simulation.gui_rect("Name").unwrap();
        let value = simulation.gui_rect("Value").unwrap();
        assert!(
            value.x > name.x && (value.y - name.y).abs() < 4.0,
            "{name:?} {value:?}"
        );
    }

    #[test]
    fn errors_in_gui_stop_the_game() {
        let folder = game(
            "gui_error",
            &[
                ("conf.luau", WINDOW),
                (
                    "main.luau",
                    "function gui(ui: Ui)
                        ui:window('Broken', function(ui)
                            ui:panel('middle', function(ui) end)
                        end)
                    end",
                ),
            ],
        );
        let mut simulation = simulation(&folder);
        simulation.step();
        let err = stopped_with(&simulation);
        assert!(err.contains(r#"not "middle""#), "{err}");
    }

    #[test]
    fn a_ui_kept_after_gui_returns_fails() {
        let folder = game(
            "gui_kept",
            &[
                ("conf.luau", WINDOW),
                (
                    "main.luau",
                    "local kept = nil
                    function gui(ui: Ui)
                        kept = ui
                    end
                    function update()
                        if kept then
                            kept:label('Too late')
                        end
                    end",
                ),
            ],
        );
        let mut simulation = simulation(&folder);
        simulation.run_frames(2);
        let err = stopped_with(&simulation);
        assert!(err.contains("destructed"), "{err}");
    }
}

/// Games reloading in place, as they do watching their folders when a file changes.
mod reload {
    use std::{
        thread,
        time::{Duration, Instant},
    };

    use super::*;

    /// A scene whose root's props have a `title`.
    fn titled(title: &str) -> String {
        format!("return {{ format = 1, props = {{ title = '{title}' }} }}")
    }

    #[test]
    fn reloading_runs_the_changed_scripts_and_reads_assets_again() {
        let folder = game(
            "reload_changes",
            &[
                (
                    "main.luau",
                    "value = require('./module').value
                    room = scene.load('room.scene.luau')",
                ),
                ("module.luau", "return { value = 1 }"),
                ("room.scene.luau", &titled("Old")),
            ],
        );
        let mut simulation = simulation(&folder);
        simulation.step();
        assert_eq!(simulation.eval::<f64>("value"), 1.0);
        assert_eq!(simulation.eval::<String>("room:props().title"), "Old");

        fs::write(folder.join("module.luau"), "return { value = 2 }").unwrap();
        fs::write(folder.join("room.scene.luau"), titled("New")).unwrap();
        simulation.reload();
        assert!(simulation.game().error().is_none());
        assert_eq!(simulation.eval::<f64>("value"), 2.0);
        assert_eq!(simulation.eval::<String>("room:props().title"), "New");
    }

    #[test]
    fn what_reloading_returns_the_reloaded_game_gets() {
        let folder = game(
            "reload_kept",
            &[(
                "main.luau",
                "count = 0
                started = window.reloaded()
                function update() count += 1 end
                function reloading()
                    return { count = count, name = 'kept', list = { 1, 2 } }
                end",
            )],
        );
        let mut simulation = simulation(&folder);
        assert!(
            simulation.eval::<bool>("started == nil"),
            "it started fresh"
        );
        simulation.run_frames(3);

        simulation.reload();
        assert_eq!(
            simulation.eval::<(f64, String, f64, f64)>(
                "started.count, started.name, started.list[2], count"
            ),
            (3.0, "kept".into(), 2.0, 0.0)
        );
        // Each call returns a copy of its own.
        simulation.exec("window.reloaded().count = 99");
        assert_eq!(simulation.eval::<f64>("window.reloaded().count"), 3.0);

        // A game without `reloading` keeps nothing.
        fs::write(folder.join("main.luau"), "started = window.reloaded()").unwrap();
        simulation.reload();
        simulation.reload();
        assert!(simulation.eval::<bool>("started == nil"));
    }

    #[test]
    fn a_failing_reloading_keeps_nothing() {
        let folder = game(
            "reload_failing",
            &[("main.luau", "function reloading() error('no state') end")],
        );
        let mut simulation = simulation(&folder);
        // Its own `reloading` fails, then the next game's gives what saves can't hold.
        fs::write(
            folder.join("main.luau"),
            "started = window.reloaded()
            function reloading() return { act = function() end } end",
        )
        .unwrap();
        simulation.reload();
        assert!(simulation.game().error().is_none());
        assert!(simulation.eval::<bool>("started == nil"));
        simulation.reload();
        assert!(simulation.game().error().is_none());
        assert!(simulation.eval::<bool>("started == nil"));
    }

    #[test]
    fn reloaded_games_start_with_no_sounds_or_bindings() {
        let folder = game(
            "reload_reset",
            &[(
                "main.luau",
                "input.bind('jump', { 'Space' })
                audio.play(audio.load('sounds/blip.wav'))
                audio.play_music('sounds/blip.wav')",
            )],
        );
        let sounds = folder.join("sounds");
        fs::create_dir_all(&sounds).unwrap();
        let blip = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/sounds/blip.wav");
        fs::copy(blip, sounds.join("blip.wav")).unwrap();
        let mut simulation = simulation(&folder);
        simulation.step();
        assert_eq!(simulation.context().audio().sounds_playing(), 1);
        assert!(simulation.context().audio().music_playing());

        fs::write(
            folder.join("main.luau"),
            "function update() jumping = input.held('jump') end",
        )
        .unwrap();
        simulation.reload();
        simulation.step();
        assert_eq!(simulation.context().audio().sounds_playing(), 0);
        assert!(!simulation.context().audio().music_playing());
        simulation.hold(Key::Space, 0.1);
        assert!(!simulation.eval::<bool>("jumping"), "the action is unbound");
    }

    #[test]
    fn a_game_stopped_by_an_error_reloads_once_fixed_keeping_its_state() {
        let folder = game(
            "reload_fixed",
            &[(
                "main.luau",
                "count = 5
                function reloading() return { count = count } end
                function update() error('broken') end",
            )],
        );
        let mut simulation = simulation(&folder);
        simulation.step();
        assert!(stopped_with(&simulation).contains("broken"));

        fs::write(
            folder.join("main.luau"),
            "count = window.reloaded().count
            function update() count += 1 end",
        )
        .unwrap();
        simulation.reload();
        assert!(simulation.game().error().is_none());
        simulation.step();
        assert_eq!(simulation.eval::<f64>("count"), 6.0);
    }

    #[test]
    fn a_failed_reload_shows_its_error_and_keeps_the_state_for_the_next() {
        let folder = game(
            "reload_broken",
            &[(
                "main.luau",
                "count = 7
                function reloading() return { count = count } end",
            )],
        );
        let mut simulation = simulation(&folder);

        fs::write(folder.join("main.luau"), "count = = 1").unwrap();
        simulation.reload();
        let err = stopped_with(&simulation);
        assert!(err.contains("main.luau"), "{err}");
        // The old game went with its VM.
        assert!(simulation.eval::<bool>("count == nil"));
        simulation.step();

        fs::write(folder.join("main.luau"), "kept = window.reloaded()").unwrap();
        simulation.reload();
        assert!(simulation.game().error().is_none());
        assert_eq!(simulation.eval::<f64>("kept.count"), 7.0);
    }

    #[test]
    fn settings_are_read_again_but_the_window_keeps_its_own() {
        let folder = game(
            "reload_settings",
            &[("conf.luau", SMALL_WINDOW), ("main.luau", "")],
        );
        let mut simulation = simulation(&folder);
        fs::write(
            folder.join("conf.luau"),
            "return { title = 'Bigger', resolution = { width = 40, height = 40 } }",
        )
        .unwrap();
        simulation.reload();
        assert!(simulation.game().error().is_none());
        assert_eq!(
            simulation.eval::<(f32, f32)>("graphics.size()"),
            (20.0, 20.0)
        );

        fs::write(folder.join("conf.luau"), "return { colour = 'red' }").unwrap();
        simulation.reload();
        let err = stopped_with(&simulation);
        assert!(err.contains("conf.luau") && err.contains("colour"), "{err}");
    }

    #[test]
    fn games_showing_errors_start_stopped_by_them() {
        let folder = game(
            "showing_errors",
            &[("main.luau", "error('broken at start')")],
        );
        let config = runtime::config(&folder).unwrap();
        let mut simulation = Simulation::new(&config.window, |ctx| {
            LuauGame::load_showing_errors(ctx, &folder, &config)
        })
        .expect("a game that fails to load still starts");
        assert!(stopped_with(&simulation).contains("broken at start"));
        simulation.step();
        assert!(stopped_with(&simulation).contains("broken at start"));
    }

    #[test]
    fn watching_games_start_stopped_by_errors_and_reload_as_files_change() {
        let folder = game(
            "reload_watching",
            &[("main.luau", "error('broken at start')")],
        );
        let config = runtime::config(&folder).unwrap();
        let mut simulation = Simulation::new(&config.window, |ctx| {
            LuauGame::load_watching(ctx, &folder, &config)
        })
        .expect("a game that fails to load still starts");
        assert!(stopped_with(&simulation).contains("broken at start"));

        fs::write(folder.join("main.luau"), "fixed = true").unwrap();
        let start = Instant::now();
        while simulation.game().error().is_some() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "the game never reloaded"
            );
            thread::sleep(Duration::from_millis(20));
            simulation.step();
        }
        assert!(simulation.eval::<bool>("fixed"));
    }
}
