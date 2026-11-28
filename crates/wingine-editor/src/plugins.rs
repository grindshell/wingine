//! Editor plugins: Luau a game carries in `.editor/<name>/plugin.luau`, which adds screens of the
//! game's own to the editor, for authoring its content, as Godot runs a project's own editor
//! plugins. Plugins are found without running anything, and run only once the person trusts the
//! game's folder, each in a VM of its own ([`PluginVm`]), kept to the game's folder. The
//! repository's examples are trusted without asking.
//!
//! `plugin.luau` returns what the plugin adds:
//!
//! ```lua
//! return {
//!     name = "Items",
//!     api = 1,
//!     screens = {
//!         items = { title = "Items", gui = function(ui, editor) ... end },
//!     },
//! }
//! ```
//!
//! A screen's `gui` lays it out with the `Ui` a game's `gui(ui)` gets, less the calls that would
//! restyle the editor, and the `editor` API: `editor.print(...)` writes to Output, and
//! `editor.redraw()` asks for another frame, as the editor draws only when something changes.

use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

use wingine::{
    Context,
    gui::egui,
    lua::mlua::{self, Function, Table, Value as LuaValue},
    runtime::PluginVm,
    scene::Value,
};

use crate::docks::output::Level;

/// The version of the editor's API for plugins. A plugin written for a newer one isn't loaded.
pub const API: u32 = 1;

/// The folder in a game's folder that holds its plugins, a folder each.
pub const FOLDER: &str = ".editor";

/// The editor's save holding the folders trusted to run their plugins.
const TRUSTED: &str = "trusted_folders";

/// What a plugin's table may hold.
const FIELDS: [&str; 3] = ["name", "api", "screens"];

/// Whether the game's plugins may run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trust {
    /// Not known yet: the editor's saves are read the next frame.
    Unknown,
    /// Asking the person, as nothing has said.
    Asking,
    /// Trusted, so they run.
    Trusted,
    /// The person said not now: they don't run until the game is opened again.
    Declined,
}

/// The plugins of the game open.
pub struct Plugins {
    /// The game's folder.
    folder: PathBuf,
    /// The plugins' folders, found by their `plugin.luau`, as paths in the game's folder, such as
    /// `.editor/story`, in order.
    found: Vec<String>,
    trust: Trust,
    /// The plugins loaded, in the order they were found, once the folder is trusted.
    loaded: Vec<Plugin>,
    /// Whether to load them again the next frame.
    stale: bool,
    /// What plugins have written to Output since the editor last took it.
    messages: Rc<RefCell<Vec<(Level, String)>>>,
    /// Whether a plugin has asked for another frame.
    redraw: Rc<Cell<bool>>,
}

/// A plugin, loaded or failing to.
pub struct Plugin {
    /// Its folder in the game's, such as `.editor/story`.
    pub folder: String,
    /// What it's called: its `name`, or its folder's while it hasn't loaded.
    pub name: String,
    /// The plugin running, or why it isn't, which its screens show in their place.
    state: Result<Running, String>,
    /// Its screens, by id, which stay listed while it fails, so they show why.
    screens: Vec<Screen>,
}

struct Running {
    vm: PluginVm,
    /// The `editor` its functions are given.
    editor: Table,
}

/// A main screen a plugin adds.
pub struct Screen {
    pub id: String,
    pub title: String,
    gui: Function,
}

impl Plugins {
    /// The plugins of the game in `folder`, found but not run.
    pub fn find(folder: &Path) -> Self {
        Self {
            folder: folder.to_owned(),
            found: find(folder),
            trust: Trust::Unknown,
            loaded: Vec::new(),
            stale: true,
            messages: Rc::default(),
            redraw: Rc::default(),
        }
    }

    /// The plugins' folders, as paths in the game's folder, such as `.editor/story`.
    pub fn found(&self) -> &[String] {
        &self.found
    }

    pub fn trust(&self) -> Trust {
        self.trust
    }

    /// The plugins loaded, or failing to.
    pub fn loaded(&self) -> &[Plugin] {
        &self.loaded
    }

    /// Finds the plugins again, and loads them again the next frame if they may run.
    pub fn reload(&mut self) {
        self.found = find(&self.folder);
        self.stale = true;
    }

    /// Trusts the game's folder, as the person asked, remembering it in `ctx`'s saves, and runs
    /// its plugins the next frame.
    pub fn trust_folder(&mut self, ctx: &Context) {
        self.trust = Trust::Trusted;
        self.stale = true;
        let Some(folder) = canonical(&self.folder) else {
            return;
        };
        let mut trusted = trusted(ctx);
        if !trusted.contains(&folder) {
            trusted.push(folder);
            let list = trusted.into_iter().map(Value::String).collect();
            if let Err(err) = ctx.saves().write(TRUSTED, &Value::List(list)) {
                self.messages.borrow_mut().push((
                    Level::Warning,
                    format!("Couldn't remember that the folder is trusted: {err}"),
                ));
            }
        }
    }

    /// Doesn't run the plugins until the game is opened again.
    pub fn decline(&mut self) {
        self.trust = Trust::Declined;
    }

    /// Settles whether the plugins may run, and loads them if they may and they're stale, as each
    /// frame begins.
    pub fn prepare(&mut self, ctx: &Context) {
        if self.found.is_empty() {
            self.loaded.clear();
            return;
        }
        if self.trust == Trust::Unknown {
            self.trust = if is_trusted(ctx, &self.folder) {
                Trust::Trusted
            } else {
                Trust::Asking
            };
        }
        if self.trust == Trust::Trusted && std::mem::take(&mut self.stale) {
            self.load(ctx);
        }
    }

    /// Loads every plugin found, each in a VM of its own, in place of those loaded before.
    fn load(&mut self, ctx: &Context) {
        self.loaded = self
            .found
            .iter()
            .map(|folder| {
                let plugin = Plugin::load(ctx, &self.folder, folder, &self.messages, &self.redraw);
                if let Err(err) = &plugin.state {
                    let name = &plugin.name;
                    self.messages
                        .borrow_mut()
                        .push((Level::Error, format!("The plugin {name} failed: {err}")));
                }
                plugin
            })
            .collect();
    }

    /// The screens of the plugins loaded, in order, with the plugin each is from.
    pub fn screens(&self) -> impl Iterator<Item = (&Plugin, &Screen)> {
        self.loaded
            .iter()
            .flat_map(|plugin| plugin.screens.iter().map(move |screen| (plugin, screen)))
    }

    /// Lays out the screen at `index` among [`Plugins::screens`], or the error stopping its
    /// plugin. A plugin whose screen fails stops, until it's loaded again.
    pub fn screen_ui(&mut self, ui: &mut egui::Ui, index: usize) {
        let found = self
            .loaded
            .iter()
            .enumerate()
            .flat_map(|(p, plugin)| (0..plugin.screens.len()).map(move |s| (p, s)))
            .nth(index);
        let Some((p, screen)) = found else {
            ui.weak("The plugin that added this screen isn't loaded.");
            return;
        };
        let plugin = &mut self.loaded[p];
        let gui = plugin.screens[screen].gui.clone();
        let result = match &plugin.state {
            Ok(running) => running
                .vm
                .call_gui::<()>(ui, &gui, (running.editor.clone(), LuaValue::Nil))
                .map_err(|err| err.to_string()),
            Err(err) => {
                show_error(ui, &plugin.name, err);
                return;
            }
        };
        if let Err(err) = result {
            let name = &plugin.name;
            self.messages
                .borrow_mut()
                .push((Level::Error, format!("The plugin {name} failed: {err}")));
            plugin.state = Err(err);
            self.redraw.set(true);
        }
    }

    /// What plugins have written to Output since this was last asked.
    pub fn take_messages(&self) -> Vec<(Level, String)> {
        std::mem::take(&mut self.messages.borrow_mut())
    }

    /// Whether a plugin has asked for another frame since this was last asked.
    pub fn take_redraw(&self) -> bool {
        self.redraw.take()
    }
}

impl Plugin {
    /// Loads the plugin in `folder`, a folder of the game's in `game`, sending what it prints
    /// to `messages`, and noting in `redraw` when it asks for a frame.
    fn load(
        ctx: &Context,
        game: &Path,
        folder: &str,
        messages: &Rc<RefCell<Vec<(Level, String)>>>,
        redraw: &Rc<Cell<bool>>,
    ) -> Self {
        let mut plugin = Self {
            folder: folder.to_owned(),
            name: folder.rsplit('/').next().unwrap_or(folder).to_owned(),
            state: Err(String::new()),
            screens: Vec::new(),
        };
        let path = format!("{folder}/plugin.luau");
        let loaded = PluginVm::load(ctx, game, &path)
            .map_err(|err| err.to_string())
            .and_then(|(vm, returned)| {
                let table = match returned {
                    LuaValue::Table(table) => table,
                    other => {
                        return Err(format!(
                            "{path} returns a {}, not a table of what the plugin adds",
                            other.type_name()
                        ));
                    }
                };
                let (name, screens) =
                    read_plugin(&table).map_err(|err| format!("{path}: {err}"))?;
                plugin.name = name.clone();
                plugin.screens = screens;
                let editor =
                    editor_api(vm.lua(), &name, messages, redraw).map_err(|err| err.to_string())?;
                Ok(Running { vm, editor })
            });
        plugin.state = loaded;
        plugin
    }

    /// Why the plugin isn't running, if it isn't.
    pub fn error(&self) -> Option<&str> {
        self.state.as_ref().err().map(String::as_str)
    }
}

/// The name and screens of the plugin whose `plugin.luau` returned `table`.
fn read_plugin(table: &Table) -> Result<(String, Vec<Screen>), String> {
    let err = |err: mlua::Error| err.to_string();
    for pair in table.pairs::<LuaValue, LuaValue>() {
        let (key, _) = pair.map_err(err)?;
        let known = key
            .as_string()
            .and_then(|key| key.to_str().ok())
            .is_some_and(|key| FIELDS.contains(&&*key));
        if !known {
            let key = key.to_string().map_err(err)?;
            let fields = FIELDS.join("`, `");
            return Err(format!("a plugin has `{fields}`, not `{key}`"));
        }
    }
    let name: String = table
        .get::<Option<String>>("name")
        .map_err(|err| format!("`name`: {err}"))?
        .ok_or("a plugin has a `name`, which the editor calls it")?;
    let api: u32 = table
        .get::<Option<u32>>("api")
        .map_err(|err| format!("`api`: {err}"))?
        .ok_or("a plugin says which version of the editor's API it's for, in `api`")?;
    if api > API || api == 0 {
        return Err(format!(
            "it's for version {api} of the editor's API, and this editor has version {API}"
        ));
    }
    let mut screens = BTreeMap::new();
    let listed: Option<Table> = table
        .get("screens")
        .map_err(|err| format!("`screens`: {err}"))?;
    if let Some(listed) = listed {
        for pair in listed.pairs::<String, Table>() {
            let (id, screen) = pair.map_err(|err| format!("`screens`: {err}"))?;
            let at = |err: mlua::Error| format!("`screens.{id}`: {err}");
            let title: Option<String> = screen.get("title").map_err(at)?;
            let gui: Option<Function> = screen.get("gui").map_err(at)?;
            let gui = gui.ok_or_else(|| format!("`screens.{id}` has no `gui` to lay it out"))?;
            let title = title.unwrap_or_else(|| id.clone());
            screens.insert(id.clone(), Screen { id, title, gui });
        }
    }
    Ok((name, screens.into_values().collect()))
}

/// The `editor` the functions of the plugin `name` in `lua` are given: what they can ask of the
/// editor.
fn editor_api(
    lua: &mlua::Lua,
    name: &str,
    messages: &Rc<RefCell<Vec<(Level, String)>>>,
    redraw: &Rc<Cell<bool>>,
) -> mlua::Result<Table> {
    let editor = lua.create_table()?;
    editor.set("print", {
        let messages = Rc::clone(messages);
        let name = name.to_owned();
        lua.create_function(move |_, args: mlua::MultiValue| {
            let words: Vec<String> = args
                .iter()
                .map(|value| value.to_string())
                .collect::<mlua::Result<_>>()?;
            let text = format!("[{name}] {}", words.join(" "));
            messages.borrow_mut().push((Level::Info, text));
            Ok(())
        })?
    })?;
    editor.set("redraw", {
        let redraw = Rc::clone(redraw);
        lua.create_function(move |_, ()| {
            redraw.set(true);
            Ok(())
        })?
    })?;
    editor.set_readonly(true);
    Ok(editor)
}

/// Shows why the plugin `name` isn't running, in place of its screen.
fn show_error(ui: &mut egui::Ui, name: &str, err: &str) {
    ui.colored_label(
        ui.visuals().error_fg_color,
        format!("The plugin {name} stopped:"),
    );
    ui.monospace(err);
    ui.weak("It runs again once it's loaded again: Project ▸ Read Files Again.");
}

/// The plugins' folders in the game's `folder`: each folder in its `.editor` with a
/// `plugin.luau`, as a path in the game's folder, in order.
fn find(folder: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(folder.join(FOLDER)) else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("plugin.luau").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .map(|name| format!("{FOLDER}/{name}"))
        .collect();
    found.sort();
    found
}

/// `folder` made canonical, as trust remembers it.
fn canonical(folder: &Path) -> Option<String> {
    Some(fs::canonicalize(folder).ok()?.display().to_string())
}

/// The folders trusted, as `ctx`'s saves remember them.
fn trusted(ctx: &Context) -> Vec<String> {
    match ctx.saves().read(TRUSTED) {
        Ok(Some(Value::List(folders))) => folders
            .into_iter()
            .filter_map(|folder| match folder {
                Value::String(folder) => Some(folder),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// Whether the plugins of the game in `folder` may run without asking: it's trusted, or one of
/// the repository's examples.
fn is_trusted(ctx: &Context, folder: &Path) -> bool {
    let Some(folder) = canonical(folder) else {
        return false;
    };
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let is_example = canonical(&examples).is_some_and(|examples| {
        Path::new(&folder).starts_with(&examples) && Path::new(&folder) != Path::new(&examples)
    });
    is_example || trusted(ctx).contains(&folder)
}
