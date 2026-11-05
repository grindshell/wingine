//! The `scene` table, for loading scenes and their clips.

use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc};

use mlua::{Lua, Table};

use super::{Lent, Textures};
use crate::{
    Context, Result,
    graphics::{Mesh, Model, Texture},
    lua::SceneNode,
    scene::{Assets, Scene, SceneFile, Shape, TilesetFile},
};

/// The table, loading files relative to `folder`, and images through `textures`.
pub(super) fn module(
    lua: &Lua,
    lent: &Lent,
    folder: PathBuf,
    textures: Textures,
) -> mlua::Result<Table> {
    let scene = lua.create_table()?;
    // Each file is read once, for loading scenes and their clips.
    let files = Rc::new(Files {
        folder,
        scenes: RefCell::default(),
        tilesets: RefCell::default(),
    });
    scene.set("load", {
        let lent = lent.clone();
        let files = Rc::clone(&files);
        // Each load copies the scene a file describes, built once.
        let loaded: RefCell<HashMap<String, Scene>> = RefCell::default();
        lua.create_function(move |_, path: String| {
            // Through the context even when loaded already, so this works at the same times
            // either way.
            let scene = lent
                .context(|ctx| -> Result<Scene> {
                    let size = ctx.canvas_size();
                    let mut scene = match loaded.borrow().get(&path) {
                        Some(scene) => scene.clone(),
                        None => {
                            let font = ctx.default_font().clone();
                            let mut assets = GameAssets {
                                ctx,
                                textures: &textures,
                                files: &files,
                            };
                            let file = assets.scene(&path)?;
                            Scene::new(&file, &path, &font, &mut assets)?
                        }
                    };
                    loaded
                        .borrow_mut()
                        .entry(path.clone())
                        .or_insert_with(|| scene.clone());
                    // Laid out on the canvas until it's drawn, which fits it to the canvas again.
                    scene.set_size(size);
                    Ok(scene)
                })?
                .map_err(mlua::Error::external)?;
            Ok(SceneNode::root(scene))
        })?
    })?;
    scene.set(
        "clips",
        lua.create_function(move |lua, path: String| {
            let file = files.scene(&path).map_err(mlua::Error::external)?;
            let clips = lua.create_table_with_capacity(0, file.clips.len())?;
            for (name, clip) in &file.clips {
                clips.set(name.as_str(), clip.clone())?;
            }
            Ok(clips)
        })?,
    )?;
    scene.set_readonly(true);
    Ok(scene)
}

/// The scene and tileset files a game has read, by their paths in its folder.
struct Files {
    folder: PathBuf,
    scenes: RefCell<HashMap<String, Rc<SceneFile>>>,
    tilesets: RefCell<HashMap<String, Rc<TilesetFile>>>,
}

impl Files {
    /// The scene file at `path`, reading it if it hasn't been.
    fn scene(&self, path: &str) -> Result<Rc<SceneFile>> {
        if let Some(file) = self.scenes.borrow().get(path) {
            return Ok(Rc::clone(file));
        }
        let file = Rc::new(SceneFile::read(self.folder.join(path))?);
        let mut scenes = self.scenes.borrow_mut();
        scenes.insert(path.to_owned(), Rc::clone(&file));
        Ok(file)
    }

    /// The tileset file at `path`, reading it if it hasn't been.
    fn tileset(&self, path: &str) -> Result<Rc<TilesetFile>> {
        if let Some(file) = self.tilesets.borrow().get(path) {
            return Ok(Rc::clone(file));
        }
        let file = Rc::new(TilesetFile::read(self.folder.join(path))?);
        let mut tilesets = self.tilesets.borrow_mut();
        tilesets.insert(path.to_owned(), Rc::clone(&file));
        Ok(file)
    }
}

/// A game's files and images, each read once.
struct GameAssets<'a> {
    ctx: &'a mut Context,
    textures: &'a Textures,
    files: &'a Files,
}

impl Assets for GameAssets<'_> {
    fn texture(&mut self, path: &str) -> Result<Texture> {
        self.textures.load(self.ctx, self.files.folder.join(path))
    }

    fn scene(&mut self, path: &str) -> Result<Rc<SceneFile>> {
        self.files.scene(path)
    }

    fn tileset(&mut self, path: &str) -> Result<Rc<TilesetFile>> {
        self.files.tileset(path)
    }

    fn model(&mut self, path: &str) -> Result<Model> {
        self.textures.model(self.ctx, self.files.folder.join(path))
    }

    fn mesh(&mut self, shape: &Shape) -> Result<Mesh> {
        self.ctx.create_mesh(&shape.mesh_data())
    }
}
