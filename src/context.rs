use std::path::Path;

use crate::{
    Error, Result,
    graphics::{Texture, gpu::Gpu, renderer::Renderer},
};

/// Engine services available to a game, such as loading assets.
pub struct Context {
    pub(crate) gpu: Gpu,
    pub(crate) renderer: Renderer,
}

impl Context {
    /// Loads a PNG file into a texture.
    pub fn load_texture(&mut self, path: impl AsRef<Path>) -> Result<Texture> {
        let path = path.as_ref();
        let image = image::open(path)
            .map_err(|source| Error::LoadImage {
                path: path.to_owned(),
                source,
            })?
            .into_rgba8();
        Ok(self.renderer.create_texture(
            &self.gpu.device,
            &self.gpu.queue,
            image.width(),
            image.height(),
            image.as_raw(),
        ))
    }
}
