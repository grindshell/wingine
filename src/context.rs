use std::path::Path;

use crate::{
    Error, Result,
    graphics::{Texture, gpu::Gpu, renderer::Renderer},
    input::Input,
};

/// Engine services available to a game: input, the canvas size, and asset loading.
pub struct Context {
    pub(crate) gpu: Gpu,
    pub(crate) renderer: Renderer,
    pub(crate) input: Input,
    pub(crate) canvas_size: [f32; 2],
}

impl Context {
    pub(crate) fn new(gpu: Gpu, renderer: Renderer) -> Self {
        Self {
            gpu,
            renderer,
            input: Input::new(),
            canvas_size: [0.0, 0.0],
        }
    }

    /// Keyboard and mouse state for the current frame.
    pub fn input(&self) -> &Input {
        &self.input
    }

    /// Keyboard and mouse state, for binding actions.
    pub fn input_mut(&mut self) -> &mut Input {
        &mut self.input
    }

    /// Width and height of the canvas this frame will be drawn on, in canvas pixels. Zero
    /// before the first frame.
    pub fn canvas_size(&self) -> [f32; 2] {
        self.canvas_size
    }

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
