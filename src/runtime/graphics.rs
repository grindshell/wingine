//! The `graphics` table, for drawing.

use std::{
    cell::RefCell,
    collections::HashMap,
    path::{Path, PathBuf},
};

use mlua::{Lua, Table, UserDataRef};

use super::{CANVAS, with_canvas, with_context, with_lent};
use crate::{
    FrameStats,
    graphics::{Animation, Camera, Canvas, Color, Font, Sprite, SpriteSheet, TextLayout, Texture},
    lua::check_layer,
    math::Rect,
    tilemap::Tilemap,
};

/// The arguments of `graphics.draw_sprite_rotated`: the sprite, where it goes and its angle, then
/// optionally its tint and size.
type RotatedSpriteArgs = (
    UserDataRef<Sprite>,
    f32,
    f32,
    f32,
    Option<Color>,
    Option<f32>,
    Option<f32>,
);

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
        "draw_sprite_rotated",
        lua.create_function(
            |lua, (sprite, x, y, angle, tint, width, height): RotatedSpriteArgs| {
                let width = width.unwrap_or(sprite.width());
                let height = height.unwrap_or(sprite.height());
                let tint = tint.unwrap_or(Color::WHITE);
                with_canvas(lua, |canvas| {
                    let dest = Rect::new(x, y, width, height);
                    canvas.draw_sprite_rotated(&sprite, dest, angle, tint)
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
    graphics.set(
        "frame_stats",
        lua.create_function(|lua, ()| {
            let stats = lua
                .app_data_ref::<FrameStats>()
                .map_or_else(FrameStats::default, |stats| *stats);
            let table = lua.create_table()?;
            table.set("frame", stats.frame)?;
            table.set("update", stats.update)?;
            table.set("draw", stats.draw)?;
            table.set("render", stats.render)?;
            table.set("quads", stats.quads)?;
            table.set("draw_calls", stats.draw_calls)?;
            Ok(table)
        })?,
    )?;
    graphics.set(
        "new_animation",
        lua.create_function(|_, (frames, frame_time): (Vec<UserDataRef<Sprite>>, f32)| {
            let frames = frames.iter().map(|sprite| Sprite::clone(sprite)).collect();
            Animation::try_new(frames, frame_time).map_err(mlua::Error::runtime)
        })?,
    )?;
    let root = folder.to_owned();
    // Each image is loaded once, and shared by every sheet made from it.
    let textures: RefCell<HashMap<PathBuf, Texture>> = RefCell::default();
    graphics.set(
        "load_sprite_sheet",
        lua.create_function(
            move |lua, (path, tile_width, tile_height): (String, u32, u32)| {
                let path = root.join(path);
                // Through the context even when cached, so this works at the same times either
                // way.
                let texture = with_context(lua, |ctx| -> crate::Result<Texture> {
                    if let Some(texture) = textures.borrow().get(&path) {
                        return Ok(texture.clone());
                    }
                    let texture = ctx.load_texture(&path)?;
                    textures.borrow_mut().insert(path, texture.clone());
                    Ok(texture)
                })?
                .map_err(mlua::Error::external)?;
                SpriteSheet::try_new(texture, tile_width, tile_height).map_err(mlua::Error::runtime)
            },
        )?,
    )?;
    graphics.set_readonly(true);
    Ok(graphics)
}
