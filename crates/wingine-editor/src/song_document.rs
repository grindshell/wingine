//! A song file open on the Music screen: the file as it's being edited, its history for undo,
//! what's selected in it, and the song it makes, to play.

use std::{mem, path::Path, time::SystemTime};

use wingine::music::{Pitch, Song, SongFile};

use crate::{document::Reloaded, history::History, script::modified, songs};

/// What's selected in a song, by name, which undo selects again.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Selection {
    pub instrument: Option<String>,
    pub pattern: Option<String>,
    pub section: Option<String>,
    /// A note of the selected pattern, by its beat and pitch.
    pub note: Option<(f64, Pitch)>,
}

struct Snapshot {
    file: SongFile,
    selection: Selection,
}

/// A song file open in a tab.
pub struct SongDocument {
    /// Its path in the project, with `/` between names.
    pub path: String,
    file: SongFile,
    /// The file as it is on disk.
    saved: SongFile,
    /// When its file was changed, as of the last time it was read or saved.
    modified: Option<SystemTime>,
    history: History<Snapshot>,
    pub selection: Selection,
    /// The song the file made when it was last made, which file, and how many times one has been
    /// made, so a song playing knows when to play the new one.
    made: Option<(SongFile, Result<Song, String>)>,
    revision: u64,
}

impl SongDocument {
    /// Opens the song file at `path` in the game's `folder`.
    pub fn open(folder: &Path, path: &str) -> wingine::Result<Self> {
        let at = folder.join(path);
        let modified = modified(&at);
        let file = SongFile::read(at)?;
        Ok(Self {
            modified,
            ..Self::new(path, file)
        })
    }

    pub fn new(path: &str, file: SongFile) -> Self {
        let mut document = Self {
            path: path.to_owned(),
            saved: file.clone(),
            file,
            modified: None,
            history: History::default(),
            selection: Selection::default(),
            made: None,
            revision: 0,
        };
        document.tidy_selection();
        document
    }

    /// The file's name, without its folders.
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    pub fn file(&self) -> &SongFile {
        &self.file
    }

    /// The file, to change. Every change until [`SongDocument::end_change`] is undone as one.
    pub fn edit(&mut self) -> &mut SongFile {
        self.history.begin(|| Snapshot {
            file: self.file.clone(),
            selection: self.selection.clone(),
        });
        &mut self.file
    }

    /// Ends the change being made, if any, keeping it to undo if it changed anything, and selects
    /// something in place of what it took away.
    pub fn end_change(&mut self) {
        self.history.end(|before| before.file != self.file);
        self.tidy_selection();
    }

    /// Whether a change is being made: something's being dragged or typed.
    pub fn is_changing(&self) -> bool {
        self.history.is_changing()
    }

    /// Whether the file differs from what's on disk.
    pub fn is_dirty(&self) -> bool {
        self.file != self.saved
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo(|before| before.file != self.file)
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    pub fn undo(&mut self) {
        self.end_change();
        let mut history = mem::take(&mut self.history);
        history.undo(|before| self.restore(before));
        self.history = history;
    }

    pub fn redo(&mut self) {
        self.end_change();
        let mut history = mem::take(&mut self.history);
        history.redo(|after| self.restore(after));
        self.history = history;
    }

    fn restore(&mut self, snapshot: Snapshot) -> Snapshot {
        let now = Snapshot {
            file: mem::replace(&mut self.file, snapshot.file),
            selection: mem::replace(&mut self.selection, snapshot.selection),
        };
        self.tidy_selection();
        now
    }

    /// Selects nothing in place of what the file doesn't have, and the first of each kind in
    /// place of nothing, but a note.
    fn tidy_selection(&mut self) {
        let file = &self.file;
        let selection = &mut self.selection;
        let keep =
            |name: &mut Option<String>, has: &dyn Fn(&str) -> bool, first: Option<&String>| {
                if name.as_deref().is_none_or(|name| !has(name)) {
                    *name = first.cloned();
                }
            };
        keep(
            &mut selection.instrument,
            &|name| file.instruments.contains_key(name),
            file.instruments.keys().next(),
        );
        let had_pattern = selection.pattern.clone();
        keep(
            &mut selection.pattern,
            &|name| file.patterns.contains_key(name),
            file.patterns.keys().next(),
        );
        let start = file
            .sections
            .contains_key(&file.start)
            .then_some(&file.start)
            .or_else(|| file.sections.keys().next());
        keep(
            &mut selection.section,
            &|name| file.sections.contains_key(name),
            start,
        );
        let note = selection.note.filter(|&(beat, pitch)| {
            selection.pattern == had_pattern
                && selection
                    .pattern
                    .as_ref()
                    .and_then(|pattern| file.patterns.get(pattern))
                    .is_some_and(|pattern| songs::find_note(pattern, beat, pitch).is_some())
        });
        selection.note = note;
    }

    /// The song the file makes, or why it makes none, made again only once the file has changed.
    pub fn song(&mut self) -> Result<Song, String> {
        if self
            .made
            .as_ref()
            .is_none_or(|(file, _)| *file != self.file)
        {
            let song = Song::new(&self.file).map_err(|err| err.to_string());
            self.made = Some((self.file.clone(), song));
            self.revision += 1;
        }
        self.made
            .as_ref()
            .expect("the song was just made")
            .1
            .clone()
    }

    /// How many times the song has been made of a changed file, as of the last
    /// [`SongDocument::song`].
    pub fn revision(&self) -> u64 {
        self.revision
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
    /// read or saved, unless the song has changes of its own: `None` if it hasn't changed.
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
        match SongFile::read(&path) {
            Ok(file) if file == self.file => None,
            Ok(file) => {
                self.end_change();
                self.saved = file.clone();
                self.file = file;
                self.tidy_selection();
                Some(Reloaded::Read)
            }
            Err(err) => Some(Reloaded::Failed(err.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn songs_undo_with_what_was_selected() {
        let mut document = SongDocument::new("a.song.luau", songs::new_song());
        assert_eq!(document.selection.pattern.as_deref(), Some("lead"));
        assert_eq!(document.selection.section.as_deref(), Some("main"));
        let first = document.revision();
        assert!(document.song().is_ok());
        assert!(document.revision() > first);

        let name = songs::add_pattern(document.edit(), "riff", 2);
        document.selection.pattern = Some(name);
        document.end_change();
        assert!(document.is_dirty() && document.can_undo());
        document.undo();
        assert_eq!(document.selection.pattern.as_deref(), Some("lead"));
        assert!(!document.file().patterns.contains_key("riff"));
        document.redo();
        assert_eq!(document.selection.pattern.as_deref(), Some("riff"));

        // A song that can't play says why, and the same file isn't made again.
        document.song().unwrap();
        let made = document.revision();
        document.song().unwrap();
        assert_eq!(document.revision(), made);
        document.edit().start = "nowhere".into();
        document.end_change();
        assert!(document.song().unwrap_err().contains("nowhere"));
    }
}
