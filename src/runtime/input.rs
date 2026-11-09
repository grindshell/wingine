//! The `input` table, for the keyboard, mouse and gamepads.

use mlua::{FromLua, Function, Lua, Table};

use super::Lent;
use crate::{
    input::{Control, Gamepad, GamepadButton, Input, Key, MouseButton},
    math::Rect,
};

pub(super) fn module(lua: &Lua, lent: &Lent) -> mlua::Result<Table> {
    let input = lua.create_table()?;
    input.set("bind", {
        let lent = lent.clone();
        lua.create_function(
            move |_, (action, controls, gamepad): (String, Vec<Control>, Option<u32>)| {
                lent.context(|ctx| ctx.input_mut().bind_to(action, controls, gamepad))
            },
        )?
    })?;
    input.set("bindings", {
        let lent = lent.clone();
        lua.create_function(move |_, action: mlua::LuaString| {
            let action = action.to_str()?;
            lent.context(|ctx| {
                let input = ctx.input();
                (
                    input.bindings(&action).to_vec(),
                    input.bound_gamepad(&action),
                )
            })
        })?
    })?;
    input.set("pressed_controls", {
        let lent = lent.clone();
        lua.create_function(move |_, ()| {
            lent.context(|ctx| ctx.input().pressed_controls().to_vec())
        })?
    })?;
    input.set("wheel", {
        let lent = lent.clone();
        lua.create_function(move |_, ()| {
            lent.context(|ctx| {
                let [x, y] = ctx.input().wheel();
                (x, y)
            })
        })?
    })?;
    input.set("held", action_query(lua, lent, Input::held)?)?;
    input.set("pressed", action_query(lua, lent, Input::pressed)?)?;
    input.set("released", action_query(lua, lent, Input::released)?)?;
    input.set("repeated", action_query(lua, lent, Input::repeated)?)?;
    input.set("strength", action_query(lua, lent, Input::strength)?)?;
    input.set("axis", {
        let lent = lent.clone();
        lua.create_function(
            move |_, (negative, positive): (mlua::LuaString, mlua::LuaString)| {
                let (negative, positive) = (negative.to_str()?, positive.to_str()?);
                lent.context(|ctx| ctx.input().axis(&negative, &positive))
            },
        )?
    })?;
    input.set("key_held", query::<Key>(lua, lent, Input::key_held)?)?;
    input.set("key_pressed", query::<Key>(lua, lent, Input::key_pressed)?)?;
    input.set(
        "key_released",
        query::<Key>(lua, lent, Input::key_released)?,
    )?;
    input.set(
        "key_repeated",
        query::<Key>(lua, lent, Input::key_repeated)?,
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
    input.set("text", {
        let lent = lent.clone();
        lua.create_function(move |lua, ()| {
            lent.context(|ctx| lua.create_string(ctx.input().text()))?
        })?
    })?;
    input.set("composition", {
        let lent = lent.clone();
        lua.create_function(move |lua, ()| {
            lent.context(|ctx| lua.create_string(ctx.input().composition()))?
        })?
    })?;
    input.set("set_input_method", {
        let lent = lent.clone();
        lua.create_function(move |_, area: Option<Rect>| {
            lent.context(|ctx| ctx.input_mut().set_input_method(area))
        })?
    })?;
    input.set("mouse_position", {
        let lent = lent.clone();
        lua.create_function(move |_, ()| {
            lent.context(|ctx| match ctx.input().mouse_position() {
                Some([x, y]) => (Some(x), Some(y)),
                None => (None, None),
            })
        })?
    })?;
    input.set("gamepads", {
        let lent = lent.clone();
        lua.create_function(move |_, ()| {
            lent.context(|ctx| {
                let numbers: Vec<u32> = ctx.input().gamepads().map(|(number, _)| number).collect();
                numbers
            })
        })?
    })?;
    input.set("gamepad_name", {
        let lent = lent.clone();
        lua.create_function(move |_, gamepad: u32| {
            lent.context(|ctx| {
                let gamepad = ctx.input().gamepad(gamepad);
                gamepad.map(|gamepad| gamepad.name().to_owned())
            })
        })?
    })?;
    input.set("button_held", button_query(lua, lent, Gamepad::held)?)?;
    input.set("button_pressed", button_query(lua, lent, Gamepad::pressed)?)?;
    input.set(
        "button_released",
        button_query(lua, lent, Gamepad::released)?,
    )?;
    input.set("button_strength", {
        let lent = lent.clone();
        lua.create_function(move |_, (button, gamepad): (GamepadButton, Option<u32>)| {
            lent.context(|ctx| {
                ctx.input()
                    .matching_gamepads(gamepad)
                    .map(|pad| pad.strength(button))
                    .fold(0.0, f32::max)
            })
        })?
    })?;
    input.set_readonly(true);
    Ok(input)
}

/// A function asking `check` about an action, by name.
fn action_query<T: mlua::IntoLua + 'static>(
    lua: &Lua,
    lent: &Lent,
    check: fn(&Input, &str) -> T,
) -> mlua::Result<Function> {
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

/// A function asking `check` about a button on a gamepad, by its number, or on any gamepad.
fn button_query(
    lua: &Lua,
    lent: &Lent,
    check: fn(&Gamepad, GamepadButton) -> bool,
) -> mlua::Result<Function> {
    let lent = lent.clone();
    lua.create_function(move |_, (button, gamepad): (GamepadButton, Option<u32>)| {
        lent.context(|ctx| {
            ctx.input()
                .matching_gamepads(gamepad)
                .any(|pad| check(pad, button))
        })
    })
}
