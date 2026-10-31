//! Rendering and running games without a window: for screenshot tests, for testing game behavior
//! with simulated input, and for offline rendering.

use std::{
    path::Path,
    sync::{Condvar, Mutex, PoisonError, mpsc},
    thread::{self, ThreadId},
    time::Instant,
};

use crate::{
    Context, Error, Game, Result, WindowConfig,
    app::{Timestep, run_frame},
    audio::Audio,
    graphics::{Canvas, Resolution, gpu::Gpu, renderer::Renderer, screen::Screen},
    input::{Key, MouseButton},
};

/// Format of headless render targets. sRGB, matching what the window surface is viewed as.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

/// Renders frames into memory instead of a window.
///
/// The GPU adapter is chosen as for a window, including wgpu's environment variable overrides
/// (`WGPU_BACKEND`, `WGPU_ADAPTER_NAME`, `WGPU_POWER_PREF`). Software adapters such as WARP and
/// lavapipe work, which lets this run on machines without a GPU.
///
/// Every `Headless` in a process shares one GPU device, and only one thread at a time can have
/// any: creating one waits until other threads have dropped theirs. Software adapters render
/// wrongly or crash when used from several threads at once, so this keeps parallel tests
/// reliable.
pub struct Headless {
    ctx: Context,
    screen: Screen,
    canvas: Canvas,
    /// What frames render to when they aren't read back, kept while frames stay the same size.
    target: Option<(wgpu::Texture, wgpu::TextureView)>,
    /// Declared last so it's released after the GPU resources above are dropped.
    _lease: GpuLease,
}

impl Headless {
    pub fn new() -> Result<Self> {
        let lease = GpuLease::acquire();
        let gpu = shared_gpu()?;
        let renderer = Renderer::new(&gpu.device, &gpu.queue, FORMAT);
        Ok(Self {
            ctx: Context::new(gpu, renderer, Audio::silent()),
            screen: Screen::new(Resolution::Window),
            canvas: Canvas::new(),
            target: None,
            _lease: lease,
        })
    }

    /// Sets the canvas size for later frames, as [`WindowConfig::resolution`] does for a window.
    ///
    /// [`WindowConfig::resolution`]: crate::WindowConfig::resolution
    pub fn set_resolution(&mut self, resolution: Resolution) {
        self.screen = Screen::new(resolution);
    }

    /// Engine services, for loading the assets a frame draws.
    pub fn context(&mut self) -> &mut Context {
        &mut self.ctx
    }

    /// Name and backend of the GPU adapter rendering the frames, for diagnostics.
    pub fn adapter_name(&self) -> String {
        let info = self.ctx.gpu.adapter.get_info();
        format!("{} ({:?})", info.name, info.backend)
    }

    /// Renders one `width` by `height` frame drawn by `draw` and returns its pixels.
    ///
    /// The frame is treated like a window of that size with a scale factor of 1: with
    /// [`Resolution::Window`] the canvas is `width` by `height`, and with a fixed resolution the
    /// canvas is scaled onto the frame.
    pub fn render(
        &mut self,
        width: u32,
        height: u32,
        draw: impl FnOnce(&mut Canvas),
    ) -> Result<Image> {
        self.draw([width, height], draw);
        self.capture([width, height])
    }

    /// Records a frame on the canvas, sized for an `output`-sized frame.
    fn draw(&mut self, output: [u32; 2], draw: impl FnOnce(&mut Canvas)) {
        let [width, height] = self.screen.canvas_size(output, 1.0);
        self.canvas.begin(width, height);
        draw(&mut self.canvas);
    }

    /// Renders the recorded canvas onto an `output`-sized frame and reads it back.
    fn capture(&mut self, output: [u32; 2]) -> Result<Image> {
        let Context { gpu, renderer, .. } = &mut self.ctx;
        let texture = create_frame(gpu, output);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.screen.render(
            &gpu.device,
            &gpu.queue,
            renderer,
            &self.canvas,
            &view,
            output,
        );
        read_texture(gpu, &texture)
    }

    /// Renders the recorded canvas onto an `output`-sized frame, as a window would, then waits
    /// for the GPU to finish it. Returns the seconds rendering took on the CPU.
    fn render_frame(&mut self, output: [u32; 2]) -> f32 {
        let Context { gpu, renderer, .. } = &mut self.ctx;
        let fits = |(texture, _): &(wgpu::Texture, wgpu::TextureView)| {
            [texture.width(), texture.height()] == output
        };
        if !self.target.as_ref().is_some_and(fits) {
            let texture = create_frame(gpu, output);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.target = Some((texture, view));
        }
        let (_, view) = self.target.as_ref().expect("created above");
        let start = Instant::now();
        self.screen.render(
            &gpu.device,
            &gpu.queue,
            renderer,
            &self.canvas,
            view,
            output,
        );
        let render = start.elapsed().as_secs_f32();
        if let Err(err) = gpu.device.poll(wgpu::PollType::wait_indefinitely()) {
            log::warn!("couldn't wait for the GPU to render a frame: {err}");
        }
        render
    }
}

/// A texture to render an `output`-sized frame on, which can be read back.
fn create_frame(gpu: &Gpu, [width, height]: [u32; 2]) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("headless frame"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Runs a [`Game`] without a window, driven by simulated input: for tests of game behavior, and
/// for scripted runs (see the `WINGINE_SCRIPT` variable in [`run`](crate::run)).
///
/// Frames only advance when asked to, each by exactly [`Simulation::FRAME_TIME`], so runs are
/// deterministic. With an [update rate](WindowConfig::update_rate), each frame runs the updates
/// that time holds, as a window would. Input takes effect on the next frame, as events from a
/// window would: for example, a key held down is pressed on the next frame and held until
/// released.
///
/// Frames are drawn but not rendered, unless [`Simulation::set_render_every_frame`] says to:
/// only screenshots render.
///
/// Like [`Headless`], only one thread at a time can have simulations.
pub struct Simulation<G> {
    /// Declared before `headless`, so the game's GPU resources drop while it still holds the GPU.
    game: G,
    headless: Headless,
    timestep: Timestep,
    /// The virtual window's size, in physical pixels.
    output: [u32; 2],
    /// Whether the canvas holds a frame drawn at the current output size.
    drawn: bool,
    render_every_frame: bool,
    frames: u64,
}

impl<G> Simulation<G> {
    /// Seconds of game time each frame advances: 60 frames per second.
    pub const FRAME_TIME: f32 = 1.0 / 60.0;

    /// How many frames `seconds` of game time takes, to the nearest whole frame.
    pub(crate) fn frames_in(seconds: f32) -> u32 {
        (seconds / Self::FRAME_TIME).round() as u32
    }
}

impl<G: Game> Simulation<G> {
    /// Starts the game built by `init`, as [`run`](crate::run) would with `config`. The virtual
    /// window is `config.width` by `config.height` physical pixels, with a scale factor of 1.
    ///
    /// # Panics
    ///
    /// If `config` has an update rate that isn't above 0.
    pub fn new(
        config: &WindowConfig,
        init: impl FnOnce(&mut Context) -> Result<G>,
    ) -> Result<Self> {
        let timestep = Timestep::new(config.update_rate);
        let mut headless = Headless::new()?;
        headless.set_resolution(config.resolution);
        let output = [config.width, config.height];
        headless.ctx.canvas_size = headless.screen.canvas_size(output, 1.0);
        let game = init(&mut headless.ctx)?;
        Ok(Self {
            headless,
            game,
            timestep,
            output,
            drawn: false,
            render_every_frame: false,
            frames: 0,
        })
    }

    pub fn game(&self) -> &G {
        &self.game
    }

    pub fn game_mut(&mut self) -> &mut G {
        &mut self.game
    }

    pub fn context(&mut self) -> &mut Context {
        &mut self.headless.ctx
    }

    /// Name and backend of the GPU adapter rendering screenshots, for diagnostics.
    pub fn adapter_name(&self) -> String {
        self.headless.adapter_name()
    }

    /// Frames run so far.
    pub fn frame_count(&self) -> u64 {
        self.frames
    }

    /// Whether each frame is rendered on the GPU after it's drawn, as a window would render it,
    /// and waited for. Off by default. With it on, frames take longer, and their
    /// [`FrameStats`](crate::FrameStats) include rendering, as benchmarks want.
    ///
    /// Turning it on renders the latest frame once, without counting it, so that setting up to
    /// render doesn't slow the next frame.
    pub fn set_render_every_frame(&mut self, render: bool) {
        if render && !self.render_every_frame {
            self.headless.render_frame(self.output);
        }
        self.render_every_frame = render;
    }

    /// Runs one frame: updates the game, then draws it.
    pub fn step(&mut self) {
        let start = Instant::now();
        let Self {
            headless,
            game,
            timestep,
            output,
            ..
        } = self;
        let canvas_size = headless.screen.canvas_size(*output, 1.0);
        let mut stats = run_frame(
            game,
            &mut headless.ctx,
            &mut headless.canvas,
            canvas_size,
            timestep,
            Self::FRAME_TIME,
        );
        if self.render_every_frame {
            stats.render = self.headless.render_frame(self.output);
        }
        stats.frame = start.elapsed().as_secs_f32();
        self.headless.ctx.frame_stats = stats;
        self.drawn = true;
        self.frames += 1;
    }

    pub fn run_frames(&mut self, frames: u32) {
        for _ in 0..frames {
            self.step();
        }
    }

    /// Runs `seconds` of game time, rounded to whole frames.
    pub fn run_for(&mut self, seconds: f32) {
        self.run_frames(Self::frames_in(seconds));
    }

    pub fn key_down(&mut self, key: Key) {
        self.headless.ctx.input.key_down(key);
    }

    pub fn key_up(&mut self, key: Key) {
        self.headless.ctx.input.key_up(key);
    }

    /// Presses `key`, runs one frame, and releases it.
    pub fn tap(&mut self, key: Key) {
        self.key_down(key);
        self.step();
        self.key_up(key);
    }

    /// Holds `key` down for `seconds` of game time, rounded to whole frames.
    pub fn hold(&mut self, key: Key, seconds: f32) {
        self.hold_frames(key, Self::frames_in(seconds));
    }

    /// Holds `key` down for `frames` frames.
    pub fn hold_frames(&mut self, key: Key, frames: u32) {
        self.key_down(key);
        self.run_frames(frames);
        self.key_up(key);
    }

    /// Moves the cursor to `x`, `y` in canvas pixels.
    pub fn move_mouse(&mut self, x: f32, y: f32) {
        self.headless.ctx.input.set_mouse_position(Some([x, y]));
    }

    pub fn mouse_down(&mut self, button: MouseButton) {
        self.headless.ctx.input.mouse_down(button);
    }

    pub fn mouse_up(&mut self, button: MouseButton) {
        self.headless.ctx.input.mouse_up(button);
    }

    /// Moves the cursor to `x`, `y` in canvas pixels, presses `button`, runs one frame, and
    /// releases it.
    pub fn click(&mut self, button: MouseButton, x: f32, y: f32) {
        self.move_mouse(x, y);
        self.tap_mouse(button);
    }

    /// Presses `button` where the cursor is, runs one frame, and releases it, as
    /// [`Simulation::tap`] does with a key.
    pub fn tap_mouse(&mut self, button: MouseButton) {
        self.mouse_down(button);
        self.step();
        self.mouse_up(button);
    }

    /// Resizes the virtual window to `width` by `height` physical pixels.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.output = [width, height];
        self.drawn = false;
    }

    /// Renders the latest frame as the window would show it. Before the first frame, or after a
    /// resize, the game draws again first, without updating.
    pub fn screenshot(&mut self) -> Result<Image> {
        if !self.drawn {
            let Self {
                headless,
                game,
                output,
                ..
            } = self;
            headless.draw(*output, |canvas| game.draw(canvas));
            self.drawn = true;
        }
        self.headless.capture(self.output)
    }
}

/// The device shared by every [`Headless`] in the process.
///
/// Creating a device is slow on software adapters, and Mesa's lavapipe intermittently crashes
/// when a process uses several at once, as parallel tests would.
fn shared_gpu() -> Result<Gpu> {
    static GPU: Mutex<Option<Gpu>> = Mutex::new(None);
    // A panic while creating the device leaves nothing behind, so a poisoned lock is still usable.
    let mut shared = GPU.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(gpu) = &*shared {
        return Ok(gpu.clone());
    }
    let instance =
        wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let gpu = pollster::block_on(Gpu::new(instance, None))?;
    *shared = Some(gpu.clone());
    Ok(gpu)
}

/// Use of the GPU by one thread at a time, held by each [`Headless`] for its lifetime.
///
/// Software adapters misbehave when a process renders from several threads at once, as parallel
/// tests do: lavapipe crashes with a device per thread, and WARP renders wrong results with a
/// shared device. So headless rendering runs on one thread at a time; other threads wait for the
/// lease. A thread can hold several leases, so it can use several `Headless`es at once.
struct GpuLease(());

struct Leases {
    holder: Option<ThreadId>,
    count: usize,
}

static LEASES: Mutex<Leases> = Mutex::new(Leases {
    holder: None,
    count: 0,
});
static LEASE_RELEASED: Condvar = Condvar::new();

impl GpuLease {
    fn acquire() -> Self {
        let this_thread = thread::current().id();
        let mut leases = LEASES.lock().unwrap_or_else(PoisonError::into_inner);
        while leases.holder.is_some_and(|holder| holder != this_thread) {
            leases = LEASE_RELEASED
                .wait(leases)
                .unwrap_or_else(PoisonError::into_inner);
        }
        leases.holder = Some(this_thread);
        leases.count += 1;
        Self(())
    }
}

impl Drop for GpuLease {
    fn drop(&mut self) {
        let mut leases = LEASES.lock().unwrap_or_else(PoisonError::into_inner);
        leases.count -= 1;
        if leases.count == 0 {
            leases.holder = None;
            LEASE_RELEASED.notify_all();
        }
    }
}

/// Copies an RGBA8 texture's pixels back to the CPU.
fn read_texture(gpu: &Gpu, texture: &wgpu::Texture) -> Result<Image> {
    let (width, height) = (texture.width(), texture.height());
    let row_bytes = width * 4;
    // Texture-to-buffer copies need rows padded to a fixed alignment.
    let padded_row_bytes = row_bytes.next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);

    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("headless readback"),
        size: u64::from(padded_row_bytes) * u64::from(height),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("headless readback"),
        });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row_bytes),
                rows_per_image: Some(height),
            },
        },
        texture.size(),
    );
    gpu.queue.submit([encoder.finish()]);

    let (sender, receiver) = mpsc::channel();
    buffer.map_async(wgpu::MapMode::Read, .., move |result| {
        let _ = sender.send(result);
    });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|err| Error::ReadFrame(err.into()))?;
    receiver
        .recv()
        .expect("map callback runs during poll")
        .map_err(|err| Error::ReadFrame(err.into()))?;

    let mapped = buffer
        .get_mapped_range(..)
        .map_err(|err| Error::ReadFrame(err.into()))?;
    let pixels = mapped
        .chunks_exact(padded_row_bytes as usize)
        .flat_map(|row| &row[..row_bytes as usize])
        .copied()
        .collect();
    Ok(Image {
        width,
        height,
        pixels,
    })
}

/// A rendered frame: tightly packed 8-bit RGBA pixels in sRGB, row by row from the top-left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Image {
    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }

    /// Saves the image as a PNG file.
    pub fn save_png(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        image::save_buffer_with_format(
            path,
            &self.pixels,
            self.width,
            self.height,
            image::ExtendedColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .map_err(|source| Error::SaveImage {
            path: path.to_owned(),
            source,
        })
    }
}
