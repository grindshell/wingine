# Project structure

A game is a folder. The engine runs its `main.luau`, reads its settings from `conf.luau`, loads
files from it by their paths relative to the folder, and loads native libraries from its `native`
folder:

```text
my-game/
├── main.luau          the game's script: the only file a game needs
├── conf.luau          settings: the window, the resolution, the update rate, native libraries
├── player.luau        modules that main.luau requires
├── enemies/
│   └── bat.luau
├── sprites.png        images, sounds and music, wherever the game likes
├── sounds/
│   ├── jump.wav
│   └── music.ogg
└── native/            native libraries, built for each platform the game runs on
    ├── particles.dll          Windows
    ├── libparticles.so        Linux
    └── libparticles.dylib     macOS
```

The folder's name is the window's title, unless `conf.luau` gives one.

## `main.luau`

`main.luau` is the game. The engine runs it in three parts:

1. **Its top level runs once**, when the window and GPU are ready, to set the game up: require
   modules, load sprites and sounds, bind keys to actions, build maps, start music, and start any
   tasks.
2. **`update(dt)` runs every frame** with `dt`, the seconds since the frame before, to move the
   game on. (With an [update rate](#update_rate), it runs at that rate instead.)
3. **`draw()` runs every frame**, after the frame's updates, to draw the game as it now is.

`update` and `draw` are global functions the script defines. Both are optional: a game that only
draws needs no `update`.

```lua
local player = require("./player")

local sheet = graphics.load_sprite_sheet("sprites.png", 16, 16)
local jump = audio.load("sounds/jump.wav")

input.bind("jump", { "Space", "W", "Up" })
audio.play_music("sounds/music.ogg")

function update(dt: number)
    if input.pressed("jump") and player.on_ground() then
        player.jump()
        audio.play(jump)
    end
    player.update(dt)
end

function draw()
    graphics.clear("#1d232b")
    player.draw(sheet)
end
```

### A frame

Each frame, the engine:

1. Runs the hooks of native libraries that go before `update`.
2. Calls `update(dt)`.
3. Resumes the tasks that are waiting (see [Tasks](#tasks)), each with the same `dt`.
4. Runs the hooks of native libraries that go after `update`.
5. Clears the canvas to black, with no camera set.
6. Runs the hooks that go before `draw`, calls `draw()`, then runs the hooks after `draw`.
7. Shows the frame.

With an update rate, steps 1 to 4 run as many times as the frame's time holds, possibly none,
before the frame draws.

A key pressed or released between frames counts as pressed or released in the next update, and
only that one: `input.pressed` is true for one update per press, however many frames pass. A key
tapped and let go between two updates counts as both pressed and released.

### What works when

The engine lends scripts its services at the times they make sense, and a function used at
another time raises an error saying so:

| | Top level | `update` and tasks | `draw` |
|---|:---:|:---:|:---:|
| Drawing: `graphics.clear`, `graphics.draw_*`, `graphics.fill_rect`, `graphics.set_camera` | | | ✓ |
| Loading sprite sheets: `graphics.load_sprite_sheet` | ✓ | ✓ | |
| The keyboard and mouse: `input` | ✓ | ✓ | |
| Playing sound and music: `audio` (apart from `audio.load`) | ✓ | ✓ | |
| `graphics.size` | ✓ | ✓ | ✓ |
| Everything else: `audio.load`, `tilemap`, `grid`, fonts, animations, `graphics.color`, `graphics.frame_stats` | ✓ | ✓ | ✓ |

So read input and change the game's state in `update`, and only draw in `draw`.

### Tasks

Some things take longer than a frame: a cutscene, an enemy's attack pattern, a door that opens,
waits, then closes. A task is a function that runs alongside the game and can wait, so these read
as a list of steps:

```lua
local function blink_warning()
    for _ = 1, 3 do
        warning_visible = true
        wait(0.25)
        warning_visible = false
        wait(0.25)
    end
end

spawn(blink_warning)
```

`spawn(f, ...)` starts `f(...)` as a task, which runs straight away until it first waits.
`wait(seconds)` pauses it for that long in game time, and `wait_until(condition)` until
`condition()` returns true. The engine resumes waiting tasks every update, after `update`. A task
that raises an error stops, logging the error, and the game and other tasks carry on.

`update` and `draw` themselves can't wait: they must return each frame. [Built-ins](api/globals.md)
documents the task functions.

### Modules

`require` loads another script by its path relative to the script that requires it, without the
`.luau`:

```lua
-- main.luau
local player = require("./player")
local bat = require("./enemies/bat")
```

```lua
-- player.luau
local player = { x = 0, y = 0 }

function player.update(dt: number)
    -- ...
end

return player
```

A module runs once, the first time it's required, and `require` returns what it returns. Modules
see the engine's tables and the game's globals, as `main.luau` does. `main.luau` can require
`conf.luau` too, to read the game's settings, such as its resolution.

### Globals and the sandbox

Scripts run in Luau's sandbox. The standard library (`math`, `string`, `table` and the rest) and
the engine's tables are read-only, so a script can't add to them or replace their functions.
Globals a script defines, such as `update`, are seen by every script loaded after it.

One consequence of the sandbox surprises people. Luau assumes that, in a sandbox, globals don't
change under a script, so when a script loads, it looks up each `global.field` and
`global.field.field` the script names, if that global exists by then, and the script sees those
values from then on, however the fields change:

```lua
-- main.luau
state = { score = 0 }
local hud = require("./hud") -- loads after `state` exists
```

```lua
-- hud.luau
local hud = {}

function hud.draw()
    -- Always "0": `state.score` was looked up when hud.luau loaded.
    graphics.draw_text(graphics.default_font(), tostring(state.score), 4, 4, "#ffffff")
end

return hud
```

A global that didn't exist yet when the script loaded is looked up each time, so `main.luau`
sees the changes to globals it creates itself, and so do modules it required before creating
them. So:

- Prefer locals and module tables to globals for the game's state: a module that returns a
  table, `local state = require("./state")`, shares it with every script that requires it.
- If the game keeps its state in a global table, create it in `main.luau` after requiring every
  module, as the examples' `pong`, `rpg` and `shmup` tables are.

### Errors

An error while the game loads, at the top level of `main.luau` or in a module it requires, closes
the window and prints the error. An error in `update` or `draw` stops the
game and shows the error in the window, with the script's name and line. An error in a task stops
that task only. [Running games](running-games.md#errors-and-output) has more.

## `conf.luau`

`conf.luau` is optional, and returns a table of settings. Every setting is optional, and a setting
the engine doesn't know is an error, so a misspelling doesn't go unnoticed:

```lua
return {
    title = "My Game",
    width = 1280,
    height = 720,
    resizable = true,
    resolution = { width = 320, height = 180 },
    vsync = true,
    update_rate = 60,
    libraries = { "particles", { name = "danmaku", optional = true } },
}
```

The engine reads it before the game starts, in a sandbox of its own: the engine's tables aren't
there, so `conf.luau` holds plain values.

### `title`

The window's title. By default, the name of the game's folder.

### `width` and `height`

The window's size when it opens, in logical pixels: on a display scaled to 150%, a window 1280
logical pixels wide is 1920 physical pixels wide. By default, 1280 by 720.

### `resizable`

Whether the player can resize the window. By default, `true`.

### `resolution`

The size of the canvas the game draws on, as `{ width = ..., height = ... }`.

Without it, the canvas is the window's size in logical pixels, and it changes size when the
window does: call `graphics.size()` for its size this frame, and lay the game out to fit.

With it, the canvas is always that size, and the engine scales it up to the window by the
largest whole number that fits, filling the space left over with black bars. A 320x180 canvas
fills a 1280x720 window four times over, and 1920x1080 six times over. This is the choice for
pixel art: pixels stay square and crisp, and every player sees the same area. A window smaller
than the canvas shows it scaled down to fit.

### `vsync`

Whether frames wait for the display to refresh before the next starts. By default, `true`, which
keeps frames in step with the display and saves power. Turned off, frames come as fast as the game
makes them, which shows how long a frame really takes: see
[Measuring performance](running-games.md#measuring-performance).

### `update_rate`

How many times a second the game updates, each time by the same `dt`. By default, there's no
rate: the game updates once a frame, by however long the frame took.

```lua
return { update_rate = 60 }
```

With a rate, the game plays the same at any frame rate: on a 144 Hz display, most frames run no
update, and if a frame takes long, the next runs two. Games that must play exactly the same every
time, such as those that replay inputs or count in frames, want a rate. So do games whose physics
behaves differently with different steps. Each update gets `dt = 1 / update_rate`.

A frame never updates for more than a quarter of a second, so a stall, such as dragging the
window, doesn't make the game leap forward.

### `libraries`

The native libraries the game loads, in order, from its `native` folder. Each entry is a
library's name, or a table with its `name` and whether it's `optional`:

```lua
return {
    libraries = {
        "particles",
        { name = "danmaku", optional = true },
    },
}
```

See the next section.

## Native libraries

A native library is compiled code, written in Rust, that the engine loads when the game starts. It
adds a global table of functions and objects for the game's scripts, and can run code before and
after each `update` and `draw`. [Native libraries](native-libraries.md) explains them, and the
[native library quick start](native-quick-start.md) builds one.

A game lists the libraries it loads in `conf.luau`, and keeps them in its `native` folder, each
named as its platform names libraries:

| Platform | `particles` is |
|---|---|
| Windows | `native/particles.dll` |
| Linux | `native/libparticles.so` |
| macOS | `native/libparticles.dylib` |

So a game that runs everywhere ships each library three times, built on each platform. Names may
hold letters, digits, `_` and `-`.

The engine loads the listed libraries in order, before `main.luau` runs, so their tables are
there from the script's first line:

```lua
-- main.luau
local sparks = particles.new_emitter("#f2c14e")
```

A library that's listed but missing is an error that stops the game, unless it's marked
`optional`. An optional library that's missing is skipped, and its table is `nil`, so the game
can check for it and do without:

```lua
-- main.luau: bullets in Rust if the library is there, in Luau if not.
local bullets = if danmaku then danmaku else require("./bullets")
```

A library that's there but fails to load, such as one built for another version of the engine,
is an error either way.

Unlike scripts, native libraries aren't sandboxed: a game that loads one runs it with the
player's full permissions. Only load libraries you trust.
