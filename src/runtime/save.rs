//! The `save` table, for data kept between runs of the game.

use mlua::{IntoLua, Lua, Table};

use crate::{Error, save::Saves, scene::plain};

/// The table, over the game's `saves`.
pub(super) fn module(lua: &Lua, saves: &Saves) -> mlua::Result<Table> {
    let save = lua.create_table()?;
    save.set("write", {
        let saves = saves.clone();
        lua.create_function(move |_, (name, data): (String, mlua::Value)| {
            let data = plain(data, "the data").map_err(|message| Error::Save {
                name: name.clone(),
                message,
            });
            data.and_then(|data| saves.write(&name, &data))
                .map_err(mlua::Error::external)
        })?
    })?;
    save.set("read", {
        let saves = saves.clone();
        lua.create_function(move |lua, name: String| {
            match saves.read(&name).map_err(mlua::Error::external)? {
                Some(data) => (&data).into_lua(lua),
                None => Ok(mlua::Value::Nil),
            }
        })?
    })?;
    save.set("remove", {
        let saves = saves.clone();
        lua.create_function(move |_, name: String| {
            saves.remove(&name).map_err(mlua::Error::external)
        })?
    })?;
    save.set("list", {
        let saves = saves.clone();
        lua.create_function(move |_, ()| saves.list().map_err(mlua::Error::external))?
    })?;
    save.set_readonly(true);
    Ok(save)
}
