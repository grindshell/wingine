//! Undo and redo of whole-file snapshots, for each kind of file the editor edits.
//!
//! A snapshot is what the file was, and whatever's selected that undo selects again. A change
//! begins with [`History::begin`], which keeps the file as it is unless a change is being made, so
//! a drag or typing in a field is one change, and ends with [`History::end`], which keeps the
//! snapshot to undo only if the file is different from it now.

/// How many changes undo goes back through.
pub const HISTORY: usize = 200;

/// The snapshots to undo and redo, and the one before the change being made, if one is.
pub struct History<S> {
    /// Before each change, oldest first.
    undo: Vec<S>,
    /// After each change undone, latest undone last.
    redo: Vec<S>,
    /// Before the change being made, while it's being made.
    before: Option<S>,
}

impl<S> Default for History<S> {
    fn default() -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            before: None,
        }
    }
}

impl<S> History<S> {
    /// Keeps `now()`, the file as it is, to undo the change about to be made, unless one's being
    /// made.
    pub fn begin(&mut self, now: impl FnOnce() -> S) {
        if self.before.is_none() {
            self.before = Some(now());
        }
    }

    /// Ends the change being made, if any, keeping it to undo if `changed` says the file is
    /// different from the snapshot before it.
    pub fn end(&mut self, changed: impl FnOnce(&S) -> bool) {
        let Some(before) = self.before.take() else {
            return;
        };
        if changed(&before) {
            if self.undo.len() == HISTORY {
                self.undo.remove(0);
            }
            self.undo.push(before);
            self.redo.clear();
        }
    }

    /// Whether there's a change to undo: one kept, or one being made that `changed` says changed
    /// the file.
    pub fn can_undo(&self, changed: impl FnOnce(&S) -> bool) -> bool {
        !self.undo.is_empty() || self.before.as_ref().is_some_and(changed)
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Undoes the last change kept, handing its snapshot to `restore`, which puts the file back
    /// and returns the file as it was, to redo. End the change being made first.
    pub fn undo(&mut self, restore: impl FnOnce(S) -> S) {
        if let Some(before) = self.undo.pop() {
            let after = restore(before);
            self.redo.push(after);
        }
    }

    /// Makes the last change undone again, as [`History::undo`] undoes one.
    pub fn redo(&mut self, restore: impl FnOnce(S) -> S) {
        if let Some(after) = self.redo.pop() {
            let before = restore(after);
            self.undo.push(before);
        }
    }

    /// Whether a change is being made.
    pub fn is_changing(&self) -> bool {
        self.before.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changes_undo_and_redo_whole() {
        let mut history = History::default();
        let mut file = 1;
        // A drag: three edits, one change.
        for value in [2, 3, 4] {
            history.begin(|| file);
            file = value;
        }
        assert!(history.can_undo(|before| *before != file));
        history.end(|before| *before != file);
        history.undo(|before| std::mem::replace(&mut file, before));
        assert_eq!(file, 1);
        assert!(history.can_redo());
        history.redo(|after| std::mem::replace(&mut file, after));
        assert_eq!(file, 4);

        // A change that changes nothing isn't kept, and a new change forgets what was undone.
        history.begin(|| file);
        history.end(|before| *before != file);
        history.undo(|before| std::mem::replace(&mut file, before));
        history.begin(|| file);
        file = 9;
        history.end(|before| *before != file);
        assert!(!history.can_redo());
        history.undo(|before| std::mem::replace(&mut file, before));
        assert_eq!(file, 1);
        assert!(!history.can_undo(|before| *before != file));
    }

    #[test]
    fn history_keeps_the_latest_changes() {
        let mut history = History::default();
        let mut file = 0;
        for value in 1..=HISTORY + 10 {
            history.begin(|| file);
            file = value;
            history.end(|before| *before != file);
        }
        let mut undone = 0;
        while history.can_undo(|_| false) {
            history.undo(|before| std::mem::replace(&mut file, before));
            undone += 1;
        }
        assert_eq!(undone, HISTORY);
        assert_eq!(file, 10);
    }
}
