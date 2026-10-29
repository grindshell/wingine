//! Wingine: a 2D-first desktop game engine.

mod app;
mod context;
mod error;
pub mod graphics;
pub mod grid;
pub mod headless;
pub mod input;
pub mod math;
pub mod script;
pub mod tilemap;

pub use app::{Game, WindowConfig, run};
pub use context::Context;
pub use error::{Error, Result};
