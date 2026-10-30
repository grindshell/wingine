use std::sync::Arc;

use super::Sprite;

/// Sprites shown one after another at a steady pace, such as a walk cycle.
///
/// An animation doesn't keep time itself. The game keeps the time it has been playing, such as
/// how long a character has been walking, and asks for the frame to draw with
/// [`Animation::frame`], so one animation can be shared by everything that plays it.
///
/// Cloning is cheap; the frames are shared.
#[derive(Clone, Debug)]
pub struct Animation {
    frames: Arc<[Sprite]>,
    frame_time: f32,
    looping: bool,
}

impl Animation {
    /// An animation showing each of `frames` in turn for `frame_time` seconds, then starting
    /// over.
    ///
    /// # Panics
    ///
    /// If there are no frames, or `frame_time` isn't a positive number of seconds.
    pub fn new(frames: impl IntoIterator<Item = Sprite>, frame_time: f32) -> Self {
        Self::try_new(frames.into_iter().collect(), frame_time)
            .unwrap_or_else(|err| panic!("{err}"))
    }

    /// [`Animation::new`], or what's wrong with its frames or frame time.
    pub(crate) fn try_new(frames: Vec<Sprite>, frame_time: f32) -> Result<Self, String> {
        if frames.is_empty() {
            return Err("an animation needs at least one frame".into());
        }
        if !(frame_time.is_finite() && frame_time > 0.0) {
            return Err(format!(
                "frame time must be a positive number of seconds, not {frame_time}"
            ));
        }
        Ok(Self {
            frames: frames.into(),
            frame_time,
            looping: true,
        })
    }

    /// Returns this animation playing once and then holding its last frame, rather than starting
    /// over.
    pub fn once(mut self) -> Self {
        self.looping = false;
        self
    }

    pub fn frames(&self) -> &[Sprite] {
        &self.frames
    }

    /// Seconds each frame shows for.
    pub fn frame_time(&self) -> f32 {
        self.frame_time
    }

    /// Whether it starts over after its last frame, rather than holding it.
    pub fn is_looping(&self) -> bool {
        self.looping
    }

    /// Seconds it takes to show every frame once.
    pub fn duration(&self) -> f32 {
        self.frame_time * self.frames.len() as f32
    }

    /// The frame showing `time` seconds after the animation started. Before it starts, that's
    /// the first frame.
    pub fn frame(&self, time: f32) -> &Sprite {
        &self.frames[self.frame_index(time)]
    }

    /// The index in [`Animation::frames`] of the frame showing `time` seconds after the animation
    /// started.
    pub fn frame_index(&self, time: f32) -> usize {
        frame_index(self.frames.len(), self.frame_time, self.looping, time)
    }

    /// Whether an animation that plays once has finished showing its last frame, `time` seconds
    /// after it started. One that loops never finishes.
    pub fn is_finished(&self, time: f32) -> bool {
        !self.looping && time >= self.duration()
    }

    /// Returns this animation with every frame mirrored left-to-right.
    pub fn flipped_x(self) -> Self {
        self.map_frames(Sprite::flipped_x)
    }

    /// Returns this animation with every frame mirrored top-to-bottom.
    pub fn flipped_y(self) -> Self {
        self.map_frames(Sprite::flipped_y)
    }

    fn map_frames(self, f: impl FnMut(Sprite) -> Sprite) -> Self {
        Self {
            frames: self.frames.iter().cloned().map(f).collect(),
            ..self
        }
    }
}

/// The index of the frame showing `time` seconds into an animation of `count` frames, each shown
/// for `frame_time` seconds.
fn frame_index(count: usize, frame_time: f32, looping: bool, time: f32) -> usize {
    // `max` turns NaN into 0 too, and `as` rounds down, saturating for times too large to count.
    let shown = (time.max(0.0) / frame_time) as usize;
    if looping {
        shown % count
    } else {
        shown.min(count - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_follow_each_other_and_loop() {
        let indices: Vec<_> = [0.0, 0.09, 0.1, 0.25, 0.35, 0.45]
            .into_iter()
            .map(|time| frame_index(3, 0.1, true, time))
            .collect();
        assert_eq!(indices, [0, 0, 1, 2, 0, 1]);
    }

    #[test]
    fn playing_once_holds_the_last_frame() {
        assert_eq!(frame_index(3, 0.1, false, 0.25), 2);
        assert_eq!(frame_index(3, 0.1, false, 10.0), 2);
    }

    #[test]
    fn times_before_the_start_show_the_first_frame() {
        for time in [-1.0, f32::NEG_INFINITY, f32::NAN] {
            assert_eq!(frame_index(3, 0.1, true, time), 0, "{time}");
        }
        assert_eq!(frame_index(3, 0.1, false, f32::INFINITY), 2);
    }
}
