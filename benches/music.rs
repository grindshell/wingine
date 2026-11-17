//! What songs cost to play: the bandstand's song, and songs as dense as one can be, 64 notes
//! sounding at once, the most a performance plays. Run with `cargo bench --bench music`.
//!
//! Each case renders a song's section as `Song::render` does, at 48,000 frames a second, which
//! runs the sequencer and the synth that the audio thread runs for a song played live. It reports
//! the median of [`BATCHES`] renders as the time a second of the song takes, which is also the
//! share of a core playing it live takes.

use std::{
    collections::BTreeMap,
    path::Path,
    time::{Duration, Instant},
};

use wingine::music::{
    Filter, FilterKind, Instrument, Note, PatternDef, Pitch, SectionDef, Song, SongFile, TrackDef,
    Vibrato, Wave,
};

const BATCHES: usize = 7;
const RATE: u32 = 48_000;
/// Tracks in a dense song, each playing a chord of [`CHORD`] notes held a bar, so [`VOICES`]
/// sound at once, and as many more ring out as each bar starts.
const TRACKS: usize = 16;
const CHORD: usize = 4;
const VOICES: usize = TRACKS * CHORD;
/// How long a dense song's section lasts, at its tempo of 120.
const BARS: u32 = 4;

fn main() {
    let bandstand =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/bandstand/bandstand.song.luau");
    let written = SongFile::read(&bandstand).unwrap();
    let mut layered = written.clone();
    for track in &mut layered.tracks {
        track.volume = 1.0;
    }

    let saw = Instrument {
        wave: Wave::Saw,
        vibrato: Some(Vibrato {
            rate: 6.0,
            depth: 0.2,
            delay: 0.0,
        }),
        filter: Some(Filter {
            kind: FilterKind::Low,
            cutoff: 1200.0,
            resonance: 0.5,
            sweep: 2.0,
            sweep_time: 0.3,
        }),
        ..held(Wave::Saw)
    };
    let noise = Instrument {
        filter: Some(Filter {
            kind: FilterKind::Band,
            cutoff: 3000.0,
            resonance: 0.3,
            ..Default::default()
        }),
        ..held(Wave::Noise)
    };

    println!("The median of {BATCHES} renders, at {RATE} frames a second:");
    report("bandstand, its chorus as written", &written, "chorus");
    report(
        "bandstand, its chorus with every layer in",
        &layered,
        "chorus",
    );
    for (case, instrument) in [
        (format!("{VOICES} sines"), held(Wave::Sine)),
        (format!("{VOICES} squares"), held(Wave::Square)),
        (format!("{VOICES} triangles"), held(Wave::Triangle)),
        (format!("{VOICES} saws"), held(Wave::Saw)),
        (
            format!("{VOICES} saws, with vibrato, through a swept low-pass"),
            saw,
        ),
        (format!("{VOICES} noises through a band-pass"), noise),
    ] {
        report(&case, &dense(instrument), "loop");
    }
    if cfg!(debug_assertions) {
        println!("(a debug build: run `cargo bench` for one built for speed)");
    }
}

/// Renders `file`'s `section` [`BATCHES`] times, after once to warm up, and reports the median.
fn report(case: &str, file: &SongFile, section: &str) {
    let song = Song::new(file).unwrap();
    let render = || {
        let start = Instant::now();
        let rendered = song.render(RATE, Some(section)).unwrap();
        (start.elapsed(), rendered.duration())
    };
    let (_, seconds) = render();
    let mut times: Vec<Duration> = (0..BATCHES).map(|_| render().0).collect();
    times.sort();
    let share = times[BATCHES / 2].as_secs_f64() / seconds;
    println!(
        "  {case:<56}{:>8.2} ms a second, {:>5.2}% of a core",
        share * 1000.0,
        share * 100.0
    );
}

/// An instrument playing `wave`, held while its notes are, quietly enough that a dense song's
/// notes together don't clip.
fn held(wave: Wave) -> Instrument {
    Instrument {
        wave,
        attack: 0.01,
        decay: 0.1,
        sustain: 0.8,
        release: 0.05,
        volume: 1.0 / VOICES as f32,
        ..Default::default()
    }
}

/// A song of [`TRACKS`] tracks of `instrument`, each a chord held a bar, every bar: [`VOICES`]
/// notes held at once, and as each bar starts, as many more starting as the last bar's are let
/// go, so new notes take fading notes' places.
fn dense(instrument: Instrument) -> SongFile {
    let mut file = SongFile {
        start: "loop".to_owned(),
        ..Default::default()
    };
    file.instruments.insert("voice".to_owned(), instrument);
    let mut section = SectionDef::new(BARS);
    section.next = Some("loop".to_owned());
    for track in 0..TRACKS {
        let name = format!("track{track}");
        // Chords of fifths, a few semitones apart from track to track.
        let notes = (0..CHORD)
            .map(|note| Note {
                beat: 0.0,
                pitch: Pitch::new((36 + track * 3 + note * 7) as u8).unwrap(),
                length: 4.0,
                velocity: 1.0,
            })
            .collect();
        file.patterns
            .insert(name.clone(), PatternDef { bars: 1, notes });
        section.tracks.insert(name.clone(), name.clone());
        file.tracks.push(TrackDef::new(name, "voice"));
    }
    file.sections = BTreeMap::from([("loop".to_owned(), section)]);
    file
}
