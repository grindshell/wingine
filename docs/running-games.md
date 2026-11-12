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

To ship a game to players, export it: see [Shipping a game](#shipping-a-game).

The window closes, and the game ends, when the player closes it, unless the game's
[`close_requested`](api/globals.md#close_requested) asks first, or when a script calls
[`window.quit`](api/window.md#windowquit).

## Reloading as you work

With `--watch` before the game's folder, the game reloads whenever a file in its folder changes,
so changes show as soon as they're saved:

```bash
cargo run -- --watch path/to/game
```

```bash
wingine --watch path/to/game
```

The window stays open, and the game starts again in it: `main.luau` runs from the top in a new
VM, reading every script, scene, image, sound and model again. The editor's Play runs games this
way too, so saving a scene in the editor shows it in the game playing. The engine looks at the
folder's files four times a second, but not at hidden ones, whose names start with `.`.

A reload starts the game afresh: sound effects and music stop, volumes go back to 1, actions are
unbound, and tasks end. Saves stay, since they're files outside the game's folder. `conf.luau` is
read again for its [`libraries`](project-structure.md#libraries), but the window's settings, its
size, resolution, update rate and the rest, change only when the game starts again, and the
window stays covering the screen, or not, and scaled as the game left it.

**Carrying on where it was.** Before reloading, the engine calls the game's
[`reloading`](api/globals.md#reloading), if it has one, and the reloaded game gets what it returned
from [`window.reloaded`](api/window.md#windowreloaded). Return plain data, as saves hold: a
boolean, number, string, or table of them. Pong keeps its match this way, so changing how the
computer plays changes it mid-rally:

```lua
pong = { score = { left = 0, right = 0 }, phase = "ready" --[[ and the rest ]] }

function reloading()
    return pong
end

local kept = window.reloaded()
if kept then
    for key, value in kept do
        pong[key] = value
    end
end
```

Copying what was kept over the fresh state keeps fields the new version adds. Sprites, sounds,
scene nodes and the like aren't plain data: keep what they're made from, such as which level the
player is on, and make them again.

**When it fails.** A game that stops with an error reloads too, so fixing the script brings it
back, and `reloading` is called even then, so it carries on with what it had. A game that fails to
load, as it starts or as it reloads, shows the error in the window, and loads again at the next
change, getting what `reloading` returned before it failed. Only an error reading `conf.luau` as
the game starts ends it, since the window needs its settings.

[Native libraries](native-libraries.md) aren't loaded again, since a library stays loaded until
the program ends, so rebuilding one needs the game to start again. Each reload gives them new
state, as a new game would.

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
script, and run the game again, or run it with `--watch` and save the fix: it
[reloads](#reloading-as-you-work).

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
| `mouse-by 40 -10` | Moves the mouse itself, 40 right and 10 away from the player, as [`input.mouse_motion`](api/input.md#inputmouse_motion) has it, for a game that has [captured the mouse](api/input.md#capturing-the-mouse). It does nothing while the game hasn't. |
| `mouse-down Left` | Holds a mouse button down: `Left`, `Right`, `Middle`, `Back` or `Forward`. |
| `mouse-up Left` | Lets a mouse button go. |
| `click 120 64` | Moves the cursor, presses the left button, runs one frame, and lets it go. |
| `click Right` | Clicks another button where the cursor is. |
| `scroll 0 -2` | Turns the mouse wheel where the cursor is: 2 notches toward the player, as to scroll down. The first number tilts it right, as [`input.wheel`](api/input.md#inputwheel) has it. |
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

## Testing a game in Luau

For checks worth keeping, write tests in Luau, in files named `*.test.luau` anywhere in the game's
folder, such as `pong.test.luau` beside `main.luau` or in a `tests` folder. Each declares its
tests with [`test`](api/testing.md#test), drives the game with [`sim`](api/testing.md#sim), and
checks its state with [`assert_equal`](api/testing.md#assert_equal), as Pong's do:

```lua
-- pong.test.luau
test("missing the ball scores for the other side", function()
    pong.ball = { x = 100, y = 20, vx = -200, vy = 0 }
    pong.phase = "rally"
    wait_until(function() return pong.phase ~= "rally" end)
    assert_equal(pong.score, { left = 0, right = 1 })

    -- The next serve goes to the player, who lost the point, a second later.
    wait(1.1)
    assert(pong.ball.vx < 0, "toward the player")
end)
```

`wingine test` runs them without a window, and says how each went:

```bash
cargo run -- test path/to/game
```

```text
running the tests of path/to/game
test pong.test.luau: the ball waits for a serve ... ok
test pong.test.luau: missing the ball scores for the other side ... FAILED

failures:

---- pong.test.luau: missing the ball scores for the other side ----
pong.test.luau:6: expected { left = 0, right = 1 }, got { left = 1, right = 0 }
stack traceback:
	pong.test.luau:6: in function <pong.test.luau:2>

test result: FAILED. 1 passed; 1 failed; 0 filtered out
```

It ends with a failing exit code if any test failed, so a CI job can run it. A word after the
game's folder runs only the tests whose files and names contain it:
`cargo run -- test path/to/game serve`.

**Each test starts afresh.** Every test runs against a new copy of the game, as it is once its
`main.luau` has run, before its first frame: its saves start empty and its audio is silent, and
nothing one test changes reaches another. Test files run in the order of their paths, and their
tests in the order they're declared.

**Tests read the game as it is.** A test file runs in the game's own VM, so it reads and changes
the game's globals directly, as they are at the time: `pong.ball.vx`, after any number of frames.
Keep the game's state in a global, such as Pong's `pong`, so tests can reach it. Tests call the
engine's functions too, such as `input.bind` or `scene.load`, and `require` the game's modules
by their paths from the test file, getting the same ones the game has.

**Time passes only when a test asks.** `sim.step` and `sim.run_for` run frames, at a fixed 60 a
second, as do `sim.tap` and the rest that press and let go, and `wait` and `wait_until`, which work
in tests as in tasks. A test that waits for what never happens fails at its time limit, 60 seconds
of game time unless it says otherwise.

**Failing.** A test fails when it raises an error, as a failed assertion does, with the line it
failed at; when the game stops with an error; or past its time limit. A game or test file that
fails to load fails once, for the file.

**Screenshots.** [`sim.matches_screenshot`](api/testing.md#simmatches_screenshot) compares the
latest frame with a PNG saved before, and returns `false` and what differs if it doesn't match:

```lua
test("the court before the first serve", function()
    sim.step()
    assert(sim.matches_screenshot("screenshots/ready.png"))
end)
```

The first run fails, as there's nothing to compare with yet. `--update` saves the frame as the
reference instead, and saves frames that don't match over theirs:

```bash
cargo run -- test --update path/to/game
```

Look at each one it saves before keeping it. A frame that doesn't match goes in the game's
`.test-output` folder, with a picture of where it differs in magenta; leave that folder out of
version control. Pixels may differ by 2 in each channel by default, so frames match on any GPU.

From Rust, `wingine::runtime::testing::run` runs a game's tests and returns how they went, so
`cargo test` can run them as Pong's are, in [`tests/pong.rs`](../tests/pong.rs).

## Testing a game from Rust

Tests can be written in Rust too, as the engine's own are, so `cargo test` runs them with
everything else. A test starts the game in a `wingine::headless::Simulation`, drives it with
simulated input, and reads the game's state with Luau:

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
  and `click`, `move_mouse`, `move_mouse_by`, `connect_gamepad` and `button_down` the rest.
- `tap_action`, `hold_action`, `action_down` and `action_up` press what an action is bound to:
  its first key, or if it has none, its first mouse button, or else its first gamepad button. So
  a test presses `"serve"` and keeps working when the game binds it to another key.
- `eval::<T>(expression)` returns the value of a Luau expression, and `exec(code)` runs Luau, such
  as to set up a state. Each is a chunk of its own, run between frames, which the engine lends what
  it lends `update`, so engine functions work in it: it can load a level, bind an action or play a
  sound. Its globals are the game's, and it `require`s modules by their paths from the game's
  folder, as `main.luau` does, getting the same ones the game has: `require("./opponent")`. Both
  panic with the error if the Luau fails; `try_exec` and `try_eval` return it instead, for a test
  that expects one.
- `screenshot()` renders the latest frame as an `Image`, whose `pixel(x, y)` reads a pixel and
  `save_png` writes it out.

Keep the game's state in a global, such as Pong's `pong`, so tests can reach it.
[`tests/pong.rs`](../tests/pong.rs) is a small example, and [`tests/rpg.rs`](../tests/rpg.rs) a
larger one.

### Comparing frames

A test can check how the game looks by comparing a frame with one saved before, as the engine's
own screenshot tests do. `Image::load_png` loads a PNG, and `compare` says how two images differ:
`Comparison::SameSize` with how many pixels differ by more than a tolerance in any channel, and the
most any channel differs by, or `Comparison::DifferentSizes`.

```rust
use wingine::headless::Image;

let frame = simulation.screenshot().unwrap();
let reference = Image::load_png("tests/screenshots/title.png").unwrap();
let comparison = frame.compare(&reference, 2);
if !comparison.differs_by_at_most(0) {
    frame.save_png("target/title.actual.png").unwrap();
    if let Some(diff) = frame.diff(&reference, 2) {
        diff.save_png("target/title.diff.png").unwrap();
    }
    panic!("the title screen has changed: {comparison:?}");
}
```

A tolerance of 0 compares exactly, which suits frames made on the same machine: a game that plays
the same way each run renders the same bytes each run. Other GPUs, and the software ones CI
machines use, round blending a little differently, so frames compared with a reference made
elsewhere want a small tolerance: the engine's own tests allow 2. `diff` shows where two images
differ: the frame dimmed, with the pixels that differ in magenta.

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

## Shipping a game

`wingine export` makes a folder to ship, which players open by double-clicking one file, for the
platform it runs on. Export with a release build, since a debug build runs games far slower:

```bash
cargo run --release -- export path/to/game dist/my-game
```

```text
dist/my-game/
├── my-game.exe     wingine, named by the game's identity (no .exe on Linux)
├── game/           the game's folder, without its tests (*.test.luau) or
│                   hidden files and folders (.luaurc, .test-output)
└── licenses/       wingine's licenses, and those of the crates it's built of
```

The executable, started without a game's folder, as a double-click starts it, runs the folder
called `game` beside it. Players see any error loading the game in its window rather than in a
console, which they don't have: one in `main.luau`, or in `conf.luau`, shown in a window with the
default settings. On Windows the exported executable opens no console window behind the game,
and prints nothing; the `wingine` you run in a terminal still does. And if the game has an
[`icon`](project-structure.md#icon), the exported executable shows it in Explorer and in
shortcuts to it, as its window does.

Zip the folder and ship it. It runs on the platform it was exported on, so export on each one the
game ships for, with any [native libraries](native-libraries.md) the game loads built there in
its `native` folder; `export` warns about a library the game needs that isn't. An export goes into
a new or empty folder, never over anything. A game whose identity is `game` or `licenses` can't be
exported, since its executable would have the name of a folder beside it; a link in the game's
folder back to a folder it's in isn't copied, and `export` warns about it.

`licenses/` holds wingine's licenses, which a game built on it ships, and
`THIRD-PARTY-NOTICES.txt`, those of the crates wingine is built of, copied from beside the
executable. The [release archives](https://github.com/grindshell/wingine/releases) have it there;
for a wingine built from
source, `export` warns that it isn't, and the repository's `.github/workflows/release.yml` shows
how `cargo about` makes it.

On macOS, an export is the same folder, not an app bundle.