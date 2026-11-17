//! The `files` table, for folders the player picks and the files in them.

use mlua::{Function, Lua, Table, UserDataRef};

use crate::folders::{Folder, Folders};

/// `files.pick_folder`, given as `...` the functions that ask for the dialog and take its answer:
/// it asks, then waits until the dialog is answered, as a task.
const PICK_FOLDER: &str = r#"
local ask, answer = ...

return function()
    if not coroutine.isyieldable() then
        error("files.pick_folder waits for the player to pick a folder, so it works only in a task, such as one `spawn` starts", 2)
    end
    local ticket = ask()
    while true do
        local answered, folder = answer(ticket)
        if answered then
            return folder
        end
        coroutine.yield()
    end
end
"#;

/// The table, over the game's `folders`.
pub(super) fn module(lua: &Lua, folders: &Folders) -> mlua::Result<Table> {
    let files = lua.create_table()?;
    let ask = {
        let folders = folders.clone();
        lua.create_function(move |_, ()| folders.pick().map_err(mlua::Error::external))?
    };
    let answer = {
        let folders = folders.clone();
        lua.create_function(move |_, ticket: u64| {
            Ok(match folders.answer_to(ticket) {
                Some(folder) => (true, folder),
                None => (false, None),
            })
        })?
    };
    let pick_folder: Function = lua
        .load(PICK_FOLDER)
        .set_name("=files.pick_folder")
        .call((ask, answer))?;
    files.set("pick_folder", pick_folder)?;
    files.set("folders", {
        let folders = folders.clone();
        lua.create_function(move |_, ()| Ok(folders.picked()))?
    })?;
    files.set("forget", {
        let folders = folders.clone();
        lua.create_function(move |_, folder: UserDataRef<Folder>| {
            folders.forget(&folder);
            Ok(())
        })?
    })?;
    files.set_readonly(true);
    Ok(files)
}
