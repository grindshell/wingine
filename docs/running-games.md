# Running games

## Running a game

The `wingine` executable runs a game from its folder. From the engine's repository, `cargo run`
builds it and runs it, with the game's folder after `--`:

```bash
cargo run -- path/to/game
```

That's a debug build, which builds quickly but runs Luau and the engine far more slowly. For a
game that does a lot each frame, and always when measuring how fast a game runs, use a release
build:

```bash
cargo run --release -- path/to/game
```

To run games from anywhere, install the executable, which puts `wingine` on your `PATH`:

```bash
cargo install --path .
```

```bash
wingine path/to/game
```

To ship a game, build the executable with `cargo build --release`, which leaves it in
`target/release/` as `wingine.exe` on Windows and `wingine` elsewhere, and ship it with the game's
folder. Start it with the folder's path, such as from a shortcut or a small launch script. Build
the executable, and any native libraries, on each platform the game runs on.

The window closes, and the game ends, when the player closes it. Scripts can't close it
themselves.

## Errors and output

The engine writes to standard error, so what a game prints shows in the terminal it was started
from.

**`print`** logs its arguments, separated by tabs, as a line tagged `lua`:

```lua
print("score", 1200, nil)
```

```text
[INFO lua] score	1200	nil
```

Native libraries log with the tag `native`, and the engine logs its warnings and errors with tags
starting `wingine`, such as when a sound can't play because too many are playing at once.

**An error while the game loads**, at the top level of `main.luau` or in a module it requires,
closes the window, prints the error with the script's file and line, and ends the program with a
failing exit code:

```text
error: Luau: runtime error: path/to/game/main.luau:12: attempt to index nil with 'x'
stack traceback:
	[C]: in ?
	path/to/game/main.luau:12: in function <path/to/game/main.luau:1>
```

So does a problem with `conf.luau`, such as an unknown setting, and a native library that's
missing or fails to load.

**An error in `update` or `draw`** stops the game where it is and shows the error in the window,
in place of the game, with a traceback. The error is logged too. Close the window, fix the
script, and run the game again: the engine doesn't reload scripts while it runs.

**An error in a task** stops that task, and logs the error. The game and its other tasks carry
on.

To handle an error rather than stop, use Luau's `pcall`:

```lua
local ok, err = pcall(function()
    sprites = graphics.load_sprite_sheet("missing.png", 16, 16)
end)
if not ok then
    print("no sprites:", err)
end
```

## Checking a game without a window

A game can run without a window, following a script of simulated input and saving screenshots,
which is quicker than playing it by hand to check something, and repeatable. Write the script as
a text file:

```text
# check.txt
screenshot shots/start.png
hold Right 0.5s
tap Space
wait 1s
screenshot shots/after.png
```

Then run the game with the `WINGINE_SCRIPT` environment variable set to the script's path. In
bash:

```bash
WINGINE_SCRIPT=check.txt cargo run -- path/to/game
```

In PowerShell:

```powershell
$env:WINGINE_SCRIPT = "check.txt"; cargo run -- path/to/game; $env:WINGINE_SCRIPT = $null
```

Instead of opening a window, the engine runs the game in a virtual window the size of the game's
`width` and `height` settings, follows the script, and exits. Frames advance at exactly 60 a
second of game time, however long they take to compute, so a run behaves the same every time if
the game does. Audio is silent, but sounds still start and finish on time. Screenshots are saved
relative to the script's folder, and each is reported on standard output.

The script has one command per line. `#` starts a comment.

| Command | What it does |
|---|---|
| `wait 30` | Runs 30 frames. |
| `wait 0.5s` | Runs half a second of game time: 30 frames. |
| `press Right` | Holds a key down. Keys are named as in [Key names](api/input.md#key-names), in any case. |
| `release Right` | Lets a key go. |
| `tap Space` | Presses a key, runs one frame, and lets it go. |
| `hold W 1s` | Presses a key, runs for a second (or a number of frames, as `hold W 60`), and lets it go. |
| `mouse 120 64` | Moves the cursor to canvas pixel (120, 64). |
| `mouse-down Left` | Holds a mouse button down: `Left`, `Right`, `Middle`, `Back` or `Forward`. |
| `mouse-up Left` | Lets a mouse button go. |
| `click 120 64` | Moves the cursor, presses the left button, runs one frame, and lets it go. |
| `click Right` | Clicks another button where the cursor is. |
| `resize 640 360` | Resizes the virtual window, in physical pixels. |
| `screenshot shots/walk.png` | Saves the latest frame as a PNG. |
| `benchmark 10s` | Runs for 10 seconds (or a number of frames), rendering every frame, and reports how long they took. See below. |
| `benchmark 600 run.csv` | The same, also saving each frame's stats as CSV. |

Mouse positions are canvas pixels, but screenshots are the size of the window. With a fixed
resolution, divide a position in a screenshot by the scale, after taking away the black bars, to
find the canvas pixel to click.

For checks worth keeping, the engine's own tests drive games the same way from Rust, with
`wingine::headless::Simulation`, and check the game's state by evaluating Luau expressions:
[`tests/pong.rs`](../tests/pong.rs) is a small example.

## Measuring performance

Measure before optimizing: the time usually goes somewhere other than where it's expected.

**In the game**, `graphics.frame_stats()` returns how long the frame before took, and how much it
drew, which a game can show as it runs:

```lua
local font = graphics.default_font()

function draw()
    -- ...
    local stats = graphics.frame_stats()
    local text = string.format(
        "%.1f ms: update %.1f, draw %.1f, render %.1f\n%d quads, %d draw calls",
        stats.frame * 1000,
        stats.update * 1000,
        stats.draw * 1000,
        stats.render * 1000,
        stats.quads,
        stats.draw_calls
    )
    graphics.draw_text(font, text, 4, 4, "#ffffff")
end
```

With vsync on, `frame` includes waiting for the display, so a frame takes a 60th of a second on a
60 Hz display however little the game does. Set `vsync = false` in `conf.luau` to see what a
frame really costs. [`graphics.frame_stats`](api/graphics.md#graphicsframe_stats) says what each
number measures.

**Without a window**, the `benchmark` command renders every frame for as long as it's told, and
reports the mean, median, 99th percentile and longest of each time, in milliseconds. This is the
RPG's village, after `wait 1s` and `benchmark 600`:

```text
frames 61 to 660, rendered on NVIDIA GeForce RTX 3080 (Vulkan):
              mean   median      99%      max  (ms)
  frame      0.229    0.217    0.459    2.196
  update     0.015    0.015    0.023    0.028
  draw       0.029    0.028    0.046    0.075
  render     0.111    0.102    0.208    2.143
  quads: 503 on average, 512 at most
  draw calls: 4 on average, 7 at most
```

Each frame waits for the GPU to finish before the next starts, so `frame` includes the GPU's
work. With a CSV file, `benchmark` also saves every frame's stats, to plot or compare. Benchmark a
release build: a debug build says so in its report, since its times mean little.

Drawing is usually where a Luau game's time goes. Each call from Luau into the engine costs more
than recording the sprite it draws, so:

- Turn colors into numbers once, with `graphics.color`, rather than passing hex strings or tables
  each frame. Each string or table is read again every time it's passed.
- Draw many sprites from one sheet in a row: the engine batches consecutive sprites with the same
  texture into one draw call.
- For tens of thousands of sprites, draw them from a [native library](native-libraries.md), which
  can hand the engine all of them in one call.
