//! Games written entirely in Luau, run from their folder.
//!
//! A game is a folder with a `main.luau` script, run with the `wingine` executable
//! (`cargo run -- path/to/game`) or [`run`]. The script's top level runs once, when the window is
//! ready, to set the game up. Then each frame the engine calls the script's global function
//! `update(dt)`, with the seconds since the last frame, resumes the tasks started with `spawn`,
//! and calls `draw()`. Both functions are optional. The built-ins `wait`, `wait_until`, `spawn`
//! and `print` work as described in the [`lua`](crate::lua) module, which also lists how engine
//! types such as colors and keys are written in Luau.
//!
//! The window's settings come from an optional `window.luau` that returns a table. Every setting
//! is optional:
//!
//! ```lua
//! return {
//!     title = "Pong", -- by default, the folder's name
//!     width = 1280, -- the window's initial size in logical pixels
//!     height = 720,
//!     resizable = true,
//!     -- A fixed canvas size, scaled up by whole numbers. By default, the canvas is the window.
//!     resolution = { width = 320, height = 180 },
//! }
//! ```
//!
//! Scripts `require` other scripts by their path relative to the script, such as
//! `require("./opponent")`. Files the engine loads, such as sounds and images, are relative to
//! the game's folder.
//!
//! The engine's API is in five global tables:
//!
//! `graphics`, for drawing in `draw`. `graphics.size` and `graphics.default_font` work anywhere,
//! and `graphics.load_sprite_sheet` while the game loads and in `update`.
//!
//! - `graphics.clear(color)` fills the canvas with `color`, discarding anything drawn so far.
//! - `graphics.fill_rect(x, y, width, height, color)`
//! - `graphics.draw_sprite(sprite, x, y, tint, width, height)` draws `sprite` with its top-left
//!   corner at `x`, `y`, its colors multiplied by `tint` (white, unchanged, by default), and
//!   stretched to `width` by `height` (its own size by default).
//! - `graphics.draw_text(font, text, x, y, color)` draws `text` with its top-left corner at `x`,
//!   `y`, rounded to whole pixels.
//! - `graphics.draw_text_layout(layout, x, y, color, glyphs)` draws text laid out with
//!   `font:layout`, as `draw_text` does. With `glyphs`, it draws only that many of the layout's
//!   glyphs, rounded down, so drawing a growing number reveals the text a little at a time.
//! - `graphics.draw_tilemap(map, layer)` draws one layer of `map`, or all of them in order
//!   without a layer. Only the cells the canvas shows are drawn.
//! - `graphics.set_camera(x, y, bounds)` centers the view on world position `x`, `y` for whatever
//!   is drawn next, keeping it inside `bounds`, a rect such as `map:bounds()`, if given.
//!   `graphics.set_camera(nil)`, the default at the start of each frame, draws in canvas pixels,
//!   as for a HUD.
//! - `graphics.size()` returns the canvas's width and height. In a window, they're zero until
//!   the first frame.
//! - `graphics.default_font()` returns the built-in pixel font.
//! - `graphics.load_sprite_sheet(path, tile_width, tile_height)` loads a PNG divided into a grid
//!   of sprites `tile_width` by `tile_height` texels, starting at its top-left corner.
//!
//! `tilemap`, for building maps of tiles, anywhere:
//!
//! - `tilemap.new_tileset(tile_width, tile_height)` returns an empty tileset of tiles
//!   `tile_width` by `tile_height` world units, which are canvas pixels without a camera.
//! - `tilemap.new(tileset, columns, rows)` returns a map `columns` by `rows` tiles in size,
//!   with no layers yet, built from a copy of `tileset`.
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
//! An error while the game loads ends [`run`] with that error. An error in `update` or `draw`
//! stops the game and shows the error in the window. A task that fails stops and logs its error,
//! and the other tasks carry on.

mod audio;
mod graphics;
mod grid;
mod input;
mod tilemap;

use std::{fs, path::Path};

use crate::{
    Context, Error, Game, Result, WindowConfig,
    graphics::{Canvas, Color, Font, Resolution},
    lua::{Scripts, mlua},
};

/// Registry keys under which the engine lends its services to its Luau functions, while scripts
/// may use them.
const CONTEXT: &str = "wingine.context";
const CANVAS: &str = "wingine.canvas";

/// Runs the Luau game in `folder` until its window is closed. See the [module docs](self).
pub fn run(folder: impl AsRef<Path>) -> Result<()> {
    let folder = folder.as_ref();
    crate::run(config(folder)?, |ctx| LuauGame::load(ctx, folder))
}

/// The window settings of the game in `folder`, from its `window.luau` if it has one.
pub fn config(folder: impl AsRef<Path>) -> Result<WindowConfig> {
    let folder = folder.as_ref();
    let mut config = WindowConfig {
        title: folder_name(folder).unwrap_or_else(|| WindowConfig::default().title),
        ..Default::default()
    };
    let path = folder.join("window.luau");
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
    config: &mut WindowConfig,
    lua: &mlua::Lua,
    name: &str,
    value: mlua::Value,
) -> mlua::Result<()> {
    match name {
        "title" => config.title = lua.unpack(value)?,
        "width" => config.width = positive(lua.unpack(value)?)?,
        "height" => config.height = positive(lua.unpack(value)?)?,
        "resizable" => config.resizable = lua.unpack(value)?,
        "resolution" => {
            let size: mlua::Table = lua.unpack(value)?;
            config.resolution = Resolution::Fixed {
                width: positive(size.get("width")?)?,
                height: positive(size.get("height")?)?,
            };
        }
        _ => return Err(mlua::Error::runtime("unknown setting")),
    }
    Ok(())
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

/// A game written in Luau, loaded from its folder.
pub struct LuauGame {
    scripts: Scripts,
    /// For showing an error.
    font: Font,
    /// The error that stopped the game, if one has.
    error: Option<Error>,
}

impl LuauGame {
    /// Loads the game in `folder`, running its `main.luau`.
    pub fn load(ctx: &mut Context, folder: impl AsRef<Path>) -> Result<Self> {
        let folder = folder.as_ref();
        let scripts = Scripts::new()?;
        let lua = scripts.lua();
        let font = ctx.default_font().clone();
        let globals = lua.globals();
        globals.set("graphics", graphics::module(lua, folder, font.clone())?)?;
        globals.set("input", input::module(lua)?)?;
        globals.set("audio", audio::module(lua, folder)?)?;
        globals.set("tilemap", tilemap::module(lua)?)?;
        globals.set("grid", grid::module(lua)?)?;
        lend(lua, CONTEXT, ctx, || scripts.load(folder.join("main.luau")))?;
        Ok(Self {
            scripts,
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
        let lua = self.scripts.lua().clone();
        let result = lend(&lua, CONTEXT, ctx, || {
            self.callback("update", dt)?;
            if let Err(err) = self.scripts.update(dt) {
                log::error!("{err}");
            }
            Ok(())
        });
        if let Err(err) = result {
            self.stop(err);
        }
    }

    fn draw(&mut self, canvas: &mut Canvas) {
        if self.error.is_none() {
            let lua = self.scripts.lua().clone();
            if let Err(err) = lend(&lua, CANVAS, canvas, || self.callback("draw", ())) {
                self.stop(err);
            }
        }
        if let Some(err) = &self.error {
            draw_error(canvas, &self.font, err);
        }
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

/// Lends `value` to the engine's Luau functions under `key` while `f` runs. They reach it with
/// [`with_lent`], and fail if they're called when it isn't lent.
fn lend<T: 'static, R>(
    lua: &mlua::Lua,
    key: &str,
    value: &mut T,
    f: impl FnOnce() -> Result<R>,
) -> Result<R> {
    lua.scope(|scope| {
        lua.set_named_registry_value(key, scope.create_any_userdata_ref_mut(value)?)?;
        let result = f();
        lua.unset_named_registry_value(key)?;
        Ok(result)
    })?
}

/// Runs `f` with what the engine lent under `key`, if it lent anything.
fn with_lent<T: 'static, R>(
    lua: &mlua::Lua,
    key: &str,
    f: impl FnOnce(&mut T) -> R,
) -> mlua::Result<Option<R>> {
    match lua.named_registry_value::<Option<mlua::AnyUserData>>(key)? {
        Some(lent) => lent.borrow_mut_scoped(f).map(Some),
        None => Ok(None),
    }
}

/// Runs `f` with the context, which the engine lends while the game loads and updates.
fn with_context<R>(lua: &mlua::Lua, f: impl FnOnce(&mut Context) -> R) -> mlua::Result<R> {
    with_lent(lua, CONTEXT, f)?.ok_or_else(|| {
        mlua::Error::runtime("this only works while the game loads or in `update`, not in `draw`")
    })
}

/// Runs `f` with the canvas, which the engine lends while the game draws.
fn with_canvas<R>(lua: &mlua::Lua, f: impl FnOnce(&mut Canvas) -> R) -> mlua::Result<R> {
    with_lent(lua, CANVAS, f)?.ok_or_else(|| mlua::Error::runtime("drawing only works in `draw`"))
}
