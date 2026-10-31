//! The `scene` table, for loading scenes.

use std::{cell::RefCell, collections::HashMap, path::Path, path::PathBuf, rc::Rc};

use mlua::{Lua, Table};

use super::{Lent, Textures};
use crate::{
    Context, Result,
    graphics::Texture,
    lua::SceneNode,
    scene::{Assets, Scene, SceneFile, TilesetFile},
};

/// The table, loading files relative to `folder`, and images through `textures`.
pub(super) fn module(
    lua: &Lua,
    lent: &Lent,
    folder: PathBuf,
    textures: Textures,
) -> mlua::Result<Table> {
    let scene = lua.create_table()?;
    scene.set("load", {
        let lent = lent.clone();
        // Each file is read once, and each load copies the scene it describes.
        let files = Files::default();
        let loaded: RefCell<HashMap<String, Scene>> = RefCell::default();
        lua.create_function(move |_, path: String| {
            // Through the context even when loaded already, so this works at the same times
            // either way.
            let scene = lent
                .context(|ctx| -> Result<Scene> {
                    if let Some(scene) = loaded.borrow().get(&path) {
                        return Ok(scene.clone());
                    }
                    let font = ctx.default_font().clone();
                    let mut assets = GameAssets {
                        ctx,
                        folder: &folder,
                        textures: &textures,
                        files: &files,
                    };
                    let file = assets.scene(&path)?;
                    let scene = Scene::new(&file, &path, &font, &mut assets)?;
                    loaded.borrow_mut().insert(path.clone(), scene.clone());
                    Ok(scene)
                })?
                .map_err(mlua::Error::external)?;
            Ok(SceneNode::root(scene))
        })?
    })?;
    scene.set_readonly(true);
    Ok(scene)
}

/// The scene and tileset files a game has read, by path.
#[derive(Default)]
struct Files {
    scenes: RefCell<HashMap<String, Rc<SceneFile>>>,
    tilesets: RefCell<HashMap<String, Rc<TilesetFile>>>,
}

/// A game's files and images, each read once.
struct GameAssets<'a> {
    ctx: &'a mut Context,
    folder: &'a Path,
    textures: &'a Textures,
    files: &'a Files,
}

impl Assets for GameAssets<'_> {
    fn texture(&mut self, path: &str) -> Result<Texture> {
        self.textures.load(self.ctx, self.folder.join(path))
    }

    fn scene(&mut self, path: &str) -> Result<Rc<SceneFile>> {
        if let Some(file) = self.files.scenes.borrow().get(path) {
            return Ok(Rc::clone(file));
        }
        let file = Rc::new(SceneFile::read(self.folder.join(path))?);
        let mut scenes = self.files.scenes.borrow_mut();
        scenes.insert(path.to_owned(), Rc::clone(&file));
        Ok(file)
    }

    fn tileset(&mut self, path: &str) -> Result<Rc<TilesetFile>> {
        if let Some(file) = self.files.tilesets.borrow().get(path) {
            return Ok(Rc::clone(file));
        }
        let file = Rc::new(TilesetFile::read(self.folder.join(path))?);
        let mut tilesets = self.files.tilesets.borrow_mut();
        tilesets.insert(path.to_owned(), Rc::clone(&file));
        Ok(file)
    }
}
