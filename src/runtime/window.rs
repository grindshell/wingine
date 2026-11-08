//! The `window` table, for the game's window.

use mlua::{IntoLua, Lua, Table};

use super::Lent;
use crate::scene::Value;

/// The table. `kept` is what the game before this one in the window returned from its
/// `reloading`, which `window.reloaded` returns.
pub(super) fn module(lua: &Lua, lent: &Lent, kept: Option<Value>) -> mlua::Result<Table> {
    let window = lua.create_table()?;
    window.set("quit", {
        let lent = lent.clone();
        lua.create_function(move |_, ()| lent.context(|ctx| ctx.quit()))?
    })?;
    window.set(
        "reloaded",
        lua.create_function(move |lua, ()| match &kept {
            Some(kept) => kept.into_lua(lua),
            None => Ok(mlua::Value::Nil),
        })?,
    )?;
    window.set_readonly(true);
    Ok(window)
}
