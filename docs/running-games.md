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

The window closes, and the game ends, when the player closes it, unless the game's
[`close_requested`](api/globals.md#close_requested) asks first, or when a script calls
[`window.quit`](api/window.md#windowquit).

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
the game does. Audio is silent, but sounds still start and finish on time. The game's
[saves](api/save.md) go in a temporary folder, which starts empty unless the script's `saves` fills
it, and goes when the run ends, so a run never changes the player's saves. Screenshots are saved relative to the script's
folder, and each is reported on standard output.

The script has one command per line. `#` starts a comment.

| Command | What it does |
|---|---|
| `saves deep career` | Starts the game with copies of the saves in a folder, relative to the script's. It must come first. See below. |
| `wait 30` | Runs 30 frames. |
| `wait 0.5s` | Runs half a second of game time: 30 frames. |
| `press Right` | Holds a key down. Keys are named as in [Key names](api/input.md#key-names), in any case. |
| `release Right` | Lets a key go. |
| `tap Space` | Presses a key, runs one frame, and lets it go. |
| `hold W 1s` | Presses a key, runs for a second (or a number of frames, as `hold W 60`), and lets it go. |
| `type Ada > ?` | Types the rest of the line, [as the game reads it](api/input.md#inputtext), without pressing any keys: anything but `#`, which starts a comment. |
| `mouse 120 64` | Moves the cursor to canvas pixel (120, 64). |
| `mouse-down Left` | Holds a mouse button down: `Left`, `Right`, `Middle`, `Back` or `Forward`. |
| `mouse-up Left` | Lets a mouse button go. |
| `click 120 64` | Moves the cursor, presses the left button, runs one frame, and lets it go. |
| `click Right` | Clicks another button where the cursor is. |
| `click-gui Go to town` | Clicks the first widget of the game's [interface](api/gui.md) labelled the rest of the line, such as a button's text. See below. |
| `pad-down 1 South` | Holds gamepad 1's button down. Buttons are named as in [Button names](api/input.md#button-names), in any case, and a stick's direction, such as `LeftStickLeft`, pushes the stick all the way. |
| `pad-up 1 South` | Lets a gamepad's button go. |
| `pad-tap 2 Start` | Presses gamepad 2's button, runs one frame, and lets it go. |
| `pad-hold 1 DPadLeft 1s` | Presses a gamepad's button, runs for a second (or a number of frames), and lets it go. |
| `stick 1 Left 0.5 -1` | Moves gamepad 1's left stick to `x`, `y`, each from -1 to 1, with `y` down: here, up and a little right. |
| `key-repeat 0.25s 20` | Repeats keys held down after a quarter of a second (or a number of frames), 20 times a second. |
| `key-repeat off` | Stops repeating keys held down. |
| `resize 640 360` | Resizes the virtual window, in physical pixels. |
| `screenshot shots/walk.png` | Saves the latest frame as a PNG. |
| `benchmark 10s` | Runs for 10 seconds (or a number of frames), rendering every frame, and reports how long they took. See below. |
| `benchmark 600 run.csv` | The same, also saving each frame's stats as CSV. |

Gamepads are numbered from 1, and connect when a command first names them, with any numbered
before them, since a scripted run has no real gamepads.

To check something deep in a game without playing up to it, start the run somewhere: either from
saves, or through a debug window of the game's own.

- `saves` starts the game with copies of the saves in a folder, such as one a check keeps beside
  it, or the folder a window keeps the player's in (see [`save`](api/save.md)). The run writes
  only to its copies, which go when it ends, so the folder is never changed. It must be the
  script's first command, since the game may read its saves as it starts.
- `click-gui` clicks a widget of the game's interface by its label, as the engine's tests do, so a
  debug window's buttons can be pressed without finding them in a screenshot. The interface must
  have shown the widget in the frame before, and a window shows from the frame after it opens, so
  wait a frame or two for one:

  ```text
  tap F1                 # show the debug window
  wait 2
  click-gui Shrine       # its button that jumps to the shrine
  tap F1
  ```

  A run stops with an error if no widget has the label. Labels are matched whole, and the first
  of several, row by row from the top, is clicked.

A key held down [repeats](api/input.md#inputkey_repeated) as a window's system would repeat it:
by default, after half a second, 30 times a second, until it's let go or another key is pressed.
So `hold L 1s` presses `L`, then repeats it 15 times. Keys type what they would on a US keyboard,
each time they're pressed and repeated, with Shift if it's held, so `hold L 1s` types `l` 16
times, and `press LeftShift` then `tap Period` types `>`. Other layouts' characters, and what input
methods compose, are for `type`.

Mouse positions are canvas pixels, but screenshots are the size of the window. With a fixed
resolution, divide a position in a screenshot by the scale, after taking away the black bars, to
find the canvas pixel to click.

## Testing a game from Rust

For checks worth keeping, write them as Rust tests, as the engine's own are, so `cargo test` runs
them with everything else. A test starts the game in a `wingine::headless::Simulation`, drives it
with simulated input, and reads the game's state with Luau:

```rust
use wingine::{
    headless::Simulation,
    runtime::{self, LuauGame},
};

const GAME: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/pong");

#[test]
fn serving_starts_a_rally() {
    let config = runtime::config(GAME).unwrap();
    let mut simulation =
        Simulation::new(&config.window, |ctx| LuauGame::load_with(ctx, GAME, &config)).unwrap();
    simulation.exec("pong.score.left = 4");
    simulation.run_for(1.0);
    simulation.tap_action("serve");
    assert_eq!(simulation.eval::<String>("pong.phase"), "rally");
    assert_eq!(simulation.eval::<u32>("pong.score.left"), 4);
}
```

- `tap`, `hold`, `key_down` and `key_up` press keys, as the script commands of the same names do,
  and `click`, `move_mouse`, `connect_gamepad` and `button_down` the rest.
- `tap_action`, `hold_action`, `action_down` and `action_up` press what an action is bound to:
  its first key, or if it has none, its first gamepad button. So a test presses `"serve"` and
  keeps working when the game binds it to another key.
- `eval::<T>(expression)` returns the value of a Luau expression, and `exec(code)` runs Luau, such
  as to set up a state. Each is a chunk of its own, run between frames, which the engine lends what
  it lends `update`, so engine functions work in it: it can load a level, bind an action or play a
  sound. Its globals are the game's, and it `require`s modules by their paths from the game's
  folder, as `main.luau` does, getting the same ones the game has: `require("./opponent")`. Both
  panic with the error if the Luau fails; `try_exec` and `try_eval` return it instead, for a test
  that expects one.

Keep the game's state in a global, such as Pong's `pong`, so tests can reach it.
[`tests/pong.rs`](../tests/pong.rs) is a small example, and [`tests/rpg.rs`](../tests/rpg.rs) a
larger one.

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

Or, without writing any of that, set a [`debug_key`](project-structure.md#debug_key) in
`conf.luau`: pressing it shows a window of the same numbers over the game, with a graph of recent
frames.

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
