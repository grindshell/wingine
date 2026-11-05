use std::{fmt, sync::Arc};

use super::{Mesh, Sprite};
use crate::math::{Affine3, Bounds3};

/// Meshes placed together, each showing a sprite or none, as a model file holds them: loaded with
/// [`Context::load_model`](crate::Context::load_model), or made of meshes with [`Model::new`], and
/// drawn with [`Canvas3d::draw_model`](super::Canvas3d::draw_model).
///
/// Cloning is cheap; clones share the same meshes and textures.
#[derive(Clone)]
pub struct Model {
    inner: Arc<ModelInner>,
}

struct ModelInner {
    parts: Vec<ModelPart>,
    bounds: Option<Bounds3>,
    triangles: u32,
}

/// A mesh of a [`Model`]: where it is in the model, and the sprite it shows, if any.
#[derive(Clone, Debug)]
pub struct ModelPart {
    pub mesh: Mesh,
    pub sprite: Option<Sprite>,
    /// Places the mesh in the model, before the model is placed where it's drawn.
    pub transform: Affine3,
}

impl Model {
    /// A model of `parts`, drawn in order.
    pub fn new(parts: Vec<ModelPart>) -> Self {
        let bounds = parts
            .iter()
            .filter_map(|part| Some(part.mesh.bounds()?.transformed(&part.transform)))
            .reduce(Bounds3::union);
        let triangles = parts.iter().map(|part| part.mesh.triangle_count()).sum();
        Self {
            inner: Arc::new(ModelInner {
                parts,
                bounds,
                triangles,
            }),
        }
    }

    /// Its parts, each drawn with a draw call of its own.
    pub fn parts(&self) -> &[ModelPart] {
        &self.inner.parts
    }

    /// How many triangles its parts have together.
    pub fn triangle_count(&self) -> u32 {
        self.inner.triangles
    }

    /// The box its parts take up, placed in it, or `None` if they have no vertices.
    pub fn bounds(&self) -> Option<Bounds3> {
        self.inner.bounds
    }
}

impl fmt::Debug for Model {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Model")
            .field("parts", &self.inner.parts.len())
            .field("triangles", &self.inner.triangles)
            .finish_non_exhaustive()
    }
}
