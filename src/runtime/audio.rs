//! The `audio` table, for sound effects and music.

use std::path::Path;

use mlua::{Function, Lua, Table, UserDataRef};

use super::Lent;
use crate::audio::{Audio, PlaySettings, Sound};

/// The table, loading files relative to `folder`.
pub(super) fn module(lua: &Lua, lent: &Lent, folder: &Path) -> mlua::Result<Table> {
    let audio = lua.create_table()?;
    let root = folder.to_owned();
    audio.set(
        "load",
        lua.create_function(move |_, path: String| {
            Sound::load(root.join(path)).map_err(mlua::Error::external)
        })?,
    )?;
    audio.set("play", {
        let lent = lent.clone();
        lua.create_function(
            move |_, (sound, settings): (UserDataRef<Sound>, PlaySettings)| {
                lent.context(|ctx| ctx.audio().play_with(&sound, settings))
            },
        )?
    })?;
    audio.set("stop_sounds", {
        let lent = lent.clone();
        lua.create_function(move |_, fade: Option<f32>| {
            lent.context(|ctx| ctx.audio().stop_sounds(fade.unwrap_or(0.0)))
        })?
    })?;
    audio.set("sounds_playing", {
        let lent = lent.clone();
        lua.create_function(move |_, ()| lent.context(|ctx| ctx.audio().sounds_playing()))?
    })?;
    audio.set("play_music", {
        let lent = lent.clone();
        let root = folder.to_owned();
        lua.create_function(move |_, (path, fade): (String, Option<f32>)| {
            let path = root.join(path);
            lent.context(|ctx| ctx.audio().play_music(path, fade.unwrap_or(0.0)))?
                .map_err(mlua::Error::external)
        })?
    })?;
    audio.set("stop_music", {
        let lent = lent.clone();
        lua.create_function(move |_, fade: Option<f32>| {
            lent.context(|ctx| ctx.audio().stop_music(fade.unwrap_or(0.0)))
        })?
    })?;
    audio.set("music_playing", {
        let lent = lent.clone();
        lua.create_function(move |_, ()| lent.context(|ctx| ctx.audio().music_playing()))?
    })?;
    audio.set("set_volume", volume_setter(lua, lent, Audio::set_volume)?)?;
    audio.set(
        "set_sound_volume",
        volume_setter(lua, lent, Audio::set_sound_volume)?,
    )?;
    audio.set(
        "set_music_volume",
        volume_setter(lua, lent, Audio::set_music_volume)?,
    )?;
    audio.set_readonly(true);
    Ok(audio)
}

/// A function setting a volume with `set`.
fn volume_setter(lua: &Lua, lent: &Lent, set: fn(&mut Audio, f32)) -> mlua::Result<Function> {
    let lent = lent.clone();
    lua.create_function(move |_, volume: f32| lent.context(|ctx| set(ctx.audio(), volume)))
}
