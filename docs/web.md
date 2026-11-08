# The web

The engine also builds for `wasm32-unknown-unknown`, and Luau games run in a browser, drawing with
WebGPU in a canvas on the page. The RPG example runs this way in `wingine-wasm-test`, a repository
checked out beside this one, which embeds a game's folder in a module and serves it with a page;
its README has the steps to build and play it.

## Building

Luau is C++, which Rust's `wasm32-unknown-unknown` target can't compile on its own. mlua's master
branch builds it with [WASI SDK](https://github.com/WebAssembly/wasi-sdk) 34 or later, against
WASI libc and libc++, with Wasm exceptions for Luau's errors: see mlua's
[Compiling for WebAssembly](https://github.com/mlua-rs/mlua/blob/master/docs/wasm32.md). The
compilers, flags and libraries to link are set through environment variables, as
`wingine-wasm-test/scripts/wasm-env.ps1` sets them.

No release of mlua does this yet, and Cargo can't take a dependency from crates.io for one target
and from git for another, so the engine depends on mlua's release, and the workspace that builds
for the web patches in its master branch:

```toml
[patch.crates-io]
mlua = { git = "https://github.com/mlua-rs/mlua", rev = "a0ad4535fc72ba89967a931c5be57b162b5f0f3f" }
```

Once a release builds for the web, the engine can require it and the patch can go.

Luau's native code generation (mlua's `luau-jit`) is only built off the web.

A module built this way imports WASI's functions (`wasi_snapshot_preview1`), which the page
provides, and must:

- use WASI libc's allocator (`posix_memalign` and `free`) as Rust's global allocator, so the two
  don't both manage the same memory;
- call `__wasm_call_ctors` once, such as from a `#[wasm_bindgen(start)]` function. Otherwise the
  linker treats the module as a command and runs the C++ constructors again at the start of every
  exported function, and Luau loops forever through its list of flags.

The browser needs WebGPU and Wasm exception handling with `exnref`.

## What's different

- `wingine::run` and `runtime::run` add a canvas to the page, start the game in it once the
  browser has given it a GPU, and return at once. An error building the game is logged, as it
  can't be returned. The page's styles size the canvas, in place of the window's size.
- There's no file system. The page gives the engine the game's files with `wingine::files::add`
  before it starts, and scripts, scenes, images, models and sounds are read from those. Luau's
  `require` finds modules among them as it finds files off the web.
- Saves are kept in the browser's local storage, under `Wingine/<identity>/<name>.luau`.
- kira can't stream there, so music is decoded into memory when it starts, as a sound effect is.
- Native library files can't be loaded: games have only libraries linked into the program with
  `runtime::link_library`.
- Browsers start audio only after the player has interacted with the page, so a page starts the
  game from a click, or sounds stay silent.
