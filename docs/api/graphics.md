# `graphics`

Drawing, the camera, sprites, animations and text.

Everything drawn in a frame is drawn in order, later things on top of earlier ones, onto a canvas
that starts the frame cleared to black. Drawing functions work only in `draw`.

- [Drawing](#drawing): [`clear`](#graphicsclear), [`fill_rect`](#graphicsfill_rect),
  [`draw_sprite`](#graphicsdraw_sprite), [`draw_sprite_rotated`](#graphicsdraw_sprite_rotated),
  [`draw_text`](#graphicsdraw_text), [`draw_text_layout`](#graphicsdraw_text_layout),
  [`draw_tilemap`](#graphicsdraw_tilemap)
- [The camera](#the-camera): [`set_camera`](#graphicsset_camera)
- [The canvas and frames](#the-canvas-and-frames): [`size`](#graphicssize),
  [`color`](#graphicscolor), [`frame_stats`](#graphicsframe_stats)
- [Sprites](#sprites): [`load_sprite_sheet`](#graphicsload_sprite_sheet),
  [`SpriteSheet`](#spritesheet), [`Sprite`](#sprite)
- [Animation](#animation): [`new_animation`](#graphicsnew_animation), [`Animation`](#animation-1)
- [Text](#text): [`default_font`](#graphicsdefault_font), [`Font`](#font),
  [`TextLayout`](#textlayout)

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

## The camera

### `graphics.set_camera`

```lua
graphics.set_camera(x: number, y: number, bounds: Rect?)
graphics.set_camera(nil)
```

Centers the view on world position `x`, `y` for everything drawn after this call, so positions
are world positions rather than canvas pixels. With `bounds`, such as a map's
[`map:bounds()`](tilemap.md#mapbounds), the view stays inside them, so a camera following the
player stops at the map's edges. Where the view is larger than the bounds, it centers them.

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

The view moves in whole canvas pixels, so the world doesn't shimmer as the camera moves by
fractions of a pixel. Shaking the view is a matter of moving the camera by a few pixels for a few
frames. The camera costs nothing to set, so set it as often as needed.

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
| `draw_calls` | How many draw calls those took: one for each run of quads with the same texture. |

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
