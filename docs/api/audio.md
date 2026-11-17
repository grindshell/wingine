# `audio`

Sound effects, music and songs. Files can be WAV or OGG Vorbis, and a sound effect can be
[made of samples](#audiomake) instead. [Songs](#songs) are written in song files of the engine's
own instruments, as [Songs](../music.md) describes, and play live, changing as the game asks.

Sound effects are loaded into memory once, start at once, and can overlap, even with themselves.
Each plays once, or loops, as a [voice](#voice), by which the game changes, pauses or stops it as
it plays. Music streams from its file as it plays, and loops; one piece plays at a time, and
changing it crossfades. Sound effects and music have volumes of their own, under an overall
volume.

Volumes are linear: 1 plays a sound as recorded, 0.5 at half its amplitude, and 0 is silent.

Everything in `audio`, and voices' and performances' methods, work while the game loads and in
`update` and its tasks, apart from `audio.load`, `audio.make` and `audio.load_song`, which work
any time. Without a sound device, and when a game runs
without a window, audio is silent, but sounds still start, play and finish on time.

```lua
local wg = require("@wingine")
local jump = wg.audio.load("sounds/jump.wav")
wg.audio.play_music("sounds/theme.ogg", 1)

function update(dt: number)
    if wg.input.pressed("jump") then
        wg.audio.play(jump, { volume = 0.8 })
    end
end
```

## Sound effects

### `audio.load`

```lua
wg.audio.load(path: string): Sound
```

Loads the sound effect at `path`, relative to the game's folder, decoding all of it into memory.
Load each sound once, when the game loads, rather than each time it plays.

### `audio.make`

```lua
wg.audio.make(samples: buffer, rate: number, channels: number?): Sound
```

Makes a sound effect of samples the game works out, rather than a file's: a shot, a step, a tone,
different every run if the game likes. `samples` is a `buffer` of 32-bit floats, as
`buffer.writef32` writes them, each from -1 to 1; `rate` is how many of them play a second, for
each of the sound's `channels`. A sound of 1 channel, the default, is heard from both speakers
alike; one of 2 has a sample for the left, then one for the right, and so on.

```lua
-- A tenth of a second of noise, dying away.
local RATE = 22050
local count = RATE // 10
local samples = buffer.create(count * 4)
for i = 0, count - 1 do
    local fade = 1 - i / count
    buffer.writef32(samples, i * 4, (math.random() * 2 - 1) * fade * fade)
end
local hiss = wg.audio.make(samples, RATE)
```

Make each sound once, not each time it plays: working out samples in Luau takes a millisecond or
so for each second of a simple sound, in a script that's `--!native`. A sound made is as one
loaded, and plays at any `speed`, so one made sound can be many pitches.

Samples past -1 and 1 clip as they're heard. It's an error if the buffer is empty, or isn't whole
samples for each channel; if a sample isn't a number; if `rate` isn't a whole number from 1 to
192,000; or if `channels` isn't 1 or 2.

### `audio.play`

```lua
wg.audio.play(sound: Sound, settings: PlaySettings?): Voice
```

Plays `sound`, and returns its [voice](#voice), which a game keeps if it will change or stop the
sound as it plays, and otherwise ignores. `settings` is a table with any of these:

| Setting | What it does | Default |
|---|---|---|
| `volume` | Linear volume: 1 is as recorded. | 1 |
| `speed` | Playback speed, which changes the pitch with it: 2 is twice as fast and an octave higher. Above 0. | 1 |
| `pan` | Where the sound is between the speakers, from -1 (left) through 0 (center) to 1 (right). | 0 |
| `looping` | Whether the sound plays over and over until its voice stops it, rather than once. | `false` |
| `fade_in` | Seconds the sound fades in over, from silence. | 0: at once |

```lua
-- Higher as the combo grows, and from where the hit happened.
wg.audio.play(hit, { speed = 1 + combo * 0.05, pan = (x / canvas_width) * 2 - 1 })

-- A hum for as long as the engine runs.
local hum = wg.audio.play(engine, { looping = true, fade_in = 0.5 })
```

A volume, speed or pan that isn't a number, or a speed that isn't above 0, is an error. If too
many sounds are playing at once to start another, the sound doesn't play, and the engine logs a
warning rather than stopping the game; its voice has finished already.

### `audio.stop_sounds`

```lua
wg.audio.stop_sounds(fade: number?)
```

Stops every sound effect playing, fading them out over `fade` seconds (by default, 0: at once).
Music carries on.

### `audio.pause_sounds`

```lua
wg.audio.pause_sounds(fade: number?)
```

Pauses every sound effect playing, fading them out over `fade` seconds (by default, 0: at once),
as when the game pauses. Sounds played after play as usual, so a pause menu's own sounds are heard.
Music carries on.

### `audio.resume_sounds`

```lua
wg.audio.resume_sounds(fade: number?)
```

Resumes every sound effect paused, by `audio.pause_sounds` or by its voice, fading them in over
`fade` seconds (by default, 0: at once).

```lua
if wg.input.pressed("pause") then
    paused = not paused
    if paused then
        wg.audio.pause_sounds()
    else
        wg.audio.resume_sounds()
    end
end
```

### `audio.sounds_playing`

```lua
wg.audio.sounds_playing(): number
```

Returns how many sound effects are playing, paused ones too.

### `Sound`

#### `sound:duration`

```lua
sound:duration(): number
```

Returns how long the sound lasts at its normal speed, in seconds.

### `Voice`

A sound effect playing, from [`audio.play`](#audioplay), by which the game changes, pauses or
stops it as it plays. Once the sound has finished, or been stopped, its voice does nothing, and
`voice:is_playing()` is false; voices aren't reused, so an old one never reaches a newer sound.

Each method that changes the sound takes a `fade`: the seconds it takes to get there, by default 0,
at once.

```lua
local wg = require("@wingine")
local steps = wg.audio.load("sounds/steps.wav")
local walking: Voice? = nil

function update(dt: number)
    local moving = wg.input.held("left") or wg.input.held("right")
    if moving and not walking then
        walking = wg.audio.play(steps, { looping = true })
    elseif not moving and walking then
        walking:stop(0.1)
        walking = nil
    end
    if walking then
        -- Quicker steps while running.
        walking:set_speed(if wg.input.held("run") then 1.5 else 1, 0.2)
    end
end
```

#### `voice:stop`

```lua
voice:stop(fade: number?)
```

Stops the sound, fading it out over `fade` seconds. It's over at once, as far as the game can
tell, and can't start again: play the sound again for that.

#### `voice:pause`

```lua
voice:pause(fade: number?)
```

Pauses the sound, fading it out over `fade` seconds, until it's resumed.

#### `voice:resume`

```lua
voice:resume(fade: number?)
```

Resumes the sound from where it paused, fading it in over `fade` seconds.

#### `voice:set_volume`

```lua
voice:set_volume(volume: number, fade: number?)
```

Changes the sound's volume, as `audio.play`'s `volume` sets it, over `fade` seconds. A volume that
isn't a number is an error.

#### `voice:set_speed`

```lua
voice:set_speed(speed: number, fade: number?)
```

Changes the sound's speed, and its pitch with it, as `audio.play`'s `speed` sets it, over `fade`
seconds. A speed that isn't a number above 0 is an error.

#### `voice:set_pan`

```lua
voice:set_pan(pan: number, fade: number?)
```

Moves the sound between the speakers, from -1 (left) to 1 (right), as `audio.play`'s `pan` sets
it, over `fade` seconds. A pan that isn't a number is an error.

#### `voice:is_playing`

```lua
voice:is_playing(): boolean
```

Returns whether the sound is still playing: it hasn't finished or been stopped. A paused sound is
still playing, only paused.

#### `voice:is_paused`

```lua
voice:is_paused(): boolean
```

Returns whether the sound is paused.

## Music

### `audio.play_music`

```lua
wg.audio.play_music(path: string, fade: number?)
```

Starts looping the music file at `path`, relative to the game's folder, streaming it from disk as
it plays. Music already playing fades out over `fade` seconds as the new music fades in (by
default, 0: it switches at once). A file that can't be read is an error.

### `audio.stop_music`

```lua
wg.audio.stop_music(fade: number?)
```

Stops the music, fading it out over `fade` seconds (by default, 0: at once).

### `audio.music_playing`

```lua
wg.audio.music_playing(): boolean
```

Returns whether music is playing.

## Songs

A song plays live: the engine works out its notes as it plays, from a [song file](../music.md), so
the game can change its tempo, move between its sections, fade its tracks in and out, and hear its
beats, sections, markers and notes as they pass. Songs play on the music track, alongside any music
and one another, so `audio.set_music_volume` sets their volume.

```lua
local wg = require("@wingine")
local theme = wg.audio.load_song("music/theme.song.luau")
local band = wg.audio.play_song(theme, { fade_in = 1 })

-- The drums flash the lights; the boss fight starts at the next bar.
band:on("note:drums", function(note)
    lights.flash = note.velocity
end)

function update(dt: number)
    if boss_appeared then
        band:go_to("boss", "bar")
        band:set_track_volume("strings", 1, 2)
    end
end
```

What happens in a song is heard an update after the mixer plays it, which is a few milliseconds
after the speakers do. Without a sound device, and when a game runs without a window, songs are
silent but keep time: what happens in them lands on the same updates every run, so tests can count
on it.

### `audio.load_song`

```lua
wg.audio.load_song(path: string): Song
```

Loads the song file at `path`, relative to the game's folder. A file that can't be read, or a song
that names an instrument, pattern or section it doesn't have, is an error. Load each song once.

### `audio.play_song`

```lua
wg.audio.play_song(song: Song, settings: SongSettings?): Performance
```

Plays `song` from its start, and returns its [performance](#performance), by which the game
changes it and hears what happens in it. `settings` is a table with any of these:

| Setting | What it does | Default |
|---|---|---|
| `volume` | Linear volume: 1 is as written. | 1 |
| `fade_in` | Seconds the song fades in over, from silence. | 0: at once |
| `section` | The section it starts with, by name. | the song's `start` |
| `beat` | Beats into that section it starts. | 0 |

A section the song doesn't have, or a volume, fade or beat that isn't a number, is an error.

### `audio.songs_playing`

```lua
wg.audio.songs_playing(): number
```

Returns how many songs are playing, paused ones too.

### `audio.stop_songs`

```lua
wg.audio.stop_songs(fade: number?)
```

Stops every song, fading them out over `fade` seconds (by default, 0: at once).

### `Song`

A song loaded by `audio.load_song`, whose methods tell what's in it.

| Method | Returns |
|---|---|
| `song:sections()` | The names of its sections, in order of their names. |
| `song:tracks()` | The names of its tracks, in order. |
| `song:start()` | The name of the section it starts with. |
| `song:tempo()` | Its tempo as it starts, in beats a minute. |
| `song:meter()` | Its meter: beats a bar, and the note that's a beat, such as `6, 8`. |
| `song:bars(section)` | How many bars the section lasts, or nil if there's no such section. |
| `song:markers(section)` | The names of the section's markers, in order, or nil if there's no such section. |

### `Performance`

A song playing, from [`audio.play_song`](#audioplay_song). Once it's stopped, or the update after
it ends, it's over: changing it does nothing, `performance:is_playing()` is false, and it has no
events or position. A name the song doesn't have is an error, even then.

Changes happen `at` one of these times:

| Time | When |
|---|---|
| `"now"` | At once. The default. |
| `"beat"` | At the start of the next beat. |
| `"bar"` | At the start of the next bar. |
| `"section"` | When the section playing ends, in place of whatever would follow it. |

#### `performance:go_to`

```lua
performance:go_to(section: string, at: SongTime?)
```

Moves to the section named `section`, `at` the time given. A section that's playing starts again.
What's ringing rings on, as the notes it was playing would have.

```lua
-- Into the chorus at the next bar, and on into the boss's music once the chorus ends.
band:go_to("chorus", "bar")
band:wait_for("section:chorus")
band:go_to("boss", "section")
```

#### `performance:set_tempo`

```lua
performance:set_tempo(tempo: number, settings: TempoSettings?)
```

Changes the tempo to `tempo` beats a minute, kept from 1 to 999. `settings` is a table of `at`, the
time it starts to change (by default, now), and `over`, how many beats it slides there over (by
default, 0: at once). Notes are as long in beats whatever the tempo, so a song goes faster as a
whole; instruments' envelopes are in seconds, so a note's attack lasts as long.

```lua
-- Faster each lap, sliding over a bar from the start of the next one.
band:set_tempo(120 + lap * 8, { at = "bar", over = 4 })
```

#### `performance:set_track_volume`

```lua
performance:set_track_volume(track: string, volume: number, fade: number?)
```

Fades the track named `track` to `volume` over `fade` seconds (by default, 0: at once, but never
so quickly it clicks). A song brings its layers in this way: tracks written with a `volume` of 0
are silent until the game fades them in.

#### `performance:listen`

```lua
performance:listen(track: string, listen: boolean?)
```

Has [`performance:events`](#performanceevents) list the notes the track named `track` starts, or,
with `false`, stop. Listening for `"note:<track>"` with `on` or `wait_for` listens to the track.

#### `performance:set_volume`, `performance:pause`, `performance:resume` and `performance:stop`

```lua
performance:set_volume(volume: number, fade: number?)
performance:pause(fade: number?)
performance:resume(fade: number?)
performance:stop(fade: number?)
```

As a [voice's](#voice): each over `fade` seconds. A paused song holds its place, beats and all,
until it's resumed. A stopped one is over at once, as far as the game can tell.

#### `performance:is_playing`, `performance:is_paused` and `performance:has_ended`

```lua
performance:is_playing(): boolean
performance:is_paused(): boolean
performance:has_ended(): boolean
```

Whether the song isn't over (a paused one is still playing), whether it's paused, and whether it
ended in the last update: its last section is over and its notes have faded. The update it ended in
is the last it plays.

#### `performance:position`

```lua
performance:position(): (string?, number?, number?)
```

Returns the section playing, the bar of it from 1, and how far into the bar from 1: 2.5 is halfway
through its second beat. All nil once it's over. It's where the mixer had played to at the last
update, so a game timing the player's presses to the music compares them with it.

#### `performance:tempo`

```lua
performance:tempo(): number?
```

Returns the tempo now, in beats a minute, or nil once it's over.

#### `performance:events`

```lua
performance:events(): { SongEvent }
```

Returns what happened in the song as the last update played, in order, each a table with a `kind`:

| `kind` | When | Its other fields |
|---|---|---|
| `"section"` | A section started. | `section` |
| `"beat"` | A beat started. | `bar` and `beat`, each from 1 |
| `"marker"` | The song passed a marker. | `section` and `name` |
| `"note"` | A note started on a track listened to. | `track`, `pitch` (such as `"C#4"`) and `velocity` |
| `"end"` | The song ended. | |

#### `performance:on` and `performance:off`

```lua
performance:on(event: string, listener: (SongEvent) -> ()): SongListener
performance:off(listener: SongListener)
```

Calls `listener` with each event named `event`, from the next update on, until `off` stops it or
the song is over. Each call is a task of its own, after `update`, so a listener can wait without
missing what happens meanwhile, and one that fails stops alone. Events are named:

| `event` | Calls `listener` for |
|---|---|
| `"beat"` | Every beat. |
| `"bar"` | Every beat that starts a bar. |
| `"section"`, `"section:chorus"` | Every section starting, or the one named. |
| `"marker"`, `"marker:drop"` | Every marker, or those named. |
| `"note:drums"` | Every note the track named starts, listening to it. |
| `"end"` | The song ending. |

```lua
local beats = 0
band:on("beat", function()
    beats += 1
end)
band:on("marker:drop", function()
    stage.confetti = true
end)
```

#### `performance:wait_for`

```lua
performance:wait_for(event: string): SongEvent?
```

Waits, in a task, for the next event named `event`, as `on` names them, and returns it; or nil if
the song is over first.

```lua
wg.spawn(function()
    band:wait_for("bar")
    band:set_track_volume("bass", 1, 0.5)
end)
```

#### `performance:song`

```lua
performance:song(): Song
```

Returns the song playing.

## Volume

### `audio.set_volume`

```lua
wg.audio.set_volume(volume: number)
```

Sets the volume of everything: sound effects and music.

### `audio.set_sound_volume`

```lua
wg.audio.set_sound_volume(volume: number)
```

Sets the volume of sound effects, apart from music, such as from an options menu.

### `audio.set_music_volume`

```lua
wg.audio.set_music_volume(volume: number)
```

Sets the volume of music, apart from sound effects.

A volume that isn't a number, such as `math.huge`, is an error for each of the three.

Each sound's own volume multiplies with its track's and the overall volume: a sound played at
`volume = 0.5`, with sound effects at 0.5, plays at a quarter of its amplitude.
