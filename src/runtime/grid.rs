//! The `grid` table, for walking from tile to tile.

use mlua::{Lua, Table};

use crate::grid::{Direction, GridMover};

pub(super) fn module(lua: &Lua) -> mlua::Result<Table> {
    let grid = lua.create_table()?;
    grid.set(
        "new_mover",
        lua.create_function(|_, (column, row, seconds): (i32, i32, f32)| {
            GridMover::try_new([column, row], seconds).map_err(mlua::Error::runtime)
        })?,
    )?;
    grid.set(
        "neighbor",
        lua.create_function(|_, (direction, column, row): (Direction, i32, i32)| {
            let [column, row] = direction.checked_neighbor([column, row]).ok_or_else(|| {
                mlua::Error::runtime(format!(
                    "tile {column}, {row} has no neighbor that way: it's at the edge"
                ))
            })?;
            Ok((column, row))
        })?,
    )?;
    grid.set(
        "opposite",
        lua.create_function(|_, direction: Direction| Ok(direction.opposite()))?,
    )?;
    grid.set_readonly(true);
    Ok(grid)
}
