//! The game being edited: its folder, its settings, and the files in it.

use std::{
    collections::HashMap,
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, RecvTimeoutError, Sender},
    },
    thread,
    time::{Duration, SystemTime},
};

use wingine::{
    Context, Waker,
    graphics::{Resolution, Texture},
    runtime::{self, Config},
    scene::{Assets, SceneFile, TilesetFile},
};

/// How deep into the folder's folders the file tree looks.
const MAX_DEPTH: usize = 8;
/// How often a [`Watcher`] looks at the folder's files.
const WATCH_INTERVAL: Duration = Duration::from_secs(1);

/// A game's folder, opened in the editor.
pub struct Project {
    /// Where it is, as given.
    pub folder: PathBuf,
    /// The folder's name.
    pub name: String,
    /// Its settings, from `conf.luau`, or the error reading them.
    pub config: Result<Config, String>,
    /// What's in the folder.
    pub files: Vec<Entry>,
}

/// A file or folder in a project, by its path from the project's folder, with `/` between names
/// as scene files write paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: String,
    /// What's inside it, for a folder.
    pub children: Option<Vec<Entry>>,
}

impl Project {
    /// Opens the game in `folder`, which must be a folder.
    pub fn open(folder: impl Into<PathBuf>) -> Result<Self, String> {
        let folder = folder.into();
        if !folder.is_dir() {
            return Err(format!("{} isn't a folder", folder.display()));
        }
        let name = folder
            .canonicalize()
            .ok()
            .and_then(|folder| Some(folder.file_name()?.to_string_lossy().into_owned()))
            .unwrap_or_else(|| folder.display().to_string());
        let mut project = Self {
            config: Err(String::new()),
            files: Vec::new(),
            folder,
            name,
        };
        project.rescan();
        Ok(project)
    }

    /// Reads the folder's files and settings again.
    pub fn rescan(&mut self) {
        self.files = scan(&self.folder, "", 0);
        self.config = runtime::config(&self.folder).map_err(|err| err.to_string());
    }

    /// Whether the folder holds a game: a `main.luau`.
    pub fn has_main(&self) -> bool {
        self.folder.join("main.luau").is_file()
    }

    /// The path of the file at `path` in the project, as files name it.
    pub fn absolute(&self, path: &str) -> PathBuf {
        self.folder.join(path)
    }

    /// The size of the game's canvas, which scenes are laid out on: its fixed resolution, or its
    /// window's size.
    pub fn canvas_size(&self) -> [f32; 2] {
        let window = match &self.config {
            Ok(config) => config.window.clone(),
            Err(_) => Default::default(),
        };
        match window.resolution {
            Resolution::Fixed { width, height } => [width as f32, height as f32],
            Resolution::Window => [window.width as f32, window.height as f32],
        }
    }

    /// The paths of the project's files whose names end in `suffix`, such as `.scene.luau`, in
    /// order.
    pub fn files_ending(&self, suffix: &str) -> Vec<String> {
        let mut found = Vec::new();
        collect(&self.files, &mut |entry| {
            if entry.children.is_none() && entry.name.ends_with(suffix) {
                found.push(entry.path.clone());
            }
        });
        found.sort();
        found
    }
}

fn collect(entries: &[Entry], visit: &mut impl FnMut(&Entry)) {
    for entry in entries {
        visit(entry);
        if let Some(children) = &entry.children {
            collect(children, visit);
        }
    }
}

/// The entries of the folder at `path` in `root`, folders first, each in order of name, without
/// hidden ones.
fn scan(root: &Path, path: &str, depth: usize) -> Vec<Entry> {
    let Ok(read) = fs::read_dir(root.join(path)) else {
        return Vec::new();
    };
    let mut entries: Vec<Entry> = read
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            if name.starts_with('.') {
                return None;
            }
            let path = if path.is_empty() {
                name.clone()
            } else {
                format!("{path}/{name}")
            };
            let is_folder = entry.file_type().ok()?.is_dir();
            let children = (is_folder && depth < MAX_DEPTH).then(|| scan(root, &path, depth + 1));
            Some(Entry {
                name,
                path,
                children: children.or_else(|| is_folder.then(Vec::new)),
            })
        })
        .collect();
    entries.sort_by(|a, b| (a.children.is_none(), &a.name).cmp(&(b.children.is_none(), &b.name)));
    entries
}

/// Watches a game's folder for files changed, added or removed, by the editor or anything else,
/// from a thread that looks at the files the file tree shows every second, and wakes the editor
/// when they've changed. So the editor reads the folder again only when it's changed, and
/// needn't run frames to find out.
pub struct Watcher {
    changed: Arc<AtomicBool>,
    /// Dropped with the watcher, which ends its thread.
    _stop: Sender<()>,
}

impl Watcher {
    /// Watches `folder`, waking the editor with `waker` when it changes.
    pub fn new(folder: PathBuf, waker: Waker) -> Self {
        let changed = Arc::new(AtomicBool::new(false));
        let (stop, stopped) = mpsc::channel();
        let flag = Arc::clone(&changed);
        thread::spawn(move || {
            let mut last = fingerprint(&folder);
            while let Err(RecvTimeoutError::Timeout) = stopped.recv_timeout(WATCH_INTERVAL) {
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

/// A hash of the paths, sizes and modified times of the files and folders the file tree of
/// `folder` shows, which changes when any of them does.
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

/// The images, scene files and tileset files scenes use, each read once and again only when its
/// file changes.
#[derive(Default)]
pub struct AssetCache {
    textures: HashMap<PathBuf, (Option<SystemTime>, Texture)>,
    scenes: HashMap<PathBuf, (Option<SystemTime>, Rc<SceneFile>)>,
    tilesets: HashMap<PathBuf, (Option<SystemTime>, Rc<TilesetFile>)>,
}

impl AssetCache {
    /// The cache, as [`Assets`] for building a scene of the game in `folder`.
    pub fn loader<'a>(&'a mut self, ctx: &'a mut Context, folder: &'a Path) -> Loader<'a> {
        Loader {
            cache: self,
            ctx,
            folder,
        }
    }
}

/// [`Assets`] from an [`AssetCache`], for a game's folder.
pub struct Loader<'a> {
    cache: &'a mut AssetCache,
    ctx: &'a mut Context,
    folder: &'a Path,
}

/// The entry for `path` in `cache`, read again with `read` if its file has changed since.
fn cached<T: Clone>(
    cache: &mut HashMap<PathBuf, (Option<SystemTime>, T)>,
    path: PathBuf,
    read: impl FnOnce(&Path) -> wingine::Result<T>,
) -> wingine::Result<T> {
    let modified = fs::metadata(&path).and_then(|meta| meta.modified()).ok();
    if let Some((when, value)) = cache.get(&path)
        && *when == modified
    {
        return Ok(value.clone());
    }
    let value = read(&path)?;
    cache.insert(path, (modified, value.clone()));
    Ok(value)
}

impl Loader<'_> {
    /// The image at `path` in the game's folder.
    pub fn texture(&mut self, path: &str) -> wingine::Result<Texture> {
        let ctx = &mut *self.ctx;
        cached(&mut self.cache.textures, self.folder.join(path), |path| {
            ctx.load_texture(path)
        })
    }
}

impl Assets for Loader<'_> {
    fn texture(&mut self, path: &str) -> wingine::Result<Texture> {
        Loader::texture(self, path)
    }

    fn scene(&mut self, path: &str) -> wingine::Result<Rc<SceneFile>> {
        cached(&mut self.cache.scenes, self.folder.join(path), |path| {
            SceneFile::read(path).map(Rc::new)
        })
    }

    fn tileset(&mut self, path: &str) -> wingine::Result<Rc<TilesetFile>> {
        cached(&mut self.cache.tilesets, self.folder.join(path), |path| {
            TilesetFile::read(path).map(Rc::new)
        })
    }
}
