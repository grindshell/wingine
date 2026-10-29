//! Wingine: a 2D-first desktop game engine.

mod app;
mod context;
mod error;
pub mod graphics;
pub mod headless;
pub mod math;

pub use app::{Game, WindowConfig, run};
pub use context::Context;
pub use error::{Error, Result};
