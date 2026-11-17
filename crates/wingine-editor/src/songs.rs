//! Pure operations on song files: adding, renaming and removing their instruments, tracks,
//! patterns and sections, keeping every name that refers to them in step, and placing notes.
//!
//! Each fails, changing nothing, rather than leave the file naming something it doesn't have,
//! except where a name is given up on purpose: removing a pattern or track takes it out of every
//! section that played it.

use std::collections::BTreeMap;

use wingine::music::{
    DEFAULT_METER, DEFAULT_TEMPO, Instrument, MAX_BARS, MAX_TRACKS, Note, PatternDef, Pitch,
    SectionDef, SongFile, TrackDef, Wave,
};

/// How near two beats must be to be the same beat.
pub const SAME_BEAT: f64 = 1e-6;

/// A new song: a section of four bars, `main`, that loops, with a track playing a square-wave
/// lead in a pattern of a bar with nothing in it yet.
pub fn new_song() -> SongFile {
    let lead = Instrument {
        wave: Wave::Square,
        decay: 0.2,
        sustain: 0.6,
        release: 0.1,
        volume: 0.4,
        ..Instrument::default()
    };
    let mut section = SectionDef::new(4);
    section.tracks.insert("lead".into(), "lead".into());
    section.next = Some("main".into());
    SongFile {
        tempo: DEFAULT_TEMPO,
        meter: DEFAULT_METER,
        start: "main".into(),
        instruments: BTreeMap::from([("lead".into(), lead)]),
        tracks: vec![TrackDef::new("lead", "lead")],
        patterns: BTreeMap::from([(
            "lead".into(),
            PatternDef {
                bars: 1,
                notes: Vec::new(),
            },
        )]),
        sections: BTreeMap::from([("main".into(), section)]),
    }
}

/// `base`, or `base` with the first number after it that `taken` doesn't take.
pub fn unique_name(base: &str, taken: impl Fn(&str) -> bool) -> String {
    if !taken(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base}{n}"))
        .find(|name| !taken(name))
        .expect("some number is free")
}

/// Fails unless `name` can name something new among those `taken` takes.
fn check_new_name(name: &str, what: &str, taken: impl Fn(&str) -> bool) -> Result<(), String> {
    if name.trim().is_empty() {
        Err(format!("A {what} needs a name."))
    } else if taken(name) {
        Err(format!("There's a {what} named {name} already."))
    } else {
        Ok(())
    }
}

/// Adds an instrument named for `base`, as `instrument`, and returns its name.
pub fn add_instrument(song: &mut SongFile, base: &str, instrument: Instrument) -> String {
    let name = unique_name(base, |name| song.instruments.contains_key(name));
    song.instruments.insert(name.clone(), instrument);
    name
}

/// Renames the instrument `from` to `to`, and the tracks that play it with it.
pub fn rename_instrument(song: &mut SongFile, from: &str, to: &str) -> Result<(), String> {
    if from == to {
        return Ok(());
    }
    check_new_name(to, "instrument", |name| song.instruments.contains_key(name))?;
    let instrument = song
        .instruments
        .remove(from)
        .ok_or_else(|| format!("There's no instrument named {from}."))?;
    song.instruments.insert(to.to_owned(), instrument);
    for track in &mut song.tracks {
        if track.instrument == from {
            to.clone_into(&mut track.instrument);
        }
    }
    Ok(())
}

/// Removes the instrument `name`, unless a track plays it.
pub fn remove_instrument(song: &mut SongFile, name: &str) -> Result<(), String> {
    if let Some(track) = song.tracks.iter().find(|track| track.instrument == name) {
        return Err(format!(
            "Track {} plays {name}: give it another instrument first.",
            track.name
        ));
    }
    song.instruments.remove(name);
    Ok(())
}

/// Adds a track named for `base`, playing `instrument`, at the end, and returns its name.
pub fn add_track(song: &mut SongFile, base: &str, instrument: &str) -> Result<String, String> {
    if song.tracks.len() >= MAX_TRACKS {
        return Err(format!("A song has at most {MAX_TRACKS} tracks."));
    }
    let name = unique_name(base, |name| song.tracks.iter().any(|t| t.name == name));
    song.tracks.push(TrackDef::new(name.clone(), instrument));
    Ok(name)
}

/// Renames the track `from` to `to`, and its place in every section with it.
pub fn rename_track(song: &mut SongFile, from: &str, to: &str) -> Result<(), String> {
    if from == to {
        return Ok(());
    }
    check_new_name(to, "track", |name| {
        song.tracks.iter().any(|t| t.name == name)
    })?;
    let track = song
        .tracks
        .iter_mut()
        .find(|track| track.name == from)
        .ok_or_else(|| format!("There's no track named {from}."))?;
    to.clone_into(&mut track.name);
    for section in song.sections.values_mut() {
        if let Some(pattern) = section.tracks.remove(from) {
            section.tracks.insert(to.to_owned(), pattern);
        }
    }
    Ok(())
}

/// Removes the track `name`, and its place in every section.
pub fn remove_track(song: &mut SongFile, name: &str) {
    song.tracks.retain(|track| track.name != name);
    for section in song.sections.values_mut() {
        section.tracks.remove(name);
    }
}

/// Moves the track `name` up a place (`-1`) or down one (`1`), if it can go.
pub fn move_track(song: &mut SongFile, name: &str, by: isize) {
    let Some(index) = song.tracks.iter().position(|track| track.name == name) else {
        return;
    };
    let Some(to) = index
        .checked_add_signed(by)
        .filter(|to| *to < song.tracks.len())
    else {
        return;
    };
    song.tracks.swap(index, to);
}

/// Adds an empty pattern of `bars` named for `base`, and returns its name.
pub fn add_pattern(song: &mut SongFile, base: &str, bars: u32) -> String {
    let name = unique_name(base, |name| song.patterns.contains_key(name));
    song.patterns.insert(
        name.clone(),
        PatternDef {
            bars: bars.clamp(1, MAX_BARS),
            notes: Vec::new(),
        },
    );
    name
}

/// Adds a copy of the pattern `name`, and returns the copy's name.
pub fn duplicate_pattern(song: &mut SongFile, name: &str) -> Option<String> {
    let pattern = song.patterns.get(name)?.clone();
    let copy = unique_name(name, |name| song.patterns.contains_key(name));
    song.patterns.insert(copy.clone(), pattern);
    Some(copy)
}

/// Renames the pattern `from` to `to`, and every section's track that plays it with it.
pub fn rename_pattern(song: &mut SongFile, from: &str, to: &str) -> Result<(), String> {
    if from == to {
        return Ok(());
    }
    check_new_name(to, "pattern", |name| song.patterns.contains_key(name))?;
    let pattern = song
        .patterns
        .remove(from)
        .ok_or_else(|| format!("There's no pattern named {from}."))?;
    song.patterns.insert(to.to_owned(), pattern);
    for section in song.sections.values_mut() {
        for pattern in section.tracks.values_mut() {
            if pattern == from {
                to.clone_into(pattern);
            }
        }
    }
    Ok(())
}

/// Removes the pattern `name`, and every section's track that played it is left silent.
pub fn remove_pattern(song: &mut SongFile, name: &str) {
    song.patterns.remove(name);
    for section in song.sections.values_mut() {
        section.tracks.retain(|_, pattern| pattern != name);
    }
}

/// Sets the pattern the track `track` plays in the section `section`, or none.
pub fn set_cell(song: &mut SongFile, section: &str, track: &str, pattern: Option<&str>) {
    let Some(section) = song.sections.get_mut(section) else {
        return;
    };
    match pattern {
        Some(pattern) => {
            section.tracks.insert(track.to_owned(), pattern.to_owned());
        }
        None => {
            section.tracks.remove(track);
        }
    }
}

/// Adds a section of `bars` with nothing in it, named for `base`, and returns its name. It
/// follows the last section in the song's order, which ended the song or looped, and loops itself
/// as that one did.
pub fn add_section(song: &mut SongFile, base: &str, bars: u32) -> String {
    let name = unique_name(base, |name| song.sections.contains_key(name));
    let mut section = SectionDef::new(bars.clamp(1, MAX_BARS));
    let last = section_order(song).last().cloned();
    if let Some(last) = last.and_then(|last| song.sections.get_mut(&last)) {
        let looped = last.next.take();
        last.next = Some(name.clone());
        section.next = looped.map(|_| name.clone());
    }
    if song.sections.is_empty() {
        name.clone_into(&mut song.start);
    }
    song.sections.insert(name.clone(), section);
    name
}

/// Renames the section `from` to `to`, and every `next` naming it and the song's start with it.
pub fn rename_section(song: &mut SongFile, from: &str, to: &str) -> Result<(), String> {
    if from == to {
        return Ok(());
    }
    check_new_name(to, "section", |name| song.sections.contains_key(name))?;
    let section = song
        .sections
        .remove(from)
        .ok_or_else(|| format!("There's no section named {from}."))?;
    song.sections.insert(to.to_owned(), section);
    for section in song.sections.values_mut() {
        if section.next.as_deref() == Some(from) {
            section.next = Some(to.to_owned());
        }
    }
    if song.start == from {
        to.clone_into(&mut song.start);
    }
    Ok(())
}

/// Removes the section `name`, unless the song starts with it. Sections that went on to it go on
/// to what it went on to instead, unless that was itself, when they end the song.
pub fn remove_section(song: &mut SongFile, name: &str) -> Result<(), String> {
    if song.start == name {
        return Err(format!(
            "The song starts with {name}: start it with another section first."
        ));
    }
    let Some(removed) = song.sections.remove(name) else {
        return Ok(());
    };
    let onward = removed.next.filter(|next| next != name);
    for section in song.sections.values_mut() {
        if section.next.as_deref() == Some(name) {
            section.next.clone_from(&onward);
        }
    }
    Ok(())
}

/// The song's sections in the order they play: from its start, along each `next`, then those it
/// never reaches, by name.
pub fn section_order(song: &SongFile) -> Vec<String> {
    let mut order: Vec<String> = Vec::new();
    let mut at = Some(song.start.clone());
    while let Some(name) = at {
        if order.contains(&name) || !song.sections.contains_key(&name) {
            break;
        }
        at = song.sections[&name].next.clone();
        order.push(name);
    }
    for name in song.sections.keys() {
        if !order.contains(name) {
            order.push(name.clone());
        }
    }
    order
}

/// How many beats the pattern `pattern` lasts.
pub fn pattern_beats(song: &SongFile, pattern: &PatternDef) -> f64 {
    f64::from(pattern.bars * song.beats_per_bar())
}

/// Where the note of `pitch` starting on `beat` is in `pattern`'s notes, if it has one.
pub fn find_note(pattern: &PatternDef, beat: f64, pitch: Pitch) -> Option<usize> {
    pattern
        .notes
        .iter()
        .position(|note| note.pitch == pitch && (note.beat - beat).abs() < SAME_BEAT)
}

/// Adds `note` to `pattern`, which lasts `beats`, in order. Fails for a note outside the pattern,
/// or where it has one of that pitch already.
pub fn add_note(pattern: &mut PatternDef, beats: f64, note: Note) -> Result<(), String> {
    if !(0.0..beats).contains(&note.beat) || note.length <= 0.0 {
        return Err("Notes go inside their pattern.".into());
    }
    if find_note(pattern, note.beat, note.pitch).is_some() {
        return Err(format!("There's a {} on that beat already.", note.pitch));
    }
    let index = pattern
        .notes
        .partition_point(|other| (other.beat, other.pitch) < (note.beat, note.pitch));
    pattern.notes.insert(index, note);
    Ok(())
}

/// Removes the note of `pitch` starting on `beat` from `pattern`, if it has one.
pub fn remove_note(pattern: &mut PatternDef, beat: f64, pitch: Pitch) -> Option<Note> {
    let index = find_note(pattern, beat, pitch)?;
    Some(pattern.notes.remove(index))
}

/// Moves the note of `pitch` starting on `beat` to start on `to_beat` at `to_pitch`, as long as
/// it's held. Fails, changing nothing, for a place outside the pattern, which lasts `beats`, or
/// one with a note of that pitch already.
pub fn move_note(
    pattern: &mut PatternDef,
    beats: f64,
    (beat, pitch): (f64, Pitch),
    (to_beat, to_pitch): (f64, Pitch),
) -> Result<(), String> {
    let note = remove_note(pattern, beat, pitch).ok_or("There's no note there.")?;
    let moved = Note {
        beat: to_beat,
        pitch: to_pitch,
        ..note
    };
    add_note(pattern, beats, moved).inspect_err(|_| {
        add_note(pattern, beats, note).expect("a note goes back where it was");
    })
}

/// Holds the note of `pitch` starting on `beat` for `length` beats, at least `least`.
pub fn resize_note(pattern: &mut PatternDef, (beat, pitch): (f64, Pitch), length: f64, least: f64) {
    if let Some(index) = find_note(pattern, beat, pitch) {
        pattern.notes[index].length = length.max(least);
    }
}

/// Changes the pattern `pattern`'s length to `bars`, leaving out the notes that would start past
/// its end.
pub fn set_pattern_bars(song: &mut SongFile, pattern: &str, bars: u32) {
    let per_bar = song.beats_per_bar();
    if let Some(pattern) = song.patterns.get_mut(pattern) {
        pattern.bars = bars.clamp(1, MAX_BARS);
        let beats = f64::from(pattern.bars * per_bar);
        pattern.notes.retain(|note| note.beat < beats);
    }
}

/// Changes the section `section`'s length to `bars`, leaving out the markers past its end.
pub fn set_section_bars(song: &mut SongFile, section: &str, bars: u32) {
    let per_bar = song.beats_per_bar();
    if let Some(section) = song.sections.get_mut(section) {
        section.bars = bars.clamp(1, MAX_BARS);
        let beats = f64::from(section.bars * per_bar);
        section.markers.retain(|marker| marker.beat < beats);
    }
}

/// Changes how many beats a bar has, leaving out notes and markers that would start past the end
/// of their patterns and sections, which last as many bars as they did.
pub fn set_beats_per_bar(song: &mut SongFile, beats: u32) {
    song.meter[0] = beats.clamp(1, 32);
    let patterns: Vec<String> = song.patterns.keys().cloned().collect();
    for pattern in patterns {
        let bars = song.patterns[&pattern].bars;
        set_pattern_bars(song, &pattern, bars);
    }
    let sections: Vec<String> = song.sections.keys().cloned().collect();
    for section in sections {
        let bars = song.sections[&section].bars;
        set_section_bars(song, &section, bars);
    }
}

/// `beat` rounded down to a whole number of `step`s, tidied of float noise.
pub fn snap(beat: f64, step: f64) -> f64 {
    let steps = (beat / step + SAME_BEAT).floor();
    tidy(steps * step)
}

/// `beat` rounded to the nearest of `step`s, tidied of float noise.
pub fn snap_nearest(beat: f64, step: f64) -> f64 {
    tidy((beat / step).round() * step)
}

/// `beat` rounded to a millionth, so a third of a beat written and read is the same.
pub fn tidy(beat: f64) -> f64 {
    (beat * 1e6).round() / 1e6
}

#[cfg(test)]
mod tests {
    use super::*;
    use wingine::music::{Marker, Song};

    fn pitch(name: &str) -> Pitch {
        Pitch::from_name(name).unwrap()
    }

    fn note(beat: f64, name: &str) -> Note {
        Note {
            beat,
            pitch: pitch(name),
            length: 1.0,
            velocity: 1.0,
        }
    }

    /// A song with two sections and two tracks, that plays.
    fn song() -> SongFile {
        let mut song = new_song();
        add_instrument(&mut song, "bass", Instrument::default());
        add_track(&mut song, "bass", "bass").unwrap();
        add_pattern(&mut song, "line", 2);
        let chorus = add_section(&mut song, "chorus", 8);
        set_cell(&mut song, &chorus, "bass", Some("line"));
        song
    }

    #[test]
    fn new_songs_play() {
        let song = new_song();
        assert!(Song::new(&song).is_ok());
        assert_eq!(song.to_luau().unwrap(), {
            let text = song.to_luau().unwrap();
            SongFile::parse(&text, "new.song.luau")
                .unwrap()
                .to_luau()
                .unwrap()
        });
    }

    #[test]
    fn new_sections_follow_the_last() {
        let song = song();
        // `main` looped, so `chorus` follows it and loops in its place.
        assert_eq!(song.sections["main"].next.as_deref(), Some("chorus"));
        assert_eq!(song.sections["chorus"].next.as_deref(), Some("chorus"));
        assert_eq!(section_order(&song), ["main", "chorus"]);
        assert!(Song::new(&song).is_ok());
        let mut ending = new_song();
        ending.sections.get_mut("main").unwrap().next = None;
        add_section(&mut ending, "outro", 2);
        assert_eq!(ending.sections["main"].next.as_deref(), Some("outro"));
        assert_eq!(ending.sections["outro"].next, None);
        let mut empty = new_song();
        empty.sections.clear();
        assert_eq!(add_section(&mut empty, "first", 4), "first");
        assert_eq!(empty.start, "first");
    }

    #[test]
    fn renaming_carries_names_along() {
        let mut song = song();
        rename_instrument(&mut song, "bass", "sub").unwrap();
        assert_eq!(song.tracks[1].instrument, "sub");
        rename_track(&mut song, "bass", "low").unwrap();
        assert_eq!(song.sections["chorus"].tracks["low"], "line");
        rename_pattern(&mut song, "line", "riff").unwrap();
        assert_eq!(song.sections["chorus"].tracks["low"], "riff");
        rename_section(&mut song, "main", "verse").unwrap();
        assert_eq!(song.start, "verse");
        rename_section(&mut song, "chorus", "hook").unwrap();
        assert_eq!(song.sections["verse"].next.as_deref(), Some("hook"));
        assert_eq!(song.sections["hook"].next.as_deref(), Some("hook"));
        assert!(Song::new(&song).is_ok());

        assert_eq!(
            rename_pattern(&mut song, "riff", "lead").unwrap_err(),
            "There's a pattern named lead already."
        );
        assert!(rename_track(&mut song, "low", " ").is_err());
        assert!(rename_section(&mut song, "nothing", "else").is_err());
    }

    #[test]
    fn removing_leaves_nothing_named() {
        let mut song = song();
        assert!(
            remove_instrument(&mut song, "bass")
                .unwrap_err()
                .contains("Track bass")
        );
        remove_pattern(&mut song, "line");
        assert!(song.sections["chorus"].tracks.is_empty());
        remove_track(&mut song, "lead");
        assert!(song.sections["main"].tracks.is_empty());
        remove_instrument(&mut song, "lead").unwrap();
        assert!(!song.instruments.contains_key("lead"));
        assert!(
            remove_section(&mut song, "main")
                .unwrap_err()
                .contains("starts with")
        );
        remove_section(&mut song, "chorus").unwrap();
        // `main` went on to `chorus`, which looped: now the song ends with it.
        assert_eq!(song.sections["main"].next, None);
        assert!(Song::new(&song).is_ok());
    }

    #[test]
    fn tracks_move_and_are_limited() {
        let mut song = song();
        move_track(&mut song, "bass", -1);
        assert_eq!(song.tracks[0].name, "bass");
        move_track(&mut song, "bass", -1);
        assert_eq!(song.tracks[0].name, "bass", "it's first already");
        while song.tracks.len() < MAX_TRACKS {
            add_track(&mut song, "more", "lead").unwrap();
        }
        assert!(add_track(&mut song, "more", "lead").is_err());
        assert_eq!(unique_name("more", |name| name == "more"), "more2");
    }

    #[test]
    fn notes_keep_their_order_and_places() {
        let mut song = new_song();
        let pattern = song.patterns.get_mut("lead").unwrap();
        add_note(pattern, 4.0, note(2.0, "C4")).unwrap();
        add_note(pattern, 4.0, note(0.0, "E4")).unwrap();
        add_note(pattern, 4.0, note(0.0, "C4")).unwrap();
        let names: Vec<String> = pattern.notes.iter().map(|n| n.pitch.to_string()).collect();
        assert_eq!(names, ["C4", "E4", "C4"]);
        assert!(add_note(pattern, 4.0, note(0.0, "C4")).is_err());
        assert!(add_note(pattern, 4.0, note(4.0, "C4")).is_err());

        move_note(pattern, 4.0, (2.0, pitch("C4")), (3.0, pitch("G4"))).unwrap();
        assert_eq!(find_note(pattern, 3.0, pitch("G4")), Some(2));
        // Onto a note of the same pitch, or past the end: it stays put.
        assert!(move_note(pattern, 4.0, (3.0, pitch("G4")), (0.0, pitch("C4"))).is_err());
        assert!(move_note(pattern, 4.0, (3.0, pitch("G4")), (4.5, pitch("G4"))).is_err());
        assert_eq!(find_note(pattern, 3.0, pitch("G4")), Some(2));

        resize_note(pattern, (3.0, pitch("G4")), 0.0, 0.25);
        assert_eq!(pattern.notes[2].length, 0.25);
        assert!(remove_note(pattern, 0.0, pitch("E4")).is_some());
        assert_eq!(pattern.notes.len(), 2);
        assert!(Song::new(&song).is_ok());
    }

    #[test]
    fn shorter_bars_leave_out_what_no_longer_fits() {
        let mut song = new_song();
        set_pattern_bars(&mut song, "lead", 2);
        let pattern = song.patterns.get_mut("lead").unwrap();
        add_note(pattern, 8.0, note(1.0, "C4")).unwrap();
        add_note(pattern, 8.0, note(5.0, "C4")).unwrap();
        song.sections.get_mut("main").unwrap().markers.push(Marker {
            beat: 13.0,
            name: "late".into(),
        });
        set_beats_per_bar(&mut song, 3);
        assert_eq!(
            song.patterns["lead"].notes.len(),
            2,
            "6 beats still hold beat 5"
        );
        set_pattern_bars(&mut song, "lead", 1);
        assert_eq!(song.patterns["lead"].notes.len(), 1);
        assert!(
            song.sections["main"].markers.is_empty(),
            "12 beats don't hold beat 13"
        );
        set_section_bars(&mut song, "main", 0);
        assert_eq!(song.sections["main"].bars, 1);
        assert!(Song::new(&song).is_ok());
    }

    #[test]
    fn beats_snap_to_steps() {
        assert_eq!(snap(1.37, 0.25), 1.25);
        assert_eq!(snap(1.0, 0.25), 1.0);
        assert_eq!(snap(0.9999999999, 0.25), 1.0);
        assert_eq!(snap_nearest(1.4, 0.5), 1.5);
        assert_eq!(snap(2.0, 1.0 / 3.0), 2.0);
        assert_eq!(tidy(1.0 / 3.0), 0.333333);
    }
}
