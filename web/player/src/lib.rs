//! A Wingine game in the browser, without native libraries of its own: the game's folder is
//! embedded in the module by `build.rs`, the RPG example unless `WINGINE_GAME` names another.
//! `web/build.sh` builds it into a page.

#![cfg(target_arch = "wasm32")]

use wasm_bindgen::prelude::*;

/// Starts the game in a canvas added to the page, which calls it once the player has clicked.
/// `on_error` is called with each error the engine or the game logs, and with a panic's message.
#[wasm_bindgen]
pub fn start(on_error: js_sys::Function) -> Result<(), JsValue> {
    wingine_web::start(&wingine_web::game!(), on_error)
}
