//! The `window` table, for the game's window.

use mlua::{Lua, Table};

use super::Lent;

pub(super) fn module(lua: &Lua, lent: &Lent) -> mlua::Result<Table> {
    let window = lua.create_table()?;
    window.set("quit", {
        let lent = lent.clone();
        lua.create_function(move |_, ()| lent.context(|ctx| ctx.quit()))?
    })?;
    window.set_readonly(true);
    Ok(window)
}
