# Making games with Wingine

Wingine is a 2D game engine for desktops: Windows, macOS and Linux. You write a game as a folder
of [Luau](https://luau.org) scripts and assets, and the `wingine` executable runs it. The engine
draws sprites, tilemaps and text with the GPU, plays sound effects and music, reads the keyboard
and mouse, and runs your scripts once a frame or at a fixed rate.

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
3. [Running games](running-games.md): running a game, reading its errors and output, checking it
   without a window, and measuring how fast it runs.

When a game needs compiled code:

4. [Native libraries](native-libraries.md): what they are, how the engine loads them, and when
   they're worth it.
5. [Native library quick start](native-quick-start.md): build one, load it into a game, and call
   it from Luau.

And to look things up:

6. [API reference](api/README.md): every table, function and type scripts use, the built-ins
   such as `wait` and `spawn`, and the `wingine-native` crate for native libraries.

## Examples

The repository's examples are complete games, and the best place to see how the pieces fit:

| Example | What it shows |
|---|---|
| [`examples/pong`](../examples/pong) | A small game in one script and a module: input, sound, music, text, a camera that shakes |
| [`examples/rpg`](../examples/rpg) | A top-down RPG: tilemaps, grid movement, animation, dialogue, menus and turn-based battles |
| [`examples/shmup`](../examples/shmup) | A bullet hell at a fixed update rate, with an optional native library for its bullets |
| [`examples/native-particles`](../examples/native-particles/src/lib.rs) | A small native library: functions, a class and hooks |
| [`examples/native-danmaku`](../examples/native-danmaku/src/lib.rs) | The shmup's bullets in Rust, drawn with one call per kind of bullet |

Run any of them from the repository with `cargo run -- examples/pong`.
