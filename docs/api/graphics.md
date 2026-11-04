# `graphics`

Drawing, the camera, sprites, animations, text, and 3D views.

Everything drawn in a frame is drawn in order, later things on top of earlier ones, onto a canvas
that starts the frame cleared to black. 3D views take their place in that order too. Drawing
functions work only in `draw`.

- [Drawing](#drawing): [`clear`](#graphicsclear), [`fill_rect`](#graphicsfill_rect),
  [`outline_rect`](#graphicsoutline_rect), [`draw_line`](#graphicsdraw_line),
  [`fill_circle`](#graphicsfill_circle), [`outline_circle`](#graphicsoutline_circle),
  [`draw_sprite`](#graphicsdraw_sprite), [`draw_sprite_rotated`](#graphicsdraw_sprite_rotated),
  [`draw_text`](#graphicsdraw_text), [`draw_text_layout`](#graphicsdraw_text_layout),
  [`draw_tilemap`](#graphicsdraw_tilemap), [`draw_scene`](#graphicsdraw_scene)
- [The camera](#the-camera): [`set_camera`](#graphicsset_camera)
- [The canvas and frames](#the-canvas-and-frames): [`size`](#graphicssize),
  [`color`](#graphicscolor), [`frame_stats`](#graphicsframe_stats)
- [Sprites](#sprites): [`load_sprite_sheet`](#graphicsload_sprite_sheet),
  [`SpriteSheet`](#spritesheet), [`Sprite`](#sprite)
- [Animation](#animation): [`new_animation`](#graphicsnew_animation), [`Animation`](#animation-1)
- [Text](#text): [`default_font`](#graphicsdefault_font), [`Font`](#font),
  [`TextLayout`](#textlayout)
- [3D views](#3d-views): [`begin_3d`](#graphicsbegin_3d), [`draw_mesh`](#graphicsdraw_mesh),
  [`draw_billboard`](#graphicsdraw_billboard), [`project`](#graphicsproject),
  [`ray`](#graphicsray)
- [Meshes](#meshes): [`new_cube`](#graphicsnew_cube), [`new_cuboid`](#graphicsnew_cuboid),
  [`new_plane`](#graphicsnew_plane), [`new_sphere`](#graphicsnew_sphere),
  [`new_mesh`](#graphicsnew_mesh), [`Mesh`](#mesh)

## Drawing

### `graphics.clear`

```lua
graphics.clear(color: Color)
```

Fills the canvas with `color`, discarding anything drawn so far this frame. Games usually call it
first in `draw`; without it, the canvas is black.

### `graphics.fill_rect`

```lua
graphics.fill_rect(x: number, y: number, width: number, height: number, color: Color)
```

Fills a rectangle with its top-left corner at `x`, `y`. A translucent color blends with what's
under it.

```lua
-- A half-transparent black panel over the bottom of a 320x180 canvas.
graphics.fill_rect(0, 140, 320, 40, "#00000080")
```

Like everything drawn, a shape covers the pixels whose middles are inside it. Pixel `x`, `y` is
the square from `x`, `y` to `x + 1`, `y + 1`, so a rectangle at whole numbers covers whole pixels,
and a 1-pixel line or outline is crisp along the middles of pixels, at `x + 0.5`.

### `graphics.outline_rect`

```lua
graphics.outline_rect(x: number, y: number, width: number, height: number, color: Color, thickness: number?)
```

Draws the outline of the rectangle `fill_rect` would fill, `thickness` wide (by default 1) inside
its edges. Its sides don't overlap, so a translucent outline is as see-through at its corners as
along its sides. An outline thick enough to meet itself fills the rectangle.

```lua
-- A hitbox, outlined over the fighter, with its bounds from a scene node.
local box = hitbox:bounds()
graphics.outline_rect(box.x, box.y, box.width, box.height, "#ff4040c0")
```

### `graphics.draw_line`

```lua
graphics.draw_line(x1: number, y1: number, x2: number, y2: number, color: Color, thickness: number?)
```

Draws a line from `x1`, `y1` to `x2`, `y2`, `thickness` wide (by default 1), centered on the line
between them and ending square at them. A line from a point to itself draws nothing.

A line 1 wide lights a row or column of pixels when it runs along their middles, so to draw a line
across the pixels of row 5, from column 0 to column 10:

```lua
graphics.draw_line(0, 5.5, 11, 5.5, "#ffffff") -- the pixels fill_rect(0, 5, 11, 1) fills
```

### `graphics.fill_circle`

```lua
graphics.fill_circle(x: number, y: number, radius: number, color: Color)
```

Fills a circle of `radius` about `x`, `y`. It's drawn as a polygon with as many sides as its size
on the canvas needs to look round, more for larger circles.

A circle about the corner between four pixels covers whole pixels evenly on all sides, so one of
`radius` 3 about `10`, `10` is 6 pixels across. About the middle of a pixel, such as `10.5`,
`10.5`, it has a pixel in the middle, so its width in pixels is odd.

### `graphics.outline_circle`

```lua
graphics.outline_circle(x: number, y: number, radius: number, color: Color, thickness: number?)
```

Draws the outline of the circle `fill_circle` would fill, `thickness` wide (by default 1) inside
its edge, as evenly as `outline_rect`. An outline thick enough to meet itself fills the circle.

```lua
-- A shield's bubble, shrinking as it wears down.
graphics.outline_circle(fighter.x, fighter.y - 12, 6 + 12 * shield, "#80ccff", 2)
```

### `graphics.draw_sprite`

```lua
graphics.draw_sprite(sprite: Sprite, x: number, y: number, tint: Color?, width: number?, height: number?)
```

Draws `sprite` with its top-left corner at `x`, `y`, stretched to `width` by `height` (by
default, its own size in texels), with its colors multiplied by `tint` (by default, white, which
leaves them as they are).

Tinting a white sprite colors it; tinting with alpha fades it:

```lua
graphics.draw_sprite(coin, 40, 20)
graphics.draw_sprite(coin, 60, 20, "#ff8080") -- reddened
graphics.draw_sprite(coin, 80, 20, { r = 1, g = 1, b = 1, a = 0.5 }) -- half faded
graphics.draw_sprite(coin, 100, 20, nil, 32, 32) -- twice the size of a 16x16 sprite
```

Sprites are sampled nearest-neighbor, so pixel art stays crisp. Consecutive sprites from the same
image are drawn together in one draw call, so drawing everything from one sprite sheet in a row is
cheaper than alternating between sheets.

### `graphics.draw_sprite_rotated`

```lua
graphics.draw_sprite_rotated(sprite: Sprite, x: number, y: number, angle: number, tint: Color?, width: number?, height: number?)
```

Draws `sprite` as [`draw_sprite`](#graphicsdraw_sprite) does, then turned by `angle` radians about
the center of where it's drawn. Positive angles turn clockwise. To draw a sprite turned about its
own center at `cx`, `cy`:

```lua
local width, height = arrow:size()
graphics.draw_sprite_rotated(arrow, cx - width / 2, cy - height / 2, angle)
```

### `graphics.draw_text`

```lua
graphics.draw_text(font: Font, text: string, x: number, y: number, color: Color)
```

Draws `text` with its top-left corner at `x`, `y`, rounded to whole pixels so pixel fonts stay
crisp. `"\n"` starts a new line. Text isn't wrapped; lay it out with
[`font:layout`](#fontlayout) to wrap it.

```lua
local font = graphics.default_font()

function draw()
    graphics.draw_text(font, `Score: {score}`, 4, 4, "#ffffff")
end
```

### `graphics.draw_text_layout`

```lua
graphics.draw_text_layout(layout: TextLayout, x: number, y: number, color: Color, glyphs: number?)
```

Draws text laid out with [`font:layout`](#fontlayout), as `draw_text` draws text. With `glyphs`,
it draws only that many of the layout's glyphs, rounded down, so drawing a growing number reveals
the text a little at a time, as dialogue does in RPGs. Since the text is laid out once, whole, words
don't jump from one line to the next as they appear.

```lua
local layout = font:layout("Welcome to Mossbrook. The elder is expecting you.", 200)
local shown = 0
-- Characters a second.
local SPEED = 30

function update(dt: number)
    shown = math.min(shown + SPEED * dt, layout:glyph_count())
end

function draw()
    graphics.draw_text_layout(layout, 60, 120, "#ffffff", shown)
end
```

### `graphics.draw_tilemap`

```lua
graphics.draw_tilemap(map: Tilemap, layer: number?)
```

Draws `map`, every layer in order, or only `layer`. The map's top-left corner is at the world's
origin, so draw it with a [camera](#graphicsset_camera) set. Only the cells the canvas shows are
drawn, so large maps cost no more than small ones.

Drawing one layer at a time lets a game draw characters between layers, such as above the ground
but under treetops:

```lua
graphics.draw_tilemap(map, 0) -- the ground
draw_characters()
graphics.draw_tilemap(map, 1) -- treetops and roofs
```

### `graphics.draw_scene`

```lua
graphics.draw_scene(node: SceneNode)
```

Draws `node` and every node inside it, each before its children, where they are in their
[scene](scene.md): moved, turned and stretched by every node they're inside. Hidden nodes, and
nodes inside hidden ones, don't draw, and nothing is drawn where it's off the canvas. Pass a
scene's root, from [`scene.load`](scene.md#sceneload), to draw the whole scene.

The scene's coordinates are the ones drawing uses: canvas pixels, or with a
[camera](#graphicsset_camera) set, the world's. Drawing first lays the scene out on the canvas,
so nodes [anchored](../scenes.md#laying-out-screens) to its edges follow them when it changes
size.

```lua
local level = scene.load("level.scene.luau")

function draw()
    graphics.set_camera(player_x, player_y)
    graphics.draw_scene(level:find("world"))
    graphics.set_camera(nil)
    graphics.draw_scene(level:find("hud"))
end
```

## The camera

### `graphics.set_camera`

```lua
graphics.set_camera(x: number, y: number, bounds: Rect?, zoom: number?)
graphics.set_camera(nil)
```

Centers the view on world position `x`, `y` for everything drawn after this call, so positions
are world positions rather than canvas pixels. With `bounds`, such as a map's
[`map:bounds()`](tilemap.md#mapbounds), the view stays inside them, so a camera following the
player stops at the map's edges. Where the view is larger than the bounds, it centers them.

`zoom` scales the view about its center: each world unit takes `zoom` canvas pixels, so at 2 the
world is drawn twice as large, and the view shows half as much of it each way, and at 0.5 half as
large. It's 1 by default, and must be above 0. Everything drawn with the camera is scaled, text
included. Bounds keep the view inside them at whatever zoom it's at, so zooming out until the view
is larger than the bounds centers them.

`graphics.set_camera(nil)` goes back to drawing in canvas pixels, as for a HUD. Every frame starts
with no camera.

```lua
function draw()
    -- The world, centered on the player's middle and kept inside the map.
    graphics.set_camera(player.x + 8, player.y + 8, map:bounds())
    graphics.draw_tilemap(map)
    graphics.draw_sprite(player.sprite, player.x, player.y)

    -- The HUD, in canvas pixels.
    graphics.set_camera(nil)
    graphics.draw_text(font, `HP {player.hp}`, 4, 4, "#ffffff")
end
```

A camera that keeps two players in view zooms out as they move apart:

```lua
-- Zoomed to fit both players with 48 pixels to spare around them, but no further in than 2.
local width, height = graphics.size()
local spread_x = math.abs(a.x - b.x) + 96
local spread_y = math.abs(a.y - b.y) + 96
local zoom = math.min(2, width / spread_x, height / spread_y)
graphics.set_camera((a.x + b.x) / 2, (a.y + b.y) / 2, stage_bounds, zoom)
```

The view moves in whole canvas pixels, so the world doesn't shimmer as the camera moves by
fractions of a pixel. Shaking the view is a matter of moving the camera by a few pixels for a few
frames. The camera costs nothing to set, so set it as often as needed.

At whole-number zooms, every texel of pixel art covers the same number of canvas pixels. At other
zooms some cover one more than others, so pixel art looks uneven while it's there: a camera easing
between zooms looks best ending on a whole number.

## The canvas and frames

### `graphics.size`

```lua
graphics.size(): (number, number)
```

Returns the canvas's width and height in canvas pixels. With a fixed
[`resolution`](../project-structure.md#resolution), that's the resolution. Without one, it's the
window's size in logical pixels, which changes as the player resizes the window, so games without
a fixed resolution lay themselves out from it each frame. While the game loads, it's the size the
first frame will have.

### `graphics.color`

```lua
graphics.color(color: Color): number
```

Returns `color` as a number `0xRRGGBBAA`. Numbers are the fastest way to pass a color: the engine
reads a hex string or table again every time it's passed, which costs more than drawing a sprite.
Code that draws a lot turns its colors into numbers once:

```lua
local BULLET = graphics.color("#f2c14e")

function draw()
    for _, bullet in bullets do
        graphics.draw_sprite(pellet, bullet.x, bullet.y, BULLET)
    end
end
```

### `graphics.frame_stats`

```lua
graphics.frame_stats(): FrameStats
```

Returns how long the frame before this one took, and how much it drew, as a table. Times are in
seconds, measured on the CPU:

| Field | What it is |
|---|---|
| `frame` | The whole frame, from its start to the next frame's start: updating, drawing, rendering, and waiting for vsync and the GPU. |
| `update` | The frame's updates: `update`, the tasks and native libraries' update hooks. |
| `draw` | `draw` and native libraries' draw hooks: recording what to draw. |
| `render` | Rendering what was drawn: uploading it to the GPU and submitting draw calls. Not the GPU's own work. |
| `quads` | How many quads the frame drew: sprites, rectangles and glyphs of text. |
| `triangles` | How many triangles of meshes and billboards the frame drew in [3D views](#3d-views). |
| `draw_calls` | How many draw calls those took: one for each run of quads with the same texture, and one for each mesh or billboard. |

Before the first frame has finished, everything is 0. To see what a frame really costs, turn
[`vsync`](../project-structure.md#vsync) off, and run a release build. See
[Measuring performance](../running-games.md#measuring-performance).

## Sprites

A sprite is a rectangle of an image, to draw. Sprites come from sprite sheets: images divided into
a grid of sprites the same size.

### `graphics.load_sprite_sheet`

```lua
graphics.load_sprite_sheet(path: string, tile_width: number, tile_height: number): SpriteSheet
```

Loads the PNG image at `path`, relative to the game's folder, divided into a grid of sprites
`tile_width` by `tile_height` texels, starting at its top-left corner, with no spacing between
them. Partial sprites at the right and bottom edges are left out.

Each image is read once: loading it again, even divided differently, reuses it, so one image can
hold sprites of different sizes:

```lua
local small = graphics.load_sprite_sheet("bullets.png", 8, 8)
local large = graphics.load_sprite_sheet("bullets.png", 16, 16)
```

For an image that's a single sprite, use its whole size: `load_sprite_sheet("title.png", 320,
180):sprite(0, 0)`.

Works while the game loads and in `update`.

### `SpriteSheet`

#### `sheet:sprite`

```lua
sheet:sprite(column: number, row: number): Sprite
```

Returns the sprite at `column` and `row`, counting from 0 at the top-left. A position outside the
sheet is an error.

#### `sheet:size`

```lua
sheet:size(): (number, number)
```

Returns how many columns and rows of sprites the sheet has.

### `Sprite`

#### `sprite:size`

```lua
sprite:size(): (number, number)
```

Returns the sprite's width and height in texels.

#### `sprite:flipped_x`

```lua
sprite:flipped_x(): Sprite
```

Returns a copy of the sprite mirrored left to right, such as a character facing the other way.

#### `sprite:flipped_y`

```lua
sprite:flipped_y(): Sprite
```

Returns a copy of the sprite mirrored top to bottom.

## Animation

An animation is a list of sprites shown one after another at a steady pace, such as a walk cycle.
It doesn't keep time itself: the game keeps how long it has been playing, and asks for the frame
to draw. So one animation can be shared by every character that plays it, each at its own time.

```lua
local walk = graphics.new_animation({ sheet:sprite(0, 2), sheet:sprite(1, 2) }, 0.2)
local walk_time = 0

function update(dt: number)
    if player.walking then
        walk_time += dt
    else
        walk_time = 0
    end
end

function draw()
    graphics.draw_sprite(walk:frame(walk_time), player.x, player.y)
end
```

### `graphics.new_animation`

```lua
graphics.new_animation(frames: { Sprite }, frame_time: number): Animation
```

Returns an animation that shows each sprite in `frames` for `frame_time` seconds, in order, then
starts over. There must be at least one frame, and `frame_time` must be above 0.

### `Animation`

#### `animation:frame`

```lua
animation:frame(time: number): Sprite
```

Returns the sprite showing `time` seconds after the animation started. Before it starts, at a
negative time, that's the first frame.

#### `animation:frame_index`

```lua
animation:frame_index(time: number): number
```

Returns the number of the frame showing at `time`, counting from 1, like the table of frames the
animation was made from.

#### `animation:frame_count`

```lua
animation:frame_count(): number
```

Returns how many frames the animation has.

#### `animation:duration`

```lua
animation:duration(): number
```

Returns the seconds it takes to show every frame once.

#### `animation:once`

```lua
animation:once(): Animation
```

Returns a copy that plays once, then holds its last frame, rather than starting over, such as for
an explosion.

#### `animation:is_finished`

```lua
animation:is_finished(time: number): boolean
```

Returns whether an animation made with `once` has finished showing its last frame by `time`. One
that loops never finishes.

#### `animation:flipped_x`

```lua
animation:flipped_x(): Animation
```

Returns a copy with every frame mirrored left to right.

#### `animation:flipped_y`

```lua
animation:flipped_y(): Animation
```

Returns a copy with every frame mirrored top to bottom.

## Text

The engine has one font built in: a pixel font of printable ASCII, the ellipsis (`…`) and en and
em dashes (`–`, `—`), with capitals 7 pixels tall and lines 10 pixels apart. Characters it lacks draw as `?`. Measured widths run to the edge of the last
letter's ink, so text centers exactly.

### `graphics.default_font`

```lua
graphics.default_font(): Font
```

Returns the built-in pixel font.

### `Font`

#### `font:scaled`

```lua
font:scaled(scale: number): Font
```

Returns the font drawn `scale` times as large, such as for a title. Whole numbers keep a pixel
font crisp. `scale` must be above 0.

#### `font:measure`

```lua
font:measure(text: string): (number, number)
```

Returns the width and height of `text`, drawn without wrapping. The width runs to the right edge
of the ink on the longest line; the height is a whole number of lines. To center text:

```lua
local width, height = font:measure("Game over")
local canvas_width, canvas_height = graphics.size()
graphics.draw_text(font, "Game over", (canvas_width - width) / 2, (canvas_height - height) / 2, "#ffffff")
```

#### `font:line_height`

```lua
font:line_height(): number
```

Returns the distance from one line of text to the next.

#### `font:layout`

```lua
font:layout(text: string, max_width: number?): TextLayout
```

Lays `text` out once, to draw with [`graphics.draw_text_layout`](#graphicsdraw_text_layout). With
`max_width`, lines wrap between words to fit it, and a word too long for a line of its own breaks
between letters. `"\n"` always starts a new line.

### `TextLayout`

#### `layout:size`

```lua
layout:size(): (number, number)
```

Returns the width and height the text takes up, measured as [`font:measure`](#fontmeasure)
measures.

#### `layout:glyph_count`

```lua
layout:glyph_count(): number
```

Returns how many glyphs there are to draw. Spaces and line breaks don't count, so revealing one
glyph at a time never pauses on a space.

#### `layout:line_count`

```lua
layout:line_count(): number
```

Returns how many lines the text takes.

## 3D views

A 3D view shows meshes, and sprites standing among them, in a 3D world, on all or part of the
canvas. It takes its place among what's drawn in 2D: what's drawn before it is under it, and what's
drawn after it, such as a HUD, is over it. Within a view, nearer things hide farther ones, whatever
order they're drawn in. A view renders at the canvas's resolution, so with a fixed
[`resolution`](../project-structure.md#resolution) it has the same large pixels as the rest of the
game.

3D positions are Luau's vectors, made with `vector.create(x, y, z)`, which add, subtract and
scale with `+`, `-` and `*`. Coordinates are right-handed with `y` up: looking along `-z`, `x` points
right and `z` toward the camera. A world unit is whatever the game makes it; a tile or a crate is
a good size for 1. Angles in 3D are radians, and positive angles turn counter-clockwise about
their axis, seen from its positive end.

```lua
local crate = graphics.new_cube(1)
local ground = graphics.new_plane(6, 6)
local hero = graphics.load_sprite_sheet("tiles.png", 16, 16):sprite(27, 0)
local font = graphics.default_font()

local view = {
    camera = { position = vector.create(0, 4, 6), target = vector.create(0, 0.5, 0) },
}

function draw()
    graphics.clear("#8cc4e8")
    graphics.begin_3d(view)
    graphics.draw_mesh(ground, vector.zero, { tint = "#4f8a3c" })
    graphics.draw_mesh(crate, vector.create(0, 0.5, 0), { yaw = 0.3, tint = "#b07a48" })
    graphics.draw_billboard(hero, vector.create(1.5, 0, 0.5))
    -- Anything else drawn ends the view, and goes over it.
    graphics.draw_text(font, "Crates: 1", 4, 4, "#ffffff")
end
```

### `graphics.begin_3d`

```lua
graphics.begin_3d(view: View3d)
```

Begins a 3D view over what's been drawn so far. The meshes and billboards drawn after it go in it,
until anything else is drawn, which ends it and goes over it. Each view has its own depth: things
in one hide each other by how near they are, but a later view draws over an earlier one.

`view` is a table:

| Field | What it is |
|---|---|
| `camera` | The camera the view looks through: a table, below. |
| `rect` | Where on the canvas the view shows, a [`Rect`](README.md#rect). By default, the whole canvas. With a [camera](#graphicsset_camera) set, it's in world positions, as everything drawn is. |
| `lighting` | How the view is lit: a table, below. By default, the default sun and ambient light. |

The camera's table:

| Field | What it is |
|---|---|
| `position` | Where the camera is, a vector. |
| `target` | The point it looks at, a vector, which shows in the middle of the view. |
| `up` | Which way is up the view, as near as it can be: a vector, `+y` by default. Looking straight down, up the view is `-z`, and looking straight up, `+z`. |
| `fov` | With perspective, where farther things look smaller: the angle the view takes in from its bottom edge to its top, in radians. By default, 0.8, about 46 degrees. |
| `height` | Without perspective, where things look the same size however far away they are, as in isometric games: how many world units the view shows from its bottom edge to its top. A camera has a `fov` or a `height`, not both. |
| `near`, `far` | How near to the camera and how far from it things show, by default 0.1 and 1000 world units. Depth is told apart more finely the closer they are to each other. |

The lighting's table:

| Field | What it is |
|---|---|
| `direction` | The way the sun's light goes, a vector of any length. By default, `vector.create(-0.3, -1, -0.5)`: down from above, a little to the right and in front. |
| `color` | The sun's light, a [`Color`](README.md#color), on surfaces facing it, and less on those turned partly away. By default, `"#cccccc"`. |
| `ambient` | The light on every surface, even those facing away from the sun. By default, `"#808080"`. |

White light shows a surface as it is, and the sun and ambient light together light it no brighter
than that. For no shading at all, light everything with ambient light alone:
`{ color = "#000000", ambient = "#ffffff" }`.

### `graphics.draw_mesh`

```lua
graphics.draw_mesh(mesh: Mesh, position: vector, options: MeshOptions?)
```

Draws `mesh` in the 3D view begun last, with its origin at `position`. Drawing one when no view is
open, or after something else has ended it, is an error. `options` is a table that places and
colors it:

| Field | What it is |
|---|---|
| `yaw` | Radians to turn it about `y`, counter-clockwise seen from above. |
| `pitch` | Radians to turn it about `x`, counter-clockwise seen from `+x`. |
| `roll` | Radians to turn it about `z`, counter-clockwise seen from `+z`. |
| `scale` | How much to scale it: a number, along every axis, or a vector, along each. A negative scale mirrors it. |
| `sprite` | The sprite it shows. A mesh's corners show points of the sprite from 0 to 1 across it, so each side of a [cube](#graphicsnew_cube) shows the whole sprite. By default, none: the mesh is white. |
| `tint` | A color its colors are multiplied by. By default, white. |

The mesh is scaled, rolled, pitched and turned, in that order, about its origin, then moved to
`position`. Pixels of the sprite less than half opaque aren't drawn, so what's behind shows through
them, and the rest are drawn opaque: meshes don't blend.

```lua
-- A crate turned the way the player last walked, showing planks, and a ball stretched tall.
graphics.draw_mesh(crate, vector.create(2, 0.5, -1), { yaw = facing, sprite = planks })
graphics.draw_mesh(ball, vector.create(0, 1, 0), { scale = vector.create(1, 2, 1), tint = "#e05040" })
```

Each mesh is a draw call of its own.

### `graphics.draw_billboard`

```lua
graphics.draw_billboard(sprite: Sprite, position: vector, tint: Color?, width: number?, height: number?)
```

Draws `sprite` standing upright in the 3D view begun last, with the middle of its bottom edge at
`position`, turned to face the camera across the ground, as characters do in 2.5D games: it stays
upright however far down the camera looks. It's `height` world units tall, 1 by default, and
`width` wide, by default as wide as its shape makes it at that height. A negative `width` mirrors
it, as [`sprite:flipped_x`](#spriteflipped_x) does.

A billboard is lit as the ground is, whichever way it faces, so it matches what it stands on. It
hides what's behind it as meshes do, its pixels less than half opaque left out, and its colors are
multiplied by `tint`, white by default. Drawing one when no view is open is an error.

```lua
-- The player, a tile tall, facing the way they last walked.
local sprite = if player.facing == "left" then hero:flipped_x() else hero
graphics.draw_billboard(sprite, player.position)
```

### `graphics.project`

```lua
graphics.project(view: View3d, point: vector): (number?, number?)
```

Returns where `point` shows on the canvas through `view`, in the coordinates its rect is in, or
nothing if it's nearer than the camera's near plane, such as behind the camera, or beyond its far
plane. Points to the sides of the view are outside its rect, but still placed. Labels over things
in a 3D view are drawn where it puts them:

```lua
local x, y = graphics.project(view, player.position + vector.create(0, 1.2, 0))
if x and y then
    local width = font:measure(player.name)
    graphics.draw_text(font, player.name, x - width / 2, y - 10, "#ffffff")
end
```

Works in `draw`, and while the game loads and in `update`, where a view without a rect fills the
whole canvas.

### `graphics.ray`

```lua
graphics.ray(view: View3d, x: number, y: number): (vector, vector)
```

Returns the ray into `view` through `x`, `y` on the canvas: where it starts, on the camera's near
plane, and the way it goes, a vector of length 1. Everything that shows at that point lies along
it. To find the point on the ground, at height 0, under the mouse:

```lua
local mouse_x, mouse_y = input.mouse_position()
if mouse_x and mouse_y then
    local origin, direction = graphics.ray(view, mouse_x, mouse_y)
    if direction.y < 0 then
        pointed_at = origin + direction * (-origin.y / direction.y)
    end
end
```

Works when [`project`](#graphicsproject) does.

## Meshes

A mesh is triangles, uploaded to the GPU once and drawn as many times as a game likes, in
[3D views](#3d-views). Meshes are made while the game loads and in `update`.

### `graphics.new_cube`

```lua
graphics.new_cube(size: number): Mesh
```

Returns a cube `size` along each side, centered on its origin. Each side shows the whole sprite it's
drawn with, upright seen from outside, and the top shows it with its top toward `-z`.

### `graphics.new_cuboid`

```lua
graphics.new_cuboid(width: number, height: number, depth: number): Mesh
```

Returns a box `width` along `x`, `height` along `y` and `depth` along `z`, centered on its origin,
showing the sprite as a cube does.

### `graphics.new_plane`

```lua
graphics.new_plane(width: number, depth: number): Mesh
```

Returns a flat rectangle facing up, `width` along `x` and `depth` along `z`, centered on its origin,
showing the sprite once across it with its top toward `-z`. It shows only from above.

### `graphics.new_sphere`

```lua
graphics.new_sphere(radius: number, segments: number?, rings: number?): Mesh
```

Returns a sphere of `radius` about its origin, made of `segments` slices around it (16 by default,
at least 3) and `rings` from top to bottom (8 by default, at least 2). The sprite wraps once around
it, its left and right edges meeting at the back, toward `-z`.

### `graphics.new_mesh`

```lua
graphics.new_mesh(data: MeshData): Mesh
```

Returns a mesh of the triangles `data` describes, a table of `vertices` and `indices`. Each vertex
is a table:

| Field | What it is |
|---|---|
| `position` | Where the corner is, a vector. |
| `normal` | Which way the surface faces there, for lighting: a vector of any length. By default, up. |
| `u`, `v` | Where on the sprite the corner shows, from 0 at its left and top to 1 at its right and bottom. By default, 0. |
| `color` | A color the sprite's colors are multiplied by there, and between corners, by a blend of theirs. By default, white. |

`indices` lists three vertices for each triangle, counting from 1, as Luau's tables do,
counter-clockwise seen from the side the triangle faces. Triangles show only from that side.

```lua
-- A floor of 1x1 tiles, each showing the whole sprite, alternately shaded.
local vertices, indices = {}, {}
for row = 0, 7 do
    for column = 0, 7 do
        local color = if (row + column) % 2 == 0 then "#ffffff" else "#d8d8d8"
        local x, z = column - 4, row - 4
        local first = #vertices
        table.insert(vertices, { position = vector.create(x, 0, z + 1), u = 0, v = 1, color = color })
        table.insert(vertices, { position = vector.create(x + 1, 0, z + 1), u = 1, v = 1, color = color })
        table.insert(vertices, { position = vector.create(x + 1, 0, z), u = 1, v = 0, color = color })
        table.insert(vertices, { position = vector.create(x, 0, z), u = 0, v = 0, color = color })
        for _, corner in { 1, 2, 3, 1, 3, 4 } do
            table.insert(indices, first + corner)
        end
    end
end
local floor = graphics.new_mesh({ vertices = vertices, indices = indices })
```

An index that isn't one of the vertices, or a count of indices that isn't three for each triangle,
is an error.

### `Mesh`

#### `mesh:triangle_count`

```lua
mesh:triangle_count(): number
```

Returns how many triangles the mesh has.
