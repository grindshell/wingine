//! Playing a song in time: walking its sections and their patterns, starting notes on their
//! tracks' instruments and letting them go, and saying what happens as it goes.
//!
//! A [`Sequencer`] renders a song a frame at a time, for baking and for playing live alike. Where
//! everything happens is worked out from the frame its tempo last changed at, and tempo slides and
//! track fades move on every [`CONTROL`] frames from the song's start, so what it renders never
//! depends on how a mixer cuts its buffers. It holds every voice it can play at once from the
//! start, so it allocates nothing as it plays, and can run on the audio thread.

use std::sync::Arc;

use super::{
    MAX_TRACKS, Note, Pitch, SongData,
    synth::{CONTROL, Voice, balance},
};

/// How many notes a song plays at once, at most. Past that, starting a note takes the place of
/// the oldest let go, or else the oldest.
pub(crate) const VOICES: usize = 64;

/// How far apart, in beats, two beats may be and be the same: far less than a frame at any tempo.
const SAME_BEAT: f64 = 1e-9;

/// How far past a frame, in frames, something may be and still happen at it: far more than the
/// rounding of working out where it is, and far less than a frame.
const SAME_FRAME: f64 = 1e-6;

/// The least time a track's volume takes to change, in seconds, so it never clicks.
const MIN_FADE: f32 = 0.005;

/// Something that happens as a song plays.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SongEvent {
    /// The section at this index of [`Song::sections`](super::Song::sections) started.
    Section(usize),
    /// A beat of the section playing started: `bar` from 1 at the section's start, and `beat`
    /// from 1 at the bar's.
    Beat { bar: u32, beat: u32 },
    /// The song reached a marker: the section's index, and the marker's among its markers.
    Marker { section: usize, marker: usize },
    /// A note started on a track listened to.
    Note {
        track: usize,
        pitch: Pitch,
        velocity: f32,
    },
    /// The song ended: its last section is over, and its notes have faded.
    End,
}

/// An event, with the frame it happened at, counted from the song's start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Heard {
    pub frame: u64,
    pub event: SongEvent,
}

/// When a change to a playing song happens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum At {
    /// At once.
    #[default]
    Now,
    /// At the start of the next beat.
    Beat,
    /// At the start of the next bar.
    Bar,
    /// When the section playing ends, in place of what would follow it.
    Section,
}

impl At {
    /// Every time, as games name them.
    pub const ALL: [Self; 4] = [Self::Now, Self::Beat, Self::Bar, Self::Section];

    /// Its name, as Luau gives it: `"now"`, `"beat"`, `"bar"` or `"section"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Now => "now",
            Self::Beat => "beat",
            Self::Bar => "bar",
            Self::Section => "section",
        }
    }

    /// The time named `name`, as [`At::name`] gives it.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|at| at.name() == name)
    }
}

/// A note playing, on a track.
#[derive(Clone, Debug)]
struct Playing {
    voice: Voice,
    track: usize,
    /// The song beat it's let go at.
    release_at: f64,
    /// How many notes started before it, so the oldest can give way.
    order: u64,
}

/// A track's volume, fading toward where it's going.
#[derive(Clone, Copy, Debug)]
struct Fader {
    /// Its volume at the start of this control step.
    volume: f32,
    /// Its volume at the start of the next.
    next: f32,
    target: f32,
    /// How much it moves a frame.
    speed: f32,
}

impl Fader {
    fn at(volume: f32) -> Self {
        Self {
            volume,
            next: volume,
            target: volume,
            speed: 0.0,
        }
    }

    /// Moves it on to the next control step.
    fn step(&mut self) {
        self.volume = self.next;
        let step = self.speed * CONTROL as f32;
        self.next = if (self.target - self.volume).abs() <= step {
            self.target
        } else {
            self.volume + step.copysign(self.target - self.volume)
        };
    }
}

/// A tempo sliding from one to another over some beats.
#[derive(Clone, Copy, Debug)]
struct Ramp {
    from: f64,
    to: f64,
    /// The song beat it starts at.
    start: f64,
    beats: f64,
}

/// A change waiting for its time.
#[derive(Clone, Copy, Debug)]
struct Waiting<T> {
    change: T,
    at: At,
}

/// Plays a song, rendering it a frame at a time.
#[derive(Clone, Debug)]
pub(crate) struct Sequencer {
    song: Arc<SongData>,
    rate: f64,
    /// Frames rendered since the song started.
    frame: u64,
    /// The frame the control step playing started at.
    step_frame: u64,
    /// The frame and song beat the tempo last changed at, and the beats a frame since.
    since_frame: u64,
    since_beat: f64,
    per_frame: f64,
    /// Beats a minute now.
    tempo: f64,
    ramp: Option<Ramp>,
    /// The section playing, and the song beat its beat 0 is at.
    section: usize,
    origin: f64,
    /// Everything up to this song beat has happened.
    done: f64,
    /// A section to start before anything else, and the beat to start it at: the first, or one
    /// jumped to at once.
    arriving: Option<(usize, f64)>,
    /// Whether the song has run out of sections, so it starts no more notes.
    ended: bool,
    /// Whether it has said so, once its notes faded.
    finished: bool,
    jump: Option<Waiting<usize>>,
    tempo_change: Option<Waiting<(f64, f64)>>,
    /// The section that follows each one in place of its `next`, if any.
    next: Option<usize>,
    /// The sections it has played, if it ends rather than play one again, as a bake does.
    once: Option<Vec<bool>>,
    /// The frame it would have played a section again at, and the section, once it would have.
    looped_at: Option<(u64, usize)>,
    voices: [Option<Playing>; VOICES],
    started: u64,
    faders: [Fader; MAX_TRACKS],
    /// Which tracks' notes it tells of, a bit each.
    listening: u32,
    scratch: [[f32; 2]; CONTROL as usize],
}

impl Sequencer {
    /// Starts `song` `beat` beats into the section at `section`, rendering `rate` frames a second.
    pub(crate) fn new(song: Arc<SongData>, rate: u32, section: usize, beat: f64) -> Self {
        let tempo = song.tempo;
        let mut faders = [Fader::at(1.0); MAX_TRACKS];
        for (fader, track) in faders.iter_mut().zip(&song.tracks) {
            *fader = Fader::at(track.volume);
        }
        let beat = start_within(&song, section, beat);
        let rate = f64::from(rate);
        Self {
            song,
            rate,
            frame: 0,
            step_frame: 0,
            since_frame: 0,
            since_beat: 0.0,
            per_frame: tempo / 60.0 / rate,
            tempo,
            ramp: None,
            section,
            origin: 0.0,
            done: 0.0,
            arriving: Some((section, beat)),
            ended: false,
            finished: false,
            jump: None,
            tempo_change: None,
            next: None,
            once: None,
            looped_at: None,
            voices: std::array::from_fn(|_| None),
            started: 0,
            faders,
            listening: 0,
            scratch: [[0.0; 2]; CONTROL as usize],
        }
    }

    /// Makes it end rather than play a section it has played already, as a bake renders a song
    /// once through: what's playing fades as it would, but no more notes start.
    pub(crate) fn play_once(&mut self) {
        self.once = Some(vec![false; self.song.sections.len()]);
    }

    /// The frame it would have played a section again at, and the section, once it would have, if
    /// it plays once.
    pub(crate) fn looped_at(&self) -> Option<(u64, usize)> {
        self.looped_at
    }

    /// Has the section at `section` follow every one in place of its `next`, as a bake of one
    /// section loops it.
    pub(crate) fn set_next(&mut self, section: Option<usize>) {
        self.next = section;
    }

    /// Whether its last section is over and its notes have faded.
    pub(crate) fn is_finished(&self) -> bool {
        self.finished
    }

    /// Frames rendered since it started.
    pub(crate) fn frame(&self) -> u64 {
        self.frame
    }

    /// Beats a minute now.
    pub(crate) fn tempo(&self) -> f64 {
        self.tempo
    }

    /// The section playing, and how many beats into it the song is.
    pub(crate) fn position(&self) -> (usize, f64) {
        match self.arriving {
            Some(arriving) => arriving,
            None => (
                self.section,
                (self.beat_at(self.frame) - self.origin).max(0.0),
            ),
        }
    }

    /// Moves to the section at `section`, `at` the time given; a section that's playing starts
    /// again.
    pub(crate) fn go_to(&mut self, section: usize, at: At) {
        if section >= self.song.sections.len() || self.ended {
            return;
        }
        if at == At::Now {
            self.jump = None;
            self.arriving = Some((section, 0.0));
        } else {
            self.jump = Some(Waiting {
                change: section,
                at,
            });
        }
    }

    /// Changes the tempo to `tempo` beats a minute, `at` the time given, sliding there over
    /// `over` beats.
    pub(crate) fn set_tempo(&mut self, tempo: f64, at: At, over: f64) {
        let change = (tempo, over.max(0.0));
        if at == At::Now {
            self.tempo_change = None;
            self.start_ramp(change, self.beat_at(self.frame));
        } else {
            self.tempo_change = Some(Waiting { change, at });
        }
    }

    /// Fades the track at `track` to `volume` over `fade` seconds.
    pub(crate) fn set_track_volume(&mut self, track: usize, volume: f32, fade: f32) {
        if let Some(fader) = self.faders.get_mut(track) {
            let frames = fade.max(MIN_FADE) * self.rate as f32;
            fader.target = volume.max(0.0);
            fader.speed = (fader.target - fader.next).abs() / frames;
        }
    }

    /// Tells, or stops telling, of the notes the track at `track` starts.
    pub(crate) fn listen(&mut self, track: usize, listen: bool) {
        if track < MAX_TRACKS {
            let bit = 1 << track;
            self.listening = if listen {
                self.listening | bit
            } else {
                self.listening & !bit
            };
        }
    }

    /// Adds the next frames of the song to `out`, telling `heard` what happens as it does.
    pub(crate) fn render(&mut self, out: &mut [[f32; 2]], heard: &mut dyn FnMut(Heard)) {
        let mut at = 0;
        while at < out.len() {
            if self.frame == self.step_frame + CONTROL {
                self.next_step();
            }
            let left = (self.step_frame + CONTROL - self.frame) as usize;
            let frames = left.min(out.len() - at);
            self.advance(&mut out[at..at + frames], heard);
            at += frames;
        }
    }

    /// Starts the next control step: slides the tempo on, and moves the faders on.
    fn next_step(&mut self) {
        self.step_frame += CONTROL;
        if let Some(ramp) = self.ramp {
            let progress =
                ((self.beat_at(self.step_frame) - ramp.start) / ramp.beats).clamp(0.0, 1.0);
            if progress >= 1.0 {
                self.ramp = None;
            }
            self.change_tempo(ramp.from + (ramp.to - ramp.from) * progress);
        }
        for fader in &mut self.faders[..self.song.tracks.len()] {
            fader.step();
        }
    }

    /// Plays at `tempo` from the frame playing.
    fn change_tempo(&mut self, tempo: f64) {
        self.since_beat = self.beat_at(self.frame);
        self.since_frame = self.frame;
        self.tempo = tempo;
        self.per_frame = tempo / 60.0 / self.rate;
    }

    /// The song beat at `frame`, at the tempo playing.
    fn beat_at(&self, frame: u64) -> f64 {
        self.since_beat + (frame - self.since_frame) as f64 * self.per_frame
    }

    /// The first frame at or after the song beat `beat`, at the tempo playing.
    fn frame_at(&self, beat: f64) -> u64 {
        let frames = ((beat - self.since_beat) / self.per_frame - SAME_FRAME).ceil();
        self.since_frame + frames.max(0.0) as u64
    }

    /// Renders `out`, part of a control step, with everything that happens in it.
    fn advance(&mut self, out: &mut [[f32; 2]], heard: &mut dyn FnMut(Heard)) {
        let end = self.frame + out.len() as u64;
        if let Some((section, beat)) = self.arriving.take() {
            let now = self.beat_at(self.frame);
            self.done = now;
            self.arrive(section, now, beat, heard);
        }
        let mut at = 0;
        loop {
            if self.ended && !self.finished && self.voices.iter().all(Option::is_none) {
                self.finished = true;
                heard(Heard {
                    frame: self.frame,
                    event: SongEvent::End,
                });
            }
            let next = self
                .next_beat()
                .map(|beat| (beat, self.frame_at(beat).max(self.frame)))
                .filter(|&(_, frame)| frame < end);
            let Some((beat, frame)) = next else {
                self.play(&mut out[at..]);
                self.frame = end;
                return;
            };
            let frames = (frame - self.frame) as usize;
            self.play(&mut out[at..at + frames]);
            at += frames;
            self.frame = frame;
            self.happen(beat, heard);
        }
    }

    /// The song beat of the next thing to happen after [`Sequencer::done`], if anything will.
    fn next_beat(&self) -> Option<f64> {
        let after = self.done;
        let mut next = f64::INFINITY;
        let mut consider = |beat: f64| {
            if beat > after + SAME_BEAT && beat < next {
                next = beat;
            }
        };
        for playing in self.voices.iter().flatten() {
            if !playing.voice.is_released() {
                consider(playing.release_at);
            }
        }
        if !self.ended {
            let section = &self.song.sections[self.section];
            let within = after - self.origin;
            consider(self.origin + section.beats);
            // Every beat, for beat events, and changes waiting for one.
            consider(self.origin + (within + SAME_BEAT).floor() + 1.0);
            let markers = &section.markers;
            let index = markers.partition_point(|marker| marker.beat <= within + SAME_BEAT);
            if let Some(marker) = markers.get(index) {
                consider(self.origin + marker.beat);
            }
            for pattern in section.patterns.iter().flatten() {
                if let Some(beat) = self.next_note(*pattern, within) {
                    consider(self.origin + beat);
                }
            }
        }
        next.is_finite().then_some(next)
    }

    /// The beat in the section playing of the next note the pattern at `pattern` starts after
    /// the section's beat `after`, if it starts one before the section ends.
    fn next_note(&self, pattern: usize, after: f64) -> Option<f64> {
        let section = &self.song.sections[self.section];
        let pattern = &self.song.patterns[pattern];
        let first = pattern.notes.first()?.beat;
        let repeat = ((after + SAME_BEAT) / pattern.beats).floor().max(0.0);
        let within = after - repeat * pattern.beats;
        let index = pattern
            .notes
            .partition_point(|note| note.beat <= within + SAME_BEAT);
        let beat = match pattern.notes.get(index) {
            Some(note) => repeat * pattern.beats + note.beat,
            None => (repeat + 1.0) * pattern.beats + first,
        };
        (beat < section.beats - SAME_BEAT).then_some(beat)
    }

    /// Everything that happens at the song beat `beat`.
    fn happen(&mut self, beat: f64, heard: &mut dyn FnMut(Heard)) {
        self.done = beat;
        for playing in self.voices.iter_mut().flatten() {
            if !playing.voice.is_released() && playing.release_at <= beat + SAME_BEAT {
                playing.voice.release();
            }
        }
        if self.ended {
            return;
        }
        let section = &self.song.sections[self.section];
        let within = beat - self.origin;
        let whole = within.round();
        let on_beat = (within - whole).abs() <= SAME_BEAT;
        let on_bar = on_beat && (whole as u64).is_multiple_of(self.song.beats_per_bar.into());
        if within >= section.beats - SAME_BEAT {
            // The end of a section is the start of a beat and a bar, so any jump happens.
            let next = match self.jump.take() {
                Some(jump) => Some(jump.change),
                None => self.next.or(section.next),
            };
            match next {
                Some(next) => self.arrive(next, beat, 0.0, heard),
                None => self.ended = true,
            }
            return;
        }
        if let Some(Waiting { change, at }) = self.jump
            && ((at == At::Beat && on_beat) || (at == At::Bar && on_bar))
        {
            self.jump = None;
            self.arrive(change, beat, 0.0, heard);
            return;
        }
        self.tempo_waiting(beat, on_beat, on_bar, false);
        self.sound(beat, within, heard);
    }

    /// Starts the section at `section` at the song beat `beat`, `into` beats into it, and what
    /// happens there.
    fn arrive(&mut self, section: usize, beat: f64, into: f64, heard: &mut dyn FnMut(Heard)) {
        if let Some(played) = &mut self.once {
            if played[section] {
                self.ended = true;
                self.looped_at = Some((self.frame, section));
                return;
            }
            played[section] = true;
        }
        self.section = section;
        self.origin = beat - into;
        heard(Heard {
            frame: self.frame,
            event: SongEvent::Section(section),
        });
        self.tempo_waiting(beat, true, true, true);
        self.sound(beat, into, heard);
    }

    /// Starts a tempo change waiting for the song beat `beat`, if it's waiting for a beat and
    /// that's `on_beat`, a bar and that's `on_bar`, or a section and that's `entered` one.
    fn tempo_waiting(&mut self, beat: f64, on_beat: bool, on_bar: bool, entered: bool) {
        if let Some(Waiting { change, at }) = self.tempo_change {
            let now = match at {
                At::Now => true,
                At::Beat => on_beat,
                At::Bar => on_bar,
                At::Section => entered,
            };
            if now {
                self.tempo_change = None;
                self.start_ramp(change, beat);
            }
        }
    }

    /// The markers, beat and notes at the section's beat `within`, the song beat `beat`.
    fn sound(&mut self, beat: f64, within: f64, heard: &mut dyn FnMut(Heard)) {
        let frame = self.frame;
        let song = Arc::clone(&self.song);
        let section = &song.sections[self.section];
        for (index, marker) in section.markers.iter().enumerate() {
            if (marker.beat - within).abs() <= SAME_BEAT {
                heard(Heard {
                    frame,
                    event: SongEvent::Marker {
                        section: self.section,
                        marker: index,
                    },
                });
            }
        }
        let whole = within.round();
        if (within - whole).abs() <= SAME_BEAT {
            let whole = whole as u32;
            let per_bar = song.beats_per_bar;
            heard(Heard {
                frame,
                event: SongEvent::Beat {
                    bar: whole / per_bar + 1,
                    beat: whole % per_bar + 1,
                },
            });
        }
        for (track, pattern) in section.patterns.iter().enumerate() {
            let Some(pattern) = pattern else { continue };
            let pattern = &song.patterns[*pattern];
            let repeat = ((within + SAME_BEAT) / pattern.beats).floor();
            let at = within - repeat * pattern.beats;
            let first = pattern
                .notes
                .partition_point(|note| note.beat < at - SAME_BEAT);
            for note in pattern.notes[first..]
                .iter()
                .take_while(|note| note.beat <= at + SAME_BEAT)
            {
                self.start_note(track, *note, beat, heard);
            }
        }
    }

    fn start_ramp(&mut self, (tempo, over): (f64, f64), beat: f64) {
        let [least, most] = super::TEMPO_RANGE;
        let tempo = tempo.clamp(least, most);
        if over > 0.0 {
            self.ramp = Some(Ramp {
                from: self.tempo,
                to: tempo,
                start: beat,
                beats: over,
            });
        } else {
            self.ramp = None;
            self.change_tempo(tempo);
        }
    }

    fn start_note(&mut self, track: usize, note: Note, beat: f64, heard: &mut dyn FnMut(Heard)) {
        let voice = Voice::new(
            self.song.tracks[track].instrument,
            note.pitch.frequency(),
            note.velocity,
            self.rate as u32,
            self.started as u32,
        );
        let slot = self.free_slot();
        self.voices[slot] = Some(Playing {
            voice,
            track,
            release_at: beat + note.length,
            order: self.started,
        });
        self.started += 1;
        if self.listening & (1 << track) != 0 {
            heard(Heard {
                frame: self.frame,
                event: SongEvent::Note {
                    track,
                    pitch: note.pitch,
                    velocity: note.velocity,
                },
            });
        }
    }

    /// The slot for a new voice: an empty one, or the oldest let go, or the oldest.
    fn free_slot(&self) -> usize {
        if let Some(empty) = self.voices.iter().position(Option::is_none) {
            return empty;
        }
        let oldest = |released: bool| {
            self.voices
                .iter()
                .enumerate()
                .filter_map(|(slot, playing)| Some((slot, playing.as_ref()?)))
                .filter(|(_, playing)| !released || playing.voice.is_released())
                .min_by_key(|(_, playing)| playing.order)
                .map(|(slot, _)| slot)
        };
        oldest(true).or_else(|| oldest(false)).unwrap_or(0)
    }

    /// Renders every note playing into `out`, at its track's volume and pan.
    fn play(&mut self, out: &mut [[f32; 2]]) {
        if out.is_empty() {
            return;
        }
        let offset = (self.frame - self.step_frame) as f32;
        for slot in &mut self.voices {
            let Some(playing) = slot else { continue };
            let scratch = &mut self.scratch[..out.len()];
            scratch.fill([0.0; 2]);
            playing.voice.render(scratch);
            let fader = self.faders[playing.track];
            let [left, right] = balance(self.song.tracks[playing.track].pan);
            let change = (fader.next - fader.volume) / CONTROL as f32;
            for (i, (frame, sample)) in out.iter_mut().zip(scratch.iter()).enumerate() {
                let volume = fader.volume + change * (offset + i as f32);
                frame[0] += sample[0] * volume * left;
                frame[1] += sample[1] * volume * right;
            }
            if playing.voice.is_finished() {
                *slot = None;
            }
        }
    }
}

/// `beat` as a beat to start the section at `section` at: within it, from 0 up to its end.
fn start_within(song: &SongData, section: usize, beat: f64) -> f64 {
    let beats = song.sections[section].beats;
    if beat > 0.0 {
        beat.min(beats - 1e-6)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::{Song, SongFile};

    const RATE: u32 = 48_000;

    /// A beat at 120 beats a minute, in frames.
    const BEAT: u64 = RATE as u64 / 2;

    fn song(source: &str) -> Song {
        Song::new(&SongFile::parse(source, "test.song.luau").unwrap()).unwrap()
    }

    /// A song at 120 beats a minute, so a beat is half a second.
    const SONG: &str = r#"return {
        format = 1,
        start = "a",
        instruments = { click = { decay = 0.01, sustain = 0 } },
        tracks = { { name = "t", instrument = "click" } },
        patterns = { p = { bars = 1, notes = { { 0, "C4", 0.5 }, { 2.5, "E4", 0.5 } } } },
        sections = {
            a = { bars = 2, tracks = { t = "p" }, next = "b", markers = { { 5, "five" } } },
            b = { bars = 1 },
        },
    }"#;

    /// Plays `sequencer` for `seconds` in pieces of `piece` frames, giving what it rendered and
    /// what happened.
    fn run(sequencer: &mut Sequencer, seconds: f64, piece: usize) -> (Vec<[f32; 2]>, Vec<Heard>) {
        let mut out = vec![[0.0; 2]; (seconds * f64::from(RATE)) as usize];
        let mut heard = Vec::new();
        for chunk in out.chunks_mut(piece) {
            sequencer.render(chunk, &mut |event| heard.push(event));
        }
        (out, heard)
    }

    fn sequencer(song: &Song) -> Sequencer {
        let mut sequencer = Sequencer::new(Arc::clone(song.data()), RATE, song.data().start, 0.0);
        sequencer.listen(0, true);
        sequencer
    }

    /// The sections that started, with the frames they started at.
    fn sections(heard: &[Heard]) -> Vec<(usize, u64)> {
        heard
            .iter()
            .filter_map(|heard| match heard.event {
                SongEvent::Section(section) => Some((section, heard.frame)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn events_happen_on_their_frames() {
        let song = song(SONG);
        let (_, heard) = run(&mut sequencer(&song), 7.0, 128);
        let at = |frame: u64, event| Heard { frame, event };
        let beat = |bar, beat| SongEvent::Beat { bar, beat };
        let note = |name| SongEvent::Note {
            track: 0,
            pitch: Pitch::from_name(name).unwrap(),
            velocity: 1.0,
        };
        let expected = vec![
            at(0, SongEvent::Section(0)),
            at(0, beat(1, 1)),
            at(0, note("C4")),
            at(BEAT, beat(1, 2)),
            at(2 * BEAT, beat(1, 3)),
            at(5 * BEAT / 2, note("E4")),
            at(3 * BEAT, beat(1, 4)),
            at(4 * BEAT, beat(2, 1)),
            at(4 * BEAT, note("C4")),
            at(
                5 * BEAT,
                SongEvent::Marker {
                    section: 0,
                    marker: 0,
                },
            ),
            at(5 * BEAT, beat(2, 2)),
            at(6 * BEAT, beat(2, 3)),
            at(13 * BEAT / 2, note("E4")),
            at(7 * BEAT, beat(2, 4)),
            at(8 * BEAT, SongEvent::Section(1)),
            at(8 * BEAT, beat(1, 1)),
            at(9 * BEAT, beat(1, 2)),
            at(10 * BEAT, beat(1, 3)),
            at(11 * BEAT, beat(1, 4)),
            // The song ends with its last section, its last note long faded.
            at(12 * BEAT, SongEvent::End),
        ];
        assert_eq!(heard, expected);
        assert!(sequencer(&song).position() == (0, 0.0));
    }

    #[test]
    fn rendering_in_any_pieces_renders_the_same() {
        let song = song(SONG);
        let whole = run(&mut sequencer(&song), 2.0, 96_000);
        for piece in [1, 7, 128, 1000] {
            assert_eq!(
                run(&mut sequencer(&song), 2.0, piece),
                whole,
                "in pieces of {piece}"
            );
        }
        assert!(whole.0.iter().any(|frame| frame[0] != 0.0));
    }

    #[test]
    fn notes_ring_until_let_go() {
        let held = song(
            &SONG
                .replace("{ 0, \"C4\", 0.5 }", "{ 0, \"C4\", 1.5 }")
                .replace(
                    "click = { decay = 0.01, sustain = 0 }",
                    "click = { release = 0.01 }",
                ),
        );
        let (out, _) = run(&mut sequencer(&held), 1.0, 128);
        let loud = |frame: usize| out[frame][0] != 0.0;
        // Held for a beat and a half, 0.75 seconds, then a hundredth of a second fading.
        assert!(loud(36_000 - 100));
        assert!((36_000 + 600..36_000 + 2000).all(|frame| !loud(frame)));
    }

    #[test]
    fn jumps_wait_for_their_beat_or_bar() {
        let song = song(SONG);
        let sections_at = |at: At| {
            let mut sequencer = sequencer(&song);
            let mut heard = Vec::new();
            let mut out = vec![[0.0; 2]; 128];
            // A third of a beat in, then on.
            while sequencer.frame() < BEAT / 3 {
                sequencer.render(&mut out, &mut |event| heard.push(event));
            }
            sequencer.go_to(1, at);
            for _ in 0..(6 * BEAT / 128) {
                sequencer.render(&mut out, &mut |event| heard.push(event));
            }
            sections(&heard)
        };
        assert_eq!(sections_at(At::Beat), [(0, 0), (1, BEAT)]);
        assert_eq!(sections_at(At::Bar), [(0, 0), (1, 4 * BEAT)]);
        // At once: the next frame the sequencer renders.
        let now = sections_at(At::Now);
        assert_eq!(now.len(), 2);
        assert!((BEAT / 3..BEAT / 3 + 128).contains(&now[1].1), "{now:?}");
    }

    #[test]
    fn jumps_at_a_sections_end_replace_what_follows() {
        let song = song(&SONG.replace("b = { bars = 1 }", "b = { bars = 1 }, c = { bars = 1 }"));
        let mut sequencer = sequencer(&song);
        sequencer.go_to(2, At::Section);
        let (_, heard) = run(&mut sequencer, 5.0, 128);
        assert_eq!(sections(&heard), [(0, 0), (2, 8 * BEAT)]);
    }

    #[test]
    fn sections_loop_and_tempos_change() {
        let looping = song(&SONG.replace("b = { bars = 1 }", "b = { bars = 1, next = \"b\" }"));
        let mut sequencer = sequencer(&looping);
        let mut heard = Vec::new();
        let mut out = [[0.0; 2]; 128];
        // A third of a beat in, the tempo doubles at the next bar.
        while sequencer.frame() < BEAT / 3 {
            sequencer.render(&mut out, &mut |event| heard.push(event));
        }
        sequencer.set_tempo(240.0, At::Bar, 0.0);
        while sequencer.frame() < 5 * u64::from(RATE) + 1000 {
            sequencer.render(&mut out, &mut |event| heard.push(event));
        }
        // The first bar at 120, then twice as fast, so section b loops every second.
        let fast = BEAT / 2;
        assert_eq!(
            sections(&heard),
            [
                (0, 0),
                (1, 4 * BEAT + 4 * fast),
                (1, 4 * BEAT + 8 * fast),
                (1, 4 * BEAT + 12 * fast)
            ]
        );
        assert_eq!(sequencer.tempo(), 240.0);

        // Waiting for a bar before the song starts, the first bar is the next.
        let mut sequencer = self::sequencer(&looping);
        sequencer.set_tempo(240.0, At::Bar, 0.0);
        let (_, heard) = run(&mut sequencer, 3.1, 128);
        assert_eq!(sections(&heard), [(0, 0), (1, 8 * fast), (1, 12 * fast)]);
    }

    #[test]
    fn tempos_slide_over_beats() {
        let song = song(SONG);
        let mut sequencer = sequencer(&song);
        sequencer.set_tempo(60.0, At::Now, 4.0);
        let (_, heard) = run(&mut sequencer, 6.0, 128);
        let beats: Vec<u64> = heard
            .iter()
            .filter(|heard| matches!(heard.event, SongEvent::Beat { .. }))
            .map(|heard| heard.frame)
            .collect();
        // Each beat longer than the last over the first four, then a second each.
        let lengths: Vec<u64> = beats.windows(2).map(|pair| pair[1] - pair[0]).collect();
        assert!(
            lengths[..4].windows(2).all(|pair| pair[1] > pair[0]),
            "{lengths:?}"
        );
        assert!(
            lengths[4..]
                .iter()
                .all(|&length| length.abs_diff(u64::from(RATE)) <= 1),
            "{lengths:?}"
        );
    }

    #[test]
    fn tracks_fade() {
        let song = song(
            &SONG
                .replace("click = { decay = 0.01, sustain = 0 }", "click = {}")
                .replace("{ 0, \"C4\", 0.5 }", "{ 0, \"C4\", 4 }"),
        );
        let mut sequencer = sequencer(&song);
        sequencer.set_track_volume(0, 0.0, 0.5);
        let (out, _) = run(&mut sequencer, 1.0, 128);
        let peak = |from: usize, to: usize| {
            out[from..to]
                .iter()
                .fold(0.0_f32, |peak, frame| peak.max(frame[0].abs()))
        };
        assert!(peak(0, 2400) > 0.9);
        let half = peak(11_000, 13_000);
        assert!((0.45..0.55).contains(&half), "{half}");
        assert_eq!(peak(24_100, 48_000), 0.0);
    }

    #[test]
    fn playing_once_ends_where_it_would_loop() {
        let looping = song(&SONG.replace("b = { bars = 1 }", "b = { bars = 1, next = \"a\" }"));
        let mut sequencer = sequencer(&looping);
        sequencer.play_once();
        let (_, heard) = run(&mut sequencer, 8.0, 128);
        assert_eq!(sequencer.looped_at(), Some((12 * BEAT, 0)));
        assert!(sequencer.is_finished());
        let end = heard.last().unwrap();
        assert_eq!((end.frame, end.event), (12 * BEAT, SongEvent::End));
        assert_eq!(sections(&heard), [(0, 0), (1, 8 * BEAT)]);
    }

    #[test]
    fn songs_start_where_they_are_asked_to() {
        let song = song(SONG);
        let mut sequencer = Sequencer::new(Arc::clone(song.data()), RATE, 0, 5.0);
        assert_eq!(sequencer.position(), (0, 5.0));
        let (_, heard) = run(&mut sequencer, 0.1, 128);
        assert_eq!(
            heard.iter().map(|heard| heard.event).collect::<Vec<_>>(),
            [
                SongEvent::Section(0),
                SongEvent::Marker {
                    section: 0,
                    marker: 0
                },
                SongEvent::Beat { bar: 2, beat: 2 },
            ]
        );
        let (section, beat) = sequencer.position();
        assert_eq!(section, 0);
        assert!((beat - 5.2).abs() < 1e-9, "{beat}");
    }

    #[test]
    fn too_many_notes_take_the_oldest_ones_places() {
        let notes: Vec<String> = (0..80)
            .map(|i| format!("{{ {}, \"C4\", 100 }}", f64::from(i) / 100.0))
            .collect();
        let song = song(&format!(
            r#"return {{
                format = 1,
                start = "a",
                instruments = {{ sine = {{}} }},
                tracks = {{ {{ name = "t", instrument = "sine" }} }},
                patterns = {{ p = {{ bars = 1, notes = {{ {} }} }} }},
                sections = {{ a = {{ bars = 1, tracks = {{ t = "p" }} }} }},
            }}"#,
            notes.join(", ")
        ));
        let mut sequencer = sequencer(&song);
        run(&mut sequencer, 1.0, 128);
        let playing: Vec<u64> = sequencer.voices.iter().flatten().map(|p| p.order).collect();
        assert_eq!(playing.len(), VOICES);
        assert_eq!(playing.iter().min(), Some(&(80 - VOICES as u64)));
    }
}
