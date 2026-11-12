//! Tests for the runner of Luau games' own tests, `wingine test`, with small games and their
//! tests written to temporary folders.

use std::{
    fs,
    path::{Path, PathBuf},
};

use wingine::{
    headless::Image,
    runtime::testing::{self, OUTPUT_FOLDER, Options, Outcome, Report},
};

/// Writes a game made of `files`, each a path in the game's folder and its contents, to a fresh
/// folder named after `test`.
fn game(test: &str, files: &[(&str, &str)]) -> PathBuf {
    let folder = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("testing-tests")
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

/// Runs the tests of the game in `folder` with `options`.
fn run_with(folder: &Path, options: &Options) -> Report {
    let mut seen = Vec::new();
    let report = testing::run(folder, options, |outcome| seen.push(outcome.clone())).unwrap();
    assert_eq!(seen, report.outcomes, "each outcome is told as it comes");
    report
}

fn run(folder: &Path) -> Report {
    run_with(folder, &Options::default())
}

/// The outcome of the test titled `title`, as `file: name`.
fn outcome<'a>(report: &'a Report, title: &str) -> &'a Outcome {
    report
        .outcomes
        .iter()
        .find(|outcome| outcome.title() == title)
        .unwrap_or_else(|| panic!("no test {title}: {report}"))
}

/// Why the test titled `title` failed, which it should have.
fn failure(report: &Report, title: &str) -> String {
    let outcome = outcome(report, title);
    outcome
        .failure
        .clone()
        .unwrap_or_else(|| panic!("{title} passed"))
}

/// Asserts that every test passed, showing the failures if not.
fn assert_passed(report: &Report) {
    assert!(report.passed(), "{report}");
}

#[test]
fn tests_are_found_run_and_reported() {
    let folder = game(
        "found",
        &[
            ("main.luau", "value = 1"),
            (
                "a.test.luau",
                "test('passes', function()
                    assert_equal(value, 1)
                    assert_near(0.1 + 0.2, 0.3)
                    assert(true)
                end)
                test('compares', function()
                    assert_equal(value, 2, 'the value')
                end)
                test('raises', function()
                    error('broken')
                end)",
            ),
            ("levels/b.test.luau", "test('nested', function() end)"),
            (".hidden/c.test.luau", "test('hidden', function() end)"),
            ("helpers.luau", "test('not a test file', function() end)"),
        ],
    );
    let report = run(&folder);
    let titles: Vec<String> = report.outcomes.iter().map(Outcome::title).collect();
    assert_eq!(
        titles,
        [
            "a.test.luau: passes",
            "a.test.luau: compares",
            "a.test.luau: raises",
            "levels/b.test.luau: nested",
        ]
    );
    assert!(outcome(&report, "a.test.luau: passes").passed());
    let compared = failure(&report, "a.test.luau: compares");
    assert!(
        compared.starts_with("a.test.luau:7: the value: expected 2, got 1"),
        "{compared}"
    );
    let raised = failure(&report, "a.test.luau: raises");
    assert!(raised.starts_with("a.test.luau:10: broken"), "{raised}");
    assert_eq!((report.passed_count(), report.failed_count()), (2, 2));
    let shown = report.to_string();
    assert!(shown.contains("---- a.test.luau: raises ----"), "{shown}");
    assert!(
        shown.ends_with("test result: FAILED. 2 passed; 2 failed; 0 ignored; 0 filtered out"),
        "{shown}"
    );
}

#[test]
fn each_test_starts_from_a_fresh_game() {
    let folder = game(
        "fresh",
        &[
            ("main.luau", "state = { visits = 0 }"),
            (
                "fresh.test.luau",
                "test('first', function()
                    state.visits += 1
                    marked = true
                    save.write('progress', { level = 3 })
                    input.bind('jump', { 'Space' })
                    sim.step()
                end)
                test('second', function()
                    assert_equal(state.visits, 0)
                    assert_equal(marked, nil)
                    assert_equal(save.list(), {})
                    assert_equal(sim.frame_count(), 0)
                    local ok = pcall(sim.press_action, 'jump')
                    assert(not ok, 'the binding went with the game')
                end)",
            ),
        ],
    );
    assert_passed(&run(&folder));
}

#[test]
fn tests_read_the_games_state_as_it_is_now() {
    let folder = game(
        "current",
        &[
            (
                "main.luau",
                "world = { frames = 0, player = { x = 0 } }
                function update()
                    world.frames += 1
                    world.player = { x = world.frames }
                end
                function has_test_globals()
                    return sim ~= nil or test ~= nil or assert_equal ~= nil
                end",
            ),
            (
                "current.test.luau",
                "test('reads', function()
                    sim.step(2)
                    assert_equal(world.frames, 2)
                    assert_equal(world.player.x, 2)
                    world.frames = 10
                    sim.step()
                    assert_equal(world.player, { x = 11 })
                    assert(not has_test_globals(), 'the game sees none of the tests globals')
                end)",
            ),
        ],
    );
    assert_passed(&run(&folder));
}

#[test]
fn waits_and_input_reach_the_game() {
    let folder = game(
        "input",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                "seen = { frames = 0, jumps = 0, keys = 0, clicks = 0, fires = 0, text = '' }
                input.bind('jump', { 'Space' })
                input.bind('fire', { 'South' })
                function update(dt)
                    seen.frames += 1
                    if input.pressed('jump') then seen.jumps += 1 end
                    if input.key_pressed('W') then seen.keys += 1 end
                    seen.held = input.key_held('W')
                    if input.mouse_pressed('right') then
                        seen.clicks += 1
                        seen.at = { input.mouse_position() }
                    end
                    if input.pressed('fire') then seen.fires += 1 end
                    if input.key_pressed('C') then input.set_mouse_captured(true) end
                    seen.moved = { input.mouse_motion() }
                    seen.lean = input.button_strength('LeftStickRight')
                    seen.text ..= input.text()
                end
                function close_requested()
                    seen.asked = true
                    return false
                end",
            ),
            (
                "input.test.luau",
                "test('time', function()
                    wait(0.5)
                    assert_equal(seen.frames, 30)
                    sim.run_for(1)
                    assert_equal(seen.frames, 90)
                    wait_until(function() return seen.frames >= 100 end)
                    assert_equal(seen.frames, 100)
                    assert_equal(sim.frame_count(), 100)
                end)
                test('keys and actions', function()
                    sim.tap('Space')
                    sim.tap_action('jump')
                    sim.hold_action('jump', 0.1)
                    assert_equal(seen.jumps, 3)
                    sim.press('W')
                    sim.step()
                    assert(seen.held)
                    sim.release('W')
                    sim.hold('W', 0.2)
                    assert_equal(seen.keys, 2)
                    -- Keys pressed type what they would on a US keyboard.
                    assert_equal(seen.text, '   ww')
                    sim.type_text('hi')
                    sim.step()
                    assert_equal(seen.text, '   wwhi')
                end)
                test('the mouse', function()
                    sim.click(4, 6, 'right')
                    assert_equal(seen.clicks, 1)
                    assert_equal(seen.at, { 4, 6 })
                    -- The mouse's moves reach a game that has captured it, and no other.
                    sim.move_mouse_by(7, 7)
                    sim.step()
                    assert_equal(seen.moved, { 0, 0 })
                    sim.tap('C')
                    sim.move_mouse_by(12, -3)
                    sim.move_mouse_by(0.5, 0)
                    sim.step()
                    assert_equal(seen.moved, { 12.5, -3 })
                    sim.step()
                    assert_equal(seen.moved, { 0, 0 })
                end)
                test('gamepads', function()
                    local pad = sim.connect_gamepad()
                    assert_equal(pad, 1)
                    sim.tap_button(pad, 'South')
                    sim.tap_action('fire')
                    assert_equal(seen.fires, 2)
                    sim.move_stick(pad, 'left', 0.5, 0)
                    sim.step()
                    assert(seen.lean > 0, 'the stick leans right')
                end)
                test('the window', function()
                    assert_equal(sim.request_close(), false)
                    assert(seen.asked)
                    assert_equal(sim.has_quit(), false)
                end)",
            ),
        ],
    );
    assert_passed(&run(&folder));
}

#[test]
fn tests_fail_when_the_game_stops_or_their_time_runs_out() {
    let folder = game(
        "stopping",
        &[
            (
                "main.luau",
                "function update()
                    if broken then error('the gears jammed') end
                end",
            ),
            (
                "stopping.test.luau",
                "test('the game stops', function()
                    broken = true
                    sim.step()
                end)
                test('never', function()
                    wait_until(function() return false end)
                end, { time_limit = 0.5 })
                test('too long', function()
                    sim.run_for(61)
                end)
                test('long enough', function()
                    sim.run_for(61)
                end, { time_limit = 62 })",
            ),
        ],
    );
    let report = run(&folder);
    let stopped = failure(&report, "stopping.test.luau: the game stops");
    assert!(
        stopped.starts_with("the game stopped: ") && stopped.contains("the gears jammed"),
        "{stopped}"
    );
    let never = failure(&report, "stopping.test.luau: never");
    assert!(
        never.starts_with("it ran past its time limit of 0.5s of game time"),
        "{never}"
    );
    assert!(
        never.contains("stopping.test.luau:6"),
        "it says where: {never}"
    );
    let long = failure(&report, "stopping.test.luau: too long");
    assert!(long.contains("time limit of 60s"), "{long}");
    assert!(outcome(&report, "stopping.test.luau: long enough").passed());
}

#[test]
fn what_sim_cant_do_is_an_error_where_it_was_asked() {
    let folder = game(
        "refusals",
        &[
            ("main.luau", "input.bind('fire', { 'South' })"),
            (
                "refusals.test.luau",
                "test('an unbound action', function()
                    sim.tap_action('jump')
                end)
                test('an unknown key', function()
                    sim.tap('Spcae')
                end)
                test('a gamepad not connected', function()
                    sim.tap_button(2, 'South')
                end)
                test('caught', function()
                    local ok, err = pcall(sim.tap_action, 'fire')
                    assert(not ok)
                    assert(string.find(err, 'none is connected'), err)
                    sim.step()
                end)",
            ),
        ],
    );
    let report = run(&folder);
    let unbound = failure(&report, "refusals.test.luau: an unbound action");
    assert!(
        unbound.starts_with("refusals.test.luau:2: sim.tap_action: `jump` isn't bound to anything"),
        "{unbound}"
    );
    let key = failure(&report, "refusals.test.luau: an unknown key");
    assert!(
        key.starts_with("refusals.test.luau:5: sim.tap: argument 1:") && key.contains("Spcae"),
        "{key}"
    );
    let gamepad = failure(&report, "refusals.test.luau: a gamepad not connected");
    assert!(
        gamepad.starts_with("refusals.test.luau:8: sim.tap_button: gamepad 2 isn't connected"),
        "{gamepad}"
    );
    assert!(outcome(&report, "refusals.test.luau: caught").passed());
}

#[test]
fn files_that_dont_load_fail_once() {
    let folder = game(
        "unloaded",
        &[
            ("main.luau", ""),
            ("syntax.test.luau", "test('one', function() end"),
            (
                "twice.test.luau",
                "test('same', function() end)
                test('same', function() end)",
            ),
            (
                "late.test.luau",
                "test('declares', function()
                    test('later', function() end)
                end)",
            ),
        ],
    );
    let report = run(&folder);
    let syntax = failure(&report, "syntax.test.luau");
    assert!(syntax.starts_with("the file didn't load: "), "{syntax}");
    let twice = failure(&report, "twice.test.luau");
    assert!(
        twice.contains(r#"the file already declares a test called "same""#),
        "{twice}"
    );
    let late = failure(&report, "late.test.luau: declares");
    assert!(
        late.contains("tests are declared as their file loads"),
        "{late}"
    );

    let broken = game(
        "unloaded_game",
        &[
            ("main.luau", "error('no game today')"),
            ("game.test.luau", "test('one', function() end)"),
        ],
    );
    let report = run(&broken);
    let failed = failure(&report, "game.test.luau");
    assert!(
        failed.starts_with("the game didn't load: ") && failed.contains("no game today"),
        "{failed}"
    );
}

#[test]
fn test_files_get_the_games_own_modules() {
    let folder = game(
        "modules",
        &[
            ("lib/score.luau", "return { points = 0 }"),
            (
                "main.luau",
                "score = require('./lib/score') score.points = 5",
            ),
            (
                "tests/score.test.luau",
                "local score_module = require('../lib/score')
                test('shared', function()
                    assert_equal(score_module.points, 5)
                    assert(score_module == score, 'the same table as the game has')
                end)",
            ),
        ],
    );
    assert_passed(&run(&folder));
}

#[test]
fn filters_pick_tests_by_file_and_name() {
    let folder = game(
        "filtered",
        &[
            ("main.luau", ""),
            (
                "one.test.luau",
                "test('red', function() end) test('blue', function() end)",
            ),
            ("two.test.luau", "test('red', function() end)"),
        ],
    );
    let options = Options {
        filter: Some("red".into()),
        ..Default::default()
    };
    let report = run_with(&folder, &options);
    let titles: Vec<String> = report.outcomes.iter().map(Outcome::title).collect();
    assert_eq!(titles, ["one.test.luau: red", "two.test.luau: red"]);
    assert_eq!(report.filtered_out, 1);

    let options = Options {
        filter: Some("two.test".into()),
        ..Default::default()
    };
    assert_eq!(run_with(&folder, &options).outcomes.len(), 1);
}

#[test]
fn ignored_tests_run_only_when_asked_for() {
    let folder = game(
        "ignored",
        &[
            ("main.luau", ""),
            (
                "slow.test.luau",
                "test('quick', function() end)
                test('benchmark', function() end, { ignored = true })
                test('screenshots', function() end, { ignored = true, time_limit = 120 })
                test('kept', function() end, { ignored = false })",
            ),
            (
                "bad.test.luau",
                "test('which', function() end, { ignored = 'yes' })",
            ),
        ],
    );
    let titles =
        |report: &Report| -> Vec<String> { report.outcomes.iter().map(Outcome::title).collect() };
    let report = run(&folder);
    let bad = failure(&report, "bad.test.luau");
    assert!(bad.contains("`ignored` is true or false"), "{bad}");
    assert_eq!(
        titles(&report)[1..],
        ["slow.test.luau: quick", "slow.test.luau: kept"]
    );
    assert_eq!((report.ignored, report.filtered_out), (2, 0));
    assert!(
        report
            .to_string()
            .ends_with("1 failed; 2 ignored; 0 filtered out"),
        "{report}"
    );

    // Asked for, only the ignored run, and the filter picks among them.
    let ignored = Options {
        ignored: true,
        ..Default::default()
    };
    let report = run_with(&folder, &ignored);
    assert_eq!(
        titles(&report)[1..],
        ["slow.test.luau: benchmark", "slow.test.luau: screenshots"]
    );
    assert_eq!((report.ignored, report.filtered_out), (0, 2));
    let picked = Options {
        ignored: true,
        filter: Some("bench".into()),
        ..Default::default()
    };
    let report = run_with(&folder, &picked);
    assert_eq!(titles(&report)[1..], ["slow.test.luau: benchmark"]);
    assert_eq!((report.ignored, report.filtered_out), (0, 3));
}

#[test]
fn tests_read_the_frames_pixels_and_files() {
    let folder = game(
        "pixels",
        &[
            ("conf.luau", SMALL_WINDOW),
            (
                "main.luau",
                "function draw()
                    graphics.clear('#204060')
                    graphics.fill_rect(5, 0, 1, 20, '#ff00ff')
                end",
            ),
            ("data/levels.txt", "first\nsecond\n"),
            (
                "tests/pixels.test.luau",
                "test('pixels', function()
                    sim.step()
                    assert_equal({ sim.pixel(0, 0) }, { 0x20, 0x40, 0x60, 255 })
                    assert_equal({ sim.pixel(5, 19) }, { 255, 0, 255, 255 })
                    assert_equal({ sim.pixel(19, 19) }, { 0x20, 0x40, 0x60, 255 })
                    local ok, err = pcall(sim.pixel, 20, 0)
                    assert(not ok and string.find(err, 'from 0, 0 to 19, 19, not 20, 0'), err)
                    ok, err = pcall(sim.pixel, 1.5, 0)
                    assert(not ok, 'a pixel is whole')
                end)
                test('files', function()
                    assert_equal(sim.read_file('../data/levels.txt'), 'first\\nsecond\\n')
                    local ok, err = pcall(sim.read_file, 'missing.txt')
                    assert(not ok and string.find(err, \"sim.read_file: can't read missing.txt\"), err)
                end)",
            ),
        ],
    );
    assert_passed(&run(&folder));
}

#[test]
fn screenshots_match_their_references() {
    let shade = |color: &str| format!("function draw() graphics.clear('{color}') end");
    let test = "test('court', function()
        sim.step()
        assert(sim.matches_screenshot('shots/court.png'))
    end)
    test('saved', function()
        sim.step()
        sim.screenshot('shots/saved.png')
    end)";
    let folder = game(
        "screenshots",
        &[
            ("conf.luau", SMALL_WINDOW),
            ("main.luau", &shade("#204060")),
            ("court.test.luau", test),
        ],
    );
    let reference = folder.join("shots/court.png");

    // Without a reference, it fails, saving the frame.
    let report = run(&folder);
    let missing = failure(&report, "court.test.luau: court");
    assert!(
        missing.contains("there's no shots/court.png to compare the frame with"),
        "{missing}"
    );
    let output = folder.join(OUTPUT_FOLDER);
    assert!(output.join("shots/court.actual.png").is_file());
    assert_eq!(
        Image::load_png(folder.join("shots/saved.png"))
            .unwrap()
            .width(),
        20
    );

    // Updating saves it, and then it matches, clearing what the run before left.
    let update = Options {
        update_screenshots: true,
        ..Default::default()
    };
    let report = run_with(&folder, &update);
    assert_passed(&report);
    assert_eq!(
        outcome(&report, "court.test.luau: court").written,
        ["shots/court.png"]
    );
    assert!(!output.exists());
    let saved = Image::load_png(&reference).unwrap();
    assert_eq!(saved.pixel(10, 10), [0x20, 0x40, 0x60, 0xff]);
    assert_passed(&run(&folder));

    // A different frame fails, showing where it differs.
    fs::write(folder.join("main.luau"), shade("#206040")).unwrap();
    let report = run(&folder);
    let differs = failure(&report, "court.test.luau: court");
    assert!(
        differs.contains("400 pixels of the frame differ from shots/court.png by more than 2"),
        "{differs}"
    );
    assert!(
        differs.contains(".test-output/shots/court.diff.png"),
        "{differs}"
    );
    let diff = Image::load_png(output.join("shots/court.diff.png")).unwrap();
    assert_eq!(diff.pixel(0, 0), [255, 0, 255, 255]);

    // Within a tolerance, it matches.
    let tolerant = "test('court', function()
        sim.step()
        assert(sim.matches_screenshot('shots/court.png', { tolerance = 40 }))
    end)";
    fs::write(folder.join("court.test.luau"), tolerant).unwrap();
    assert_passed(&run(&folder));
}

#[cfg(feature = "egui")]
#[test]
fn tests_click_the_games_interface() {
    let folder = game(
        "interface",
        &[
            ("conf.luau", "return { width = 400, height = 300 }"),
            (
                "main.luau",
                "clicks = 0
                function gui(ui: Ui)
                    ui:window('Tools', function(ui)
                        if ui:button('Go') then clicks += 1 end
                    end)
                end",
            ),
            (
                "interface.test.luau",
                "test('clicks', function()
                    -- A window shows from the frame after it opens.
                    sim.step(2)
                    local go = sim.gui_rect('Go')
                    assert(go and go.width > 0, 'the button is there')
                    assert_equal(sim.gui_rect('Stop'), nil)
                    sim.click_gui('Go')
                    assert_equal(clicks, 1)
                    local ok, err = pcall(sim.click_gui, 'Stop')
                    assert(not ok and string.find(err, 'no widget labelled \"Stop\"'), err)
                end)",
            ),
        ],
    );
    assert_passed(&run(&folder));
}
