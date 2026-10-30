//! The `tilemap` table, for building tile maps.

use mlua::{Lua, Table, UserDataRef};

use crate::{
    lua::check_tilemap_size,
    tilemap::{Tilemap, Tileset},
};

pub(super) fn module(lua: &Lua) -> mlua::Result<Table> {
    let tilemap = lua.create_table()?;
    tilemap.set(
        "new_tileset",
        lua.create_function(|_, (tile_width, tile_height): (u32, u32)| {
            if tile_width == 0 || tile_height == 0 {
                return Err(mlua::Error::runtime("tiles must be at least 1x1"));
            }
            Ok(Tileset::new(tile_width, tile_height))
        })?,
    )?;
    tilemap.set(
        "new",
        lua.create_function(
            |_, (tileset, columns, rows): (UserDataRef<Tileset>, u32, u32)| {
                // Fail now, rather than at the first layer, for a map too big for even one.
                check_tilemap_size(columns, rows, 1)?;
                Ok(Tilemap::new(tileset.clone(), columns, rows))
            },
        )?,
    )?;
    tilemap.set_readonly(true);
    Ok(tilemap)
}
