# Quick start

This page sets up the engine, runs a game that draws "Hello world", then makes the text move with
the arrow keys.

## Set up the engine

You need:

- A stable [Rust toolchain](https://rustup.rs).
- A C++ compiler, since the engine builds Luau from source: MSVC on Windows (the "Desktop
  development with C++" workload of Visual Studio or its Build Tools), `g++` or `clang` elsewhere.
- On Linux, the ALSA headers for audio: `libasound2-dev` on Debian and Ubuntu.

Clone the engine's repository and build it:

```bash
git clone https://github.com/grindshell/wingine.git
cd wingine
cargo build
```

The first build takes a while, about 30 seconds of it compiling Luau. Later builds are quick.

## Hello world

A game is a folder with a `main.luau` script in it. Make a folder called `hello`, anywhere, and
save this in it as `main.luau`:

```lua
function draw()
    graphics.draw_text(graphics.default_font(), "Hello world", 16, 16, "#ffffff")
end
```

Then run it from the engine's folder, giving the path to yours:

```bash
cargo run -- ../hello
```

A window opens with "Hello world" in white in its top-left corner. Close the window to stop the
game.

The engine calls the global function `draw` every frame, after clearing the canvas to black.
`graphics.draw_text` draws text in a font, here the engine's built-in pixel font, with its
top-left corner at `x`, `y` in pixels, in a color written as a hex string.

## Make it bigger

The built-in font is a pixel font, with capitals 7 pixels tall, which is small in a 1280x720
window. Rather than draw everything larger, give the game a fixed resolution: a canvas of a set
size that the engine scales up to fill the window by a whole number, so pixels stay square and
crisp.

Settings go in `conf.luau`, next to `main.luau`. Save this in `hello/conf.luau`:

```lua
return {
    title = "Hello",
    resolution = { width = 320, height = 180 },
}
```

Now the canvas is 320 by 180 pixels, shown four times as large in the 1280x720 window, and the
window's title is "Hello". [Project structure](project-structure.md#confluau) lists every
setting.

## Make it move

Replace `main.luau` with this:

```lua
local font = graphics.default_font()
local MESSAGE = "Hello world"
-- Canvas pixels a second.
local SPEED = 80

-- Start in the middle of the canvas.
local canvas_width, canvas_height = graphics.size()
local text_width, text_height = font:measure(MESSAGE)
local x = (canvas_width - text_width) / 2
local y = (canvas_height - text_height) / 2

input.bind("left", { "Left", "A" })
input.bind("right", { "Right", "D" })
input.bind("up", { "Up", "W" })
input.bind("down", { "Down", "S" })

function update(dt: number)
    if input.held("left") then
        x -= SPEED * dt
    end
    if input.held("right") then
        x += SPEED * dt
    end
    if input.held("up") then
        y -= SPEED * dt
    end
    if input.held("down") then
        y += SPEED * dt
    end
end

function draw()
    graphics.clear("#1d232b")
    graphics.draw_text(font, MESSAGE, x, y, "#f2c14e")
end
```

Run it again, and move the text with the arrow keys or WASD.

The script has three parts, as most games do:

- **The top level** runs once, when the game starts. It sets the game up: here, it measures the
  text to center it, and binds named actions to keys, so the rest of the game asks whether
  `"left"` is held rather than about particular keys.
- **`update(dt)`** runs every frame, with the seconds since the frame before. Moving by
  `SPEED * dt` moves at the same speed however fast frames come.
- **`draw()`** runs every frame, after `update`, and draws the game as it now is. Drawing
  functions only work here.

## Next

- [Project structure](project-structure.md) explains how a game's folder is laid out and how
  `main.luau` runs, frame by frame.
- [Running games](running-games.md) covers errors, printing, and checking a game without a
  window.
- The [API reference](api/README.md) lists everything scripts can do: sprites, animation,
  tilemaps, sound and more.
