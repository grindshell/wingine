//! The `audio` table, for sound effects and music.

use std::path::Path;

use mlua::{Function, Lua, Table, UserDataRef};

use super::with_context;
use crate::audio::{Audio, PlaySettings, Sound};

/// The table, loading files relative to `folder`.
pub(super) fn module(lua: &Lua, folder: &Path) -> mlua::Result<Table> {
    let audio = lua.create_table()?;
    let root = folder.to_owned();
    audio.set(
        "load",
        lua.create_function(move |_, path: String| {
            Sound::load(root.join(path)).map_err(mlua::Error::external)
        })?,
    )?;
    audio.set(
        "play",
        lua.create_function(
            |lua, (sound, settings): (UserDataRef<Sound>, PlaySettings)| {
                with_context(lua, |ctx| ctx.audio().play_with(&sound, settings))
            },
        )?,
    )?;
    audio.set(
        "stop_sounds",
        lua.create_function(|lua, fade: Option<f32>| {
            with_context(lua, |ctx| ctx.audio().stop_sounds(fade.unwrap_or(0.0)))
        })?,
    )?;
    audio.set(
        "sounds_playing",
        lua.create_function(|lua, ()| with_context(lua, |ctx| ctx.audio().sounds_playing()))?,
    )?;
    let root = folder.to_owned();
    audio.set(
        "play_music",
        lua.create_function(move |lua, (path, fade): (String, Option<f32>)| {
            let path = root.join(path);
            with_context(lua, |ctx| ctx.audio().play_music(path, fade.unwrap_or(0.0)))?
                .map_err(mlua::Error::external)
        })?,
    )?;
    audio.set(
        "stop_music",
        lua.create_function(|lua, fade: Option<f32>| {
            with_context(lua, |ctx| ctx.audio().stop_music(fade.unwrap_or(0.0)))
        })?,
    )?;
    audio.set(
        "music_playing",
        lua.create_function(|lua, ()| with_context(lua, |ctx| ctx.audio().music_playing()))?,
    )?;
    audio.set("set_volume", volume_setter(lua, Audio::set_volume)?)?;
    audio.set(
        "set_sound_volume",
        volume_setter(lua, Audio::set_sound_volume)?,
    )?;
    audio.set(
        "set_music_volume",
        volume_setter(lua, Audio::set_music_volume)?,
    )?;
    audio.set_readonly(true);
    Ok(audio)
}

/// A function setting a volume with `set`.
fn volume_setter(lua: &Lua, set: fn(&mut Audio, f32)) -> mlua::Result<Function> {
    lua.create_function(move |lua, volume: f32| with_context(lua, |ctx| set(ctx.audio(), volume)))
}
