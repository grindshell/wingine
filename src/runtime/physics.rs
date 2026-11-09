//! The `physics` table: bodies and characters among a scene's nodes, in 2D and in 3D views.

use mlua::{Lua, Table};

pub(super) fn module(lua: &Lua) -> mlua::Result<Table> {
    let physics = lua.create_table()?;
    physics.set(
        "world",
        lua.create_function(|_, options: Option<Table>| world(options))?,
    )?;
    physics.set(
        "world_3d",
        lua.create_function(|_, options: Option<Table>| world_3d(options))?,
    )?;
    physics.set_readonly(true);
    Ok(physics)
}

/// A 2D world with `options.metre` of the scene's units to a metre, 16 by default, whose dynamic
/// bodies fall down the scene with `options.gravity`, by default at 9.81 metres a second each
/// second, as Earth's do.
#[cfg(feature = "physics")]
fn world(options: Option<Table>) -> mlua::Result<crate::lua::PhysicsWorld> {
    let number = |name: &str| match &options {
        Some(options) => options.get::<Option<f32>>(name),
        None => Ok(None),
    };
    let metre = number("metre")?.unwrap_or(16.0);
    if !(metre.is_finite() && metre > 0.0) {
        let message = format!("a metre must be more than 0 of the scene's units, not {metre}");
        return Err(mlua::Error::runtime(message));
    }
    let gravity = number("gravity")?.unwrap_or(9.81 * metre);
    if !gravity.is_finite() {
        let message = format!("gravity must be finite, not {gravity}");
        return Err(mlua::Error::runtime(message));
    }
    Ok(crate::lua::PhysicsWorld::new(gravity, metre))
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
fn world(_: Option<Table>) -> mlua::Result<()> {
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
