//! Music and sound effects, played with [kira](https://docs.rs/kira).
//!
//! [`Audio`] mixes sound effects and music on separate tracks, each with its own volume. A
//! [`Sound`] is decoded into memory when it's loaded, so it starts at once and can overlap
//! itself; music streams from its file as it plays, and loops. Games reach the `Audio` through
//! [`Context::audio`](crate::Context::audio).
//!
//! Files can be WAV or OGG Vorbis. Volumes are linear: 1 plays a sound as recorded, 0.5 at half
//! the amplitude, and 0 is silent.
//!
//! Without a sound device, and always when running headless, audio is silent. The mixer still
//! runs, in step with game time, so sounds start, play and finish as they would otherwise; only
//! nothing is heard.

use std::{fmt, path::Path, time::Duration};

use kira::{
    AudioManager, AudioManagerSettings, Decibels, Tween,
    backend::{
        cpal::CpalBackend,
        mock::{MockBackend, MockBackendSettings},
    },
    sound::{
        FromFileError, PlaybackState,
        static_sound::{StaticSoundData, StaticSoundHandle},
        streaming::{StreamingSoundData, StreamingSoundHandle},
    },
    track::{MainTrackHandle, TrackBuilder, TrackHandle},
};

use crate::{Error, Result};

/// Samples per second of silent output. Nothing is heard, so it only needs to be high enough for
/// sounds to keep time.
const SILENT_SAMPLE_RATE: u32 = 8000;
/// Frames the mixer renders at a time.
const BUFFER_FRAMES: usize = 128;

/// Plays sound effects and music.
pub struct Audio {
    output: Output,
    sound_track: TrackHandle,
    music_track: TrackHandle,
    /// Sound effects started and not yet known to have finished.
    sounds: Vec<StaticSoundHandle>,
    music: Option<StreamingSoundHandle<FromFileError>>,
}

/// The managers are boxed because they're large, and differently so.
enum Output {
    Device(Box<AudioManager<CpalBackend>>),
    /// A mixer that renders to nowhere, run by [`Audio::advance`] at game time.
    Silent {
        manager: Box<AudioManager<MockBackend>>,
        /// Frames of game time not yet rendered.
        frames_due: f64,
    },
}

/// A sound effect, loaded into memory. Cloning is cheap: clones share the samples.
#[derive(Clone)]
pub struct Sound {
    data: StaticSoundData,
}

/// How [`Audio::play_with`] plays a sound. Every setting must be a finite number.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaySettings {
    /// Linear volume: 1 is as recorded.
    pub volume: f32,
    /// Playback speed, which changes the pitch with it: 2 is twice as fast and an octave up.
    /// Above 0.
    pub speed: f32,
    /// Stereo position, from -1 (left) through 0 (center) to 1 (right).
    pub pan: f32,
}

impl Default for PlaySettings {
    fn default() -> Self {
        Self {
            volume: 1.0,
            speed: 1.0,
            pan: 0.0,
        }
    }
}

impl PlaySettings {
    /// What's wrong with these settings, if anything.
    pub(crate) fn problem(&self) -> Option<String> {
        let Self { volume, speed, pan } = *self;
        if !(volume.is_finite() && pan.is_finite()) {
            Some(format!("volume {volume} and pan {pan} must be numbers"))
        } else if !(speed.is_finite() && speed > 0.0) {
            Some(format!("speed must be a number above 0, not {speed}"))
        } else {
            None
        }
    }
}

impl Audio {
    /// Plays through the default sound device, or silently if there isn't one.
    pub(crate) fn new() -> Self {
        match AudioManager::<CpalBackend>::new(AudioManagerSettings::default()) {
            Ok(manager) => Self::with_output(Output::Device(Box::new(manager))),
            Err(err) => {
                log::warn!("no sound device, so audio is silent: {err}");
                Self::silent()
            }
        }
    }

    /// Plays to nowhere, as when running headless.
    pub(crate) fn silent() -> Self {
        let manager = AudioManager::<MockBackend>::new(AudioManagerSettings {
            internal_buffer_size: BUFFER_FRAMES,
            backend_settings: MockBackendSettings {
                sample_rate: SILENT_SAMPLE_RATE,
            },
            ..Default::default()
        })
        .expect("the silent backend always starts");
        Self::with_output(Output::Silent {
            manager: Box::new(manager),
            frames_due: 0.0,
        })
    }

    fn with_output(mut output: Output) -> Self {
        Self {
            sound_track: output.add_track(),
            music_track: output.add_track(),
            output,
            sounds: Vec::new(),
            music: None,
        }
    }

    /// Whether audio goes nowhere: there's no sound device, or the game runs headless.
    pub fn is_silent(&self) -> bool {
        matches!(self.output, Output::Silent { .. })
    }

    /// Plays `sound` once, as recorded.
    pub fn play(&mut self, sound: &Sound) {
        self.play_with(sound, PlaySettings::default());
    }

    /// Plays `sound` once, with its volume, speed or panning changed.
    pub fn play_with(&mut self, sound: &Sound, settings: PlaySettings) {
        // A sound at speed 0 would never end, and other values could fill the mix with NaNs.
        if let Some(problem) = settings.problem() {
            log::warn!("couldn't play a sound: {problem}");
            return;
        }
        let data = sound
            .data
            .volume(decibels(settings.volume))
            .playback_rate(f64::from(settings.speed))
            .panning(settings.pan);
        match self.sound_track.play(data) {
            Ok(handle) => self.sounds.push(handle),
            // Too many sounds at once. Dropping one is better than stopping the game.
            Err(err) => log::warn!("couldn't play a sound: {err}"),
        }
    }

    /// Stops every sound effect, fading them out over `fade` seconds.
    pub fn stop_sounds(&mut self, fade: f32) {
        for sound in &mut self.sounds {
            sound.stop(tween(fade));
        }
    }

    /// How many sound effects are playing, as of the latest frame.
    pub fn sounds_playing(&self) -> usize {
        self.sounds
            .iter()
            .filter(|sound| is_playing(sound.state()))
            .count()
    }

    /// Starts looping the music file at `path`, streaming it from disk as it plays. Music already
    /// playing fades out over `fade` seconds as the new music fades in.
    pub fn play_music(&mut self, path: impl AsRef<Path>, fade: f32) -> Result<()> {
        let path = path.as_ref();
        let data = StreamingSoundData::from_file(path)
            .map_err(|source| Error::LoadAudio {
                path: path.to_owned(),
                source,
            })?
            .loop_region(..)
            .fade_in_tween((fade > 0.0).then(|| tween(fade)));
        self.stop_music(fade);
        let music = self
            .music_track
            .play(data)
            .map_err(|err| Error::PlayAudio(err.to_string()))?;
        self.music = Some(music);
        Ok(())
    }

    /// Stops the music, fading it out over `fade` seconds.
    pub fn stop_music(&mut self, fade: f32) {
        if let Some(mut music) = self.music.take() {
            music.stop(tween(fade));
        }
    }

    /// Whether music started with [`Audio::play_music`] is playing.
    pub fn music_playing(&self) -> bool {
        self.music
            .as_ref()
            .is_some_and(|music| is_playing(music.state()))
    }

    /// Sets the volume of everything.
    pub fn set_volume(&mut self, volume: f32) {
        self.output
            .main_track()
            .set_volume(decibels(volume), Tween::default());
    }

    /// Sets the volume of sound effects, apart from music.
    pub fn set_sound_volume(&mut self, volume: f32) {
        self.sound_track
            .set_volume(decibels(volume), Tween::default());
    }

    /// Sets the volume of music, apart from sound effects.
    pub fn set_music_volume(&mut self, volume: f32) {
        self.music_track
            .set_volume(decibels(volume), Tween::default());
    }

    /// Called once a frame: renders `dt` seconds of silent output, and forgets sound effects
    /// that have finished. A sound device renders on its own thread instead.
    pub(crate) fn advance(&mut self, dt: f32) {
        if let Output::Silent {
            manager,
            frames_due,
        } = &mut self.output
        {
            *frames_due += f64::from(dt) * f64::from(SILENT_SAMPLE_RATE);
            let backend = manager.backend_mut();
            while *frames_due >= BUFFER_FRAMES as f64 {
                backend.on_start_processing();
                backend.process();
                *frames_due -= BUFFER_FRAMES as f64;
            }
        }
        self.sounds.retain(|sound| is_playing(sound.state()));
    }
}

impl Output {
    fn add_track(&mut self) -> TrackHandle {
        let track = match self {
            Self::Device(manager) => manager.add_sub_track(TrackBuilder::new()),
            Self::Silent { manager, .. } => manager.add_sub_track(TrackBuilder::new()),
        };
        track.expect("a new mixer has room for the engine's tracks")
    }

    fn main_track(&mut self) -> &mut MainTrackHandle {
        match self {
            Self::Device(manager) => manager.main_track(),
            Self::Silent { manager, .. } => manager.main_track(),
        }
    }
}

impl Sound {
    /// Loads a WAV or OGG Vorbis file, decoding all of it.
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let data = StaticSoundData::from_file(path).map_err(|source| Error::LoadAudio {
            path: path.to_owned(),
            source,
        })?;
        Ok(Self { data })
    }

    /// How long the sound lasts at its normal speed, in seconds.
    pub fn duration(&self) -> f32 {
        self.data.duration().as_secs_f32()
    }
}

impl fmt::Debug for Sound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Sound")
            .field("duration", &self.duration())
            .finish_non_exhaustive()
    }
}

fn is_playing(state: PlaybackState) -> bool {
    state != PlaybackState::Stopped
}

/// A linear volume in decibels, where kira treats 60 dB down as silence.
fn decibels(volume: f32) -> Decibels {
    if volume > 0.0 {
        Decibels((20.0 * volume.log10()).max(Decibels::SILENCE.0))
    } else {
        Decibels::SILENCE
    }
}

/// A tween lasting `seconds`: none for a negative number or NaN, and the longest there is for
/// one too long for a `Duration`, such as infinity.
fn tween(seconds: f32) -> Tween {
    Tween {
        duration: Duration::try_from_secs_f32(seconds.max(0.0)).unwrap_or(Duration::MAX),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: f32 = 1.0 / 60.0;
    const SOUNDS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/sounds/");

    fn sound(name: &str) -> Sound {
        Sound::load(format!("{SOUNDS}{name}")).unwrap()
    }

    fn run(audio: &mut Audio, seconds: f32) {
        for _ in 0..(seconds / FRAME).round() as u32 {
            audio.advance(FRAME);
        }
    }

    #[test]
    fn loads_wav_and_ogg() {
        assert!((sound("chime.wav").duration() - 0.44).abs() < 0.01);
        assert!((sound("music.ogg").duration() - 8.0).abs() < 0.05);
    }

    #[test]
    fn missing_files_name_the_path() {
        let err = Sound::load("no/such/sound.wav").unwrap_err();
        assert!(err.to_string().contains("no/such/sound.wav"), "{err}");
        let err = Audio::silent()
            .play_music("no/such/music.ogg", 0.0)
            .unwrap_err();
        assert!(matches!(err, Error::LoadAudio { .. }));
    }

    #[test]
    fn sounds_play_until_they_end() {
        let mut audio = Audio::silent();
        let chime = sound("chime.wav");
        audio.play(&chime);
        audio.play_with(
            &chime,
            PlaySettings {
                speed: 2.0,
                ..Default::default()
            },
        );
        audio.advance(FRAME);
        assert_eq!(audio.sounds_playing(), 2);
        // The faster one finishes in half the time.
        run(&mut audio, 0.3);
        assert_eq!(audio.sounds_playing(), 1);
        run(&mut audio, 0.3);
        assert_eq!(audio.sounds_playing(), 0);
    }

    #[test]
    fn stopping_sounds_fades_them_out() {
        let mut audio = Audio::silent();
        audio.play(&sound("chime.wav"));
        audio.stop_sounds(0.05);
        run(&mut audio, 0.1);
        assert_eq!(audio.sounds_playing(), 0);
    }

    #[test]
    fn music_loops_until_stopped() {
        let mut audio = Audio::silent();
        audio.play_music(format!("{SOUNDS}music.ogg"), 0.5).unwrap();
        audio.set_music_volume(0.5);
        run(&mut audio, 0.5);
        assert!(audio.music_playing());
        audio.stop_music(0.2);
        assert!(!audio.music_playing());
    }

    #[test]
    fn endless_fades_and_invalid_settings_do_not_panic() {
        let mut audio = Audio::silent();
        let music = format!("{SOUNDS}music.ogg");
        audio.play_music(&music, f32::INFINITY).unwrap();
        audio.stop_music(1e30);
        audio.stop_sounds(f32::INFINITY);
        let speed = PlaySettings {
            speed: 0.0,
            ..Default::default()
        };
        audio.play_with(&sound("chime.wav"), speed);
        audio.advance(FRAME);
        assert_eq!(audio.sounds_playing(), 0, "a sound that would never end");
    }

    /// Plays music through the sound device at zero volume, and checks the device renders it.
    /// CI runners have no sound device, so run it by hand: `cargo test -- --ignored`.
    #[test]
    #[ignore = "needs a sound device"]
    fn plays_through_the_sound_device() {
        let mut audio = Audio::new();
        assert!(!audio.is_silent(), "no sound device was found");
        audio.set_volume(0.0);
        audio.play_music(format!("{SOUNDS}music.ogg"), 0.0).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        let position = audio.music.as_ref().unwrap().position();
        assert!(position > 0.2, "the device rendered {position}s of music");
    }

    #[test]
    fn volumes_convert_to_decibels() {
        assert_eq!(decibels(1.0), Decibels::IDENTITY);
        assert!((decibels(0.5).0 + 6.02).abs() < 0.01);
        assert_eq!(decibels(0.0), Decibels::SILENCE);
        assert_eq!(decibels(1e-9), Decibels::SILENCE);
        assert_eq!(decibels(-1.0), Decibels::SILENCE);
    }
}
