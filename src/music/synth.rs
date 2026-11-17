//! Instruments, Wingine's own sound generators, and the voices that play their notes.
//!
//! An [`Instrument`] says what a note sounds like: the [`Wave`] its oscillator plays, an envelope
//! that shapes its volume (attack, decay, sustain and release), a sweep and vibrato that move its
//! pitch, and a [`Filter`] that shapes its tone. A [`Pitch`] is a note on the keyboard, from C0 to
//! B8, tuned to A4 at 440 Hz.
//!
//! A note is rendered a frame at a time by a [`Voice`]. It's worked out with `libm`, never the
//! platform's math library, so a note renders to the same bits on every platform; and its pitch
//! and filter change every [`CONTROL`] frames from its start, so how a mixer cuts its buffers
//! never changes what it renders.

use std::{
    f32::consts::{PI, TAU},
    fmt,
};

/// How many frames a voice renders between working out its pitch and filter again.
pub(crate) const CONTROL: u64 = 32;

/// The least time an attack or release takes, in seconds, so notes never click as they start or
/// end.
pub const MIN_RAMP: f32 = 0.002;

/// The most time an envelope's stage or a sweep takes, in seconds.
pub const MAX_TIME: f32 = 60.0;

/// How many times noise changes its value in each cycle of its note's pitch, so higher notes hiss
/// brighter.
const NOISE_STEPS: f64 = 32.0;

/// What an instrument's oscillator plays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Wave {
    /// A pure tone.
    #[default]
    Sine,
    /// High for the instrument's `duty` of each cycle, and low for the rest: hollow at a duty of
    /// a half, thinner and more nasal toward either end.
    Square,
    /// Rising across each cycle, then dropping: bright and buzzy.
    Saw,
    /// Rising for half of each cycle and falling for the other: soft, between sine and square.
    Triangle,
    /// Random values, changing faster for higher notes: for drums, cymbals and wind.
    Noise,
}

impl Wave {
    /// Every wave, in the order the editor lists them.
    pub const ALL: [Self; 5] = [
        Self::Sine,
        Self::Square,
        Self::Saw,
        Self::Triangle,
        Self::Noise,
    ];

    /// The wave's name in song files, such as `"square"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Sine => "sine",
            Self::Square => "square",
            Self::Saw => "saw",
            Self::Triangle => "triangle",
            Self::Noise => "noise",
        }
    }

    /// The wave named `name`, as [`Wave::name`] gives it.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|wave| wave.name() == name)
    }
}

/// A wobble in an instrument's pitch, as a singer holds a note.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vibrato {
    /// Wobbles a second.
    pub rate: f32,
    /// How far the pitch moves either way, in semitones.
    pub depth: f32,
    /// Seconds into each note before it starts.
    pub delay: f32,
}

impl Default for Vibrato {
    fn default() -> Self {
        Self {
            rate: 5.0,
            depth: 0.25,
            delay: 0.0,
        }
    }
}

/// Which frequencies a [`Filter`] lets through.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FilterKind {
    /// Those below its cutoff: darker and softer.
    #[default]
    Low,
    /// Those above its cutoff: thinner and brighter.
    High,
    /// Those near its cutoff: nasal, like a telephone.
    Band,
}

impl FilterKind {
    /// Every kind, in the order the editor lists them.
    pub const ALL: [Self; 3] = [Self::Low, Self::High, Self::Band];

    /// The kind's name in song files, such as `"low"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::High => "high",
            Self::Band => "band",
        }
    }

    /// The kind named `name`, as [`FilterKind::name`] gives it.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

/// A filter that shapes an instrument's tone, letting some frequencies through and quieting the
/// rest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Filter {
    /// Which frequencies it lets through.
    pub kind: FilterKind,
    /// The frequency, in Hz, where it starts to quiet them.
    pub cutoff: f32,
    /// How much it rings at its cutoff, from 0, not at all, to 1, nearly whistling.
    pub resonance: f32,
    /// Octaves its cutoff starts each note away from `cutoff`, sliding there over `sweep_time`:
    /// above, for a pluck that darkens, or below, for one that opens.
    pub sweep: f32,
    /// Seconds the cutoff takes to slide from its sweep to `cutoff`.
    pub sweep_time: f32,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            kind: FilterKind::Low,
            cutoff: 2000.0,
            resonance: 0.0,
            sweep: 0.0,
            sweep_time: 0.0,
        }
    }
}

/// What an instrument's notes sound like.
///
/// Its envelope's times are seconds, so a note's attack lasts as long whatever the song's tempo,
/// while how long a note is held is the song's to say. Attacks and releases shorter than
/// [`MIN_RAMP`] take that long, so notes never click.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Instrument {
    /// What its oscillator plays.
    pub wave: Wave,
    /// The part of each cycle a square wave is high, above 0 and below 1.
    pub duty: f32,
    /// Seconds a note takes to rise from silence to full volume.
    pub attack: f32,
    /// Seconds a note then takes to fall to its sustain.
    pub decay: f32,
    /// The volume a held note stays at after its decay, from 0 to 1. At 0, a note is over once it
    /// has decayed, however long it's held, as a drum's is.
    pub sustain: f32,
    /// Seconds a note takes to fall to silence once it's let go.
    pub release: f32,
    /// Linear volume: 1 is full.
    pub volume: f32,
    /// Stereo position, from -1 (left) through 0 (center) to 1 (right).
    pub pan: f32,
    /// Semitones each note's pitch starts away from the note, sliding to it over `sweep_time`:
    /// above, for a kick drum's thump or a laser's zap, or below, for a swoop up.
    pub sweep: f32,
    /// Seconds the pitch takes to slide from its sweep to the note.
    pub sweep_time: f32,
    /// A wobble in its pitch, if any.
    pub vibrato: Option<Vibrato>,
    /// A filter shaping its tone, if any.
    pub filter: Option<Filter>,
}

impl Default for Instrument {
    /// A sine wave held at full volume while its note is.
    fn default() -> Self {
        Self {
            wave: Wave::Sine,
            duty: 0.5,
            attack: 0.0,
            decay: 0.0,
            sustain: 1.0,
            release: 0.0,
            volume: 1.0,
            pan: 0.0,
            sweep: 0.0,
            sweep_time: 0.0,
            vibrato: None,
            filter: None,
        }
    }
}

impl Instrument {
    /// What's wrong with the instrument, if anything: every setting must be a number, and each
    /// within its range.
    pub fn problem(&self) -> Option<String> {
        let Self {
            wave: _,
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
        } = *self;
        let mut checks = vec![
            within("duty", duty, Range::Inside(0.0, 1.0)),
            within("attack", attack, Range::Time),
            within("decay", decay, Range::Time),
            within("sustain", sustain, Range::Closed(0.0, 1.0)),
            within("release", release, Range::Time),
            within("volume", volume, Range::From(0.0)),
            within("pan", pan, Range::Closed(-1.0, 1.0)),
            within("sweep", sweep, Range::Any),
            within("sweep_time", sweep_time, Range::Time),
        ];
        if let Some(Vibrato { rate, depth, delay }) = vibrato {
            checks.extend([
                within("vibrato.rate", rate, Range::From(0.0)),
                within("vibrato.depth", depth, Range::Any),
                within("vibrato.delay", delay, Range::Time),
            ]);
        }
        if let Some(Filter {
            kind: _,
            cutoff,
            resonance,
            sweep,
            sweep_time,
        }) = filter
        {
            checks.extend([
                within("filter.cutoff", cutoff, Range::Above(0.0)),
                within("filter.resonance", resonance, Range::Closed(0.0, 1.0)),
                within("filter.sweep", sweep, Range::Any),
                within("filter.sweep_time", sweep_time, Range::Time),
            ]);
        }
        checks.into_iter().flatten().next()
    }

    /// Renders a note of `pitch` held for `held` seconds, then let go, at `velocity` (from 0 to
    /// 1), until it has faded: interleaved stereo samples, `rate` frames a second, as
    /// [`Sound::from_samples`](crate::audio::Sound::from_samples) takes them.
    ///
    /// # Panics
    ///
    /// If the instrument has a [`problem`](Instrument::problem), or `rate` is 0.
    pub fn render_note(&self, pitch: Pitch, held: f32, velocity: f32, rate: u32) -> Vec<f32> {
        if let Some(problem) = self.problem() {
            panic!("the instrument can't play: {problem}");
        }
        assert!(rate > 0, "a note renders at least 1 frame a second");
        let mut voice = Voice::new(
            *self,
            pitch.frequency(),
            velocity,
            rate,
            u32::from(pitch.number()),
        );
        let held = (f64::from(held.clamp(0.0, MAX_TIME)) * f64::from(rate)).round() as usize;
        let mut frames = vec![[0.0; 2]; held];
        voice.render(&mut frames);
        voice.release();
        let mut tail = [[0.0; 2]; CONTROL as usize];
        while !voice.is_finished() {
            tail.fill([0.0; 2]);
            let rendered = voice.render(&mut tail);
            frames.extend_from_slice(&tail[..rendered]);
        }
        frames.into_iter().flatten().collect()
    }
}

/// The range a setting must be in.
#[derive(Clone, Copy)]
enum Range {
    /// Any number.
    Any,
    /// From the first number to the second, both included.
    Closed(f32, f32),
    /// Above the first number and below the second.
    Inside(f32, f32),
    /// The number or more.
    From(f32),
    /// More than the number.
    Above(f32),
    /// Seconds, from 0 to [`MAX_TIME`].
    Time,
}

/// What's wrong with `value` as the setting `name`, if it isn't in `range`.
fn within(name: &str, value: f32, range: Range) -> Option<String> {
    if !value.is_finite() {
        return Some(format!("`{name}` must be a number, not {value}"));
    }
    let wanted = match range {
        Range::Any => return None,
        Range::Closed(least, most) if !(least..=most).contains(&value) => {
            format!("from {least} to {most}")
        }
        Range::Inside(least, most) if !(value > least && value < most) => {
            format!("above {least} and below {most}")
        }
        Range::From(least) if value < least => format!("{least} or more"),
        Range::Above(least) if value <= least => format!("above {least}"),
        Range::Time if !(0.0..=MAX_TIME).contains(&value) => {
            format!("from 0 to {MAX_TIME} seconds")
        }
        _ => return None,
    };
    Some(format!("`{name}` must be {wanted}, not {value}"))
}

/// A note on the keyboard, from C0 to B8, numbered as MIDI numbers them: middle C, C4, is 60, and
/// A4, tuned to 440 Hz, is 69.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Pitch(u8);

/// Names of the notes in an octave, from C, as written.
const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

impl Pitch {
    /// C0, the lowest pitch.
    pub const LOWEST: Self = Self(12);
    /// B8, the highest pitch.
    pub const HIGHEST: Self = Self(119);
    /// A4, tuned to 440 Hz.
    pub const A4: Self = Self(69);

    /// The pitch MIDI numbers `number`, if it's from C0 (12) to B8 (119).
    pub fn new(number: u8) -> Option<Self> {
        (Self::LOWEST.0..=Self::HIGHEST.0)
            .contains(&number)
            .then_some(Self(number))
    }

    /// Its MIDI number.
    pub fn number(self) -> u8 {
        self.0
    }

    /// The pitch named `name`: a letter from A to G, then `#` for sharp or `b` for flat if it is,
    /// then its octave from 0 to 8, such as `"C4"`, `"F#3"` or `"Bb2"`.
    pub fn from_name(name: &str) -> Option<Self> {
        let mut chars = name.chars();
        let letter = chars.next()?;
        let mut semitone: i32 = match letter {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => return None,
        };
        let rest = chars.as_str();
        let octave = if let Some(rest) = rest.strip_prefix('#') {
            semitone += 1;
            rest
        } else if let Some(rest) = rest.strip_prefix('b') {
            semitone -= 1;
            rest
        } else {
            rest
        };
        if octave.len() != 1 {
            return None;
        }
        let octave = octave.parse::<u8>().ok()?;
        let number = (i32::from(octave) + 1) * 12 + semitone;
        Self::new(u8::try_from(number).ok()?)
    }

    /// Its frequency, in Hz.
    pub fn frequency(self) -> f32 {
        440.0 * libm::exp2f((f32::from(self.0) - 69.0) / 12.0)
    }
}

impl fmt::Display for Pitch {
    /// Its name, with a sharp for a black key, such as `"C#4"`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let octave = self.0 / 12 - 1;
        write!(f, "{}{octave}", NOTE_NAMES[usize::from(self.0 % 12)])
    }
}

/// A note an instrument is playing, rendered a frame at a time. It holds nothing it needs to
/// allocate, so the audio thread can start and drop voices.
#[derive(Clone, Debug)]
pub(crate) struct Voice {
    instrument: Instrument,
    /// The note's frequency, before sweeps and vibrato, in Hz.
    frequency: f32,
    /// How loud it is: its velocity times its instrument's volume.
    gain: f32,
    /// Its gains in the left and right speakers.
    gains: [f32; 2],
    /// Frames a second.
    rate: f64,
    /// Frames rendered since it started.
    frames: u64,
    /// The frame it was let go at, if it has been, and its envelope's level then.
    released: Option<(u64, f32)>,
    /// Its envelope's level at the last frame.
    level: f32,
    finished: bool,
    /// How far through its cycle the oscillator is, from 0 up to 1.
    phase: f64,
    /// How far the phase moves each frame, worked out every [`CONTROL`] frames.
    step: f64,
    /// How far noise is toward its next value, from 0 up to 1.
    noise_phase: f64,
    noise: u32,
    noise_value: f32,
    filter: FilterState,
}

impl Voice {
    /// A note of `frequency` on `instrument`, at `velocity`, rendered `rate` frames a second. A
    /// noise wave's randomness starts from `seed`.
    pub(crate) fn new(
        instrument: Instrument,
        frequency: f32,
        velocity: f32,
        rate: u32,
        seed: u32,
    ) -> Self {
        let mut noise = Noise::new(seed);
        let noise_value = noise.next();
        // NaN is silent.
        let velocity = if velocity >= 0.0 {
            velocity.min(1.0)
        } else {
            0.0
        };
        Self {
            instrument,
            frequency,
            gain: velocity * instrument.volume,
            gains: balance(instrument.pan),
            rate: f64::from(rate),
            frames: 0,
            released: None,
            level: 0.0,
            finished: false,
            phase: 0.0,
            step: 0.0,
            noise_phase: 0.0,
            noise: noise.0,
            noise_value,
            filter: FilterState::default(),
        }
    }

    /// Lets the note go, so it fades over its release.
    pub(crate) fn release(&mut self) {
        if self.released.is_none() {
            self.released = Some((self.frames, self.level));
        }
    }

    /// Whether it has been let go.
    pub(crate) fn is_released(&self) -> bool {
        self.released.is_some()
    }

    /// Whether it has faded to silence, and renders nothing more.
    pub(crate) fn is_finished(&self) -> bool {
        self.finished
    }

    /// Adds the next frames of the note to `out`, until it finishes, returning how many it
    /// rendered.
    pub(crate) fn render(&mut self, out: &mut [[f32; 2]]) -> usize {
        for (rendered, frame) in out.iter_mut().enumerate() {
            let Some([left, right]) = self.frame() else {
                return rendered;
            };
            frame[0] += left;
            frame[1] += right;
        }
        out.len()
    }

    /// The next frame of the note, unless it has finished.
    fn frame(&mut self) -> Option<[f32; 2]> {
        if self.finished {
            return None;
        }
        if self.frames.is_multiple_of(CONTROL) {
            self.control();
        }
        self.level = self.envelope();
        if self.finished {
            return None;
        }
        let mut sample = self.oscillate();
        if let Some(filter) = self.instrument.filter {
            sample = self.filter.process(sample, filter.kind);
        }
        self.frames += 1;
        let sample = sample * self.level * self.gain;
        Some([sample * self.gains[0], sample * self.gains[1]])
    }

    /// Seconds since the note started.
    fn time(&self) -> f64 {
        self.frames as f64 / self.rate
    }

    /// Its envelope's level now, from 0 to 1, finishing it once it has faded to nothing.
    fn envelope(&mut self) -> f32 {
        let Instrument {
            attack,
            decay,
            sustain,
            release,
            ..
        } = self.instrument;
        if let Some((at, from)) = self.released {
            let since = ((self.frames - at) as f64 / self.rate) as f32;
            let release = release.max(MIN_RAMP);
            if since >= release {
                self.finished = true;
                return 0.0;
            }
            return from * falling(since / release);
        }
        let time = self.time() as f32;
        let attack = attack.max(MIN_RAMP);
        if time < attack {
            time / attack
        } else if time - attack < decay {
            sustain + (1.0 - sustain) * falling((time - attack) / decay)
        } else {
            if sustain == 0.0 {
                self.finished = true;
            }
            sustain
        }
    }

    /// Works out the oscillator's step and the filter's cutoff for the next [`CONTROL`] frames.
    fn control(&mut self) {
        let time = self.time();
        let Instrument {
            sweep,
            sweep_time,
            vibrato,
            filter,
            ..
        } = self.instrument;
        let mut semitones = sweep * sliding(time, sweep_time);
        if let Some(Vibrato { rate, depth, delay }) = vibrato
            && time > f64::from(delay)
        {
            let cycles = (f64::from(rate) * (time - f64::from(delay))).fract();
            semitones += depth * libm::sinf(TAU * cycles as f32);
        }
        let frequency = self.frequency * libm::exp2f(semitones / 12.0);
        // Above half the rate, a wave would fold back down into one lower.
        self.step = (f64::from(frequency) / self.rate).min(0.5);
        if let Some(filter) = filter {
            let octaves = filter.sweep * sliding(time, filter.sweep_time);
            let cutoff = filter.cutoff * libm::exp2f(octaves);
            self.filter.tune(cutoff, filter.resonance, self.rate as f32);
        }
    }

    /// The oscillator's next sample, from -1 to 1, smoothing the jumps of square and saw waves so
    /// they don't alias into other notes (PolyBLEP).
    fn oscillate(&mut self) -> f32 {
        let phase = self.phase as f32;
        let step = self.step as f32;
        let sample = match self.instrument.wave {
            Wave::Sine => libm::sinf(TAU * phase),
            Wave::Square => {
                let duty = self.instrument.duty;
                let high = if phase < duty { 1.0 } else { -1.0 };
                let fall = (phase - duty + 1.0).fract();
                high + smoothing(phase, step) - smoothing(fall, step)
            }
            Wave::Saw => 2.0 * phase - 1.0 - smoothing(phase, step),
            Wave::Triangle => 1.0 - 4.0 * (phase - 0.5).abs(),
            Wave::Noise => self.noise_value,
        };
        self.phase += self.step;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        if self.instrument.wave == Wave::Noise {
            self.noise_phase += self.step * NOISE_STEPS;
            while self.noise_phase >= 1.0 {
                self.noise_phase -= 1.0;
                let mut noise = Noise(self.noise);
                self.noise_value = noise.next();
                self.noise = noise.0;
            }
        }
        sample
    }
}

/// How far a sweep lasting `seconds` still has to go `time` seconds into a note, from 1 as it
/// starts to 0 once it's over; 0 for one that takes no time.
fn sliding(time: f64, seconds: f32) -> f32 {
    if f64::from(seconds) > time {
        (1.0 - time / f64::from(seconds)) as f32
    } else {
        0.0
    }
}

/// A fall from 1 to 0 as `progress` goes from 0 to 1, quick at first and gentle at the end, as
/// sounds fade.
fn falling(progress: f32) -> f32 {
    let left = 1.0 - progress.clamp(0.0, 1.0);
    left * left
}

/// The left and right gains of a sound panned to `pan`: both 1 in the middle, and the far side
/// fading out as it moves to one side.
pub(crate) fn balance(pan: f32) -> [f32; 2] {
    [(1.0 - pan).min(1.0), (1.0 + pan).min(1.0)]
}

/// The correction to a wave's jump from -1 to 1 at phase 0, `phase` through its cycle with
/// `step` a frame, which rounds the jump off over the frames either side of it.
fn smoothing(phase: f32, step: f32) -> f32 {
    if phase < step {
        let t = phase / step;
        t + t - t * t - 1.0
    } else if phase > 1.0 - step {
        let t = (phase - 1.0) / step;
        t * t + t + t + 1.0
    } else {
        0.0
    }
}

/// A xorshift generator, the same on every platform, for noise.
struct Noise(u32);

impl Noise {
    fn new(seed: u32) -> Self {
        // Mixed, so seeds near one another start far apart, and never 0, where xorshift stays.
        let mut z = seed.wrapping_add(0x9e37_79b9);
        z = (z ^ (z >> 16)).wrapping_mul(0x85eb_ca6b);
        z = (z ^ (z >> 13)).wrapping_mul(0xc2b2_ae35);
        z ^= z >> 16;
        Self(if z == 0 { 0x9e37_79b9 } else { z })
    }

    /// A number from -1 up to 1.
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1 << 23) as f32 - 1.0
    }
}

/// A state-variable filter, as Zavalishin's topology-preserving transform has it, which stays
/// stable however fast its cutoff moves.
#[derive(Clone, Copy, Debug, Default)]
struct FilterState {
    a1: f32,
    a2: f32,
    a3: f32,
    /// How much it damps its ringing: 2 not at all, nearer 0 more.
    k: f32,
    ic1eq: f32,
    ic2eq: f32,
}

impl FilterState {
    /// Tunes the filter to `cutoff` Hz, ringing by `resonance`, at `rate` frames a second.
    fn tune(&mut self, cutoff: f32, resonance: f32, rate: f32) {
        // Within what the rate can hold, where the tangent stays finite.
        let cutoff = cutoff.clamp(10.0, rate * 0.45);
        let g = libm::tanf(PI * cutoff / rate);
        let k = 2.0 - 1.9 * resonance;
        self.a1 = 1.0 / (1.0 + g * (g + k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
        self.k = k;
    }

    /// Filters the next sample.
    fn process(&mut self, v0: f32, kind: FilterKind) -> f32 {
        let v3 = v0 - self.ic2eq;
        let v1 = self.a1 * self.ic1eq + self.a2 * v3;
        let v2 = self.ic2eq + self.a2 * self.ic1eq + self.a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;
        match kind {
            FilterKind::Low => v2,
            FilterKind::Band => v1,
            FilterKind::High => v0 - self.k * v1 - v2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    fn pitch(name: &str) -> Pitch {
        Pitch::from_name(name).unwrap()
    }

    /// The left channel of interleaved stereo samples.
    fn left(samples: &[f32]) -> Vec<f32> {
        samples.iter().step_by(2).copied().collect()
    }

    /// How many times `samples` rise through 0.
    fn rises(samples: &[f32]) -> usize {
        samples
            .windows(2)
            .filter(|pair| pair[0] < 0.0 && pair[1] >= 0.0)
            .count()
    }

    fn loudness(samples: &[f32]) -> f32 {
        (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn pitches_are_named_and_tuned() {
        assert_eq!(Pitch::A4.frequency(), 440.0);
        assert!((pitch("C4").frequency() - 261.626).abs() < 0.01);
        assert_eq!(pitch("C4").number(), 60);
        assert_eq!(pitch("Bb2"), pitch("A#2"));
        assert_eq!(pitch("Cb4"), pitch("B3"));
        assert_eq!(pitch("C0"), Pitch::LOWEST);
        assert_eq!(pitch("B8"), Pitch::HIGHEST);
        for number in Pitch::LOWEST.0..=Pitch::HIGHEST.0 {
            let pitch = Pitch::new(number).unwrap();
            assert_eq!(Pitch::from_name(&pitch.to_string()), Some(pitch));
        }
        assert_eq!(pitch("F#3").to_string(), "F#3");
        for name in ["", "H4", "C", "C9", "Cb0", "C#", "c4", "C44", "C-1", "C 4"] {
            assert_eq!(Pitch::from_name(name), None, "{name:?}");
        }
        assert_eq!(Pitch::new(11), None);
        assert_eq!(Pitch::new(120), None);
    }

    #[test]
    fn notes_play_their_pitch() {
        for wave in [Wave::Sine, Wave::Square, Wave::Saw, Wave::Triangle] {
            let instrument = Instrument {
                wave,
                ..Default::default()
            };
            let samples = left(&instrument.render_note(Pitch::A4, 1.0, 1.0, RATE));
            let cycles = rises(&samples[..RATE as usize]);
            assert!((439..=441).contains(&cycles), "{wave:?}: {cycles}");
        }
    }

    #[test]
    fn envelopes_rise_fall_and_fade() {
        let instrument = Instrument {
            attack: 0.1,
            decay: 0.1,
            sustain: 0.5,
            release: 0.2,
            ..Default::default()
        };
        let mut voice = Voice::new(instrument, 100.0, 1.0, 1000, 0);
        let level_at = |voice: &mut Voice, frames: usize| {
            let mut out = vec![[0.0; 2]; frames];
            voice.render(&mut out);
            voice.level
        };
        assert!((level_at(&mut voice, 50) - 0.49).abs() < 1e-4, "half risen");
        // Halfway through its decay, it has fallen three quarters of the way to its sustain.
        assert!((level_at(&mut voice, 100) - 0.625).abs() < 0.01);
        assert_eq!(level_at(&mut voice, 150), 0.5, "sustained");
        voice.release();
        assert!(voice.is_released());
        assert!((level_at(&mut voice, 100) - 0.125).abs() < 0.01);
        assert!(!voice.is_finished());
        level_at(&mut voice, 101);
        assert!(voice.is_finished(), "faded");
        assert_eq!(voice.render(&mut [[0.0; 2]; 4]), 0);
    }

    #[test]
    fn notes_without_sustain_end_once_they_decay() {
        let drum = Instrument {
            decay: 0.1,
            sustain: 0.0,
            ..Default::default()
        };
        let frames = drum.render_note(pitch("C2"), 2.0, 1.0, RATE).len() / 2;
        // Held for two seconds, but silent after its attack and decay.
        assert_eq!(frames, (2.0 * RATE as f32) as usize);
        let mut voice = Voice::new(drum, 65.0, 1.0, RATE, 0);
        let rendered = voice.render(&mut vec![[0.0; 2]; RATE as usize]);
        assert!(voice.is_finished());
        assert!((rendered as f32 / RATE as f32 - 0.102).abs() < 0.001);
    }

    #[test]
    fn short_notes_fade_from_where_they_are() {
        let pad = Instrument {
            attack: 1.0,
            release: 0.5,
            ..Default::default()
        };
        let samples = left(&pad.render_note(Pitch::A4, 0.1, 1.0, RATE));
        assert_eq!(samples.len(), (0.6 * RATE as f32).round() as usize);
        let peak = samples
            .iter()
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        assert!(
            peak < 0.11,
            "it never got past a tenth of its attack: {peak}"
        );
    }

    #[test]
    fn sweeps_and_vibrato_move_the_pitch() {
        let kick = Instrument {
            sweep: 12.0,
            sweep_time: 0.5,
            ..Default::default()
        };
        let samples = left(&kick.render_note(Pitch::A4, 1.0, 1.0, RATE));
        let tenth = (RATE / 10) as usize;
        // An octave up as it starts, sliding down to its note.
        let start = rises(&samples[..tenth]);
        assert!((80..=86).contains(&start), "{start}");
        let end = rises(&samples[RATE as usize / 2..][..tenth]);
        assert!((43..=45).contains(&end), "{end}");

        let singer = Instrument {
            vibrato: Some(Vibrato {
                rate: 5.0,
                depth: 1.0,
                delay: 0.0,
            }),
            ..Default::default()
        };
        let samples = left(&singer.render_note(Pitch::A4, 1.0, 1.0, RATE));
        // A twentieth of a second around its highest and lowest, a semitone either way.
        let twentieth = (RATE / 20) as usize;
        let high = rises(&samples[twentieth / 2..][..twentieth]);
        let low = rises(&samples[twentieth * 5 / 2..][..twentieth]);
        assert!(high > 22 && low < 22, "{high} and {low}");
    }

    #[test]
    fn filters_quiet_what_they_dont_let_through() {
        let saw = |filter| Instrument {
            wave: Wave::Saw,
            filter,
            ..Default::default()
        };
        let loudness_with =
            |filter| loudness(&saw(filter).render_note(pitch("A5"), 0.5, 1.0, RATE));
        let open = loudness_with(None);
        let low = |cutoff| {
            loudness_with(Some(Filter {
                cutoff,
                ..Default::default()
            }))
        };
        assert!(low(20_000.0) > open * 0.9);
        assert!(low(200.0) < open * 0.3);
        let high = loudness_with(Some(Filter {
            kind: FilterKind::High,
            cutoff: 8000.0,
            ..Default::default()
        }));
        assert!(high < open * 0.5);
    }

    #[test]
    fn filters_stay_stable_ringing_and_sweeping() {
        for kind in FilterKind::ALL {
            let instrument = Instrument {
                wave: Wave::Square,
                filter: Some(Filter {
                    kind,
                    cutoff: 30.0,
                    resonance: 1.0,
                    sweep: 10.0,
                    sweep_time: 0.3,
                }),
                ..Default::default()
            };
            for low in [8000, RATE] {
                let samples = instrument.render_note(pitch("C1"), 0.5, 1.0, low);
                assert!(
                    samples.iter().all(|sample| sample.abs() < 30.0),
                    "{kind:?} at {low}"
                );
            }
        }
    }

    #[test]
    fn noise_is_random_but_the_same_every_time() {
        let hiss = Instrument {
            wave: Wave::Noise,
            ..Default::default()
        };
        let samples = hiss.render_note(pitch("C6"), 0.1, 1.0, RATE);
        assert_eq!(samples, hiss.render_note(pitch("C6"), 0.1, 1.0, RATE));
        assert!(loudness(&samples) > 0.4);
        let mut low = Voice::new(hiss, pitch("C2").frequency(), 1.0, RATE, 1);
        let mut high = Voice::new(hiss, pitch("C2").frequency(), 1.0, RATE, 2);
        let mut lows = vec![[0.0; 2]; 100];
        let mut highs = vec![[0.0; 2]; 100];
        low.render(&mut lows);
        high.render(&mut highs);
        assert_ne!(lows, highs, "seeds start it differently");
    }

    #[test]
    fn velocity_volume_and_pan_set_each_side() {
        let instrument = Instrument {
            volume: 0.5,
            pan: -0.5,
            ..Default::default()
        };
        let samples = instrument.render_note(Pitch::A4, 0.5, 0.5, RATE);
        let (left, right): (Vec<f32>, Vec<f32>) = samples
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&[left, right]| (left, right))
            .unzip();
        let peak = |side: &[f32]| side.iter().fold(0.0_f32, |peak, s| peak.max(s.abs()));
        assert!((peak(&left) - 0.25).abs() < 0.001);
        assert!((peak(&right) - 0.125).abs() < 0.001);
        assert_eq!(balance(0.0), [1.0, 1.0]);
        assert_eq!(balance(1.0), [0.0, 1.0]);
    }

    #[test]
    fn rendering_in_pieces_renders_the_same() {
        let instrument = Instrument {
            wave: Wave::Square,
            duty: 0.25,
            decay: 0.05,
            sustain: 0.4,
            sweep: -5.0,
            sweep_time: 0.03,
            vibrato: Some(Vibrato::default()),
            filter: Some(Filter {
                sweep: 2.0,
                sweep_time: 0.04,
                resonance: 0.5,
                ..Default::default()
            }),
            ..Default::default()
        };
        let whole = {
            let mut voice = Voice::new(instrument, 220.0, 0.8, RATE, 3);
            let mut out = vec![[0.0; 2]; 5000];
            voice.render(&mut out);
            out
        };
        for piece in [1, 7, 128, 1000] {
            let mut voice = Voice::new(instrument, 220.0, 0.8, RATE, 3);
            let mut out = vec![[0.0; 2]; 5000];
            for chunk in out.chunks_mut(piece) {
                voice.render(chunk);
            }
            assert_eq!(out, whole, "in pieces of {piece}");
        }
    }

    #[test]
    fn instruments_say_whats_wrong() {
        let problem = |instrument: Instrument| instrument.problem().unwrap();
        assert_eq!(Instrument::default().problem(), None);
        assert_eq!(
            problem(Instrument {
                duty: 1.0,
                ..Default::default()
            }),
            "`duty` must be above 0 and below 1, not 1"
        );
        assert_eq!(
            problem(Instrument {
                attack: f32::NAN,
                ..Default::default()
            }),
            "`attack` must be a number, not NaN"
        );
        assert_eq!(
            problem(Instrument {
                release: 61.0,
                ..Default::default()
            }),
            "`release` must be from 0 to 60 seconds, not 61"
        );
        assert_eq!(
            problem(Instrument {
                sustain: 1.5,
                ..Default::default()
            }),
            "`sustain` must be from 0 to 1, not 1.5"
        );
        assert_eq!(
            problem(Instrument {
                filter: Some(Filter {
                    cutoff: 0.0,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            "`filter.cutoff` must be above 0, not 0"
        );
        assert_eq!(
            problem(Instrument {
                vibrato: Some(Vibrato {
                    rate: -1.0,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            "`vibrato.rate` must be 0 or more, not -1"
        );
    }

    /// FNV-1a over the bits of every sample.
    fn hash(samples: &[f32]) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for byte in samples
            .iter()
            .flat_map(|sample| sample.to_bits().to_le_bytes())
        {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
        hash
    }

    /// Notes with every setting render to the same bits on Windows and Linux, debug and release,
    /// since their math is `libm`'s. A change to how notes sound changes it.
    #[test]
    fn notes_render_the_same_on_every_platform() {
        let instruments = [
            Instrument {
                wave: Wave::Square,
                duty: 0.3,
                attack: 0.01,
                decay: 0.1,
                sustain: 0.6,
                release: 0.05,
                vibrato: Some(Vibrato {
                    rate: 6.0,
                    depth: 0.3,
                    delay: 0.05,
                }),
                ..Default::default()
            },
            Instrument {
                wave: Wave::Saw,
                pan: 0.4,
                filter: Some(Filter {
                    kind: FilterKind::Low,
                    cutoff: 600.0,
                    resonance: 0.7,
                    sweep: 3.0,
                    sweep_time: 0.1,
                }),
                ..Default::default()
            },
            Instrument {
                wave: Wave::Sine,
                decay: 0.2,
                sustain: 0.0,
                sweep: 30.0,
                sweep_time: 0.05,
                ..Default::default()
            },
            Instrument {
                wave: Wave::Noise,
                decay: 0.08,
                sustain: 0.0,
                filter: Some(Filter {
                    kind: FilterKind::High,
                    cutoff: 5000.0,
                    ..Default::default()
                }),
                ..Default::default()
            },
            Instrument {
                wave: Wave::Triangle,
                filter: Some(Filter {
                    kind: FilterKind::Band,
                    cutoff: 900.0,
                    resonance: 0.5,
                    ..Default::default()
                }),
                ..Default::default()
            },
        ];
        let samples: Vec<f32> = instruments
            .iter()
            .flat_map(|instrument| instrument.render_note(pitch("E3"), 0.2, 0.9, 22_050))
            .collect();
        assert_eq!(hash(&samples), 0xa1e7_bdc2_c415_94a7);
    }
}
