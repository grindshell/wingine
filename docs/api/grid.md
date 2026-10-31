# `grid`

Movement from tile to tile, as in a top-down RPG: a character stands on a tile, walks to a
neighboring one over a fixed time, and faces the way it last tried to go.

Directions are the strings `"up"`, `"down"`, `"left"` and `"right"`. Tiles are a column and a row,
as in [tilemaps](tilemap.md). Everything here works any time.

```lua
-- `map` is a Tilemap with 16x16 tiles, made as on the tilemap page, and `hero` a Sprite.
local TILE = 16
local player = grid.new_mover(5, 5, 0.2) -- column 5, row 5, a fifth of a second a step

-- Actions named after the directions they walk.
local DIRECTIONS = { "up", "down", "left", "right" }
input.bind("up", { "Up", "W" })
input.bind("down", { "Down", "S" })
input.bind("left", { "Left", "A" })
input.bind("right", { "Right", "D" })

local function blocked(column: number, row: number): boolean
    return map:is_solid(column, row)
end

function update(dt: number)
    player:update(dt)
    for _, direction in DIRECTIONS do
        if input.held(direction) then
            player:step(direction, blocked)
            break
        end
    end
end

function draw()
    local x, y = map:tile_to_world(player:position())
    graphics.set_camera(x + TILE / 2, y + TILE / 2, map:bounds())
    graphics.draw_tilemap(map)
    graphics.draw_sprite(hero, x, y)
end
```

Each update, call `mover:update(dt)` first, to move the step under way on, then `mover:step` to
start the next one: it does nothing while a step is under way. A step started in the same update
the last one finished picks up the time left over, so a character walking several tiles keeps an
even pace, however updates fall.

## Functions

### `grid.new_mover`

```lua
grid.new_mover(column: number, row: number, step_time: number): GridMover
```

Returns a mover standing on the tile at `column`, `row` and facing down, which takes `step_time`
seconds to walk from one tile to the next. `step_time` must be above 0.

### `grid.neighbor`

```lua
grid.neighbor(direction: Direction, column: number, row: number): (number, number)
```

Returns the column and row of the next tile from `column`, `row` in `direction`. Rows grow
downward, so `"up"` is the row before.

### `grid.opposite`

```lua
grid.opposite(direction: Direction): Direction
```

Returns the opposite direction: `"down"` for `"up"`, and so on. An NPC the player talks to turns to
face `grid.opposite(player:facing())`.

## `GridMover`

### `mover:update`

```lua
mover:update(dt: number)
```

Moves the step under way on by `dt` seconds, which must be 0 or more. Call it every update.

### `mover:step`

```lua
mover:step(direction: Direction, blocked: ((column: number, row: number) -> boolean)?): boolean
```

Turns the mover toward `direction` and starts it stepping onto the next tile that way, unless
`blocked`, called with that tile's column and row, returns true. Returns whether a step started.
A blocked mover still turns, so pushing against a wall faces it.

While a step is under way, `step` does nothing and returns false.

`blocked` decides what may be entered, so games combine walls with their own rules, such as other
characters, which `blocked` can check by their [`tile`](#movertile). The mover may be looked at
from inside `blocked`, too.

```lua
local function blocked(column: number, row: number): boolean
    if map:is_solid(column, row) then
        return true
    end
    for _, npc in npcs do
        local npc_column, npc_row = npc.mover:tile()
        if npc_column == column and npc_row == row then
            return true
        end
    end
    return false
end
```

### `mover:tile`

```lua
mover:tile(): (number, number)
```

Returns the column and row of the tile the mover stands on, or while it's stepping, the tile it's
stepping onto. That tile counts as the mover's from the moment the step starts, so two movers
checking each other's tiles can't step onto the same one.

### `mover:position`

```lua
mover:position(): (number, number)
```

Returns where the mover is, in tiles: whole numbers when it's standing on a tile, and between two
during a step. Convert it to a world position with [`map:tile_to_world`](tilemap.md#maptile_to_world)
to draw it.

### `mover:facing`

```lua
mover:facing(): Direction
```

Returns the direction the mover faces: the way it last stepped, or tried to.

### `mover:face`

```lua
mover:face(direction: Direction)
```

Turns the mover to face `direction` without moving, as when an NPC turns to talk.

### `mover:is_moving`

```lua
mover:is_moving(): boolean
```

Returns whether a step is under way.

### `mover:step_time`

```lua
mover:step_time(): number
```

Returns the seconds the mover takes to walk one tile.

### `mover:set_step_time`

```lua
mover:set_step_time(seconds: number)
```

Changes how long walking one tile takes, as when starting to run. A step under way keeps the share
of it already walked. `seconds` must be above 0.

### `mover:place`

```lua
mover:place(column: number, row: number)
```

Moves the mover straight to a tile, ending any step under way, as when entering another map.
