//! Rendering songs ahead of time, and baking them to WAV files.
//!
//! A song renders once through, from its start along each section's `next`, until it ends or
//! would play a section it has played already. One that loops back to the section it started
//! with renders as one pass of its loop, with what rings past the end mixed into its start, so
//! played over and over, as music is, it loops without a seam. Rendering one section renders it
//! as a loop the same way.

use std::{path::Path, sync::Arc};

use super::{Sequencer, Song, SongEvent};
use crate::{Error, Result, audio::Sound, save};

/// The longest a song renders, in seconds.
const MAX_SECONDS: f64 = 3600.0;

/// Frames rendered at a time.
const CHUNK: usize = 4096;

/// A song rendered ahead of time.
#[derive(Clone, Debug, PartialEq)]
pub struct Rendered {
    /// Its samples, from -1 to 1 but where they clip: for each frame, the left then the right.
    pub samples: Vec<f32>,
    /// Frames a second.
    pub rate: u32,
    /// Whether it loops: it leads back into its start, which holds what rang past its end.
    pub loops: bool,
}

impl Rendered {
    /// How long it lasts, in seconds.
    pub fn duration(&self) -> f64 {
        self.samples.len() as f64 / 2.0 / f64::from(self.rate)
    }

    /// It as a sound, to play.
    pub fn to_sound(&self) -> Result<Sound> {
        Sound::from_samples(&self.samples, self.rate, 2)
    }

    /// It as the bytes of a WAV file, 16-bit stereo, clipping samples past -1 and 1.
    pub fn to_wav(&self) -> Vec<u8> {
        let data = u32::try_from(self.samples.len() * 2).unwrap_or(u32::MAX - 36);
        let mut wav = Vec::with_capacity(44 + data as usize);
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(36 + data).to_le_bytes());
        wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16_u32.to_le_bytes());
        // Integer samples, two channels, at the rate, two bytes a sample and four a frame.
        wav.extend_from_slice(&1_u16.to_le_bytes());
        wav.extend_from_slice(&2_u16.to_le_bytes());
        wav.extend_from_slice(&self.rate.to_le_bytes());
        wav.extend_from_slice(&(self.rate * 4).to_le_bytes());
        wav.extend_from_slice(&4_u16.to_le_bytes());
        wav.extend_from_slice(&16_u16.to_le_bytes());
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&data.to_le_bytes());
        for sample in &self.samples {
            let sample = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
            wav.extend_from_slice(&sample.to_le_bytes());
        }
        wav
    }
}

impl Song {
    /// Renders the song once through, `rate` frames a second: from its start along each section's
    /// `next`, until it ends or would play a section it has played already. One that loops back to
    /// the section it started with renders as one pass of its loop, with what rings past its end
    /// mixed into its start, so played over and over, as music is, it loops without a seam. With
    /// `section`, it renders that section alone, as a loop.
    ///
    /// Fails if there's no such section, if `rate` isn't from 1 to [`Sound::MAX_RATE`], or if
    /// the song would last more than an hour.
    pub fn render(&self, rate: u32, section: Option<&str>) -> Result<Rendered> {
        if !(1..=Sound::MAX_RATE).contains(&rate) {
            return Err(Error::InvalidSong(format!(
                "a song renders from 1 to {} frames a second, not {rate}",
                Sound::MAX_RATE
            )));
        }
        let data = self.data();
        let start = match section {
            Some(name) => data.section(name).ok_or_else(|| {
                Error::InvalidSong(format!("the song has no section `{name}` to render"))
            })?,
            None => data.start,
        };
        let mut sequencer = Sequencer::new(Arc::clone(data), rate, start, 0.0);
        sequencer.play_once();
        if section.is_some() {
            sequencer.set_next(Some(start));
        }
        let limit = (MAX_SECONDS * f64::from(rate)) as u64;
        let mut frames: Vec<[f32; 2]> = Vec::new();
        let mut end = None;
        while !sequencer.is_finished() {
            if sequencer.frame() >= limit {
                return Err(Error::InvalidSong(
                    "the song lasts more than an hour, too long to render".to_owned(),
                ));
            }
            let mut chunk = [[0.0; 2]; CHUNK];
            sequencer.render(&mut chunk, &mut |heard| {
                if heard.event == SongEvent::End {
                    end = Some(heard.frame);
                }
            });
            frames.extend_from_slice(&chunk);
        }
        frames.truncate(end.unwrap_or(sequencer.frame()) as usize);
        let looped = sequencer
            .looped_at()
            .filter(|&(at, section)| section == start && at > 0);
        if let Some((at, _)) = looped {
            let at = at as usize;
            for i in at..frames.len() {
                let [left, right] = frames[i];
                let start = &mut frames[(i - at) % at];
                start[0] += left;
                start[1] += right;
            }
            frames.truncate(at);
        }
        Ok(Rendered {
            samples: frames.into_iter().flatten().collect(),
            rate,
            loops: looped.is_some(),
        })
    }

    /// Renders the song, or its section `section`, as [`Song::render`] does, and writes it to
    /// the file at `path` as a 16-bit stereo WAV, which [`Audio::play_music`] and [`Sound::load`]
    /// play. The file is written in full to one of its own first, then takes the old one's place.
    ///
    /// [`Audio::play_music`]: crate::audio::Audio::play_music
    pub fn bake(
        &self,
        path: impl AsRef<Path>,
        rate: u32,
        section: Option<&str>,
    ) -> Result<Rendered> {
        let rendered = self.render(rate, section)?;
        save::write_replacing(path.as_ref(), &rendered.to_wav())?;
        Ok(rendered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::SongFile;

    const RATE: u32 = 48_000;

    fn song(source: &str) -> Song {
        Song::new(&SongFile::parse(source, "test.song.luau").unwrap()).unwrap()
    }

    /// At 120 beats a minute, a beat lasts half a second.
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

    #[test]
    fn songs_render_once_through_and_ring_out() {
        let rendered = song(SONG).render(RATE, None).unwrap();
        assert!(!rendered.loops);
        // Two bars, two seconds each, then nothing rings on: the last note ended in section b.
        assert_eq!(rendered.samples.len(), 4 * RATE as usize * 2);
        assert!((rendered.duration() - 4.0).abs() < 1e-9);
        let frame = |seconds: f64| rendered.samples[2 * (seconds * f64::from(RATE)) as usize];
        assert!(frame(0.25) != 0.0 && frame(1.6) != 0.0 && frame(2.4) != 0.0);
        assert_eq!(frame(3.0), 0.0);
    }

    #[test]
    fn songs_that_loop_render_one_seamless_pass() {
        let looping = song(&SONG.replace("b = { bars = 1 }", "b = { bars = 1, next = \"a\" }"));
        let rendered = looping.render(RATE, None).unwrap();
        assert!(rendered.loops);
        assert_eq!(rendered.samples.len(), 4 * RATE as usize * 2);

        // A note held across the loop rings on into the start.
        let held = song(
            &SONG
                .replace("{ 3, \"G4\", 2 }", "{ 3, \"G4\", 4 }")
                .replace("next = \"b\"", "next = \"a\""),
        );
        let rendered = held.render(RATE, None).unwrap();
        assert!(rendered.loops);
        assert_eq!(rendered.samples.len(), 2 * RATE as usize * 2);
        // Half a second in, after the first note's beat, the G rung from the last bar.
        let at = |seconds: f64| rendered.samples[2 * (seconds * f64::from(RATE)) as usize];
        assert!(at(0.75) != 0.0);
        let unlooped = held.render(RATE, Some("b"));
        assert!(unlooped.is_ok());
    }

    #[test]
    fn sections_render_as_loops() {
        let song = song(SONG);
        let rendered = song.render(RATE, Some("a")).unwrap();
        assert!(rendered.loops);
        assert_eq!(rendered.samples.len(), 2 * RATE as usize * 2);
        // What rang past the bar is at the start: the G held two beats from beat 3.
        let once = song.render(RATE, None).unwrap();
        let beat = RATE as usize;
        assert_ne!(rendered.samples[..beat], once.samples[..beat]);
        assert_eq!(
            rendered.samples[beat + 6000..2 * beat],
            once.samples[beat + 6000..2 * beat]
        );
        let silent = song.render(RATE, Some("b")).unwrap();
        assert!(silent.samples.iter().all(|&sample| sample == 0.0));
    }

    #[test]
    fn what_cant_render_says_why() {
        let song = song(SONG);
        let err = song.render(RATE, Some("chorus")).unwrap_err().to_string();
        assert!(err.contains("no section `chorus`"), "{err}");
        let err = song.render(0, None).unwrap_err().to_string();
        assert!(err.contains("not 0"), "{err}");
    }

    #[test]
    fn bakes_are_wav_files_that_play() {
        let dir = std::env::temp_dir().join(format!("wingine-bake-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("song.wav");
        let rendered = song(SONG).bake(&path, 22_050, None).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), 44 + rendered.samples.len() * 2);
        assert_eq!(&bytes[..4], b"RIFF");
        let sound = Sound::load(&path).unwrap();
        assert!(
            (sound.duration() - 4.0).abs() < 0.01,
            "{}",
            sound.duration()
        );
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(rendered.to_sound().is_ok());
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

    /// A song with every kind of instrument, a tempo slide and a loop renders to the same bits
    /// on Windows and Linux, debug and release. A change to how songs sound changes it.
    #[test]
    fn songs_render_the_same_on_every_platform() {
        let song = song(
            r#"return {
            format = 1,
            tempo = 150,
            meter = { 3, 4 },
            start = "a",
            instruments = {
                bass = { wave = "saw", decay = 0.1, sustain = 0.6, release = 0.05,
                         filter = { cutoff = 500, resonance = 0.6, sweep = 2, sweep_time = 0.1 } },
                hat = { wave = "noise", decay = 0.04, sustain = 0, filter = { kind = "high", cutoff = 6000 } },
                kick = { decay = 0.2, sustain = 0, sweep = 30, sweep_time = 0.05 },
                lead = { wave = "square", duty = 0.3, attack = 0.02, release = 0.1, pan = 0.3,
                         vibrato = { rate = 6, depth = 0.2, delay = 0.1 } },
            },
            tracks = {
                { name = "kick", instrument = "kick" },
                { name = "hat", instrument = "hat", pan = -0.4 },
                { name = "bass", instrument = "bass", volume = 0.7 },
                { name = "lead", instrument = "lead", volume = 0.5 },
            },
            patterns = {
                beat = { bars = 1, notes = { { 0, "C2", 0.5 }, { 1.5, "C2", 0.5, 0.7 } } },
                hats = { bars = 1, notes = { { 0.5, "C7", 0.25 }, { 1.5, "C7", 0.25 }, { 2.5, "C7", 0.25 } } },
                line = { bars = 2, notes = { { 0, "A1", 1 }, { 1, "A1", 0.5 }, { 3, "E2", 2.5 } } },
                tune = { bars = 2, notes = { { 0, "E5", 1.5 }, { 0, "A4", 1.5 }, { 2, "G5", 2, 0.6 } } },
            },
            sections = {
                a = { bars = 2, tracks = { kick = "beat", hat = "hats", bass = "line" }, next = "b" },
                b = { bars = 2, tracks = { kick = "beat", hat = "hats", bass = "line", lead = "tune" }, next = "a" },
            },
        }"#,
        );
        let rendered = song.render(22_050, None).unwrap();
        assert!(rendered.loops);
        assert_eq!(hash(&rendered.samples), 0xa90d_6e7e_a0c7_b864);
    }
}
