//! Tests of Luau games, written in Luau, which `wingine test path/to/game` runs, as does [`run`].
//!
//! Tests are in files named `*.test.luau` anywhere in the game's folder (but in hidden folders,
//! whose names start with `.`), and each declares its tests with `test`:
//!
//! ```lua
//! test("the ball waits for a serve", function()
//!     sim.run_for(1)
//!     assert_equal(pong.phase, "ready")
//!     sim.tap("Space")
//!     assert_equal(pong.phase, "rally")
//! end)
//! ```
//!
//! Each test runs against a fresh copy of the game, without a window, as a
//! [`Simulation`] runs it: its `main.luau` has run, but no frame has, its saves are in a folder
//! of its own that starts empty, and its audio is silent. Frames come at a fixed 60 a second, and
//! only when the test runs them. Test files run in the order of their paths, and their tests in
//! the order they declare them, each loading the game and the file again.
//!
//! Test code runs in the game's VM, lent the context as `update` is, so it reads and changes the
//! game's globals as they are at the time (`pong.ball.vx`), calls the engine's functions, and
//! `require`s the game's modules, getting the same ones the game has. A test file's own globals
//! are the game's too, but for these, which only tests have:
//!
//! - `test(name, run, options)` declares a test, as the file loads. Names are unique in a file.
//!   `options.time_limit` is how many seconds of game time it may run, 60 by default.
//! - `assert_equal(actual, expected, message)` fails the test unless `actual` and `expected` are
//!   equal, tables by their fields, saying what each was. `assert_near(actual, expected,
//!   tolerance, message)` fails it unless two numbers are within `tolerance` (1e-6 by default).
//!   Luau's own `assert(value, message)` fails it unless `value` is true.
//! - `sim` drives the game, as a player would, and runs frames. Its functions are listed below.
//!
//! `wait(seconds)` and `wait_until(condition)` work in tests as they do in tasks, running a frame
//! at a time: `wait_until(function() return pong.phase == "pause" end)`.
//!
//! A test fails if it raises an error, as a failed assertion does, saying where; if the game stops
//! with an error in a frame it runs; or if it runs past its time limit, as one waiting for
//! something that never happens does. A game or test file that fails to load fails once, for the
//! file.
//!
//! `sim`'s functions, whose keys, mouse buttons and gamepad buttons are named as the engine names
//! them (`"Space"`, `"left"`, `"South"`), and where a mouse button left out is `"left"`:
//!
//! - Time: `sim.step(frames)` runs that many frames (1 by default), `sim.run_for(seconds)` runs
//!   that much game time, rounded to whole frames, and `sim.frame_count()` returns how many have
//!   run.
//! - Keys: `sim.press(key)` and `sim.release(key)` hold a key down and let it go;
//!   `sim.tap(key)` presses it, runs a frame and lets it go; `sim.hold(key, seconds)` holds it for
//!   that long; and `sim.type_text(text)` types text without pressing keys. Held keys repeat as a
//!   system repeats them.
//! - Actions, through what the game binds them to, a key or else a gamepad's button:
//!   `sim.press_action(action)`, `sim.release_action(action)`, `sim.tap_action(action)` and
//!   `sim.hold_action(action, seconds)`.
//! - The mouse, in canvas pixels: `sim.move_mouse(x, y)`, `sim.press_mouse(button)`,
//!   `sim.release_mouse(button)`, and `sim.click(x, y, button)`, which moves there, presses,
//!   runs a frame and lets go; and `sim.scroll(x, y)`, which turns the wheel `y` notches away
//!   from the player, and tilts it `x` right.
//! - Gamepads: `sim.connect_gamepad(name)` connects one and returns its number;
//!   `sim.disconnect_gamepad(gamepad)`; `sim.press_button(gamepad, button, strength)` (1 by
//!   default), `sim.release_button(gamepad, button)`, `sim.tap_button(gamepad, button)` and
//!   `sim.hold_button(gamepad, button, seconds)`; and `sim.move_stick(gamepad, stick, x, y)`,
//!   `stick` being `"left"` or `"right"`, and `x` and `y` from -1 to 1, with `y` down.
//! - The game's interface, with the `egui` feature: `sim.click_gui(label)` clicks the widget
//!   labelled `label`, laid out in the frame before, and runs the frame that completes the click;
//!   `sim.gui_rect(label)` returns where it is, a rect in canvas pixels, or `nil`.
//! - The window: `sim.resize(width, height)` resizes it, `sim.request_close()` presses its close
//!   button, returning whether it would close, and `sim.has_quit()` tells whether the game called
//!   `window.quit`.
//! - Screenshots, of the latest frame as the window would show it, at its size:
//!   `sim.screenshot(path)` saves one as a PNG, and `sim.matches_screenshot(path, options)`
//!   compares one with the PNG at `path`, returning `true` if they match, or `false` and what
//!   differs, so `assert(sim.matches_screenshot("start.png"))` fails saying it. Pixels match
//!   when no channel differs by more than `options.tolerance` (2 by default, so frames rendered
//!   on different GPUs match), and the screenshot matches when at most `options.pixels` don't (0
//!   by default). Paths are relative to the test file. A screenshot that doesn't match, or has
//!   nothing to match, is saved in the game's `.test-output` folder, with a picture of where it
//!   differs, which the run clears as it starts. With [`Options::update_screenshots`]
//!   (`wingine test --update`), it's saved at `path` instead, and matches.
//!
//! A function that can't do what it's asked, such as pressing an action the game hasn't bound,
//! raises an error where the test called it.

use std::{
    cell::RefCell,
    fmt::{self, Write as _},
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use mlua::{FromLua, Lua, MultiValue, Table, Thread, thread::ThreadStatus};

use super::{Config, LuauGame, config};
use crate::{
    Result,
    headless::{Comparison, Image, Simulation},
    input::{GamepadButton, Key, MouseButton, Stick},
};

/// The end of the names of test files.
const SUFFIX: &str = ".test.luau";
/// How deep into the game's folders test files are looked for.
const MAX_DEPTH: usize = 8;
/// The folder in the game's that screenshots that don't match go to.
pub const OUTPUT_FOLDER: &str = ".test-output";
/// Seconds of game time a test may run, unless it says otherwise.
const TIME_LIMIT: f32 = 60.0;
/// How much a channel of a screenshot's pixel may differ from its reference's, by default.
const TOLERANCE: u8 = 2;

/// The functions of `sim`, each a request a test makes of the runner, which carries it out
/// between frames.
const REQUESTS: &[&str] = &[
    "step",
    "run_for",
    "frame_count",
    "press",
    "release",
    "tap",
    "hold",
    "type_text",
    "press_action",
    "release_action",
    "tap_action",
    "hold_action",
    "move_mouse",
    "press_mouse",
    "release_mouse",
    "click",
    "scroll",
    "connect_gamepad",
    "disconnect_gamepad",
    "press_button",
    "release_button",
    "tap_button",
    "hold_button",
    "move_stick",
    "resize",
    "request_close",
    "has_quit",
    "screenshot",
    "matches_screenshot",
];

/// The requests about the game's interface, with the `egui` feature.
#[cfg(feature = "egui")]
const GUI_REQUESTS: &[&str] = &["click_gui", "gui_rect"];
#[cfg(not(feature = "egui"))]
const GUI_REQUESTS: &[&str] = &[];

/// What tests have that the game doesn't, before `sim`'s functions: what makes assertions'
/// messages. The functions are locals, returned at the end, so the game never sees them.
const PRELUDE: &str = r#"
local sim = {}

-- Whether `a` and `b` are equal, tables by their fields.
local function same(a: any, b: any): boolean
    if a == b then
        return true
    end
    if type(a) ~= "table" or type(b) ~= "table" then
        return false
    end
    for key, value in a do
        if not same(value, b[key]) then
            return false
        end
    end
    for key in b do
        if a[key] == nil then
            return false
        end
    end
    return true
end

-- Numbers before strings, each in order, then the rest.
local function key_order(a: any, b: any): boolean
    local ta, tb = type(a), type(b)
    if ta == tb and (ta == "number" or ta == "string") then
        return a < b
    end
    return ta < tb
end

-- `value` written out for a message: strings quoted, and tables with their fields in order.
local function show(value: any, depth: number): string
    if type(value) == "string" then
        return string.format("%q", value)
    elseif type(value) ~= "table" then
        return tostring(value)
    elseif depth > 4 then
        return "{ ... }"
    end
    local keys = {}
    for key in value do
        table.insert(keys, key)
    end
    if #keys == 0 then
        return "{}"
    end
    table.sort(keys, key_order)
    local is_list = #keys == #value
    local fields = {}
    for _, key in keys do
        local shown = show(value[key], depth + 1)
        if is_list then
            table.insert(fields, shown)
        elseif type(key) == "string" and string.match(key, "^[%a_][%w_]*$") then
            table.insert(fields, `{key} = {shown}`)
        else
            table.insert(fields, `[{show(key, depth + 1)}] = {shown}`)
        end
    end
    return "{ " .. table.concat(fields, ", ") .. " }"
end

local function assert_equal(actual: any, expected: any, message: string?)
    if not same(actual, expected) then
        local prefix = if message then `{message}: ` else ""
        error(`{prefix}expected {show(expected, 0)}, got {show(actual, 0)}`, 2)
    end
end

local function assert_near(actual: number, expected: number, tolerance: number?, message: string?)
    local tolerance = tolerance or 1e-6
    if type(actual) ~= "number" or not (math.abs(actual - expected) <= tolerance) then
        local prefix = if message then `{message}: ` else ""
        error(`{prefix}expected {show(expected, 0)} within {tolerance}, got {show(actual, 0)}`, 2)
    end
end
"#;

/// The prelude of tests: [`PRELUDE`], then `sim`'s functions, each yielding its request to the
/// runner and raising the error it answers with where the test called it.
fn prelude() -> String {
    let mut prelude = PRELUDE.to_owned();
    for name in REQUESTS.iter().chain(GUI_REQUESTS) {
        // Each is a function of its own, never inlined, so the error is the caller's.
        let _ = write!(
            prelude,
            r#"
function sim.{name}(...)
    local reply = table.pack(coroutine.yield("{name}", ...))
    if not reply[1] then
        error(reply[2], 2)
    end
    return table.unpack(reply, 2, reply.n)
end
"#
        );
    }
    prelude.push_str(
        "\nreturn { sim = sim, assert_equal = assert_equal, assert_near = assert_near }\n",
    );
    prelude
}

/// How [`run`] runs a game's tests.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Runs only the tests whose files and names, as [`Outcome::title`] writes them, contain this.
    pub filter: Option<String>,
    /// Saves screenshots that don't match their references, or have none, as their references,
    /// rather than failing.
    pub update_screenshots: bool,
}

/// How a test went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    /// The test file, by its path in the game's folder, with `/` between names.
    pub file: String,
    /// The test's name, or `None` when the file's tests couldn't run at all, as when the game or
    /// the file failed to load.
    pub name: Option<String>,
    /// Why it failed, or `None` if it passed.
    pub failure: Option<String>,
    /// The references it saved, updating screenshots, by their paths in the game's folder.
    pub written: Vec<String>,
}

impl Outcome {
    pub fn passed(&self) -> bool {
        self.failure.is_none()
    }

    /// The file and the name, as `pong.test.luau: serving`.
    pub fn title(&self) -> String {
        match &self.name {
            Some(name) => format!("{}: {name}", self.file),
            None => self.file.clone(),
        }
    }
}

/// How a run of a game's tests went.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    /// Each test's outcome, in the order they ran.
    pub outcomes: Vec<Outcome>,
    /// How many tests the filter left out.
    pub filtered_out: usize,
}

impl Report {
    /// Whether every test that ran passed.
    pub fn passed(&self) -> bool {
        self.outcomes.iter().all(Outcome::passed)
    }

    pub fn passed_count(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|outcome| outcome.passed())
            .count()
    }

    pub fn failed_count(&self) -> usize {
        self.outcomes.len() - self.passed_count()
    }
}

/// The failures, each with why, then a summary, as `cargo test` writes them.
impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let failures: Vec<_> = self.outcomes.iter().filter(|o| !o.passed()).collect();
        if !failures.is_empty() {
            writeln!(f, "failures:")?;
            for outcome in &failures {
                let failure = outcome.failure.as_deref().unwrap_or_default();
                writeln!(f, "\n---- {} ----\n{failure}", outcome.title())?;
            }
            writeln!(f)?;
        }
        let result = if self.passed() { "ok" } else { "FAILED" };
        write!(
            f,
            "test result: {result}. {} passed; {} failed; {} filtered out",
            self.passed_count(),
            self.failed_count(),
            self.filtered_out
        )
    }
}

/// Runs the tests of the Luau game in `folder`, as the module docs say, calling `on_outcome` with
/// each outcome as it comes, and returns them all. Only an error reading the game's `conf.luau`,
/// or clearing its `.test-output` folder, ends the run with an error; the rest are failures.
pub fn run(
    folder: impl AsRef<Path>,
    options: &Options,
    mut on_outcome: impl FnMut(&Outcome),
) -> Result<Report> {
    let folder = folder.as_ref();
    let config = config(folder)?;
    let output = folder.join(OUTPUT_FOLDER);
    if output.is_dir() {
        fs::remove_dir_all(&output).map_err(|source| crate::Error::WriteFile {
            path: output.clone(),
            source,
        })?;
    }
    let mut report = Report::default();
    for file in test_files(folder) {
        let mut record = |outcome: Outcome| {
            on_outcome(&outcome);
            report.outcomes.push(outcome);
        };
        let filtered_out = run_file(folder, &config, &file, options, &mut record);
        report.filtered_out += filtered_out;
    }
    Ok(report)
}

/// The test files in `folder`, by their paths in it with `/` between names, in order.
fn test_files(folder: &Path) -> Vec<String> {
    fn visit(folder: &Path, path: &str, depth: usize, found: &mut Vec<String>) {
        let Ok(read) = fs::read_dir(folder.join(path)) else {
            return;
        };
        for entry in read.filter_map(std::result::Result::ok) {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let inside = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}/{name}")
            };
            match entry.file_type() {
                Ok(kind) if kind.is_dir() && depth < MAX_DEPTH => {
                    visit(folder, &inside, depth + 1, found);
                }
                Ok(kind) if kind.is_file() && name.ends_with(SUFFIX) => found.push(inside),
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    visit(folder, "", 0, &mut found);
    found.sort();
    found
}

/// A test, as a file declared it.
struct Declaration {
    name: String,
    run: mlua::Function,
    time_limit: f32,
}

/// The tests a file declares as it loads.
#[derive(Default)]
struct Declared {
    tests: Vec<Declaration>,
    /// Whether the file is still loading, so it may declare more.
    loading: bool,
}

/// A game with a test file loaded in it, and the tests it declared.
struct Loaded {
    // The tests' functions go before the VM they're in.
    tests: Vec<Declaration>,
    simulation: Simulation<LuauGame>,
}

impl Loaded {
    fn names(&self) -> Vec<&str> {
        self.tests.iter().map(|test| test.name.as_str()).collect()
    }
}

/// Runs the tests in `file` in the game in `folder` that `options` picks, each against a fresh
/// game, recording their outcomes, and returns how many it left out.
fn run_file(
    folder: &Path,
    config: &Config,
    file: &str,
    options: &Options,
    record: &mut impl FnMut(Outcome),
) -> usize {
    let path = folder.join(file);
    let outcome = |name: Option<&str>, failure: Option<String>, written| Outcome {
        file: file.to_owned(),
        name: name.map(str::to_owned),
        failure: failure.map(|failure| tidy(folder, &failure)),
        written,
    };
    // The first game loaded finds the file's tests, and runs the first.
    let mut fresh = match load(folder, config, &path) {
        Ok(loaded) => Some(loaded),
        Err(failure) => {
            record(outcome(None, Some(failure), Vec::new()));
            return 0;
        }
    };
    let names: Vec<String> = fresh.as_ref().map_or_else(Vec::new, |loaded| {
        loaded.names().into_iter().map(str::to_owned).collect()
    });
    let mut filtered_out = 0;
    for (index, name) in names.iter().enumerate() {
        let title = format!("{file}: {name}");
        if options
            .filter
            .as_ref()
            .is_some_and(|filter| !title.contains(filter.as_str()))
        {
            filtered_out += 1;
            continue;
        }
        let loaded = match fresh.take().map_or_else(|| load(folder, config, &path), Ok) {
            Ok(loaded) if loaded.names() == names => loaded,
            Ok(_) => {
                let failure = "the file declared different tests when it loaded again".to_owned();
                record(outcome(Some(name), Some(failure), Vec::new()));
                continue;
            }
            Err(failure) => {
                record(outcome(Some(name), Some(failure), Vec::new()));
                continue;
            }
        };
        let Loaded {
            tests,
            mut simulation,
        } = loaded;
        let mut runner = Runner {
            lua: simulation.game().scripts().lua().clone(),
            folder,
            test_folder: path.parent().unwrap_or(folder).to_owned(),
            update_screenshots: options.update_screenshots,
            time_limit: TIME_LIMIT,
            end_frame: 0,
            written: Vec::new(),
        };
        let result = runner.run(&mut simulation, &tests[index]);
        // The VM closes with the simulation, while it holds the GPU the textures its globals hold
        // are on: so the runner's handle on the VM, and the tests' functions, go first.
        let written = std::mem::take(&mut runner.written);
        drop(runner);
        drop(tests);
        drop(simulation);
        record(outcome(Some(name), result.err(), written));
    }
    filtered_out
}

/// `failure` with paths in the game's `folder` from there, and without the lines of tracebacks
/// that are the runner's own.
fn tidy(folder: &Path, failure: &str) -> String {
    let folder = format!("{}{}", folder.display(), std::path::MAIN_SEPARATOR);
    failure
        .replace(&folder, "")
        .lines()
        .filter(|line| {
            let line = line.trim_start();
            !(line.starts_with("[C]: in function 'error'") || line.starts_with("test prelude:"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Starts the game in `folder` with its settings, `config`, and loads the test file at `path` in
/// it, or says why it couldn't.
fn load(folder: &Path, config: &Config, path: &Path) -> std::result::Result<Loaded, String> {
    let mut simulation = Simulation::new(&config.window, |ctx| {
        LuauGame::load_with(ctx, folder, config)
    })
    .map_err(|err| format!("the game didn't load: {err}"))?;
    let tests =
        declare(&mut simulation, path).map_err(|err| format!("the file didn't load: {err}"))?;
    Ok(Loaded { tests, simulation })
}

/// Loads the test file at `path` in the simulation's game, with an environment of its own, and
/// returns the tests it declared.
fn declare(simulation: &mut Simulation<LuauGame>, path: &Path) -> Result<Vec<Declaration>> {
    let source = fs::read_to_string(path).map_err(|source| crate::Error::ReadFile {
        path: path.to_owned(),
        source,
    })?;
    let (game, ctx) = simulation.parts();
    let lua = game.scripts().lua();
    let declared = Rc::new(RefCell::new(Declared {
        loading: true,
        ..Default::default()
    }));
    let environment = environment(lua, &declared)?;
    let chunk = lua
        .load(source)
        .set_name(format!("@{}", path.display()))
        .set_environment(environment);
    game.lent.lend(ctx, || Ok(chunk.exec()?))?;
    let mut declared = declared.borrow_mut();
    declared.loading = false;
    Ok(std::mem::take(&mut declared.tests))
}

/// The environment of a test file: a table of the globals only tests have, `test`, `sim`,
/// `assert_equal` and `assert_near`, which reads and writes the game's globals through its
/// metatable. Unlike the game's own, it isn't a "safe" environment, so Luau doesn't resolve the
/// game's globals' fields that the file names as it loads: `pong.phase` reads the field each time.
fn environment(lua: &Lua, declared: &Rc<RefCell<Declared>>) -> mlua::Result<Table> {
    let globals = lua.globals();
    let environment = lua.create_table()?;
    let metatable = lua.create_table()?;
    metatable.set("__index", globals.clone())?;
    metatable.set("__newindex", globals)?;
    environment.set_metatable(Some(metatable))?;
    let prelude: Table = lua
        .load(prelude())
        .set_name("=test prelude")
        .set_environment(environment.clone())
        .eval()?;
    for pair in prelude.pairs::<String, mlua::Value>() {
        let (name, value) = pair?;
        environment.raw_set(name, value)?;
    }
    environment.raw_set("test", test_function(lua, Rc::clone(declared))?)?;
    Ok(environment)
}

/// `test(name, run, options)`, which declares a test in `declared`.
fn test_function(lua: &Lua, declared: Rc<RefCell<Declared>>) -> mlua::Result<mlua::Function> {
    lua.create_function(
        move |_, (name, run, options): (String, mlua::Function, Option<Table>)| {
            let mut declared = declared.borrow_mut();
            if !declared.loading {
                let message = "tests are declared as their file loads, not as they run";
                return Err(mlua::Error::runtime(message));
            }
            if declared.tests.iter().any(|test| test.name == name) {
                let message = format!("the file already declares a test called {name:?}");
                return Err(mlua::Error::runtime(message));
            }
            let mut time_limit = TIME_LIMIT;
            if let Some(options) = options {
                for pair in options.pairs::<String, mlua::Value>() {
                    let (option, value) = pair?;
                    match (option.as_str(), value) {
                        ("time_limit", mlua::Value::Number(seconds))
                            if seconds > 0.0 && seconds.is_finite() =>
                        {
                            time_limit = seconds as f32;
                        }
                        ("time_limit", mlua::Value::Integer(seconds)) if seconds > 0 => {
                            time_limit = seconds as f32;
                        }
                        ("time_limit", _) => {
                            let message = "`time_limit` is seconds of game time, above 0";
                            return Err(mlua::Error::runtime(message));
                        }
                        _ => {
                            let message = format!("a test has a `time_limit`, not `{option}`");
                            return Err(mlua::Error::runtime(message));
                        }
                    }
                }
            }
            declared.tests.push(Declaration {
                name,
                run,
                time_limit,
            });
            Ok(())
        },
    )
}

/// Why a request wasn't carried out.
enum Refusal {
    /// It can't be, as when it names a key there isn't: an error where the test asked.
    Raise(String),
    /// The test is over: the game stopped, or the test ran past its time limit.
    Fail(String),
}

type Reply = std::result::Result<Vec<mlua::Value>, Refusal>;

/// Runs a test, carrying out its requests.
struct Runner<'a> {
    lua: Lua,
    /// The game's folder.
    folder: &'a Path,
    /// The folder the test file is in, which screenshots' paths are relative to.
    test_folder: PathBuf,
    update_screenshots: bool,
    /// The test's time limit, in seconds of game time, and the frame it runs out at.
    time_limit: f32,
    end_frame: u64,
    /// The references it saved.
    written: Vec<String>,
}

impl Runner<'_> {
    /// Runs `test` to its end, carrying out what it asks, and says why it failed, if it did.
    fn run(
        &mut self,
        simulation: &mut Simulation<LuauGame>,
        test: &Declaration,
    ) -> std::result::Result<(), String> {
        let limit = u64::from(Simulation::<LuauGame>::frames_in(test.time_limit));
        self.time_limit = test.time_limit;
        self.end_frame = simulation.frame_count() + limit;
        let thread = self
            .lua
            .create_thread(test.run.clone())
            .map_err(|err| self.message(&err))?;
        let mut reply = MultiValue::new();
        loop {
            let asked = resume(simulation, &thread, reply).map_err(|err| self.message(&err))?;
            if thread.status() != ThreadStatus::Resumable {
                return Ok(());
            }
            if asked.is_empty() {
                // As `wait` and `wait_until` yield, for a frame, whose time `wait` counts.
                if let Err(Refusal::Raise(failure) | Refusal::Fail(failure)) =
                    self.frames(simulation, 1)
                {
                    return Err(self.at(&thread, failure));
                }
                let dt = f64::from(Simulation::<LuauGame>::FRAME_TIME);
                reply = MultiValue::from_vec(vec![mlua::Value::Number(dt)]);
                continue;
            }
            let answer = self.carry_out(simulation, asked);
            reply = match answer {
                Ok(results) => std::iter::once(mlua::Value::Boolean(true))
                    .chain(results)
                    .collect(),
                Err(Refusal::Raise(message)) => {
                    let message = mlua::Value::String(
                        self.lua
                            .create_string(&message)
                            .map_err(|err| err.to_string())?,
                    );
                    MultiValue::from_vec(vec![mlua::Value::Boolean(false), message])
                }
                Err(Refusal::Fail(failure)) => return Err(self.at(&thread, failure)),
            };
        }
    }

    /// `failure`, which ended the test while it waited, with where it was waiting.
    fn at(&self, thread: &Thread, failure: String) -> String {
        let traceback = self
            .lua
            .globals()
            .get::<Table>("debug")
            .and_then(|debug| debug.get::<mlua::Function>("traceback"))
            .and_then(|traceback| traceback.call::<String>(thread.clone()));
        match traceback {
            Ok(traceback) if !traceback.trim().is_empty() => {
                format!("{failure}\nstack traceback:\n{}", traceback.trim_end())
            }
            _ => failure,
        }
    }

    /// Carries out the request the test yielded.
    fn carry_out(&mut self, simulation: &mut Simulation<LuauGame>, asked: MultiValue) -> Reply {
        let mut values = asked.into_iter();
        let first = values.next().unwrap_or(mlua::Value::Nil);
        let name = match &first {
            mlua::Value::String(name) => name.to_string_lossy(),
            _ => String::new(),
        };
        if !REQUESTS.contains(&name.as_str()) && !GUI_REQUESTS.contains(&name.as_str()) {
            return Err(Refusal::Fail(format!(
                "the test yielded {}, which isn't something it can ask for: yield only through \
                 `sim`'s functions, `wait` and `wait_until`",
                first.type_name()
            )));
        }
        let args = Args {
            lua: self.lua.clone(),
            values: values.collect(),
        };
        self.request(simulation, &name, &args)
            .map_err(|refusal| match refusal {
                Refusal::Raise(message) => Refusal::Raise(format!("sim.{name}: {message}")),
                fail => fail,
            })
    }

    /// Carries out the request `name`, with `args`.
    fn request(&mut self, simulation: &mut Simulation<LuauGame>, name: &str, args: &Args) -> Reply {
        let nothing = Ok(Vec::new());
        match name {
            "step" => {
                let frames = args.optional::<u32>(0)?.unwrap_or(1);
                self.frames(simulation, frames)?;
                nothing
            }
            "run_for" => {
                let frames = args.seconds(0)?;
                self.frames(simulation, frames)?;
                nothing
            }
            "frame_count" => args.results((simulation.frame_count() as f64,)),
            "press" => {
                simulation.key_down(args.get::<Key>(0)?);
                nothing
            }
            "release" => {
                simulation.key_up(args.get::<Key>(0)?);
                nothing
            }
            "tap" => {
                let key = args.get::<Key>(0)?;
                simulation.key_down(key);
                let ran = self.frames(simulation, 1);
                simulation.key_up(key);
                ran.and(nothing)
            }
            "hold" => {
                let (key, frames) = (args.get::<Key>(0)?, args.seconds(1)?);
                self.check_time(simulation, frames)?;
                simulation.key_down(key);
                let ran = self.frames(simulation, frames);
                simulation.key_up(key);
                ran.and(nothing)
            }
            "type_text" => {
                simulation.type_text(&args.get::<String>(0)?);
                nothing
            }
            "press_action" => {
                let action = args.get::<String>(0)?;
                simulation
                    .try_action_down(&action)
                    .map_err(Refusal::Raise)?;
                nothing
            }
            "release_action" => {
                simulation.action_up(&args.get::<String>(0)?);
                nothing
            }
            "tap_action" | "hold_action" => {
                let action = args.get::<String>(0)?;
                let frames = if name == "tap_action" {
                    1
                } else {
                    args.seconds(1)?
                };
                self.check_time(simulation, frames)?;
                simulation
                    .try_action_down(&action)
                    .map_err(Refusal::Raise)?;
                let ran = self.frames(simulation, frames);
                simulation.action_up(&action);
                ran.and(nothing)
            }
            "move_mouse" => {
                simulation.move_mouse(args.get(0)?, args.get(1)?);
                nothing
            }
            "press_mouse" => {
                simulation.mouse_down(args.button(0)?);
                nothing
            }
            "release_mouse" => {
                simulation.mouse_up(args.button(0)?);
                nothing
            }
            "click" => {
                let (x, y, button) = (args.get(0)?, args.get(1)?, args.button(2)?);
                simulation.move_mouse(x, y);
                simulation.mouse_down(button);
                let ran = self.frames(simulation, 1);
                simulation.mouse_up(button);
                ran.and(nothing)
            }
            "scroll" => {
                simulation.scroll(args.get(0)?, args.get(1)?);
                nothing
            }
            "connect_gamepad" => {
                let name = args.optional::<String>(0)?;
                let number = simulation.connect_gamepad(name.as_deref().unwrap_or("Gamepad"));
                args.results((number,))
            }
            "disconnect_gamepad" => {
                let gamepad = args.gamepad(simulation, 0)?;
                simulation.disconnect_gamepad(gamepad);
                nothing
            }
            "press_button" => {
                let (gamepad, button) = (args.gamepad(simulation, 0)?, args.get(1)?);
                let strength = args.optional::<f32>(2)?.unwrap_or(1.0);
                if !(0.0..=1.0).contains(&strength) {
                    let message = format!("a button is pushed from 0 to 1, not {strength}");
                    return Err(Refusal::Raise(message));
                }
                simulation.push_button(gamepad, button, strength);
                nothing
            }
            "release_button" => {
                let (gamepad, button) = (args.gamepad(simulation, 0)?, args.get(1)?);
                simulation.button_up(gamepad, button);
                nothing
            }
            "tap_button" | "hold_button" => {
                let gamepad = args.gamepad(simulation, 0)?;
                let button: GamepadButton = args.get(1)?;
                let frames = if name == "tap_button" {
                    1
                } else {
                    args.seconds(2)?
                };
                self.check_time(simulation, frames)?;
                simulation.button_down(gamepad, button);
                let ran = self.frames(simulation, frames);
                simulation.button_up(gamepad, button);
                ran.and(nothing)
            }
            "move_stick" => {
                let gamepad = args.gamepad(simulation, 0)?;
                let stick = match args.get::<String>(1)?.as_str() {
                    "left" => Stick::Left,
                    "right" => Stick::Right,
                    other => {
                        let message = format!(r#"a stick is "left" or "right", not {other:?}"#);
                        return Err(Refusal::Raise(message));
                    }
                };
                let (x, y): (f32, f32) = (args.get(2)?, args.get(3)?);
                if !(-1.0..=1.0).contains(&x) || !(-1.0..=1.0).contains(&y) {
                    let message = format!("a stick moves from -1 to 1 each way, not {x}, {y}");
                    return Err(Refusal::Raise(message));
                }
                simulation.move_stick(gamepad, stick, x, y);
                nothing
            }
            #[cfg(feature = "egui")]
            "click_gui" => {
                let label = args.get::<String>(0)?;
                let Some(rect) = simulation.gui_rect(&label) else {
                    let message = format!(
                        "the interface laid out no widget labelled {label:?} in the frame before"
                    );
                    return Err(Refusal::Raise(message));
                };
                self.check_time(simulation, 2)?;
                simulation.move_mouse(rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
                simulation.mouse_down(MouseButton::Left);
                let ran = self.frames(simulation, 1);
                simulation.mouse_up(MouseButton::Left);
                ran?;
                // egui sees the release that completes the click in the next.
                self.frames(simulation, 1)?;
                nothing
            }
            #[cfg(feature = "egui")]
            "gui_rect" => {
                let rect = simulation.gui_rect(&args.get::<String>(0)?);
                args.results((rect,))
            }
            "resize" => {
                let (width, height): (u32, u32) = (args.get(0)?, args.get(1)?);
                if width == 0 || height == 0 {
                    let message = format!("a window can't be {width}x{height}");
                    return Err(Refusal::Raise(message));
                }
                simulation.resize(width, height);
                nothing
            }
            "request_close" => {
                let closes = simulation.request_close();
                self.check_game(simulation)?;
                args.results((closes,))
            }
            "has_quit" => args.results((simulation.has_quit(),)),
            "screenshot" => {
                let path = self.test_folder.join(args.get::<String>(0)?);
                let frame = simulation.screenshot().map_err(raise)?;
                save(&frame, &path)?;
                nothing
            }
            "matches_screenshot" => {
                let path = args.get::<String>(0)?;
                let options = args.optional::<Table>(1)?;
                self.matches_screenshot(simulation, &path, options)
            }
            _ => unreachable!("every request is carried out"),
        }
    }

    /// Runs `frames` frames, unless they'd run past the test's time limit, stopping at once if the
    /// game stops with an error.
    fn frames(&mut self, simulation: &mut Simulation<LuauGame>, frames: u32) -> Reply {
        self.check_time(simulation, frames)?;
        for _ in 0..frames {
            simulation.step();
            self.check_game(simulation)?;
        }
        Ok(Vec::new())
    }

    /// Fails the test if `frames` more would run past its time limit.
    fn check_time(&self, simulation: &Simulation<LuauGame>, frames: u32) -> Reply {
        if simulation.frame_count() + u64::from(frames) > self.end_frame {
            return Err(Refusal::Fail(format!(
                "it ran past its time limit of {}s of game time: give it longer with \
                 `test(name, run, {{ time_limit = seconds }})`",
                self.time_limit
            )));
        }
        Ok(Vec::new())
    }

    /// Fails the test if the game has stopped with an error.
    fn check_game(&self, simulation: &Simulation<LuauGame>) -> Reply {
        match simulation.game().error() {
            Some(err) => Err(Refusal::Fail(format!("the game stopped: {err}"))),
            None => Ok(Vec::new()),
        }
    }

    /// Whether the latest frame matches the PNG at `path`, as `sim.matches_screenshot` answers:
    /// `true`, or `false` and why not, saving the frame and where it differs in the output folder,
    /// or when updating screenshots, saving it at `path`, and `true`.
    fn matches_screenshot(
        &mut self,
        simulation: &mut Simulation<LuauGame>,
        path: &str,
        options: Option<Table>,
    ) -> Reply {
        let (mut tolerance, mut pixels) = (TOLERANCE, 0);
        if let Some(options) = options {
            for pair in options.pairs::<String, mlua::Value>() {
                let (option, value) = pair.map_err(raise)?;
                match option.as_str() {
                    "tolerance" => tolerance = self.lua.unpack(value).map_err(raise)?,
                    "pixels" => pixels = self.lua.unpack(value).map_err(raise)?,
                    _ => {
                        let message =
                            format!("a screenshot has a `tolerance` and `pixels`, not `{option}`");
                        return Err(Refusal::Raise(message));
                    }
                }
            }
        }
        let frame = simulation.screenshot().map_err(raise)?;
        let reference_path = self.test_folder.join(path);
        let shown = self.in_folder(&reference_path);
        let (problem, reference) = if reference_path.is_file() {
            let reference = Image::load_png(&reference_path).map_err(raise)?;
            let comparison = frame.compare(&reference, tolerance);
            if comparison.differs_by_at_most(pixels) {
                return Ok(vec![mlua::Value::Boolean(true)]);
            }
            let problem = match comparison {
                Comparison::SameSize { differing, .. } => format!(
                    "{differing} pixels of the frame differ from {shown} by more than {tolerance}"
                ),
                Comparison::DifferentSizes => format!(
                    "the frame is {}x{}, and {shown} is {}x{}",
                    frame.width(),
                    frame.height(),
                    reference.width(),
                    reference.height()
                ),
            };
            (problem, Some(reference))
        } else {
            let problem = format!(
                "there's no {shown} to compare the frame with (`wingine test --update` saves it)"
            );
            (problem, None)
        };
        if self.update_screenshots {
            save(&frame, &reference_path)?;
            self.written.push(shown);
            return Ok(vec![mlua::Value::Boolean(true)]);
        }
        let output = self.folder.join(OUTPUT_FOLDER).join(&shown);
        let actual = output.with_extension("actual.png");
        save(&frame, &actual)?;
        let mut message = format!("{problem}: the frame is at {}", self.in_folder(&actual));
        if let Some(diff) = reference.and_then(|reference| frame.diff(&reference, tolerance)) {
            let diff_path = output.with_extension("diff.png");
            save(&diff, &diff_path)?;
            let _ = write!(
                message,
                ", and where it differs, in magenta, at {}",
                self.in_folder(&diff_path)
            );
        }
        let message = self.lua.create_string(&message).map_err(raise)?;
        Ok(vec![
            mlua::Value::Boolean(false),
            mlua::Value::String(message),
        ])
    }

    /// What `err`, which ended the test, says: Luau's message and traceback.
    fn message(&self, err: &mlua::Error) -> String {
        match err {
            mlua::Error::RuntimeError(message) => message.clone(),
            err => err.to_string(),
        }
    }

    /// `path`, in the game's folder, as its path from there with `/` between names, or as it is
    /// if it's elsewhere.
    fn in_folder(&self, path: &Path) -> String {
        let inside = path.strip_prefix(self.folder).unwrap_or(path);
        inside.to_string_lossy().replace('\\', "/")
    }
}

/// Resumes the test's `thread` with `args`, lending the game the context as `update` does, and
/// returns what it yielded, or its error.
fn resume(
    simulation: &mut Simulation<LuauGame>,
    thread: &Thread,
    args: MultiValue,
) -> mlua::Result<MultiValue> {
    let (game, ctx) = simulation.parts();
    let mut resumed = None;
    // Lending never fails by itself: only `resume` does.
    let _ = game.lent.lend(ctx, || {
        resumed = Some(thread.resume::<MultiValue>(args));
        Ok(())
    });
    resumed.expect("lending runs what it's given")
}

/// Saves `image` as a PNG at `path`, making its folder.
fn save(image: &Image, path: &Path) -> std::result::Result<(), Refusal> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(raise)?;
    }
    image.save_png(path).map_err(raise)
}

fn raise(err: impl fmt::Display) -> Refusal {
    Refusal::Raise(err.to_string())
}

/// The arguments of a request.
struct Args {
    lua: Lua,
    values: Vec<mlua::Value>,
}

impl Args {
    /// Argument `index`, counting from 0.
    fn get<T: FromLua>(&self, index: usize) -> std::result::Result<T, Refusal> {
        let value = self.values.get(index).cloned().unwrap_or(mlua::Value::Nil);
        self.lua
            .unpack(value)
            .map_err(|err| Refusal::Raise(format!("argument {}: {err}", index + 1)))
    }

    /// Argument `index`, if it's given.
    fn optional<T: FromLua>(&self, index: usize) -> std::result::Result<Option<T>, Refusal> {
        self.get(index)
    }

    /// Argument `index` as seconds of game time, in whole frames.
    fn seconds(&self, index: usize) -> std::result::Result<u32, Refusal> {
        let seconds: f32 = self.get(index)?;
        if !(seconds >= 0.0 && seconds.is_finite()) {
            let message = format!("time is seconds, 0 or more, not {seconds}");
            return Err(Refusal::Raise(message));
        }
        Ok(Simulation::<LuauGame>::frames_in(seconds))
    }

    /// Argument `index` as a mouse button, the left if it's not given.
    fn button(&self, index: usize) -> std::result::Result<MouseButton, Refusal> {
        Ok(self.optional(index)?.unwrap_or(MouseButton::Left))
    }

    /// Argument `index` as the number of a connected gamepad.
    fn gamepad(
        &self,
        simulation: &mut Simulation<LuauGame>,
        index: usize,
    ) -> std::result::Result<u32, Refusal> {
        let gamepad: u32 = self.get(index)?;
        let input = &simulation.context().input;
        if !input.gamepad(gamepad).is_some_and(|pad| pad.is_connected()) {
            let message = format!(
                "gamepad {gamepad} isn't connected: connect it first with `sim.connect_gamepad()`"
            );
            return Err(Refusal::Raise(message));
        }
        Ok(gamepad)
    }

    /// `results` as a request's results.
    fn results(&self, results: impl mlua::IntoLuaMulti) -> Reply {
        let results = results.into_lua_multi(&self.lua).map_err(raise)?;
        Ok(results.into_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fields the definitions file declares `sim` has, in order, which are those with the
    /// `egui` feature.
    #[cfg(feature = "egui")]
    fn declared_sim() -> Vec<String> {
        let definitions = super::super::DEFINITIONS.replace("\r\n", "\n");
        let block = definitions
            .split_once("\ndeclare sim: {\n")
            .and_then(|(_, rest)| rest.split_once("\n}"))
            .expect("the definitions declare `sim`")
            .0;
        let mut fields: Vec<String> = block
            .lines()
            .filter_map(|line| line.strip_prefix("    "))
            .filter(|line| !line.starts_with("--"))
            .filter_map(|line| line.split_once(": ").map(|(name, _)| name.to_owned()))
            .collect();
        fields.sort();
        fields
    }

    #[cfg(feature = "egui")]
    #[test]
    fn the_definitions_declare_every_function_of_sim() {
        let mut requests: Vec<String> = REQUESTS
            .iter()
            .chain(GUI_REQUESTS)
            .map(|name| (*name).to_owned())
            .collect();
        requests.sort();
        assert_eq!(declared_sim(), requests);
    }

    #[test]
    fn the_prelude_compiles_and_shows_values() {
        let lua = Lua::new();
        let prelude: Table = lua.load(prelude()).eval().unwrap();
        let sim: Table = prelude.get("sim").unwrap();
        assert_eq!(
            sim.pairs::<String, mlua::Value>().count(),
            REQUESTS.len() + GUI_REQUESTS.len()
        );
        lua.globals()
            .set(
                "assert_equal",
                prelude.get::<mlua::Function>("assert_equal").unwrap(),
            )
            .unwrap();
        let err = lua
            .load("assert_equal({ 1, 'two', { x = 3 } }, { 1, 'two', { x = 4, [true] = 1 } }, 'parts')")
            .set_name("=check")
            .exec()
            .unwrap_err()
            .to_string();
        assert!(
            err.contains(
                r#"check:1: parts: expected { 1, "two", { [true] = 1, x = 4 } }, got { 1, "two", { x = 3 } }"#
            ),
            "{err}"
        );
        lua.load("assert_equal({ a = { 1, 2 } }, { a = { 1, 2 } })")
            .exec()
            .unwrap();
    }
}
