//! Wingine: a 2D-first desktop game engine.

mod app;
pub mod audio;
mod context;
mod error;
pub mod files;
mod gamepads;
pub mod graphics;
pub mod grid;
#[cfg(feature = "egui")]
pub mod gui;
pub mod headless;
mod icon;
pub mod input;
pub mod lua;
pub mod math;
pub mod menu;
#[cfg(feature = "physics")]
pub mod physics;
pub mod runtime;
pub mod save;
pub mod scene;
pub mod script;
pub mod tilemap;
#[cfg(not(target_arch = "wasm32"))]
pub mod watch;

pub use app::{Fullscreen, Game, Redraw, Waker, WindowConfig, run};
pub use context::{Context, FrameStats};
pub use error::{BoxedError, Error, Result};
pub use icon::WindowIcon;
