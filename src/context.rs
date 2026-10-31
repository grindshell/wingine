use std::path::Path;

use crate::{
    Error, Result,
    audio::Audio,
    graphics::{Font, Texture, gpu::Gpu, renderer::Renderer},
    input::Input,
};

/// Engine services available to a game: input, audio, the canvas size, the built-in font, asset
/// loading, and how long frames take.
pub struct Context {
    pub(crate) gpu: Gpu,
    pub(crate) renderer: Renderer,
    pub(crate) input: Input,
    pub(crate) audio: Audio,
    pub(crate) canvas_size: [f32; 2],
    pub(crate) frame_stats: FrameStats,
    default_font: Font,
}

/// How long the engine took over a frame, and how much it drew, for games to show how they
/// perform. Times are wall-clock seconds measured on the CPU.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameStats {
    /// The whole frame. In a window, from its start to the next frame's, including waiting for
    /// vsync and the GPU. In a [`Simulation`](crate::headless::Simulation), the frame alone,
    /// including waiting for the GPU when it renders every frame.
    pub frame: f32,
    /// [`Game::update`](crate::Game::update).
    pub update: f32,
    /// [`Game::draw`](crate::Game::draw), recording what to draw.
    pub draw: f32,
    /// Rendering what was drawn: uploading it and submitting draw calls to the GPU, but not the
    /// GPU's own work. Zero for a frame that wasn't rendered.
    pub render: f32,
    /// The quads the game drew: sprites, rectangles and glyphs.
    pub quads: u32,
    /// The draw calls they took: one for each run of quads drawn with the same texture.
    pub draw_calls: u32,
}

impl Context {
    pub(crate) fn new(gpu: Gpu, renderer: Renderer, audio: Audio) -> Self {
        let default_font = Font::built_in(&renderer, &gpu.device, &gpu.queue);
        Self {
            gpu,
            renderer,
            input: Input::new(),
            audio,
            canvas_size: [0.0, 0.0],
            frame_stats: FrameStats::default(),
            default_font,
        }
    }

    /// How long the frame before this one took, and how much it drew. All zero until a frame
    /// has finished.
    pub fn frame_stats(&self) -> FrameStats {
        self.frame_stats
    }

    /// Sound effects and music. Silent when running headless.
    pub fn audio(&mut self) -> &mut Audio {
        &mut self.audio
    }

    /// The built-in pixel font: printable ASCII and a little typographic punctuation, with
    /// capitals 7 pixels tall and lines 10 pixels apart. Characters it lacks draw as `?`.
    pub fn default_font(&self) -> &Font {
        &self.default_font
    }

    /// Keyboard and mouse state for the current frame.
    pub fn input(&self) -> &Input {
        &self.input
    }

    /// Keyboard and mouse state, for binding actions.
    pub fn input_mut(&mut self) -> &mut Input {
        &mut self.input
    }

    /// Width and height of the canvas this frame will be drawn on, in canvas pixels. While the
    /// game is being created, the size the first frame will have.
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
