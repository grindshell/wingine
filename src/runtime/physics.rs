//! The `physics` table, for collision among a scene's nodes.

use mlua::{Lua, Table};

pub(super) fn module(lua: &Lua) -> mlua::Result<Table> {
    let physics = lua.create_table()?;
    physics.set("world", lua.create_function(|_, ()| world())?)?;
    physics.set_readonly(true);
    Ok(physics)
}

#[cfg(feature = "physics")]
fn world() -> mlua::Result<crate::lua::PhysicsWorld> {
    Ok(crate::lua::PhysicsWorld::default())
}

#[cfg(not(feature = "physics"))]
fn world() -> mlua::Result<()> {
    Err(mlua::Error::runtime(
        "the engine was built without its `physics` feature, so it has no physics worlds",
    ))
}
