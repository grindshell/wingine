# Making games with Wingine

Wingine is a 2D game engine for desktops: Windows, macOS and Linux. You write a game as a folder
of [Luau](https://luau.org) scripts and assets, and the `wingine` executable runs it. The engine
draws sprites, tilemaps and text with the GPU, plays sound effects and music, reads the keyboard,
mouse and gamepads, keeps saves between runs, and runs your scripts once a frame or at a fixed rate.

Luau is a fast, sandboxed language derived from Lua 5.1, with type annotations, compound
assignment (`x += 1`), `continue` and a few other additions. If you know Lua, you know most of it;
[luau.org](https://luau.org) covers the differences.

When a script isn't fast enough, part of a game can be written in Rust as a native library: a
compiled library the engine loads at runtime from the game's folder, which adds functions and
objects for the scripts to use. The [shmup example](../examples/shmup) runs its bullets either
way, and fits about 3.4 times as many in a frame when they're in Rust.

The engine is a Rust crate too, and games can be written against it in Rust. These docs cover
games written in Luau; the crate's own docs (`cargo doc --open`) and
[`examples/sprites.rs`](../examples/sprites.rs) cover Rust.

## The docs

Read these in order the first time:

1. [Quick start](quick-start.md): a game that draws "Hello world", then one that moves.
2. [Project structure](project-structure.md): what goes in a game's folder, how `main.luau` runs,
   the settings in `conf.luau`, and where native libraries go.
3. [Running games](running-games.md): running a game, reloading it as you work, reading its errors
   and output, checking it without a window, testing it from Rust, and measuring how fast it runs.

When a game's levels, screens or characters are laid out rather than coded:

4. [Scenes](scenes.md): scene files, trees of sprites, rectangles, text and areas that a game
   loads, finds its way around, and draws, with screens anchored to the canvas, and characters
   rigged from parts and animated by clips.
5. [The editor](editor.md): laying scenes out with the mouse, the Inspector and a tree of nodes,
   and playing the game from it.

When a game needs compiled code:

6. [Native libraries](native-libraries.md): what they are, how the engine loads them, and when
   they're worth it.
7. [Native library quick start](native-quick-start.md): build one, load it into a game, and call
   it from Luau.

And to look things up:

8. [API reference](api/README.md): every table, function and type scripts use, the built-ins
   such as `wait` and `spawn`, and the `wingine-native` crate for native libraries.

## Examples

The repository's examples are complete games, and the best place to see how the pieces fit:

| Example | What it shows |
|---|---|
| [`examples/pong`](../examples/pong) | A small game in one script and a module: input, sound, music, text, a camera that shakes, and `exact` trig, so it plays the same everywhere |
| [`examples/rpg`](../examples/rpg) | A top-down RPG: tilemaps, grid movement, animation, dialogue, menus, turn-based battles, and a title screen that continues a saved game |
| [`examples/shmup`](../examples/shmup) | A bullet hell at a fixed update rate, with an optional native library for its bullets, and a HUD laid out in a scene, whose menu takes the mouse |
| [`examples/garden`](../examples/garden) | A garden laid out in a scene file: nodes found by path and tag, turned together, solid in a physics world the player walks in, and areas for notes, which F1 outlines |
| [`examples/orchard`](../examples/orchard) | A character rigged from sprites and animated by clips, layered and crossfaded each frame, catching apples |
| [`examples/course`](../examples/course) | A course in a 3D view of models: fixed bodies shaped by their triangles, crates that fall and tumble, and a character that walks up stairs and slopes, jumps and pushes crates |
| [`examples/native-particles`](../examples/native-particles/src/lib.rs) | A small native library: functions, a class and hooks |
| [`examples/native-danmaku`](../examples/native-danmaku/src/lib.rs) | The shmup's bullets in Rust, drawn with one call per kind of bullet |

Run any of them from the repository with `cargo run -- examples/pong`.
