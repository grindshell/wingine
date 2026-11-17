# Making games with Wingine

Wingine is a 2D game engine for desktops: Windows, macOS and Linux, and experimentally the web.
You write a game as a folder of [Luau](https://luau.org) scripts and assets, and the `wingine`
executable runs it. The engine draws sprites, tilemaps, text and particles with the GPU, with 3D
views among them, lays levels and screens out in scenes, makes them solid with physics, plays
sound effects, music and songs of its own instruments, reads the keyboard, mouse and gamepads,
shows interfaces, keeps saves between runs, and runs your scripts once a frame or at a fixed rate.
The [README](../README.md#features) lists its features.

Luau is a fast, sandboxed language derived from Lua 5.1, with type annotations, compound
assignment (`x += 1`), `continue` and a few other additions. If you know Lua, you know most of it;
[luau.org](https://luau.org) covers the differences.

When a script isn't fast enough, part of a game can be written in Rust as a native library: a
compiled library the engine loads at runtime from the game's folder, which adds functions and
objects for the scripts to use. The [shmup example](../examples/shmup) runs its bullets either
way, and fits [several times as many](../README.md#luau-and-rust-compared) in a frame when
they're in Rust.

The engine is a Rust crate too, and games can be written against it in Rust. These docs cover
games written in Luau; the crate's own docs (`cargo doc --open`) and
[`examples/sprites.rs`](../examples/sprites.rs) cover Rust.

## The docs

Read these in order the first time:

1. [Quick start](quick-start.md): a game that draws "Hello world", then one that moves.
2. [Project structure](project-structure.md): what goes in a game's folder, how `main.luau` runs,
   the settings in `conf.luau`, and where native libraries go.
3. [Running games](running-games.md): running a game, reloading it as you work, reading its errors
   and output, checking it without a window, testing it in Luau or Rust, measuring how fast it
   runs, and exporting it to ship.

When a game's levels, screens or characters are laid out rather than coded:

4. [Scenes](scenes.md): scene files, trees of sprites, rectangles, text and areas that a game
   loads, finds its way around, and draws, with screens anchored to the canvas, and characters
   rigged from parts and animated by clips.
5. [The editor](editor.md): laying scenes out with the mouse, the Inspector and a tree of nodes,
   and playing the game from it.

When a game's music is written rather than recorded:

6. [Songs](music.md): song files of the engine's own instruments, baked to WAV files or played
   live, where the game changes their tempo, moves between their sections, brings tracks in and
   out, and hears their beats.

When a game needs compiled code:

7. [Native libraries](native-libraries.md): what they are, how the engine loads them, and when
   they're worth it.
8. [Native library quick start](native-quick-start.md): build one, load it into a game, and call
   it from Luau.

To put a game on a web page, experimentally:

9. [The web](web.md): building a game into a page, and what's different in a browser.

And to look things up:

10. [API reference](api/README.md): every table, function and type scripts use, the built-ins
   such as `wait` and `spawn`, what tests have, and the `wingine-native` crate for native
   libraries.

## Examples

The repository's examples are complete games, and the best place to see how the pieces fit:

| Example | What it shows |
|---|---|
| [`examples/pong`](../examples/pong) | A small game in one script and a module: input, sound, music, text, a camera that shakes, and `exact` trig, so it plays the same everywhere, with tests in Luau, a screenshot among them |
| [`examples/rpg`](../examples/rpg) | A top-down RPG: tilemaps, grid movement, animation, dialogue, menus, turn-based battles, and a title screen that continues a saved game |
| [`examples/shmup`](../examples/shmup) | A bullet hell at a fixed update rate, with an optional native library for its bullets, and a HUD laid out in a scene, whose menu takes the mouse |
| [`examples/garden`](../examples/garden) | A garden laid out in a scene file: nodes found by path and tag, turned together, solid in a physics world the player walks in, and areas for notes, which F1 outlines |
| [`examples/orchard`](../examples/orchard) | A character rigged from sprites and animated by clips, layered and crossfaded each frame, catching apples |
| [`examples/cellar`](../examples/cellar) | A 2.5D cellar in a 3D view of models and billboards, lit by candles that glow and cast shadows, or keep to a box, with props named under the mouse |
| [`examples/course`](../examples/course) | A course in a 3D view of models: fixed bodies shaped by their triangles, crates that fall and tumble, a character that walks up stairs and slopes, jumps and pushes crates, and fog fading into a sky |
| [`examples/quarry`](../examples/quarry) | A level seen from the side in a 2D physics world: a tile map solid where its tiles are, crates that fall and stack, a boulder that rolls and a lift that carries what's on it |
| [`examples/bonfire`](../examples/bonfire) | A night of particles nodes and blend modes, with fireworks |
| [`examples/lettering`](../examples/lettering) | The built-in font, a bitmap font and a TrueType font, in a scene's text nodes |
| [`examples/bandstand`](../examples/bandstand) | A rhythm game played to a song played live, which the game changes as it plays and whose beats and markers it hears |
| [`examples/native-particles`](../examples/native-particles/src/lib.rs) | A small native library: functions, a class and hooks |
| [`examples/native-danmaku`](../examples/native-danmaku/src/lib.rs) | The shmup's bullets in Rust, drawn with one call per kind of bullet |

Run any of them from the repository with `cargo run -- examples/pong`. The
[README](../README.md#examples) shows each, with how to play it.
