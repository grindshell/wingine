//! Clips: keyframes on nodes' properties, for posing rigs.

/// An animation of a rig, such as a walk: tracks of keyframes, each changing one property of one
/// node over time. Like an [`Animation`](crate::graphics::Animation), it doesn't keep time: games
/// keep how long it has played and pose nodes with it at that time, with
/// [`Scene::pose`](super::Scene::pose), so one clip poses any number of rigs.
///
/// Tracks name their nodes by path from the node posed, so a clip poses every copy of a rig, such
/// as each instance of a character's scene file.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    /// Seconds it lasts, above 0.
    pub duration: f32,
    /// Whether it starts over after its duration, rather than holding its last pose.
    pub looping: bool,
    pub tracks: Vec<Track>,
}

/// The keyframes of one property of one node.
#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    /// The node's path from the node posed, such as `"torso/arm_l"`, or `""` for that node itself.
    pub node: String,
    pub keys: Keys,
}

/// A track's property, and its keys: a time in seconds from the clip's start and the value then,
/// in order of time. Before the first key a property has the first key's value, and after the last,
/// the last key's.
#[derive(Clone, Debug, PartialEq)]
pub enum Keys {
    /// One of the node's numbers, from key to key as `interpolation` has it.
    Number {
        property: Property,
        interpolation: Interpolation,
        keys: Vec<(f32, f32)>,
    },
    /// Whether the node is visible, which steps from key to key.
    Visible(Vec<(f32, bool)>),
    /// The cell of its sheet a sprite node shows, its column and row, which steps from key to key.
    Cell(Vec<(f32, [u32; 2])>),
}

/// What a track sets its property to at some time: one of its keys' values, or for numbers, one
/// between them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum KeyValue {
    Number(f32),
    Visible(bool),
    Cell([u32; 2]),
}

/// A number of a node a track changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Property {
    X,
    Y,
    Rotation,
    ScaleX,
    ScaleY,
}

/// How a number gets from one key to the next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Interpolation {
    /// In a straight line, at an even pace.
    #[default]
    Linear,
    /// Not at all: it keeps one key's value until the next key's time.
    Step,
}

impl Clip {
    /// The time within the clip `time` seconds after it started: wrapped to its duration if it
    /// loops, or held between its start and its end.
    pub fn time_in(&self, time: f32) -> f32 {
        if self.looping {
            time.rem_euclid(self.duration)
        } else {
            time.clamp(0.0, self.duration)
        }
    }

    /// Whether a clip that doesn't loop has reached its end, `time` seconds after it started. One
    /// that loops never does.
    pub fn is_finished(&self, time: f32) -> bool {
        !self.looping && time >= self.duration
    }
}

impl Keys {
    /// The property's name in scene files, such as `"rotation"`.
    pub fn property_name(&self) -> &'static str {
        match self {
            Self::Number { property, .. } => property.name(),
            Self::Visible(_) => "visible",
            Self::Cell(_) => "cell",
        }
    }

    /// What the keys set the property to `time` seconds into the clip, as posing does, if there
    /// are any keys.
    pub fn value_at(&self, time: f32) -> Option<KeyValue> {
        match self {
            Self::Number {
                interpolation,
                keys,
                ..
            } => number(keys, *interpolation, time).map(KeyValue::Number),
            Self::Visible(keys) => step(keys, time).map(KeyValue::Visible),
            Self::Cell(keys) => step(keys, time).map(KeyValue::Cell),
        }
    }

    /// The times of the keys, in order.
    pub fn times(&self) -> Vec<f32> {
        match self {
            Self::Number { keys, .. } => keys.iter().map(|key| key.0).collect(),
            Self::Visible(keys) => keys.iter().map(|key| key.0).collect(),
            Self::Cell(keys) => keys.iter().map(|key| key.0).collect(),
        }
    }
}

impl Property {
    pub const ALL: [Property; 5] = [Self::X, Self::Y, Self::Rotation, Self::ScaleX, Self::ScaleY];

    /// Its name in scene files, as a node's field: `"x"`, `"y"`, `"rotation"`, `"scale_x"` or
    /// `"scale_y"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Y => "y",
            Self::Rotation => "rotation",
            Self::ScaleX => "scale_x",
            Self::ScaleY => "scale_y",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|property| property.name() == name)
    }
}

impl Interpolation {
    /// Its name in scene files: `"linear"` or `"step"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Step => "step",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [Self::Linear, Self::Step]
            .into_iter()
            .find(|interpolation| interpolation.name() == name)
    }
}

/// The index of the last of `keys` at or before `time`, if any is.
fn last_reached<T>(keys: &[(f32, T)], time: f32) -> Option<usize> {
    keys.partition_point(|key| key.0 <= time).checked_sub(1)
}

/// The value of stepped `keys` at `time`, if there are any keys.
pub(super) fn step<T: Copy>(keys: &[(f32, T)], time: f32) -> Option<T> {
    let first = keys.first()?;
    Some(last_reached(keys, time).map_or(first.1, |i| keys[i].1))
}

/// The value of number `keys` at `time`, if there are any keys.
pub(super) fn number(keys: &[(f32, f32)], interpolation: Interpolation, time: f32) -> Option<f32> {
    let first = keys.first()?;
    let Some(i) = last_reached(keys, time) else {
        return Some(first.1);
    };
    let (from_time, from) = keys[i];
    let Some(&(to_time, to)) = keys.get(i + 1) else {
        return Some(from);
    };
    Some(match interpolation {
        Interpolation::Step => from,
        Interpolation::Linear => from + (to - from) * ((time - from_time) / (to_time - from_time)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_ease_between_keys_and_hold_beyond_them() {
        let keys = [(0.5, 1.0), (1.0, 3.0), (2.0, -1.0)];
        let at = |time| number(&keys, Interpolation::Linear, time).unwrap();
        assert_eq!(at(0.0), 1.0, "before the first key");
        assert_eq!(at(0.5), 1.0);
        assert_eq!(at(0.75), 2.0);
        assert_eq!(at(1.0), 3.0);
        assert_eq!(at(1.5), 1.0);
        assert_eq!(at(2.0), -1.0);
        assert_eq!(at(9.0), -1.0, "after the last key");
        let stepped = |time| number(&keys, Interpolation::Step, time).unwrap();
        assert_eq!(stepped(0.99), 1.0);
        assert_eq!(stepped(1.0), 3.0);
        assert_eq!(stepped(1.99), 3.0);
        assert_eq!(number(&[], Interpolation::Linear, 1.0), None);
    }

    #[test]
    fn steps_keep_a_value_until_the_next_key() {
        let keys = [(0.0, [0, 0]), (2.9, [1, 0]), (3.0, [0, 0])];
        assert_eq!(step(&keys, -1.0), Some([0, 0]));
        assert_eq!(step(&keys, 2.89), Some([0, 0]));
        assert_eq!(step(&keys, 2.95), Some([1, 0]));
        assert_eq!(step(&keys, 3.0), Some([0, 0]));
        assert_eq!(step::<bool>(&[], 3.0), None);
    }

    #[test]
    fn tracks_give_their_values_as_posing_does() {
        let keys = Keys::Number {
            property: Property::Rotation,
            interpolation: Interpolation::Linear,
            keys: vec![(0.0, 0.0), (1.0, 2.0)],
        };
        assert_eq!(keys.value_at(0.25), Some(KeyValue::Number(0.5)));
        assert_eq!(keys.times(), [0.0, 1.0]);
        let keys = Keys::Cell(vec![(0.0, [1, 0]), (0.5, [2, 0])]);
        assert_eq!(keys.value_at(0.75), Some(KeyValue::Cell([2, 0])));
        assert_eq!(Keys::Visible(Vec::new()).value_at(0.0), None);
    }

    #[test]
    fn clips_loop_or_hold_their_end() {
        let mut clip = Clip {
            duration: 2.0,
            looping: true,
            tracks: Vec::new(),
        };
        assert_eq!(clip.time_in(2.5), 0.5);
        assert_eq!(clip.time_in(-0.5), 1.5);
        assert!(!clip.is_finished(10.0));
        clip.looping = false;
        assert_eq!(clip.time_in(2.5), 2.0);
        assert_eq!(clip.time_in(-0.5), 0.0);
        assert!(!clip.is_finished(1.9));
        assert!(clip.is_finished(2.0));
    }

    #[test]
    fn names_are_as_files_have_them() {
        for property in Property::ALL {
            assert_eq!(Property::from_name(property.name()), Some(property));
        }
        assert_eq!(Property::from_name("scale_y"), Some(Property::ScaleY));
        assert_eq!(Property::from_name("visible"), None);
        assert_eq!(Interpolation::from_name("step"), Some(Interpolation::Step));
        assert_eq!(Interpolation::from_name("smooth"), None);
    }
}
