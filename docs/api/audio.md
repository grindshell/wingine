# `audio`

Sound effects and music. Files can be WAV or OGG Vorbis.

Sound effects are loaded into memory once, start at once, and can overlap, even with themselves.
Music streams from its file as it plays, and loops; one piece plays at a time, and changing it
crossfades. Sound effects and music have volumes of their own, under an overall volume.

Volumes are linear: 1 plays a sound as recorded, 0.5 at half its amplitude, and 0 is silent.

Everything in `audio` works while the game loads and in `update` and its tasks, apart from
`audio.load`, which works any time. Without a sound device, and when a game runs without a window,
audio is silent, but sounds still start, play and finish on time.

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
audio.play(sound: Sound, settings: PlaySettings?)
```

Plays `sound` once. `settings` is a table with any of these, each a number:

| Setting | What it does | Default |
|---|---|---|
| `volume` | Linear volume: 1 is as recorded. | 1 |
| `speed` | Playback speed, which changes the pitch with it: 2 is twice as fast and an octave higher. Above 0. | 1 |
| `pan` | Where the sound is between the speakers, from -1 (left) through 0 (center) to 1 (right). | 0 |

```lua
-- Higher as the combo grows, and from where the hit happened.
audio.play(hit, { speed = 1 + combo * 0.05, pan = (x / canvas_width) * 2 - 1 })
```

Settings that aren't numbers, or a speed that isn't above 0, are an error. If too many sounds are
playing at once to start another, the sound doesn't play, and the engine logs a warning rather
than stopping the game.

### `audio.stop_sounds`

```lua
audio.stop_sounds(fade: number?)
```

Stops every sound effect playing, fading them out over `fade` seconds (by default, 0: at once).
Music carries on.

### `audio.sounds_playing`

```lua
audio.sounds_playing(): number
```

Returns how many sound effects are playing.

### `Sound`

#### `sound:duration`

```lua
sound:duration(): number
```

Returns how long the sound lasts at its normal speed, in seconds.

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

Each sound's own volume multiplies with its track's and the overall volume: a sound played at
`volume = 0.5`, with sound effects at 0.5, plays at a quarter of its amplitude.
