//! The `graphics` table, for drawing.

use mlua::{Lua, Table, UserDataRef};

use super::{CANVAS, with_canvas, with_context, with_lent};
use crate::{
    graphics::{Camera, Canvas, Color, Font},
    math::Rect,
};

pub(super) fn module(lua: &Lua, default_font: Font) -> mlua::Result<Table> {
    let graphics = lua.create_table()?;
    graphics.set(
        "clear",
        lua.create_function(|lua, color: Color| with_canvas(lua, |canvas| canvas.clear(color)))?,
    )?;
    graphics.set(
        "fill_rect",
        lua.create_function(
            |lua, (x, y, width, height, color): (f32, f32, f32, f32, Color)| {
                with_canvas(lua, |canvas| {
                    canvas.fill_rect(Rect::new(x, y, width, height), color)
                })
            },
        )?,
    )?;
    graphics.set(
        "draw_text",
        lua.create_function(
            |lua, (font, text, x, y, color): (UserDataRef<Font>, mlua::LuaString, f32, f32, Color)| {
                let text = text.to_str()?;
                with_canvas(lua, |canvas| canvas.draw_text(&font, &text, [x, y], color))
            },
        )?,
    )?;
    graphics.set(
        "set_camera",
        lua.create_function(|lua, position: (Option<f32>, Option<f32>)| {
            let camera = match position {
                (Some(x), Some(y)) => Some(Camera::new(x, y)),
                (None, None) => None,
                _ => {
                    let message = "set_camera takes a position, x and y, or nil for none";
                    return Err(mlua::Error::runtime(message));
                }
            };
            with_canvas(lua, |canvas| canvas.set_camera(camera))
        })?,
    )?;
    graphics.set(
        "size",
        lua.create_function(|lua, ()| {
            let size = |canvas: &mut Canvas| (canvas.width(), canvas.height());
            match with_lent(lua, CANVAS, size)? {
                Some(size) => Ok(size),
                None => with_context(lua, |ctx| ctx.canvas_size().into()),
            }
        })?,
    )?;
    graphics.set(
        "default_font",
        lua.create_function(move |_, ()| Ok(default_font.clone()))?,
    )?;
    graphics.set_readonly(true);
    Ok(graphics)
}
