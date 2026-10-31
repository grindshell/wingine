# Native libraries

A native library is compiled code, written in Rust, that a game loads when it starts. It adds a
global table to the game's Luau, as the engine's `graphics` and `input` are, holding:

- **Functions** scripts call, such as `particles.count()`.
- **Classes** of objects scripts hold and call methods on, such as an emitter made by
  `particles.new_emitter("#f2c14e")`, with `emitter:burst(x, y, 24)`.
- **Hooks**: code the engine runs before and after each `update` and `draw`, such as moving every
  particle after each update.
- **Luau** that runs as the library loads, for functions better written in Luau, such as ones that
  `wait`.

The game's scripts can't tell a library's table from one written in Luau, so a game can keep the
same interface in both and choose at startup.

The [quick start](native-quick-start.md) builds one; this page explains how they work.

## When to write one

Luau is fast for a scripting language, and most games never need a native library. Write one
when measuring shows a script can't keep up, typically because it does the same work for
thousands of things every frame:

- **Many objects moving each frame**, such as bullets or particles. Rust moves a bullet in about
  a third of the time Luau does.
- **Drawing many sprites.** Each call from Luau into the engine costs more than the engine takes
  to record the sprite, so a script drawing 100,000 bullets spends most of its time calling. A
  library hands the engine every sprite of a kind in one call.
- **Heavy computation**, such as pathfinding over a large map, or procedural generation.

The [shmup example](../examples/shmup) has both: its bullets are in [`bullets.luau`](../examples/shmup/bullets.luau)
and in the [`danmaku`](../examples/native-danmaku/src/lib.rs) library, behind one interface, and
the library fits about 3.4 times as many bullets in a frame. The
[README](../README.md#luau-and-rust-compared) breaks down where the time goes.

What a library doesn't speed up is calling it. A call from Luau into a library costs more than
drawing a sprite, so a library pays off when each call does a lot of work: "move every bullet",
not "move this bullet".

## How a game loads one

A game lists the libraries it loads in its `conf.luau`, by name, and keeps them in its `native`
folder, named as the platform names libraries:

```lua
-- conf.luau
return {
    libraries = { "particles", { name = "danmaku", optional = true } },
}
```

```text
my-game/native/particles.dll        Windows
my-game/native/libparticles.so      Linux
my-game/native/libparticles.dylib   macOS
```

When the game starts, before `main.luau` runs, the engine loads each library in the order listed:

1. It opens the library's file, checks the library was built for this version of the engine, and
   has it register its functions, classes and hooks. The first game in a process to load a
   library does this; the library then stays loaded until the program ends, and is never
   unloaded.
2. It creates the library's state for this game, with the library's `new` function. Each game
   that loads a library gets state of its own, which every function and hook of the library
   receives.
3. It adds the library's table to the game's globals, under the name the library gives it, then
   runs the library's Luau, if it has any.

A library that's listed but not there stops the game with an error, unless it's `optional`: then
the game runs without it, and its table is `nil`. A library that's there but fails to load is an
error either way.

When the game ends, the engine frees each library's state for it, in the reverse of the order the
libraries loaded.

## A frame with libraries

Hooks run in the order the libraries are listed, around the game's own functions:

1. Hooks before `update`, with the update's `dt`.
2. The game's `update(dt)`, then its tasks.
3. Hooks after `update`.
4. Hooks before `draw`.
5. The game's `draw()`.
6. Hooks after `draw`.

With an update rate, steps 1 to 3 run for each update in the frame, so update hooks always see the
same `dt` the game does.

Hooks before `draw` draw first, under what the game draws, and in the coordinates of no camera.
Hooks after `draw` draw on top, in canvas pixels, whatever camera the game left set: they suit
overlays such as a performance graph.

## What a library can reach

A library reaches the engine only through the services the engine lends it, at the same times it
lends them to scripts:

- **The context**, while the game loads and in `update`: the canvas's size, whether actions are
  held, pressed or released, the mouse's position, and creating sprites from pixels.
- **The canvas**, in `draw`: its size, filling rectangles, and drawing sprites, one at a time or
  many in one call.

Update hooks receive the context, and draw hooks the canvas. A function or method asks for
whichever the script that called it could use: a function called from `draw` gets the canvas,
and asking for the context there is an error, as calling `input.held` from `draw` is.

A library sees input through the actions the game binds with `input.bind`, not keys directly, so
the game decides the controls. It draws sprites the game hands it, such as from a sprite sheet the
script loaded, or ones it creates from pixels:

```lua
-- After the shmup: the game loads the sprite, and the library draws it for every pellet. A
-- pellet hits within 2 pixels of its center, and isn't turned to the way it's heading.
local sheet = graphics.load_sprite_sheet("bullets.png", 8, 8)
local pellet = danmaku.kind(sheet:sprite(0, 0), 2, false)
```

The [API reference](api/native.md) lists everything a library can call, and how arguments and
results convert between Luau and Rust.

## Errors

- A function or method that returns an error, or panics, raises a Luau error in the script that
  called it, with the library's message. Scripts can catch it with `pcall`.
- A hook that returns an error, or panics, stops the game, as an error in the game's own `update`
  or `draw` does, and the window shows the error.
- An error creating the library's state stops the game from loading.

Panics never cross into the engine: the `wingine-native` crate catches them and turns them into
errors.

## Safety

Scripts run in a sandbox: they can't write files, run programs or reach the network. Native
libraries don't: a library is a program like any other,
and a game that loads one runs it with the player's full permissions. Only load libraries you
trust, and tell players when a game includes them.

## Versions and platforms

A library is built for one platform, so a game that runs on Windows, Linux and macOS ships each
library three times, built on each.

A library is also built for a version of the engine's interface to libraries, which `wingine-native`
records in it. The engine loads libraries built for the same major version and the same or an
earlier minor version, and refuses others with an error saying which version each is. Build
libraries with the `wingine-native` crate from the same checkout of the engine as the `wingine`
executable that runs them, and rebuild them when you update the engine.

Build libraries for release (`cargo build --release`): a debug build of a library is far slower,
which defeats the point of one.

## Linking a library into Rust

A Rust program that runs Luau games, such as a test, can link a library in rather than load it
from a file, with `wingine::runtime::link_library`. Games that list that name then use the linked
library in place of one in their `native` folder:

```rust
wingine::runtime::link_library("particles", wingine_native::library::<Particles>());
```

The engine's [`tests/native.rs`](../tests/native.rs) tests libraries this way.
