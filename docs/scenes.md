# Scenes

A scene is a tree of sprites, rectangles, text and areas, laid out in a file of its own rather
than in code: a level, a screen, a character. The game loads it with
[`scene.load`](api/scene.md#sceneload), finds the nodes it cares about by name or tag, moves and
changes them, and draws the whole tree with one call:

```lua
local garden = scene.load("garden.scene.luau")
local sails = garden:find("mill/sails")

function update(dt: number)
    sails:set_rotation(sails:rotation() + dt)
end

function draw()
    graphics.draw_scene(garden)
end
```

Scene files are Luau, but only data: one table of tables, strings, numbers and booleans. That
way a tool, such as an editor, can read a file, change it and write it back, and people can still
write and read them by hand. [`examples/garden`](../examples/garden) is a game built around one.

## A scene file

```lua
-- garden.scene.luau
return {
    format = 1,
    sheets = {
        tiles = {
            path = "tiles.png",
            tile_width = 16,
            tile_height = 16,
        },
    },
    props = {
        music = "sounds/garden.ogg",
    },
    children = {
        {
            name = "mill",
            kind = "sprite",
            x = 232,
            y = 76,
            sheet = "tiles",
            cell = { 4, 19 },
            pivot = { 8, 16 },
            tint = "#d8b070",
            tags = { "solid" },
            children = {
                {
                    name = "sails",
                    kind = "node",
                    y = -14,
                },
            },
        },
        {
            name = "pond",
            kind = "area",
            x = 32,
            y = 128,
            width = 80,
            height = 34,
            tags = { "solid" },
            props = {
                text = "The pond is full of frogs.",
            },
        },
    },
}
```

The file returns the scene, and runs with nothing in scope: it can't call functions, such as
`math.floor`, or reach the engine or the game. Write numbers, strings, booleans and tables, with
no variables, loops or arithmetic apart from negative numbers. Name scene files `*.scene.luau`,
so they're easy to tell from scripts.

At the top of the file:

| Field | What it is |
|---|---|
| `format` | Required: the version of the format, `1`. The engine refuses files newer than it reads. |
| `sheets` | The sprite sheets the scene's sprites come from, by name: each an image's `path`, relative to the game's folder, divided into sprites `tile_width` by `tile_height` texels, as [`graphics.load_sprite_sheet`](api/graphics.md#graphicsload_sprite_sheet) divides one. |
| `props` | Data for the game about the scene as a whole, such as its music. |
| `children` | The nodes at the top of the scene's tree. |

## Nodes

Every node has a `name` and a `kind`, and may have any of the rest:

| Field | What it is | Default |
|---|---|---|
| `name` | Required. Unique among the node's siblings, and without `/`. | |
| `kind` | Required. `"node"`, `"sprite"`, `"rect"`, `"text"`, `"area"`, `"tilemap"`, `"tile_layer"` or `"instance"`. | |
| `x`, `y` | Where the node's origin is within its parent. | `0` |
| `rotation` | Radians, clockwise, about its origin. | `0` |
| `scale_x`, `scale_y` | How much it's stretched along its own axes, about its origin. Negative scales mirror it. | `1` |
| `visible` | Whether it and its children draw. | `true` |
| `tags` | A list of strings, for finding nodes of a sort, such as `{ "solid" }`. | none |
| `props` | Data for the game. | none |
| `children` | The nodes inside it. | none |

A node's children are placed within it: they move, turn and stretch with it. Turning the mill's
`sails` node turns everything inside it, however it's arranged. The scene's coordinates are the
coordinates it's drawn in: canvas pixels, or the world's after
[`graphics.set_camera`](api/graphics.md#graphicsset_camera).

Each kind has fields of its own:

- **`node`**: nothing but a place, for grouping nodes, turning them together, or marking a spot,
  such as where the player starts.
- **`sprite`**: `sheet`, the name of one of the scene's sheets, and `cell`, the sprite's column and
  row in it, `{ column, row }`, counting from 0 at the top-left. Optionally `width` and `height`
  to stretch it to (both or neither; its own size by default), and `tint`, a color its colors are
  multiplied by.
- **`rect`**: a filled rectangle, `width` by `height`, of `color`.
- **`text`**: `text` in the built-in font, `font_scale` times its size (1 by default), wrapped
  between words to `wrap` if given, in `color`. Text moves with its node, but doesn't turn or
  stretch.
- **`area`**: a rectangle `width` by `height` that doesn't draw, for marking out part of a scene:
  where the player can't walk, where something happens, a door to another scene.
- **`tilemap`** and **`tile_layer`**: a grid of tiles, and its layers. See [Tilemaps](#tilemaps).
- **`instance`**: another scene file, placed here. See [Instances](#instances).

A sprite, rect, text or area has its top-left corner at the node's origin, unless it has a
`pivot`: the point of it, from its top-left corner, `{ x, y }`, that's at the origin instead, and
that it turns and stretches about. A person 16 texels tall standing at `x`, `y` has a pivot of
`{ 8, 16 }`, the middle of their feet, so they stand on the spot, turn about their feet and mirror
in place.

Colors are hex strings such as `"#f2c14e"`, or `"#00000080"` with alpha, and are white if left
out. Props hold booleans, numbers, strings, and tables of them that are lists or have strings for
keys.

Nodes draw in order, each before its children, so later nodes are on top: the ground first, the
HUD last.

Anything wrong with a file is an error when it loads, naming the file and the node: a field a node
doesn't have, a missing one it must have, a sheet the scene doesn't have, or two siblings with the
same name.

```text
scene garden.scene.luau: node `mill/sails`: a plain node has no field `colour`
```

## Tilemaps

A `tilemap` node is a grid of `columns` by `rows` tiles, with its top-left corner at the node's
origin. Its tiles come from a tileset file, `tileset`, and its layers are its children of the kind
`tile_layer`, which draw in order with its other children. So characters can stand on the ground
but under the treetops: put them in a node between the two layers.

```lua
{
    name = "map",
    kind = "tilemap",
    tileset = "world.tileset.luau",
    columns = 6,
    rows = 3,
    children = {
        {
            name = "ground",
            kind = "tile_layer",
            legend = {
                ["."] = "grass",
                [":"] = "path",
            },
            tiles = {
                "..::..",
                "..::..",
                "..::..",
            },
        },
        {
            name = "people",
            kind = "node",
        },
        {
            name = "trees",
            kind = "tile_layer",
            legend = {
                T = "tree",
            },
            tiles = {
                "T    T",
                "      ",
                "  T  T",
            },
        },
    },
},
```

A layer's `tiles` is a string for each row, with a character for each column, and its `legend`
says which tile, by its name in the tileset, each character stands for. A space is an empty
cell. Each layer has a legend of its own, so a character can mean different tiles on different
layers.

A tilemap moves with its node, but doesn't turn or stretch, so it has no `rotation`, `scale_x` or
`scale_y`, and its layers are where it is, so they have no `x` or `y` either. A cell blocks
movement if it holds a solid tile on any layer.

### Tileset files

A tileset file names the kinds of tile tilemaps are made of. Like a scene file, it's plain Luau
data:

```lua
-- world.tileset.luau
return {
    format = 1,
    tile_width = 16,
    tile_height = 16,
    sheets = {
        tiles = {
            path = "tiles.png",
            tile_width = 16,
            tile_height = 16,
        },
    },
    tiles = {
        grass = {
            sheet = "tiles",
            cell = { 5, 0 },
            tint = "#3e6b3f",
        },
        tree = {
            sheet = "tiles",
            cell = { 0, 1 },
            tint = "#6aa84f",
            solid = true,
            props = {
                text = "An old oak.",
            },
        },
    },
}
```

Every tile is `tile_width` by `tile_height` in the map, and its sprite, a `cell` of one of the
tileset's `sheets`, stretches to fit. A tile may have a `tint`, be `solid`, and have `props` of its
own, which [`node:tile_props`](api/scene.md#nodetile_props) reads. Name tileset files
`*.tileset.luau`. Many tilemaps, in many scenes, can share one.

The [RPG's](../examples/rpg) maps are scenes like this: a tilemap over
[`world.tileset.luau`](../examples/rpg/world.tileset.luau) with layers for the ground's color,
its sprites and the objects on it, and areas and nodes marking out places, exits, characters and
foes, whose props say the rest.

## Instances

An `instance` node places another scene file, `scene`, where it is: a tree, a villager, a door
that's the same everywhere it's used. It becomes a plain node holding that scene's nodes, with
the scene's props and its own, its own winning where they differ, and any children of its own
after the scene's.

`overrides` changes nodes of the placed scene, by their paths from its top, setting fields as
the scene file would have them:

```lua
{
    name = "miller",
    kind = "instance",
    x = 136,
    y = 80,
    scene = "villager.scene.luau",
    overrides = {
        body = {
            cell = { 24, 0 },
            tint = "#c8b8e8",
        },
        ["body/hat"] = {
            visible = false,
        },
    },
},
```

Any field but `name`, `kind` and `children` can be overridden. `props` change one by one rather
than being replaced, and a `sheet` is one of the sheets of the file the node comes from. A path
can reach into an instance inside the placed scene, such as `["family/mother/body"]`. A scene that
places itself, through its instances or theirs, is an error.

## Files that tools write

The engine can write scenes and tilesets too, from Rust: `SceneFile::write` and
`TilesetFile::write` write one in a single, fixed layout, every field of a node on a line of its
own, in the same order, with fields at their defaults left out, as the examples above are, and a
tile layer's rows one to a line, lined up as the map is. Writing the same scene always gives the
same text, so a change to a scene changes few lines of its file, and version control shows what
changed. Reading what it writes gives back the same scene. It doesn't keep comments.

The scene files of [`examples/garden`](../examples/garden) and the
[RPG's maps](../examples/rpg/maps) are in that layout, as an editor would leave them, and tests
check the garden's stay so.

## Using a scene

[`scene.load`](api/scene.md#sceneload) returns the scene's root node, from which
[`node:find`](api/scene.md#nodefind) reaches any other by its path, and
[`node:tagged`](api/scene.md#nodetagged) finds every node with a tag:

```lua
local garden = scene.load("garden.scene.luau")
local player = garden:find("player")
local solids = garden:tagged("solid")

-- Whether the point `x`, `y` is inside anything solid.
local function blocked(x: number, y: number): boolean
    for _, solid in solids do
        if solid:contains(x, y) then
            return true
        end
    end
    return false
end
```

Props are the game's to read: [`node:props()`](api/scene.md#nodeprops) returns them as a table.
A sign could carry its text, a door the scene it leads to, a spinning thing its speed:

```lua
local sails = garden:find("mill/sails")
local SPEED = sails:props().speed
```

Each `scene.load` returns a fresh copy of the scene, so loading one file twice gives two scenes
that change independently. The file is read once, however many times it's loaded.

A tilemap node answers questions about its map, as a [`Tilemap`](api/tilemap.md#tilemap) does,
in the scene's coordinates, and its layers' cells can change:

```lua
local level = scene.load("level.scene.luau")
local map = level:find("map")
local trees = level:find("map/trees")

if not map:is_solid(column, row) then
    -- Walk there.
end
trees:set_tile(column, row, nil) -- cut the tree down
local x, y = map:tile_to_world(column, row)
```

Nodes come and go with [`node:add`](api/scene.md#nodeadd), which adds a copy of a node, or of a
whole scene, and [`node:remove`](api/scene.md#noderemove):

```lua
-- A bat, from a scene of its own, at the cave's mouth.
local bat = level:find("creatures"):add(scene.load("bat.scene.luau"), "bat_1")
bat:set_position(cave_x, cave_y)

-- Later, beaten.
bat:remove()
```

[`examples/garden`](../examples/garden) plants flowers this way, and picks them.

See [the `scene` reference](api/scene.md) for everything a node can do.
