//! The `physics` table: collision among a scene's nodes, and bodies and characters in 3D views.

use mlua::{Lua, Table};

pub(super) fn module(lua: &Lua) -> mlua::Result<Table> {
    let physics = lua.create_table()?;
    physics.set("world", lua.create_function(|_, ()| world())?)?;
    physics.set(
        "world_3d",
        lua.create_function(|_, options: Option<Table>| world_3d(options))?,
    )?;
    physics.set_readonly(true);
    Ok(physics)
}

#[cfg(feature = "physics")]
fn world() -> mlua::Result<crate::lua::PhysicsWorld> {
    Ok(crate::lua::PhysicsWorld::default())
}

/// A world whose dynamic bodies fall with `options.gravity`, by default at 9.81 units a second
/// each second, as Earth's do in metres.
#[cfg(feature = "physics")]
fn world_3d(options: Option<Table>) -> mlua::Result<crate::lua::PhysicsWorld3d> {
    let gravity = match options {
        Some(options) => options.get::<Option<mlua::Vector>>("gravity")?,
        None => None,
    };
    let gravity = gravity.map_or([0.0, -9.81, 0.0], crate::lua::vector3);
    if !gravity.iter().all(|g| g.is_finite()) {
        let message = format!("gravity must be finite, not {gravity:?}");
        return Err(mlua::Error::runtime(message));
    }
    Ok(crate::lua::PhysicsWorld3d::new(gravity))
}

#[cfg(not(feature = "physics"))]
fn world() -> mlua::Result<()> {
    Err(without())
}

#[cfg(not(feature = "physics"))]
fn world_3d(_: Option<Table>) -> mlua::Result<()> {
    Err(without())
}

#[cfg(not(feature = "physics"))]
fn without() -> mlua::Error {
    mlua::Error::runtime(
        "the engine was built without its `physics` feature, so it has no physics worlds",
    )
}
