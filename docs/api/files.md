# `files`

Folders the player picks, and the files in them, for a game that works on the player's own files,
as a text editor does on its documents.

Scripts are sandboxed: they read the game's own files and keep [saves](save.md), and reach
nothing else on the player's computer. A game that needs the player's files asks the player to
pick a folder, in the system's own dialog, and gets a [`Folder`](#folder), which reaches the files
in that folder, and in the folders inside it, and nothing outside it. The engine remembers the
folders picked, so the game can open the one it had open last without asking again.

```lua
local project = nil

-- Opens the folder the game had open last, if there is one.
project = wg.files.folders()[1]

function open_folder()
    -- Picking waits for the player, so it runs in a task.
    wg.spawn(function()
        local picked = wg.files.pick_folder()
        if picked then
            project = picked
        end
    end)
end

function save_draft(name: string, text: string)
    project:write(`drafts/{name}.md`, text)
end
```

`files.pick_folder` works only in tasks, since it waits. Everything else in `files`, and a
folder's methods, works any time.

## Paths

A folder's methods name what's in it by paths relative to it: names joined by `/` on every
system, such as `"wanderer.md"` or `"dialogs/wanderer.md"`. A path can't leave its folder, and
anything that would is an error:

- no `..` or `.`, and no `/` at the start or end, or two together;
- no `\` or `:`, so no absolute path;
- no link, such as a symbolic link or a Windows junction, on the way that leads out of the folder.
  A link that leads to somewhere in it is followed.

So that a game's paths work on every system, a name can't end with `.` or a space, which Windows
drops, nor be a name Windows keeps for a device, such as `con`, `nul` or `com1`, with or without an
extension, on any system. [`folder:list`](#folderlist) leaves out what no path could name.

## Functions

### `files.pick_folder`

```lua
wg.files.pick_folder(): Folder?
```

Opens the system's dialog for picking a folder, over the window, once the frame is done, then
waits until the player has picked one, and returns it, or `nil` if they cancelled. The engine
remembers it, first among [`files.folders`](#filesfolders).

It waits, so it works only in a task: call it in a function [`spawn`](globals.md#spawn) starts.
While a dialog is open, the player can't use the window behind it, and asking for another is an
error.

There are no folders to pick on the web yet: there, it's an error.

### `files.folders`

```lua
wg.files.folders(): { Folder }
```

Returns the folders the player has picked and the game hasn't forgotten, most recently first,
as the engine remembers them between runs: the last twenty, those still there.

The engine keeps them beside the game's [saves](save.md#where-saves-are-kept), in a file named
`.folders.luau`, which isn't a save, so `save` never lists or changes it.

### `files.forget`

```lua
wg.files.forget(folder: Folder)
```

Leaves `folder` out of [`files.folders`](#filesfolders) from now on, until it's picked again, as
when the player closes a project. The folder itself still works.

## `Folder`

A folder the player picked. Two are equal, `==`, when they're the same folder, however they were
reached.

### `folder:name`

```lua
folder:name(): string
```

Returns the folder's name: the last of its path's names, such as `"dialogs"`. A script never sees
the rest of its path.

### `folder:list`

```lua
folder:list(path: string?): { FolderEntry }?
```

Returns what's in the folder at `path`, or in this one without it: a list of its files and
folders, in order of their paths, each a table of:

- `path`: its path in the folder picked, which the folder's other methods take, such as
  `"dialogs/wanderer.md"`;
- `kind`: `"file"` or `"folder"`;
- `size`: its size in bytes, or 0 for a folder;
- `modified`: when it last changed, in seconds since 1970 began, UTC, or 0 where the system can't
  tell. A game that looks for changes made outside it, as by another program, compares these.

It returns `nil` if there's no such folder, and fails if `path` names a file. It lists only what's
in that folder, not what's in the folders inside it: list those by their paths. A link is listed
as what it leads to, if that's in the folder picked, and left out if it leads out of it.

```lua
for _, entry in project:list("dialogs") or {} do
    if entry.kind == "file" and string.sub(entry.path, -3) == ".md" then
        print(entry.path)
    end
end
```

### `folder:read`

```lua
folder:read(path: string): string?
```

Returns what the file at `path` holds, as a string of its bytes, or `nil` if there's no such file.
A path that names a folder is an error. Text is returned as it's kept, line endings included, so
a game that writes it back keeps them.

### `folder:write`

```lua
folder:write(path: string, contents: string)
```

Writes `contents` to the file at `path`, replacing any, and makes the folders it's in if they
aren't there. The file is written in full to a file of its own first, then takes the old one's
place, as saves are, so a game that stops partway through writing leaves the old one as it was.

A path that names a folder is an error.

### `folder:rename`

```lua
folder:rename(from: string, to: string)
```

Moves the file or folder at `from` to `to`, making the folders `to` is in if they aren't there.
Renaming never replaces anything: it's an error if there's something at `to` already, unless it's
what's at `from`, as when a name changes only its case on a system that doesn't tell cases apart.
It's an error too if there's nothing at `from`.

### `folder:remove`

```lua
folder:remove(path: string)
```

Removes the file at `path`, if there is one. A link is removed, not what it leads to. It never
removes a folder: a path that names one is an error.

## Errors

A path that isn't one, or that leads out of the folder, or names a folder where a file is wanted,
is an error naming the folder and the path:

```text
folder `dialogs`, path `../secrets.md`: `.` and `..` aren't names, and a path stays in its folder
```

So is a file that can't be read or written, naming it, as when the player has taken away the
permission to, or the disk is full.

## Testing

A test, or a run [without a window](../running-games.md#checking-a-game-without-a-window), opens
no dialog: it answers the game's with [`sim.pick`](testing.md#simpick), or a script's `pick`
command, which hand the game a copy of a folder, so checks and tests never change the player's
files. They remember the folders picked only while they run.
