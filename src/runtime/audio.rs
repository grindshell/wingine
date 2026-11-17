//! The `audio` table, for sound effects, music and songs, the voices `audio.play` gives, and the
//! performances `audio.play_song` gives.

use std::path::Path;

use mlua::{Function, Lua, Table, UserDataFields, UserDataMethods, UserDataRef};

use super::Lent;
use crate::{
    audio::{Audio, PlaySettings, Sound, Voice, setting_problem},
    music::{At, Performance, PlayingSong, Song, SongEvent, SongSettings},
};

/// The table, loading files relative to `folder`. Songs' listeners run as tasks `spawn` starts.
pub(super) fn module(
    lua: &Lua,
    lent: &Lent,
    folder: &Path,
    spawn: Function,
) -> mlua::Result<Table> {
    let audio = lua.create_table()?;
    let root = folder.to_owned();
    audio.set(
        "load",
        lua.create_function(move |_, path: String| {
            Sound::load(root.join(path)).map_err(mlua::Error::external)
        })?,
    )?;
    audio.set(
        "make",
        lua.create_function(
            |_, (samples, rate, channels): (mlua::Buffer, f64, Option<f64>)| {
                make(&samples.to_vec(), rate, channels.unwrap_or(1.0)).map_err(mlua::Error::runtime)
            },
        )?,
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
    add_songs(lua, &audio, lent, folder, spawn)?;
    audio.set_readonly(true);
    Ok(audio)
}

/// A sound of the samples in a buffer's `bytes`, each a 32-bit float as `buffer.writef32` writes
/// one, at `rate` a second for each of its `channels`; or what's wrong with them.
fn make(bytes: &[u8], rate: f64, channels: f64) -> Result<Sound, String> {
    if !bytes.len().is_multiple_of(4) {
        return Err(format!(
            "a sound's samples are 4 bytes each, which a buffer of {} isn't",
            bytes.len()
        ));
    }
    // A rate or a number of channels that isn't whole, or is past what a sound has, is none it
    // has: 0 is refused for both.
    let whole = |number: f64| {
        if number.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&number) {
            number as u32
        } else {
            0
        }
    };
    if whole(rate) == 0 {
        return Err(format!(
            "a sound's rate must be a whole number from 1 to {}, not {rate}",
            Sound::MAX_RATE
        ));
    }
    if whole(channels) == 0 {
        return Err(format!("a sound has 1 channel or 2, not {channels}"));
    }
    let samples: Vec<f32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|&bytes| f32::from_le_bytes(bytes))
        .collect();
    Sound::from_samples(&samples, whole(rate), whole(channels)).map_err(|err| err.to_string())
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

/// `on`, `off` and `wait_for`, the methods of songs playing that are Luau so `wait_for` can wait,
/// given `spawn` as `...`. Each listener is a task that looks through the events of each update,
/// starting each call of its function as a task of its own, so a listener can wait without
/// missing what happens meanwhile.
const SONG_LISTENERS: &str = r#"
local spawn = ...

local KINDS = { beat = true, bar = true, section = true, marker = true, note = true, ["end"] = true }

-- The kind of event `event` names, and the name in it, if any: "marker:drop" is a marker named
-- "drop".
local function parse(performance, event, level)
    if type(event) ~= "string" then
        error(`a song's events are named by strings, not {typeof(event)}`, level)
    end
    local kind, name = string.match(event, "^([^:]+):(.+)$")
    if kind == nil then
        kind = event
    end
    if not KINDS[kind] then
        error(`a song's events are "beat", "bar", "section", "marker", "note:<track>" and "end", not "{event}"`, level)
    end
    local song = performance:song()
    if kind == "note" then
        if name == nil then
            error(`"note" events name the track to hear, as "note:drums"`, level)
        end
        if not table.find(song:tracks(), name) then
            error(`the song has no track "{name}"`, level)
        end
        performance:listen(name, true)
    elseif kind == "section" and name ~= nil then
        if not table.find(song:sections(), name) then
            error(`the song has no section "{name}"`, level)
        end
    elseif kind == "marker" and name ~= nil then
        local found = false
        for _, section in song:sections() do
            found = found or table.find(song:markers(section), name) ~= nil
        end
        if not found then
            error(`the song has no marker "{name}"`, level)
        end
    elseif name ~= nil then
        error(`"{kind}" events have no names, so "{event}" is none`, level)
    end
    return kind, name
end

-- Whether `event` is of `kind`, and named `name` if that's given.
local function matches(event, kind, name)
    if kind == "bar" then
        return event.kind == "beat" and event.beat == 1
    elseif event.kind ~= kind then
        return false
    elseif name == nil then
        return true
    elseif kind == "section" then
        return event.section == name
    elseif kind == "marker" then
        return event.name == name
    else
        return event.track == name
    end
end

local listeners = {}

function listeners.on(performance, event, listener)
    local kind, name = parse(performance, event, 3)
    if type(listener) ~= "function" then
        error(`a song's listener is a function, not {typeof(listener)}`, 2)
    end
    local handle = { listening = true }
    spawn(function()
        while true do
            -- From the next update on: what the song says in this one happened before.
            coroutine.yield()
            if not handle.listening or not performance:is_playing() then
                return
            end
            for _, heard in performance:events() do
                if handle.listening and matches(heard, kind, name) then
                    spawn(listener, heard)
                end
            end
        end
    end)
    return handle
end

function listeners.off(_performance, handle)
    handle.listening = false
end

function listeners.wait_for(performance, event)
    local kind, name = parse(performance, event, 3)
    if not coroutine.isyieldable() then
        error("wait_for waits for the song, so it works only in a task, such as one `spawn` starts", 2)
    end
    while true do
        coroutine.yield()
        if not performance:is_playing() then
            return nil
        end
        for _, heard in performance:events() do
            if matches(heard, kind, name) then
                return heard
            end
        end
    end
end

return listeners
"#;

/// The Luau methods of songs playing, as [`SONG_LISTENERS`] makes them.
struct Listeners(Table);

/// Adds the functions for songs to `audio`, loading them relative to `folder`.
fn add_songs(
    lua: &Lua,
    audio: &Table,
    lent: &Lent,
    folder: &Path,
    spawn: Function,
) -> mlua::Result<()> {
    let listeners: Table = lua
        .load(SONG_LISTENERS)
        .set_name("=audio.songs")
        .call(spawn)?;
    listeners.set_readonly(true);
    lua.set_app_data(Listeners(listeners));
    let root = folder.to_owned();
    audio.set(
        "load_song",
        lua.create_function(move |_, path: String| {
            Song::load(root.join(path)).map_err(mlua::Error::external)
        })?,
    )?;
    audio.set("play_song", {
        let lent = lent.clone();
        lua.create_function(
            move |_, (song, settings): (UserDataRef<Song>, SongSettings)| {
                let performance = lent
                    .context(|ctx| ctx.audio().play_song(&song, settings))?
                    .map_err(mlua::Error::external)?;
                Ok(ScriptSong {
                    performance,
                    song: Song::clone(&song),
                    lent: lent.clone(),
                })
            },
        )?
    })?;
    audio.set("songs_playing", {
        let lent = lent.clone();
        lua.create_function(move |_, ()| lent.context(|ctx| ctx.audio().songs_playing()))?
    })?;
    audio.set("stop_songs", fader(lua, lent, Audio::stop_songs)?)?;
    Ok(())
}

/// A song playing, as scripts hold it: the [`Performance`] `audio.play_song` gave, the song, and
/// the context it reaches the [`Audio`] through. Its methods are in the [module docs](super).
struct ScriptSong {
    performance: Performance,
    song: Song,
    lent: Lent,
}

impl ScriptSong {
    /// Does `act` to the song, unless it's over.
    fn with<R>(&self, act: impl FnOnce(&mut PlayingSong) -> R) -> mlua::Result<Option<R>> {
        self.lent
            .context(|ctx| ctx.audio().song(self.performance).map(act))
    }

    /// Fails unless the song has a section named `section`.
    fn check_section(&self, section: &str) -> mlua::Result<()> {
        match self.song.section_index(section) {
            Some(_) => Ok(()),
            None => Err(mlua::Error::runtime(format!(
                "the song has no section \"{section}\""
            ))),
        }
    }

    /// Fails unless the song has a track named `track`.
    fn check_track(&self, track: &str) -> mlua::Result<()> {
        match self.song.track_index(track) {
            Some(_) => Ok(()),
            None => Err(mlua::Error::runtime(format!(
                "the song has no track \"{track}\""
            ))),
        }
    }
}

impl mlua::UserData for ScriptSong {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        for name in ["on", "off", "wait_for"] {
            fields.add_field_function_get(name, move |lua, _| {
                let listeners = lua
                    .app_data_ref::<Listeners>()
                    .ok_or_else(|| mlua::Error::runtime("songs' listeners aren't loaded"))?;
                listeners.0.get::<Function>(name)
            });
        }
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "go_to",
            |_, this, (section, at): (String, Option<String>)| {
                this.check_section(&section)?;
                let at = song_time(at)?;
                this.with(|song| song.go_to(&section, at))?
                    .transpose()
                    .map_err(mlua::Error::external)?;
                Ok(())
            },
        );
        methods.add_method(
            "set_tempo",
            |_, this, (tempo, settings): (f64, Option<Table>)| {
                if !tempo.is_finite() {
                    return Err(mlua::Error::runtime(format!(
                        "tempo must be a number, not {tempo}"
                    )));
                }
                let (at, over) = match settings {
                    Some(settings) => (
                        song_time(settings.get("at")?)?,
                        settings.get::<Option<f64>>("over")?.unwrap_or(0.0),
                    ),
                    None => (At::Now, 0.0),
                };
                if !over.is_finite() {
                    return Err(mlua::Error::runtime(format!(
                        "over must be a number of beats, not {over}"
                    )));
                }
                this.with(|song| song.set_tempo(tempo, at, over))?;
                Ok(())
            },
        );
        methods.add_method(
            "set_track_volume",
            |_, this, (track, volume, fade): (String, f32, Option<f32>)| {
                this.check_track(&track)?;
                if let Some(problem) = setting_problem("volume", volume) {
                    return Err(mlua::Error::runtime(problem));
                }
                this.with(|song| song.set_track_volume(&track, volume, fade.unwrap_or(0.0)))?
                    .transpose()
                    .map_err(mlua::Error::external)?;
                Ok(())
            },
        );
        methods.add_method(
            "listen",
            |_, this, (track, listen): (String, Option<bool>)| {
                this.check_track(&track)?;
                this.with(|song| song.listen(&track, listen.unwrap_or(true)))?
                    .transpose()
                    .map_err(mlua::Error::external)?;
                Ok(())
            },
        );
        methods.add_method(
            "set_volume",
            |_, this, (volume, fade): (f32, Option<f32>)| {
                if let Some(problem) = setting_problem("volume", volume) {
                    return Err(mlua::Error::runtime(problem));
                }
                this.with(|song| song.set_volume(volume, fade.unwrap_or(0.0)))?;
                Ok(())
            },
        );
        methods.add_method("pause", |_, this, fade: Option<f32>| {
            this.with(|song| song.pause(fade.unwrap_or(0.0)))?;
            Ok(())
        });
        methods.add_method("resume", |_, this, fade: Option<f32>| {
            this.with(|song| song.resume(fade.unwrap_or(0.0)))?;
            Ok(())
        });
        methods.add_method("stop", |_, this, fade: Option<f32>| {
            this.with(|song| song.stop(fade.unwrap_or(0.0)))?;
            Ok(())
        });
        methods.add_method("is_playing", |_, this, ()| Ok(this.with(|_| ())?.is_some()));
        methods.add_method("is_paused", |_, this, ()| {
            Ok(this.with(|song| song.is_paused())?.unwrap_or(false))
        });
        methods.add_method("has_ended", |_, this, ()| {
            Ok(this.with(|song| song.has_ended())?.unwrap_or(false))
        });
        methods.add_method("position", |_, this, ()| {
            let Some(position) = this.with(|song| song.position())? else {
                return Ok((None, None, None));
            };
            let (bar, beat) = position.bar(this.song.meter()[0]);
            let section = this.song.section_name(position.section).map(str::to_owned);
            Ok((section, Some(bar), Some(beat)))
        });
        methods.add_method("tempo", |_, this, ()| {
            this.with(|song| song.position().tempo)
        });
        methods.add_method("events", |lua, this, ()| {
            let events = this
                .with(|song| song.events().to_vec())?
                .unwrap_or_default();
            let list = lua.create_table_with_capacity(events.len(), 0)?;
            for event in events {
                list.push(event_table(lua, &this.song, event)?)?;
            }
            Ok(list)
        });
        methods.add_method("song", |_, this, ()| Ok(this.song.clone()));
    }
}

/// The time `name` names, or now without one.
fn song_time(name: Option<String>) -> mlua::Result<At> {
    match name {
        None => Ok(At::Now),
        Some(name) => At::from_name(&name).ok_or_else(|| {
            mlua::Error::runtime(format!(
                "a change to a song happens at \"now\", \"beat\", \"bar\" or \"section\", not \"{name}\""
            ))
        }),
    }
}

/// `event` as scripts see it: a table with its `kind`, and what else it has by name.
fn event_table(lua: &Lua, song: &Song, event: SongEvent) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    match event {
        SongEvent::Section(section) => {
            table.set("kind", "section")?;
            table.set("section", song.section_name(section))?;
        }
        SongEvent::Beat { bar, beat } => {
            table.set("kind", "beat")?;
            table.set("bar", bar)?;
            table.set("beat", beat)?;
        }
        SongEvent::Marker { section, marker } => {
            table.set("kind", "marker")?;
            table.set("section", song.section_name(section))?;
            table.set("name", song.marker_name(section, marker))?;
        }
        SongEvent::Note {
            track,
            pitch,
            velocity,
        } => {
            table.set("kind", "note")?;
            table.set("track", song.track_name(track))?;
            table.set("pitch", pitch.to_string())?;
            table.set("velocity", velocity)?;
        }
        SongEvent::End => table.set("kind", "end")?,
    }
    Ok(table)
}
