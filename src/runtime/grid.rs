//! The `grid` table, for walking from tile to tile.

use mlua::{Lua, Table};

use crate::{
    grid::{Direction, GridMover},
    lua::step_time,
};

pub(super) fn module(lua: &Lua) -> mlua::Result<Table> {
    let grid = lua.create_table()?;
    grid.set(
        "new_mover",
        lua.create_function(|_, (column, row, seconds): (i32, i32, f32)| {
            Ok(GridMover::new([column, row], step_time(seconds)?))
        })?,
    )?;
    grid.set(
        "neighbor",
        lua.create_function(|_, (direction, column, row): (Direction, i32, i32)| {
            let [column, row] = direction.neighbor([column, row]);
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
