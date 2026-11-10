#[cfg(feature = "egui")]
use std::sync::Weak;
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
    /// For showing the texture in egui: a view without sRGB decoding, since egui reads colors as
    /// they're stored.
    #[cfg_attr(not(feature = "egui"), expect(dead_code))]
    gui_view: wgpu::TextureView,
    width: u32,
    height: u32,
}

impl Texture {
    pub(crate) fn new(
        bind_group: wgpu::BindGroup,
        gui_view: wgpu::TextureView,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            inner: Arc::new(TextureInner {
                bind_group,
                gui_view,
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

    /// What tells the GPU texture from others while handles to it are kept, as
    /// [`Texture::same_as`] does. Never 0.
    pub(crate) fn id(&self) -> usize {
        Arc::as_ptr(&self.inner).addr()
    }

    #[cfg(feature = "egui")]
    pub(crate) fn gui_view(&self) -> &wgpu::TextureView {
        &self.inner.gui_view
    }

    /// A handle that doesn't keep the texture alive, for telling when it's gone.
    #[cfg(feature = "egui")]
    pub(crate) fn downgrade(&self) -> WeakTexture {
        WeakTexture(Arc::downgrade(&self.inner))
    }
}

/// A [`Texture`] that may have been dropped.
#[cfg(feature = "egui")]
pub(crate) struct WeakTexture(Weak<TextureInner>);

#[cfg(feature = "egui")]
impl WeakTexture {
    pub fn is(&self, texture: &Texture) -> bool {
        self.0.as_ptr() == Arc::as_ptr(&texture.inner)
    }

    /// Whether some handle still holds the texture.
    pub fn is_alive(&self) -> bool {
        self.0.strong_count() > 0
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
