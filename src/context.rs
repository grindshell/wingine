use std::{path::Path, sync::Arc, time::Duration};

use crate::{
    Error, Result, Waker,
    app::Redraws,
    audio::Audio,
    graphics::{Font, Mesh, MeshData, Texture, gpu::Gpu, renderer::Renderer},
    input::Input,
    save::Saves,
};
#[cfg(feature = "models")]
use crate::{
    graphics::{Model, ModelPart, Sprite, model_file},
    math::Rect,
};

/// Engine services available to a game: input, audio, saves, the canvas size, the built-in font,
/// asset loading, and how long frames take.
pub struct Context {
    pub(crate) gpu: Gpu,
    pub(crate) renderer: Renderer,
    pub(crate) input: Input,
    pub(crate) audio: Audio,
    pub(crate) saves: Saves,
    pub(crate) canvas_size: [f32; 2],
    pub(crate) frame_stats: FrameStats,
    /// Whether the game has asked to close its window.
    pub(crate) quit: bool,
    /// When the game wants its next frame, if it redraws on demand.
    pub(crate) redraws: Arc<Redraws>,
    #[cfg(feature = "egui")]
    pub(crate) gui: crate::gui::Gui,
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
    /// The triangles of the meshes the game drew in 3D views.
    pub triangles: u32,
    /// The draw calls they took: one for each run of quads drawn with the same texture, and one
    /// for each mesh.
    pub draw_calls: u32,
}

impl Context {
    pub(crate) fn new(gpu: Gpu, renderer: Renderer, audio: Audio) -> Self {
        let default_font = Font::built_in(&renderer, &gpu.device, &gpu.queue);
        let redraws = Arc::<Redraws>::default();
        #[cfg(feature = "egui")]
        let gui = {
            let gui = crate::gui::Gui::new(renderer.target_format());
            // egui asks for frames as its animations and text cursors need them, and as code on
            // any thread asks it to repaint.
            let redraws = Arc::clone(&redraws);
            gui.egui
                .set_request_repaint_callback(move |info| redraws.request_after(info.delay));
            gui
        };
        Self {
            #[cfg(feature = "egui")]
            gui,
            gpu,
            renderer,
            input: Input::new(),
            audio,
            saves: Saves::default(),
            canvas_size: [0.0, 0.0],
            frame_stats: FrameStats::default(),
            quit: false,
            redraws,
            default_font,
        }
    }

    /// Closes the window once this frame is done, ending [`run`](crate::run). A
    /// [`Simulation`](crate::headless::Simulation) only notes it: see
    /// [`Simulation::has_quit`](crate::headless::Simulation::has_quit).
    pub fn quit(&mut self) {
        self.quit = true;
    }

    /// Asks for another frame after this one, for a game that [redraws on
    /// demand](crate::Redraw::OnDemand) and has something more to show, such as an animation
    /// playing. Games whose frames come one after another needn't ask.
    pub fn request_redraw(&self) {
        self.redraws.request_after(Duration::ZERO);
    }

    /// Asks for a frame `seconds` from now, at the latest, for a game that [redraws on
    /// demand](crate::Redraw::OnDemand) and has something to show then.
    pub fn request_redraw_after(&self, seconds: f32) {
        if let Ok(delay) = Duration::try_from_secs_f32(seconds.max(0.0)) {
            self.redraws.request_after(delay);
        }
    }

    /// A [`Waker`], which asks for frames from other threads.
    pub fn waker(&self) -> Waker {
        Waker(Arc::clone(&self.redraws))
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

    /// The game's saves, which last after it ends. A
    /// [`Simulation`](crate::headless::Simulation) keeps them in a folder of its own, unless its
    /// config names one: see [`save`](crate::save).
    pub fn saves(&self) -> &Saves {
        &self.saves
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

    /// Uploads `data` to the GPU as a mesh, to draw in 3D views. Fails if its indices aren't three
    /// for each triangle, or reach past its vertices.
    pub fn create_mesh(&mut self, data: &MeshData) -> Result<Mesh> {
        Mesh::new(&self.gpu.device, data).map_err(Error::Mesh)
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

    /// Loads a model file, glTF 2.0 as `.glb` or `.gltf`, with the files it names beside it, for
    /// drawing with [`Canvas3d::draw_model`](crate::graphics::Canvas3d::draw_model).
    ///
    /// It's what the default scene's nodes place: their meshes' triangles, with their normals
    /// (flat where the file has none), first texture coordinates and vertex colors, and each
    /// material's base color and its texture, which must be a PNG. A material's alpha cuts out
    /// what's less than half opaque, as meshes do, unless it's opaque, when its alpha is ignored.
    /// Skins, morph targets, animations, cameras, lights and the rest of materials are left out,
    /// and files that need an extension are refused.
    #[cfg(feature = "models")]
    pub fn load_model(&mut self, path: impl AsRef<Path>) -> Result<Model> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|source| Error::ReadFile {
            path: path.to_owned(),
            source,
        })?;
        let error = |message: String| Error::Model {
            path: path.to_owned(),
            message,
        };
        let folder = path.parent().unwrap_or(Path::new(""));
        let data = model_file::read(&bytes, folder).map_err(error)?;
        let meshes = data
            .meshes
            .iter()
            .map(|mesh| Mesh::new(&self.gpu.device, mesh))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(error)?;
        let sprites: Vec<Sprite> = data
            .images
            .iter()
            .map(|image| {
                let [width, height] = [image.width(), image.height()];
                let texture = self.renderer.create_texture(
                    &self.gpu.device,
                    &self.gpu.queue,
                    width,
                    height,
                    image.as_raw(),
                );
                Sprite::new(texture, Rect::new(0.0, 0.0, width as f32, height as f32))
            })
            .collect();
        let parts = data
            .parts
            .iter()
            .map(|part| ModelPart {
                mesh: meshes[part.mesh].clone(),
                sprite: part.image.map(|image| sprites[image].clone()),
                transform: part.transform,
            })
            .collect();
        Ok(Model::new(parts))
    }
}
