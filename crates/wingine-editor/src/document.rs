//! A scene file open in the editor: the file as it's being edited, its history for undo, what's
//! selected, and the scene it makes, to show.

use std::{mem, path::Path};

use wingine::{
    Context,
    scene::{NodeId, Scene, SceneFile},
};

use crate::{
    edit::{self, NodePath},
    project::AssetCache,
};

/// How many changes undo goes back through.
const HISTORY: usize = 200;

/// A scene file open in a tab.
pub struct Document {
    /// Its path in the project, as files name each other.
    pub path: String,
    file: SceneFile,
    /// The file as it is on disk.
    saved: SceneFile,
    /// The file before each change, oldest first.
    undo: Vec<SceneFile>,
    /// The file after each change undone, latest undone last.
    redo: Vec<SceneFile>,
    /// The file before the change being made, while it's being made: a drag, or typing in a
    /// field, is one change.
    before: Option<SceneFile>,
    /// The selected node, or the scene itself for the empty path.
    pub selected: Option<NodePath>,
    /// The scene the file makes, as it was last built without an error.
    pub preview: Option<Scene>,
    /// Why the file doesn't make a scene, if it doesn't.
    pub error: Option<String>,
    /// Whether the file has changed since the preview was built.
    stale: bool,
    /// Where the viewport looks, in the scene's coordinates, and its zoom: `None` until it's
    /// first shown, when it frames the canvas.
    pub view: Option<View>,
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
        let file = SceneFile::read(folder.join(path))?;
        Ok(Self::new(path, file))
    }

    pub fn new(path: &str, file: SceneFile) -> Self {
        Self {
            path: path.to_owned(),
            saved: file.clone(),
            file,
            undo: Vec::new(),
            redo: Vec::new(),
            before: None,
            selected: None,
            preview: None,
            error: None,
            stale: true,
            view: None,
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
        if self.before.is_none() {
            self.before = Some(self.file.clone());
        }
        self.stale = true;
        &mut self.file
    }

    /// Ends the change being made, if any, keeping it to undo if it changed anything.
    pub fn end_change(&mut self) {
        let Some(before) = self.before.take() else {
            return;
        };
        if before != self.file {
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
        !self.undo.is_empty() || self.before.as_ref().is_some_and(|b| *b != self.file)
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        self.end_change();
        if let Some(before) = self.undo.pop() {
            self.redo.push(mem::replace(&mut self.file, before));
            self.changed();
        }
    }

    pub fn redo(&mut self) {
        self.end_change();
        if let Some(after) = self.redo.pop() {
            self.undo.push(mem::replace(&mut self.file, after));
            self.changed();
        }
    }

    /// After the file changed out from under the selection: keeps it only if it's still there.
    fn changed(&mut self) {
        self.stale = true;
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
        self.file.write(folder.join(&self.path))?;
        self.saved = self.file.clone();
        Ok(())
    }

    /// Builds the scene again if the file changed, laid out on a canvas `canvas_size` in size.
    /// Returns the error building it, if it's a new one.
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
        if !self.stale {
            return None;
        }
        self.stale = false;
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
    use wingine::scene::{KindDef, NodeDef};

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
    fn undo_drops_a_selection_that_is_gone() {
        let mut document = document();
        let path = edit::add(document.edit(), &[], NodeDef::new("b", KindDef::Node)).unwrap();
        document.end_change();
        document.selected = Some(path);
        document.undo();
        assert_eq!(document.selected, None);
    }
}
