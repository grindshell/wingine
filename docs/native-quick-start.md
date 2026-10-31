# Native library quick start

This page builds a native library called `stars`: a field of stars that drift down the screen,
moved and drawn in Rust. Along the way it covers a function scripts call, state the library keeps
for the game, a hook that runs after every update, and drawing many sprites in one call.
[Native libraries](native-libraries.md) explains how libraries work.

You need the engine set up as in the [quick start](quick-start.md). The steps below assume the
engine's repository is in a folder called `wingine`, with the library and the game beside it:

```text
wingine/        the engine's repository
stars/          the library
stars-game/     a game that loads it
```

## Create the library

A library is a Rust library crate, built as a dynamic library (a `cdylib`) that depends on the
engine's `wingine-native` crate. Next to `wingine`, run:

```bash
cargo new --lib stars
```

Then replace `stars/Cargo.toml` with this:

```toml
[package]
name = "stars"
version = "0.1.0"
edition = "2024"

[lib]
# A library other programs load: stars.dll, libstars.so or libstars.dylib.
crate-type = ["cdylib"]

[dependencies]
wingine-native = { path = "../wingine/crates/wingine-native" }
```

The library's file is named after the crate's library name, which is the package's name unless
`[lib]` sets a `name`.

## Say hello

Replace `stars/src/lib.rs` with a library that has one function:

```rust
use wingine_native::{Context, Library, Registry, Result};

/// The library's state for a game.
struct Stars;

impl Library for Stars {
    /// The name of the library's table in Luau.
    const NAME: &str = "stars";

    /// Creates the library's state for a game, before its `main.luau` runs.
    fn new(_ctx: &mut Context) -> Result<Self> {
        Ok(Stars)
    }

    /// Registers the library's functions, classes and hooks.
    fn register(registry: &mut Registry<Self>) {
        // stars.greet(name)
        registry.function("greet", |_stars, _call, name: String| {
            Ok(format!("Hello, {name}!"))
        });
    }
}

// Exports the library, so the engine can find it in the built file.
wingine_native::export!(Stars);
```

`Library` is the trait every library implements. Its type holds the library's state for one game,
which each function receives first, followed by the call (for reaching the engine) and the
arguments, converted from Luau. What the function returns converts back to Luau, and an error it
returns is a Luau error for the script that called it.

Build it for release:

```bash
cd stars
cargo build --release
```

The library is now in `stars/target/release/`, as `stars.dll` on Windows, `libstars.so` on Linux
or `libstars.dylib` on macOS.

## Load it into a game

Make the game's folder, `stars-game`, with a `native` folder inside it, and copy the library
there. On Windows:

```powershell
New-Item -ItemType Directory -Force stars-game/native
Copy-Item stars/target/release/stars.dll stars-game/native/
```

On Linux (use `libstars.dylib` on macOS):

```bash
mkdir -p stars-game/native
cp stars/target/release/libstars.so stars-game/native/
```

List the library in the game's `conf.luau`:

```lua
return {
    title = "Stars",
    resolution = { width = 320, height = 180 },
    libraries = { "stars" },
}
```

And call it from `main.luau`:

```lua
print(stars.greet("world"))
```

Run the game from the engine's folder:

```bash
cd wingine
cargo run --release -- ../stars-game
```

The terminal shows the greeting:

```text
[INFO lua] Hello, world!
```

## Make it do something

Now the stars. The library keeps a list of them for the game, adds them when a script asks, moves
them after every update with a hook, and draws them all with one call when a script asks.

Replace `stars/src/lib.rs`:

```rust
use wingine_native::{Color, Context, Library, Rect, Registry, Result, Sprite, SpriteDraw};

/// Canvas pixels a second the fastest stars fall.
const MAX_SPEED: f32 = 40.0;

/// Stars drifting down the canvas.
struct Stars {
    /// A white pixel, stretched and tinted to draw each star.
    pixel: Sprite,
    stars: Vec<Star>,
    /// For scattering stars, from a xorshift generator.
    seed: u32,
    /// How to draw each star, kept between frames so drawing doesn't allocate.
    draws: Vec<SpriteDraw>,
}

struct Star {
    x: f32,
    y: f32,
    /// Canvas pixels a second.
    speed: f32,
}

impl Stars {
    /// A number from 0 up to 1.
    fn random(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / (1 << 24) as f32
    }
}

impl Library for Stars {
    const NAME: &str = "stars";

    fn new(ctx: &mut Context) -> Result<Self> {
        Ok(Self {
            pixel: ctx.create_sprite(1, 1, &[255, 255, 255, 255])?,
            stars: Vec::new(),
            seed: 0x9e37_79b9,
            draws: Vec::new(),
        })
    }

    fn register(registry: &mut Registry<Self>) {
        registry
            // stars.greet(name)
            .function("greet", |_, _, name: String| Ok(format!("Hello, {name}!")))
            // stars.add(count): scatters `count` stars over the canvas.
            .function("add", |stars, call, count: u32| {
                let [width, height] = call.context()?.canvas_size();
                for _ in 0..count {
                    let star = Star {
                        x: stars.random() * width,
                        y: stars.random() * height,
                        speed: MAX_SPEED * (0.25 + stars.random() * 0.75),
                    };
                    stars.stars.push(star);
                }
                Ok(())
            })
            // stars.count()
            .function("count", |stars, _, ()| Ok(stars.stars.len()))
            // stars.draw(), in `draw`: the faster a star, the brighter.
            .function("draw", |stars, call, ()| {
                let canvas = call.canvas()?;
                stars.draws.clear();
                for star in &stars.stars {
                    let dest = Rect::new(star.x.floor(), star.y.floor(), 1.0, 1.0);
                    stars.draws.push(SpriteDraw {
                        tint: Color::rgba(1.0, 1.0, 1.0, star.speed / MAX_SPEED),
                        ..SpriteDraw::new(dest)
                    });
                }
                canvas.draw_sprites(&stars.pixel, &stars.draws);
                Ok(())
            })
            // After each update, the stars fall, wrapping around to the top.
            .after_update(|stars, ctx, dt| {
                let height = ctx.canvas_size()[1];
                for star in &mut stars.stars {
                    star.y = (star.y + star.speed * dt) % height;
                }
                Ok(())
            });
    }
}

wingine_native::export!(Stars);
```

A few things to notice:

- **State.** `new` creates the library's state when a game loads it, with the engine's
  `Context`. Here it creates the sprite the stars are drawn with, from one white pixel.
- **The call.** Functions ask the call for what the engine lent the script that called them:
  `call.context()` while the game loads and in `update`, for input and the canvas's size, and
  `call.canvas()` in `draw`. Asking for the other one is an error.
- **Hooks.** `after_update` runs after every one of the game's updates, with the same `dt`, so
  the stars move whether or not the game's script mentions them.
- **Drawing in one call.** `draw_sprites` draws every star with one call into the engine. A
  script drawing each star itself would call into the engine once a star, which costs more than
  the drawing does.

Build it again, and copy it to `stars-game/native/` again, as before. Then replace
`stars-game/main.luau`:

```lua
local font = graphics.default_font()

stars.add(300)

function update(dt: number)
    if input.key_pressed("Space") then
        stars.add(300)
    end
end

function draw()
    graphics.clear("#0b0e1a")
    stars.draw()
    graphics.draw_text(font, `{stars.count()} stars: Space for more`, 4, 4, "#f2c14e")
end
```

Run the game again. Stars fall down the window, and Space adds more. (`` `...` `` is a Luau
interpolated string, which formats the values between braces.)

## Next steps

**Classes.** A library can give scripts objects with methods, such as a separate field of stars
for each layer of a parallax background. Register a type that implements `Class`, return values
of it from functions, and add its methods:

```rust
use wingine_native::Class;

struct Layer {
    speed: f32,
}

impl Class for Layer {
    const NAME: &str = "Layer";
}

// In `register`:
registry.function("new_layer", |_, _, speed: f32| Ok(Layer { speed }));
registry
    .class::<Layer>()
    // layer:speed()
    .method("speed", |_stars, layer, _call, ()| Ok(layer.speed))
    // layer:set_speed(speed)
    .method("set_speed", |_stars, layer, _call, speed: f32| {
        layer.speed = speed;
        Ok(())
    });
```

**Luau.** Some functions are easier to write in Luau, such as ones that wait. `registry.luau` adds
a chunk that runs as the library loads, receiving the library's table as `...`:

```rust
registry.luau(
    "wait",
    "local stars = ...
    function stars.wait_for(count)
        wait_until(function() return stars.count() >= count end)
    end",
);
```

**Examples.** [`examples/native-particles`](../examples/native-particles/src/lib.rs) is a small
library with a class, a hook and a chunk of Luau, and
[`examples/native-danmaku`](../examples/native-danmaku/src/lib.rs) a large one that runs the
shmup's bullets, with arguments it converts itself.

**Reference.** [The `wingine-native` API](api/native.md) lists every function a library can
call, and how each type converts between Luau and Rust.
