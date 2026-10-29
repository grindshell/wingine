use std::{fmt, sync::Arc};

use super::Sprite;
use crate::math::Rect;

/// An image uploaded to the GPU, created with [`Context::load_texture`](crate::Context::load_texture).
///
/// Cloning is cheap; clones share the same GPU texture.
#[derive(Clone)]
pub struct Texture {
    inner: Arc<TextureInner>,
}

struct TextureInner {
    /// Binds the texture and its sampler for the quad shader. Keeps the GPU texture alive.
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

impl Texture {
    pub(crate) fn new(bind_group: wgpu::BindGroup, width: u32, height: u32) -> Self {
        Self {
            inner: Arc::new(TextureInner {
                bind_group,
                width,
                height,
            }),
        }
    }

    /// Width in texels.
    pub fn width(&self) -> u32 {
        self.inner.width
    }

    /// Height in texels.
    pub fn height(&self) -> u32 {
        self.inner.height
    }

    /// A sprite covering the whole texture.
    pub fn sprite(&self) -> Sprite {
        let region = Rect::new(0.0, 0.0, self.width() as f32, self.height() as f32);
        Sprite::new(self.clone(), region)
    }

    pub(crate) fn bind_group(&self) -> &wgpu::BindGroup {
        &self.inner.bind_group
    }

    /// Whether both handles refer to the same GPU texture.
    pub(crate) fn same_as(&self, other: &Texture) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl fmt::Debug for Texture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Texture")
            .field("width", &self.width())
            .field("height", &self.height())
            .finish_non_exhaustive()
    }
}
