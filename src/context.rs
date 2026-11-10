use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use crate::{
    Error, Fullscreen, Result, Waker,
    app::Redraws,
    audio::Audio,
    graphics::{
        Font, FontSettings, Mesh, MeshData, Resolution, Scaling, Texture,
        gpu::{Gpu, texture_size_problem},
        renderer::Renderer,
        screen::Screen,
    },
    input::Input,
    save::Saves,
    scene::FontFile,
};
#[cfg(feature = "models")]
use crate::{
    graphics::{Model, ModelPart, Sprite, model_file},
    math::Rect,
};

/// Engine services available to a game: input, audio, saves, the canvas size, the built-in font,
/// loading textures, fonts and models, and how long frames take.
pub struct Context {
    pub(crate) gpu: Gpu,
    pub(crate) renderer: Renderer,
    pub(crate) input: Input,
    pub(crate) audio: Audio,
    pub(crate) saves: Saves,
    pub(crate) canvas_size: [f32; 2],
    /// Draws the canvas onto the window, or a headless frame, at its resolution and scaling.
    pub(crate) screen: Screen,
    /// Whether the window covers the screen, or is to once the frame is done.
    pub(crate) fullscreen: Fullscreen,
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
    /// for each kind of mesh each 3D view drew, the same mesh showing the same texture.
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
            screen: Screen::new(Resolution::Window),
            fullscreen: Fullscreen::Off,
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

    /// Whether the window covers the screen: as it opened ([`WindowConfig::fullscreen`]), as
    /// the game last set it, or as the player changed it through the system, such as with
    /// macOS's green button or a browser's Escape. A window that couldn't be exclusive is
    /// [borderless](Fullscreen::Borderless).
    ///
    /// [`WindowConfig::fullscreen`]: crate::WindowConfig::fullscreen
    pub fn fullscreen(&self) -> Fullscreen {
        self.fullscreen
    }

    /// Makes the window cover the screen, or a window of its own size again, once this frame is
    /// done. The canvas changes size with it, unless it has a fixed resolution, which is
    /// [scaled](Context::set_scaling) to the screen. A
    /// [`Simulation`](crate::headless::Simulation) only notes it, keeping its frames' size.
    pub fn set_fullscreen(&mut self, fullscreen: Fullscreen) {
        self.fullscreen = fullscreen;
    }

    /// How a fixed resolution fills the window: as it opened ([`WindowConfig::scaling`]), or
    /// as the game last set it.
    ///
    /// [`WindowConfig::scaling`]: crate::WindowConfig::scaling
    pub fn scaling(&self) -> Scaling {
        self.screen.scaling()
    }

    /// Scales a fixed resolution to fill the window by `scaling`, from the frame this one draws.
    /// It changes nothing without a fixed resolution, since the canvas is the window.
    pub fn set_scaling(&mut self, scaling: Scaling) {
        self.screen.set_scaling(scaling);
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
        let error = |source| Error::LoadImage {
            path: path.to_owned(),
            source,
        };
        #[cfg(not(target_arch = "wasm32"))]
        let image = image::open(path).map_err(error)?.into_rgba8();
        #[cfg(target_arch = "wasm32")]
        let image = {
            let bytes = crate::files::read(path).map_err(|err| error(err.into()))?;
            image::load_from_memory(&bytes).map_err(error)?.into_rgba8()
        };
        let size = [image.width(), image.height()];
        if let Some(message) = texture_size_problem(&self.gpu.device, "an image", size) {
            return Err(Error::Image {
                path: path.to_owned(),
                message,
            });
        }
        Ok(self.renderer.create_texture(
            &self.gpu.device,
            &self.gpu.queue,
            image.width(),
            image.height(),
            image.as_raw(),
        ))
    }

    /// Loads a font file, `*.font.luau` by convention: a bitmap font, its glyphs drawn in a PNG
    /// divided into cells, or a TrueType or OpenType font at a size, each named by its path from
    /// the font file's folder. See [`scene`](crate::scene) for the format.
    pub fn load_font(&mut self, path: impl AsRef<Path>) -> Result<Font> {
        let path = path.as_ref();
        let folder = path.parent().unwrap_or(Path::new(""));
        match FontFile::read(path)? {
            FontFile::Bitmap(def) => {
                let image_path = folder.join(&def.image);
                let bytes = crate::files::read(&image_path).map_err(|source| Error::ReadFile {
                    path: image_path.clone(),
                    source,
                })?;
                let image = image::load_from_memory(&bytes)
                    .map_err(|source| Error::LoadImage {
                        path: image_path.clone(),
                        source,
                    })?
                    .into_rgba8();
                let files = vec![path.to_owned(), image_path];
                Font::bitmap(
                    &self.renderer,
                    &self.gpu.device,
                    &self.gpu.queue,
                    &def,
                    &image,
                    files,
                )
                .map_err(|message| Error::Font {
                    path: path.to_owned(),
                    message,
                })
            }
            FontFile::Vector(def) => {
                let file = folder.join(&def.file);
                let settings = FontSettings {
                    size: def.size,
                    smooth: def.smooth,
                };
                let files = vec![path.to_owned(), file.clone()];
                self.vector_font(&file, settings, files)
            }
        }
    }

    /// Loads a TrueType or OpenType font file, `.ttf` or `.otf`, to draw as `settings` say. Its
    /// glyphs are rasterized as text first needs them, so it can hold as many as it likes.
    /// Characters it lacks draw as its `?`.
    pub fn load_vector_font(
        &mut self,
        path: impl AsRef<Path>,
        settings: FontSettings,
    ) -> Result<Font> {
        let path = path.as_ref();
        self.vector_font(path, settings, vec![path.to_owned()])
    }

    /// The TrueType or OpenType font at `path`, read from `files`.
    fn vector_font(
        &mut self,
        path: &Path,
        settings: FontSettings,
        files: Vec<PathBuf>,
    ) -> Result<Font> {
        let bytes = crate::files::read(path).map_err(|source| Error::ReadFile {
            path: path.to_owned(),
            source,
        })?;
        let pages = self.renderer.glyph_pages(&self.gpu.device, &self.gpu.queue);
        Font::vector(bytes, settings, files, pages).map_err(|message| Error::Font {
            path: path.to_owned(),
            message,
        })
    }

    /// Loads a model file, glTF 2.0 as `.glb` or `.gltf`, with the files it names beside it, for
    /// drawing with [`Canvas3d::draw_model`](crate::graphics::Canvas3d::draw_model).
    ///
    /// It's what the default scene's nodes place: their meshes' triangles, with their normals
    /// (flat where the file has none), first texture coordinates and vertex colors, and each
    /// material's base color and its texture, which must be a PNG, repeating or mirrored where
    /// texture coordinates leave it if its sampler says so, and moved by its
    /// `KHR_texture_transform`. A material's alpha cuts out what's less than half opaque, as
    /// meshes do, unless it's opaque, when its alpha is ignored, and a double-sided material shows
    /// from behind too. Skins, morph targets, animations, cameras, lights and the rest of
    /// materials are left out, and files that need an extension are refused.
    #[cfg(feature = "models")]
    pub fn load_model(&mut self, path: impl AsRef<Path>) -> Result<Model> {
        let path = path.as_ref();
        let bytes = crate::files::read(path).map_err(|source| Error::ReadFile {
            path: path.to_owned(),
            source,
        })?;
        let error = |message: String| Error::Model {
            path: path.to_owned(),
            message,
        };
        let folder = path.parent().unwrap_or(Path::new(""));
        let data = model_file::read(&bytes, folder).map_err(error)?;
        for texture in &data.textures {
            let size = [texture.image.width(), texture.image.height()];
            if let Some(problem) = texture_size_problem(&self.gpu.device, "a texture", size) {
                return Err(error(problem));
            }
        }
        let meshes = data
            .meshes
            .iter()
            .map(|mesh| Mesh::new(&self.gpu.device, mesh))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(error)?;
        let sprites: Vec<Sprite> = data
            .textures
            .iter()
            .map(|texture| {
                let image = &texture.image;
                let [width, height] = [image.width(), image.height()];
                let address = texture.wrap.map(|wrap| match wrap {
                    model_file::Wrap::Clamp => wgpu::AddressMode::ClampToEdge,
                    model_file::Wrap::Repeat => wgpu::AddressMode::Repeat,
                    model_file::Wrap::Mirror => wgpu::AddressMode::MirrorRepeat,
                });
                let texture = self.renderer.create_wrapped_texture(
                    &self.gpu.device,
                    &self.gpu.queue,
                    [width, height],
                    image.as_raw(),
                    address,
                );
                Sprite::new(texture, Rect::new(0.0, 0.0, width as f32, height as f32))
            })
            .collect();
        let parts = data
            .parts
            .iter()
            .map(|part| ModelPart {
                mesh: meshes[part.mesh].clone(),
                sprite: part.texture.map(|texture| sprites[texture].clone()),
                transform: part.transform,
                double_sided: part.double_sided,
            })
            .collect();
        Ok(Model::read_from(parts, data.files))
    }
}

#[cfg(test)]
mod tests {
    use image::RgbaImage;

    use crate::{Error, headless::Headless, save::TemporaryFolder};

    #[test]
    fn images_larger_than_the_gpu_takes_are_errors() {
        let mut headless = Headless::new().expect("a GPU adapter is available");
        let ctx = headless.context();
        let limit = ctx.gpu.device.limits().max_texture_dimension_2d;
        let folder = TemporaryFolder::new();
        std::fs::create_dir(folder.path()).unwrap();
        let [fits, tall] = ["fits.png", "tall.png"].map(|name| folder.path().join(name));
        RgbaImage::new(limit, 1).save(&fits).unwrap();
        RgbaImage::new(1, limit + 1).save(&tall).unwrap();
        assert_eq!(ctx.load_texture(&fits).unwrap().width(), limit);
        let err = ctx.load_texture(&tall).unwrap_err();
        assert!(matches!(err, Error::Image { .. }), "{err}");
        let message = format!(
            "an image is from 1x1 to {limit}x{limit} pixels, not 1x{}",
            limit + 1
        );
        assert!(err.to_string().ends_with(&message), "{err}");
    }
}
