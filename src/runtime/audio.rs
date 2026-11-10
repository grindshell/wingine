//! The `audio` table, for sound effects and music, and the voices `audio.play` gives.

use std::path::Path;

use mlua::{Function, Lua, Table, UserDataMethods, UserDataRef};

use super::Lent;
use crate::audio::{Audio, PlaySettings, Sound, Voice, setting_problem};

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
                let voice = lent.context(|ctx| ctx.audio().play_with(&sound, settings))?;
                Ok(ScriptVoice {
                    voice,
                    lent: lent.clone(),
                })
            },
        )?
    })?;
    audio.set("stop_sounds", fader(lua, lent, Audio::stop_sounds)?)?;
    audio.set("pause_sounds", fader(lua, lent, Audio::pause_sounds)?)?;
    audio.set("resume_sounds", fader(lua, lent, Audio::resume_sounds)?)?;
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
    audio.set("stop_music", fader(lua, lent, Audio::stop_music)?)?;
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

/// A function setting a volume with `set`, which fails for a volume that isn't a number.
fn volume_setter(lua: &Lua, lent: &Lent, set: fn(&mut Audio, f32)) -> mlua::Result<Function> {
    let lent = lent.clone();
    lua.create_function(move |_, volume: f32| {
        if let Some(problem) = setting_problem("volume", volume) {
            return Err(mlua::Error::runtime(problem));
        }
        lent.context(|ctx| set(ctx.audio(), volume))
    })
}

/// A function doing `act` over a fade of seconds it takes, 0 if it's left out.
fn fader(lua: &Lua, lent: &Lent, act: fn(&mut Audio, f32)) -> mlua::Result<Function> {
    let lent = lent.clone();
    lua.create_function(move |_, fade: Option<f32>| {
        lent.context(|ctx| act(ctx.audio(), fade.unwrap_or(0.0)))
    })
}

/// A sound effect playing, as scripts hold it: the [`Voice`] `audio.play` gave, with the context
/// it reaches the [`Audio`] through. Its methods are in the [module docs](super).
struct ScriptVoice {
    voice: Voice,
    lent: Lent,
}

impl mlua::UserData for ScriptVoice {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        add_fade(methods, "stop", Audio::stop_voice);
        add_fade(methods, "pause", Audio::pause_voice);
        add_fade(methods, "resume", Audio::resume_voice);
        add_setting(methods, "volume", Audio::set_voice_volume);
        add_setting(methods, "speed", Audio::set_voice_speed);
        add_setting(methods, "pan", Audio::set_voice_pan);
        methods.add_method("is_playing", |_, this, ()| {
            this.lent
                .context(|ctx| ctx.audio().voice_playing(this.voice))
        });
        methods.add_method("is_paused", |_, this, ()| {
            this.lent
                .context(|ctx| ctx.audio().voice_paused(this.voice))
        });
    }
}

/// Adds the method `name`, doing `act` to the voice over a fade of seconds it takes.
fn add_fade<M: UserDataMethods<ScriptVoice>>(
    methods: &mut M,
    name: &'static str,
    act: fn(&mut Audio, Voice, f32),
) {
    methods.add_method(name, move |_, this, fade: Option<f32>| {
        this.lent
            .context(|ctx| act(ctx.audio(), this.voice, fade.unwrap_or(0.0)))
    });
}

/// Adds the method `set_<setting>`, changing the voice's `setting` with `set` over a fade.
fn add_setting<M: UserDataMethods<ScriptVoice>>(
    methods: &mut M,
    setting: &'static str,
    set: fn(&mut Audio, Voice, f32, f32),
) {
    methods.add_method(
        format!("set_{setting}"),
        move |_, this, (value, fade): (f32, Option<f32>)| {
            if let Some(problem) = setting_problem(setting, value) {
                return Err(mlua::Error::runtime(problem));
            }
            this.lent
                .context(|ctx| set(ctx.audio(), this.voice, value, fade.unwrap_or(0.0)))
        },
    );
}
