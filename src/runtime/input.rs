//! The `input` table, for the keyboard and mouse.

use mlua::{FromLua, Function, Lua, Table};

use super::Lent;
use crate::input::{Input, Key, MouseButton};

pub(super) fn module(lua: &Lua, lent: &Lent) -> mlua::Result<Table> {
    let input = lua.create_table()?;
    input.set("bind", {
        let lent = lent.clone();
        lua.create_function(move |_, (action, keys): (String, Vec<Key>)| {
            lent.context(|ctx| ctx.input_mut().bind(action, keys))
        })?
    })?;
    input.set("held", action_query(lua, lent, Input::held)?)?;
    input.set("pressed", action_query(lua, lent, Input::pressed)?)?;
    input.set("released", action_query(lua, lent, Input::released)?)?;
    input.set("key_held", query::<Key>(lua, lent, Input::key_held)?)?;
    input.set("key_pressed", query::<Key>(lua, lent, Input::key_pressed)?)?;
    input.set(
        "key_released",
        query::<Key>(lua, lent, Input::key_released)?,
    )?;
    input.set(
        "mouse_held",
        query::<MouseButton>(lua, lent, Input::mouse_held)?,
    )?;
    input.set(
        "mouse_pressed",
        query::<MouseButton>(lua, lent, Input::mouse_pressed)?,
    )?;
    input.set(
        "mouse_released",
        query::<MouseButton>(lua, lent, Input::mouse_released)?,
    )?;
    input.set("mouse_position", {
        let lent = lent.clone();
        lua.create_function(move |_, ()| {
            lent.context(|ctx| match ctx.input().mouse_position() {
                Some([x, y]) => (Some(x), Some(y)),
                None => (None, None),
            })
        })?
    })?;
    input.set_readonly(true);
    Ok(input)
}

/// A function asking `check` about an action, by name.
fn action_query(lua: &Lua, lent: &Lent, check: fn(&Input, &str) -> bool) -> mlua::Result<Function> {
    let lent = lent.clone();
    lua.create_function(move |_, action: mlua::LuaString| {
        let action = action.to_str()?;
        lent.context(|ctx| check(ctx.input(), &action))
    })
}

/// A function asking `check` about a key or mouse button.
fn query<T: FromLua + 'static>(
    lua: &Lua,
    lent: &Lent,
    check: fn(&Input, T) -> bool,
) -> mlua::Result<Function> {
    let lent = lent.clone();
    lua.create_function(move |_, value: T| lent.context(|ctx| check(ctx.input(), value)))
}
