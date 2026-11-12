//! Drawing: 2D, and meshes in 3D views drawn among it.

mod animation;
mod blend;
mod camera;
mod camera3d;
mod canvas;
mod color;
mod default_font;
mod font;
pub(crate) mod gpu;
mod mesh;
mod model;
#[cfg(feature = "models")]
pub(crate) mod model_file;
pub(crate) mod renderer;
pub(crate) mod screen;
mod sprite;
pub(crate) mod surface;
mod texture;
mod view3d;

pub use animation::Animation;
pub use blend::Blend;
pub use camera::Camera;
pub use camera3d::{Camera3d, Projection, Ray};
pub use canvas::Canvas;
pub use color::Color;
pub use font::{Align, Font, FontSettings, TextLayout};
pub use mesh::{Mesh, MeshData, MeshVertex};
pub use model::{Model, ModelPart};
pub use screen::{Resolution, Scaling};
pub use sprite::{Sprite, SpriteSheet};
pub use texture::Texture;
#[cfg(feature = "egui")]
pub(crate) use texture::WeakTexture;
pub use view3d::{Canvas3d, Fog, Light, Lighting, View3d};
