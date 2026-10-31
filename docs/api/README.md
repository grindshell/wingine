# API reference

Scripts reach the engine through five global tables, a few built-in functions, and the objects
those hand out:

| Page | What it covers |
|---|---|
| [Built-ins](globals.md) | The game's `update` and `draw`, tasks (`spawn`, `wait`, `wait_until`), `print` and `require` |
| [`graphics`](graphics.md) | Drawing, the camera, sprite sheets, sprites, animations, fonts and text, frame stats |
| [`input`](input.md) | The keyboard and mouse, and actions bound to keys |
| [`audio`](audio.md) | Sound effects, music and volume |
| [`tilemap`](tilemap.md) | Tilesets and maps of tiles in layers |
| [`grid`](grid.md) | Characters that walk from tile to tile |
| [`wingine-native`](native.md) | The Rust crate native libraries are written with |

The engine's tables are read-only. Luau's standard library is there too, in its sandboxed form:
`math`, `string`, `table`, `coroutine`, `bit32`, `utf8`, `buffer`, `vector`, `debug`, and `os`
with `clock`, `date`, `difftime` and `time`, but nothing that reads or writes files.
[luau.org](https://luau.org/library) documents it.

## Conventions

- **Positions** are canvas pixels, from the top-left corner, with `y` growing downward. After
  `graphics.set_camera`, they're world positions instead, in the same units.
- **Angles** are radians. Positive angles turn clockwise, since `y` grows downward.
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
`sprite:size()`.

## When functions work

The engine lends scripts its services at the times they make sense. A function used at another
time raises an error saying when it works.

- **Drawing** works only in `draw`: `graphics.clear`, `graphics.fill_rect`, `graphics.draw_sprite`,
  `graphics.draw_sprite_rotated`, `graphics.draw_text`, `graphics.draw_text_layout`,
  `graphics.draw_tilemap` and `graphics.set_camera`.
- **The keyboard, mouse and sound** work while the game loads (the top level of `main.luau` and
  the modules it requires) and in `update` and the tasks it resumes: everything in `input`, and
  everything in `audio` but `audio.load`. So does `graphics.load_sprite_sheet`.
- **Everything else** works any time: `graphics.size`, `graphics.color`,
  `graphics.default_font`, `graphics.frame_stats`, `graphics.new_animation`, `audio.load`,
  `tilemap`, `grid`, and the methods of every object.

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
[`graphics.set_camera`](graphics.md#graphicsset_camera).

### `Direction`

One of the strings `"up"`, `"down"`, `"left"` and `"right"`, for [grid movement](grid.md).

### `Key`

A key's name, such as `"Space"`, `"W"` or `"Digit1"`, in any case. Keys are physical, named for
where they are on a US QWERTY keyboard. [Key names](input.md#key-names) lists them.

### `MouseButton`

One of `"left"`, `"right"`, `"middle"`, `"back"` and `"forward"`, in any case.

### `PlaySettings`

How [`audio.play`](audio.md#audioplay) plays a sound: a table with any of `volume`, `speed` and
`pan`. See [`audio.play`](audio.md#audioplay).

### `Tile`

A kind of tile, for [`tileset:add`](tilemap.md#tilesetadd): a table
`{ sprite = ..., tint = ..., solid = ... }`. See [`tileset:add`](tilemap.md#tilesetadd).
