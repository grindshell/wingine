//! The `scene` table, for loading scenes.

use std::{cell::RefCell, collections::HashMap, path::PathBuf};

use mlua::{Lua, Table};

use super::{Lent, Textures};
use crate::{
    lua::SceneNode,
    scene::{Scene, SceneFile},
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
        let loaded: RefCell<HashMap<PathBuf, Scene>> = RefCell::default();
        lua.create_function(move |_, path: String| {
            let path = folder.join(path);
            // Through the context even when loaded already, so this works at the same times
            // either way.
            let scene = lent
                .context(|ctx| -> crate::Result<Scene> {
                    if let Some(scene) = loaded.borrow().get(&path) {
                        return Ok(scene.clone());
                    }
                    let file = SceneFile::read(&path)?;
                    let font = ctx.default_font().clone();
                    let scene = Scene::new(&file, &path, &font, |sheet| {
                        textures.load(ctx, folder.join(&sheet.path))
                    })?;
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
