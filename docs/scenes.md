# Scenes

A scene is a tree of sprites, rectangles, text, areas, stacks and panels, and 3D views of meshes,
models, billboards and lights, laid out in a file of its own rather than in code: a level, a
screen, a character. The game loads it with
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
way a tool, such as [the editor](editor.md), can read a file, change it and write it back, and
people can still write and read them by hand. [`examples/garden`](../examples/garden) is a game
built around one, [`examples/orchard`](../examples/orchard) one around a character animated by a
scene's clips, and [`examples/cellar`](../examples/cellar) one whose level is a 3D view.

## A scene file

```lua
-- garden.scene.luau
return {
    format = 4,
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
| `format` | Required: the version of the format, `4`, which added particles and blend modes, as `3` added stacks and panels and `2` 3D views. The engine reads files of formats 1 to 3 too, and refuses files newer than it reads. |
| `sheets` | The sprite sheets the scene's sprites come from, by name: each an image's `path`, relative to the game's folder, divided into sprites `tile_width` by `tile_height` texels, as [`graphics.load_sprite_sheet`](api/graphics.md#graphicsload_sprite_sheet) divides one. |
| `props` | Data for the game about the scene as a whole, such as its music. |
| `children` | The nodes at the top of the scene's tree. |
| `clips` | Animations of the scene's nodes, by name, such as a character's walk. See [Rigging characters](#rigging-characters). |

## Nodes

Every node has a `name` and a `kind`, and may have any of the rest:

| Field | What it is | Default |
|---|---|---|
| `name` | Required. Unique among the node's siblings, and without `/`. | |
| `kind` | Required. `"node"`, `"sprite"`, `"rect"`, `"text"`, `"area"`, `"stack"`, `"panel"`, `"particles"`, `"tilemap"`, `"tile_layer"`, `"instance"`, `"view3d"`, `"mesh"`, `"model"`, `"billboard"` or `"light"`. | |
| `x`, `y` | Where the node's origin is within its parent. | `0` |
| `anchor` | The point of the parent's rectangle that `x` and `y` are measured from, `{ x, y }`, as fractions of its width and height. See [Laying out screens](#laying-out-screens). | the parent's origin |
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
  to stretch it to (both or neither; its own size by default), `stretch`, as a rect's, and
  `tint`, a color its colors are multiplied by.
- **`rect`**: a filled rectangle, `width` by `height`, of `color`. Optionally `stretch`, to grow
  with its parent: see [Laying out screens](#laying-out-screens).
- **`text`**: `text` in `font`, a [font file](api/graphics.md#font-files) by its path from the
  game's folder (the built-in font without one), `font_scale` times its size (1 by default),
  wrapped between words to `wrap` if given, in `color`, and lined up by `align`: `"left"`, the
  default, `"center"` or `"right"`. Text moves with its node, but doesn't turn or stretch.
- **`area`**: a rectangle `width` by `height` that doesn't draw, for marking out part of a scene:
  where the player can't walk, where something happens, a door to another scene, a button to
  click. Like a rect, it may `stretch`.
- **`stack`** and **`panel`**: a rectangle that lays out the nodes inside it one after another,
  such as a menu's items, and fits them; a panel draws behind them, as a box or a frame. See
  [Stacks and panels](#stacks-and-panels).
- **`particles`**: an emitter of particles, for fire, smoke, sparks and the like. See
  [Particles](#particles).
- **`tilemap`** and **`tile_layer`**: a grid of tiles, and its layers. See [Tilemaps](#tilemaps).
- **`instance`**: another scene file, placed here. See [Instances](#instances).
- **`view3d`**, and **`mesh`**, **`model`**, **`billboard`** and **`light`** in it: a 3D world
  shown in a rectangle of the scene, what's in it, and what lights it. See [3D views](#3d-views).

A sprite, rect, text or area has its top-left corner at the node's origin, unless it has a
`pivot`: the point of it, from its top-left corner, `{ x, y }`, that's at the origin instead, and
that it turns and stretches about. A person 16 texels tall standing at `x`, `y` has a pivot of
`{ 8, 16 }`, the middle of their feet, so they stand on the spot, turn about their feet and mirror
in place.

Colors are hex strings such as `"#f2c14e"`, or `"#00000080"` with alpha, and are white if left
out. A sprite, rect, text, tile layer or particles node may have a `blend`, how what it draws mixes
with what's under it: `"normal"`, the default, `"add"`, `"multiply"` or `"screen"`, as
[`graphics.set_blend`](api/graphics.md#blend-modes) has them. A rect of `"multiply"` over a scene
makes it night, and a glow of `"screen"` or `"add"` over that lights it. Props hold booleans, numbers, strings, and tables of them that are lists or have strings for
keys.

Nodes draw in order, each before its children, so later nodes are on top: the ground first, the
HUD last.

Anything wrong with a file is an error when it loads, naming the file and the node: a field a node
doesn't have, a missing one it must have, a sheet the scene doesn't have, or two siblings with the
same name.

```text
scene garden.scene.luau: node `mill/sails`: a plain node has no field `colour`
```

## Laying out screens

Menus and HUDs sit against the canvas's edges, or in its middle, whatever its size. An `anchor`
says which point of the parent's rectangle a node is placed from: `{ 0, 0 }` is its top-left
corner, `{ 0.5, 0.5 }` its middle and `{ 1, 1 }` its bottom-right. `x` and `y` are then measured
from there, so a node `anchor = { 1, 0 }` with `x = -8` stays 8 in from its parent's right edge.
`stretch` makes a sprite, rect, area, stack or panel grow with its parent: it adds those fractions of the
parent's width and height to its own, so `width = 0` with `stretch = { 1, 0 }` is as wide as the
parent, and `width = -16` 16 narrower.

```lua
{
    name = "bar",
    kind = "rect",
    anchor = { 0, 1 },
    width = 0,
    height = 12,
    stretch = { 1, 0 },
    pivot = { 0, 12 },
    color = "#14182c",
    children = {
        {
            name = "score",
            kind = "text",
            x = -4,
            y = 1,
            anchor = { 1, 0 },
            text = "1,250",
            align = "right",
        },
    },
},
```

That's a bar 12 tall along the bottom of the canvas, as wide as it, with a score that ends 4 in
from its right end however long it gets: right-aligned text ends at its origin, and centered text
is centered on it.

Which rectangle a node anchors to:

- A sprite, rect, text, area or tilemap's own, pivot and all.
- For the nodes at the top of a scene, the canvas's. The scene is laid out on the canvas when it's
  loaded and again each time it's drawn, so it follows a window that's resized.
- A plain node covers nothing, so it passes its parent's rectangle on to its children, carried
  along with it as it moves. Grouping nodes in one changes nothing, and moving the group moves
  everything in it, anchored or not.
- A stack or a panel places the nodes inside it itself, inside its padding: see
  [Stacks and panels](#stacks-and-panels).

A node without an `anchor` is placed from its parent's origin, which is where a pivot puts it, as
the parts of a character are. In a rect with a pivot at its bottom, an unanchored child is placed
from its bottom.

The shmup's [panel, title screen and messages](../examples/shmup/hud.scene.luau) are a scene
like this, which [`hud.luau`](../examples/shmup/hud.luau) fills in each frame.

## Stacks and panels

Lists — a menu's items, a panel's lines, a row of icons — are laid out by a `stack`, which places
the nodes inside it one after another, so the game changes what's in the list, or hides some of
it, without placing anything. A `panel` is a stack that draws behind what it holds.

```lua
{
    name = "menu",
    kind = "panel",
    y = 80,
    anchor = { 0.5, 0 },
    gap = 2,
    padding = { 6, 5, 6, 5 },
    color = "#14182c",
    border = 1,
    border_color = "#f2d3ab",
    children = {
        { name = "continue", kind = "text", text = "Continue" },
        { name = "new_game", kind = "text", text = "New Game" },
    },
}
```

That's a menu 80 down the canvas, in the middle of it, as wide as its widest line and its padding,
with each line 2 below the one before. Hide `continue` and `new_game` moves to the top, and the
menu is a line shorter.

| Field | What it is | Default |
|---|---|---|
| `direction` | `"column"`, down from the top, or `"row"`, across from the left. | `"column"` |
| `gap` | The space between each node and the next. | `0` |
| `padding` | The space between the stack's edges and what it holds: a number for every edge, or `{ left, top, right, bottom }`. | `0` |
| `width`, `height` | Its size, with a `stretch`, as an area has. Without one, or with `false`, it fits what it holds that way, with its padding, and can't stretch that way. | fits |
| `pivot` | The point of it at its origin, measured from the point its `anchor` names (its top-left corner without one). | `{ 0, 0 }` |

How a stack places what's inside it:

- Along it, each node takes the space it covers, turned and stretched as it's drawn, after the one
  before and the gap. A hidden node takes no space, and nor does its gap.
- Across it, each node is lined up by its `anchor`: `{ 0.5, 0 }` in a column centers it, and
  `{ 1, 0 }` puts it at the right. Without an anchor, it's at the start. A node that stretches
  across, `width = 0` with `stretch = { 1, 0 }` in a column, is as wide as the stack's inside.
- A node's pivot doesn't move it (it still turns about it), and its `x` and `y` nudge it from where
  the stack puts it, without moving the rest.
- A plain node, and so an instance, has its origin at the start of its space, and takes as much as
  what's inside it reaches past its origin. What's inside anchors and stretches across the stack,
  so a line of a name at its left and a value anchored at its right spans the stack.

A stack's `pivot` is measured from the point its anchor names, so one anchored at the bottom-right
corner of its parent, `anchor = { 1, 1 }`, keeps its own bottom-right corner there however much it
holds, growing up and to the left.

A panel draws one of two looks:

- Filled with `color`, inside a `border` that many pixels wide of `border_color`, as above. Both
  colors are white if left out, and there's no border unless it's given.
- A sprite from one of the scene's sheets, `sheet` and `cell`, sliced as a frame is: `slice`, a
  number or `{ left, top, right, bottom }`, is how many texels along its edges keep their size, so
  its corners stay as they're drawn and the rest stretches to fit, times `tint`. That's
  [`graphics.draw_sprite_sliced`](api/graphics.md#graphicsdraw_sprite_sliced).

The RPG's [title menu](../examples/rpg/title.scene.luau) is a panel showing its frame sprite,
whose choices are copies of a hidden row that [`title.luau`](../examples/rpg/title.luau) adds with
[`node:add`](api/scene.md#nodeadd), and its [bag's stats](../examples/rpg/bag.scene.luau) are
another. The shmup's panel stacks its rows, and hides the play's to show the stress test's.

## Particles

A `particles` node is an emitter: it sends particles out, small sprites or squares that fly,
change color and size as they age, and vanish. Its fields are the
[particle settings](api/graphics.md#particle-settings) `graphics.new_particles` takes, and:

| Field | What it is | Default |
|---|---|---|
| `sheet`, `cell` | The sprite each particle shows, as a sprite node's. | a white square |
| `width`, `height` | The rectangle particles start in, at random points of it. | `0`, a point |
| `stretch`, `pivot` | As an area's. | |

```lua
{
    name = "flames",
    kind = "particles",
    sheet = "fx",
    cell = { 4, 0 },
    width = 10,
    height = 2,
    pivot = { 5, 2 },
    rate = 40,
    warmup = 1,
    lifetime = { 0.4, 0.8 },
    speed = { 10, 22 },
    direction = -1.5707964,
    spread = 0.5,
    gravity = { 0, -24 },
    scales = { 1.5, 0.5 },
    colors = { "#fff2a0", "#ffa040", "#ff482000" },
    blend = "add",
},
```

Particles start where the node is laid out, and fly the way it's turned. Those that aren't
`attached` stay where they started in the scene, so a moving node leaves a trail; attached ones
move, turn and stretch with it. They draw where the node is among the scene's nodes, after what's
before it and under what's after, however far they've flown from it.

Scenes keep no time of their own, so a game moves particles on with
[`node:advance_particles(dt)`](api/scene.md#nodeadvance_particles), on the scene's root to advance
them all, or on part of the scene to advance only that. Each emitter's randomness starts from its
path, so emitters alike look different, and a scene plays the same every time. A copy made with
[`node:add`](api/scene.md#nodeadd) starts over, with none of its source's particles.
[`examples/bonfire`](../examples/bonfire) is a night made of them: stars, smoke, flames, sparks and
fireflies.

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
`scale_y`, and its layers are where it is, so they have no `x`, `y` or `anchor` either. A cell
blocks movement if it holds a solid tile on any layer, and a tilemap added to a
[physics world](api/physics.md) is solid there.

### Tileset files

A tileset file names the kinds of tile tilemaps are made of. Like a scene file, it's plain Luau
data:

```lua
-- world.tileset.luau
return {
    format = 4,
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

A stack or panel's `width = false` or `height = false` restores fitting that axis instead of
inheriting a fixed size; its `stretch` must be zero that way too. A panel's appearance can be
replaced: setting `sheet` and `cell` replaces its fill and border with a sprite, and setting
`color`, `border` or `border_color` replaces its sprite with a fill. Fields from the previous
appearance are discarded; explicitly mixing fill and sprite fields in one override is an error.

## 3D views

A `view3d` node is a rectangle of the scene showing a 3D world through a camera, as
[`graphics.begin_3d`](api/graphics.md#graphicsbegin_3d) shows one, and its children are that
world's nodes: meshes, models made in tools such as Blender, sprites standing among them,
billboards, and lights at places, such as lamps. A level of a 2.5D game is one; so is a 3D picture
in a menu.

```lua
{
    name = "view",
    kind = "view3d",
    width = 0,
    height = 0,
    stretch = { 1, 1 },
    camera = {
        position = { 5.5, 7, 9 },
        target = { 5.5, 0, 5.5 },
    },
    lighting = {
        color = "#b0a8a0",
        ambient = "#505060",
    },
    children = {
        {
            name = "floor",
            kind = "mesh",
            x = 5.5,
            z = 3.5,
            shape = "plane",
            size = { 11, 7 },
            tint = "#4a4038",
        },
        {
            name = "crate",
            kind = "model",
            x = 2.5,
            z = 1.5,
            model = "models/crate.glb",
            tags = { "solid" },
        },
        {
            name = "hero",
            kind = "billboard",
            x = 5.5,
            z = 5.5,
            sheet = "tiles",
            cell = { 27, 0 },
            height = 0.9,
        },
        {
            name = "lamp",
            kind = "light",
            x = 2.5,
            y = 3,
            z = 1.5,
            pitch = 1.5707964,
            color = "#ffd9a0",
            range = 6,
            cone = 0.6,
        },
    },
},
```

A view is laid out as an area is, `width` by `height` with a `stretch` and a `pivot`, so this one
fills the canvas; it moves with its node, but doesn't turn or stretch. Its other fields:

- `camera`, required: `position` and `target`, `{ x, y, z }`, where it is and the point it looks at,
  in the middle of the view. Optionally `up`, which way is up the view (`{ 0, 1, 0 }`); `fov`, the
  angle the view takes in from its bottom edge to its top, in radians (`0.8`), or instead `height`,
  how many world units it shows from bottom to top, for an orthographic camera, under which things
  look the same size however far away they are; and `near` and `far`, how near and far things show
  (`0.1` and `1000`).
- `lighting`: `direction`, the way the sun's light goes, `{ x, y, z }`; `color`, the sun's light;
  and `ambient`, the light on every surface. Each is the default's if left out, and so is the
  whole of it: a sun high above and in front. Lights at places aren't listed here: they're the
  view's `light` nodes.

The world is right-handed with `y` up, as glTF's is. The kinds of node in it:

- **`mesh`**: a built-in shape, `shape`, centered on its origin: `"cuboid"`, `size = { width,
  height, depth }`; `"plane"`, flat and facing up, `size = { width, depth }`; or `"sphere"`, of a
  `radius`. Sizes are 1, and a sphere's radius a half, if left out. Optionally a sprite, `sheet`
  and `cell`, shown on each face, and a `tint`.
- **`model`**: the parts of a model file, `model`, a `.glb` or `.gltf` file's path from the
  game's folder, as [`graphics.load_model`](api/graphics.md#graphicsload_model) loads it, and a
  `tint`.
- **`billboard`**: a sprite, `sheet` and `cell`, standing upright with the middle of its bottom
  edge at the node's origin, turned to face the camera across the ground, `width` by `height`
  world units (1 by default; a negative width mirrors it), and a `tint`. It moves with its node,
  but doesn't turn or stretch.
- **`light`**: a light at the node's origin, of a `color` (white), reaching `range` world units
  (5), above 0, its light falling off in a straight line to nothing there, and giving `energy`
  times its color (1), 0 or more. With a `cone`, half the cone's angle in radians, above 0 and up
  to pi, it's a spot light, shining along its node's `+z`, the way a model's front faces, and
  fading across the `edge` of the cone, a part of it from its rim inward, from 0 to 1 (0, a hard
  rim). Without a `cone` it's a point light, shining every way, and has no `edge`. It moves and
  turns with its node, but doesn't stretch.
- **`node`** and **`instance`**: a place, and a scene file placed here, as in the 2D scene. A
  prop can be a scene of its own, of meshes or models, placed into levels by instances and changed
  by overrides.

Nodes in a view are placed in 3D, by `x`, `y` and `z`; `yaw`, `pitch` and `roll`, radians about
`y`, `x` and `z`, counter-clockwise seen from each axis's end; and `scale_x`, `scale_y` and
`scale_z`, 0 and 1 by default. A node is stretched, then rolled, pitched and turned, then moved,
about its origin, as [`graphics.draw_mesh`](api/graphics.md#graphicsdraw_mesh) places a mesh, and
its children with it. They have no `rotation` or `anchor`, which are the 2D scene's.

Only views hold meshes, models, billboards and lights, and views hold only them, plain nodes and
instances: a sprite in a view, or a mesh outside one, is an error naming the node. That's checked
once instances have placed their scenes, so a prop's own file can hold a model at its top, and be
placed in a view. Plain nodes there are placed in 3D too, at the defaults if their file doesn't
say.

A view's lights add to its sun and ambient light, as the lights of
[`graphics.begin_3d`](api/graphics.md#graphicsbegin_3d) do, which says how a light lights a
surface. A light lights everything in its view within its range, wherever it is in the view's
tree, and casts no shadows: it shines through walls. The example's lamp is a spot light over the
crate, turned a quarter turn by its `pitch` to shine straight down. A light that isn't `visible`,
or is inside a node that isn't, is out, and a [clip](#clips) that moves or turns a node moves or
turns its light, so a lamp can swing. A view is lit by 32 lights at once: of those that reach what
its camera sees, the nearest to the camera. Scripts read and change a light with
[`node:light`](api/scene.md#nodelight) and [`node:set_light`](api/scene.md#nodeset_light).

Meshes and models hide what's behind them, whatever order they're drawn in; the view draws over
what was drawn before it, and nodes after it draw over it. Scripts move nodes in views with
[`node:set_position_3d`](api/scene.md#nodeset_position_3d) and
[`node:set_orientation`](api/scene.md#nodeset_orientation), follow things with
[`view:set_camera`](api/scene.md#nodeset_camera), and find what's under the mouse with
[`view:pick`](api/scene.md#nodepick), which gives the nodes whose boxes are under a point,
nearest first:

```lua
local level = scene.load("cellar.scene.luau")
local view = level:find("view")

function update(dt: number)
    local x, y = input.mouse_position()
    if x and y then
        local under = view:pick(x, y)[1]
        if under and under:has_tag("solid") then
            -- Point at it.
        end
    end
end
```

[`examples/cellar`](../examples/cellar)'s level is a view like this, whose walls are tagged
`solid` and candles `candle`, which [`main.luau`](../examples/cellar/main.luau) reads.

## Rigging characters

A character can be a rig: a sprite for each part, the head, the body, each half of each arm and
leg, every part the child of the part it hangs from, with its pivot at its joint. Turning an upper
arm then turns the forearm and hand with it, as a cutout puppet's would. Parts are rigid; the
engine doesn't bend sprites.

```lua
{
    name = "hips",
    kind = "node",
    y = -14,
    children = {
        {
            name = "leg",
            kind = "sprite",
            sheet = "parts",
            cell = { 3, 1 },
            pivot = { 7.5, 3 },
            children = {
                {
                    name = "shin",
                    kind = "sprite",
                    y = 6,
                    sheet = "parts",
                    cell = { 4, 1 },
                    pivot = { 7.5, 3 },
                },
            },
        },
    },
},
```

Nodes draw in the order they're in, so a part that should be behind another comes first. When a
part must move with one node but draw in front of another, such as a near arm that moves with
the body but is in front of the near leg, give it a plain node of its own, where it should draw,
and turn that node as the body turns.

### Clips

Clips animate a rig. The file's `clips` names them, each with its `duration` in seconds, whether
it `loop`s, and its `tracks`:

```lua
clips = {
    walk = {
        duration = 0.4,
        loop = true,
        tracks = {
            {
                node = "hips/leg",
                property = "rotation",
                keys = {
                    { 0, -0.6 },
                    { 0.2, 0.6 },
                    { 0.4, -0.6 },
                },
            },
            {
                node = "hips",
                property = "y",
                interpolation = "step",
                keys = {
                    { 0, -14 },
                    { 0.2, -13 },
                },
            },
        },
    },
},
```

| Field | What it is | Default |
|---|---|---|
| `duration` | Required. Seconds the clip lasts, above 0. | |
| `loop` | Whether it starts over at its end, rather than holding its last pose. | `false` |
| `tracks` | What it changes: a list of tracks. | none |

Each track changes one property of one node:

| Field | What it is | Default |
|---|---|---|
| `node` | The node's path from the node the clip poses. | that node itself |
| `property` | Required. `"x"`, `"y"`, `"rotation"`, `"scale_x"`, `"scale_y"`, `"visible"` or `"cell"` (a sprite's, a billboard's or a mesh's), and for nodes in 3D views, `"z"`, `"yaw"`, `"pitch"`, `"roll"` and `"scale_z"`, but not `"rotation"`. | |
| `interpolation` | How a number gets from one key to the next: `"linear"`, in a straight line, or `"step"`, jumping at each key. `visible` and `cell` always step. | `"linear"` |
| `keys` | Required. Pairs of a time, in seconds from 0 to the duration, and the value then, `{ time, value }`, in order of time. | |

Before its first key, a property has the first key's value, and after its last, the last's. A
loop that comes back to where it started has its first key's value again at its end. Stepping a
`cell` swaps a sprite for another of its sheet, such as a face that blinks; stepping `visible`
shows and hides a part.

A clip doesn't keep time, as an [`Animation`](api/graphics.md#animation) doesn't: the game keeps
how long it has played, and [poses](api/scene.md#nodepose) a node with it then.
[`scene.clips`](api/scene.md#sceneclips) returns a file's clips:

```lua
local hero = scene.load("level.scene.luau"):find("hero")
local clips = scene.clips("hero.scene.luau")
local time = 0

function update(dt: number)
    time += dt
    hero:pose(clips.walk, time)
end
```

Tracks find their nodes by path from the node posed, so one clip poses each instance of its file:
here, `hero` is an instance of `hero.scene.luau` in the level.

A clip sets only the properties it has tracks for; the others keep whatever they had. Games
layer clips by posing with one after another: a pose with a weight below 1 moves numbers only that
fraction of the way to the clip's, and changes `visible` and `cell` only from a weight of a half.
So a game that poses with `idle`, then with `walk` at a weight rising from 0 to 1, crossfades from
standing to walking. Pose first with a clip that has tracks for everything the others change, so
nothing keeps a pose from an earlier frame:

```lua
hero:pose(clips.idle, time)
hero:pose(clips.walk, walk_time, walking) -- `walking` eases between 0 and 1
hero:pose(clips.blink, time)
```

The picker in [`examples/orchard`](../examples/orchard) is a rig like this, with clips for
standing, walking, blinking, catching and missing apples, and giving up, which
[`main.luau`](../examples/orchard/main.luau) layers each frame.

## Files that tools write

The engine can write scenes and tilesets too, from Rust: `SceneFile::write` and
`TilesetFile::write` write one in a single, fixed layout, every field of a node on a line of its
own, in the same order, with fields at their defaults left out, as the examples above are, and a
tile layer's rows one to a line, lined up as the map is. Writing the same scene always gives the
same text, so a change to a scene changes few lines of its file, and version control shows what
changed. Reading what it writes gives back the same scene. It doesn't keep comments.

Every example's scene files are in that layout, as [the editor](editor.md) leaves them, and each
game's tests check they stay so. A clip's keys go one to a line, and a camera's and lighting's
fields too. Files are written at the newest format, 4.

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

A scene drawn without a camera is in canvas pixels, which is where
[`input.mouse_position`](api/input.md#inputmouse_position) is too, so
[`node:contains`](api/scene.md#nodecontains) tells which button the mouse is over:

```lua
local menu = scene.load("menu.scene.luau")
local buttons = menu:tagged("button")

function update(dt: number)
    local x, y = input.mouse_position()
    if x and y and input.mouse_pressed("left") then
        for _, button in buttons do
            if button:contains(x, y) then
                start(button:props().action)
            end
        end
    end
end

function draw()
    graphics.draw_scene(menu)
end
```

For a menu the keys move through as well as the mouse, [`ui.menu`](api/ui.md) makes one of a
node's children, such as a stack's rows, or the nodes inside it with a tag. It moves the selection
by where the items are drawn, wraps, passes over items that can't be picked, and says each update
what the player did, while the game shows the selection however it likes:

```lua
local menu = ui.menu(screen:find("buttons"))

function update(dt: number)
    local happened, item = menu:update()
    if happened == "picked" then
        start(menu:items()[item]:props().action)
    end
end
```

The RPG's title screen and questions, and the shmup's title menu, are picked this way.

See [the `scene` reference](api/scene.md) for everything a node can do.
