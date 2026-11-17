//! Songs: music made of Wingine's own instruments, written in song files, and baked to WAV files
//! or played as a game runs.
//!
//! A song plays its notes on [`Instrument`]s, sound generators of the engine's own: an oscillator
//! playing a [`Wave`], an envelope shaping each note's volume, a sweep and vibrato moving its
//! pitch, and a [`Filter`] shaping its tone. Its notes are in patterns, a few bars each, which its
//! tracks play in sections: stretches of the song a game can jump between as it plays, or that
//! follow one another, as an intro leads into a verse that loops.
//!
//! [`SongFile`] is what a song file holds, as the editor edits it; [`Song`] is one checked and
//! ready to play, which [`Song::render`] and [`Song::bake`] render.
//!
//! # Song files
//!
//! A song file, `*.song.luau` by convention, holds plain Luau values, as scene files do: it runs
//! with nothing in scope and returns a table. Positions and lengths are in beats, counted from 0,
//! and a bar is as many beats as the meter's first number.
//!
//! ```lua
//! return {
//!     format = 1,
//!     tempo = 128,          -- beats a minute; 120 if left out
//!     meter = { 4, 4 },     -- beats a bar, and the note a beat is; { 4, 4 } if left out
//!     start = "intro",      -- the section the song starts with
//!     instruments = {
//!         bass = {
//!             wave = "saw",
//!             decay = 0.15,
//!             sustain = 0.5,
//!             release = 0.08,
//!             filter = {
//!                 cutoff = 900,
//!                 resonance = 0.4,
//!             },
//!         },
//!         kick = {
//!             decay = 0.3,
//!             sustain = 0,
//!             sweep = 36,
//!             sweep_time = 0.06,
//!         },
//!     },
//!     tracks = {            -- in order, each playing one instrument
//!         {
//!             name = "drums",
//!             instrument = "kick",
//!         },
//!         {
//!             name = "bass",
//!             instrument = "bass",
//!             volume = 0,   -- silent, for a game to fade in
//!         },
//!     },
//!     patterns = {
//!         four = {
//!             bars = 1,
//!             notes = {     -- { beat, pitch, beats held, velocity if not 1 }
//!                 { 0, "C2", 0.5 },
//!                 { 1, "C2", 0.5 },
//!                 { 2, "C2", 0.5 },
//!                 { 3, "C2", 0.5, 0.6 },
//!             },
//!         },
//!     },
//!     sections = {
//!         intro = {
//!             bars = 4,
//!             tracks = {    -- the pattern each track plays; tracks left out are silent
//!                 drums = "four",
//!             },
//!             next = "verse",
//!         },
//!         verse = {
//!             bars = 8,
//!             tracks = {
//!                 bass = "four",
//!                 drums = "four",
//!             },
//!             next = "verse",
//!             markers = {   -- { beat, name }, for games to hear as they pass
//!                 { 16, "drop" },
//!             },
//!         },
//!     },
//! }
//! ```
//!
//! - **`tempo`** is from 1 to 999 beats a minute. **`meter`**'s note (1, 2, 4, 8, 16 or 32) only
//!   says how the song is written: tempo counts beats, whatever note they are.
//! - **Instruments** have these fields, each left out at its default:
//!   - `wave`: `"sine"` (the default), `"square"`, `"saw"`, `"triangle"` or `"noise"`, which
//!     hisses brighter for higher notes.
//!   - `duty`: the part of each cycle a square wave is high, above 0 and below 1; 0.5.
//!   - `attack`, `decay` and `release`: seconds a note takes to rise from silence, then fall to
//!     its sustain, and to fade once it's let go; each 0, and at most 60. Attacks and releases
//!     take at least [`MIN_RAMP`], so notes never click.
//!   - `sustain`: the volume a held note stays at after its decay, from 0 to 1; 1. At 0, a note
//!     is over once it has decayed, as a drum's is.
//!   - `volume`, 1, and `pan`, from -1 (left) to 1 (right), 0.
//!   - `sweep` and `sweep_time`: semitones each note's pitch starts away from the note, sliding
//!     to it over that many seconds, as a kick drum thumps; 0.
//!   - `vibrato = { rate, depth, delay }`: a wobble in the pitch, `rate` times a second and
//!     `depth` semitones either way, starting `delay` seconds into each note (0 if left out).
//!   - `filter = { kind, cutoff, resonance, sweep, sweep_time }`: `kind` is `"low"` (the
//!     default), `"high"` or `"band"`, which frequencies it lets through around `cutoff` Hz;
//!     `resonance`, from 0 to 1, how much it rings there; and `sweep`, octaves the cutoff starts
//!     each note away, sliding over `sweep_time` seconds. All but `cutoff` are 0 if left out.
//! - **Tracks** have a `name` no other track has, the `instrument` they play, and a `volume` (1)
//!   and `pan` (0). A song has at most [`MAX_TRACKS`].
//! - **Patterns** last some `bars`, at most [`MAX_BARS`], and hold `notes`, each
//!   `{ beat, pitch, beats, velocity }`: the beat it starts on, from 0 up to the pattern's
//!   length; its pitch, a name from `"C0"` to `"B8"` with `#` for sharp or `b` for flat; how
//!   many beats it's held, which may ring past the pattern's end; and how hard it's played, from
//!   0 to 1, 1 if left out. A pattern can't have two notes of one pitch on one beat.
//! - **Sections** last some `bars`, and have each of the `tracks` they name play a pattern,
//!   repeating it to fill the section if it's shorter. `next` names the section after it, which
//!   may be itself, so it loops; without one the song ends with it. `markers` name beats in it.
//!
//! Notes' lengths are beats, so they follow the tempo; instruments' times are seconds, so a note's
//! attack lasts as long however fast the song goes.
//!
//! [`SongFile::to_luau`] writes a song in its canonical layout, which sorts notes by their beats
//! and markers by theirs, and writes sharps for flats.

mod file;
mod live;
mod render;
mod sequencer;
mod synth;

use std::{path::Path, sync::Arc};

pub use file::{
    DEFAULT_METER, DEFAULT_TEMPO, MAX_BARS, MAX_TRACKS, Marker, Note, PatternDef, SONG_FORMAT,
    SectionDef, SongFile, TEMPO_RANGE, TrackDef,
};
pub use live::{Performance, PlayingSong, SongPosition, SongSettings};
pub use render::Rendered;
pub(crate) use sequencer::Sequencer;
pub use sequencer::{At, SongEvent};
pub use synth::{Filter, FilterKind, Instrument, MAX_TIME, MIN_RAMP, Pitch, Vibrato, Wave};

use crate::{Error, Result};

/// A song ready to play: a [`SongFile`] whose names all refer to something. Cloning is cheap:
/// clones share the song.
#[derive(Clone)]
pub struct Song {
    data: Arc<SongData>,
}

/// A song as it plays, with what it names found.
#[derive(Debug)]
pub(crate) struct SongData {
    pub tempo: f64,
    pub beats_per_bar: u32,
    pub meter: [u32; 2],
    pub tracks: Vec<TrackData>,
    pub patterns: Vec<PatternData>,
    pub sections: Vec<SectionData>,
    /// The section the song starts with.
    pub start: usize,
}

#[derive(Debug)]
pub(crate) struct TrackData {
    pub name: String,
    pub instrument: Instrument,
    pub volume: f32,
    pub pan: f32,
}

#[derive(Debug)]
pub(crate) struct PatternData {
    /// How many beats it lasts.
    pub beats: f64,
    /// Its notes, by their beats.
    pub notes: Vec<Note>,
}

#[derive(Debug)]
pub(crate) struct SectionData {
    pub name: String,
    /// How many beats it lasts.
    pub beats: f64,
    /// The pattern each track plays, by the tracks' indices.
    pub patterns: Vec<Option<usize>>,
    pub next: Option<usize>,
    /// Its markers, by their beats.
    pub markers: Vec<Marker>,
}

impl Song {
    /// Reads the song file at `path`, and checks it can be played.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        Self::new(&SongFile::read(path)?).map_err(|err| match err {
            Error::InvalidSong(message) => Error::Song {
                path: path.to_owned(),
                message,
            },
            err => err,
        })
    }

    /// The song `file` holds, or why it can't be played: it starts with a section it doesn't
    /// have, or a track, section or `next` names something it doesn't have.
    pub fn new(file: &SongFile) -> Result<Self> {
        Self::check(file).map_err(Error::InvalidSong)
    }

    fn check(file: &SongFile) -> Result<Self, String> {
        let beats_per_bar = file.beats_per_bar();
        let tracks = file
            .tracks
            .iter()
            .map(|track| {
                let instrument = file.instruments.get(&track.instrument).ok_or_else(|| {
                    format!(
                        "track `{}` plays instrument `{}`, which the song doesn't have",
                        track.name, track.instrument
                    )
                })?;
                Ok(TrackData {
                    name: track.name.clone(),
                    instrument: *instrument,
                    volume: track.volume,
                    pan: track.pan,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let pattern_names: Vec<&String> = file.patterns.keys().collect();
        let section_names: Vec<&String> = file.sections.keys().collect();
        let section_index = |name: &str| section_names.iter().position(|other| *other == name);
        let patterns = file
            .patterns
            .values()
            .map(|pattern| PatternData {
                beats: f64::from(pattern.bars * beats_per_bar),
                notes: pattern.notes.clone(),
            })
            .collect();
        let sections = file
            .sections
            .iter()
            .map(|(name, section)| {
                let mut patterns = vec![None; tracks.len()];
                for (track, pattern) in &section.tracks {
                    let index = file
                        .tracks
                        .iter()
                        .position(|other| &other.name == track)
                        .ok_or_else(|| {
                            format!("section `{name}` names track `{track}`, which the song doesn't have")
                        })?;
                    patterns[index] = Some(
                        pattern_names
                            .iter()
                            .position(|other| *other == pattern)
                            .ok_or_else(|| {
                                format!(
                                    "section `{name}` has track `{track}` play pattern \
                                     `{pattern}`, which the song doesn't have"
                                )
                            })?,
                    );
                }
                let next = section
                    .next
                    .as_ref()
                    .map(|next| {
                        section_index(next).ok_or_else(|| {
                            format!(
                                "section `{name}` is followed by section `{next}`, which the \
                                 song doesn't have"
                            )
                        })
                    })
                    .transpose()?;
                Ok(SectionData {
                    name: name.clone(),
                    beats: f64::from(section.bars * beats_per_bar),
                    patterns,
                    next,
                    markers: section.markers.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        let start = section_index(&file.start).ok_or_else(|| {
            format!(
                "the song starts with section `{}`, which it doesn't have",
                file.start
            )
        })?;
        Ok(Self {
            data: Arc::new(SongData {
                tempo: file.tempo,
                beats_per_bar,
                meter: file.meter,
                tracks,
                patterns,
                sections,
                start,
            }),
        })
    }

    pub(crate) fn data(&self) -> &Arc<SongData> {
        &self.data
    }

    /// Beats a minute, as the song starts.
    pub fn tempo(&self) -> f64 {
        self.data.tempo
    }

    /// Beats a bar, and the note that's a beat.
    pub fn meter(&self) -> [u32; 2] {
        self.data.meter
    }

    /// The section the song starts with.
    pub fn start(&self) -> &str {
        &self.data.sections[self.data.start].name
    }

    /// The names of its sections, in order of their names.
    pub fn sections(&self) -> impl Iterator<Item = &str> {
        self.data
            .sections
            .iter()
            .map(|section| section.name.as_str())
    }

    /// The names of its tracks, in order.
    pub fn tracks(&self) -> impl Iterator<Item = &str> {
        self.data.tracks.iter().map(|track| track.name.as_str())
    }

    /// The index of the section named `name`, as [`SongEvent::Section`] gives it.
    pub fn section_index(&self, name: &str) -> Option<usize> {
        self.data.section(name)
    }

    /// The name of the section at `index`, as [`SongEvent::Section`] gives it.
    pub fn section_name(&self, index: usize) -> Option<&str> {
        self.data
            .sections
            .get(index)
            .map(|section| section.name.as_str())
    }

    /// The index of the track named `name`, as [`SongEvent::Note`] gives it.
    pub fn track_index(&self, name: &str) -> Option<usize> {
        self.data.tracks.iter().position(|track| track.name == name)
    }

    /// The name of the track at `index`, as [`SongEvent::Note`] gives it.
    pub fn track_name(&self, index: usize) -> Option<&str> {
        self.data.tracks.get(index).map(|track| track.name.as_str())
    }

    /// The name of marker `marker` of the section at `section`, as [`SongEvent::Marker`] gives
    /// them.
    pub fn marker_name(&self, section: usize, marker: usize) -> Option<&str> {
        let markers = &self.data.sections.get(section)?.markers;
        markers.get(marker).map(|marker| marker.name.as_str())
    }

    /// The names of the markers in the section named `section`, in order of their beats.
    pub fn markers(&self, section: &str) -> Option<impl Iterator<Item = &str>> {
        let section = &self.data.sections[self.data.section(section)?];
        Some(section.markers.iter().map(|marker| marker.name.as_str()))
    }

    /// How many bars the section named `name` lasts.
    pub fn section_bars(&self, name: &str) -> Option<u32> {
        let section = &self.data.sections[self.data.section(name)?];
        Some((section.beats / f64::from(self.data.beats_per_bar)) as u32)
    }
}

impl SongData {
    pub(crate) fn section(&self, name: &str) -> Option<usize> {
        self.sections
            .iter()
            .position(|section| section.name == name)
    }
}

impl std::fmt::Debug for Song {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Song")
            .field("tempo", &self.data.tempo)
            .field("sections", &self.sections().collect::<Vec<_>>())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(source: &str) -> SongFile {
        SongFile::parse(source, "test.song.luau").unwrap()
    }

    fn problem(source: &str) -> String {
        Song::new(&file(source)).unwrap_err().to_string()
    }

    const SONG: &str = r#"return {
        format = 1,
        start = "a",
        instruments = { sine = {} },
        tracks = { { name = "lead", instrument = "sine" }, { name = "bass", instrument = "sine" } },
        patterns = { p = { bars = 1, notes = { { 0, "C4", 1 } } }, q = { bars = 2 } },
        sections = {
            a = { bars = 2, tracks = { bass = "q", lead = "p" }, next = "b", markers = { { 1, "m" } } },
            b = { bars = 1, next = "b" },
        },
    }"#;

    #[test]
    fn songs_find_what_they_name() {
        let song = Song::new(&file(SONG)).unwrap();
        assert_eq!(song.start(), "a");
        assert_eq!(song.sections().collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(song.tracks().collect::<Vec<_>>(), ["lead", "bass"]);
        assert_eq!(song.section_bars("a"), Some(2));
        assert_eq!(song.marker_name(0, 0), Some("m"));
        assert_eq!(song.track_index("bass"), Some(1));
        let data = song.data();
        assert_eq!(data.sections[0].patterns, [Some(0), Some(1)]);
        assert_eq!(data.sections[0].next, Some(1));
        assert_eq!(data.sections[1].patterns, [None, None]);
        assert_eq!(data.patterns[1].beats, 8.0);
    }

    #[test]
    fn songs_that_name_what_they_dont_have_say_so() {
        let cases = [
            (
                r#"start = "a""#,
                r#"start = "z""#,
                "the song starts with section `z`",
            ),
            (
                r#"instrument = "sine" }, { name = "bass""#,
                r#"instrument = "organ" }, { name = "bass""#,
                "track `lead` plays instrument `organ`, which the song doesn't have",
            ),
            (
                r#"lead = "p""#,
                r#"lead = "r""#,
                "section `a` has track `lead` play pattern `r`",
            ),
            (
                r#"lead = "p""#,
                r#"solo = "p""#,
                "section `a` names track `solo`",
            ),
            (
                r#"next = "b" }"#,
                r#"next = "c" }"#,
                "section `b` is followed by section `c`",
            ),
        ];
        for (from, to, expected) in cases {
            assert!(SONG.contains(from), "{from}");
            let problem = problem(&SONG.replacen(from, to, 1));
            assert!(problem.contains(expected), "{problem}");
        }
    }

    #[test]
    fn loading_names_the_file() {
        let dir = std::env::temp_dir().join(format!("wingine-song-load-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.song.luau");
        std::fs::write(&path, SONG.replace(r#"start = "a""#, r#"start = "z""#)).unwrap();
        let err = Song::load(&path).unwrap_err().to_string();
        assert!(
            err.starts_with("song ") && err.contains("bad.song.luau: the song starts"),
            "{err}"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
