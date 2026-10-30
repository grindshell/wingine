//! Luau scripting: gameplay scripts in a sandboxed VM, with tasks that run over many frames.
//!
//! [`Scripts`] holds one Luau VM. A game registers Rust functions for scripts to call, loads its
//! script files, and starts script functions as tasks. A task is a coroutine that
//! [`Scripts::update`] resumes once a frame, so a script reads as a sequence of steps that can
//! wait for time to pass or for something to happen, without holding up the game:
//!
//! ```lua
//! function open_gate()
//!     say("The gate creaks open…")  -- a game function that waits for the player
//!     wait(0.5)
//!     remove_gate()
//! end
//! ```
//!
//! Every script can use these built-ins:
//!
//! - `wait(seconds)` pauses the task for `seconds` of game time. Without an argument, it waits
//!   for the next frame.
//! - `wait_until(condition)` pauses the task until `condition()` returns true, checking once a
//!   frame. Games use it to make their own functions wait, such as dialogue that waits for the
//!   player to close it.
//! - `spawn(f, ...)` starts `f(...)` as a separate task, running it right away until it first
//!   waits.
//! - `print(...)` logs its arguments at info level, with the target `lua`.
//!
//! Engine types convert to and from Lua values:
//!
//! - A [`Direction`] is one of the strings `"up"`, `"down"`, `"left"` or `"right"`.
//! - A [`Key`] or [`MouseButton`] is its name, such as `"Space"` or `"left"`, in any case.
//! - A [`Color`] is a hex string such as `"#f2c14e"`, with an optional alpha byte
//!   (`"#00000080"`), or a table `{ r, g, b, a }` of components from 0 to 1, where `a` defaults
//!   to 1.
//! - [`PlaySettings`] are a table with any of `volume`, `speed` and `pan`; the rest, or all of
//!   them for `nil`, keep their defaults.
//! - A [`Font`] is userdata with the methods `font:scaled(scale)`, `font:measure(text)` (its
//!   width and height) and `font:line_height()`, and a [`Sound`] is userdata with
//!   `sound:duration()`, in seconds.
//!
//! Scripts are [Luau](https://luau.org), not standard Lua 5.x, and run in Luau's sandbox: the
//! standard library and built-ins are read-only, and the globals a script defines are seen by the
//! scripts loaded after it. Luau compiles scripts to native code on platforms that support it.
//!
//! The engine re-exports [`mlua`], which games use to register functions and convert values.

use std::{fmt, fs, path::Path, str::FromStr};

pub use mlua;
use mlua::{ErrorContext, FromLua, FromLuaMulti, IntoLua, IntoLuaMulti, thread::ThreadStatus};

use crate::{
    Error, Result,
    audio::{PlaySettings, Sound},
    graphics::{Color, Font},
    grid::Direction,
    input::{Key, MouseButton},
};

/// Built-ins written in Luau, defined before the sandbox makes the globals read-only. Each frame,
/// `update` resumes waiting tasks with the frame's duration, which `coroutine.yield` returns.
const PRELUDE: &str = r#"
function wait(seconds: number?)
    local remaining = seconds or 0
    repeat
        remaining -= coroutine.yield()
    until remaining <= 0
end

function wait_until(condition: () -> boolean)
    while not condition() do
        coroutine.yield()
    end
end
"#;

/// A Luau VM for a game's scripts, and the tasks running in it.
pub struct Scripts {
    lua: mlua::Lua,
    tasks: Vec<Task>,
    next_task: u64,
}

/// Identifies a task started by [`Scripts::spawn`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TaskId(u64);

struct Task {
    id: TaskId,
    thread: mlua::Thread,
}

/// Tasks started by scripts with `spawn`, until `Scripts` adds them to its own.
#[derive(Default)]
struct Spawned(Vec<mlua::Thread>);

impl Scripts {
    /// A VM with the built-ins and nothing else loaded.
    pub fn new() -> Result<Self> {
        let lua = mlua::Lua::new();
        lua.set_app_data(Spawned::default());
        let globals = lua.globals();
        globals.set("print", lua.create_function(print)?)?;
        globals.set("spawn", lua.create_function(spawn)?)?;
        lua.load(PRELUDE).set_name("=prelude").exec()?;
        lua.sandbox(true)?;
        Ok(Self {
            lua,
            tasks: Vec::new(),
            next_task: 0,
        })
    }

    /// The underlying VM, for registering tables and userdata or converting values.
    pub fn lua(&self) -> &mlua::Lua {
        &self.lua
    }

    /// Makes `function` callable from scripts as the global `name`. Its arguments and results
    /// convert from and to Lua values, as with [`mlua::Lua::create_function`]. A function that
    /// shares state with the game usually holds an `Rc<RefCell<_>>`.
    pub fn set_function<A, R>(
        &self,
        name: &str,
        function: impl Fn(&mlua::Lua, A) -> mlua::Result<R> + 'static,
    ) -> Result<()>
    where
        A: FromLuaMulti,
        R: IntoLuaMulti,
    {
        let function = self.lua.create_function(function)?;
        self.lua.globals().set(name, function)?;
        Ok(())
    }

    /// Runs the script file at `path`, which usually defines functions for later.
    pub fn load(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let source = fs::read_to_string(path).map_err(|source| Error::ReadFile {
            path: path.to_owned(),
            source,
        })?;
        self.lua
            .load(source)
            .set_name(format!("@{}", path.display()))
            .exec()?;
        Ok(())
    }

    /// Runs Luau `source`, naming it `name` in error messages.
    pub fn load_source(&self, name: &str, source: &str) -> Result<()> {
        self.lua.load(source).set_name(format!("={name}")).exec()?;
        Ok(())
    }

    /// Calls the global function `name` and returns its results. It runs to completion, so it
    /// can't wait; start functions that wait with [`Scripts::spawn`].
    pub fn call<R: FromLuaMulti>(&self, name: &str, args: impl IntoLuaMulti) -> Result<R> {
        self.function(name)?.call(args).map_err(|err| {
            // Luau's own message for this doesn't say what to do instead.
            if err.to_string().contains("attempt to yield") {
                err.context(format!(
                    "`{name}` waits, so it has to run as a task: start it with `Scripts::spawn`"
                ))
            } else {
                err
            }
            .into()
        })
    }

    /// Starts the global function `name` as a task, running it right away until it first waits
    /// or finishes. [`Scripts::update`] resumes it from then on.
    pub fn spawn(&mut self, name: &str, args: impl IntoLuaMulti) -> Result<TaskId> {
        let thread = self.lua.create_thread(self.function(name)?)?;
        let result = thread.resume::<()>(args);
        // Whatever it spawned has run already, so it joins the other tasks even on an error.
        self.adopt_spawned();
        result?;
        Ok(self.add_task(thread))
    }

    /// Resumes every waiting task with `dt` seconds of game time passed.
    ///
    /// A task that raises an error stops, and the other tasks carry on. The first error is
    /// returned once every task has run.
    pub fn update(&mut self, dt: f32) -> Result<()> {
        // Tasks scripts started since the last update, such as while a script loaded, wait for
        // this frame like the rest.
        self.adopt_spawned();
        let mut first_error = None;
        self.tasks
            .retain(|task| match task.thread.resume::<()>(dt) {
                Ok(()) => task.thread.status() == ThreadStatus::Resumable,
                Err(err) => {
                    first_error.get_or_insert(err);
                    false
                }
            });
        self.adopt_spawned();
        first_error.map_or(Ok(()), |err| Err(err.into()))
    }

    /// Whether `task` is still waiting to run more: it hasn't finished, failed, or been
    /// cancelled.
    pub fn is_running(&self, task: TaskId) -> bool {
        self.tasks.iter().any(|t| t.id == task)
    }

    /// Stops `task` where it is, if it's running.
    pub fn cancel(&mut self, task: TaskId) {
        self.tasks.retain(|t| t.id != task);
    }

    /// How many tasks are running, including those scripts started with `spawn`.
    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    fn function(&self, name: &str) -> Result<mlua::Function> {
        match self.lua.globals().get(name)? {
            mlua::Value::Function(function) => Ok(function),
            value => Err(mlua::Error::runtime(format!(
                "`{name}` is {}, not a function",
                value.type_name()
            ))
            .into()),
        }
    }

    fn add_task(&mut self, thread: mlua::Thread) -> TaskId {
        let id = TaskId(self.next_task);
        self.next_task += 1;
        if thread.status() == ThreadStatus::Resumable {
            self.tasks.push(Task { id, thread });
        }
        id
    }

    fn adopt_spawned(&mut self) {
        let spawned = std::mem::take(
            &mut self
                .lua
                .app_data_mut::<Spawned>()
                .expect("set when the VM is created")
                .0,
        );
        for thread in spawned {
            self.add_task(thread);
        }
    }
}

/// The `print` built-in.
fn print(_: &mlua::Lua, values: mlua::Variadic<mlua::Value>) -> mlua::Result<()> {
    let text = values
        .iter()
        .map(mlua::Value::to_string)
        .collect::<mlua::Result<Vec<_>>>()?;
    log::info!(target: "lua", "{}", text.join("\t"));
    Ok(())
}

/// The `spawn` built-in.
fn spawn(
    lua: &mlua::Lua,
    (function, args): (mlua::Function, mlua::MultiValue),
) -> mlua::Result<()> {
    let thread = lua.create_thread(function)?;
    thread.resume::<()>(args)?;
    if thread.status() == ThreadStatus::Resumable {
        lua.app_data_mut::<Spawned>()
            .expect("set when the VM is created")
            .0
            .push(thread);
    }
    Ok(())
}

impl Direction {
    fn name(self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Left => "left",
            Self::Right => "right",
        }
    }
}

impl IntoLua for Direction {
    fn into_lua(self, lua: &mlua::Lua) -> mlua::Result<mlua::Value> {
        self.name().into_lua(lua)
    }
}

impl FromLua for Direction {
    fn from_lua(value: mlua::Value, _: &mlua::Lua) -> mlua::Result<Self> {
        let name = match &value {
            mlua::Value::String(name) => name.to_str()?.to_owned(),
            _ => String::new(),
        };
        Direction::ALL
            .into_iter()
            .find(|direction| direction.name() == name)
            .ok_or_else(|| mlua::Error::FromLuaConversionError {
                from: value.type_name(),
                to: "Direction".into(),
                message: Some(r#"expected "up", "down", "left" or "right""#.into()),
            })
    }
}

impl FromLua for Key {
    fn from_lua(value: mlua::Value, _: &mlua::Lua) -> mlua::Result<Self> {
        from_name(&value, "Key")
    }
}

impl FromLua for MouseButton {
    fn from_lua(value: mlua::Value, _: &mlua::Lua) -> mlua::Result<Self> {
        from_name(&value, "MouseButton")
    }
}

impl FromLua for Color {
    fn from_lua(value: mlua::Value, _: &mlua::Lua) -> mlua::Result<Self> {
        match &value {
            mlua::Value::String(hex) => {
                let hex = hex.to_str()?;
                parse_hex(&hex).ok_or_else(|| {
                    let message =
                        format!(r##"`{hex}` is not a color like "#f2c14e" or "#00000080""##);
                    conversion_error(&value, "Color", message)
                })
            }
            mlua::Value::Table(table) => Ok(Color::rgba(
                table.get("r")?,
                table.get("g")?,
                table.get("b")?,
                table.get::<Option<f32>>("a")?.unwrap_or(1.0),
            )),
            _ => {
                let message =
                    r##"expected a hex string such as "#f2c14e", or a table { r, g, b, a }"##;
                Err(conversion_error(&value, "Color", message.into()))
            }
        }
    }
}

impl FromLua for PlaySettings {
    fn from_lua(value: mlua::Value, _: &mlua::Lua) -> mlua::Result<Self> {
        let defaults = Self::default();
        match &value {
            mlua::Value::Nil => Ok(defaults),
            mlua::Value::Table(table) => Ok(Self {
                volume: table
                    .get::<Option<f32>>("volume")?
                    .unwrap_or(defaults.volume),
                speed: table.get::<Option<f32>>("speed")?.unwrap_or(defaults.speed),
                pan: table.get::<Option<f32>>("pan")?.unwrap_or(defaults.pan),
            }),
            _ => {
                let message = "expected a table { volume, speed, pan }".into();
                Err(conversion_error(&value, "PlaySettings", message))
            }
        }
    }
}

impl mlua::UserData for Font {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("scaled", |_, font, scale: f32| {
            if !(scale.is_finite() && scale > 0.0) {
                let message = format!("font scale must be a positive number, not {scale}");
                return Err(mlua::Error::runtime(message));
            }
            Ok(font.scaled(scale))
        });
        methods.add_method("measure", |_, font, text: mlua::LuaString| {
            let [width, height] = font.measure(&text.to_str()?);
            Ok((width, height))
        });
        methods.add_method("line_height", |_, font, ()| Ok(font.line_height()));
    }
}

impl mlua::UserData for Sound {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("duration", |_, sound, ()| Ok(sound.duration()));
    }
}

/// Converts a string to `T` by parsing it, for engine types scripts refer to by name.
fn from_name<T>(value: &mlua::Value, to: &str) -> mlua::Result<T>
where
    T: FromStr,
    T::Err: fmt::Display,
{
    let mlua::Value::String(name) = value else {
        return Err(conversion_error(value, to, "expected a name".into()));
    };
    name.to_str()?
        .parse()
        .map_err(|err: T::Err| conversion_error(value, to, err.to_string()))
}

fn conversion_error(value: &mlua::Value, to: &str, message: String) -> mlua::Error {
    mlua::Error::FromLuaConversionError {
        from: value.type_name(),
        to: to.into(),
        message: Some(message),
    }
}

/// Parses a color written `#rrggbb` or `#rrggbbaa`.
fn parse_hex(text: &str) -> Option<Color> {
    let digits = text.strip_prefix('#')?;
    if !matches!(digits.len(), 6 | 8) || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&digits[i * 2..i * 2 + 2], 16).ok();
    let alpha = if digits.len() == 8 { byte(3)? } else { 255 };
    Some(Color::rgba8(byte(0)?, byte(1)?, byte(2)?, alpha))
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use super::*;

    const FRAME: f32 = 1.0 / 60.0;

    fn scripts(source: &str) -> Scripts {
        let scripts = Scripts::new().unwrap();
        scripts.load_source("test", source).unwrap();
        scripts
    }

    /// Counts calls to the global `tick`, for watching tasks progress.
    fn counter(scripts: &Scripts) -> Rc<Cell<u32>> {
        let count = Rc::new(Cell::new(0));
        let ticks = Rc::clone(&count);
        scripts
            .set_function("tick", move |_, ()| {
                ticks.set(ticks.get() + 1);
                Ok(())
            })
            .unwrap();
        count
    }

    #[test]
    fn calls_functions_with_arguments_and_results() {
        let scripts = scripts("function add(a: number, b: number) return a + b end");
        assert_eq!(scripts.call::<i32>("add", (2, 3)).unwrap(), 5);
    }

    #[test]
    fn rust_functions_are_callable() {
        let scripts = Scripts::new().unwrap();
        scripts
            .set_function("double", |_, n: i32| Ok(n * 2))
            .unwrap();
        scripts
            .load_source("test", "function quadruple(n) return double(double(n)) end")
            .unwrap();
        assert_eq!(scripts.call::<i32>("quadruple", 3).unwrap(), 12);
    }

    #[test]
    fn a_task_runs_until_it_waits_then_once_a_frame() {
        let mut scripts = scripts("function run() tick() wait() tick() wait() tick() end");
        let count = counter(&scripts);
        let task = scripts.spawn("run", ()).unwrap();
        assert_eq!(count.get(), 1, "ran up to the first wait");
        assert!(scripts.is_running(task));
        scripts.update(FRAME).unwrap();
        assert_eq!(count.get(), 2);
        scripts.update(FRAME).unwrap();
        assert_eq!(count.get(), 3);
        assert!(!scripts.is_running(task), "finished");
        assert_eq!(scripts.task_count(), 0);
    }

    #[test]
    fn waiting_takes_game_time() {
        let mut scripts = scripts("function run() wait(0.5) tick() end");
        let count = counter(&scripts);
        scripts.spawn("run", ()).unwrap();
        for _ in 0..29 {
            scripts.update(FRAME).unwrap();
        }
        assert_eq!(count.get(), 0);
        scripts.update(FRAME).unwrap();
        assert_eq!(count.get(), 1, "after 30 frames");
    }

    #[test]
    fn wait_until_checks_once_a_frame() {
        let mut scripts = scripts("function run() wait_until(is_open) tick() end");
        let count = counter(&scripts);
        let open = Rc::new(Cell::new(false));
        let is_open = Rc::clone(&open);
        scripts
            .set_function("is_open", move |_, ()| Ok(is_open.get()))
            .unwrap();
        scripts.spawn("run", ()).unwrap();
        scripts.update(FRAME).unwrap();
        assert_eq!(count.get(), 0);
        open.set(true);
        scripts.update(FRAME).unwrap();
        assert_eq!(count.get(), 1);
    }

    #[test]
    fn scripts_spawn_tasks_that_run_alongside() {
        let mut scripts = scripts(
            "function child(times) for _ = 1, times do tick() wait() end end
             function parent() spawn(child, 3) wait() tick() end",
        );
        let count = counter(&scripts);
        let parent = scripts.spawn("parent", ()).unwrap();
        assert_eq!(count.get(), 1, "the child ran right away");
        assert_eq!(scripts.task_count(), 2);
        scripts.update(FRAME).unwrap();
        assert_eq!(count.get(), 3, "both ran");
        assert!(!scripts.is_running(parent));
        scripts.update(FRAME).unwrap();
        scripts.update(FRAME).unwrap();
        assert_eq!(count.get(), 4);
        assert_eq!(scripts.task_count(), 0);
    }

    #[test]
    fn tasks_spawned_while_loading_resume_on_the_next_update() {
        let mut scripts = Scripts::new().unwrap();
        let count = counter(&scripts);
        scripts
            .load_source("test", "spawn(function() tick() wait() tick() end)")
            .unwrap();
        assert_eq!(count.get(), 1);
        scripts.update(FRAME).unwrap();
        assert_eq!(count.get(), 2, "resumed on the first update");
        assert_eq!(scripts.task_count(), 0, "finished");
    }

    #[test]
    fn a_failing_task_stops_and_the_others_carry_on() {
        let mut scripts = scripts(
            "function fail() wait() error(\"broken\") end
             function steady() while true do tick() wait() end end",
        );
        let count = counter(&scripts);
        let failing = scripts.spawn("fail", ()).unwrap();
        let steady = scripts.spawn("steady", ()).unwrap();
        let err = scripts.update(FRAME).unwrap_err();
        assert!(err.to_string().contains("test:1: broken"), "{err}");
        assert!(!scripts.is_running(failing));
        assert!(scripts.is_running(steady));
        assert_eq!(count.get(), 2);
        scripts.update(FRAME).unwrap();
        assert_eq!(count.get(), 3);
    }

    #[test]
    fn cancelled_tasks_stop() {
        let mut scripts = scripts("function run() while true do tick() wait() end end");
        let count = counter(&scripts);
        let task = scripts.spawn("run", ()).unwrap();
        scripts.cancel(task);
        scripts.update(FRAME).unwrap();
        assert_eq!(count.get(), 1);
        assert!(!scripts.is_running(task));
    }

    #[test]
    fn calling_a_function_that_waits_fails() {
        let scripts = scripts("function run() wait(1) end");
        let err = scripts.call::<()>("run", ()).unwrap_err();
        assert!(
            err.to_string().contains("start it with `Scripts::spawn`"),
            "{err}"
        );
    }

    #[test]
    fn missing_functions_are_named() {
        let mut scripts = scripts("value = 3");
        let err = scripts.spawn("value", ()).unwrap_err();
        assert!(err.to_string().contains("`value` is integer"), "{err}");
        let err = scripts.call::<()>("nothing", ()).unwrap_err();
        assert!(err.to_string().contains("`nothing` is nil"), "{err}");
    }

    #[test]
    fn syntax_errors_name_the_file_and_line() {
        let directory = std::env::temp_dir().join("wingine-lua-tests");
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("broken.luau");
        fs::write(&path, "function ok() end\nfunction broken(\n").unwrap();
        let err = Scripts::new().unwrap().load(&path).unwrap_err();
        assert!(err.to_string().contains("broken.luau:"), "{err}");
    }

    #[test]
    fn missing_files_fail_to_load() {
        let err = Scripts::new().unwrap().load("no/such/script.luau");
        assert!(matches!(err, Err(Error::ReadFile { .. })));
    }

    #[test]
    fn the_standard_library_is_read_only() {
        let scripts = Scripts::new().unwrap();
        assert!(scripts.load_source("test", "string.upper = nil").is_err());
        assert!(
            scripts
                .load_source("test", "print('hello', 1, nil)")
                .is_ok()
        );
    }

    #[test]
    fn globals_carry_over_between_scripts() {
        let scripts = scripts("greeting = 'hi'");
        scripts
            .load_source("second", "function greet() return greeting end")
            .unwrap();
        assert_eq!(scripts.call::<String>("greet", ()).unwrap(), "hi");
    }

    #[test]
    fn directions_are_strings() {
        let scripts = scripts("function turn(d) return d end");
        let turned: Direction = scripts.call("turn", Direction::Left).unwrap();
        assert_eq!(turned, Direction::Left);
        assert_eq!(scripts.call::<String>("turn", Direction::Up).unwrap(), "up");
        assert!(scripts.call::<Direction>("turn", "north").is_err());
    }

    /// Converts the Luau `expression` to `T`.
    fn convert<T: FromLua>(expression: &str) -> mlua::Result<T> {
        let scripts = Scripts::new().unwrap();
        scripts.lua().load(format!("return {expression}")).eval()
    }

    #[test]
    fn keys_and_buttons_are_names() {
        assert_eq!(convert::<Key>("'space'").unwrap(), Key::Space);
        assert_eq!(convert::<MouseButton>("'Left'").unwrap(), MouseButton::Left);
        let err = convert::<Key>("'Shift'").unwrap_err();
        assert!(err.to_string().contains("unknown key `Shift`"), "{err}");
        assert!(convert::<Key>("32").is_err());
    }

    #[test]
    fn colors_are_hex_strings_or_tables() {
        assert_eq!(
            convert::<Color>("'#f2c14e'").unwrap(),
            Color::rgb8(0xf2, 0xc1, 0x4e)
        );
        assert_eq!(
            convert::<Color>("'#00000080'").unwrap(),
            Color::rgba8(0, 0, 0, 0x80)
        );
        assert_eq!(
            convert::<Color>("{ r = 1, g = 0.5, b = 0 }").unwrap(),
            Color::rgb(1.0, 0.5, 0.0)
        );
        assert_eq!(
            convert::<Color>("{ r = 0, g = 0, b = 0, a = 0.25 }").unwrap(),
            Color::rgba(0.0, 0.0, 0.0, 0.25)
        );
        for invalid in ["'f2c14e'", "'#f2c14'", "'#+2c14e'", "'red'", "3"] {
            assert!(convert::<Color>(invalid).is_err(), "{invalid}");
        }
    }

    #[test]
    fn play_settings_default_what_they_leave_out() {
        assert_eq!(
            convert::<PlaySettings>("nil").unwrap(),
            PlaySettings::default()
        );
        assert_eq!(
            convert::<PlaySettings>("{ pan = -0.5 }").unwrap(),
            PlaySettings {
                pan: -0.5,
                ..Default::default()
            }
        );
        assert!(convert::<PlaySettings>("1").is_err());
    }
}
