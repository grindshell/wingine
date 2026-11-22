# The web

The engine also builds for `wasm32-unknown-unknown`, and Luau games run in a browser, drawing with
WebGPU in a canvas on the page. This is experimental: it builds against mlua's unreleased master
branch, and CI neither builds nor tests it.

## Building a game

The `web` folder is a Cargo workspace of its own, which builds a game into a page:

```bash
web/build.sh
```

builds the RPG example into `web/dist`, and

```bash
python -m http.server 8737 --directory web/dist
```

serves it at <http://localhost:8737>. On Windows, run `build.sh` from Git Bash. It takes:

- `--game path/to/game`: another game's folder, such as `examples/pong`;
- `--dev`: faster to build, slower to play;
- `--title "Pong"`: the page's title until the game starts, when the window's title replaces it;
- `--out path`: where to put the page;
- `--page path`: another page to copy beside the module, in place of `web/page`; it imports
  `./pkg/game.js` and `./wasi.js`, as `web/page/index.html` does;
- `--manifest path/to/Cargo.toml` and `--package name`: a crate of another workspace that builds
  the module, as a game with native libraries of its own needs (see below).

It needs:

- [WASI SDK](https://github.com/WebAssembly/wasi-sdk/releases) 34 or later, in a `wasi-sdk-*`
  folder in `~/.local/wasi-sdk` or wherever `WASI_SDK_PATH` says, at a path without spaces;
- the target: `rustup target add wasm32-unknown-unknown`;
- wasm-bindgen's CLI, at the version of `wasm-bindgen` the build locks:
  `cargo install wasm-bindgen-cli --version 0.2.129`. `build.sh` checks.

A game's folder is embedded in the module, all but folders whose names start with `.`, such as
tests' `.test-output`.

## How it's built

Luau is C++, which Rust's `wasm32-unknown-unknown` target can't compile on its own. mlua's master
branch builds it with WASI SDK, against WASI libc and libc++, with Wasm exceptions for Luau's
errors: see mlua's
[Compiling for WebAssembly](https://github.com/mlua-rs/mlua/blob/master/docs/wasm32.md). The
compilers, flags and libraries to link are set through environment variables, as
`web/wasm-env.sh` sets them; source it to run Cargo by hand.

No release of mlua does this, and Cargo can't take mlua from two places in one workspace, so the
engine depends on mlua's release, and the `web` workspace, with a `Cargo.lock` of its own, patches
in its master branch, in the `[patch.crates-io]` of `web/Cargo.toml`. A patch counts only in the
root of the workspace being built, so building in the repository's root keeps the release, and
building in `web` takes master.

Luau's native code generation (mlua's `luau-jit`) is only built off the web.

The workspace has two crates:

- `wingine-web`: what a module needs beside the engine, which imports WASI's functions from the
  page's `wasi.js`. Its `start` hands the engine a game's files and runs it, logging to the
  browser's console; `embed_game`, for build scripts, embeds a game's folder, which its `game!`
  gives.
- `player`: the module of a game without native libraries of its own, the RPG unless
  `WINGINE_GAME` names another folder.

The browser needs WebGPU and Wasm exception handling with `exnref`.

## A game with native libraries

The web can't load native library files, so a game that lists libraries of its own needs a module
that links them in. That's a crate of its own, a `cdylib`, in a workspace whose root has the same
patch as `web/Cargo.toml`. It depends on `wingine-web` (also among its build dependencies),
`wingine`, `wingine-native`, `wasm-bindgen` and `js-sys` from the same checkout, and on its
libraries' crates,
built as `rlib`s too, and its `start` links each in before starting the game:

```toml
[lib]
crate-type = ["cdylib"]

[build-dependencies]
wingine-web = { path = "../../wingine/web/wingine-web" }

[target.'cfg(target_arch = "wasm32")'.dependencies]
counter = { path = "../native-counter" }
js-sys = "0.3.106"
wasm-bindgen = "=0.2.129"
wingine = { path = "../../wingine" }
wingine-native = { path = "../../wingine/crates/wingine-native" }
wingine-web = { path = "../../wingine/web/wingine-web" }
```

```rust
// build.rs
fn main() {
    wingine_web::embed_game("../game");
}
```

```rust
// src/lib.rs
#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn start(on_error: js_sys::Function) -> Result<(), JsValue> {
    wingine::runtime::link_library("counter", wingine_native::library::<counter::Counter>());
    wingine_web::start(&wingine_web::game!(), on_error)
}
```

Then `web/build.sh --manifest path/to/Cargo.toml --package name --out dist` builds its page. The
libraries' own dependencies are in the module too, so one that uses mlua must use the engine's,
patched the same way.

## What's different

- `wingine::run` and `runtime::run` add a canvas to the page, start the game in it once the
  browser has given it a GPU, and return at once. An error building the game is logged, as it
  can't be returned. The page's styles size the canvas, in place of the window's size.
- There's no file system. The page gives the engine the game's files with `wingine::files::add`
  before it starts, and scripts, scenes, images, models and sounds are read from those. Luau's
  `require` finds modules among them as it finds files off the web.
- Saves are kept in the browser's local storage, under `Wingine/<identity>/<name>.luau`.
- There are no folders to pick: [`files.pick_folder`](api/files.md#filespick_folder) is an
  error, and `files.folders` returns none.
- kira can't stream there, so music is decoded into memory when it starts, as a sound effect is.
- Native library files can't be loaded: games have only libraries linked into the program with
  `runtime::link_library`.
- Browsers start audio only after the player has interacted with the page, so a page starts the
  game from a click, or sounds stay silent.
- The browser doesn't act on keys pressed while the game's canvas has focus, so Ctrl and plus
  zoom the game's interface, as [`ui:scale`](api/gui.md#scale-and-appearance) says, not the page.
  Zooming from the browser's menu changes the display's scale, which the game follows, as it does
  a window moved to another screen.
