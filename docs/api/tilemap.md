# `tilemap`

Maps of tiles: grids of cells in layers, each holding a kind of tile from a tileset or nothing.
Layers draw in order, later ones on top, and tiles marked solid block movement.

Everything here works any time, apart from drawing a map, which is
[`graphics.draw_tilemap`](graphics.md#graphicsdraw_tilemap).

```lua
local TILE = 16
local sheet = graphics.load_sprite_sheet("tiles.png", TILE, TILE)

-- The kinds of tile.
local tileset = tilemap.new_tileset(TILE, TILE)
local GRASS = tileset:add({ sprite = sheet:sprite(0, 0) })
local WALL = tileset:add({ sprite = sheet:sprite(1, 0), solid = true })
local FLOWER = tileset:add({ sprite = sheet:sprite(2, 0), tint = "#f2c14e" })

-- A 20x12 map: grass everywhere, with a wall around the edge and a flower in the middle.
local map = tilemap.new(tileset, 20, 12)
local ground = map:add_layer()
local above = map:add_layer()
for row = 0, 11 do
    for column = 0, 19 do
        map:set(ground, column, row, GRASS)
        local edge = column == 0 or row == 0 or column == 19 or row == 11
        if edge then
            map:set(above, column, row, WALL)
        end
    end
end
map:set(above, 10, 6, FLOWER)

function draw()
    -- Centered on the map's middle.
    graphics.set_camera(10 * TILE, 6 * TILE, map:bounds())
    graphics.draw_tilemap(map)
end
```

[`examples/rpg/world.luau`](../../examples/rpg/world.luau) builds its maps from rows of symbols,
which is easier to edit than code.

## Tilesets

### `tilemap.new_tileset`

```lua
tilemap.new_tileset(tile_width: number, tile_height: number): Tileset
```

Returns an empty tileset of tiles `tile_width` by `tile_height` world units, which are canvas
pixels without a camera. Both must be at least 1.

### `Tileset`

#### `tileset:add`

```lua
tileset:add(tile: Tile): number
```

Adds a kind of tile and returns its ID, for placing it with [`map:set`](#mapset). IDs count from
0, in the order tiles are added. `tile` is a table:

| Field | What it is | Default |
|---|---|---|
| `sprite` | The `Sprite` drawn, stretched to fill the cell. | Required |
| `tint` | A `Color` the sprite's colors are multiplied by. | White, which leaves them as they are |
| `solid` | Whether a cell holding this tile, on any layer, blocks movement. | `false` |

## Maps

### `tilemap.new`

```lua
tilemap.new(tileset: Tileset, columns: number, rows: number): Tilemap
```

Returns a map `columns` by `rows` cells in size, with no layers yet, built from a copy of
`tileset`: add every kind of tile before making the map, since tiles added to the tileset
afterwards aren't in it.

A map holds at most 16,777,216 cells across all its layers, such as 2048 by 2048 with 4 layers.

### `Tilemap`

Cells are addressed by column and row, counting from 0 at the map's top-left corner, which is at
the world's origin. Layers are the numbers [`map:add_layer`](#mapadd_layer) returns, counting from
0.

#### `map:add_layer`

```lua
map:add_layer(): number
```

Adds an empty layer on top of the others and returns its number.

#### `map:layer_count`

```lua
map:layer_count(): number
```

Returns how many layers the map has.

#### `map:size`

```lua
map:size(): (number, number)
```

Returns the map's columns and rows.

#### `map:get`

```lua
map:get(layer: number, column: number, row: number): number?
```

Returns the ID of the tile in a cell on `layer`, or `nil` if the cell is empty or outside the map.

#### `map:set`

```lua
map:set(layer: number, column: number, row: number, tile: number?)
```

Puts the tile with ID `tile` in a cell on `layer`, or with `nil`, empties the cell. A cell outside
the map, a layer the map doesn't have, or an ID the tileset doesn't have is an error.

#### `map:contains`

```lua
map:contains(column: number, row: number): boolean
```

Returns whether a cell is inside the map.

#### `map:is_solid`

```lua
map:is_solid(column: number, row: number): boolean
```

Returns whether a cell blocks movement: whether it holds a solid tile on any layer, or is outside
the map. Grid movers use it to stop at walls and the map's edges:

```lua
player:step("up", function(column, row)
    return map:is_solid(column, row)
end)
```

#### `map:bounds`

```lua
map:bounds(): Rect
```

Returns the world area the map covers, `{ x = 0, y = 0, width = ..., height = ... }`, such as to
keep the [camera](graphics.md#graphicsset_camera) inside the map.

#### `map:tile_to_world`

```lua
map:tile_to_world(column: number, row: number): (number, number)
```

Returns the world position of the top-left corner of a cell. `column` and `row` may be fractions,
for a position between cells, such as a [grid mover's](grid.md#moverposition) partway through a
step:

```lua
local x, y = map:tile_to_world(player:position())
graphics.draw_sprite(hero, x, y)
```

#### `map:world_to_tile`

```lua
map:world_to_tile(x: number, y: number): (number, number)
```

Returns the column and row of the cell containing world position `x`, `y`, which may be outside
the map.
