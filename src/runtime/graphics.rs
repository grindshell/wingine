//! The `graphics` table, for drawing.

use std::path::Path;

use mlua::{Lua, Table, UserDataRef};

use super::{CANVAS, with_canvas, with_context, with_lent};
use crate::{
    graphics::{Camera, Canvas, Color, Font, Sprite, SpriteSheet, TextLayout},
    lua::check_layer,
    math::Rect,
    tilemap::Tilemap,
};

/// The table, loading files relative to `folder`.
pub(super) fn module(lua: &Lua, folder: &Path, default_font: Font) -> mlua::Result<Table> {
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
        "draw_sprite",
        lua.create_function(
            |lua,
             (sprite, x, y, tint, width, height): (
                UserDataRef<Sprite>,
                f32,
                f32,
                Option<Color>,
                Option<f32>,
                Option<f32>,
            )| {
                let width = width.unwrap_or(sprite.width());
                let height = height.unwrap_or(sprite.height());
                let tint = tint.unwrap_or(Color::WHITE);
                with_canvas(lua, |canvas| {
                    canvas.draw_sprite(&sprite, Rect::new(x, y, width, height), tint)
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
        "draw_text_layout",
        lua.create_function(
            |lua,
             (layout, x, y, color, glyphs): (
                UserDataRef<TextLayout>,
                f32,
                f32,
                Color,
                Option<f32>,
            )| {
                // Counts of revealed glyphs are often fractional; `as` rounds down, and NaN is 0.
                let shown = glyphs.map_or(usize::MAX, |glyphs| glyphs.max(0.0) as usize);
                with_canvas(lua, |canvas| {
                    canvas.draw_text_layout(&layout, ..shown, [x, y], color)
                })
            },
        )?,
    )?;
    graphics.set(
        "draw_tilemap",
        lua.create_function(|lua, (map, layer): (UserDataRef<Tilemap>, Option<usize>)| {
            if let Some(layer) = layer {
                check_layer(&map, layer)?;
            }
            with_canvas(lua, |canvas| match layer {
                Some(layer) => map.draw_layer(canvas, layer),
                None => map.draw(canvas),
            })
        })?,
    )?;
    graphics.set(
        "set_camera",
        lua.create_function(
            |lua, (x, y, bounds): (Option<f32>, Option<f32>, Option<Rect>)| {
                let camera = match (x, y) {
                    (Some(x), Some(y)) => Some(Camera {
                        bounds,
                        ..Camera::new(x, y)
                    }),
                    (None, None) => None,
                    _ => {
                        let message = "set_camera takes a position, x and y, or nil for none";
                        return Err(mlua::Error::runtime(message));
                    }
                };
                with_canvas(lua, |canvas| canvas.set_camera(camera))
            },
        )?,
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
    let root = folder.to_owned();
    graphics.set(
        "load_sprite_sheet",
        lua.create_function(
            move |lua, (path, tile_width, tile_height): (String, u32, u32)| {
                if tile_width == 0 || tile_height == 0 {
                    let message = "sprite sheet tiles must be at least 1x1";
                    return Err(mlua::Error::runtime(message));
                }
                let path = root.join(path);
                let texture = with_context(lua, |ctx| ctx.load_texture(path))?
                    .map_err(mlua::Error::external)?;
                Ok(SpriteSheet::new(texture, tile_width, tile_height))
            },
        )?,
    )?;
    graphics.set_readonly(true);
    Ok(graphics)
}
