//! The game being edited: its folder, its settings, and the files in it.

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, SystemTime},
};

use wingine::{
    Context, Waker,
    graphics::{Font, Mesh, Model, Resolution, Texture},
    runtime::{self, Config},
    scene::{self, Assets, SceneFile, Shape, TilesetFile},
    watch::Watcher,
};

/// How deep into the folder's folders the file tree looks, as deep as a [`Watcher`] looks.
const MAX_DEPTH: usize = 8;
/// How often the editor's [`Watcher`] looks at the folder's files.
const WATCH_INTERVAL: Duration = Duration::from_secs(1);
/// How many meshes of shapes an [`AssetCache`] keeps.
const MAX_MESHES: usize = 64;

/// A game's folder, opened in the editor.
pub struct Project {
    /// Where it is, as given.
    pub folder: PathBuf,
    /// The folder's name.
    pub name: String,
    /// Its settings, from `conf.luau`, or the error reading them.
    pub config: Result<Config, String>,
    /// What's in the folder.
    files: Vec<Entry>,
    /// The files panels offer, found as the folder was last read.
    listed: Listed,
}

/// The paths of a project's files of the kinds panels offer, each in order.
#[derive(Default)]
pub struct Listed {
    pub images: Vec<String>,
    pub tilesets: Vec<String>,
    pub scenes: Vec<String>,
    /// `.glb` and `.gltf` files.
    pub models: Vec<String>,
    pub fonts: Vec<String>,
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
            listed: Listed::default(),
            folder,
            name,
        };
        project.rescan();
        Ok(project)
    }

    /// Reads the folder's files and settings again.
    pub fn rescan(&mut self) {
        self.files = scan(&self.folder, "", 0);
        let mut models = self.files_ending(".glb");
        models.extend(self.files_ending(".gltf"));
        models.sort();
        self.listed = Listed {
            images: self.files_ending(".png"),
            tilesets: self.files_ending(".tileset.luau"),
            scenes: self.files_ending(".scene.luau"),
            models,
            fonts: self.files_ending(".font.luau"),
        };
        self.config = runtime::config(&self.folder).map_err(|err| err.to_string());
    }

    /// What's in the folder, as of when it was last read.
    pub fn files(&self) -> &[Entry] {
        &self.files
    }

    /// The files panels offer, as of when the folder was last read.
    pub fn listed(&self) -> &Listed {
        &self.listed
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

/// Whether the file at `path` is a model file, `.glb` or `.gltf`, as model nodes show.
pub fn is_model_file(path: &str) -> bool {
    path.ends_with(".glb") || path.ends_with(".gltf")
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
/// looking at the files the file tree shows every second, and waking the editor when they've
/// changed. So the editor reads the folder again only when it's changed, and needn't run frames
/// to find out.
pub fn watch(folder: PathBuf, waker: Waker) -> Watcher {
    Watcher::new(folder, WATCH_INTERVAL, waker)
}

/// The images, scene files, tileset files, fonts and model files scenes use, each read once and
/// again only when its file changes, and the meshes of their shapes.
#[derive(Default)]
pub struct AssetCache {
    textures: HashMap<PathBuf, (Option<SystemTime>, Texture)>,
    scenes: HashMap<PathBuf, (Option<SystemTime>, Rc<SceneFile>)>,
    tilesets: HashMap<PathBuf, (Option<SystemTime>, Rc<TilesetFile>)>,
    /// With when each of the files it read last changed: the font file, then its image or font.
    fonts: HashMap<PathBuf, (Vec<Option<SystemTime>>, Font)>,
    /// With when the model file and each of the files beside it that it read last changed.
    models: HashMap<PathBuf, (Vec<Option<SystemTime>>, Model)>,
    /// Meshes by their shapes, so a preview built again doesn't make its shapes' meshes again.
    meshes: Vec<(Shape, Mesh)>,
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

/// When the file at `path` last changed, if it's there.
fn modified(path: &Path) -> Option<SystemTime> {
    fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

/// The entry for `path` in `cache`, read again with `read` if its file has changed since.
fn cached<T: Clone>(
    cache: &mut HashMap<PathBuf, (Option<SystemTime>, T)>,
    path: PathBuf,
    read: impl FnOnce(&Path) -> wingine::Result<T>,
) -> wingine::Result<T> {
    let modified = modified(&path);
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

    /// The font file at `path`, read again if it or the image or font it names has changed.
    fn font(&mut self, path: &str) -> wingine::Result<Font> {
        let path = self.folder.join(path);
        let changes = |font: &Font| {
            let files = font.files().iter();
            files.map(|file| modified(file)).collect::<Vec<_>>()
        };
        if let Some((when, font)) = self.cache.fonts.get(&path)
            && *when == changes(font)
        {
            return Ok(font.clone());
        }
        // Its own time is taken before it's read, so a change while it's read reads it again.
        let own = modified(&path);
        let font = self.ctx.load_font(&path)?;
        let beside = font.files().iter().skip(1).map(|file| modified(file));
        let when = std::iter::once(own).chain(beside).collect::<Vec<_>>();
        self.cache.fonts.insert(path, (when, font.clone()));
        Ok(font)
    }

    /// The model file at `path`, read again if it or any file beside it that it read, such as a
    /// `.gltf`'s buffers and images, has changed since.
    fn model(&mut self, path: &str) -> wingine::Result<Model> {
        let path = self.folder.join(path);
        let changes = |own: Option<SystemTime>, model: &Model| {
            let beside = model.files().iter().map(|file| modified(file));
            std::iter::once(own).chain(beside).collect::<Vec<_>>()
        };
        // Its own time is taken before it's read, so a change while it's read reads it again.
        let own = modified(&path);
        if let Some((when, model)) = self.cache.models.get(&path)
            && *when == changes(own, model)
        {
            return Ok(model.clone());
        }
        let model = scene::load_model(self.ctx, &path)?;
        self.cache
            .models
            .insert(path, (changes(own, &model), model.clone()));
        Ok(model)
    }

    fn mesh(&mut self, shape: &Shape) -> wingine::Result<Mesh> {
        if let Some((_, mesh)) = self.cache.meshes.iter().find(|(made, _)| made == shape) {
            return Ok(mesh.clone());
        }
        let mesh = self.ctx.create_mesh(&shape.mesh_data())?;
        // Dragging a shape's size through many makes a mesh of each, so only the latest are kept.
        if self.cache.meshes.len() >= MAX_MESHES {
            self.cache.meshes.remove(0);
        }
        self.cache.meshes.push((*shape, mesh.clone()));
        Ok(mesh)
    }
}
