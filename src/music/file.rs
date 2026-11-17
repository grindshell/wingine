//! What song files hold, and reading and writing them.
//!
//! A song file runs as Luau with nothing in scope, as a scene file does, and what it returns is
//! read into a [`SongFile`]: each field is checked to be what it should be, with errors naming
//! where it is, such as `pattern `bass`: note 3: …`. Whether the names it uses refer to anything
//! is [`Song::new`](super::Song::new)'s to check, so an editor can hold a file that names
//! something it no longer has.
//!
//! [`SongFile::to_luau`] writes one in its canonical layout, as scenes are written: every field on
//! a line of its own in a fixed order, fields at their defaults left out, and each note or marker
//! on one line.

use std::{collections::BTreeMap, fmt::Write, path::Path};

use super::synth::{Filter, FilterKind, Instrument, Pitch, Vibrato, Wave};
use crate::{
    Error, Result, save,
    scene::{
        Fields, Value, Writer, describe_value, evaluate, finite, list, number, positive_whole,
        quoted, read_file, single, string, whole,
    },
};

/// The version of the song format this engine writes, its `format`. It reads files of this
/// format and every one before it.
pub const SONG_FORMAT: u32 = 1;

/// The most tracks a song may have.
pub const MAX_TRACKS: usize = 32;

/// The most bars a pattern or section may last.
pub const MAX_BARS: u32 = 1000;

/// The slowest and fastest a song may go, in beats a minute.
pub const TEMPO_RANGE: [f64; 2] = [1.0, 999.0];

/// The tempo a song has unless its file says otherwise.
pub const DEFAULT_TEMPO: f64 = 120.0;

/// The meter a song has unless its file says otherwise: four beats a bar, each a quarter note.
pub const DEFAULT_METER: [u32; 2] = [4, 4];

/// What a song file holds: its tempo and meter, its instruments, its tracks, the patterns of notes
/// they play, and the sections those are arranged in. See the [module docs](super) for the format.
///
/// Reading a file and writing it back with [`SongFile::to_luau`] gives the file in its canonical
/// layout, and reading that gives the same `SongFile`.
#[derive(Clone, Debug, PartialEq)]
pub struct SongFile {
    /// Beats a minute, from 1 to 999.
    pub tempo: f64,
    /// Beats a bar, and the note that's a beat (1, 2, 4, 8, 16 or 32), as `{ 6, 8 }` is six
    /// eighth notes a bar. The note only says how the song is written: tempo counts beats.
    pub meter: [u32; 2],
    /// The section the song starts with.
    pub start: String,
    /// Instruments by name, for tracks to play.
    pub instruments: BTreeMap<String, Instrument>,
    /// The song's tracks, in order, each playing one instrument.
    pub tracks: Vec<TrackDef>,
    /// Patterns of notes by name, for sections to have tracks play.
    pub patterns: BTreeMap<String, PatternDef>,
    /// Sections by name: stretches of the song, each some bars long.
    pub sections: BTreeMap<String, SectionDef>,
}

impl Default for SongFile {
    /// A song with nothing in it, at the default tempo and meter, starting with a section named
    /// `main` it doesn't have yet.
    fn default() -> Self {
        Self {
            tempo: DEFAULT_TEMPO,
            meter: DEFAULT_METER,
            start: "main".to_owned(),
            instruments: BTreeMap::new(),
            tracks: Vec::new(),
            patterns: BTreeMap::new(),
            sections: BTreeMap::new(),
        }
    }
}

/// One of a song's tracks: a part played on one instrument, such as the drums or the bass, which a
/// game can fade in and out.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackDef {
    /// Its name, which no other track has.
    pub name: String,
    /// The instrument it plays, by name.
    pub instrument: String,
    /// Linear volume: 1 is full, and 0 silent, as a layer a game fades in later.
    pub volume: f32,
    /// Stereo position, from -1 (left) through 0 (center) to 1 (right).
    pub pan: f32,
}

impl TrackDef {
    /// A track named `name` playing `instrument`, at full volume in the middle.
    pub fn new(name: impl Into<String>, instrument: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            instrument: instrument.into(),
            volume: 1.0,
            pan: 0.0,
        }
    }
}

/// Notes for a track to play, some bars long. A pattern shorter than the section playing it
/// repeats to fill the section.
#[derive(Clone, Debug, PartialEq)]
pub struct PatternDef {
    /// How many bars it lasts.
    pub bars: u32,
    /// Its notes, in order of their beats, then their pitches.
    pub notes: Vec<Note>,
}

/// A note in a pattern.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Note {
    /// The beat it starts on, from 0 at the pattern's start.
    pub beat: f64,
    pub pitch: Pitch,
    /// How many beats it's held, above 0. It may ring past the pattern's end.
    pub length: f64,
    /// How hard it's played, from 0 to 1.
    pub velocity: f32,
}

/// A stretch of a song, some bars long, in which each track plays a pattern, or nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct SectionDef {
    /// How many bars it lasts.
    pub bars: u32,
    /// The pattern each track plays, by their names. Tracks it doesn't name are silent.
    pub tracks: BTreeMap<String, String>,
    /// The section that follows it, which may be itself, so it loops; without one, the song
    /// ends with it.
    pub next: Option<String>,
    /// Named beats in it, for games to hear as they pass, in order.
    pub markers: Vec<Marker>,
}

impl SectionDef {
    /// A section lasting `bars` with nothing in it, and nothing after it.
    pub fn new(bars: u32) -> Self {
        Self {
            bars,
            tracks: BTreeMap::new(),
            next: None,
            markers: Vec::new(),
        }
    }
}

/// A named beat in a section.
#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    /// The beat it's at, from 0 at the section's start.
    pub beat: f64,
    pub name: String,
}

impl SongFile {
    /// Reads the song file at `path`.
    pub fn read(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        Self::parse(&read_file(path)?, path)
    }

    /// Reads a song from `source`, the contents of a song file, naming it `path` in errors, as
    /// [`SceneFile::parse`](crate::scene::SceneFile::parse) reads a scene.
    pub fn parse(source: &str, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let error = |message: String| Error::Song {
            path: path.to_owned(),
            message,
        };
        read_song(evaluate(source, path).map_err(error)?).map_err(error)
    }

    /// How many beats a bar has.
    pub fn beats_per_bar(&self) -> u32 {
        self.meter[0]
    }

    /// The song as the text of a song file, in the canonical layout. Writing the same song always
    /// gives the same text.
    ///
    /// Fails if a number isn't finite, which a song file can't hold.
    pub fn to_luau(&self) -> Result<String> {
        write_song(self).map_err(Error::InvalidSong)
    }

    /// Writes the song to the file at `path`, as [`SongFile::to_luau`] gives it, replacing the
    /// old one as [`SceneFile::write`](crate::scene::SceneFile::write) does.
    pub fn write(&self, path: impl AsRef<Path>) -> Result<()> {
        save::write_replacing(path.as_ref(), self.to_luau()?.as_bytes())
    }
}

fn read_song(song: Value) -> Result<SongFile, String> {
    let at_song = |err: String| format!("the song: {err}");
    let mut fields = Fields::new(song, "a song").map_err(at_song)?;
    check_format(fields.take("format"))?;
    let tempo = match fields.take("tempo") {
        Some(tempo) => read_tempo(tempo).map_err(at_song)?,
        None => DEFAULT_TEMPO,
    };
    let meter = match fields.take("meter") {
        Some(meter) => read_meter(meter).map_err(at_song)?,
        None => DEFAULT_METER,
    };
    let beats_per_bar = meter[0];
    let start = match fields.take("start") {
        Some(start) => string(start, "start").map_err(at_song)?,
        None => return Err("the song has no `start`, the section it starts with".to_owned()),
    };
    let instruments = named(fields.take("instruments"), "instruments", |name, value| {
        read_instrument(value).map_err(|err| format!("instrument `{name}`: {err}"))
    })?;
    let tracks = match fields.take("tracks") {
        Some(tracks) => read_tracks(tracks)?,
        None => Vec::new(),
    };
    let patterns = named(fields.take("patterns"), "patterns", |name, value| {
        read_pattern(value, beats_per_bar).map_err(|err| format!("pattern `{name}`: {err}"))
    })?;
    let sections = named(fields.take("sections"), "sections", |name, value| {
        read_section(value, beats_per_bar).map_err(|err| format!("section `{name}`: {err}"))
    })?;
    fields.finish("a song").map_err(at_song)?;
    Ok(SongFile {
        tempo,
        meter,
        start,
        instruments,
        tracks,
        patterns,
        sections,
    })
}

fn check_format(format: Option<Value>) -> Result<(), String> {
    let Some(format) = format else {
        return Err(format!(
            "the song has no `format`; this engine reads formats 1 to {SONG_FORMAT}"
        ));
    };
    match whole(format, "format").map_err(|err| format!("the song: {err}"))? {
        1..=SONG_FORMAT => Ok(()),
        newer if newer > SONG_FORMAT => Err(format!(
            "the song is format {newer}, newer than this engine reads ({SONG_FORMAT})"
        )),
        other => Err(format!("there's no song format {other}")),
    }
}

/// The table `value` of things by name, each read by `read`; an empty map without it.
fn named<T>(
    value: Option<Value>,
    field: &str,
    mut read: impl FnMut(&str, Value) -> Result<T, String>,
) -> Result<BTreeMap<String, T>, String> {
    let Some(value) = value else {
        return Ok(BTreeMap::new());
    };
    let fields =
        Fields::new(value, &format!("`{field}`")).map_err(|err| format!("the song: {err}"))?;
    fields
        .fields
        .into_iter()
        .map(|(name, value)| Ok((name.clone(), read(&name, value)?)))
        .collect()
}

fn read_tempo(value: Value) -> Result<f64, String> {
    let tempo = number(value, "tempo")?;
    let [least, most] = TEMPO_RANGE;
    if (least..=most).contains(&tempo) {
        Ok(tempo)
    } else {
        Err(format!(
            "`tempo` must be from {least} to {most} beats a minute, not {tempo}"
        ))
    }
}

fn read_meter(value: Value) -> Result<[u32; 2], String> {
    let shape = || "`meter` must be two whole numbers, { beats, note }".to_owned();
    let [beats, note] = <[Value; 2]>::try_from(list(value, "meter")?).map_err(|_| shape())?;
    let beats = whole(beats, "meter").map_err(|_| shape())?;
    let note = whole(note, "meter").map_err(|_| shape())?;
    if !(1..=32).contains(&beats) {
        return Err(format!("a bar has from 1 to 32 beats, not {beats}"));
    }
    if !matches!(note, 1 | 2 | 4 | 8 | 16 | 32) {
        return Err(format!(
            "a beat is a 1, 2, 4, 8, 16 or 32nd note, not a {note}"
        ));
    }
    Ok([beats, note])
}

fn read_instrument(value: Value) -> Result<Instrument, String> {
    let mut fields = Fields::new(value, "an instrument")?;
    let defaults = Instrument::default();
    let wave = match fields.take("wave") {
        Some(wave) => {
            let name = string(wave, "wave")?;
            Wave::from_name(&name).ok_or_else(|| {
                format!("`wave` must be sine, square, saw, triangle or noise, not {name:?}")
            })?
        }
        None => defaults.wave,
    };
    let mut number_or = |field: &str, default: f32| match fields.take(field) {
        Some(value) => finite(value, field),
        None => Ok(default),
    };
    let duty = number_or("duty", defaults.duty)?;
    let attack = number_or("attack", defaults.attack)?;
    let decay = number_or("decay", defaults.decay)?;
    let sustain = number_or("sustain", defaults.sustain)?;
    let release = number_or("release", defaults.release)?;
    let volume = number_or("volume", defaults.volume)?;
    let pan = number_or("pan", defaults.pan)?;
    let sweep = number_or("sweep", defaults.sweep)?;
    let sweep_time = number_or("sweep_time", defaults.sweep_time)?;
    let vibrato = fields.take("vibrato").map(read_vibrato).transpose()?;
    let filter = fields.take("filter").map(read_filter).transpose()?;
    fields.finish("an instrument")?;
    let instrument = Instrument {
        wave,
        duty,
        attack,
        decay,
        sustain,
        release,
        volume,
        pan,
        sweep,
        sweep_time,
        vibrato,
        filter,
    };
    match instrument.problem() {
        Some(problem) => Err(problem),
        None => Ok(instrument),
    }
}

fn read_vibrato(value: Value) -> Result<Vibrato, String> {
    let mut fields = Fields::new(value, "`vibrato`")?;
    let mut required = |field: &str| match fields.take(field) {
        Some(value) => finite(value, &format!("vibrato.{field}")),
        None => Err(format!("`vibrato` has a `{field}`")),
    };
    let rate = required("rate")?;
    let depth = required("depth")?;
    let delay = match fields.take("delay") {
        Some(value) => finite(value, "vibrato.delay")?,
        None => 0.0,
    };
    fields.finish("`vibrato`")?;
    Ok(Vibrato { rate, depth, delay })
}

fn read_filter(value: Value) -> Result<Filter, String> {
    let mut fields = Fields::new(value, "`filter`")?;
    let kind = match fields.take("kind") {
        Some(kind) => {
            let name = string(kind, "filter.kind")?;
            FilterKind::from_name(&name)
                .ok_or_else(|| format!("`filter.kind` must be low, high or band, not {name:?}"))?
        }
        None => FilterKind::Low,
    };
    let cutoff = match fields.take("cutoff") {
        Some(value) => finite(value, "filter.cutoff")?,
        None => return Err("`filter` has a `cutoff`".to_owned()),
    };
    let mut number_or_0 = |field: &str| match fields.take(field) {
        Some(value) => finite(value, &format!("filter.{field}")),
        None => Ok(0.0),
    };
    let resonance = number_or_0("resonance")?;
    let sweep = number_or_0("sweep")?;
    let sweep_time = number_or_0("sweep_time")?;
    fields.finish("`filter`")?;
    Ok(Filter {
        kind,
        cutoff,
        resonance,
        sweep,
        sweep_time,
    })
}

fn read_tracks(value: Value) -> Result<Vec<TrackDef>, String> {
    let items = list(value, "tracks").map_err(|err| format!("the song: {err}"))?;
    if items.len() > MAX_TRACKS {
        return Err(format!(
            "the song has {} tracks, more than the {MAX_TRACKS} a song may have",
            items.len()
        ));
    }
    let mut tracks: Vec<TrackDef> = Vec::with_capacity(items.len());
    for (i, item) in items.into_iter().enumerate() {
        let track = read_track(item).map_err(|err| format!("track {}: {err}", i + 1))?;
        if tracks.iter().any(|other| other.name == track.name) {
            return Err(format!("two tracks are named `{}`", track.name));
        }
        tracks.push(track);
    }
    Ok(tracks)
}

fn read_track(value: Value) -> Result<TrackDef, String> {
    let mut fields = Fields::new(value, "a track")?;
    let name = match fields.take("name") {
        Some(name) => string(name, "name")?,
        None => return Err("a track has a `name`".to_owned()),
    };
    if name.is_empty() {
        return Err("a track's `name` can't be empty".to_owned());
    }
    let instrument = match fields.take("instrument") {
        Some(instrument) => string(instrument, "instrument")?,
        None => return Err(format!("track `{name}` has an `instrument`")),
    };
    let at_track = |err: String| format!("track `{name}`: {err}");
    let volume = match fields.take("volume") {
        Some(volume) => finite(volume, "volume").map_err(at_track)?,
        None => 1.0,
    };
    if volume < 0.0 {
        return Err(at_track(format!(
            "`volume` must be 0 or more, not {volume}"
        )));
    }
    let pan = match fields.take("pan") {
        Some(pan) => finite(pan, "pan").map_err(at_track)?,
        None => 0.0,
    };
    if !(-1.0..=1.0).contains(&pan) {
        return Err(at_track(format!("`pan` must be from -1 to 1, not {pan}")));
    }
    fields.finish("a track").map_err(at_track)?;
    Ok(TrackDef {
        name,
        instrument,
        volume,
        pan,
    })
}

fn read_bars(fields: &mut Fields, what: &str) -> Result<u32, String> {
    let bars = match fields.take("bars") {
        Some(bars) => positive_whole(bars, "bars")?,
        None => return Err(format!("{what} has `bars`, how many bars it lasts")),
    };
    if bars > MAX_BARS {
        return Err(format!("`bars` must be at most {MAX_BARS}, not {bars}"));
    }
    Ok(bars)
}

fn read_pattern(value: Value, beats_per_bar: u32) -> Result<PatternDef, String> {
    let mut fields = Fields::new(value, "a pattern")?;
    let bars = read_bars(&mut fields, "a pattern")?;
    let beats = f64::from(bars * beats_per_bar);
    let mut notes = match fields.take("notes") {
        Some(notes) => list(notes, "notes")?
            .into_iter()
            .enumerate()
            .map(|(i, note)| read_note(note, beats).map_err(|err| format!("note {}: {err}", i + 1)))
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };
    fields.finish("a pattern")?;
    sort_notes(&mut notes);
    if let Some(pair) = notes
        .windows(2)
        .find(|pair| pair[0].beat == pair[1].beat && pair[0].pitch == pair[1].pitch)
    {
        return Err(format!(
            "it has two notes of {} at beat {}",
            pair[0].pitch, pair[0].beat
        ));
    }
    Ok(PatternDef { bars, notes })
}

/// Sorts `notes` by their beats, then their pitches, as a pattern keeps them.
pub(crate) fn sort_notes(notes: &mut [Note]) {
    notes.sort_by(|a, b| a.beat.total_cmp(&b.beat).then(a.pitch.cmp(&b.pitch)));
}

fn read_note(value: Value, beats: f64) -> Result<Note, String> {
    let shape = || "a note must be { beat, pitch, beats } or { beat, pitch, beats, velocity }";
    let items = match value {
        Value::List(items) if (3..=4).contains(&items.len()) => items,
        _ => return Err(shape().to_owned()),
    };
    let mut items = items.into_iter();
    let mut next = || items.next();
    let beat = number(next().unwrap(), "beat").map_err(|_| shape().to_owned())?;
    if !(0.0..beats).contains(&beat) {
        return Err(format!(
            "its beat must be from 0 up to the pattern's {beats} beats, not {beat}"
        ));
    }
    let pitch = match next().unwrap() {
        Value::String(name) => Pitch::from_name(&name).ok_or_else(|| {
            format!("its pitch must be a note from C0 to B8, such as \"C#4\", not {name:?}")
        })?,
        value => {
            return Err(format!(
                "its pitch must be a note's name, such as \"C#4\", not {}",
                describe_value(&value)
            ));
        }
    };
    let length = number(next().unwrap(), "beats").map_err(|_| shape().to_owned())?;
    if !(length > 0.0 && length <= f64::from(MAX_BARS * 32)) {
        return Err(format!(
            "it must last above 0 beats and at most {}, not {length}",
            MAX_BARS * 32
        ));
    }
    let velocity = match next() {
        Some(velocity) => {
            let velocity = finite(velocity, "velocity").map_err(|_| shape().to_owned())?;
            if !(0.0..=1.0).contains(&velocity) {
                return Err(format!("its velocity must be from 0 to 1, not {velocity}"));
            }
            velocity
        }
        None => 1.0,
    };
    Ok(Note {
        beat,
        pitch,
        length,
        velocity,
    })
}

fn read_section(value: Value, beats_per_bar: u32) -> Result<SectionDef, String> {
    let mut fields = Fields::new(value, "a section")?;
    let bars = read_bars(&mut fields, "a section")?;
    let beats = f64::from(bars * beats_per_bar);
    let tracks = match fields.take("tracks") {
        Some(tracks) => Fields::new(tracks, "`tracks`")?
            .fields
            .into_iter()
            .map(|(track, pattern)| {
                let pattern = string(pattern, &format!("tracks.{track}"))?;
                Ok((track, pattern))
            })
            .collect::<Result<_, String>>()?,
        None => BTreeMap::new(),
    };
    let next = fields
        .take("next")
        .map(|next| string(next, "next"))
        .transpose()?;
    let mut markers = match fields.take("markers") {
        Some(markers) => list(markers, "markers")?
            .into_iter()
            .enumerate()
            .map(|(i, marker)| {
                read_marker(marker, beats).map_err(|err| format!("marker {}: {err}", i + 1))
            })
            .collect::<Result<Vec<_>, _>>()?,
        None => Vec::new(),
    };
    fields.finish("a section")?;
    markers.sort_by(|a, b| a.beat.total_cmp(&b.beat).then_with(|| a.name.cmp(&b.name)));
    Ok(SectionDef {
        bars,
        tracks,
        next,
        markers,
    })
}

fn read_marker(value: Value, beats: f64) -> Result<Marker, String> {
    let shape = || "a marker must be { beat, name }".to_owned();
    let [beat, name] = match value {
        Value::List(items) => <[Value; 2]>::try_from(items).map_err(|_| shape())?,
        _ => return Err(shape()),
    };
    let beat = number(beat, "beat").map_err(|_| shape())?;
    if !(0.0..beats).contains(&beat) {
        return Err(format!(
            "its beat must be from 0 up to the section's {beats} beats, not {beat}"
        ));
    }
    let name = string(name, "name").map_err(|_| shape())?;
    if name.is_empty() {
        return Err("its name can't be empty".to_owned());
    }
    Ok(Marker { beat, name })
}

/// `song` as the text of a song file, or why it can't be written: only finite numbers can.
fn write_song(song: &SongFile) -> Result<String, String> {
    let mut writer = Writer::default();
    writer.out.push_str("return ");
    writer.open();
    writer.field("format", &SONG_FORMAT.to_string());
    if song.tempo != DEFAULT_TEMPO {
        writer.field("tempo", &double(song.tempo, "tempo")?);
    }
    if song.meter != DEFAULT_METER {
        let [beats, note] = song.meter;
        writer.field("meter", &format!("{{ {beats}, {note} }}"));
    }
    writer.field("start", &quoted(&song.start));
    if !song.instruments.is_empty() {
        writer.key("instruments");
        writer.open();
        for (name, instrument) in &song.instruments {
            writer.key(name);
            write_instrument(&mut writer, instrument)
                .map_err(|err| format!("instrument `{name}`: {err}"))?;
            writer.end_field();
        }
        writer.close();
        writer.end_field();
    }
    if !song.tracks.is_empty() {
        writer.key("tracks");
        writer.open();
        for track in &song.tracks {
            write_track(&mut writer, track)
                .map_err(|err| format!("track `{}`: {err}", track.name))?;
        }
        writer.close();
        writer.end_field();
    }
    if !song.patterns.is_empty() {
        writer.key("patterns");
        writer.open();
        for (name, pattern) in &song.patterns {
            writer.key(name);
            write_pattern(&mut writer, pattern)
                .map_err(|err| format!("pattern `{name}`: {err}"))?;
            writer.end_field();
        }
        writer.close();
        writer.end_field();
    }
    if !song.sections.is_empty() {
        writer.key("sections");
        writer.open();
        for (name, section) in &song.sections {
            writer.key(name);
            write_section(&mut writer, section)
                .map_err(|err| format!("section `{name}`: {err}"))?;
            writer.end_field();
        }
        writer.close();
        writer.end_field();
    }
    writer.close();
    writer.out.push('\n');
    Ok(writer.out)
}

fn write_instrument(writer: &mut Writer, instrument: &Instrument) -> Result<(), String> {
    let defaults = Instrument::default();
    if *instrument == defaults {
        writer.out.push_str("{}");
        return Ok(());
    }
    writer.open();
    if instrument.wave != defaults.wave {
        writer.field("wave", &quoted(instrument.wave.name()));
    }
    let numbers = [
        ("duty", instrument.duty, defaults.duty),
        ("attack", instrument.attack, defaults.attack),
        ("decay", instrument.decay, defaults.decay),
        ("sustain", instrument.sustain, defaults.sustain),
        ("release", instrument.release, defaults.release),
        ("volume", instrument.volume, defaults.volume),
        ("pan", instrument.pan, defaults.pan),
        ("sweep", instrument.sweep, defaults.sweep),
        ("sweep_time", instrument.sweep_time, defaults.sweep_time),
    ];
    for (field, value, default) in numbers {
        if value != default {
            writer.number_field(field, value)?;
        }
    }
    if let Some(Vibrato { rate, depth, delay }) = instrument.vibrato {
        writer.key("vibrato");
        writer.open();
        writer.number_field("rate", rate)?;
        writer.number_field("depth", depth)?;
        if delay != 0.0 {
            writer.number_field("delay", delay)?;
        }
        writer.close();
        writer.end_field();
    }
    if let Some(filter) = instrument.filter {
        writer.key("filter");
        writer.open();
        if filter.kind != FilterKind::Low {
            writer.field("kind", &quoted(filter.kind.name()));
        }
        writer.number_field("cutoff", filter.cutoff)?;
        for (field, value) in [
            ("resonance", filter.resonance),
            ("sweep", filter.sweep),
            ("sweep_time", filter.sweep_time),
        ] {
            if value != 0.0 {
                writer.number_field(field, value)?;
            }
        }
        writer.close();
        writer.end_field();
    }
    writer.close();
    Ok(())
}

fn write_track(writer: &mut Writer, track: &TrackDef) -> Result<(), String> {
    writer.indent();
    writer.open();
    writer.field("name", &quoted(&track.name));
    writer.field("instrument", &quoted(&track.instrument));
    if track.volume != 1.0 {
        writer.number_field("volume", track.volume)?;
    }
    if track.pan != 0.0 {
        writer.number_field("pan", track.pan)?;
    }
    writer.close();
    writer.end_field();
    Ok(())
}

fn write_pattern(writer: &mut Writer, pattern: &PatternDef) -> Result<(), String> {
    writer.open();
    writer.field("bars", &pattern.bars.to_string());
    if !pattern.notes.is_empty() {
        writer.key("notes");
        writer.open();
        for (i, note) in pattern.notes.iter().enumerate() {
            let at_note = |err: String| format!("note {}: {err}", i + 1);
            writer.indent();
            write!(
                writer.out,
                "{{ {}, {}, {}",
                double(note.beat, "beat").map_err(at_note)?,
                quoted(&note.pitch.to_string()),
                double(note.length, "beats").map_err(at_note)?,
            )
            .unwrap();
            if note.velocity != 1.0 {
                write!(
                    writer.out,
                    ", {}",
                    single(note.velocity, "velocity").map_err(at_note)?
                )
                .unwrap();
            }
            writer.out.push_str(" }");
            writer.end_field();
        }
        writer.close();
        writer.end_field();
    }
    writer.close();
    Ok(())
}

fn write_section(writer: &mut Writer, section: &SectionDef) -> Result<(), String> {
    writer.open();
    writer.field("bars", &section.bars.to_string());
    if !section.tracks.is_empty() {
        writer.key("tracks");
        writer.open();
        for (track, pattern) in &section.tracks {
            writer.field(track, &quoted(pattern));
        }
        writer.close();
        writer.end_field();
    }
    if let Some(next) = &section.next {
        writer.field("next", &quoted(next));
    }
    if !section.markers.is_empty() {
        writer.key("markers");
        writer.open();
        for (i, marker) in section.markers.iter().enumerate() {
            let beat =
                double(marker.beat, "beat").map_err(|err| format!("marker {}: {err}", i + 1))?;
            writer.indent();
            write!(writer.out, "{{ {beat}, {} }}", quoted(&marker.name)).unwrap();
            writer.end_field();
        }
        writer.close();
        writer.end_field();
    }
    writer.close();
    Ok(())
}

/// `value` written so Luau reads it back as the same `f64`, if it's finite.
fn double(value: f64, field: &str) -> Result<String, String> {
    if value.is_finite() {
        Ok(format!("{value}"))
    } else {
        Err(format!(
            "`{field}` is {value}, which can't be written: song files hold only finite numbers"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A song in its canonical layout, with every field.
    const CANONICAL: &str = r#"return {
    format = 1,
    tempo = 128,
    meter = { 6, 8 },
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
                sweep = 2,
                sweep_time = 0.12,
            },
        },
        hat = {
            wave = "noise",
            decay = 0.05,
            sustain = 0,
            volume = 0.4,
            filter = {
                kind = "high",
                cutoff = 7000,
            },
        },
        kick = {
            decay = 0.3,
            sustain = 0,
            sweep = 36,
            sweep_time = 0.06,
        },
        lead = {
            wave = "square",
            duty = 0.25,
            attack = 0.01,
            pan = -0.25,
            vibrato = {
                rate = 6,
                depth = 0.2,
                delay = 0.3,
            },
        },
        sub = {},
    },
    tracks = {
        {
            name = "drums",
            instrument = "kick",
        },
        {
            name = "bass",
            instrument = "bass",
            volume = 0.8,
            pan = 0.1,
        },
        {
            name = "lead line",
            instrument = "lead",
            volume = 0,
        },
    },
    patterns = {
        beat = {
            bars = 1,
            notes = {
                { 0, "C2", 0.5 },
                { 3, "C2", 0.5 },
            },
        },
        empty = {
            bars = 2,
        },
        tune = {
            bars = 2,
            notes = {
                { 0, "A4", 1.5, 0.8 },
                { 0, "C#5", 1.5, 0.8 },
                { 1.5, "Bb4", 0.25 },
                { 10.75, "G3", 4 },
            },
        },
    },
    sections = {
        intro = {
            bars = 2,
            tracks = {
                drums = "beat",
            },
            next = "verse",
        },
        verse = {
            bars = 8,
            tracks = {
                bass = "beat",
                drums = "beat",
                ["lead line"] = "tune",
            },
            next = "verse",
            markers = {
                { 0, "start" },
                { 24, "drop" },
            },
        },
    },
}
"#;

    fn parse(source: &str) -> Result<SongFile> {
        SongFile::parse(source, "test.song.luau")
    }

    fn problem(source: &str) -> String {
        parse(source).unwrap_err().to_string()
    }

    /// A song file with `fields` among its own.
    fn song_with(fields: &str) -> String {
        format!("return {{ format = 1, start = \"main\", {fields} }}")
    }

    #[test]
    fn writing_a_canonical_file_gives_it_back() {
        let song = parse(CANONICAL).unwrap();
        // Flats are written as sharps, so the canonical file has none.
        let canonical = CANONICAL.replace("\"Bb4\"", "\"A#4\"");
        assert_eq!(song.to_luau().unwrap(), canonical);
        assert_eq!(parse(&canonical).unwrap(), song);
    }

    #[test]
    fn songs_hold_what_their_files_say() {
        let song = parse(CANONICAL).unwrap();
        assert_eq!(song.tempo, 128.0);
        assert_eq!(song.meter, [6, 8]);
        assert_eq!(song.beats_per_bar(), 6);
        assert_eq!(song.start, "intro");
        assert_eq!(song.instruments["sub"], Instrument::default());
        assert_eq!(
            song.instruments["hat"].filter.unwrap().kind,
            FilterKind::High
        );
        assert_eq!(song.tracks[2].name, "lead line");
        assert_eq!(song.tracks[2].volume, 0.0);
        let tune = &song.patterns["tune"];
        assert_eq!(tune.notes.len(), 4);
        assert_eq!(tune.notes[1].pitch, Pitch::from_name("C#5").unwrap());
        assert_eq!(tune.notes[2].velocity, 1.0);
        assert_eq!(song.sections["verse"].next.as_deref(), Some("verse"));
        assert_eq!(song.sections["intro"].markers, []);
    }

    #[test]
    fn songs_without_tempo_or_meter_have_the_defaults() {
        let song = parse(&song_with("")).unwrap();
        assert_eq!((song.tempo, song.meter), (120.0, [4, 4]));
        assert_eq!(
            song.to_luau().unwrap(),
            "return {\n    format = 1,\n    start = \"main\",\n}\n"
        );
    }

    #[test]
    fn notes_and_markers_are_kept_in_order() {
        let song = parse(&song_with(
            r#"patterns = { p = { bars = 1, notes = { { 2, "E4", 1 }, { 0, "G4", 1 }, { 0, "C4", 1 } } } },
            sections = { main = { bars = 1, markers = { { 3, "b" }, { 1, "a" } } } }"#,
        ))
        .unwrap();
        let pitches: Vec<String> = song.patterns["p"]
            .notes
            .iter()
            .map(|note| note.pitch.to_string())
            .collect();
        assert_eq!(pitches, ["C4", "G4", "E4"]);
        let names: Vec<&str> = song.sections["main"]
            .markers
            .iter()
            .map(|marker| marker.name.as_str())
            .collect();
        assert_eq!(names, ["a", "b"]);
    }

    #[test]
    fn formats_are_checked() {
        assert!(problem("return { start = \"main\" }").contains("has no `format`"));
        assert!(problem("return { format = 2, start = \"main\" }").contains("newer than"));
        assert!(problem("return { format = 0, start = \"main\" }").contains("no song format 0"));
        assert!(problem("return { format = 1 }").contains("has no `start`"));
        assert!(problem("return 3").contains("must return a table"));
    }

    #[test]
    fn errors_name_where_they_are() {
        let cases = [
            (
                "tempo = 0",
                "the song: `tempo` must be from 1 to 999 beats a minute, not 0",
            ),
            (
                "meter = { 4 }",
                "the song: `meter` must be two whole numbers",
            ),
            (
                "meter = { 4, 3 }",
                "the song: a beat is a 1, 2, 4, 8, 16 or 32nd note, not a 3",
            ),
            ("meter = { 33, 4 }", "a bar has from 1 to 32 beats, not 33"),
            ("tune = 3", "the song: a song has no field `tune`"),
            (
                "instruments = { a = { wave = \"organ\" } }",
                "instrument `a`: `wave` must be sine",
            ),
            (
                "instruments = { a = { duty = 0 } }",
                "instrument `a`: `duty` must be above 0",
            ),
            (
                "instruments = { a = { loud = true } }",
                "instrument `a`: an instrument has no field `loud`",
            ),
            (
                "instruments = { a = { vibrato = { rate = 1 } } }",
                "`vibrato` has a `depth`",
            ),
            (
                "instruments = { a = { filter = { } } }",
                "`filter` has a `cutoff`",
            ),
            (
                "instruments = { a = { filter = { cutoff = 1, kind = \"notch\" } } }",
                "`filter.kind` must be low",
            ),
            (
                "tracks = { { instrument = \"a\" } }",
                "track 1: a track has a `name`",
            ),
            (
                "tracks = { { name = \"t\" } }",
                "track 1: track `t` has an `instrument`",
            ),
            (
                "tracks = { { name = \"t\", instrument = \"a\", pan = 2 } }",
                "track 1: track `t`: `pan` must be from -1 to 1",
            ),
            (
                "tracks = { { name = \"t\", instrument = \"a\" }, { name = \"t\", instrument = \"b\" } }",
                "two tracks are named `t`",
            ),
            (
                "patterns = { p = { } }",
                "pattern `p`: a pattern has `bars`",
            ),
            (
                "patterns = { p = { bars = 0 } }",
                "pattern `p`: `bars` must be at least 1",
            ),
            (
                "patterns = { p = { bars = 1001 } }",
                "`bars` must be at most 1000",
            ),
            (
                "patterns = { p = { bars = 1, notes = { { 0, \"C4\" } } } }",
                "pattern `p`: note 1: a note must be",
            ),
            (
                "patterns = { p = { bars = 1, notes = { { 4, \"C4\", 1 } } } }",
                "note 1: its beat must be from 0 up to the pattern's 4 beats, not 4",
            ),
            (
                "patterns = { p = { bars = 1, notes = { { 0, \"H4\", 1 } } } }",
                "note 1: its pitch must be a note from C0 to B8",
            ),
            (
                "patterns = { p = { bars = 1, notes = { { 0, 60, 1 } } } }",
                "its pitch must be a note's name",
            ),
            (
                "patterns = { p = { bars = 1, notes = { { 0, \"C4\", 0 } } } }",
                "note 1: it must last above 0 beats",
            ),
            (
                "patterns = { p = { bars = 1, notes = { { 0, \"C4\", 1, 2 } } } }",
                "note 1: its velocity must be from 0 to 1, not 2",
            ),
            (
                "patterns = { p = { bars = 1, notes = { { 1, \"C4\", 1 }, { 1, \"C4\", 2 } } } }",
                "pattern `p`: it has two notes of C4 at beat 1",
            ),
            (
                "sections = { s = { bars = 1, tracks = { a = 1 } } }",
                "section `s`: `tracks.a` must be a string",
            ),
            (
                "sections = { s = { bars = 1, markers = { { 5, \"x\" } } } }",
                "section `s`: marker 1: its beat must be from 0 up to the section's 4 beats",
            ),
            (
                "sections = { s = { bars = 1, markers = { { 0 } } } }",
                "marker 1: a marker must be { beat, name }",
            ),
            (
                "sections = { s = { bars = 1, next = 1 } }",
                "section `s`: `next` must be a string",
            ),
        ];
        for (fields, expected) in cases {
            let problem = problem(&song_with(fields));
            assert!(problem.contains(expected), "{fields}: {problem}");
            assert!(problem.starts_with("song test.song.luau: "), "{problem}");
        }
    }

    #[test]
    fn numbers_that_cant_be_written_say_where_they_are() {
        let mut song = parse(CANONICAL).unwrap();
        song.patterns.get_mut("tune").unwrap().notes[0].length = f64::INFINITY;
        let err = song.to_luau().unwrap_err().to_string();
        assert!(
            err.contains("pattern `tune`: note 1: `beats` is inf"),
            "{err}"
        );
    }

    #[test]
    fn songs_write_to_files_and_read_back() {
        let dir = std::env::temp_dir().join(format!("wingine-song-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("theme.song.luau");
        let song = parse(CANONICAL).unwrap();
        song.write(&path).unwrap();
        assert_eq!(SongFile::read(&path).unwrap(), song);
        std::fs::remove_dir_all(&dir).unwrap();
        let err = SongFile::read(&path).unwrap_err().to_string();
        assert!(err.contains("theme.song.luau"), "{err}");
    }
}
