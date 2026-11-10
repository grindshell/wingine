//! A scene file open in the editor: the file as it's being edited, its history for undo, what's
//! selected, and the scene it makes, to show.

use std::{mem, path::Path, time::SystemTime};

use wingine::{
    Context,
    scene::{Clip, Kind, NodeDef, NodeId, Scene, SceneFile},
};

use crate::{
    clips,
    edit::{self, NodePath},
    project::AssetCache,
    script::modified,
};

/// How many changes undo goes back through.
const HISTORY: usize = 200;

/// The most seconds particles in a preview run for as it's built, so they show.
const PREVIEW_WARMUP: f32 = 5.0;

/// A scene file open in a tab.
pub struct Document {
    /// Its path in the project, as files name each other.
    pub path: String,
    file: SceneFile,
    /// The file as it is on disk.
    saved: SceneFile,
    /// When its file was changed, as of the last time it was read or saved.
    modified: Option<SystemTime>,
    /// The file before each change, oldest first.
    undo: Vec<Snapshot>,
    /// The file after each change undone, latest undone last.
    redo: Vec<Snapshot>,
    /// The file before the change being made, while it's being made: a drag, or typing in a
    /// field, is one change.
    before: Option<Snapshot>,
    /// How many times the file has been put back as it was, by undo or redo, or read again, which
    /// leaves behind what panels had selected in it by index.
    restores: u64,
    /// The selected node, or the scene itself for the empty path.
    pub selected: Option<NodePath>,
    /// A node an instance places, selected inside it: the instance's path, and the node's path of
    /// names from the top of the scene it places. It counts only while that instance is
    /// `selected`.
    inner: Option<(NodePath, String)>,
    /// The scene the file makes, as it was last built without an error.
    pub preview: Option<Scene>,
    /// Why the file doesn't make a scene, if it doesn't.
    pub error: Option<String>,
    /// Whether the file has changed since the preview was built.
    stale: bool,
    /// Where the viewport looks, in the scene's coordinates, and its zoom: `None` until it's
    /// first shown, when it frames the canvas.
    pub view: Option<View>,
    /// The clip open in the Animation panel, and where its playhead is.
    pub timeline: Timeline,
    /// Whether the timeline's clip poses the preview, as it does while the Animation panel shows.
    posing: bool,
    /// The clip the preview has been posed by since it was built, if any.
    posed: Option<String>,
    /// Why the clip can't pose the preview, if it can't.
    pub pose_error: Option<String>,
}

/// The file as it was at a point in its history, and what was selected in it then, which undo
/// and redo select again: nodes are selected by where they are, which a change moves.
struct Snapshot {
    file: SceneFile,
    selected: Option<NodePath>,
    inner: Option<(NodePath, String)>,
}

/// What became of a scene whose file changed on disk.
#[derive(Clone, Debug, PartialEq)]
pub enum Reloaded {
    /// It was read again.
    Read,
    /// It has changes of its own, so it wasn't.
    Kept,
    /// Its file couldn't be read, so it wasn't, and why.
    Failed(String),
}

/// A clip open in the Animation panel, and when in it its playhead is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Timeline {
    pub clip: Option<String>,
    /// Seconds from the clip's start, up to its duration.
    pub time: f32,
}

/// Where a viewport looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    /// The point of the scene at the viewport's middle.
    pub center: [f32; 2],
    /// Points of the editor's interface per unit of the scene.
    pub zoom: f32,
}

impl Document {
    /// Opens the scene file at `path` in the game's `folder`.
    pub fn open(folder: &Path, path: &str) -> wingine::Result<Self> {
        let at = folder.join(path);
        let modified = modified(&at);
        let file = SceneFile::read(at)?;
        Ok(Self {
            modified,
            ..Self::new(path, file)
        })
    }

    pub fn new(path: &str, file: SceneFile) -> Self {
        Self {
            path: path.to_owned(),
            saved: file.clone(),
            modified: None,
            file,
            undo: Vec::new(),
            redo: Vec::new(),
            before: None,
            restores: 0,
            selected: None,
            inner: None,
            preview: None,
            error: None,
            stale: true,
            view: None,
            timeline: Timeline::default(),
            posing: false,
            posed: None,
            pose_error: None,
        }
    }

    /// The file's name, without its folders.
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    pub fn file(&self) -> &SceneFile {
        &self.file
    }

    /// The file, to change. Every change until [`Document::end_change`] is undone as one.
    pub fn edit(&mut self) -> &mut SceneFile {
        self.begin_change();
        self.stale = true;
        &mut self.file
    }

    /// The file and the preview, to change alike, as painting tiles does, so the scene needn't be
    /// built again. Every change until [`Document::end_change`] is undone as one.
    pub fn edit_with_preview(&mut self) -> (&mut SceneFile, Option<&mut Scene>) {
        self.begin_change();
        (&mut self.file, self.preview.as_mut())
    }

    /// Keeps the file as it is, and what's selected, to undo the change about to be made, unless
    /// one's being made.
    fn begin_change(&mut self) {
        if self.before.is_none() {
            self.before = Some(Snapshot {
                file: self.file.clone(),
                selected: self.selected.clone(),
                inner: self.inner.clone(),
            });
        }
    }

    /// Puts the file back as `snapshot` has it, selecting what it had selected, and returns the
    /// file and selection as they were.
    fn restore(&mut self, snapshot: Snapshot) -> Snapshot {
        let now = Snapshot {
            file: mem::replace(&mut self.file, snapshot.file),
            selected: mem::replace(&mut self.selected, snapshot.selected),
            inner: mem::replace(&mut self.inner, snapshot.inner),
        };
        self.changed();
        now
    }

    /// How many times the file has been undone, redone or read again: what a panel selected in
    /// it by index is gone once this changes.
    pub fn restores(&self) -> u64 {
        self.restores
    }

    /// Selects the node at `path`, or the scene itself for the empty path, or nothing.
    pub fn select(&mut self, path: Option<NodePath>) {
        self.selected = path;
        self.inner = None;
    }

    /// Selects the node at `names`, a path of names from the top of the scene the instance at
    /// `instance` places, inside that instance.
    pub fn select_inner(&mut self, instance: NodePath, names: String) {
        self.selected = Some(instance.clone());
        self.inner = Some((instance, names));
    }

    /// The path of names, from the top of the scene it places, of the node selected inside the
    /// selected instance, if one is.
    pub fn inner(&self) -> Option<&str> {
        let (instance, names) = self.inner.as_ref()?;
        (self.selected.as_ref() == Some(instance)).then_some(names.as_str())
    }

    /// The node of the preview that's selected: one made from a node of the file, or one an
    /// instance placed.
    pub fn selected_live(&self) -> Option<NodeId> {
        let id = self.live_node(self.selected.as_ref()?)?;
        match self.inner() {
            Some(names) => self.preview.as_ref()?.find(id, names),
            None => Some(id),
        }
    }

    /// Moves on by `dt` seconds the particles of the selected node, and of every particles node
    /// inside it, the scene itself included. Whether there were any to move, so they're shown
    /// moving.
    pub fn animate_particles(&mut self, dt: f32) -> bool {
        let Some(id) = self.selected_live() else {
            return false;
        };
        let Some(scene) = &mut self.preview else {
            return false;
        };
        let emits = scene
            .descendants(id)
            .into_iter()
            .any(|node| matches!(scene.node(node).kind(), Kind::Particles(_)));
        if emits {
            scene.advance_particles(id, dt);
        }
        emits
    }

    /// Ends the change being made, if any, keeping it to undo if it changed anything.
    pub fn end_change(&mut self) {
        let Some(before) = self.before.take() else {
            return;
        };
        if before.file != self.file {
            if self.undo.len() == HISTORY {
                self.undo.remove(0);
            }
            self.undo.push(before);
            self.redo.clear();
        }
    }

    /// Whether the file differs from what's on disk.
    pub fn is_dirty(&self) -> bool {
        self.file != self.saved
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.before.as_ref().is_some_and(|b| b.file != self.file)
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Undoes the last change, selecting again what was selected before it.
    pub fn undo(&mut self) {
        self.end_change();
        if let Some(before) = self.undo.pop() {
            let after = self.restore(before);
            self.redo.push(after);
        }
    }

    /// Makes the last change undone again, selecting what was selected when it was undone.
    pub fn redo(&mut self) {
        self.end_change();
        if let Some(after) = self.redo.pop() {
            let before = self.restore(after);
            self.undo.push(before);
        }
    }

    /// After the file changed out from under the selection: keeps it only if it's still there.
    fn changed(&mut self) {
        self.stale = true;
        self.restores += 1;
        if let Some(path) = &self.selected
            && !path.is_empty()
            && edit::node(&self.file, path).is_none()
        {
            self.selected = None;
        }
    }

    /// Writes the file to `folder`, in the canonical layout.
    pub fn save(&mut self, folder: &Path) -> wingine::Result<()> {
        self.end_change();
        let path = folder.join(&self.path);
        self.file.write(&path)?;
        self.saved = self.file.clone();
        self.modified = modified(&path);
        Ok(())
    }

    /// Reads its file in the game in `folder` again if it has changed on disk since it was last
    /// read or saved, unless the scene has changes of its own: `None` if it hasn't changed.
    pub fn reload_if_changed(&mut self, folder: &Path) -> Option<Reloaded> {
        let path = folder.join(&self.path);
        let now = modified(&path);
        if now == self.modified {
            return None;
        }
        self.modified = now;
        if self.is_dirty() {
            return Some(Reloaded::Kept);
        }
        match SceneFile::read(&path) {
            Ok(file) if file == self.file => None,
            Ok(file) => {
                self.end_change();
                self.saved = file.clone();
                self.file = file;
                self.changed();
                Some(Reloaded::Read)
            }
            Err(err) => Some(Reloaded::Failed(err.to_string())),
        }
    }

    /// Poses the preview by the timeline's clip from now on, or stops posing it.
    pub fn set_posing(&mut self, posing: bool) {
        self.posing = posing;
    }

    /// The clip posing the preview, by name, and when in it, while one is.
    pub fn posing(&self) -> Option<(&str, &Clip, f32)> {
        if !self.posing {
            return None;
        }
        let (name, clip) = self
            .file
            .clips
            .get_key_value(self.timeline.clip.as_deref()?)?;
        Some((name, clip, self.timeline.time.clamp(0.0, clip.duration)))
    }

    /// The node at `path` as the preview shows it: as the file has it, with the properties the
    /// clip posing the preview has tracks of as they are then.
    pub fn shown_node(&self, path: &[usize]) -> Option<NodeDef> {
        let mut node = edit::node(&self.file, path)?.clone();
        if let Some((_, clip, time)) = self.posing() {
            let names = edit::name_path(&self.file, path)?;
            for track in clip.tracks.iter().filter(|track| track.node == names) {
                if let Some(value) = track.keys.value_at(time) {
                    clips::set_value(&mut node, track.keys.property_name(), value);
                }
            }
        }
        Some(node)
    }

    /// The properties of the node at `path` the clip posing the preview has tracks of.
    pub fn keyed(&self, path: &[usize]) -> Vec<&'static str> {
        let (Some((_, clip, _)), Some(names)) = (self.posing(), edit::name_path(&self.file, path))
        else {
            return Vec::new();
        };
        clip.tracks
            .iter()
            .filter(|track| track.node == names)
            .map(|track| track.keys.property_name())
            .collect()
    }

    /// Changes the node at `path` to `node`, the node [`Document::shown_node`] gave, changed:
    /// properties the clip posing the preview has tracks of are keyed at the playhead, and the
    /// rest change in the node.
    pub fn set_node(&mut self, path: &[usize], mut node: NodeDef) {
        let (Some(shown), Some(original)) =
            (self.shown_node(path), edit::node(&self.file, path).cloned())
        else {
            return;
        };
        if node == shown {
            return;
        }
        let mut keys = Vec::new();
        let mut keying = None;
        if let Some((name, clip, time)) = self.posing() {
            let names = edit::name_path(&self.file, path).expect("the node is there");
            for track in clip.tracks.iter().filter(|track| track.node == names) {
                let property = track.keys.property_name();
                let value = clips::value(&node, property);
                if value != clips::value(&shown, property)
                    && let Some(value) = value
                {
                    keys.push((property, value));
                }
                // The node keeps its own value, which the clip covers.
                if let Some(own) = clips::value(&original, property) {
                    clips::set_value(&mut node, property, own);
                }
            }
            keying = Some((name.to_owned(), names, time));
        }
        let file = self.edit();
        if let Some((name, names, time)) = keying
            && let Some(clip) = file.clips.get_mut(&name)
        {
            for (property, value) in keys {
                clips::set_key(clip, &names, property, time, value)
                    .expect("the value is the property's own");
            }
        }
        if node != original {
            *edit::node_mut(file, path).expect("the node is there") = node;
        }
    }

    /// Keys `properties` of the node at `path` as the preview shows them, at the playhead of the
    /// clip posing it, adding tracks for those it has none of.
    pub fn key(&mut self, path: &[usize], properties: &[&str]) {
        let (Some(node), Some(names)) = (self.shown_node(path), edit::name_path(&self.file, path))
        else {
            return;
        };
        let Some((name, _, time)) = self.posing() else {
            return;
        };
        let name = name.to_owned();
        let clip = self.edit().clips.get_mut(&name).expect("the clip is there");
        for property in properties {
            if let Some(value) = clips::value(&node, property) {
                clips::set_key(clip, &names, property, time, value)
                    .expect("the value is the property's own");
            }
        }
    }

    /// Builds the scene again if the file changed, laid out on a canvas `canvas_size` in size, and
    /// poses it by the clip posing it. Returns the error building it, if it's a new one.
    pub fn refresh(
        &mut self,
        ctx: &mut Context,
        cache: &mut AssetCache,
        folder: &Path,
        canvas_size: [f32; 2],
    ) -> Option<String> {
        if let Some(scene) = &mut self.preview {
            scene.set_size(canvas_size);
        }
        // Another clip, or none, needs the scene as the file has it.
        let posing = self.posing().map(|(name, ..)| name.to_owned());
        if self.posed.is_some() && self.posed != posing {
            self.stale = true;
        }
        let error = if self.stale {
            self.build(ctx, cache, folder, canvas_size)
        } else {
            None
        };
        self.pose();
        error
    }

    /// Poses the preview by the clip posing it, as it is at the playhead: a clip that loops
    /// shows its end there, rather than its start.
    fn pose(&mut self) {
        let posing = self
            .timeline
            .clip
            .as_ref()
            .filter(|_| self.posing)
            .and_then(|name| self.file.clips.get_key_value(name));
        let (Some(scene), Some((name, clip))) = (&mut self.preview, posing) else {
            self.pose_error = None;
            return;
        };
        let held = Clip {
            looping: false,
            ..clip.clone()
        };
        let time = self.timeline.time.clamp(0.0, clip.duration);
        self.pose_error = scene.pose(scene.root(), &held, time, 1.0).err();
        self.posed = Some(name.clone());
    }

    /// Builds the scene from the file, returning the error if it's a new one.
    fn build(
        &mut self,
        ctx: &mut Context,
        cache: &mut AssetCache,
        folder: &Path,
        canvas_size: [f32; 2],
    ) -> Option<String> {
        self.stale = false;
        // A scene that fails to build keeps the one before, posed as it is.
        self.posed = None;
        let font = ctx.default_font().clone();
        let built = Scene::new(
            &self.file,
            &self.path,
            &font,
            &mut cache.loader(ctx, folder),
        );
        match built {
            Ok(mut scene) => {
                scene.set_size(canvas_size);
                // Particles show as they are once they've been going a while, so effects show
                // even while nothing animates them.
                let root = scene.root();
                let longest = scene
                    .descendants(root)
                    .into_iter()
                    .filter_map(|id| match scene.node(id).kind() {
                        Kind::Particles(node) => Some(node.particles.longest_life()),
                        _ => None,
                    })
                    .fold(0.0, f32::max);
                if longest > 0.0 {
                    scene.advance_particles(root, longest.min(PREVIEW_WARMUP));
                }
                self.preview = Some(scene);
                self.error = None;
                None
            }
            Err(err) => {
                let message = err.to_string();
                let new = self.error.as_ref() != Some(&message);
                self.error = Some(message.clone());
                new.then_some(message)
            }
        }
    }

    /// Builds the scene again next time it's shown, as when a file it uses has changed.
    pub fn rebuild(&mut self) {
        self.stale = true;
    }

    /// Whether the node at `names` inside the instance at `instance` is placed in 3D in the
    /// preview: in a 3D view, as its own file may not say.
    pub fn is_in_3d(&self, instance: &[usize], names: &str) -> bool {
        let Some((scene, id)) = self.preview.as_ref().zip(self.live_node(instance)) else {
            return false;
        };
        scene
            .find(id, names)
            .is_some_and(|inner| scene.node(inner).placement3().is_some())
    }

    /// The node of the preview made from the node at `path`.
    pub fn live_node(&self, path: &[usize]) -> Option<NodeId> {
        let scene = self.preview.as_ref()?;
        if path.is_empty() {
            return Some(scene.root());
        }
        scene.find(scene.root(), &edit::name_path(&self.file, path)?)
    }

    /// The node of the file that made `id`, a node of the preview: itself, or for a node an
    /// instance placed, the instance.
    pub fn file_node(&self, id: NodeId) -> Option<NodePath> {
        let scene = self.preview.as_ref()?;
        scene
            .contains_node(id)
            .then(|| edit::path_of(&self.file, &scene.path(id)))
    }
}

#[cfg(test)]
mod tests {
    use wingine::scene::{Interpolation, Keys, KindDef, NodeDef, Property};

    use super::*;

    fn document() -> Document {
        let mut file = SceneFile::default();
        file.children.push(NodeDef::new("a", KindDef::Node));
        Document::new("test.scene.luau", file)
    }

    #[test]
    fn a_change_is_undone_whole() {
        let mut document = document();
        // A drag: several frames of changes, then the button comes up.
        for x in 1..=3 {
            edit::node_mut(document.edit(), &[0]).unwrap().x = x as f32;
        }
        document.end_change();
        edit::node_mut(document.edit(), &[0]).unwrap().y = 5.0;
        document.end_change();
        assert!(document.is_dirty());

        document.undo();
        assert_eq!(edit::node(document.file(), &[0]).unwrap().y, 0.0);
        assert_eq!(edit::node(document.file(), &[0]).unwrap().x, 3.0);
        document.undo();
        assert_eq!(edit::node(document.file(), &[0]).unwrap().x, 0.0);
        assert!(!document.is_dirty());
        assert!(!document.can_undo());

        document.redo();
        document.redo();
        assert_eq!(edit::node(document.file(), &[0]).unwrap().y, 5.0);
        assert!(!document.can_redo());
    }

    #[test]
    fn changing_nothing_leaves_nothing_to_undo() {
        let mut document = document();
        let _ = document.edit();
        document.end_change();
        assert!(!document.can_undo());
    }

    #[test]
    fn a_new_change_forgets_what_was_undone() {
        let mut document = document();
        edit::node_mut(document.edit(), &[0]).unwrap().x = 1.0;
        document.end_change();
        document.undo();
        edit::node_mut(document.edit(), &[0]).unwrap().y = 1.0;
        document.end_change();
        assert!(!document.can_redo());
    }

    #[test]
    fn a_clip_posing_the_preview_keys_what_it_tracks() {
        let file = SceneFile::parse(
            r#"return { format = 1,
                children = { { name = "arm", kind = "node", rotation = 1 } },
                clips = { wave = { duration = 2, tracks = {
                    { node = "arm", property = "rotation", keys = { { 0, 0 }, { 2, 2 } } },
                } } } }"#,
            "test.scene.luau",
        )
        .unwrap();
        let mut document = Document::new("test.scene.luau", file);
        document.timeline = Timeline {
            clip: Some("wave".into()),
            time: 0.5,
        };
        // Without posing, nodes are as the file has them.
        assert_eq!(document.shown_node(&[0]).unwrap().rotation, 1.0);
        document.set_posing(true);
        let mut arm = document.shown_node(&[0]).unwrap();
        assert_eq!(arm.rotation, 0.5);
        assert_eq!(document.keyed(&[0]), ["rotation"]);

        // Turning it keys it; moving it, which the clip doesn't track, moves it.
        arm.rotation = -1.0;
        arm.x = 4.0;
        document.set_node(&[0], arm);
        document.end_change();
        let node = edit::node(document.file(), &[0]).unwrap();
        assert_eq!([node.rotation, node.x], [1.0, 4.0]);
        assert_eq!(
            document.file().clips["wave"].tracks[0].keys,
            Keys::Number {
                property: Property::Rotation,
                interpolation: Interpolation::Linear,
                keys: vec![(0.0, 0.0), (0.5, -1.0), (2.0, 2.0)],
            }
        );

        // Keying what it doesn't track yet adds a track.
        document.key(&[0], &["x", "visible"]);
        let tracks = &document.file().clips["wave"].tracks;
        assert_eq!(tracks.len(), 3);
        assert_eq!(
            tracks[1].keys,
            Keys::Number {
                property: Property::X,
                interpolation: Interpolation::Linear,
                keys: vec![(0.5, 4.0)],
            }
        );
        assert_eq!(tracks[2].keys, Keys::Visible(vec![(0.5, true)]));
    }

    #[test]
    fn undo_and_redo_select_what_was_selected() {
        let mut document = document();
        document.select(Some(vec![0]));
        let copy = edit::duplicate(document.edit(), &[0]).unwrap();
        document.select(Some(copy.clone()));
        document.end_change();
        let restores = document.restores();
        // Not the node after the copy, where the copy was, which would be the next to delete.
        document.undo();
        assert_eq!(document.selected, Some(vec![0]));
        document.redo();
        assert_eq!(document.selected, Some(copy));
        assert_eq!(document.restores(), restores + 2);
    }

    #[test]
    fn undo_drops_a_selection_that_is_gone() {
        let mut document = document();
        let path = edit::add(document.edit(), &[], NodeDef::new("b", KindDef::Node)).unwrap();
        document.end_change();
        document.selected = Some(path);
        document.undo();
        assert_eq!(document.selected, None);
    }
}
