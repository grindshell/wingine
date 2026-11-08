//! Watching a game's folder for files changed, added or removed, as Luau games reloading as they're
//! edited do (see [`runtime`](crate::runtime)), and the editor does to read the folder again.

use std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError, Sender},
    },
    thread,
    time::Duration,
};

use crate::Waker;

/// How deep into the folder's folders a [`Watcher`] looks.
const MAX_DEPTH: usize = 8;

/// Watches a folder for files changed, added or removed, by anything, from a thread that looks at
/// them every so often: their paths, sizes and modified times, but not hidden ones (whose names
/// start with `.`), eight folders deep. It wakes a game that redraws on demand when they've
/// changed, so it needn't run frames to find out.
pub struct Watcher {
    changed: Arc<AtomicBool>,
    /// Dropped with the watcher, which ends its thread.
    _stop: Sender<()>,
}

impl Watcher {
    /// Watches `folder`, looking at its files every `interval`, and waking the game with `waker`
    /// when they've changed since now.
    pub fn new(folder: PathBuf, interval: Duration, waker: Waker) -> Self {
        let changed = Arc::new(AtomicBool::new(false));
        let (stop, stopped) = mpsc::channel();
        let flag = Arc::clone(&changed);
        // Looked at before the thread starts, so a change as soon as this returns counts.
        let mut last = fingerprint(&folder);
        thread::spawn(move || {
            while let Err(RecvTimeoutError::Timeout) = stopped.recv_timeout(interval) {
                let now = fingerprint(&folder);
                if now != last {
                    last = now;
                    flag.store(true, Ordering::Relaxed);
                    waker.wake();
                }
            }
        });
        Self {
            changed,
            _stop: stop,
        }
    }

    /// Whether the folder has changed since this was last asked.
    pub fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::Relaxed)
    }
}

/// A hash of the paths, sizes and modified times of the files and folders in `folder` that a
/// [`Watcher`] looks at, which changes when any of them does.
fn fingerprint(folder: &Path) -> u64 {
    fn visit(path: &Path, depth: usize, hasher: &mut DefaultHasher) {
        let Ok(read) = fs::read_dir(path) else {
            return;
        };
        let mut entries: Vec<_> = read
            .filter_map(Result::ok)
            .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
            .collect();
        entries.sort_by_key(fs::DirEntry::file_name);
        for entry in entries {
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            (depth, entry.file_name()).hash(hasher);
            (meta.is_dir(), meta.len(), meta.modified().ok()).hash(hasher);
            if meta.is_dir() && depth < MAX_DEPTH {
                visit(&entry.path(), depth + 1, hasher);
            }
        }
    }
    let mut hasher = DefaultHasher::new();
    visit(folder, 0, &mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;
    use crate::save::TemporaryFolder;

    /// A folder of its own, made, and removed when it's dropped.
    fn folder() -> TemporaryFolder {
        let folder = TemporaryFolder::new();
        fs::create_dir_all(folder.path()).unwrap();
        folder
    }

    #[test]
    fn fingerprints_change_with_files_but_not_hidden_ones() {
        let folder = folder();
        let path = folder.path();
        fs::write(path.join("main.luau"), "x = 1").unwrap();
        let before = fingerprint(path);
        assert_eq!(fingerprint(path), before);

        fs::write(path.join(".hidden"), "anything").unwrap();
        assert_eq!(fingerprint(path), before, "hidden files don't count");

        fs::write(path.join("main.luau"), "x = 12").unwrap();
        let changed = fingerprint(path);
        assert_ne!(changed, before, "a file's size changed");

        fs::create_dir(path.join("maps")).unwrap();
        fs::write(path.join("maps/town.scene.luau"), "return {}").unwrap();
        assert_ne!(fingerprint(path), changed, "a file was added in a folder");
    }

    #[test]
    fn watchers_notice_changes() {
        let folder = folder();
        let path = folder.path();
        fs::write(path.join("main.luau"), "x = 1").unwrap();
        let waker = Waker(Arc::default());
        let watcher = Watcher::new(path.to_owned(), Duration::from_millis(20), waker);
        thread::sleep(Duration::from_millis(100));
        assert!(!watcher.take_changed(), "nothing has changed");

        fs::write(path.join("main.luau"), "x = 12").unwrap();
        let start = Instant::now();
        while !watcher.take_changed() {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "the watcher never noticed"
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!watcher.take_changed(), "it's taken once");
    }
}
