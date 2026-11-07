# `ui`

Menus over a [scene](scene.md)'s nodes: a selection among them that the keys and the mouse move,
as the RPG's title screen and questions and the shmup's title menu are picked. A menu only
chooses: the scene lays its items out, and the game shows which is selected however it likes,
reading [`menu:selected`](#menuselected) as it draws. `ui.menu` works any time, and
[`menu:update`](#menuupdate) while the game loads, in `update` and the tasks it resumes, and in
`gui`, since it reads the player's input.

```lua
local title = scene.load("title.scene.luau")
-- A stack of the choices, each a row with a pointer and a label.
local choices = title:find("choices")
local menu = ui.menu(choices)

input.bind("up", { "Up", "W" })
input.bind("down", { "Down", "S" })
input.bind("confirm", { "Space", "Enter" })

function update(dt: number)
    local happened, item = menu:update()
    if happened == "picked" then
        start(menu:items()[item]:name())
    end
end

function draw()
    for i, row in menu:items() do
        row:find("pointer"):set_visible(i == menu:selected())
    end
    graphics.draw_scene(title)
end
```

A menu's items are its node's children, such as a [stack](../scenes.md#stacks-and-panels)'s, or
with the `tag` option, the nodes inside it with that tag, in the order they draw. Only those shown
are items, so a hidden row a game copies its rows from is never one, and items are found again
each time the menu is used, so rows can come and go: the selection stays on its item while it's
there. Items are numbered from 1, in order.

- **The direction actions** move the selection by where the items are drawn, so a list goes up and
  down, a row across and a grid every way: to the nearest item that way of those lined up with
  the selected one, or with none, round to the farthest of them the other way. Only with none
  lined up either way does it go to the nearest that isn't, as down from the end of a row over a
  shorter one. A held key moves it again each time the system repeats it, as
  [`input.repeated`](input.md#inputrepeated) tells.
- **The confirm action** picks the selected item, and **the cancel action** cancels the menu.
- **The mouse** selects the item it moves onto, only as it moves, so the keys choose while it
  rests, and clicking an item picks it. Items are where they're drawn, so the scene must be drawn
  without a camera, in canvas pixels, where the mouse is. A plain node is where the nodes inside it
  are drawn.
- **Items that aren't enabled**, by [`menu:set_enabled`](#menuset_enabled), are passed over every
  way, and never picked. The selection is always on an item that can be picked, while there is one.

## `ui.menu`

```lua
ui.menu(node: SceneNode, options: MenuOptions?): Menu
```

Returns a menu of `node`'s items, its children, with the first that can be picked selected.
`options` is a table of any of:

- `tag`, a string: the items are the nodes inside `node` with this tag, rather than its children.
- `wrap`, a boolean: whether moving past the last item that way goes round to the farthest the
  other way. True by default.
- `mouse`, a boolean: whether the mouse selects and picks items. True by default.
- `up`, `down`, `left`, `right`, `confirm` and `cancel`: the names of the actions the menu answers
  to, as [`input.bind`](input.md#inputbind) binds them, by default those names themselves. An
  action that isn't bound does nothing, so a menu without a cancel action is never cancelled.

```lua
-- The RPG's menus pick with the key it interacts with.
local menu = ui.menu(panel, { confirm = "interact" })
```

A menu made while the game loads or updates notes where the cursor is, so the item under it isn't
selected until it moves.

## `Menu`

### `menu:update`

```lua
menu:update(): (("moved" | "picked" | "cancelled")?, number?)
```

Takes the player's input for this update, and returns what they did, if anything, and to which
item:

- `"moved", item`: the selection moved to `item`, by a direction action or the mouse moving onto
  it.
- `"picked", item`: the player picked `item`, by the confirm action or clicking it.
- `"cancelled"`: the player pressed the cancel action.

Call it once each update while the menu is open, and not while it's closed, so it doesn't take
input meant for something else. It fails in `draw`.

### `menu:selected`

```lua
menu:selected(): number?
```

Returns the selected item's number, or `nil` if no item can be picked.

### `menu:select`

```lua
menu:select(item: number)
```

Selects item `item`, or if it can't be picked, the next that can, as when a menu opens again.
Selecting an item there isn't is an error.

### `menu:items`

```lua
menu:items(): { SceneNode }
```

Returns the items, in order.

### `menu:set_enabled`

```lua
menu:set_enabled(item: number, enabled: boolean)
```

Sets whether item `item` can be selected and picked. Every item can until this says it can't, and
it stays so while it comes and goes.

### `menu:is_enabled`

```lua
menu:is_enabled(item: number): boolean
```

Returns whether item `item` can be selected and picked.
