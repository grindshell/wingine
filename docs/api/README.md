# API reference

Scripts reach the engine through twelve global tables, a few built-in functions, the objects those
hand out, and the `Ui` a game's `gui` lays its interface out with:

| Page | What it covers |
|---|---|
| [Built-ins](globals.md) | The game's `update`, `gui`, `draw`, `close_requested` and `reloading`, tasks (`spawn`, `wait`, `wait_until`), `print`, `require` and `approach` |
| [`graphics`](graphics.md) | Drawing, the camera, blend modes, sprite sheets, sprites, animations, particles, fonts and text, frame stats, and 3D views of meshes, models and billboards, lit by a sun and by lights at places, which can cast shadows, with picking by boxes |
| [`input`](input.md) | The keyboard, mouse and gamepads, and actions bound to keys and buttons |
| [`audio`](audio.md) | Sound effects, music and volume |
| [`tilemap`](tilemap.md) | Tilesets and maps of tiles in layers |
| [`grid`](grid.md) | Characters that walk from tile to tile |
| [`physics`](physics.md) | Scene nodes and tile maps made solid, bodies that fall, roll and stack, and characters that walk among them, in 2D and in 3D views, up steps and slopes, pushing what they bump into; in both, rays, overlaps and sensors that tell where things are |
| [`scene`](scene.md) | Scenes loaded from scene files, and their nodes |
| [`ui`](ui.md) | Menus over a scene's nodes, which the keys and the mouse choose from |
| [`save`](save.md) | Data kept between runs of the game, such as progress |
| [`exact`](exact.md) | Math that gives the same bits on every platform, for games that replay |
| [`rect`](rect.md) | Whether rectangles overlap, and whether one holds a point |
| [`window`](window.md) | Quitting the game, asking before its window closes, covering the screen, scaling a fixed resolution, and what it kept as it reloaded |
| [`gui`](gui.md) | Interfaces over the game, such as debug tools, laid out with egui in the game's `gui` |
| [Tests](testing.md) | What only tests have: `test`, `sim` to drive the game and run its frames, `assert_equal` and `assert_near` |
| [`wingine-native`](native.md) | The Rust crate native libraries are written with |

The engine's tables are read-only. Luau's standard library is there too, in its sandboxed form:
`math`, `string`, `table`, `coroutine`, `bit32`, `utf8`, `buffer`, `vector`, `debug`, and `os`
with `clock`, `date`, `difftime` and `time`, but nothing that reads or writes files: games keep
data with [`save`](save.md).
[luau.org](https://luau.org/library) documents it.

## Conventions

- **Positions** are canvas pixels, from the top-left corner, with `y` growing downward. After
  `graphics.set_camera`, they're world positions instead, in the same units.
- **Angles** are radians. Positive angles turn clockwise, since `y` grows downward.
- **3D positions** are Luau vectors, `vector.create(x, y, z)`, in coordinates with `y` up, where
  positive angles turn counter-clockwise: see [3D views](graphics.md#3d-views).
- **Times** are seconds.
- **Cells count from 0**: sprites in a sheet, tiles in a map, a map's layers, and the IDs of tiles
  in a tileset. The one exception is an animation's frame numbers, which count from 1, like the
  table of frames it was made from.
- **Sizes and positions come back as two results**, not tables:
  `local width, height = graphics.size()`.
- **Optional arguments** are marked `?`, as Luau types are, and can be left out or `nil`.
- **Wrong arguments are errors**, naming the argument and what was expected, rather than being
  ignored.

Signatures are written as Luau types: `graphics.fill_rect(x: number, y: number, ...)`. Objects the
engine hands out, such as `Sprite` and `Tilemap`, are userdata with methods, called with `:`, as
`sprite:size()`. The same types, with short docs, are in the engine's definitions file,
[`src/runtime/wingine.d.luau`](../../src/runtime/wingine.d.luau), which luau-lsp checks scripts
against, in [the editor](../editor.md#checking-scripts-with-luau-lsp) or another.

## When functions work

The engine lends scripts its services at the times they make sense. A function used at another
time raises an error saying when it works.

- **Drawing** works only in `draw`: `graphics.clear`, `graphics.fill_rect`,
  `graphics.outline_rect`, `graphics.draw_line`, `graphics.fill_circle`,
  `graphics.outline_circle`, `graphics.draw_sprite`, `graphics.draw_sprite_rotated`,
  `graphics.draw_text`, `graphics.draw_text_layout`, `graphics.draw_tilemap`,
  `graphics.draw_scene`, `graphics.set_camera`, `graphics.set_blend`,
  `graphics.draw_particles`, `graphics.begin_3d`, `graphics.draw_mesh`, `graphics.draw_model` and
  `graphics.draw_billboard`.
- **The keyboard, mouse, gamepads and sound** work while the game loads (the top level of
  `main.luau` and the modules it requires), in `update` and the tasks it resumes, and in `gui`:
  everything in `input`, everything in `audio` but `audio.load`, and the methods of the
  [voices](audio.md#voice) it plays. So do
  `graphics.load_sprite_sheet`, `graphics.load_font`, `graphics.load_model`, the functions that
  make meshes
  (`graphics.new_cube`, `new_cuboid`, `new_plane`, `new_sphere` and `new_mesh`), `scene.load`,
  `menu:update` and the rest of `window` but `window.reloaded`, and all
  of them work in `close_requested` and `reloading` too. `graphics.project` and `graphics.ray` work then and in
  `draw`.
- **Interfaces** are laid out only in `gui`, with the `Ui` it's passed.
- **Everything else** works any time: `graphics.size`, `graphics.color`,
  `graphics.default_font`, `graphics.frame_stats`, `graphics.new_animation`,
  `graphics.new_particles`,
  `graphics.hit_box`, `audio.load`,
  `tilemap`, `grid`, `physics`, `save`, `ui.menu`, `window.reloaded`, and the methods of every
  object, scene nodes,
  physics worlds and menus included, but `menu:update` and voices'.

## Values

Some arguments are plain Luau values the engine reads in a particular way.

### `Color`

A color, in any of three forms:

- A hex string, `"#rrggbb"` or, with alpha, `"#rrggbbaa"`: `"#f2c14e"`, `"#00000080"`.
- A table of components from 0 to 1, `{ r = ..., g = ..., b = ..., a = ... }`, where `a` is 1 if
  left out: `{ r = 1, g = 0.5, b = 0 }`.
- A number `0xRRGGBBAA`, which always includes alpha: `0xf2c14eff`.

Colors are sRGB, as image editors and color pickers give them. Alpha is opacity: 1 (or `ff`) is
opaque, 0 invisible.

Numbers pass fastest. The engine reads a string or table again each time it's passed, which
costs more than drawing a sprite, so code that draws a lot turns its colors into numbers once,
with [`graphics.color`](graphics.md#graphicscolor):

```lua
local GOLD = graphics.color("#f2c14e") -- 0xf2c14eff
```

A number's alpha byte is always there, so `0xf2c14e` is not gold but `0x00f2c14e`: no red, and
almost transparent.

### `Rect`

A rectangle, as a table `{ x = ..., y = ..., width = ... , height = ... }`, where `x`, `y` is its
top-left corner. [`map:bounds()`](tilemap.md#mapbounds) returns one, for
[`graphics.set_camera`](graphics.md#graphicsset_camera), and [`rect`](rect.md) tests them.

### `Direction`

One of the strings `"up"`, `"down"`, `"left"` and `"right"`, for [grid movement](grid.md).

### `Key`

A key's name, such as `"Space"`, `"W"` or `"Digit1"`, in any case. Keys are physical, named for
where they are on a US QWERTY keyboard. [Key names](input.md#key-names) lists them.

### `MouseButton`

One of `"left"`, `"right"`, `"middle"`, `"back"` and `"forward"`, in any case.

### `GamepadButton`

A gamepad button's name, such as `"South"`, `"Start"` or `"DPadUp"`, or a stick's direction, such
as `"LeftStickLeft"`, in any case. Buttons are named for where they are on the gamepad.
[Button names](input.md#button-names) lists them.

### `Control`

What an action is bound to: a `Key`, a `GamepadButton`, or a mouse button named with `Mouse`
before it, such as `"MouseLeft"`, in any case.
[Mouse buttons as controls](input.md#mouse-buttons-as-controls) lists those.

### `Blend`

How what's drawn mixes with what's under it: one of `"normal"`, `"add"`, `"multiply"` and
`"screen"`. See [blend modes](graphics.md#blend-modes).

### `ParticleSettings`

What an emitter of particles sends out and how they move and look, for
[`graphics.new_particles`](graphics.md#graphicsnew_particles) and particles nodes: a table with any
of `emitting`, `rate`, `limit`, `warmup`, `seed`, `lifetime`, `speed`, `direction`, `spread`,
`gravity`, `drag`, `angle`, `spin`, `scales`, `colors`, `blend` and `attached`. See
[particle settings](graphics.md#particle-settings).

### `PlaySettings`

How [`audio.play`](audio.md#audioplay) plays a sound: a table with any of `volume`, `speed`,
`pan`, `looping` and `fade_in`. See [`audio.play`](audio.md#audioplay).

### `FontSettings`

How [`graphics.load_font`](graphics.md#graphicsload_font) draws a TrueType or OpenType font: a
table of `size`, how many pixels its em is, and optionally `smooth`. See
[`graphics.load_font`](graphics.md#graphicsload_font).

### `Tile`

A kind of tile, for [`tileset:add`](tilemap.md#tilesetadd): a table
`{ sprite = ..., tint = ..., solid = ... }`. See [`tileset:add`](tilemap.md#tilesetadd).

### `View3d`, `Camera3d`, `Lighting` and `Fog`

A 3D view, a table `{ camera = ..., rect = ..., lighting = ..., fog = ... }`, its camera a table
`{ position = ..., target = ..., ... }`, its lighting a table `{ direction = ..., color = ...,
ambient = ..., lights = ... }` and its fog, if it has any, a table `{ color = ..., near = ...,
far = ... }`, for [`graphics.begin_3d`](graphics.md#graphicsbegin_3d),
[`graphics.project`](graphics.md#graphicsproject) and [`graphics.ray`](graphics.md#graphicsray).
See [`graphics.begin_3d`](graphics.md#graphicsbegin_3d).

### `Light` and `LightSettings`

A light at a place in a 3D view, one of a `Lighting`'s `lights`: a table of a `position`, and
any of `color`, `range`, `energy`, `shadows`, and for a spot light, `cone`, `direction` and
`edge`. See [`graphics.begin_3d`](graphics.md#graphicsbegin_3d).

`LightSettings` is a scene's light node's: a table with any of `color`, `range`, `energy`, `cone`,
`edge` and `shadows`, a light's fields but its `position` and `direction`, which are its node's.
See [`node:light`](scene.md#nodelight) and [`node:set_light`](scene.md#nodeset_light).

### `MeshOptions`

How [`graphics.draw_mesh`](graphics.md#graphicsdraw_mesh) places and colors a mesh, and
[`graphics.draw_model`](graphics.md#graphicsdraw_model) a model: a table with any of `yaw`,
`pitch`, `roll`, `scale`, `sprite` (for a mesh), `tint`, `casts_shadow` and `glow`.

### `MeshData`

The triangles of a mesh, for [`graphics.new_mesh`](graphics.md#graphicsnew_mesh): a table
`{ vertices = ..., indices = ... }`.
