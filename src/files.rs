//! The files games load: scripts, scenes, images, models and sounds.
//!
//! Off the web, they're read from the file system. On the web, where there's none, they're read
//! from files the page gives the engine with [`add`] before the game starts, kept in memory by
//! their paths. Paths there are absolute, from `/`, and a relative one is taken from `/`.

use std::{
    io,
    path::{Path, PathBuf},
};

/// Reads the whole file at `path`.
pub(crate) fn read(path: &Path) -> io::Result<Vec<u8>> {
    imp::read(path)
}

/// Reads the whole file at `path`, which must be UTF-8.
pub(crate) fn read_to_string(path: &Path) -> io::Result<String> {
    let bytes = read(path)?;
    String::from_utf8(bytes).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

/// Whether there's a file at `path`.
pub(crate) fn is_file(path: &Path) -> bool {
    imp::is_file(path)
}

/// Whether there's a folder at `path`, as `require` asks.
pub(crate) fn is_dir(path: &Path) -> bool {
    imp::is_dir(path)
}

/// `path` made absolute, without `.` or `..`, if there's a file or folder there.
pub(crate) fn canonicalize(path: &Path) -> io::Result<PathBuf> {
    imp::canonicalize(path)
}

#[cfg(target_arch = "wasm32")]
pub use imp::add;

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use std::{
        fs, io,
        path::{Path, PathBuf},
    };

    pub fn read(path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    pub fn is_file(path: &Path) -> bool {
        path.is_file()
    }

    pub fn is_dir(path: &Path) -> bool {
        path.is_dir()
    }

    pub fn canonicalize(path: &Path) -> io::Result<PathBuf> {
        path.canonicalize()
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::{
        collections::BTreeMap,
        io,
        path::{Component, Path, PathBuf},
        sync::{Arc, Mutex, MutexGuard, PoisonError},
    };

    /// Every file, by its normalized path.
    static FILES: Mutex<BTreeMap<PathBuf, Arc<[u8]>>> = Mutex::new(BTreeMap::new());

    fn files() -> MutexGuard<'static, BTreeMap<PathBuf, Arc<[u8]>>> {
        FILES.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Adds a file at `path` holding `contents`, replacing any there, for the engine to read as
    /// a game loads it: a game in `/game` is a `/game/main.luau`, its other scripts, and the files
    /// they load beside them. Folders are whatever holds a file.
    pub fn add(path: impl AsRef<Path>, contents: impl Into<Arc<[u8]>>) {
        files().insert(normalize(path.as_ref()), contents.into());
    }

    /// `path` from `/`, without `.` or `..`.
    fn normalize(path: &Path) -> PathBuf {
        let mut normal = PathBuf::from("/");
        for component in path.components() {
            match component {
                Component::Normal(name) => normal.push(name),
                Component::ParentDir => {
                    normal.pop();
                }
                Component::Prefix(_) | Component::RootDir | Component::CurDir => {}
            }
        }
        normal
    }

    fn not_found(path: &Path) -> io::Error {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("there's no file {} in the page's files", path.display()),
        )
    }

    pub fn read(path: &Path) -> io::Result<Vec<u8>> {
        let file = files().get(&normalize(path)).cloned();
        file.map(|contents| contents.to_vec())
            .ok_or_else(|| not_found(path))
    }

    pub fn is_file(path: &Path) -> bool {
        files().contains_key(&normalize(path))
    }

    pub fn is_dir(path: &Path) -> bool {
        let path = normalize(path);
        // Paths order by their components, so whatever is inside a folder comes right after it.
        let files = files();
        let first = files.range(path.clone()..).next();
        first.is_some_and(|(file, _)| *file != path && file.starts_with(&path))
    }

    pub fn canonicalize(path: &Path) -> io::Result<PathBuf> {
        if is_file(path) || is_dir(path) {
            Ok(normalize(path))
        } else {
            Err(not_found(path))
        }
    }
}
