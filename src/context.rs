use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use crate::{
    Error, Fullscreen, Result, Waker,
    app::Redraws,
    audio::Audio,
    folders::Folders,
    graphics::{
        Font, FontSettings, Mesh, MeshData, MeshVertex, Resolution, Scaling, Texture,
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
    graphics::{Color, Model, ModelPart, Sprite, model_file},
    math::Rect,
};

/// Engine services available to a game: input, audio, saves, folders the player picks, the canvas
/// size, the built-in font, loading textures, fonts and models, and how long frames take.
pub struct Context {
    pub(crate) gpu: Gpu,
    pub(crate) renderer: Renderer,
    pub(crate) input: Input,
    pub(crate) audio: Audio,
    pub(crate) saves: Saves,
    pub(crate) folders: Folders,
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
    /// The size of what the frame being run is shown on, in physical pixels, and its scale
    /// factor: with the screen, what places a canvas pixel in the interface.
    #[cfg(feature = "egui")]
    pub(crate) output: ([u32; 2], f64),
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
    /// The [lights](crate::graphics::Light) that lit 3D views.
    pub lights: u32,
    /// The lights that reached what a 3D view saw but didn't light it, there being more than
    /// [`Light::MAX`](crate::graphics::Light::MAX) that did.
    pub lights_left_out: u32,
    /// The shadow maps those lights took, and one for each 3D view whose sun cast shadows
    /// ([`Lighting::shadows`](crate::graphics::Lighting::shadows)). Each is the meshes of its
    /// view that cast shadows drawn again, from a light or along the sun's, in draw calls
    /// `draw_calls` doesn't count.
    pub shadow_maps: u32,
    /// The lights with [shadows](crate::graphics::Light::shadows) that lit a 3D view without
    /// them, there being no more than
    /// [`Light::SHADOW_MAPS`](crate::graphics::Light::SHADOW_MAPS) maps in a view.
    pub shadows_left_out: u32,
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
            #[cfg(feature = "egui")]
            output: ([0, 0], 1.0),
            gpu,
            renderer,
            input: Input::new(),
            audio,
            saves: Saves::default(),
            folders: Folders::default(),
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

    /// The folders the player has picked, and the dialog for picking another, which a window
    /// opens once the frame that asks for it is done. A
    /// [`Simulation`](crate::headless::Simulation) opens none: its dialogs are answered by
    /// [`Simulation::pick`](crate::headless::Simulation::pick). See [`folders`](crate::folders).
    pub fn folders(&self) -> &Folders {
        &self.folders
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

    /// Whether the game's [interface](crate::gui) has `point` of the canvas, in canvas pixels, as
    /// the frame before laid it out: one of its windows or menus is there, or a panel, though
    /// not the space panels leave, or it's dragging something, wherever the pointer has got to.
    /// It's what the engine asks of a press, which it keeps from the game where this is true, so
    /// a game that says what's under the cursor in its world asks it too, and says nothing where
    /// the interface is. False until a frame has laid an interface out, and with the engine's
    /// own [debug window](crate::WindowConfig::debug_key) open, true over that as well.
    #[cfg(feature = "egui")]
    pub fn gui_covers(&self, point: [f32; 2]) -> bool {
        let (output, scale_factor) = self.output;
        let [x, y] = self.screen.to_output(point, output, scale_factor);
        let pixels_per_point = self.gui.egui.pixels_per_point();
        self.gui
            .wants_pointer_at(egui::pos2(x as f32, y as f32) / pixels_per_point)
    }

    /// Says whether the game has an interface to lay out, for a game that has one only
    /// sometimes, as while a menu is open: without, frames skip egui altogether, and
    /// [`Game::gui`](crate::Game::gui) isn't called, until the game says it has one again, from
    /// its `update`. A game that implements `gui` has one unless it says not, and the `gui` of
    /// one that doesn't implement it says not.
    ///
    /// `gui` is called once more after this is turned off, in a pass that should lay nothing
    /// out, so the interface forgets the windows it showed, and with them where it kept presses
    /// from the game. While the engine's [debug window](crate::WindowConfig::debug_key) is
    /// open, passes run and `gui` is called whatever this says.
    #[cfg(feature = "egui")]
    pub fn set_has_gui(&mut self, has_gui: bool) {
        self.gui.game_has_gui = has_gui;
    }

    /// Uploads `data` to the GPU as a mesh, to draw in 3D views. Fails if its indices aren't three
    /// for each triangle, or reach past its vertices.
    pub fn create_mesh(&mut self, data: &MeshData) -> Result<Mesh> {
        Mesh::new(&self.gpu.device, data).map_err(Error::Mesh)
    }

    /// Writes `vertices` over those `mesh` was made with, so a mesh whose corners move, such as
    /// a character posed each frame, a flag or water, is made once and written again, which
    /// costs much less than making it anew: about a quarter, for a mesh of 700 vertices. Its
    /// triangles stay between the same corners, wherever those now are. Fails unless there are
    /// as many vertices as the mesh was made with.
    ///
    /// Every handle to the mesh shows them, a model's parts among them, since handles share its
    /// buffers. A frame shows what was written last before it's rendered: the mesh keeps the
    /// vertices until a frame draws it, so writes between frames upload only the last, and
    /// writes with no frame rendered, as in a simulation, upload nothing.
    /// [`Mesh::bounds`] follows, worked out when it's next asked for, though a model's bounds
    /// stay as it was loaded. With the `physics` feature, colliders shaped like the mesh stay
    /// the shape it was made in.
    pub fn write_mesh(&mut self, mesh: &Mesh, vertices: &[MeshVertex]) -> Result<()> {
        self.write_mesh_from(mesh, vertices.iter().copied())
    }

    /// Writes a mesh as [`write_mesh`](Self::write_mesh) does, from vertices worked out as
    /// they're read, as a native library's are.
    pub(crate) fn write_mesh_from(
        &mut self,
        mesh: &Mesh,
        vertices: impl ExactSizeIterator<Item = MeshVertex>,
    ) -> Result<()> {
        mesh.write(vertices).map_err(Error::Mesh)
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

    /// Makes a texture of pixels the game worked out, as [`load_texture`](Self::load_texture)
    /// makes one of a file's: `width` by `height` of them, sRGB RGBA bytes, row by row from the
    /// top, sampled as a loaded texture's are. Fails unless `rgba` is four bytes for each pixel,
    /// and for a size that's empty or larger than the GPU takes.
    ///
    /// It's for what a game bakes, generates, or decodes itself, on a thread of its own if it
    /// likes, since only the upload needs the context.
    pub fn create_texture(&mut self, width: u32, height: u32, rgba: &[u8]) -> Result<Texture> {
        let size = [width, height];
        if let Some(problem) = texture_size_problem(&self.gpu.device, "a texture", size) {
            return Err(Error::Texture(problem));
        }
        let bytes = width as usize * height as usize * 4;
        if rgba.len() != bytes {
            return Err(Error::Texture(format!(
                "{width}x{height} pixels are {bytes} bytes of RGBA, not {}",
                rgba.len()
            )));
        }
        Ok(self
            .renderer
            .create_texture(&self.gpu.device, &self.gpu.queue, width, height, rgba))
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
    /// materials are left out, and files that need an extension the engine doesn't read are
    /// refused, but for `KHR_lights_punctual`, which a file with a lamp in it says it needs: its
    /// lights are left out all the same.
    ///
    /// The model doesn't keep its triangles where a game can read them: to pick by them, read
    /// them with [`Model::read_triangles`].
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
            .map(|part| {
                // Nothing is brighter than white, so a strength over 1 reaches it and no further.
                let [r, g, b] = part.emission.map(|c| c.clamp(0.0, 1.0));
                ModelPart {
                    mesh: meshes[part.mesh].clone(),
                    sprite: part.texture.map(|texture| sprites[texture].clone()),
                    transform: part.transform,
                    double_sided: part.double_sided,
                    emission: Color::from_linear([r, g, b, 1.0]),
                    emission_sprite: part
                        .emission_texture
                        .map(|texture| sprites[texture].clone()),
                }
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

    #[cfg(feature = "models")]
    #[test]
    fn models_give_off_their_emission_no_brighter_than_white() {
        let mut headless = Headless::new().expect("a GPU adapter is available");
        let ctx = headless.context();
        let folder = TemporaryFolder::new();
        std::fs::create_dir(folder.path()).unwrap();
        let positions: [f32; 9] = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
        let bytes: &[u8] = bytemuck::cast_slice(&positions);
        std::fs::write(folder.path().join("tri.bin"), bytes).unwrap();
        // A quarter and a half, four times as strong: past white in red, and so white.
        let json = r#"{"asset":{"version":"2.0"},"extensionsUsed":["KHR_materials_emissive_strength"],
            "buffers":[{"byteLength":36,"uri":"tri.bin"}],
            "bufferViews":[{"buffer":0,"byteLength":36}],
            "accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}],
            "materials":[{"emissiveFactor":[0.25,0.125,0],"extensions":{"KHR_materials_emissive_strength":{"emissiveStrength":4}}}],
            "meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0}]}],
            "nodes":[{"mesh":0}]}"#;
        let path = folder.path().join("tri.gltf");
        std::fs::write(&path, json).unwrap();
        let model = ctx.load_model(&path).unwrap();
        let part = &model.parts()[0];
        // Linear 0.5 is sRGB 0.735.
        assert_eq!(part.emission.to_rgba8(), [255, 188, 0, 255]);
        assert!(part.emission_sprite.is_none());
    }
}
