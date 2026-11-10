# `audio`

Sound effects and music. Files can be WAV or OGG Vorbis.

Sound effects are loaded into memory once, start at once, and can overlap, even with themselves.
Each plays once, or loops, as a [voice](#voice), by which the game changes, pauses or stops it as
it plays. Music streams from its file as it plays, and loops; one piece plays at a time, and
changing it crossfades. Sound effects and music have volumes of their own, under an overall
volume.

Volumes are linear: 1 plays a sound as recorded, 0.5 at half its amplitude, and 0 is silent.

Everything in `audio`, and voices' methods, work while the game loads and in `update` and its
tasks, apart from `audio.load`, which works any time. Without a sound device, and when a game runs
without a window, audio is silent, but sounds still start, play and finish on time.

```lua
local jump = audio.load("sounds/jump.wav")
audio.play_music("sounds/theme.ogg", 1)

function update(dt: number)
    if input.pressed("jump") then
        audio.play(jump, { volume = 0.8 })
    end
end
```

## Sound effects

### `audio.load`

```lua
audio.load(path: string): Sound
```

Loads the sound effect at `path`, relative to the game's folder, decoding all of it into memory.
Load each sound once, when the game loads, rather than each time it plays.

### `audio.play`

```lua
audio.play(sound: Sound, settings: PlaySettings?): Voice
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
audio.play(hit, { speed = 1 + combo * 0.05, pan = (x / canvas_width) * 2 - 1 })

-- A hum for as long as the engine runs.
local hum = audio.play(engine, { looping = true, fade_in = 0.5 })
```

A volume, speed or pan that isn't a number, or a speed that isn't above 0, is an error. If too
many sounds are playing at once to start another, the sound doesn't play, and the engine logs a
warning rather than stopping the game; its voice has finished already.

### `audio.stop_sounds`

```lua
audio.stop_sounds(fade: number?)
```

Stops every sound effect playing, fading them out over `fade` seconds (by default, 0: at once).
Music carries on.

### `audio.pause_sounds`

```lua
audio.pause_sounds(fade: number?)
```

Pauses every sound effect playing, fading them out over `fade` seconds (by default, 0: at once),
as when the game pauses. Sounds played after play as usual, so a pause menu's own sounds are heard.
Music carries on.

### `audio.resume_sounds`

```lua
audio.resume_sounds(fade: number?)
```

Resumes every sound effect paused, by `audio.pause_sounds` or by its voice, fading them in over
`fade` seconds (by default, 0: at once).

```lua
if input.pressed("pause") then
    paused = not paused
    if paused then
        audio.pause_sounds()
    else
        audio.resume_sounds()
    end
end
```

### `audio.sounds_playing`

```lua
audio.sounds_playing(): number
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
local steps = audio.load("sounds/steps.wav")
local walking: Voice? = nil

function update(dt: number)
    local moving = input.held("left") or input.held("right")
    if moving and not walking then
        walking = audio.play(steps, { looping = true })
    elseif not moving and walking then
        walking:stop(0.1)
        walking = nil
    end
    if walking then
        -- Quicker steps while running.
        walking:set_speed(if input.held("run") then 1.5 else 1, 0.2)
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
audio.play_music(path: string, fade: number?)
```

Starts looping the music file at `path`, relative to the game's folder, streaming it from disk as
it plays. Music already playing fades out over `fade` seconds as the new music fades in (by
default, 0: it switches at once). A file that can't be read is an error.

### `audio.stop_music`

```lua
audio.stop_music(fade: number?)
```

Stops the music, fading it out over `fade` seconds (by default, 0: at once).

### `audio.music_playing`

```lua
audio.music_playing(): boolean
```

Returns whether music is playing.

## Volume

### `audio.set_volume`

```lua
audio.set_volume(volume: number)
```

Sets the volume of everything: sound effects and music.

### `audio.set_sound_volume`

```lua
audio.set_sound_volume(volume: number)
```

Sets the volume of sound effects, apart from music, such as from an options menu.

### `audio.set_music_volume`

```lua
audio.set_music_volume(volume: number)
```

Sets the volume of music, apart from sound effects.

A volume that isn't a number, such as `math.huge`, is an error for each of the three.

Each sound's own volume multiplies with its track's and the overall volume: a sound played at
`volume = 0.5`, with sound effects at 0.5, plays at a quarter of its amplitude.
