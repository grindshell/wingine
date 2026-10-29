# Wingine

Wingine is a desktop game engine written in Rust, built as a library crate (`wingine`). It is
primarily a **2D** engine. 3D is possible but not a focus — don't let 3D concerns drive the design
of 2D systems, and don't build 3D features unless asked.

## Current milestone

**Ship a tile-based, top-down RPG.** Prioritize work that moves the engine toward this goal. The
milestone implies, roughly:

- Tilemap rendering (multiple layers, tilesets) and batched sprite rendering
- Sprite animation
- A 2D camera that follows the player and clamps to map bounds
- Keyboard input mapped to actions
- Grid-based movement and tile collision
- Map loading and transitions between maps
- Text rendering for dialogue and menus
- Lua-driven gameplay: NPC behavior, dialogue, events/triggers
- Music and sound effects

General-purpose features beyond this (physics engines, networking, editors, 3D pipelines) are out
of scope until the milestone is done.

## Tech stack

| Concern   | Crate   | Notes                                    |
|-----------|---------|------------------------------------------|
| Rendering | `wgpu`  | All GPU work goes through wgpu           |
| Windowing | `winit` | Window creation, event loop, raw input   |
| Audio     | `kira`  | Music and sound effects                  |
| Scripting | `mlua`  | Luau, with the `luau-jit` feature        |

- Rust edition 2024.
- Target platforms: desktop only (Windows, macOS, Linux).
- `kira` and `mlua` are not yet in `Cargo.toml`; add them when audio and scripting work starts.
- `wgpu` and `winit` change their APIs substantially between releases. Check the version pinned in
  `Cargo.toml` and that version's docs on docs.rs rather than relying on memory of older APIs.
- Scripts are **Luau**, not standard Lua 5.x. Luau has its own syntax additions (type annotations,
  compound assignment, `continue`) and a sandboxed standard library.

## Layout

- `src/app.rs`: the public `run` entry point, the `Game` trait, and the winit event loop. `run`
  takes an init closure that builds the game once the GPU is ready. With `WINGINE_SCRIPT` set,
  `run` hands off to the script runner instead of opening a window.
- `src/context.rs`: `Context`, the engine services passed to games: input, the canvas size, and
  texture loading. Games get it in their init closure and in `Game::update`.
- `src/input.rs`: `Input` (keyboard and mouse state per frame), `Key` and `MouseButton`. The
  event loop feeds it winit events, maps the cursor to canvas pixels, and clears per-frame presses
  and releases after each `update`.
- `src/graphics/`: `Gpu` (adapter + device), `WindowSurface` (presents to the window),
  `Renderer` (wgpu pipelines), `Screen` (applies the `Resolution`), and the public drawing API:
  `Canvas` (what games draw to each frame), `Camera`, `Resolution`, `Texture`, `Sprite` and
  `SpriteSheet`. All quads go through one textured pipeline; solid fills use a 1x1 white texture,
  and consecutive quads with the same texture are batched into one draw call. A fixed resolution
  renders the canvas to an offscreen texture, then draws that texture scaled onto the output
  with the same pipeline. Shaders are WGSL files next to the code that loads them.
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
- `src/script.rs`: the scripted-run format and runner, built on `Simulation`.
- `src/math.rs`: geometry types such as `Rect`.
- `examples/`: small runnable programs that exercise engine features, plus their assets.
  `sprites` has tests (enabled with `test = true` in `Cargo.toml`) that drive it with
  `Simulation`.
- `tests/screenshots.rs`: screenshot tests, with reference images in `tests/screenshots/`.
- `tests/simulation.rs`: tests for `Simulation` and scripted runs.

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
- `Color` values are sRGB, as authored. The renderer converts them to linear space, and the
  surface is viewed through an sRGB format so blending happens in linear space. Textures are
  uploaded as sRGB for the same reason.
- Textures are sampled nearest-neighbor so pixel art stays crisp.
- Keys are physical (by position, named for US QWERTY), so WASD works on any layout. Games
  bind named actions (strings, so Lua scripts can use them too) to keys and query the actions.
- The mouse position is in canvas pixels (through any fixed-resolution scaling and bars).
  `Camera::canvas_to_world` converts it to world coordinates, using `Context::canvas_size`.
- wgpu and winit types stay internal (`pub(crate)`). Games use the engine's own API.

## Commands

```bash
cargo build
cargo test
cargo clippy --all-targets
cargo fmt
cargo run --example shapes
cargo run --example sprites
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
  ```

  Mouse positions are canvas pixels; screenshots are window-sized, so with a fixed resolution
  divide screenshot coordinates by the scale (and subtract the bars) to aim a click.
- **As tests:** drive the game with `Simulation` (`tap`, `hold`, `click`, `run_for`, `game()`)
  and assert on its state, as `examples/sprites.rs` does. Prefer this for anything worth
  keeping, so CI catches regressions.

Windowing itself (focus, minimizing, real resizes, DPI) still needs a real window.

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
