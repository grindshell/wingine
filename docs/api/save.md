# `save`

Data kept between runs of the game, such as the player's progress or their settings.

A game keeps any number of saves, each with a name of letters, digits, `_` and `-`, such as
`"progress"` or `"slot-2"`. A save holds plain data: a boolean, number, string, or table of them,
where a table is a list or has strings for keys, nested as deep as needed. That's what a scene's
props hold, and saves are files of Luau like scene files, written the same way, so they're easy to
read.

Everything in `save` works any time.

```lua
local SAVE = "progress"

local saved = save.read(SAVE)
local progress = saved or { level = 1, coins = 0, unlocked = {} }

-- After finishing a level.
progress.level += 1
save.write(SAVE, progress)
```

## Where saves are kept

Each game keeps its saves in a folder of its own, named by its
[`identity`](../project-structure.md#identity), in the folder the system keeps applications' data
in:

| System | Folder |
|---|---|
| Windows | `%APPDATA%\Wingine\<identity>` |
| macOS | `~/Library/Application Support/Wingine/<identity>` |
| Linux | `$XDG_DATA_HOME/Wingine/<identity>`, or `~/.local/share/Wingine/<identity>` |

Each save is a file there named for it, `<name>.luau`. The folder is made when the first save is
written.

A game run [without a window](../running-games.md#checking-a-game-without-a-window) keeps its
saves in a temporary folder instead, which starts empty and goes when the run ends, so checks and
tests never see or change the player's saves. A scripted run can start with copies of a folder's
saves instead, with its `saves` command.

## Functions

### `save.write`

```lua
save.write(name: string, data: any)
```

Writes `data` to the save `name`, replacing any save of that name. The save is written in full to
a file of its own, then takes the old one's place, so a game that stops partway through writing
leaves the old save as it was.

Anything that isn't plain data is an error, naming where it is: a function, a sprite or other
engine object, a number that isn't finite, a table with both numbers and strings for keys, or a
list with a gap, such as `{ 1, nil, 3 }`. Nothing is written then.

```lua
save.write("progress", { level = 3, player = { name = "Pat", items = { "key", "lamp" } } })
save.write("best_time", 92.4)
```

A table is saved for its contents: a table with a metatable is saved as a plain one, and two
references to one table are saved as two tables. Numbers read back exactly as they were written.

### `save.read`

```lua
save.read(name: string): any
```

Returns the data in the save `name`, as it was written, or `nil` if there's no such save. A save
that can't be read, such as one changed by hand into something that isn't plain data, is an error.

A game that changes what it saves can keep a version number in its saves, to tell older ones
apart:

```lua
local VERSION = 2

local saved = save.read("progress")
if saved and saved.version ~= VERSION then
    saved = nil -- Saved by an older version of the game.
end
```

### `save.remove`

```lua
save.remove(name: string)
```

Removes the save `name`, if there is one.

### `save.list`

```lua
save.list(): { string }
```

Returns the names of the game's saves, in order, as for a menu of save slots.

## An example

The [RPG](../../examples/rpg) saves after every conversation, battle and trip between maps, in
[`progress.luau`](../../examples/rpg/progress.luau), and opens on a title screen whose Continue
picks up from there.
