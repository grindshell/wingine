//! Editor plugins: Luau a game carries for the Wingine editor, in a folder of its own under
//! `.editor/`, which export and the web build leave out, as they leave out every name starting
//! with `.`. The editor runs each in a [`PluginVm`] of its own, whose functions lay out screens of
//! the editor with the `Ui` a game's `gui(ui)` gets.
//!
//! A plugin's VM has the API a game's has, so the game's own modules that require it still load,
//! less what needs a running game: loading and drawing graphics (but painting on a canvas),
//! audio, input, saves, picking folders and the window fail when they're called, naming
//! themselves, and so do the `Ui`'s calls that would restyle the whole editor (`set_theme`,
//! `set_fonts`, and `scale` given a factor). Native libraries aren't loaded. `require` reads
//! modules only from the game's folder, once links are followed. A call into the VM, loading the
//! plugin included, is stopped if it runs too long, and the VM fails one that takes too much
//! memory.

use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicU32, Ordering},
};

use mlua::{FromLuaMulti, Function, IntoLuaMulti, Lua, MultiValue, Table, Value, VmState};

use super::{
    Lent, NoGame, Requirer, Textures, audio, files, fs_require::FsRequirer, graphics, grid, gui,
    input, physics, save, scene, tilemap, ui, window,
};
use crate::{
    Context, Error, Result,
    folders::Folders,
    gui::egui,
    lua::{ApiModule, Scripts, load_script},
    save::Saves,
};

/// How many times a call into a plugin may be interrupted, as Luau does at its loops and calls,
/// before it's stopped: a few tenths of a second of looping.
const MAX_INTERRUPTS: u32 = 10_000_000;

/// How much memory a plugin's VM may take, in bytes.
const MAX_MEMORY: usize = 256 << 20;

/// The API's tables whose functions all need a running game: its saves, and the folders its
/// player picked.
const ALL_NEED_A_GAME: [&str; 2] = ["save", "files"];

/// The API's tables some of whose functions need a running game, and fail, naming themselves,
/// when they find there's none.
const SOME_NEED_A_GAME: [&str; 6] = ["graphics", "input", "audio", "scene", "ui", "window"];

/// A Luau VM running an editor plugin, kept to its game's folder. See the [module docs](self).
pub struct PluginVm {
    scripts: Scripts,
    /// How many times the call running has been interrupted.
    interrupts: Arc<AtomicU32>,
}

impl PluginVm {
    /// Runs the plugin whose file is at `path` in the game's `folder`, such as
    /// `.editor/story/plugin.luau`, in a VM of its own, and returns the VM with what the file
    /// returned. Errors name files by their paths in the folder.
    pub fn load(ctx: &Context, folder: &Path, path: &str) -> Result<(Self, Value)> {
        let vm = Self {
            scripts: Scripts::new()?,
            interrupts: Arc::default(),
        };
        let lua = vm.lua();
        let lent = Lent::for_plugin();
        let api = vm.scripts.api();
        let textures = Textures::default();
        let font = ctx.default_font().clone();
        let graphics = graphics::module(lua, &lent, folder, textures.clone(), font)?;
        api.set("graphics", graphics)?;
        api.set("input", input::module(lua, &lent)?)?;
        let spawn = api.get("spawn")?;
        api.set("audio", audio::module(lua, &lent, folder, spawn)?)?;
        api.set("tilemap", tilemap::module(lua)?)?;
        api.set("grid", grid::module(lua)?)?;
        api.set("physics", physics::module(lua)?)?;
        let scene = scene::module(lua, &lent, folder.to_owned(), textures)?;
        api.set("scene", scene)?;
        api.set("ui", ui::module(lua, &lent)?)?;
        api.set("save", save::module(lua, &Saves::default())?)?;
        api.set("files", files::module(lua, &Folders::default())?)?;
        api.set("window", window::module(lua, &lent, None)?)?;
        for name in SOME_NEED_A_GAME {
            without_a_game(lua, api, name, false)?;
        }
        for name in ALL_NEED_A_GAME {
            without_a_game(lua, api, name, true)?;
        }
        gui::register(lua, &lent, folder)?;

        let chunk = format!("@{path}");
        let files =
            FsRequirer::rooted(Default::default(), folder).map_err(|source| Error::ReadFile {
                path: folder.to_owned(),
                source,
            })?;
        let requirer = Requirer {
            files,
            api: ApiModule::new(api.clone()),
            main: chunk.clone(),
        };
        lua.globals()
            .set("require", lua.create_require_function(requirer)?)?;
        lua.set_memory_limit(MAX_MEMORY)?;
        let interrupts = Arc::clone(&vm.interrupts);
        lua.set_interrupt(move |_| {
            if interrupts.fetch_add(1, Ordering::Relaxed) < MAX_INTERRUPTS {
                Ok(VmState::Continue)
            } else {
                Err(mlua::Error::runtime(
                    "the plugin ran too long and was stopped, as one stuck in a loop would be",
                ))
            }
        });

        let file = folder.join(path);
        let source = crate::files::read(&file).map_err(|source| Error::ReadFile {
            path: file.clone(),
            source,
        })?;
        vm.scripts.freeze_api();
        let main = load_script(lua, &source, chunk, None, None)?;
        let returned = vm.call(&main, ())?;
        Ok((vm, returned))
    }

    /// The VM, for making the functions and tables the editor gives its plugin.
    pub fn lua(&self) -> &Lua {
        self.scripts.lua()
    }

    /// Calls `function` with `args`, stopping it if it runs too long.
    pub fn call<R: FromLuaMulti>(&self, function: &Function, args: impl IntoLuaMulti) -> Result<R> {
        self.interrupts.store(0, Ordering::Relaxed);
        function.call(args).map_err(limited)
    }

    /// Calls `function` with `ui`, as a `Ui` it can use until it returns, then `args`, stopping it
    /// if it runs too long.
    pub fn call_gui<R: FromLuaMulti>(
        &self,
        ui: &mut egui::Ui,
        function: &Function,
        args: impl IntoLuaMulti,
    ) -> Result<R> {
        self.interrupts.store(0, Ordering::Relaxed);
        let lua = self.lua();
        lua.scope(|scope| {
            let mut args = args.into_lua_multi(lua)?;
            args.push_front(Value::UserData(scope.create_any_userdata_ref_mut(ui)?));
            function.call(args)
        })
        .map_err(limited)
    }
}

/// `err`, saying what limit the plugin went past if it's for going past one.
fn limited(err: mlua::Error) -> Error {
    match err {
        mlua::Error::MemoryError(_) => mlua::Error::runtime(format!(
            "the plugin took more than {} MiB of memory",
            MAX_MEMORY >> 20
        ))
        .into(),
        err => err.into(),
    }
}

/// Puts in place of the API's table `name` a copy whose functions fail, naming themselves, if
/// they find there's no game running, or, if `all`, whenever they're called.
fn without_a_game(lua: &Lua, api: &Table, name: &str, all: bool) -> mlua::Result<()> {
    let table: Table = api.get(name)?;
    let copy = lua.create_table()?;
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        let value = match (&key, value) {
            (Value::String(field), Value::Function(function)) => {
                let called = format!("{name}.{}", field.to_str()?);
                let wrapped = lua.create_function(move |_, args: MultiValue| {
                    if all {
                        return Err(needs_a_game(&called));
                    }
                    function.call::<MultiValue>(args).map_err(|err| {
                        if is_no_game(&err) {
                            needs_a_game(&called)
                        } else {
                            err
                        }
                    })
                })?;
                Value::Function(wrapped)
            }
            (_, value) => value,
        };
        copy.set(key, value)?;
    }
    api.set(name, copy)
}

/// The error for an editor plugin calling `called`, which needs a running game.
fn needs_a_game(called: &str) -> mlua::Error {
    mlua::Error::runtime(format!(
        "`{called}` needs a running game, and editor plugins have none"
    ))
}

/// Whether `err` is from something that found there was no game running.
fn is_no_game(err: &mlua::Error) -> bool {
    match err {
        mlua::Error::ExternalError(err) => err.is::<NoGame>(),
        mlua::Error::CallbackError { cause, .. } | mlua::Error::WithContext { cause, .. } => {
            is_no_game(cause)
        }
        _ => false,
    }
}
