//! Changes to a scene file's clips: adding, renaming and removing them, and putting keys into
//! their tracks, moving keys and taking them out. Like [`edit`](crate::edit)'s, these work on the
//! file alone. Tracks name their nodes by paths of names from the top of the scene, as the clips
//! of a scene file do.

use wingine::scene::{
    Clip, Interpolation, KeyValue, Keys, KindDef, NodeDef, Property, SceneFile, Track,
};

use crate::edit;

/// The properties of a node tracks change, as scene files name them.
pub const PROPERTIES: [&str; 7] = [
    "x", "y", "rotation", "scale_x", "scale_y", "visible", "cell",
];

/// How near two times are to be one key's, in seconds: nearer than times are written.
const SAME_TIME: f32 = 1e-4;

/// A key's time as files write it, to a ten-thousandth of a second, so steps snapped to and times
/// typed stay as short as they were.
pub fn tidy_time(time: f32) -> f32 {
    (time * 10_000.0).round() / 10_000.0
}

/// `node`'s value of `property`, one of [`PROPERTIES`], if it has the property: only sprites have
/// cells.
pub fn value(node: &NodeDef, property: &str) -> Option<KeyValue> {
    if let Some(property) = Property::from_name(property) {
        let number = match property {
            Property::X => node.x,
            Property::Y => node.y,
            Property::Rotation => node.rotation,
            Property::ScaleX => node.scale_x,
            Property::ScaleY => node.scale_y,
        };
        return Some(KeyValue::Number(number));
    }
    match (property, &node.kind) {
        ("visible", _) => Some(KeyValue::Visible(node.visible)),
        ("cell", KindDef::Sprite(sprite)) => Some(KeyValue::Cell(sprite.cell)),
        _ => None,
    }
}

/// Sets `node`'s `property` to `value`, if it has the property and the value is one for it.
pub fn set_value(node: &mut NodeDef, property: &str, value: KeyValue) {
    match (Property::from_name(property), value) {
        (Some(property), KeyValue::Number(number)) => {
            let field = match property {
                Property::X => &mut node.x,
                Property::Y => &mut node.y,
                Property::Rotation => &mut node.rotation,
                Property::ScaleX => &mut node.scale_x,
                Property::ScaleY => &mut node.scale_y,
            };
            *field = number;
        }
        (None, KeyValue::Visible(visible)) if property == "visible" => node.visible = visible,
        (None, KeyValue::Cell(cell)) if property == "cell" => {
            if let KindDef::Sprite(sprite) = &mut node.kind {
                sprite.cell = cell;
            }
        }
        _ => {}
    }
}

/// The index of `clip`'s track of `property` of the node at `node`, a path of names, if it has
/// one.
pub fn track(clip: &Clip, node: &str, property: &str) -> Option<usize> {
    clip.tracks
        .iter()
        .position(|track| track.node == node && track.keys.property_name() == property)
}

/// Puts a key of `value` at `time` into `clip`'s track of `property` of `node`, a path of names,
/// replacing a key at that time, and adding the track if there isn't one. Fails if `value` isn't
/// one for the property.
pub fn set_key(
    clip: &mut Clip,
    node: &str,
    property: &str,
    time: f32,
    value: KeyValue,
) -> Result<(), String> {
    let time = tidy_time(time.clamp(0.0, clip.duration));
    if let Some(i) = track(clip, node, property) {
        let keys = &mut clip.tracks[i].keys;
        match (keys, value) {
            (Keys::Number { keys, .. }, KeyValue::Number(number)) => put(keys, time, number),
            (Keys::Visible(keys), KeyValue::Visible(visible)) => put(keys, time, visible),
            (Keys::Cell(keys), KeyValue::Cell(cell)) => put(keys, time, cell),
            (keys, value) => {
                return Err(format!(
                    "a track of `{}` can't have a key of {value:?}",
                    keys.property_name()
                ));
            }
        };
        return Ok(());
    }
    let keys = match (Property::from_name(property), value) {
        (Some(property), KeyValue::Number(number)) => Keys::Number {
            property,
            interpolation: Interpolation::Linear,
            keys: vec![(time, number)],
        },
        (None, KeyValue::Visible(visible)) if property == "visible" => {
            Keys::Visible(vec![(time, visible)])
        }
        (None, KeyValue::Cell(cell)) if property == "cell" => Keys::Cell(vec![(time, cell)]),
        _ => return Err(format!("`{property}` can't have a key of {value:?}")),
    };
    clip.tracks.push(Track {
        node: node.to_owned(),
        keys,
    });
    Ok(())
}

/// Puts `value` at `time` among `keys`, in order of time, replacing a key at that time, and
/// returns its index.
fn put<T>(keys: &mut Vec<(f32, T)>, time: f32, value: T) -> usize {
    if let Some(i) = keys.iter().position(|key| (key.0 - time).abs() < SAME_TIME) {
        keys[i].1 = value;
        return i;
    }
    let i = keys.partition_point(|key| key.0 < time);
    keys.insert(i, (time, value));
    i
}

/// Takes the key at `key` out of the track at `track`, and the track out of the clip if that was
/// its last key, since a track has at least one.
pub fn remove_key(clip: &mut Clip, track: usize, key: usize) {
    let Some(keys) = clip.tracks.get_mut(track).map(|track| &mut track.keys) else {
        return;
    };
    let left = match keys {
        Keys::Number { keys, .. } => remove(keys, key),
        Keys::Visible(keys) => remove(keys, key),
        Keys::Cell(keys) => remove(keys, key),
    };
    if left == 0 {
        clip.tracks.remove(track);
    }
}

/// Takes the key at `i` out of `keys`, if it's there, returning how many are left.
fn remove<T>(keys: &mut Vec<(f32, T)>, i: usize) -> usize {
    if i < keys.len() {
        keys.remove(i);
    }
    keys.len()
}

/// Moves the key at `key` of the track at `track` to `time`, within the clip, replacing a key
/// there. Returns its index once it's moved, if it's there.
pub fn move_key(clip: &mut Clip, track: usize, key: usize, time: f32) -> Option<usize> {
    let time = tidy_time(time.clamp(0.0, clip.duration));
    match &mut clip.tracks.get_mut(track)?.keys {
        Keys::Number { keys, .. } => move_in(keys, key, time),
        Keys::Visible(keys) => move_in(keys, key, time),
        Keys::Cell(keys) => move_in(keys, key, time),
    }
}

fn move_in<T>(keys: &mut Vec<(f32, T)>, key: usize, time: f32) -> Option<usize> {
    if key >= keys.len() {
        return None;
    }
    let (_, value) = keys.remove(key);
    Some(put(keys, time, value))
}

/// The time of `clip`'s last key, which its duration can't be shorter than.
pub fn last_key(clip: &Clip) -> f32 {
    clip.tracks
        .iter()
        .flat_map(|track| track.keys.times())
        .fold(0.0, f32::max)
}

/// Sets how the number track at `track` gets from key to key. Other tracks always step.
pub fn set_interpolation(clip: &mut Clip, track: usize, to: Interpolation) {
    if let Some(Keys::Number { interpolation, .. }) =
        clip.tracks.get_mut(track).map(|track| &mut track.keys)
    {
        *interpolation = to;
    }
}

/// Adds a clip a second long, with no tracks, named `name`, or if a clip has that name, with a
/// number after it. Returns its name.
pub fn add_clip(file: &mut SceneFile, name: &str) -> String {
    let name = edit::unique(name, |name| file.clips.contains_key(name));
    file.clips.insert(
        name.clone(),
        Clip {
            duration: 1.0,
            looping: false,
            tracks: Vec::new(),
        },
    );
    name
}

/// Renames the clip `from` to `to`, a name no other clip has.
pub fn rename_clip(file: &mut SceneFile, from: &str, to: &str) -> Result<(), String> {
    if to.is_empty() {
        return Err("a clip needs a name".into());
    }
    if from == to {
        return Ok(());
    }
    if file.clips.contains_key(to) {
        return Err(format!("another clip is named `{to}`"));
    }
    let clip = file.clips.remove(from).ok_or("there's no such clip")?;
    file.clips.insert(to.to_owned(), clip);
    Ok(())
}

#[cfg(test)]
mod tests {
    use wingine::{graphics::Color, scene::SpriteDef};

    use super::*;

    fn clip() -> Clip {
        Clip {
            duration: 2.0,
            looping: false,
            tracks: Vec::new(),
        }
    }

    fn times(clip: &Clip, track: usize) -> Vec<f32> {
        clip.tracks[track].keys.times()
    }

    #[test]
    fn keys_go_in_order_and_replace_keys_at_their_time() {
        let mut clip = clip();
        set_key(&mut clip, "arm", "rotation", 1.0, KeyValue::Number(0.5)).unwrap();
        set_key(&mut clip, "arm", "rotation", 0.25, KeyValue::Number(-0.5)).unwrap();
        set_key(&mut clip, "arm", "rotation", 1.00001, KeyValue::Number(1.0)).unwrap();
        // Times are kept within the clip.
        set_key(&mut clip, "arm", "rotation", 5.0, KeyValue::Number(0.0)).unwrap();
        assert_eq!(clip.tracks.len(), 1);
        assert_eq!(
            clip.tracks[0].keys,
            Keys::Number {
                property: Property::Rotation,
                interpolation: Interpolation::Linear,
                keys: vec![(0.25, -0.5), (1.0, 1.0), (2.0, 0.0)],
            }
        );

        // Another property, or another node, is another track.
        set_key(&mut clip, "arm", "visible", 0.0, KeyValue::Visible(false)).unwrap();
        set_key(&mut clip, "leg", "rotation", 0.0, KeyValue::Number(0.0)).unwrap();
        assert_eq!(clip.tracks.len(), 3);
        assert_eq!(track(&clip, "leg", "rotation"), Some(2));
        assert!(set_key(&mut clip, "arm", "visible", 1.0, KeyValue::Number(1.0)).is_err());
        assert!(set_key(&mut clip, "arm", "tint", 1.0, KeyValue::Number(1.0)).is_err());
    }

    #[test]
    fn moving_keys_keeps_them_in_order() {
        let mut clip = clip();
        for (time, value) in [(0.0, 0.0), (0.5, 1.0), (1.0, 2.0)] {
            set_key(&mut clip, "", "x", time, KeyValue::Number(value)).unwrap();
        }
        assert_eq!(move_key(&mut clip, 0, 0, 0.75), Some(1));
        assert_eq!(times(&clip, 0), [0.5, 0.75, 1.0]);
        // Onto another key, which it replaces.
        assert_eq!(move_key(&mut clip, 0, 1, 1.0), Some(1));
        assert_eq!(times(&clip, 0), [0.5, 1.0]);
        assert_eq!(
            clip.tracks[0].keys.value_at(1.0),
            Some(KeyValue::Number(0.0))
        );
        assert_eq!(move_key(&mut clip, 0, 1, 9.0), Some(1));
        assert_eq!(times(&clip, 0), [0.5, 2.0]);
        assert_eq!(last_key(&clip), 2.0);
        assert_eq!(move_key(&mut clip, 0, 5, 1.0), None);
    }

    #[test]
    fn a_track_goes_with_its_last_key() {
        let mut clip = clip();
        set_key(&mut clip, "a", "x", 0.0, KeyValue::Number(0.0)).unwrap();
        set_key(&mut clip, "a", "x", 1.0, KeyValue::Number(1.0)).unwrap();
        set_key(&mut clip, "b", "cell", 0.0, KeyValue::Cell([1, 2])).unwrap();
        remove_key(&mut clip, 0, 0);
        assert_eq!(times(&clip, 0), [1.0]);
        remove_key(&mut clip, 0, 0);
        assert_eq!(clip.tracks.len(), 1);
        assert_eq!(clip.tracks[0].node, "b");
        set_interpolation(&mut clip, 0, Interpolation::Step);
        assert_eq!(clip.tracks[0].keys, Keys::Cell(vec![(0.0, [1, 2])]));
    }

    #[test]
    fn times_stay_as_short_as_they_were_snapped() {
        assert_eq!(tidy_time(0.05 * 3.0).to_string(), "0.15");
        assert_eq!(tidy_time(1.0 / 3.0).to_string(), "0.3333");
    }

    #[test]
    fn nodes_give_and_take_the_properties_they_have() {
        let mut node = NodeDef::new("arm", KindDef::Node);
        set_value(&mut node, "rotation", KeyValue::Number(0.5));
        set_value(&mut node, "visible", KeyValue::Visible(false));
        assert_eq!(value(&node, "rotation"), Some(KeyValue::Number(0.5)));
        assert_eq!(value(&node, "visible"), Some(KeyValue::Visible(false)));
        assert_eq!(value(&node, "cell"), None);
        let mut sprite = NodeDef::new(
            "face",
            KindDef::Sprite(SpriteDef {
                sheet: "parts".into(),
                cell: [0, 0],
                size: None,
                stretch: [0.0, 0.0],
                pivot: [0.0, 0.0],
                tint: Color::WHITE,
            }),
        );
        set_value(&mut sprite, "cell", KeyValue::Cell([2, 1]));
        assert_eq!(value(&sprite, "cell"), Some(KeyValue::Cell([2, 1])));
    }

    #[test]
    fn clips_have_names_of_their_own() {
        let mut file = SceneFile::default();
        assert_eq!(add_clip(&mut file, "clip"), "clip");
        assert_eq!(add_clip(&mut file, "clip"), "clip2");
        assert!(rename_clip(&mut file, "clip2", "clip").is_err());
        assert!(rename_clip(&mut file, "clip2", "").is_err());
        rename_clip(&mut file, "clip2", "walk").unwrap();
        assert_eq!(file.clips.keys().collect::<Vec<_>>(), ["clip", "walk"]);
    }
}
