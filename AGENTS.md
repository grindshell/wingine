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
  takes an init closure that builds the game once the GPU is ready.
- `src/context.rs`: `Context`, the engine services passed to games: input and texture loading.
  Games get it in their init closure and in `Game::update`.
- `src/input.rs`: `Input` (keyboard state per frame) and `Key`. The event loop feeds it winit
  key events and clears per-frame presses and releases after each `update`.
- `src/graphics/`: `Gpu` (adapter + device), `WindowSurface` (presents to the window),
  `Renderer` (wgpu pipelines), and the public drawing API: `Canvas` (what games draw to each frame), `Texture`, `Sprite` and `SpriteSheet`. All quads
  go through one textured pipeline; solid fills use a 1x1 white texture, and consecutive quads
  with the same texture are batched into one draw call. Shaders are WGSL files next to the code
  that loads them.
- `src/headless.rs`: `Headless`, which renders a `Canvas` to an in-memory image without a window.
- `src/math.rs`: geometry types such as `Rect`.
- `examples/`: small runnable programs that exercise engine features, plus their assets.
- `tests/screenshots.rs`: screenshot tests, with reference images in `tests/screenshots/`.

## Conventions

- Drawing coordinates are logical pixels with the origin at the top-left and `y` pointing down.
- `Color` values are sRGB, as authored. The renderer converts them to linear space, and the
  surface is viewed through an sRGB format so blending happens in linear space. Textures are
  uploaded as sRGB for the same reason.
- Textures are sampled nearest-neighbor so pixel art stays crisp.
- Keys are physical (by position, named for US QWERTY), so WASD works on any layout. Games
  bind named actions (strings, so Lua scripts can use them too) to keys and query the actions.
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
