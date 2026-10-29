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
- The repo is freshly scaffolded: these dependencies are not yet in `Cargo.toml`, and `src/lib.rs`
  still holds the `cargo new` placeholder.
- `wgpu` and `winit` change their APIs substantially between releases. Check the version pinned in
  `Cargo.toml` and that version's docs on docs.rs rather than relying on memory of older APIs.
- Scripts are **Luau**, not standard Lua 5.x. Luau has its own syntax additions (type annotations,
  compound assignment, `continue`) and a sandboxed standard library.

## Commands

```bash
cargo build
cargo test
cargo clippy --all-targets
cargo fmt
```

Run `cargo fmt` and `cargo clippy --all-targets` before considering a change finished, and keep
both clean.
