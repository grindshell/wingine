//! The `input` table, for the keyboard and mouse.

use mlua::{FromLua, Function, Lua, Table};

use super::with_context;
use crate::input::{Input, Key, MouseButton};

pub(super) fn module(lua: &Lua) -> mlua::Result<Table> {
    let input = lua.create_table()?;
    input.set(
        "bind",
        lua.create_function(|lua, (action, keys): (String, Vec<Key>)| {
            with_context(lua, |ctx| ctx.input_mut().bind(action, keys))
        })?,
    )?;
    input.set("held", action_query(lua, Input::held)?)?;
    input.set("pressed", action_query(lua, Input::pressed)?)?;
    input.set("released", action_query(lua, Input::released)?)?;
    input.set("key_held", query::<Key>(lua, Input::key_held)?)?;
    input.set("key_pressed", query::<Key>(lua, Input::key_pressed)?)?;
    input.set("key_released", query::<Key>(lua, Input::key_released)?)?;
    input.set("mouse_held", query::<MouseButton>(lua, Input::mouse_held)?)?;
    input.set(
        "mouse_pressed",
        query::<MouseButton>(lua, Input::mouse_pressed)?,
    )?;
    input.set(
        "mouse_released",
        query::<MouseButton>(lua, Input::mouse_released)?,
    )?;
    input.set(
        "mouse_position",
        lua.create_function(|lua, ()| {
            with_context(lua, |ctx| match ctx.input().mouse_position() {
                Some([x, y]) => (Some(x), Some(y)),
                None => (None, None),
            })
        })?,
    )?;
    input.set_readonly(true);
    Ok(input)
}

/// A function asking `check` about an action, by name.
fn action_query(lua: &Lua, check: fn(&Input, &str) -> bool) -> mlua::Result<Function> {
    lua.create_function(move |lua, action: mlua::LuaString| {
        let action = action.to_str()?;
        with_context(lua, |ctx| check(ctx.input(), &action))
    })
}

/// A function asking `check` about a key or mouse button.
fn query<T: FromLua + 'static>(lua: &Lua, check: fn(&Input, T) -> bool) -> mlua::Result<Function> {
    lua.create_function(move |lua, value: T| with_context(lua, |ctx| check(ctx.input(), value)))
}
