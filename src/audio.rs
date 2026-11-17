//! Music and sound effects, played with [kira](https://docs.rs/kira).
//!
//! [`Audio`] mixes sound effects and music on separate tracks, each with its own volume. A
//! [`Sound`] is decoded into memory when it's loaded, so it starts at once and can overlap
//! itself; music streams from its file as it plays, and loops. Games reach the `Audio` through
//! [`Context::audio`](crate::Context::audio).
//!
//! Playing a sound gives a [`Voice`], by which the game changes it as it plays (its volume, speed
//! and panning, at once or gradually), pauses and resumes it, or stops it, which is how a sound
//! that loops, such as a hum or footsteps, ends.
//!
//! [Songs](crate::music) play live on the music track too ([`Audio::play_song`]), each as a
//! [`PlayingSong`] by which the game changes its tempo, moves between its sections, fades its
//! tracks in and out, and hears its beats, sections, markers and notes as they pass.
//!
//! Files can be WAV or OGG Vorbis, and a sound can be made of samples instead
//! ([`Sound::from_samples`]), for games that work their sounds out. Volumes are linear: 1 plays a sound as recorded, 0.5 at half
//! the amplitude, and 0 is silent.
//!
//! Without a sound device, and always when running headless, audio is silent. The mixer still
//! runs, in step with game time, so sounds start, play and finish as they would otherwise; only
//! nothing is heard.
//!
//! On the web, where kira can't stream, music is decoded into memory when it starts, as a sound
//! effect is, and sounds are read from the page's [files](crate::files).

use std::{fmt, path::Path, time::Duration};

#[cfg(not(target_arch = "wasm32"))]
use kira::sound::{
    FromFileError,
    streaming::{StreamingSoundData, StreamingSoundHandle},
};
use kira::{
    AudioManager, AudioManagerSettings, Decibels, Frame, Tween,
    backend::{
        cpal::CpalBackend,
        mock::{MockBackend, MockBackendSettings},
    },
    sound::{
        PlaybackState,
        static_sound::{StaticSoundData, StaticSoundHandle, StaticSoundSettings},
    },
    track::{MainTrackHandle, TrackBuilder, TrackHandle},
};

use crate::{
    Error, Result,
    music::{Performance, PlayingSong, Song, SongSettings},
};

/// Music playing.
#[cfg(not(target_arch = "wasm32"))]
type MusicHandle = StreamingSoundHandle<FromFileError>;
#[cfg(target_arch = "wasm32")]
type MusicHandle = StaticSoundHandle;

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
    /// Sound effects started and not yet known to have finished, in the order they started, so by
    /// their voices.
    sounds: Vec<Playing>,
    /// The voice the next sound effect plays as.
    next_voice: u64,
    music: Option<MusicHandle>,
    /// Songs playing, and those that ended in the last update, in the order they started, so by
    /// their performances.
    songs: Vec<PlayingSong>,
    next_performance: u64,
}

/// A sound effect playing, as [`Audio::play`] started it, by which the game changes, pauses or
/// stops it while it plays. Copying it is free.
///
/// Once its sound finishes, or is stopped, a voice refers to nothing: changing it does nothing,
/// and [`Audio::voice_playing`] is false. A sound that couldn't start gives a voice that has
/// finished already. Voices are never reused, so an old one never reaches a newer sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Voice(u64);

/// A sound effect the mixer has, and what the game asked of it. The mixer's own state catches up
/// only once it has rendered since, so a sound stopped this frame still reads as playing there.
struct Playing {
    voice: Voice,
    handle: StaticSoundHandle,
    /// Whether the game stopped it. It may be fading out, but it's over.
    stopped: bool,
    /// Whether the game paused it, and hasn't resumed it since.
    paused: bool,
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

/// How [`Audio::play_with`] plays a sound. The volume, speed and panning must be finite numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaySettings {
    /// Linear volume: 1 is as recorded.
    pub volume: f32,
    /// Playback speed, which changes the pitch with it: 2 is twice as fast and an octave up.
    /// Above 0.
    pub speed: f32,
    /// Stereo position, from -1 (left) through 0 (center) to 1 (right).
    pub pan: f32,
    /// Whether the sound plays over and over until it's stopped, rather than once.
    pub looping: bool,
    /// Seconds the sound takes to fade in from silence: 0, by default, starts it at once.
    pub fade_in: f32,
}

impl Default for PlaySettings {
    fn default() -> Self {
        Self {
            volume: 1.0,
            speed: 1.0,
            pan: 0.0,
            looping: false,
            fade_in: 0.0,
        }
    }
}

impl PlaySettings {
    /// What's wrong with these settings, if anything.
    pub(crate) fn problem(&self) -> Option<String> {
        let Self {
            volume, speed, pan, ..
        } = *self;
        setting_problem("volume", volume)
            .or_else(|| setting_problem("speed", speed))
            .or_else(|| setting_problem("pan", pan))
    }
}

/// What's wrong with `value` as the setting `name` (`volume`, `speed` or `pan`), if anything:
/// each must be a number, and a speed above 0, since a sound at speed 0 would never end, and the
/// rest could fill the mix with NaNs.
pub(crate) fn setting_problem(name: &str, value: f32) -> Option<String> {
    if !value.is_finite() {
        Some(format!("{name} must be a number, not {value}"))
    } else if name == "speed" && value <= 0.0 {
        Some(format!("speed must be above 0, not {value}"))
    } else {
        None
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
            next_voice: 0,
            music: None,
            songs: Vec::new(),
            next_performance: 0,
        }
    }

    /// Whether audio goes nowhere: there's no sound device, or the game runs headless.
    pub fn is_silent(&self) -> bool {
        matches!(self.output, Output::Silent { .. })
    }

    /// Plays `sound` once, as recorded.
    pub fn play(&mut self, sound: &Sound) -> Voice {
        self.play_with(sound, PlaySettings::default())
    }

    /// Plays `sound` with its volume, speed or panning changed, looping or fading in as
    /// `settings` say.
    ///
    /// Settings that aren't numbers, or too many sounds playing at once, log a warning, and the
    /// sound doesn't play: its voice has finished already. Dropping a sound is better than
    /// stopping the game.
    pub fn play_with(&mut self, sound: &Sound, settings: PlaySettings) -> Voice {
        let voice = Voice(self.next_voice);
        self.next_voice += 1;
        if let Some(problem) = settings.problem() {
            log::warn!("couldn't play a sound: {problem}");
            return voice;
        }
        let fade_in = settings.fade_in;
        let mut data = sound
            .data
            .volume(decibels(settings.volume))
            .playback_rate(f64::from(settings.speed))
            .panning(settings.pan)
            .fade_in_tween((fade_in > 0.0).then(|| tween(fade_in)));
        if settings.looping {
            data = data.loop_region(..);
        }
        match self.sound_track.play(data) {
            Ok(handle) => self.sounds.push(Playing {
                voice,
                handle,
                stopped: false,
                paused: false,
            }),
            Err(err) => log::warn!("couldn't play a sound: {err}"),
        }
        voice
    }

    /// Stops every sound effect, fading them out over `fade` seconds.
    pub fn stop_sounds(&mut self, fade: f32) {
        for sound in self.sounds.iter_mut().filter(|sound| sound.is_playing()) {
            sound.stop(fade);
        }
    }

    /// Pauses every sound effect playing, fading them out over `fade` seconds, as when the game
    /// pauses. Sounds played after carry on as usual.
    pub fn pause_sounds(&mut self, fade: f32) {
        for sound in self.sounds.iter_mut().filter(|sound| sound.is_playing()) {
            sound.pause(fade);
        }
    }

    /// Resumes every sound effect paused, whether by [`Audio::pause_sounds`] or by its voice,
    /// fading them in over `fade` seconds.
    pub fn resume_sounds(&mut self, fade: f32) {
        for sound in self.sounds.iter_mut().filter(|sound| sound.is_playing()) {
            sound.resume(fade);
        }
    }

    /// How many sound effects are playing: started, and neither finished nor stopped. Paused ones
    /// count.
    pub fn sounds_playing(&self) -> usize {
        self.sounds
            .iter()
            .filter(|sound| sound.is_playing())
            .count()
    }

    /// Whether `voice` is playing: it hasn't finished or been stopped. A paused voice is still
    /// playing, only paused.
    pub fn voice_playing(&self, voice: Voice) -> bool {
        self.find(voice).is_some()
    }

    /// Whether `voice` is paused: playing, but paused since it was last resumed.
    pub fn voice_paused(&self, voice: Voice) -> bool {
        self.find(voice).is_some_and(|sound| sound.paused)
    }

    /// Stops `voice`, fading it out over `fade` seconds. It can't be resumed.
    pub fn stop_voice(&mut self, voice: Voice, fade: f32) {
        if let Some(sound) = self.find_mut(voice) {
            sound.stop(fade);
        }
    }

    /// Pauses `voice`, fading it out over `fade` seconds, until it's resumed.
    pub fn pause_voice(&mut self, voice: Voice, fade: f32) {
        if let Some(sound) = self.find_mut(voice) {
            sound.pause(fade);
        }
    }

    /// Resumes `voice` where it was paused, fading it in over `fade` seconds.
    pub fn resume_voice(&mut self, voice: Voice, fade: f32) {
        if let Some(sound) = self.find_mut(voice) {
            sound.resume(fade);
        }
    }

    /// Changes `voice`'s volume to `volume` over `fade` seconds, as [`PlaySettings::volume`].
    pub fn set_voice_volume(&mut self, voice: Voice, volume: f32, fade: f32) {
        if let Some(sound) = self.find_changing(voice, "volume", volume) {
            sound.handle.set_volume(decibels(volume), tween(fade));
        }
    }

    /// Changes `voice`'s speed, and its pitch with it, to `speed` over `fade` seconds, as
    /// [`PlaySettings::speed`].
    pub fn set_voice_speed(&mut self, voice: Voice, speed: f32, fade: f32) {
        if let Some(sound) = self.find_changing(voice, "speed", speed) {
            sound
                .handle
                .set_playback_rate(f64::from(speed), tween(fade));
        }
    }

    /// Moves `voice` between the speakers to `pan` over `fade` seconds, as [`PlaySettings::pan`].
    pub fn set_voice_pan(&mut self, voice: Voice, pan: f32, fade: f32) {
        if let Some(sound) = self.find_changing(voice, "pan", pan) {
            sound.handle.set_panning(pan, tween(fade));
        }
    }

    /// The sound playing as `voice`, unless it's over.
    fn find(&self, voice: Voice) -> Option<&Playing> {
        let index = self.index(voice)?;
        Some(&self.sounds[index]).filter(|sound| sound.is_playing())
    }

    fn find_mut(&mut self, voice: Voice) -> Option<&mut Playing> {
        let index = self.index(voice)?;
        Some(&mut self.sounds[index]).filter(|sound| sound.is_playing())
    }

    /// The sound playing as `voice`, unless it's over, to change its setting `name` to `value`; or
    /// none, logging a warning, if that's no value for it.
    fn find_changing(&mut self, voice: Voice, name: &str, value: f32) -> Option<&mut Playing> {
        if let Some(problem) = setting_problem(name, value) {
            log::warn!("couldn't change a sound: {problem}");
            return None;
        }
        self.find_mut(voice)
    }

    fn index(&self, voice: Voice) -> Option<usize> {
        self.sounds
            .binary_search_by_key(&voice.0, |sound| sound.voice.0)
            .ok()
    }

    /// Starts looping the music file at `path`, streaming it from disk as it plays. Music already
    /// playing fades out over `fade` seconds as the new music fades in.
    pub fn play_music(&mut self, path: impl AsRef<Path>, fade: f32) -> Result<()> {
        let path = path.as_ref();
        #[cfg(not(target_arch = "wasm32"))]
        let data = StreamingSoundData::from_file(path).map_err(|source| Error::LoadAudio {
            path: path.to_owned(),
            source,
        })?;
        #[cfg(target_arch = "wasm32")]
        let data = Sound::load(path)?.data;
        let data = data
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

    /// Plays `song` live on the music track, so [`Audio::set_music_volume`] sets its volume, as
    /// `settings` say. Songs play alongside music and one another.
    ///
    /// Fails if the song has no section `settings` names, or a setting isn't a number.
    pub fn play_song(&mut self, song: &Song, settings: SongSettings) -> Result<Performance> {
        let performance = Performance(self.next_performance);
        self.next_performance += 1;
        let (playing, live) = PlayingSong::start(performance, song, &settings)?;
        self.music_track
            .play(live)
            .map_err(|err| Error::PlayAudio(err.to_string()))?;
        self.songs.push(playing);
        Ok(performance)
    }

    /// The song playing as `performance`, to change it or hear what happened in it, unless
    /// it's over: stopped, or ended in an update before the last.
    pub fn song(&mut self, performance: Performance) -> Option<&mut PlayingSong> {
        let index = self
            .songs
            .binary_search_by_key(&performance.0, |song| song.performance().0)
            .ok()?;
        Some(&mut self.songs[index]).filter(|song| !song.is_over())
    }

    /// How many songs are playing: started, and neither stopped nor ended before the last
    /// update. Paused ones count.
    pub fn songs_playing(&self) -> usize {
        self.songs.iter().filter(|song| !song.is_over()).count()
    }

    /// Stops every song, fading them out over `fade` seconds.
    pub fn stop_songs(&mut self, fade: f32) {
        for song in &mut self.songs {
            song.stop(fade);
        }
    }

    /// Sets the volume of everything. A volume that isn't a number changes nothing, and logs a
    /// warning, as for a voice.
    pub fn set_volume(&mut self, volume: f32) {
        if let Some(volume) = track_volume(volume) {
            self.output
                .main_track()
                .set_volume(volume, Tween::default());
        }
    }

    /// Sets the volume of sound effects, apart from music, as [`Audio::set_volume`] does.
    pub fn set_sound_volume(&mut self, volume: f32) {
        if let Some(volume) = track_volume(volume) {
            self.sound_track.set_volume(volume, Tween::default());
        }
    }

    /// Sets the volume of music, apart from sound effects, as [`Audio::set_volume`] does.
    pub fn set_music_volume(&mut self, volume: f32) {
        if let Some(volume) = track_volume(volume) {
            self.music_track.set_volume(volume, Tween::default());
        }
    }

    /// Stops every sound effect, the music and every song at once, and sets every volume back to
    /// 1, as for a game starting again in the same window.
    pub(crate) fn reset(&mut self) {
        self.stop_sounds(0.0);
        self.stop_music(0.0);
        self.stop_songs(0.0);
        self.songs.clear();
        self.set_volume(1.0);
        self.set_sound_volume(1.0);
        self.set_music_volume(1.0);
    }

    /// Called once an update: renders `dt` seconds of silent output, forgets sound effects that
    /// have finished, and takes in what happened in each song, forgetting songs that are over. A
    /// sound device renders on its own thread instead.
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
        self.sounds.retain(|sound| is_playing(sound.handle.state()));
        for song in &mut self.songs {
            song.hear();
        }
        self.songs.retain(|song| !song.is_over());
    }
}

impl Playing {
    /// Whether the sound is still going: neither stopped nor finished.
    fn is_playing(&self) -> bool {
        !self.stopped && is_playing(self.handle.state())
    }

    fn stop(&mut self, fade: f32) {
        self.handle.stop(tween(fade));
        self.stopped = true;
    }

    fn pause(&mut self, fade: f32) {
        if !self.paused {
            self.handle.pause(tween(fade));
            self.paused = true;
        }
    }

    fn resume(&mut self, fade: f32) {
        if self.paused {
            self.handle.resume(tween(fade));
            self.paused = false;
        }
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
        let error = |source| Error::LoadAudio {
            path: path.to_owned(),
            source,
        };
        #[cfg(not(target_arch = "wasm32"))]
        let data = StaticSoundData::from_file(path).map_err(error)?;
        #[cfg(target_arch = "wasm32")]
        let data = {
            let bytes = crate::files::read(path).map_err(|err| error(err.into()))?;
            StaticSoundData::from_cursor(std::io::Cursor::new(bytes)).map_err(error)?
        };
        Ok(Self { data })
    }

    /// Makes a sound of `samples`, as a game works them out rather than records them: each from
    /// -1 to 1, `rate` of them a second for each of its `channels`, which are 1, heard from both
    /// speakers alike, or 2, a sample for the left then one for the right.
    ///
    /// Samples past -1 and 1 clip as they're heard. It's an error if there are no samples, or not
    /// one for each channel, if one isn't a number, which would fill the mix with NaNs, if `rate`
    /// isn't from 1 to [`Sound::MAX_RATE`], or if `channels` isn't 1 or 2.
    pub fn from_samples(samples: &[f32], rate: u32, channels: u32) -> Result<Self> {
        let problem = if !(1..=Self::MAX_RATE).contains(&rate) {
            Some(format!(
                "its rate must be from 1 to {}, not {rate}",
                Self::MAX_RATE
            ))
        } else if !(1..=2).contains(&channels) {
            Some(format!("it has 1 channel or 2, not {channels}"))
        } else if samples.is_empty() {
            Some("it has no samples".to_owned())
        } else if !samples.len().is_multiple_of(channels as usize) {
            Some(format!(
                "its {} samples aren't one for each of 2 channels",
                samples.len()
            ))
        } else {
            samples
                .iter()
                .position(|sample| !sample.is_finite())
                .map(|at| format!("sample {at} must be a number, not {}", samples[at]))
        };
        if let Some(problem) = problem {
            return Err(Error::Sound(problem));
        }
        let frames = if channels == 1 {
            samples
                .iter()
                .map(|&sample| Frame::from_mono(sample))
                .collect()
        } else {
            samples
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&[left, right]| Frame::new(left, right))
                .collect()
        };
        Ok(Self {
            data: StaticSoundData {
                sample_rate: rate,
                frames,
                settings: StaticSoundSettings::default(),
                slice: None,
            },
        })
    }

    /// The most samples a second a made sound can have.
    pub const MAX_RATE: u32 = 192_000;

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

/// A track's linear `volume` in decibels, or none, logging a warning, if it isn't a number:
/// infinity would fill the mix with NaNs.
fn track_volume(volume: f32) -> Option<Decibels> {
    if let Some(problem) = setting_problem("volume", volume) {
        log::warn!("couldn't change a volume: {problem}");
        return None;
    }
    Some(decibels(volume))
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
    use crate::music::{At, Pitch, SongEvent, SongFile};

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
    fn sounds_are_made_of_samples() {
        // A second of one channel, and half a second of two at the same rate.
        let samples: Vec<f32> = (0..8000).map(|i| (i as f32 * 0.3).sin()).collect();
        let mono = Sound::from_samples(&samples, 8000, 1).unwrap();
        assert!((mono.duration() - 1.0).abs() < 1e-6);
        assert_eq!(mono.data.frames[5], Frame::from_mono(samples[5]));
        let stereo = Sound::from_samples(&samples, 8000, 2).unwrap();
        assert!((stereo.duration() - 0.5).abs() < 1e-6);
        assert_eq!(stereo.data.frames[5], Frame::new(samples[10], samples[11]));

        let mut audio = Audio::silent();
        audio.play(&stereo);
        run(&mut audio, 0.3);
        assert_eq!(audio.sounds_playing(), 1);
        run(&mut audio, 0.3);
        assert_eq!(audio.sounds_playing(), 0);
    }

    #[test]
    fn samples_that_cant_be_a_sound_say_why() {
        let problem = |samples: &[f32], rate, channels| {
            Sound::from_samples(samples, rate, channels)
                .unwrap_err()
                .to_string()
        };
        assert!(problem(&[], 8000, 1).contains("no samples"));
        assert!(problem(&[0.0; 3], 8000, 2).contains("each of 2 channels"));
        assert!(problem(&[0.0, f32::NAN], 8000, 1).contains("sample 1 must be a number"));
        assert!(problem(&[0.0], 0, 1).contains("rate"));
        assert!(problem(&[0.0], Sound::MAX_RATE + 1, 1).contains("rate"));
        assert!(problem(&[0.0], 8000, 3).contains("1 channel or 2"));
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
    fn looping_sounds_play_until_their_voices_stop() {
        let mut audio = Audio::silent();
        let chime = sound("chime.wav");
        let looping = PlaySettings {
            looping: true,
            fade_in: 0.2,
            ..Default::default()
        };
        let hum = audio.play_with(&chime, looping);
        let once = audio.play(&chime);
        run(&mut audio, 1.0);
        assert!(audio.voice_playing(hum), "it loops past its end");
        assert!(!audio.voice_playing(once));
        audio.stop_voice(hum, 0.1);
        assert!(!audio.voice_playing(hum), "it's over once it's stopped");
        assert_eq!(audio.sounds_playing(), 0);
        run(&mut audio, 0.2);
        assert!(audio.sounds.is_empty(), "the mixer let it go once it faded");
    }

    #[test]
    fn paused_voices_wait_to_be_resumed() {
        let mut audio = Audio::silent();
        let chime = sound("chime.wav");
        let paused = audio.play(&chime);
        let playing = audio.play(&chime);
        run(&mut audio, 0.1);
        audio.pause_voice(paused, 0.0);
        assert!(audio.voice_paused(paused) && audio.voice_playing(paused));
        run(&mut audio, 0.5);
        assert!(audio.voice_playing(paused), "paused past its end");
        assert!(!audio.voice_playing(playing));
        assert_eq!(audio.sounds_playing(), 1, "paused sounds count");

        // It carries on from where it paused, 0.1 seconds into its 0.44.
        audio.resume_voice(paused, 0.0);
        assert!(!audio.voice_paused(paused));
        run(&mut audio, 0.2);
        assert!(audio.voice_playing(paused));
        run(&mut audio, 0.25);
        assert!(!audio.voice_playing(paused));
    }

    #[test]
    fn pausing_every_sound_leaves_new_ones_playing() {
        let mut audio = Audio::silent();
        let chime = sound("chime.wav");
        let before = audio.play(&chime);
        audio.pause_sounds(0.0);
        let after = audio.play(&chime);
        run(&mut audio, 0.6);
        assert!(audio.voice_paused(before));
        assert!(!audio.voice_playing(after));
        audio.resume_sounds(0.05);
        assert!(!audio.voice_paused(before));
        run(&mut audio, 0.6);
        assert!(!audio.voice_playing(before));
    }

    #[test]
    fn voices_change_as_they_play() {
        let mut audio = Audio::silent();
        let chime = sound("chime.wav");
        let changed = audio.play(&chime);
        let unchanged = audio.play(&chime);
        run(&mut audio, 0.1);
        audio.set_voice_speed(changed, 4.0, 0.0);
        audio.set_voice_volume(changed, 0.5, 0.1);
        audio.set_voice_pan(changed, -1.0, 0.1);
        // The rest of it, 0.34 seconds at its own speed, takes a quarter of that.
        run(&mut audio, 0.15);
        assert!(!audio.voice_playing(changed));
        assert!(audio.voice_playing(unchanged));
    }

    #[test]
    fn voices_that_are_over_change_nothing() {
        let mut audio = Audio::silent();
        let chime = sound("chime.wav");
        let old = audio.play(&chime);
        run(&mut audio, 0.6);
        let new = audio.play(&chime);
        assert_ne!(old, new, "voices aren't reused");
        audio.stop_voice(old, 0.0);
        audio.pause_voice(old, 0.0);
        audio.set_voice_volume(old, 0.0, 0.0);
        assert!(!audio.voice_playing(old));
        assert!(audio.voice_playing(new) && !audio.voice_paused(new));

        // Nor do values no sound could have.
        audio.set_voice_speed(new, 0.0, 0.0);
        audio.set_voice_pan(new, f32::NAN, 0.0);
        run(&mut audio, 0.1);
        assert!(audio.voice_playing(new));
        // A sound that couldn't play has finished already.
        let speed = PlaySettings {
            speed: -1.0,
            ..Default::default()
        };
        let refused = audio.play_with(&chime, speed);
        assert!(!audio.voice_playing(refused));
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
        let performance = audio.play_song(&song(), SongSettings::default()).unwrap();
        std::thread::sleep(Duration::from_millis(500));
        let position = audio.music.as_ref().unwrap().position();
        assert!(position > 0.2, "the device rendered {position}s of music");
        audio.advance(0.5);
        let beat = audio.song(performance).unwrap().position().beat;
        assert!(beat > 0.4, "the device rendered {beat} beats of the song");
    }

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

    /// A song at 120 beats a minute, so a beat is half a second, 30 updates: a bar of section
    /// `a`, then one of `b`, then it ends.
    fn song() -> Song {
        Song::new(&SongFile::parse(SONG, "test.song.luau").unwrap()).unwrap()
    }

    /// Runs `audio` for `updates` updates, giving what happened in `performance` in each.
    fn hear(audio: &mut Audio, performance: Performance, updates: usize) -> Vec<Vec<SongEvent>> {
        (0..updates)
            .map(|_| {
                audio.advance(FRAME);
                audio
                    .song(performance)
                    .map(|song| song.events().to_vec())
                    .unwrap_or_default()
            })
            .collect()
    }

    /// The first of `heard`'s updates in which `event` happened.
    fn when(heard: &[Vec<SongEvent>], event: SongEvent) -> Option<usize> {
        heard.iter().position(|events| events.contains(&event))
    }

    #[test]
    fn songs_say_what_happens_in_the_update_it_happens() {
        let mut audio = Audio::silent();
        let performance = audio.play_song(&song(), SongSettings::default()).unwrap();
        audio.song(performance).unwrap().listen("t", true).unwrap();
        let heard = hear(&mut audio, performance, 300);
        let c4 = SongEvent::Note {
            track: 0,
            pitch: Pitch::from_name("C4").unwrap(),
            velocity: 1.0,
        };
        let beat = |bar, beat| SongEvent::Beat { bar, beat };
        assert_eq!(heard[0], [SongEvent::Section(0), beat(1, 1), c4]);
        // Each in the update the mixer renders it in, which is up to a buffer late.
        assert_eq!(when(&heard, beat(1, 2)), Some(30));
        assert_eq!(when(&heard, SongEvent::Section(1)), Some(120));
        let end = when(&heard, SongEvent::End).unwrap();
        assert_eq!(end, 240);
        // The update after it ends is the last it plays.
        assert!(heard[end + 1..].iter().all(Vec::is_empty));
        assert!(audio.song(performance).is_none());
        assert_eq!(audio.songs_playing(), 0);
        assert!(audio.songs.is_empty());
    }

    #[test]
    fn songs_jump_and_change_tempo_when_asked() {
        let mut audio = Audio::silent();
        let jumping = audio.play_song(&song(), SongSettings::default()).unwrap();
        hear(&mut audio, jumping, 18);
        audio.song(jumping).unwrap().go_to("b", At::Beat).unwrap();
        // The next beat is half a second in, 30 updates from the start.
        let heard = hear(&mut audio, jumping, 30);
        assert_eq!(when(&heard, SongEvent::Section(1)), Some(30 - 18));
        assert_eq!(audio.song(jumping).unwrap().position().section, 1);

        let fast = audio.play_song(&song(), SongSettings::default()).unwrap();
        audio.song(fast).unwrap().set_tempo(240.0, At::Now, 0.0);
        let heard = hear(&mut audio, fast, 70);
        assert_eq!(when(&heard, SongEvent::Section(1)), Some(60));
        assert_eq!(audio.song(fast).unwrap().position().tempo, 240.0);
        assert_eq!(audio.songs_playing(), 2);
    }

    #[test]
    fn songs_pause_resume_and_stop() {
        let mut audio = Audio::silent();
        let performance = audio.play_song(&song(), SongSettings::default()).unwrap();
        hear(&mut audio, performance, 10);
        let playing = audio.song(performance).unwrap();
        playing.pause(0.0);
        assert!(playing.is_paused());
        hear(&mut audio, performance, 5);
        let paused_at = audio.song(performance).unwrap().position().beat;
        assert!(paused_at > 0.3);
        hear(&mut audio, performance, 30);
        assert_eq!(audio.song(performance).unwrap().position().beat, paused_at);
        audio.song(performance).unwrap().resume(0.0);
        hear(&mut audio, performance, 10);
        assert!(audio.song(performance).unwrap().position().beat > paused_at);

        audio.song(performance).unwrap().stop(0.1);
        assert!(
            audio.song(performance).is_none(),
            "it's over once it's stopped"
        );
        assert_eq!(audio.songs_playing(), 0);
        audio.advance(FRAME);
        assert!(audio.songs.is_empty());
    }

    #[test]
    fn songs_start_where_theyre_asked_and_reset_stops_them() {
        let mut audio = Audio::silent();
        let first = audio.play_song(&song(), SongSettings::default()).unwrap();
        audio.advance(FRAME);
        audio.reset();
        assert!(audio.song(first).is_none());
        let settings = SongSettings {
            section: Some("b".to_owned()),
            beat: 2.0,
            ..Default::default()
        };
        let second = audio.play_song(&song(), settings).unwrap();
        assert_ne!(first, second, "performances aren't reused");
        let start = audio.song(second).unwrap().position();
        assert_eq!((start.section, start.beat), (1, 2.0));
        let heard = hear(&mut audio, second, 1);
        assert_eq!(
            heard[0][..2],
            [SongEvent::Section(1), SongEvent::Beat { bar: 1, beat: 3 }]
        );
        assert!(
            audio
                .play_song(
                    &song(),
                    SongSettings {
                        section: Some("chorus".to_owned()),
                        ..Default::default()
                    }
                )
                .is_err()
        );
    }

    #[test]
    fn volumes_convert_to_decibels() {
        assert_eq!(decibels(1.0), Decibels::IDENTITY);
        assert!((decibels(0.5).0 + 6.02).abs() < 0.01);
        assert_eq!(decibels(0.0), Decibels::SILENCE);
        assert_eq!(decibels(1e-9), Decibels::SILENCE);
        assert_eq!(decibels(-1.0), Decibels::SILENCE);
    }

    #[test]
    fn tracks_take_only_volumes_that_are_numbers() {
        assert_eq!(track_volume(1.0), Some(Decibels::IDENTITY));
        for volume in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
            assert_eq!(track_volume(volume), None, "{volume}");
        }
        let mut audio = Audio::silent();
        audio.set_volume(f32::INFINITY);
        audio.set_sound_volume(f32::NAN);
        audio.set_music_volume(f32::INFINITY);
        audio.advance(FRAME);
    }
}
