# `wingine-native`

The Rust crate native libraries are written with. A library depends on it alone, not on the
`wingine` crate. [Native libraries](../native-libraries.md) explains how libraries work, and the
[quick start](../native-quick-start.md) builds one.

```toml
[lib]
crate-type = ["cdylib"]

[dependencies]
wingine-native = { path = "path/to/wingine/crates/wingine-native" }
```

Everything below is in the crate's root, `wingine_native`. For its full Rust docs, run
`cargo doc --open -p wingine-native` in the engine's repository.

- [Defining a library](#defining-a-library): [`Library`](#library), [`export!`](#export),
  [`Registry`](#registry), [`Class`](#class), [`ClassRegistry`](#classregistry)
- [Calls](#calls): [`Call`](#call), [converting arguments and results](#converting-arguments-and-results)
- [The engine's services](#the-engines-services): [`Context`](#context), [`Canvas`](#canvas),
  [`Sprite`](#sprite), [`SpriteDraw`](#spritedraw), [`Color`](#color), [`Rect`](#rect)
- [Errors and logging](#errors-and-logging): [`Error`](#error-and-result), [`log`](#log)
- [Linking a library in](#linking-a-library-in): [`library`](#library-1)

## Defining a library

### `Library`

```rust
pub trait Library: Sized + 'static {
    const NAME: &'static str;
    fn new(ctx: &mut Context) -> Result<Self>;
    fn register(registry: &mut Registry<Self>);
}
```

The library: a type holding its state for one game, and how it registers its functions, classes
and hooks.

- `NAME` is the name of the library's global table in Luau, which must be a Luau identifier, such
  as `"particles"`. It needn't match the library's file name, which is what games list in
  `conf.luau`, but keeping them the same saves confusion.
- `new` creates the library's state for a game when the game loads it, before its `main.luau`
  runs, with the engine's [`Context`](#context). An error stops the game from loading.
- `register` registers the library's functions, classes and hooks, once per process, the first
  time a game loads the library. It runs before any game's state exists, so it describes the
  library rather than doing anything for a game.

When the game ends, the engine drops its state.

### `export!`

```rust
wingine_native::export!(MyLibrary);
```

Exports a type implementing `Library` as the library the crate builds, so the engine finds it
when it loads the file. Use it once, at the crate's root.

### `Registry`

Passed to [`Library::register`](#library). Its methods return the registry, so calls chain:

```rust
registry
    .function("count", |particles, _, ()| Ok(particles.live.len()))
    .after_update(|particles, _, dt| {
        particles.advance(dt);
        Ok(())
    });
```

Functions and hooks receive the library's state for the game that's running, `&mut L`, first.
They must be `Send + Sync + 'static`: they're shared by every game in the process, so state they
change belongs in `L`, not in what they capture.

#### `registry.function`

```rust
fn function<A, R, F>(&mut self, name: &str, function: F) -> &mut Self
where
    A: FromArgs,
    R: IntoResults,
    F: Fn(&mut L, &mut Call, A) -> Result<R> + Send + Sync + 'static;
```

Adds `function` to the library's table as `name`. Scripts call it as `library.name(...)`, and it
receives the library's state, the [`Call`](#call), and the script's arguments
[converted](#converting-arguments-and-results) to `A`. What it returns converts back to Luau. An
error it returns is a Luau error for the script that called it.

```rust
// particles.burst_at(x, y, count?): `count` white particles, or 16.
registry.function("burst_at", |particles, _, (x, y, count): (f32, f32, Option<u32>)| {
    particles.burst([x, y], count.unwrap_or(16), Color::WHITE);
    Ok(())
});
```

#### `registry.class`

```rust
fn class<T: Class>(&mut self) -> ClassRegistry<'_, L, T>;
```

Adds the class `T`, whose objects scripts hold, and returns a [`ClassRegistry`](#classregistry) for
its methods. Adding a class twice is an error that fails the library's registration.

#### `registry.before_update` and `registry.after_update`

```rust
fn before_update<F>(&mut self, hook: F) -> &mut Self
where
    F: Fn(&mut L, &mut Context, f32) -> Result<()> + Send + Sync + 'static;

fn after_update<F>(&mut self, hook: F) -> &mut Self
where
    F: Fn(&mut L, &mut Context, f32) -> Result<()> + Send + Sync + 'static;
```

Runs `hook` before or after each of the game's updates, with the [`Context`](#context) and the
`dt` the game updates by, in seconds: the frame's time, or the step of the game's update rate.
Hooks after the update run after the tasks it resumes, too. An error stops the game.

#### `registry.before_draw` and `registry.after_draw`

```rust
fn before_draw<F>(&mut self, hook: F) -> &mut Self
where
    F: Fn(&mut L, &mut Canvas) -> Result<()> + Send + Sync + 'static;

fn after_draw<F>(&mut self, hook: F) -> &mut Self
where
    F: Fn(&mut L, &mut Canvas) -> Result<()> + Send + Sync + 'static;
```

Runs `hook` each frame before or after the game's `draw`, with the [`Canvas`](#canvas). Hooks
before `draw` draw under what the game draws; hooks after draw over it, in canvas pixels, whatever
camera the game set. An error stops the game.

Hooks of all kinds run in the order the game lists its libraries, and a library's hooks of one
kind in the order it registered them.

#### `registry.luau`

```rust
fn luau(&mut self, name: &str, source: &str) -> &mut Self;
```

Adds Luau `source` that runs as the library loads, after its functions are in its table, which the
chunk receives as `...`. It can add functions written in Luau, such as ones that wait, and
functions it calls get the [`Context`](#context), as they would while the game loads. `name` names
the chunk in errors.

```rust
registry.luau(
    "wait",
    "local particles = ...
    function particles.wait_until_gone()
        wait_until(function() return particles.count() == 0 end)
    end",
);
```

### `Class`

```rust
pub trait Class: Sized + 'static {
    const NAME: &'static str;
}
```

A type whose objects scripts hold, registered with [`registry.class`](#registryclass). `NAME` is
what scripts see in errors and from `tostring`.

A function or method returns a new object by returning a value of the type, which scripts own from
then on. The engine drops it once scripts no longer hold it, which may be after the game's library
state is gone, so an object shouldn't depend on that state outliving it.

```rust
struct Emitter {
    color: Color,
    bursts: u32,
}

impl Class for Emitter {
    const NAME: &str = "Emitter";
}

// In `register`: particles.new_emitter(color)
registry.function("new_emitter", |_, _, color: Color| Ok(Emitter { color, bursts: 0 }));
```

### `ClassRegistry`

Returned by [`registry.class`](#registryclass).

#### `class.method`

```rust
fn method<A, R, F>(&mut self, name: &str, method: F) -> &mut Self
where
    A: FromArgs,
    R: IntoResults,
    F: Fn(&mut L, &mut T, &mut Call, A) -> Result<R> + Send + Sync + 'static;
```

Adds `method` to the class as `name`. Scripts call it as `object:name(...)`, and it receives the
library's state, the object, the [`Call`](#call), and the script's arguments, not counting the
object. Calling it with `.` rather than `:` is an error that says so.

```rust
registry
    .class::<Emitter>()
    // emitter:burst(x, y, count)
    .method("burst", |particles, emitter, _, (x, y, count): (f32, f32, u32)| {
        particles.burst([x, y], count, emitter.color);
        emitter.bursts += 1;
        Ok(())
    })
    // emitter:bursts()
    .method("bursts", |_, emitter, _, ()| Ok(emitter.bursts));
```

The object is borrowed exclusively while its method runs.

## Calls

### `Call`

A call from a script to a function or method: its arguments, and the services the engine lent
for it.

#### `call.arg_count`

```rust
fn arg_count(&self) -> usize;
```

How many arguments the script passed, not counting the object a method is called on.

#### `call.arg`

```rust
fn arg<T: FromArg>(&self, index: usize) -> Result<T>;
```

The argument at `index`, counting from 0, [converted](#converting-arguments-and-results) to `T`.
Arguments past the last are `nil`. Most functions take their arguments as the closure's `A` instead;
`arg` is for reading them by hand, such as in a type implementing `FromArgs`.

#### `call.object` and `call.object_mut`

```rust
fn object<T: Class>(&self, index: usize) -> Result<&T>;
fn object_mut<T: Class>(&self, index: usize) -> Result<&mut T>;
```

The object of class `T` at argument `index`, borrowed until the call returns: shared with
`object`, and exclusive with `object_mut`. An argument of another class is an error. So is
borrowing an object exclusively while anything else has it borrowed, such as when it's the object
the method was called on, or an earlier argument.

```rust
// particles.merge(into, from): `from`'s bursts join `into`'s.
registry.function("merge", |_, call, ()| {
    let bursts = call.object::<Emitter>(1)?.bursts;
    call.object_mut::<Emitter>(0)?.bursts += bursts;
    Ok(())
});
```

#### `call.context`

```rust
fn context(&mut self) -> Result<&mut Context>;
```

The engine's [`Context`](#context), which the engine lends while the game loads and in `update`
and its tasks. Called from `draw`, it's an error.

#### `call.canvas`

```rust
fn canvas(&mut self) -> Result<&mut Canvas>;
```

The [`Canvas`](#canvas), which the engine lends in `draw`. Called at any other time, it's an
error.

### Converting arguments and results

A function's arguments convert to `A`: one value, a tuple of up to 8, or `()` for none. Arguments
beyond those are ignored. A conversion that fails is an error naming the argument as Luau counts
it, from 1.

| Rust | From Luau |
|---|---|
| `bool` | Any value: `nil` and `false` are false, everything else true. |
| `f64`, `f32` | A number. |
| `i8` to `i64`, `u8` to `u64`, `isize`, `usize` | A whole number in the type's range. A fraction or a number out of range is an error. |
| `String` | A string. |
| `Color` | A color in any form the engine takes: a hex string, a table, or a number `0xRRGGBBAA`. |
| `Rect` | A table `{ x, y, width, height }`. |
| `Sprite` | A sprite, such as from `sheet:sprite`. The library gets a copy of its own. |
| `Option<T>` | `nil` or a missing argument as `None`, anything else as `T`. |

Objects of the library's classes are read with [`call.object`](#callobject-and-callobject_mut).

A function's results convert from `R`: one value, a tuple of up to 8, which Luau receives as that
many results, or `()` for none.

| Rust | To Luau |
|---|---|
| `bool` | A boolean. |
| `f64`, `f32` | A number. |
| Integers | A number. Luau numbers are 64-bit floats, so integers beyond 2^53 lose precision. |
| `&str`, `String` | A string. |
| `Sprite` | A sprite scripts can draw. |
| `Option<T>` | `nil` for `None`. |
| A type implementing `Class` | A new object, which scripts own. |

For more than 8 arguments or results, or optional arguments with defaults, implement `FromArgs` or
`IntoResults` for a type of your own, reading each argument with [`call.arg`](#callarg) or pushing
each result with `IntoResult::push`:

```rust
use wingine_native::{Call, FromArgs, Result};

/// The arguments of `bullets.fire(x, y, angle, speed, count?)`.
struct Fire {
    x: f32,
    y: f32,
    angle: f32,
    speed: f32,
    count: u32,
}

impl FromArgs for Fire {
    fn from_args(call: &Call) -> Result<Self> {
        Ok(Self {
            x: call.arg(0)?,
            y: call.arg(1)?,
            angle: call.arg(2)?,
            speed: call.arg(3)?,
            count: call.arg::<Option<u32>>(4)?.unwrap_or(1),
        })
    }
}

// In `register`:
registry.function("fire", |bullets, _, fire: Fire| bullets.fire(fire));
```

## The engine's services

### `Context`

The engine's services, lent while a game loads and updates. Hooks around `update` receive it, and
functions get it from [`call.context`](#callcontext).

| Method | What it does |
|---|---|
| `fn canvas_size(&self) -> [f32; 2]` | The width and height of the canvas the frame will be drawn on, in canvas pixels. |
| `fn held(&self, action: &str) -> bool` | Whether any key bound to `action` is held. Games bind actions with `input.bind`. |
| `fn pressed(&self, action: &str) -> bool` | Whether any key bound to `action` was pressed for this update. |
| `fn released(&self, action: &str) -> bool` | Whether any key bound to `action` was released for this update. |
| `fn mouse_position(&self) -> Option<[f32; 2]>` | The cursor's position in canvas pixels, or `None` when it's outside the window. |
| `fn create_sprite(&mut self, width: u32, height: u32, rgba: &[u8]) -> Result<Sprite>` | Creates a sprite from `width` by `height` pixels: sRGB RGBA bytes, row by row from the top. Fails if `rgba` isn't `width * height * 4` bytes, or the size is empty or too large. |

### `Canvas`

What a game draws on, lent while it draws. Hooks around `draw` receive it, and functions get it
from [`call.canvas`](#callcanvas). Positions are in the coordinates of whatever camera the game
set, or canvas pixels without one.

| Method | What it does |
|---|---|
| `fn size(&self) -> [f32; 2]` | The canvas's width and height, in canvas pixels. |
| `fn fill_rect(&mut self, rect: Rect, color: Color)` | Fills `rect` with `color`. |
| `fn draw_sprite(&mut self, sprite: &Sprite, dest: Rect, tint: Color)` | Draws `sprite` stretched over `dest`, its colors multiplied by `tint`. |
| `fn draw_sprites(&mut self, sprite: &Sprite, draws: &[SpriteDraw])` | Draws `sprite` as each of `draws` says, in order. |

`draw_sprites` draws any number of sprites for the cost of one call into the engine, so it's how
to draw many. Keep a `Vec<SpriteDraw>` in the library's state, fill it each frame, and pass it,
so drawing doesn't allocate:

```rust
// bullets.draw(): every bullet, an 8x8 sprite centered on it and turned the way it's heading.
registry.function("draw", |bullets, call, ()| {
    let canvas = call.canvas()?;
    bullets.draws.clear();
    bullets.draws.extend(bullets.live.iter().map(|bullet| SpriteDraw {
        angle: bullet.angle,
        ..SpriteDraw::new(Rect::new(bullet.x - 4.0, bullet.y - 4.0, 8.0, 8.0))
    }));
    canvas.draw_sprites(&bullets.sprite, &bullets.draws);
    Ok(())
});
```

### `Sprite`

An image, or part of one, to draw: the same sprites scripts hold. A library gets them as
arguments from scripts, or creates them with [`Context::create_sprite`](#context).

| Method | What it does |
|---|---|
| `fn size(&self) -> [f32; 2]` | Its width and height in texels. |
| `clone()` | Another handle to the same image. Cheap: the image isn't copied. |

### `SpriteDraw`

```rust
pub struct SpriteDraw {
    pub dest: Rect,
    pub angle: f32,
    pub tint: Color,
}
```

How [`Canvas::draw_sprites`](#canvas) draws one sprite: stretched over `dest`, then turned by
`angle` radians about the center of `dest` (positive angles turn clockwise), its colors multiplied
by `tint`. `SpriteDraw::new(dest)` is a sprite stretched over `dest`, unturned and untinted.

### `Color`

```rust
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}
```

An RGBA color with sRGB components from 0 to 1. `Color::rgb(r, g, b)` and
`Color::rgba(r, g, b, a)` make one from components, `Color::rgb8` and `Color::rgba8` from bytes,
and `Color::WHITE`, `Color::BLACK` and `Color::TRANSPARENT` are constants.

### `Rect`

```rust
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}
```

A rectangle whose top-left corner is `x`, `y`, made with `Rect::new(x, y, w, h)`. In Luau, it's a
table `{ x, y, width, height }`.

## Errors and logging

### `Error` and `Result`

```rust
pub struct Error { /* ... */ }
pub type Result<T, E = Error> = std::result::Result<T, E>;
```

An error from a library, which scripts see as a Luau error with its message. Make one with
`Error::new(message)`. Any `std::error::Error` converts to one, so `?` works on other crates'
errors.

A panic in a function, method or hook is caught and becomes an error, `panicked: ...`, rather than
unwinding into the engine.

### `log`

```rust
pub fn log(level: Level, message: &str);
pub enum Level { Error, Warn, Info, Debug, Trace }
```

Logs `message` through the engine, with the tag `native`. The `wingine` executable shows `Info`
and more important messages:

```rust
wingine_native::log(wingine_native::Level::Info, "loaded 12 bullet kinds");
```

## Linking a library in

### `library`

```rust
pub fn library<L: Library>() -> abi::Library;
```

How `L` describes itself to the engine. A Rust program that runs Luau games, such as a test, can
link a library in rather than load it from a file, by passing this to
`wingine::runtime::link_library`:

```rust
wingine::runtime::link_library("particles", wingine_native::library::<Particles>());
```

Games that list `"particles"` then use the linked library in place of a file in their `native`
folder.

## Threads

A game's callbacks run on the thread running that game, and several games may run in one process,
on different threads, as the engine's tests do. A library's state for a game is only ever used on
that game's thread, but anything a library keeps outside it, such as in a `static`, must be
thread-safe.
