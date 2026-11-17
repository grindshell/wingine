# Songs

A song is music written for the engine's own instruments, rather than recorded: a file of notes
that the engine plays by working out each note's sound as it goes. A game can bake a song to a WAV
file and play it as any music, or play it live, where it changes as the game asks: faster, into
another section at the next bar, with a track faded in as the player does well. The game hears
its beats, sections, markers and notes as they pass, to time what it does to the music.

```lua
local wg = require("@wingine")
local theme = wg.audio.load_song("music/theme.song.luau")
local band = wg.audio.play_song(theme)

band:on("bar", function()
    stage.pulse = 1
end)
```

## What's in a song

- **Instruments** are sound generators: an oscillator playing a wave, an envelope shaping each
  note's volume, a sweep and a vibrato moving its pitch, and a filter shaping its tone.
- **Tracks** each play one instrument, such as the drums or the bass, at a volume and a place
  between the speakers. A game fades tracks in and out, so a song can be written in layers.
- **Patterns** are notes, some bars of them: which beat each starts on, its pitch, how many beats
  it's held, and how hard it's played.
- **Sections** are stretches of the song, some bars long, in which each track plays a pattern, or
  nothing. A pattern shorter than its section repeats to fill it. Each section names the one that
  follows it, which may be itself, so it loops; one that names none ends the song.

Positions and lengths are in beats, counted from 0. The song's tempo counts beats a minute, and its
meter says how many beats a bar has. Notes are as long in beats whatever the tempo, so a song plays
faster as a whole; instruments' times are in seconds, so a note's attack lasts as long.

## Song files

A song file, `name.song.luau` by convention, holds plain Luau values as a scene file does: it runs
with nothing in scope and returns a table. Fields at their defaults can be left out. The editor's
[Music screen](editor.md#writing-songs) writes them, playing a song as it's changed.

```lua
return {
    format = 1,
    tempo = 128,
    meter = { 4, 4 },
    start = "intro",
    instruments = {
        bass = {
            wave = "saw",
            decay = 0.15,
            sustain = 0.5,
            release = 0.08,
            filter = {
                cutoff = 900,
                resonance = 0.4,
            },
        },
        kick = {
            decay = 0.3,
            sustain = 0,
            sweep = 36,
            sweep_time = 0.06,
        },
    },
    tracks = {
        {
            name = "drums",
            instrument = "kick",
        },
        {
            name = "bass",
            instrument = "bass",
            volume = 0,
        },
    },
    patterns = {
        four = {
            bars = 1,
            notes = {
                { 0, "C2", 0.5 },
                { 1, "C2", 0.5 },
                { 2, "C2", 0.5 },
                { 3, "C2", 0.5, 0.6 },
            },
        },
    },
    sections = {
        intro = {
            bars = 4,
            tracks = {
                drums = "four",
            },
            next = "verse",
        },
        verse = {
            bars = 8,
            tracks = {
                bass = "four",
                drums = "four",
            },
            next = "verse",
            markers = {
                { 16, "drop" },
            },
        },
    },
}
```

| Field | What it is | Default |
|---|---|---|
| `format` | The version of the format: 1. | required |
| `tempo` | Beats a minute, from 1 to 999. | 120 |
| `meter` | Beats a bar, and the note that's a beat (1, 2, 4, 8, 16 or 32). The note only says how the song is written. | `{ 4, 4 }` |
| `start` | The section the song starts with. | required |
| `instruments` | Instruments by name. | none |
| `tracks` | The tracks, in order: each a `name` no other has, the `instrument` it plays, a `volume` (1) and a `pan` (0). At most 32. | none |
| `patterns` | Patterns by name: each lasts some `bars`, and has `notes`. | none |
| `sections` | Sections by name: each lasts some `bars`, has the `tracks` it names play a pattern, and may name the section `next` after it and `markers` in it. | none |

A note is `{ beat, pitch, beats, velocity }`: the beat it starts on, from 0 up to its pattern's
length; its pitch, from `"C0"` to `"B8"`, with `#` for sharp or `b` for flat, A4 tuned to 440 Hz;
how many beats it's held, which may ring past its pattern's end; and how hard it's played, from 0
to 1, 1 if it's left out. A pattern can't have two notes of one pitch on one beat. A marker is
`{ beat, name }`, a beat in its section that games hear pass by name.

Patterns and sections last at most 1,000 bars. Reading a file checks every field, and errors say
where the problem is, such as `` pattern `bass`: note 3: it must last above 0 beats ``. A song that
names an instrument, pattern or section it doesn't have can't be played.

### Instruments

| Field | What it does | Default |
|---|---|---|
| `wave` | What the oscillator plays: `"sine"`, a pure tone; `"square"`, hollow; `"saw"`, bright and buzzy; `"triangle"`, soft; or `"noise"`, for drums and cymbals, which hisses brighter for higher notes. | `"sine"` |
| `duty` | The part of each cycle a square wave is high, above 0 and below 1: thinner and more nasal toward either end. | 0.5 |
| `attack` | Seconds a note takes to rise from silence. | 0 |
| `decay` | Seconds it then takes to fall to its sustain. | 0 |
| `sustain` | The volume a held note stays at after its decay, from 0 to 1. At 0, a note is over once it has decayed, as a drum's is. | 1 |
| `release` | Seconds a note takes to fade once it's let go. | 0 |
| `volume` | Linear volume. | 1 |
| `pan` | From -1 (left) to 1 (right). | 0 |
| `sweep` | Semitones each note's pitch starts away from the note, sliding to it: above for a kick drum's thump or a laser's zap, below for a swoop up. | 0 |
| `sweep_time` | Seconds the pitch slides over. | 0 |
| `vibrato` | A wobble in the pitch: `{ rate, depth, delay }`, `rate` times a second, `depth` semitones either way, starting `delay` seconds into each note (0). | none |
| `filter` | `{ kind, cutoff, resonance, sweep, sweep_time }`: `kind` is `"low"`, `"high"` or `"band"`, which frequencies it lets through around `cutoff` Hz; `resonance`, from 0 to 1, how much it rings there; `sweep`, octaves the cutoff starts each note away, sliding over `sweep_time` seconds. All but `cutoff` are 0 if left out, and `kind` is `"low"`. | none |

Envelope times and sweep times are at most 60 seconds. Attacks and releases take at least 2
milliseconds, so notes never click as they start or end.

A song plays at most 64 notes at once; past that, a new note takes the place of the oldest one
fading, or else the oldest.

## Baking

```sh
wingine bake music/theme.song.luau sounds/theme.wav
wingine bake music/theme.song.luau sounds/boss.wav boss
```

`wingine bake` renders a song to a 16-bit stereo WAV file at 48,000 Hz, which `audio.play_music`
and `audio.load` play as any other. A song renders once through: from its start along each
section's `next`, until it ends or would play a section it has played already. One that loops back
to the section it started with renders as one pass of the loop, with what rings past the end mixed
into the start, so played over and over it loops without a seam. One that loops back to another
section renders up to there, then rings out. Naming a section renders that section alone, as a
loop.

A bake is the same, sample for sample, on every platform, and the same as the song played live at
that rate: only a game's changes as it plays make live music differ.

Baking suits music that doesn't change: it costs nothing to play but streaming a file. Playing live
costs a little of the audio thread's time for each note playing, a few percent of a core at most,
and lets the game change it.

## Playing live

`audio.play_song` plays a song and returns its performance, by which the game changes it and
hears it. See [`audio`](api/audio.md#songs) for each function.

- **Move between sections** with `performance:go_to(section, at)`: at once, or at the start of the
  next beat or bar, so the music changes on the beat, or when the section playing ends, in place of
  what would follow it.
- **Change the tempo** with `performance:set_tempo(tempo, { at, over })`, at once or sliding over
  some beats.
- **Bring layers in and out** with `performance:set_track_volume(track, volume, fade)`. Write the
  layers as tracks with a `volume` of 0, and fade them in.
- **Time things to the music** with `performance:on(event, listener)`, which calls a listener for
  each beat, bar, section, marker, note of a track, or the end, and `performance:wait_for(event)`,
  which waits in a task for the next. `performance:position()` says where the song is, for timing
  the player's presses.

```lua
local wg = require("@wingine")
local theme = wg.audio.load_song("music/theme.song.luau")
local band = wg.audio.play_song(theme)
local streak = 0

-- Layers come in as the player keeps time, and the chorus starts at the next bar once they have.
function hit_on_the_beat()
    streak += 1
    if streak == 8 then
        band:set_track_volume("bass", 1, 1)
    elseif streak == 16 then
        band:set_track_volume("lead", 1, 1)
        band:go_to("chorus", "bar")
    end
end
```

The [bandstand example](../examples/bandstand) is a rhythm game made this way, its presses judged
by the song's beats and position. [`audio`](api/audio.md#songs) says when a game hears what
happens in a song, and how songs keep time without a sound device.
