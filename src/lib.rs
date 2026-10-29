//! Wingine: a 2D-first desktop game engine.

mod app;
mod error;
pub mod graphics;
pub mod math;

pub use app::{Game, WindowConfig, run};
pub use error::{Error, Result};
