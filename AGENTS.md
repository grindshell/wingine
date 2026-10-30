# Wingine

Wingine is a desktop game engine written in Rust, built as a library crate (`wingine`). It is
primarily a **2D** engine. 3D is possible but not a focus — don't let 3D concerns drive the design
of 2D systems, and don't build 3D features unless asked.

## Milestones

**Done: ship a tile-based, top-down RPG.** The RPG in `examples/rpg` is that game. The milestone
covered:

- Tilemap rendering (multiple layers, tilesets) and batched sprite rendering
- Sprite animation
- A 2D camera that follows the player and clamps to map bounds
- Keyboard input mapped to actions
- Grid-based movement and tile collision
- Map loading and transitions between maps
- Text rendering for dialogue and menus
- Lua-driven gameplay: NPC behavior, dialogue, events/triggers
- Music and sound effects

No next milestone has been set. Until one is, ask before starting general-purpose features such
as physics engines, networking, editors or 3D pipelines.

## Tech stack

| Concern   | Crate   | Notes                                    |
|-----------|---------|------------------------------------------|
| Rendering | `wgpu`  | All GPU work goes through wgpu           |
| Windowing | `winit` | Window creation, event loop, raw input   |
| Audio     | `kira`  | Music and sound effects                  |
| Scripting | `mlua`  | Luau, with the `luau-jit` feature        |
| Native libraries | `libloading` | Loads them, and never unloads them |

- Rust edition 2024.
- Target platforms: desktop only (Windows, macOS, Linux).
- `wgpu`, `winit`, `mlua` and `kira` change their APIs substantially between releases. Check the
  version pinned in `Cargo.toml` and that version's docs on docs.rs rather than relying on memory
  of older APIs.
- `kira` is built with only the WAV and OGG Vorbis decoders. Its output, `cpal`, links against
  ALSA on Linux, so building there needs `libasound2-dev` (CI installs it).
- Scripts are **Luau**, not standard Lua 5.x. Luau has its own syntax additions (type annotations,
  compound assignment, `continue`, generalized `for ... in t`) and a sandboxed standard library.
- `mlua` builds Luau from C++ source, so building needs a C++ compiler (MSVC on Windows, `g++` or
  `clang` elsewhere). The first build takes about 30 seconds longer because of it.

## Layout

- `src/app.rs`: the public `run` entry point, the `Game` trait, and the winit event loop. `run`
  takes an init closure that builds the game once the GPU is ready. With `WINGINE_SCRIPT` set,
  `run` hands off to the script runner instead of opening a window.
- `src/context.rs`: `Context`, the engine services passed to games: input, audio, the canvas
  size, the built-in font, and texture loading. Games get it in their init closure and in
  `Game::update`.
- `src/audio.rs`: `Audio` (kira's mixer, with separate sound-effect and music tracks) and
  `Sound`. Sound effects are decoded into memory when loaded; music streams from its file and
  loops, crossfading when it changes. Without a sound device, and always headless, audio plays
  to a silent mixer that the engine advances by game time each frame, so sounds still start and
  finish on schedule and tests can check them (`sounds_playing`, `music_playing`).
- `src/input.rs`: `Input` (keyboard and mouse state per frame), `Key` and `MouseButton`. The
  event loop feeds it winit events, maps the cursor to canvas pixels, and clears per-frame presses
  and releases after each `update`.
- `src/graphics/`: `Gpu` (adapter + device), `WindowSurface` (presents to the window),
  `Renderer` (wgpu pipelines), `Screen` (applies the `Resolution`), and the public drawing API:
  `Canvas` (what games draw to each frame), `Camera`, `Resolution`, `Texture`, `Sprite`,
  `SpriteSheet` and `Animation`. An animation doesn't keep time: games keep how long it has played
  and ask it for the frame to draw, so one animation can be shared. All quads go through one
  textured pipeline; solid fills use a 1x1 white texture,
  and consecutive quads with the same texture are batched into one draw call. A fixed resolution
  renders the canvas to an offscreen texture, then draws that texture scaled onto the output
  with the same pipeline. Shaders are WGSL files next to the code that loads them.
- `src/graphics/font.rs` and `default_font.rs`: `Font` (glyphs packed into one texture, with
  their metrics) and `TextLayout`. `Font::layout` wraps text between words to a width, and
  `Canvas::draw_text_layout` draws a range of its glyphs, which is how dialogue reveals text
  without words jumping lines. The built-in pixel font is authored in `default_font.rs` as rows of
  `#` and `.`, and built into a texture when the `Context` is created. Layout logic is separate
  from the texture (`Metrics`), so its unit tests don't need a GPU.
- `src/tilemap.rs`: `Tilemap`, a grid of cells in layers, built from a `Tileset` of `Tile`s
  (a sprite, a tint, and whether it's solid). Layers draw in order, culled to what the canvas
  shows, and `draw_layer` lets games draw characters between them. `is_solid` is the collision
  check: a cell is solid if a tile on any layer is, and everything outside the map is solid. Its
  unit tests create a texture, so like the screenshot tests they need a GPU adapter.
- `src/grid.rs`: `Direction` and `GridMover`, for characters that walk tile by tile. Each frame a
  game calls `update`, then `step` when the mover is idle; a step carries over time left from the
  one before, so continuous walking keeps an even pace.
- `src/headless.rs`: `Headless`, which renders a `Canvas` to an in-memory image without a window,
  and `Simulation`, which runs a whole `Game` without a window: fixed 60 fps frames, simulated
  keys and mouse, and screenshots on demand. All of them in a process share one GPU device, and
  only one thread at a time may hold any (a reentrant lease): lavapipe crashes with a device per
  thread and WARP renders wrongly with a shared one, so parallel GPU tests take turns.
- `src/lua.rs`: `Scripts`, a sandboxed Luau VM for gameplay scripts, and its task scheduler.
  Games register Rust functions (`set_function`), load script files, and start script functions
  as tasks (`spawn`): coroutines that `update` resumes once a frame with the frame's duration.
  The built-ins `wait`, `wait_until`, `spawn` and `print` are documented at the top of the file.
  Its unit tests don't need a GPU.
- `src/runtime/`: games written entirely in Luau, run from a folder holding `main.luau` and an
  optional `conf.luau` of settings: the window's, and the native libraries to load. `LuauGame`
  is the `Game` that runs one, and the engine's API for it is the global tables `graphics`,
  `input`, `audio`, `tilemap` and `grid`, one file each. The module docs are the reference for
  script authors; the engine types those tables hand out (sprites, animations, tilemaps, grid
  movers, text layouts) are userdata, documented with their methods in `src/lua.rs`.
- `src/runtime/native.rs`: native libraries, compiled Rust that adds a global table of functions
  and classes to a Luau game, and hooks before and after its `update` and `draw`. Each is
  registered once per process and never unloaded, and each game that loads one gets the
  library's state of its own. Games load them from their `native` folder; tests link them in
  with `runtime::link_library`. The engine's side of the ABI is the `HOST` table of functions.
- `crates/wingine-abi/`: the C ABI between the engine and native libraries: `#[repr(C)]` types,
  the library's descriptor and the engine's `Host` table, versioned by `ABI_MAJOR` and
  `ABI_MINOR`. Its crate docs give the rules both sides follow.
- `crates/wingine-native/`: the crate native libraries are written with, which wraps the ABI:
  the `Library` and `Class` traits, a `Registry` for functions, methods and hooks, argument and
  result conversions, and `export!`. Libraries depend on it alone, never on `wingine`.
- `src/main.rs`: the `wingine` executable, which runs a Luau game: `cargo run -- examples/pong`.
- `examples/rpg/`: the milestone's RPG, written entirely in Luau. Maps in `maps/` are rows of
  symbols, which `world.luau` turns into tiles with descriptions; examining any cell shows its
  description. `story.luau` holds conversations and events, which maps refer to by name, and
  `dialogue.luau` the dialogue box, whose `say` and `ask` wait for the player. `battle.luau` runs
  one-on-one, turn-based battles against the creatures in `foes.luau`, which maps place and
  which fight when the player stands beside them; its messages and menus go through the dialogue
  box, and tests make its dice predictable by replacing `battle.roll`. `items.luau` holds things
  to use and gear, whose bonuses `items.stats` adds to the player's; `shop.luau` buys and sells
  them, and `bag.luau` (the menu key) equips gear and uses items. `ui.luau` holds the panels'
  shared look. `story.reached(stage)` compares quest stages, which run in the order of
  `story.STAGES`. The game's state is the global `rpg`.
- `src/script.rs`: scripted runs, which drive a game with simulated input from a text file
  (unrelated to Luau scripts), built on `Simulation`.
- `src/math.rs`: geometry types such as `Rect`.
- `examples/`: small runnable programs that exercise engine features, plus their assets.
  `sprites` has tests (enabled with `test = true` in `Cargo.toml`) that drive it with
  `Simulation`, and its events and NPC are scripted in `sprites.luau`. `pong/` is a game written
  entirely in Luau, with its computer opponent in `opponent.luau`. The sounds and music in
  `examples/sounds/` were synthesized for `sprites`; `pong` and `rpg` have copies of the ones
  they use, and `rpg` a copy of the tileset. `rpg/strides.png` is another copy with its people
  and creatures mid-stride (raised a pixel on stretched legs), the frame characters show for the
  first half of each step. `native-particles/` is a native library, a `cdylib` built as
  `particles`, which `tests/native.rs` builds and loads.
- `README.md`: the project's overview, with a screenshot of each example in `docs/screenshots/`.
  They were taken headlessly at 640x360 (768x432 for `sprites`, so its 256x144 canvas scales
  without bars); retake them when an example changes how it looks.
- `tests/screenshots.rs`: screenshot tests, with reference images in `tests/screenshots/`.
- `tests/simulation.rs`: tests for `Simulation` and scripted runs.
- `tests/runtime.rs`: tests for the Luau runtime, with small games written to temporary folders.
- `tests/native.rs`: tests for native libraries, most of them linked into the test. One builds
  `native-particles` with a nested `cargo build` into a target folder of its own and loads it
  from a file.
- `tests/pong.rs`: tests for the `pong` example, which check its Luau state by evaluating
  expressions such as `pong.ball.vx`.
- `tests/rpg.rs`: tests for the `rpg` example, the same way, playing through the quest.

## Conventions

- Drawing coordinates are canvas pixels with the origin at the top-left and `y` pointing down.
  The canvas is the window's size in logical pixels (`Resolution::Window`), or a fixed size scaled
  up by whole numbers with black bars (`Resolution::Fixed`, the choice for pixel art).
- `Canvas::set_camera` switches drawing to world coordinates; the camera's translation is
  applied on the CPU as quads are recorded, so switching costs nothing. Draw the world with a
  camera, then call `set_camera(None)` and draw the HUD in canvas pixels.
- Tiles are addressed as `[column, row]` (`[i32; 2]`, so neighbors off the map's edge are
  representable), with the map's top-left corner at the world origin. A `GridMover` occupies its
  destination tile from the start of a step. Movement asks a `blocked` closure whether it may
  enter a tile, so games can combine `Tilemap::is_solid` with their own rules, such as NPCs.
- Text positions are the top-left of the first line, rounded to whole units so pixel fonts stay
  crisp. Measured widths run to the edge of the ink, not the trailing advance, so centering is
  exact.
- `Color` values are sRGB, as authored. The renderer converts them to linear space, and the
  surface is viewed through an sRGB format so blending happens in linear space. Textures are
  uploaded as sRGB for the same reason.
- Textures are sampled nearest-neighbor so pixel art stays crisp.
- Luau scripts reach game state only through functions the game registers. Those functions share
  state with the game through `Rc<RefCell<_>>`, so the game must not hold a borrow while scripts
  run (`Scripts::update`, `spawn` or `call`). A game function that waits, such as showing
  dialogue until it's closed, is a Rust function that starts the action plus a few lines of Luau
  that `wait_until` it's done.
- Script errors don't stop the game: a failing task stops, the rest carry on, and `update`
  returns the error to log.
- Volumes are linear (1 is as recorded, 0 is silent) and converted to kira's decibels inside
  `Audio`. Failing to play a sound effect (too many at once) logs a warning rather than
  returning an error; loading and music return errors.
- Keys are physical (by position, named for US QWERTY), so WASD works on any layout. Games
  bind named actions (strings, so Lua scripts can use them too) to keys and query the actions.
- The mouse position is in canvas pixels (through any fixed-resolution scaling and bars).
  `Camera::canvas_to_world` converts it to world coordinates, using `Context::canvas_size`.
- wgpu and winit types stay internal (`pub(crate)`). Games use the engine's own API. `mlua` is
  the exception: `wingine::lua` re-exports it, since registering functions and converting values
  is its API. Engine types scripts use implement `FromLua`/`IntoLua` or `UserData` in
  `src/lua.rs` (`Direction` is a string such as `"up"`, `Color` a hex string such as
  `"#f2c14e"`, and `Font` userdata with methods).
- The Luau runtime lends its functions the `Context` while the game loads and updates, and the
  `Canvas` while it draws, through `Lua::scope` (`lend`, `with_context` and `with_canvas` in
  `src/runtime/mod.rs`). Called at any other time, they fail with a Lua error rather than
  reaching stale state. Each frame runs `update(dt)`, then resumes tasks, then `draw()`.
- Native libraries reach the engine only through the ABI's function pointers, and get the same
  lent services scripts do: a library function called from `draw` gets the canvas, not the
  context. Hooks run in the order libraries are listed, before and after `update` (and its
  tasks) and `draw`; hooks after `draw` draw in canvas pixels. Errors cross the ABI as strings,
  set with `set_error` before returning `Status::ERROR`, and the helper crate catches panics so
  they never unwind into the engine. Change the ABI only by adding to the end of `Host` and
  bumping `ABI_MINOR`; anything else bumps `ABI_MAJOR`.

## Commands

```bash
cargo build
cargo test
cargo clippy --all-targets
cargo fmt
cargo run --example shapes
cargo run --example sprites
cargo run -- examples/pong
cargo run -- examples/rpg
```

Run `cargo fmt` and `cargo clippy --all-targets` before considering a change finished, and keep
both clean. CI (`.github/workflows/ci.yml`) runs fmt, clippy with `-D warnings`, and `cargo test`
on Linux and Windows.

## Checking game behavior

Check how a game behaves headlessly instead of opening a window and driving it by hand:

- **Ad hoc:** write a script and run the game with `WINGINE_SCRIPT` pointing at it. The game
  runs without a window, follows the script's simulated input, and saves screenshots next to the
  script, then exits. The format is documented in `src/script.rs`:

  ```text
  screenshot shots/start.png
  hold Right 0.5s
  tap Up
  click 88 72
  wait 1s
  resize 1900 650
  screenshot shots/after.png
  ```

  ```bash
  WINGINE_SCRIPT=path/to/check.txt cargo run --example sprites
  WINGINE_SCRIPT=path/to/check.txt cargo run -- examples/pong
  ```

  Mouse positions are canvas pixels; screenshots are window-sized, so with a fixed resolution
  divide screenshot coordinates by the scale (and subtract the bars) to aim a click.
- **As tests:** drive the game with `Simulation` (`tap`, `hold`, `click`, `run_for`, `game()`)
  and assert on its state, as `examples/sprites.rs` does. For a Luau game, evaluate Luau
  expressions against its state, as `tests/pong.rs` does. Prefer this for anything worth
  keeping, so CI catches regressions.

Windowing itself (focus, minimizing, real resizes, DPI) still needs a real window. Audio in these
runs is silent; `cargo test -- --ignored` runs a check that plays (muted) through the real sound
device.

## Screenshot tests

Rendering changes are covered by screenshot tests: each renders a scene with `Headless` and
compares it with a reference PNG, allowing a per-channel difference of 2 so references pass on any
GPU. Add a test there when adding a drawing feature.

- A mismatch writes the rendered frame and a diff (mismatched pixels in magenta) to
  `target/tmp/screenshots/`. In CI they are uploaded as a build artifact.
- To create or update references, run with `UPDATE_SCREENSHOTS=1`. Only missing or mismatched
  references are rewritten. Look at every changed PNG before committing it.
- CI runners have no GPU: Linux uses lavapipe (`mesa-vulkan-drivers`) and Windows uses WARP. To
  reproduce locally, pick the adapter with wgpu's environment variables, e.g. on Windows:
  `WGPU_BACKEND=dx12 WGPU_ADAPTER_NAME="Microsoft Basic Render Driver" WGPU_DX12_COMPILER=fxc`.
  Set `WGPU_DX12_COMPILER=fxc` whenever testing on WARP: the Vulkan SDK's `dxcompiler.dll`
  crashes it if it's on `PATH`.
