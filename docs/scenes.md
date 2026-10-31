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
| `kind` | Required. `"node"`, `"sprite"`, `"rect"`, `"text"` or `"area"`. | |
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

## Files that tools write

The engine can write scenes too, from Rust: `SceneFile::write` writes one in a single, fixed
layout, every field of a node on a line of its own, in the same order, with fields at their
defaults left out, as the example above is. Writing the same scene always gives the same text,
so a change to a scene changes few lines of its file, and version control shows what changed.
Reading what it writes gives back the same scene. It doesn't keep comments.

[`examples/garden/garden.scene.luau`](../examples/garden/garden.scene.luau) is in that layout, as
an editor would leave it, and a test checks it stays so.

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

See [the `scene` reference](api/scene.md) for everything a node can do.
