//! What a WebAssembly module running a Wingine game needs beside the engine.
//!
//! A game's module is a `cdylib` crate built for `wasm32-unknown-unknown` in a workspace that
//! patches in mlua's master branch, as `web/Cargo.toml` does. It depends on this crate, embeds
//! the game's folder from its build script with [`embed_game`], and exports a `start` for the
//! page, which links in the native libraries the game lists, if any, and calls [`start`]:
//!
//! ```ignore
//! // build.rs, with wingine-web among the build dependencies too.
//! fn main() {
//!     wingine_web::embed_game("../game");
//! }
//!
//! // src/lib.rs
//! #![cfg(target_arch = "wasm32")]
//!
//! use wasm_bindgen::prelude::*;
//!
//! #[wasm_bindgen]
//! pub fn start(on_error: js_sys::Function) -> Result<(), JsValue> {
//!     wingine::runtime::link_library("counter", wingine_native::library::<Counter>());
//!     wingine_web::start(&wingine_web::game!(), on_error)
//! }
//! ```
//!
//! `web/player` is such a crate, for games without native libraries of their own.
//!
//! Luau is C++, compiled with WASI SDK against WASI libc, so the module imports WASI's functions,
//! which the page's `wasi.js` provides, and uses Wasm exceptions for Luau's errors. Linking this
//! crate makes WASI libc's allocator Rust's, so the two don't both manage the same memory, and
//! runs the C++ code's static constructors once, as the module is instantiated: the module's
//! start function is this crate's, so the crate building it has none of its own.

#[cfg(not(target_arch = "wasm32"))]
mod embed;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
pub use embed::embed_game;
#[cfg(target_arch = "wasm32")]
pub use web::start;

/// A game's folder embedded in a module by [`embed_game`], as [`game!`] gives it.
#[derive(Clone, Copy, Debug)]
pub struct Game {
    /// The folder's name, which the engine finds it by, as `/<name>`.
    pub name: &'static str,
    /// Every file in the folder, by its path in it with `/` between names.
    pub files: &'static [(&'static str, &'static [u8])],
}

/// The [`Game`] that [`embed_game`] embedded in the crate this is used in, from its build script.
#[macro_export]
macro_rules! game {
    () => {
        include!(concat!(env!("OUT_DIR"), "/wingine_game.rs"))
    };
}
