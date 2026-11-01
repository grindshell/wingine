//! Games written entirely in Luau, run from their folder.
//!
//! A game is a folder with a `main.luau` script, run with the `wingine` executable
//! (`cargo run -- path/to/game`) or [`run`]. The script's top level runs once, when the window is
//! ready, to set the game up. Then each frame the engine calls the script's global function
//! `update(dt)`, with the seconds since the last frame, resumes the tasks started with `spawn`
//! (those `update` starts, from the next update), and calls `draw()`. With an `update_rate`
//! setting, the game updates that many times a second instead, each time by the same `dt`, and
//! each frame runs as many updates as its time holds before it draws. Both functions are
//! optional. The built-ins `wait`, `wait_until`, `spawn` and `print` work as described in the
//! [`lua`](crate::lua) module, which also lists how engine types such as colors and keys are
//! written in Luau.
//!
//! The game's settings come from an optional `conf.luau` that returns a table, which the engine
//! reads before the game starts. Every setting is optional:
//!
//! ```lua
//! return {
//!     title = "Pong", -- by default, the folder's name
//!     width = 1280, -- the window's initial size in logical pixels
//!     height = 720,
//!     resizable = true,
//!     -- A fixed canvas size, scaled up by whole numbers. By default, the canvas is the window.
//!     resolution = { width = 320, height = 180 },
//!     -- Whether frames wait for the display's refresh (true by default). Turning it off shows
//!     -- how long frames really take.
//!     vsync = false,
//!     -- Updates a second, each by the same `dt`, so the game plays the same at any frame rate.
//!     -- By default, the game updates once a frame by the frame's time.
//!     update_rate = 60,
//!     -- A key that shows and hides a window of how long frames take. The game never sees it.
//!     debug_key = "F3",
//!     -- Names the folder the game's saves are kept in: letters, digits, `_` and `-`. By
//!     -- default, the folder's name, with `_` for anything else.
//!     identity = "pong",
//!     -- Native libraries to load, in order, from the game's `native` folder. One marked
//!     -- optional loads only if it's there: `if danmaku then ... end` tells whether it did.
//!     libraries = { "particles", { name = "danmaku", optional = true } },
//! }
//! ```
//!
//! Native libraries are compiled code written in Rust with the `wingine-native` crate. Each is
//! loaded from the game's `native` folder, named as the platform names libraries:
//! `particles.dll` on Windows, `libparticles.so` on Linux and `libparticles.dylib` on macOS. Once
//! loaded, a library stays loaded until the program ends. It adds a global table of its own,
//! named by the library, before `main.luau` runs. Unlike scripts, a library isn't sandboxed: a
//! game that lists one runs it with the player's full permissions.
//!
//! Scripts `require` other scripts by their path relative to the script, such as
//! `require("./opponent")`. Files the engine loads, such as sounds and images, are relative to
//! the game's folder.
//!
//! The engine's API is in seven global tables:
//!
//! `graphics`, for drawing in `draw`. `graphics.size`, `graphics.color`, `graphics.default_font`,
//! `graphics.frame_stats` and `graphics.new_animation` work anywhere, and
//! `graphics.load_sprite_sheet` while the game loads and in `update`.
//!
//! - `graphics.clear(color)` fills the canvas with `color`, discarding anything drawn so far.
//! - `graphics.fill_rect(x, y, width, height, color)`
//! - `graphics.draw_sprite(sprite, x, y, tint, width, height)` draws `sprite` with its top-left
//!   corner at `x`, `y`, its colors multiplied by `tint` (white, unchanged, by default), and
//!   stretched to `width` by `height` (its own size by default).
//! - `graphics.draw_sprite_rotated(sprite, x, y, angle, tint, width, height)` draws `sprite` as
//!   `draw_sprite` does, then rotated by `angle` radians about its center. Positive angles turn
//!   clockwise, since `y` grows downward.
//! - `graphics.draw_text(font, text, x, y, color)` draws `text` with its top-left corner at `x`,
//!   `y`, rounded to whole pixels.
//! - `graphics.draw_text_layout(layout, x, y, color, glyphs)` draws text laid out with
//!   `font:layout`, as `draw_text` does. With `glyphs`, it draws only that many of the layout's
//!   glyphs, rounded down, so drawing a growing number reveals the text a little at a time.
//! - `graphics.draw_tilemap(map, layer)` draws one layer of `map`, or all of them in order
//!   without a layer. Only the cells the canvas shows are drawn.
//! - `graphics.draw_scene(node)` draws a node of a scene and every node inside it, each before
//!   its children, where they are in the scene, first laying the scene out on the canvas, so its
//!   anchors follow the canvas's edges. Hidden nodes don't draw, nor what's inside them, and
//!   nothing is drawn where it's off the canvas.
//! - `graphics.set_camera(x, y, bounds)` centers the view on world position `x`, `y` for whatever
//!   is drawn next, keeping it inside `bounds`, a rect such as `map:bounds()`, if given.
//!   `graphics.set_camera(nil)`, the default at the start of each frame, draws in canvas pixels,
//!   as for a HUD.
//! - `graphics.size()` returns the canvas's width and height.
//! - `graphics.default_font()` returns the built-in pixel font.
//! - `graphics.color(color)` returns `color`, such as a hex string or a table, as a number
//!   `0xRRGGBBAA`, which passes faster: a string or table is read again each time it's passed,
//!   which costs more than drawing a sprite, so code that draws a lot makes its colors numbers
//!   once.
//! - `graphics.frame_stats()` returns how long the frame before took, and how much it drew, as a
//!   table: `frame`, `update`, `draw` and `render` are seconds, and `quads` and `draw_calls`
//!   counts. Before the first frame has finished, all are 0. See `FrameStats` for what each
//!   measures.
//! - `graphics.load_sprite_sheet(path, tile_width, tile_height)` loads a PNG divided into a grid
//!   of sprites `tile_width` by `tile_height` texels, starting at its top-left corner. Each image
//!   is read once; loading it again, even divided differently, reuses it.
//! - `graphics.new_animation(frames, frame_time)` returns an animation that shows each sprite in
//!   the list `frames` for `frame_time` seconds, then starts over. Draw it with
//!   `graphics.draw_sprite(animation:frame(time), x, y)`, where `time` is how long it has been
//!   playing.
//!
//! `tilemap`, for building maps of tiles, anywhere:
//!
//! - `tilemap.new_tileset(tile_width, tile_height)` returns an empty tileset of tiles
//!   `tile_width` by `tile_height` world units, which are canvas pixels without a camera.
//! - `tilemap.new(tileset, columns, rows)` returns a map `columns` by `rows` tiles in size,
//!   with no layers yet, built from a copy of `tileset`. Maps hold at most 16,777,216 cells
//!   across all their layers.
//!
//! `grid`, for walking from tile to tile, anywhere:
//!
//! - `grid.new_mover(column, row, step_time)` returns a mover standing on a tile and facing
//!   down, which takes `step_time` seconds to walk to the next. Each frame, call
//!   `mover:update(dt)`, then `mover:step(direction, blocked)` to start the next step once it
//!   isn't moving. A step that starts as another ends picks up the time left over, so walking
//!   keeps an even pace.
//! - `grid.neighbor(direction, column, row)` returns the column and row of the next tile in
//!   `direction`.
//! - `grid.opposite(direction)`
//!
//! `scene`, for loading [scenes](crate::scene):
//!
//! - `scene.load(path)`, while the game loads and in `update`, loads the scene file at `path`,
//!   the scene files its instances place, the tileset files its tilemaps use and the images they
//!   all name, and returns the scene's root node, laid out on the canvas. Each file is read once,
//!   and each load returns a copy of its own.
//! - `scene.clips(path)`, at any time, returns the clips of the scene file at `path`, a table of
//!   `Clip`s by name, for posing nodes with `node:pose(clip, time, weight)`. A clip poses any node
//!   whose nodes its tracks' paths lead to, such as each instance of the file.
//!
//! `input`, while the game loads and in `update`:
//!
//! - `input.bind(action, keys)` binds an action, such as `"jump"`, to a list of keys, such as
//!   `{ "Space", "W" }`, replacing any it had.
//! - `input.held(action)`, `input.pressed(action)` and `input.released(action)` tell whether any
//!   of the action's keys is held, or was pressed or released this frame.
//! - `input.key_held(key)`, `input.key_pressed(key)` and `input.key_released(key)` do the same for
//!   a key.
//! - `input.mouse_position()` returns the cursor's position in canvas pixels, or `nil` when the
//!   cursor is outside the window.
//! - `input.mouse_held(button)`, `input.mouse_pressed(button)` and
//!   `input.mouse_released(button)` do the same for a mouse button.
//!
//! `audio`, while the game loads and in `update`. `audio.load` works anywhere.
//!
//! - `audio.load(path)` loads a sound effect from a WAV or OGG Vorbis file.
//! - `audio.play(sound, settings)` plays a sound, with optional settings.
//! - `audio.stop_sounds(fade)` fades out every sound effect over `fade` seconds (0 by default).
//! - `audio.sounds_playing()` returns how many sound effects are playing.
//! - `audio.play_music(path, fade)` streams music from a file and loops it, crossfading from any
//!   music already playing over `fade` seconds.
//! - `audio.stop_music(fade)` and `audio.music_playing()`
//! - `audio.set_volume(volume)`, `audio.set_sound_volume(volume)` and
//!   `audio.set_music_volume(volume)` set the volume of everything, of sound effects and of
//!   music, where 1 is as recorded.
//!
//! `save`, for data kept between runs of the game, at any time. Each save has a name of letters,
//! digits, `_` and `-`, and holds plain data: a boolean, number, string, or table of them, where a
//! table is a list or has strings for keys, as scenes' props are. See [`save`](crate::save) for
//! where saves are kept.
//!
//! - `save.write(name, data)` writes `data` to the save `name`, replacing any. Anything else in
//!   it, such as a function, is an error naming where it is.
//! - `save.read(name)` returns the data in the save `name`, or `nil` if there's no such save.
//! - `save.remove(name)` removes the save `name`, if there is one.
//! - `save.list()` returns the names of the game's saves, in order.
//!
//! A game may also define a global `gui(ui)`, which the engine calls once a frame, after the
//! updates and before `draw`, with a `Ui` covering the window, to lay out an interface over the
//! game with egui: debug tools, say. The context is lent as in `update`. Widgets that change a
//! value take it and return the new one, and containers call a function with the `Ui` inside them:
//!
//! - Containers: `ui:window(title or { title, x, y, width, height, resizable, collapsible }, fn)`,
//!   `ui:panel(side or { side, size, resizable }, fn)`, `ui:horizontal(fn)`, `ui:vertical(fn)`,
//!   `ui:group(fn)`, `ui:collapsing(title, fn)`, `ui:scroll(fn)`, `ui:grid(name, fn)` with
//!   `ui:end_row()`, and `ui:enabled(enabled, fn)`.
//! - Text: `ui:label(text, color)`, `ui:heading(text)`, `ui:monospace(text)` and `ui:small(text)`.
//! - Widgets: `ui:button(text)` returns whether it was clicked; `ui:checkbox(text, checked)`,
//!   `ui:radio(current, value, text)`, `ui:combo(text, current, choices)`,
//!   `ui:slider(text, value, min, max, step)`, `ui:drag(text, value, speed, min, max)` and
//!   `ui:text_input(text, hint)` (which also returns whether Enter was pressed) return their
//!   values; `ui:progress(fraction, text)` and `ui:sprite(sprite, tint, width, height)` show them.
//! - `ui:separator()` and `ui:space(amount)`.
//!
//! A `Ui` works only until the function it was passed to returns, and inside a container's
//! function, only the `Ui` it's passed works. Without the `egui` feature, `gui` isn't called.
//!
//! An error while the game loads ends [`run`] with that error. An error in `update`, `gui` or
//! `draw` stops the game and shows the error in the window. A task that fails stops and logs its
//! error, and the other tasks carry on.

mod audio;
mod graphics;
mod grid;
#[cfg(feature = "egui")]
mod gui;
mod input;
mod native;
mod save;
mod scene;
mod tilemap;

use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    ptr,
    rc::Rc,
};

use wingine_abi::Phase;

use native::Natives;
pub use native::link_library;

use crate::{
    Context, Error, Game, Result, WindowConfig,
    graphics::{Canvas, Color, Font, Resolution, Texture},
    lua::{Scripts, mlua},
};

/// The types of the engine's API for Luau games, as a definitions file for
/// [luau-lsp](https://github.com/JohnnyMorganz/luau-lsp): the global tables and built-ins, the
/// userdata they hand out and the values they take, with their docs, so a language server checks
/// scripts against the engine and shows its docs on hover. It's `wingine.d.luau`, next to this
/// file.
pub const DEFINITIONS: &str = include_str!("wingine.d.luau");

/// Runs the Luau game in `folder` until its window is closed. See the [module docs](self).
pub fn run(folder: impl AsRef<Path>) -> Result<()> {
    let folder = folder.as_ref();
    crate::run(config(folder)?.window, |ctx| LuauGame::load(ctx, folder))
}

/// A Luau game's settings, from its `conf.luau`.
#[derive(Clone, Debug)]
pub struct Config {
    pub window: WindowConfig,
    /// The native libraries to load, in order.
    pub libraries: Vec<LibraryConfig>,
}

/// A native library a game loads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryConfig {
    pub name: String,
    /// Whether the game runs without it if there's no such library. One that's there but fails to
    /// load is an error either way.
    pub optional: bool,
}

/// The settings of the game in `folder`, from its `conf.luau` if it has one.
pub fn config(folder: impl AsRef<Path>) -> Result<Config> {
    let folder = folder.as_ref();
    let name = folder_name(folder);
    let mut config = Config {
        window: WindowConfig {
            title: name
                .clone()
                .unwrap_or_else(|| WindowConfig::default().title),
            identity: name
                .as_deref()
                .map(identity)
                .unwrap_or_else(|| WindowConfig::default().identity),
            ..Default::default()
        },
        libraries: Vec::new(),
    };
    let path = folder.join("conf.luau");
    if !path.is_file() {
        return Ok(config);
    }
    let source = fs::read_to_string(&path).map_err(|source| Error::ReadFile {
        path: path.clone(),
        source,
    })?;
    let lua = mlua::Lua::new();
    lua.sandbox(true)?;
    let settings: mlua::Table = lua
        .load(source)
        .set_name(format!("@{}", path.display()))
        .eval()?;
    for pair in settings.pairs::<String, mlua::Value>() {
        let (name, value) = pair?;
        apply_setting(&mut config, &lua, &name, value).map_err(|err| Error::Script {
            path: path.clone(),
            message: format!("`{name}`: {err}"),
        })?;
    }
    Ok(config)
}

fn apply_setting(
    config: &mut Config,
    lua: &mlua::Lua,
    name: &str,
    value: mlua::Value,
) -> mlua::Result<()> {
    let window = &mut config.window;
    match name {
        "title" => window.title = lua.unpack(value)?,
        "width" => window.width = positive(lua.unpack(value)?)?,
        "height" => window.height = positive(lua.unpack(value)?)?,
        "resizable" => window.resizable = lua.unpack(value)?,
        "vsync" => window.vsync = lua.unpack(value)?,
        "debug_key" => window.debug_key = Some(lua.unpack(value)?),
        "identity" => {
            let identity: String = lua.unpack(value)?;
            if !crate::save::is_name(&identity) {
                let message = "use only letters, digits, `_` and `-`, as it names a folder";
                return Err(mlua::Error::runtime(message));
            }
            window.identity = identity;
        }
        "update_rate" => {
            let rate: f32 = lua.unpack(value)?;
            if !(rate > 0.0 && rate.is_finite()) {
                return Err(mlua::Error::runtime("must be above 0"));
            }
            window.update_rate = Some(rate);
        }
        "resolution" => {
            let size: mlua::Table = lua.unpack(value)?;
            window.resolution = Resolution::Fixed {
                width: positive(size.get("width")?)?,
                height: positive(size.get("height")?)?,
            };
        }
        "libraries" => {
            let entries: Vec<mlua::Value> = lua.unpack(value)?;
            config.libraries = entries
                .into_iter()
                .map(|entry| library(lua, entry))
                .collect::<mlua::Result<_>>()?;
        }
        _ => return Err(mlua::Error::runtime("unknown setting")),
    }
    Ok(())
}

/// An entry in the `libraries` setting: a library's name, or a table with its `name` and whether
/// it's `optional`.
fn library(lua: &mlua::Lua, entry: mlua::Value) -> mlua::Result<LibraryConfig> {
    let library = match entry {
        mlua::Value::Table(table) => {
            for pair in table.pairs::<mlua::Value, mlua::Value>() {
                let key = pair?.0.to_string()?;
                if key != "name" && key != "optional" {
                    let message = format!("a library has a `name` and `optional`, not `{key}`");
                    return Err(mlua::Error::runtime(message));
                }
            }
            LibraryConfig {
                name: table.get("name")?,
                optional: table.get::<Option<bool>>("optional")?.unwrap_or(false),
            }
        }
        name => LibraryConfig {
            name: lua.unpack(name)?,
            optional: false,
        },
    };
    if !crate::save::is_name(&library.name) {
        return Err(mlua::Error::runtime(format!(
            "`{}` is not a library name: use only letters, digits, `_` and `-`",
            library.name
        )));
    }
    Ok(library)
}

fn positive(size: u32) -> mlua::Result<u32> {
    if size == 0 {
        return Err(mlua::Error::runtime("must be at least 1"));
    }
    Ok(size)
}

fn folder_name(folder: &Path) -> Option<String> {
    let folder = folder.canonicalize().ok()?;
    Some(folder.file_name()?.to_string_lossy().into_owned())
}

/// The identity of a game in a folder called `name`, by default: the name, with `_` for whatever
/// can't be in an identity.
fn identity(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '-' => c,
            _ => '_',
        })
        .collect()
}

/// A game written in Luau, loaded from its folder.
pub struct LuauGame {
    scripts: Scripts,
    natives: Natives,
    lent: Lent,
    /// For showing an error.
    font: Font,
    /// The error that stopped the game, if one has.
    error: Option<Error>,
}

impl LuauGame {
    /// Loads the game in `folder`: the native libraries its `conf.luau` lists, then its
    /// `main.luau`.
    pub fn load(ctx: &mut Context, folder: impl AsRef<Path>) -> Result<Self> {
        let folder = folder.as_ref();
        let libraries = config(folder)?.libraries;
        let scripts = Scripts::new()?;
        let lua = scripts.lua();
        let lent = Lent::default();
        let font = ctx.default_font().clone();
        let textures = Textures::default();
        let globals = lua.globals();
        let graphics = graphics::module(lua, &lent, folder, textures.clone(), font.clone())?;
        globals.set("graphics", graphics)?;
        globals.set("input", input::module(lua, &lent)?)?;
        globals.set("audio", audio::module(lua, &lent, folder)?)?;
        globals.set("tilemap", tilemap::module(lua)?)?;
        globals.set("grid", grid::module(lua)?)?;
        let scene = scene::module(lua, &lent, folder.to_owned(), textures)?;
        globals.set("scene", scene)?;
        globals.set("save", save::module(lua, ctx.saves())?)?;
        #[cfg(feature = "egui")]
        gui::register(lua, &lent)?;
        let natives = Natives::load(lua, &lent, ctx, folder, &libraries)?;
        lent.lend(ctx, || scripts.load(folder.join("main.luau")))?;
        Ok(Self {
            scripts,
            natives,
            lent,
            font,
            error: None,
        })
    }

    /// The game's scripts, for looking at the game's state.
    pub fn scripts(&self) -> &Scripts {
        &self.scripts
    }

    /// The error that stopped the game, if one has.
    pub fn error(&self) -> Option<&Error> {
        self.error.as_ref()
    }

    /// Calls the script's global function `name`, if it has one.
    fn callback(&self, name: &str, args: impl mlua::IntoLuaMulti) -> Result<()> {
        if self
            .scripts
            .lua()
            .globals()
            .get::<mlua::Value>(name)?
            .is_nil()
        {
            return Ok(());
        }
        self.scripts.call(name, args)
    }

    fn stop(&mut self, err: Error) {
        log::error!("{err}");
        self.error = Some(err);
    }
}

impl Game for LuauGame {
    fn update(&mut self, ctx: &mut Context, dt: f32) {
        if self.error.is_some() {
            return;
        }
        if let Err(err) = self.run_update(ctx, dt) {
            self.stop(err);
        }
    }

    #[cfg(feature = "egui")]
    fn gui(&mut self, ctx: &mut Context, ui: &mut crate::gui::egui::Ui) {
        let function = match self.error {
            None => self.scripts.lua().globals().get("gui"),
            Some(_) => Ok(None),
        };
        let result = function.map_err(Error::from).and_then(|function| {
            let Some(function) = function else {
                // Frames needn't run egui at all, as for a game with no interface.
                ctx.gui.game_has_gui = false;
                return Ok(());
            };
            let lent = self.lent.clone();
            lent.lend(ctx, || {
                gui::call::<()>(self.scripts.lua(), ui, &function).map_err(Error::from)
            })
        });
        if let Err(err) = result {
            self.stop(err);
        }
    }

    fn draw(&mut self, canvas: &mut Canvas) {
        if self.error.is_none()
            && let Err(err) = self.run_draw(canvas)
        {
            self.stop(err);
        }
        if let Some(err) = &self.error {
            draw_error(canvas, &self.font, err);
        }
    }
}

impl LuauGame {
    /// Runs the libraries' hooks before `update`, the script's `update` and its tasks, then the
    /// hooks after.
    fn run_update(&mut self, ctx: &mut Context, dt: f32) -> Result<()> {
        let lua = self.scripts.lua().clone();
        // For `graphics.frame_stats`, which works in `draw` too, where the context isn't lent.
        lua.set_app_data(ctx.frame_stats());
        self.natives.update(Phase::BEFORE, ctx, dt)?;
        // Tasks started before this frame, such as while the game loaded, resume in it. Those
        // `update` starts have run until they first waited, and resume from the next frame.
        self.scripts.adopt_spawned();
        let lent = self.lent.clone();
        lent.lend(ctx, || {
            self.callback("update", dt)?;
            if let Err(err) = self.scripts.resume_tasks(dt) {
                log::error!("{err}");
            }
            Ok(())
        })?;
        self.natives.update(Phase::AFTER, ctx, dt)
    }

    /// Runs the libraries' hooks before `draw`, the script's `draw`, then the hooks after, which
    /// draw in canvas pixels whatever camera the script left set.
    fn run_draw(&mut self, canvas: &mut Canvas) -> Result<()> {
        self.natives.draw(Phase::BEFORE, canvas)?;
        let lent = self.lent.clone();
        lent.lend(canvas, || self.callback("draw", ()))?;
        canvas.set_camera(None);
        self.natives.draw(Phase::AFTER, canvas)
    }
}

/// Shows `err` in place of the game.
fn draw_error(canvas: &mut Canvas, font: &Font, err: &Error) {
    const MARGIN: f32 = 8.0;
    canvas.clear(Color::rgb8(0x30, 0x10, 0x18));
    canvas.set_camera(None);
    // Tracebacks indent with tabs, which the font doesn't have.
    let text = format!("The game stopped with an error:\n\n{err}").replace('\t', "  ");
    let width = (canvas.width() - MARGIN * 2.0).max(MARGIN);
    let layout = font.layout(&text, Some(width));
    canvas.draw_text_layout(&layout, .., [MARGIN, MARGIN], Color::WHITE);
}

/// The images a game has loaded, by path, so each is read once however many sprite sheets and
/// scenes use it.
#[derive(Clone, Default)]
struct Textures(Rc<RefCell<HashMap<PathBuf, Texture>>>);

impl Textures {
    /// The image at `path`, loading it if it hasn't been.
    fn load(&self, ctx: &mut Context, path: PathBuf) -> Result<Texture> {
        if let Some(texture) = self.0.borrow().get(&path) {
            return Ok(texture.clone());
        }
        let texture = ctx.load_texture(&path)?;
        self.0.borrow_mut().insert(path, texture.clone());
        Ok(texture)
    }
}

/// What the engine lends its Luau functions while scripts may use it: the context while the game
/// loads and updates, and the canvas while it draws. Every engine function in a VM holds a handle
/// to the same one.
///
/// Engine functions run on every draw call, so reaching what's lent has to be cheap: it's a
/// pointer that [`Lent::lend`] sets for as long as it runs, which each function holds, rather than
/// a lookup in the VM's registry or app data.
#[derive(Clone, Default)]
struct Lent(Rc<Loans>);

impl Lent {
    /// Lends `value` to the engine's Luau functions while `f` runs. Called at any other time, they
    /// fail with a Lua error rather than reach it.
    fn lend<T: Lendable, R>(&self, value: &mut T, f: impl FnOnce() -> Result<R>) -> Result<R> {
        let loan = T::loan(&self.0);
        // Whatever an outer call lent comes back however `f` ends, so nothing reaches `value`
        // after.
        let _value = Restore(&loan.value, loan.value.replace(value));
        let _borrowed = Restore(&loan.borrowed, loan.borrowed.replace(false));
        f()
    }

    /// Runs `f` with what's lent of `T`, if anything is.
    fn with<T: Lendable, R>(&self, f: impl FnOnce(&mut T) -> R) -> mlua::Result<Option<R>> {
        T::loan(&self.0).with(f)
    }

    /// Runs `f` with the context, which the engine lends while the game loads and updates.
    fn context<R>(&self, f: impl FnOnce(&mut Context) -> R) -> mlua::Result<R> {
        self.with(f)?.ok_or_else(|| {
            mlua::Error::runtime(
                "this only works while the game loads or in `update`, not in `draw`",
            )
        })
    }

    /// Runs `f` with the canvas, which the engine lends while the game draws.
    fn canvas<R>(&self, f: impl FnOnce(&mut Canvas) -> R) -> mlua::Result<R> {
        self.with(f)?
            .ok_or_else(|| mlua::Error::runtime("drawing only works in `draw`"))
    }
}

#[derive(Default)]
struct Loans {
    context: Loan<Context>,
    canvas: Loan<Canvas>,
}

/// A value the engine lends its Luau functions, which one of them at a time may borrow.
struct Loan<T> {
    /// What's lent, or null.
    value: Cell<*mut T>,
    /// Whether a function has it borrowed.
    borrowed: Cell<bool>,
    /// What scripts call it, for errors.
    name: &'static str,
}

impl<T> Loan<T> {
    fn new(name: &'static str) -> Self {
        Self {
            value: Cell::new(ptr::null_mut()),
            borrowed: Cell::new(false),
            name,
        }
    }

    /// Runs `f` with what's lent, if anything is.
    fn with<R>(&self, f: impl FnOnce(&mut T) -> R) -> mlua::Result<Option<R>> {
        let value = self.value.get();
        if value.is_null() {
            return Ok(None);
        }
        if self.borrowed.replace(true) {
            let message = format!("the {} is already in use", self.name);
            return Err(mlua::Error::runtime(message));
        }
        let _borrowed = Restore(&self.borrowed, false);
        // SAFETY: `Lent::lend` keeps the value alive, and reaches it only through this, until it
        // takes it back, and `borrowed` makes this the only reference to it until `f` returns.
        Ok(Some(f(unsafe { &mut *value })))
    }
}

impl Default for Loan<Context> {
    fn default() -> Self {
        Self::new("context")
    }
}

impl Default for Loan<Canvas> {
    fn default() -> Self {
        Self::new("canvas")
    }
}

/// Sets a cell back to a value when dropped, however the scope it guards ends.
struct Restore<'a, T: Copy>(&'a Cell<T>, T);

impl<T: Copy> Drop for Restore<'_, T> {
    fn drop(&mut self) {
        self.0.set(self.1);
    }
}

/// The engine services its Luau functions can be lent.
trait Lendable: Sized + 'static {
    fn loan(loans: &Loans) -> &Loan<Self>;
}

impl Lendable for Context {
    fn loan(loans: &Loans) -> &Loan<Self> {
        &loans.context
    }
}

impl Lendable for Canvas {
    fn loan(loans: &Loans) -> &Loan<Self> {
        &loans.canvas
    }
}
