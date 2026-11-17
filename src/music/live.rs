//! Songs playing as a game runs: a kira sound that runs a [`Sequencer`] on the audio thread, and
//! the [`PlayingSong`] by which the game changes it and hears what happens.
//!
//! The game sends the sound changes through a queue, which it takes in before each buffer it
//! renders, and the sound sends back what happens and where it is through another, which
//! [`Audio::advance`](crate::audio::Audio) takes in once an update. Both queues are bounded, so
//! neither side allocates as they're used, and neither waits for the other: a change sent to a
//! full queue is dropped with a warning, and so is an event the game hasn't taken in, which only
//! a game stalled for seconds leaves.
//!
//! The sound makes its sequencer at the rate it's first asked to render at, which the audio
//! thread knows and the game doesn't. Without a sound device, the mixer renders at game time, as
//! [`Audio`](crate::audio::Audio) says, so what happens lands on the same updates every run.

use std::{
    convert::Infallible,
    sync::{
        Arc,
        mpsc::{Receiver, SyncSender, TrySendError, sync_channel},
    },
};

use kira::{
    Frame,
    info::Info,
    sound::{Sound, SoundData},
};

use super::{At, Sequencer, Song, SongData, SongEvent};
use crate::{Error, Result, audio::setting_problem};

/// How many changes may wait for the audio thread.
const COMMANDS: usize = 256;

/// How many events and positions may wait for the game.
const REPORTS: usize = 4096;

/// Frames rendered at a time, within what the mixer asks for.
const CHUNK: usize = 256;

/// The least time stopping, pausing or resuming a song takes, in seconds, so it never clicks.
const MIN_FADE: f32 = 0.005;

/// A song playing, as [`Audio::play_song`](crate::audio::Audio::play_song) started it. Copying it
/// is free. Once the song is stopped, or the update after it ends, it refers to nothing, and
/// [`Audio::song`](crate::audio::Audio::song) finds nothing by it. Performances are never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Performance(pub(crate) u64);

/// How [`Audio::play_song`](crate::audio::Audio::play_song) plays a song.
#[derive(Clone, Debug, PartialEq)]
pub struct SongSettings {
    /// Linear volume: 1 is as written.
    pub volume: f32,
    /// Seconds it takes to fade in from silence: 0, by default, starts it at once.
    pub fade_in: f32,
    /// The section it starts with, by name, or the song's start without one.
    pub section: Option<String>,
    /// Beats into that section it starts.
    pub beat: f64,
}

impl Default for SongSettings {
    fn default() -> Self {
        Self {
            volume: 1.0,
            fade_in: 0.0,
            section: None,
            beat: 0.0,
        }
    }
}

/// Where a song playing is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SongPosition {
    /// The section playing, as its index in [`Song::sections`].
    pub section: usize,
    /// Beats into the section, from 0.
    pub beat: f64,
    /// Beats a minute.
    pub tempo: f64,
}

impl SongPosition {
    /// The bar of the section it's in, from 1, and how far into the bar, from 1 at its start up
    /// to 1 more than it has beats, with `beats_per_bar` beats a bar.
    pub fn bar(&self, beats_per_bar: u32) -> (u32, f64) {
        let per_bar = f64::from(beats_per_bar.max(1));
        let bar = (self.beat / per_bar).floor();
        (bar as u32 + 1, self.beat - bar * per_bar + 1.0)
    }
}

/// A change to a song playing, for the audio thread.
#[derive(Clone, Copy, Debug)]
enum Command {
    GoTo(usize, At),
    Tempo {
        tempo: f64,
        at: At,
        over: f64,
    },
    TrackVolume {
        track: usize,
        volume: f32,
        fade: f32,
    },
    Listen {
        track: usize,
        listen: bool,
    },
    Volume {
        volume: f32,
        fade: f32,
    },
    Pause(f32),
    Resume(f32),
    Stop(f32),
}

/// What the audio thread tells the game.
#[derive(Clone, Copy, Debug)]
enum Report {
    Heard(SongEvent),
    Position(SongPosition),
}

/// A song playing, by which the game changes it and hears what happens in it, as
/// [`Audio::song`](crate::audio::Audio::song) gives it.
#[derive(Debug)]
pub struct PlayingSong {
    performance: Performance,
    song: Song,
    commands: SyncSender<Command>,
    reports: Receiver<Report>,
    events: Vec<SongEvent>,
    position: SongPosition,
    paused: bool,
    stopped: bool,
    /// Whether it has ended, in the last update.
    ended: bool,
}

impl PlayingSong {
    /// Plays `song` as `settings` say, as the sound kira plays and the song's half the game holds.
    pub(crate) fn start(
        performance: Performance,
        song: &Song,
        settings: &SongSettings,
    ) -> Result<(Self, LiveSong)> {
        let data = song.data();
        let section = match &settings.section {
            Some(name) => data.section(name).ok_or_else(|| {
                Error::InvalidSong(format!("the song has no section `{name}` to start with"))
            })?,
            None => data.start,
        };
        let problem = setting_problem("volume", settings.volume)
            .or_else(|| setting_problem("fade_in", settings.fade_in))
            .or_else(|| {
                (!settings.beat.is_finite())
                    .then(|| format!("beat must be a number, not {}", settings.beat))
            });
        if let Some(problem) = problem {
            return Err(Error::PlayAudio(problem));
        }
        let (commands, received) = sync_channel(COMMANDS);
        let (reporter, reports) = sync_channel(REPORTS);
        let volume = settings.volume.max(0.0);
        let mut gain = Gain::at(0.0);
        gain.to(volume, settings.fade_in);
        let live = LiveSong(Box::new(SongSound {
            start: Some((Arc::clone(data), section, settings.beat)),
            sequencer: None,
            commands: received,
            reports: reporter,
            gain,
            volume,
            then: Then::Play,
            paused: false,
            stopped: false,
            scratch: [[0.0; 2]; CHUNK],
        }));
        let playing = Self {
            performance,
            song: song.clone(),
            commands,
            reports,
            events: Vec::new(),
            position: SongPosition {
                section,
                beat: settings.beat.max(0.0),
                tempo: data.tempo,
            },
            paused: false,
            stopped: false,
            ended: false,
        };
        Ok((playing, live))
    }

    /// The performance it is.
    pub fn performance(&self) -> Performance {
        self.performance
    }

    /// The song it's playing.
    pub fn song(&self) -> &Song {
        &self.song
    }

    /// What happened in it as the game's last update played, in order. Notes are among them
    /// only for tracks [listened to](PlayingSong::listen).
    pub fn events(&self) -> &[SongEvent] {
        &self.events
    }

    /// Where it is, as of the last update. A song that hasn't started yet is where it starts.
    pub fn position(&self) -> SongPosition {
        self.position
    }

    /// Whether it's paused: paused, and not resumed since.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Whether it has ended: its last section is over and its notes have faded. It ended in the
    /// last update, so this is the last update it's playing.
    pub fn has_ended(&self) -> bool {
        self.ended
    }

    /// Moves to the section named `section`, `at` the time given: at once, or at the start of
    /// the next beat or bar, or when the section playing ends. A section playing starts again.
    /// It's an error if the song has no such section.
    pub fn go_to(&mut self, section: &str, at: At) -> Result<()> {
        let index = self
            .song
            .section_index(section)
            .ok_or_else(|| Error::InvalidSong(format!("the song has no section `{section}`")))?;
        self.send(Command::GoTo(index, at));
        Ok(())
    }

    /// Changes the tempo to `tempo` beats a minute, `at` the time given, sliding there over
    /// `over` beats, or at once for 0. Tempos are kept from 1 to 999; one that isn't a number
    /// changes nothing, and logs a warning.
    pub fn set_tempo(&mut self, tempo: f64, at: At, over: f64) {
        if !tempo.is_finite() || !over.is_finite() {
            log::warn!("couldn't change a song's tempo to {tempo} over {over} beats");
            return;
        }
        self.send(Command::Tempo { tempo, at, over });
    }

    /// Fades the track named `track` to `volume` over `fade` seconds, as a layer comes in or
    /// goes. It's an error if the song has no such track; a volume that isn't a number changes
    /// nothing, and logs a warning.
    pub fn set_track_volume(&mut self, track: &str, volume: f32, fade: f32) -> Result<()> {
        let track = self.track(track)?;
        if let Some(problem) = setting_problem("volume", volume) {
            log::warn!("couldn't change a track's volume: {problem}");
            return Ok(());
        }
        self.send(Command::TrackVolume {
            track,
            volume,
            fade: fade.max(0.0),
        });
        Ok(())
    }

    /// Has [`PlayingSong::events`] tell of the notes the track named `track` starts, or stop.
    /// It's an error if the song has no such track.
    pub fn listen(&mut self, track: &str, listen: bool) -> Result<()> {
        let track = self.track(track)?;
        self.send(Command::Listen { track, listen });
        Ok(())
    }

    /// Changes its volume to `volume` over `fade` seconds. One that isn't a number changes
    /// nothing, and logs a warning.
    pub fn set_volume(&mut self, volume: f32, fade: f32) {
        if let Some(problem) = setting_problem("volume", volume) {
            log::warn!("couldn't change a song's volume: {problem}");
            return;
        }
        self.send(Command::Volume {
            volume,
            fade: fade.max(0.0),
        });
    }

    /// Pauses it, fading it out over `fade` seconds, until it's resumed.
    pub fn pause(&mut self, fade: f32) {
        if !self.paused {
            self.paused = true;
            self.send(Command::Pause(fade));
        }
    }

    /// Resumes it where it paused, fading it in over `fade` seconds.
    pub fn resume(&mut self, fade: f32) {
        if self.paused {
            self.paused = false;
            self.send(Command::Resume(fade));
        }
    }

    /// Stops it, fading it out over `fade` seconds. It's over at once, and can't be resumed.
    pub fn stop(&mut self, fade: f32) {
        if !self.stopped {
            self.stopped = true;
            self.send(Command::Stop(fade));
        }
    }

    /// Whether it's over: stopped, or ended in an update before the last.
    pub(crate) fn is_over(&self) -> bool {
        self.stopped
    }

    /// Takes in what the audio thread has said since the last update, forgetting what it said
    /// before, and whether the song ended an update ago, so it's over.
    pub(crate) fn hear(&mut self) {
        if self.ended {
            self.stopped = true;
        }
        self.events.clear();
        for report in self.reports.try_iter() {
            match report {
                Report::Heard(event) => {
                    if event == SongEvent::End {
                        self.ended = true;
                    }
                    self.events.push(event);
                }
                Report::Position(position) => self.position = position,
            }
        }
    }

    fn track(&self, track: &str) -> Result<usize> {
        self.song
            .track_index(track)
            .ok_or_else(|| Error::InvalidSong(format!("the song has no track `{track}`")))
    }

    fn send(&self, command: Command) {
        if let Err(TrySendError::Full(command)) = self.commands.try_send(command) {
            log::warn!(
                "a song has too many changes waiting for it, so one was dropped: {command:?}"
            );
        }
    }
}

/// A song to play, as kira takes it.
pub(crate) struct LiveSong(Box<SongSound>);

impl std::fmt::Debug for LiveSong {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveSong").finish_non_exhaustive()
    }
}

impl SoundData for LiveSong {
    type Error = Infallible;
    type Handle = ();

    fn into_sound(self) -> Result<(Box<dyn Sound>, ()), Infallible> {
        Ok((self.0, ()))
    }
}

/// What happens once a song's volume has faded to where it's going.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Then {
    Play,
    Pause,
    Stop,
}

/// A volume fading toward where it's going.
#[derive(Clone, Copy, Debug)]
struct Gain {
    volume: f32,
    target: f32,
    /// How far it moves a second.
    speed: f32,
}

impl Gain {
    fn at(volume: f32) -> Self {
        Self {
            volume,
            target: volume,
            speed: 0.0,
        }
    }

    /// Fades to `target` over `fade` seconds.
    fn to(&mut self, target: f32, fade: f32) {
        self.target = target;
        self.speed = if fade > 0.0 {
            (target - self.volume).abs() / fade
        } else {
            f32::INFINITY
        };
    }

    /// Its volume after `dt` more seconds.
    fn next(&mut self, dt: f32) -> f32 {
        let step = self.speed * dt;
        self.volume = if (self.target - self.volume).abs() <= step {
            self.target
        } else {
            self.volume + step.copysign(self.target - self.volume)
        };
        self.volume
    }

    fn arrived(&self) -> bool {
        self.volume == self.target
    }
}

/// A song on the audio thread.
struct SongSound {
    /// The song, and the section and beat it starts at, until it starts.
    start: Option<(Arc<SongData>, usize, f64)>,
    sequencer: Option<Sequencer>,
    commands: Receiver<Command>,
    reports: SyncSender<Report>,
    gain: Gain,
    /// The volume it plays at, unless it's fading out.
    volume: f32,
    then: Then,
    paused: bool,
    stopped: bool,
    scratch: [[f32; 2]; CHUNK],
}

impl SongSound {
    fn apply(&mut self, command: Command) {
        let Some(sequencer) = &mut self.sequencer else {
            return;
        };
        match command {
            Command::GoTo(section, at) => sequencer.go_to(section, at),
            Command::Tempo { tempo, at, over } => sequencer.set_tempo(tempo, at, over),
            Command::TrackVolume {
                track,
                volume,
                fade,
            } => sequencer.set_track_volume(track, volume, fade),
            Command::Listen { track, listen } => sequencer.listen(track, listen),
            Command::Volume { volume, fade } => {
                self.volume = volume;
                if self.then == Then::Play && !self.paused {
                    self.gain.to(volume, fade);
                }
            }
            Command::Pause(fade) => {
                if self.then == Then::Play {
                    self.then = Then::Pause;
                    self.gain.to(0.0, fade.max(MIN_FADE));
                }
            }
            Command::Resume(fade) => {
                if self.then != Then::Stop {
                    self.paused = false;
                    self.then = Then::Play;
                    self.gain.to(self.volume, fade.max(MIN_FADE));
                }
            }
            Command::Stop(fade) => {
                self.then = Then::Stop;
                self.gain.to(0.0, fade.max(MIN_FADE));
            }
        }
    }
}

impl Sound for SongSound {
    fn process(&mut self, out: &mut [Frame], dt: f64, _info: &Info) {
        out.fill(Frame::ZERO);
        if self.stopped {
            return;
        }
        if let Some((song, section, beat)) = self.start.take() {
            let rate = (1.0 / dt).round().clamp(1.0, f64::from(u32::MAX)) as u32;
            self.sequencer = Some(Sequencer::new(song, rate, section, beat));
        }
        while let Ok(command) = self.commands.try_recv() {
            self.apply(command);
        }
        let dt = dt as f32;
        for chunk in out.chunks_mut(CHUNK) {
            if self.paused || self.stopped {
                break;
            }
            let Some(sequencer) = &mut self.sequencer else {
                return;
            };
            let scratch = &mut self.scratch[..chunk.len()];
            scratch.fill([0.0; 2]);
            let reports = &self.reports;
            sequencer.render(scratch, &mut |heard| {
                let _ = reports.try_send(Report::Heard(heard.event));
            });
            for (frame, [left, right]) in chunk.iter_mut().zip(scratch.iter()) {
                let volume = self.gain.next(dt);
                *frame = Frame::new(left * volume, right * volume);
            }
            if self.gain.arrived() {
                match self.then {
                    Then::Play => {}
                    Then::Pause => self.paused = true,
                    Then::Stop => self.stopped = true,
                }
            }
            if sequencer.is_finished() {
                self.stopped = true;
            }
        }
        if let Some(sequencer) = &self.sequencer {
            let (section, beat) = sequencer.position();
            let _ = self.reports.try_send(Report::Position(SongPosition {
                section,
                beat,
                tempo: sequencer.tempo(),
            }));
        }
    }

    fn finished(&self) -> bool {
        self.stopped
    }
}

#[cfg(test)]
mod tests {
    use kira::info::MockInfoBuilder;

    use super::*;
    use crate::music::SongFile;

    const SONG: &str = r#"return {
        format = 1,
        start = "a",
        instruments = { tone = { release = 0.05 } },
        tracks = { { name = "t", instrument = "tone" } },
        patterns = { p = { bars = 1, notes = { { 0, "C4", 1 }, { 3, "G4", 2 } } } },
        sections = {
            a = { bars = 1, tracks = { t = "p" }, next = "b" },
            b = { bars = 1 },
        },
    }"#;

    fn song() -> Song {
        Song::new(&SongFile::parse(SONG, "test.song.luau").unwrap()).unwrap()
    }

    /// Renders `sound` for `frames` frames at `rate`, in the mixer's buffers of 128.
    fn render(sound: &mut SongSound, frames: usize, rate: u32) -> Vec<f32> {
        let info = MockInfoBuilder::new().build();
        let mut samples = Vec::new();
        let mut out = [Frame::ZERO; 128];
        for _ in 0..frames.div_ceil(128) {
            sound.process(&mut out, 1.0 / f64::from(rate), &info);
            samples.extend(out.iter().flat_map(|frame| [frame.left, frame.right]));
        }
        samples.truncate(frames * 2);
        samples
    }

    #[test]
    fn songs_play_live_as_they_bake() {
        let song = song();
        let rendered = song.render(48_000, None).unwrap();
        let (_, LiveSong(mut sound)) =
            PlayingSong::start(Performance(0), &song, &SongSettings::default()).unwrap();
        let frames = rendered.samples.len() / 2;
        assert_eq!(render(&mut sound, frames, 48_000), rendered.samples);
        render(&mut sound, 128, 48_000);
        assert!(sound.finished(), "it ended with its last section");
    }

    #[test]
    fn stopping_and_pausing_fade_and_wait() {
        let song = song();
        let (mut playing, LiveSong(mut sound)) =
            PlayingSong::start(Performance(0), &song, &SongSettings::default()).unwrap();
        render(&mut sound, 4800, 48_000);
        playing.pause(0.01);
        render(&mut sound, 1000, 48_000);
        assert!(sound.paused);
        let paused = sound.sequencer.as_ref().unwrap().frame();
        assert!(render(&mut sound, 4800, 48_000).iter().all(|&s| s == 0.0));
        assert_eq!(
            sound.sequencer.as_ref().unwrap().frame(),
            paused,
            "it waits"
        );
        playing.resume(0.0);
        assert!(render(&mut sound, 4800, 48_000).iter().any(|&s| s != 0.0));
        playing.stop(0.02);
        render(&mut sound, 2000, 48_000);
        assert!(sound.finished());
        assert!(playing.is_over());
    }

    #[test]
    fn positions_count_bars_and_beats() {
        let position = SongPosition {
            section: 0,
            beat: 5.5,
            tempo: 120.0,
        };
        assert_eq!(position.bar(4), (2, 2.5));
        assert_eq!(position.bar(3), (2, 3.5));
    }

    #[test]
    fn songs_say_what_they_dont_have() {
        let song = song();
        let start = |settings: SongSettings| {
            PlayingSong::start(Performance(0), &song, &settings)
                .unwrap_err()
                .to_string()
        };
        let chorus = start(SongSettings {
            section: Some("chorus".to_owned()),
            ..Default::default()
        });
        assert!(chorus.contains("no section `chorus`"), "{chorus}");
        let volume = start(SongSettings {
            volume: f32::NAN,
            ..Default::default()
        });
        assert!(volume.contains("volume must be a number"), "{volume}");
        let (mut playing, _) =
            PlayingSong::start(Performance(0), &song, &SongSettings::default()).unwrap();
        assert!(playing.go_to("chorus", At::Bar).is_err());
        assert!(playing.set_track_volume("drums", 1.0, 0.0).is_err());
        assert!(playing.listen("t", true).is_ok());
    }
}
