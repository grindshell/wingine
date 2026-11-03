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
    input::{Axis, GamepadButton, Key, MouseButton, Stick},
    save::{Saves, TemporaryFolder},
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
        #[cfg_attr(not(feature = "egui"), expect(unused_mut))]
        let mut ctx = Context::new(gpu, renderer, Audio::silent());
        #[cfg(feature = "egui")]
        ctx.gui.set_predictable();
        Ok(Self {
            ctx,
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

    /// Renders the recorded canvas, and any interface, onto an `output`-sized frame and reads it
    /// back.
    fn capture(&mut self, output: [u32; 2]) -> Result<Image> {
        let Context {
            gpu,
            renderer,
            #[cfg(feature = "egui")]
            gui,
            ..
        } = &mut self.ctx;
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
        #[cfg(feature = "egui")]
        gui.render(&gpu.device, &gpu.queue, &texture);
        read_texture(gpu, &texture)
    }

    /// Renders the recorded canvas, and any interface, onto an `output`-sized frame, as a window
    /// would, then waits for the GPU to finish it. Returns the seconds rendering took on the CPU.
    fn render_frame(&mut self, output: [u32; 2]) -> f32 {
        let Context {
            gpu,
            renderer,
            #[cfg(feature = "egui")]
            gui,
            ..
        } = &mut self.ctx;
        let fits = |(texture, _): &(wgpu::Texture, wgpu::TextureView)| {
            [texture.width(), texture.height()] == output
        };
        if !self.target.as_ref().is_some_and(fits) {
            let texture = create_frame(gpu, output);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            self.target = Some((texture, view));
        }
        let (texture, view) = self.target.as_ref().expect("created above");
        let start = Instant::now();
        self.screen.render(
            &gpu.device,
            &gpu.queue,
            renderer,
            &self.canvas,
            view,
            output,
        );
        #[cfg(feature = "egui")]
        gui.render(&gpu.device, &gpu.queue, texture);
        #[cfg(not(feature = "egui"))]
        let _ = texture;
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
        // egui renders through a view without sRGB encoding, since it blends in sRGB space.
        view_formats: &[FORMAT.remove_srgb_suffix()],
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
    /// Input for the game's interface, gathered until the next frame.
    #[cfg(feature = "egui")]
    gui_input: crate::gui::SimulatedInput,
    /// Where the game's saves go, unless its config gave a folder. Declared after `game`, so the
    /// game has finished with it first.
    _saves: Option<TemporaryFolder>,
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
    /// The game's [saves](crate::save) are kept in `config.save_folder`, or without one, in a
    /// temporary folder of the simulation's own, which starts empty and is removed with it.
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
        let temporary = match &config.save_folder {
            Some(folder) => {
                headless.ctx.saves = Saves::in_folder(folder);
                None
            }
            None => {
                let folder = TemporaryFolder::new();
                headless.ctx.saves = Saves::in_folder(folder.path());
                Some(folder)
            }
        };
        headless.set_resolution(config.resolution);
        let output = [config.width, config.height];
        headless.ctx.canvas_size = headless.screen.canvas_size(output, 1.0);
        #[cfg(feature = "egui")]
        {
            headless.ctx.gui.debug_key = config.debug_key;
            headless.ctx.gui.enable_labels();
        }
        let game = init(&mut headless.ctx)?;
        Ok(Self {
            headless,
            game,
            timestep,
            output,
            drawn: false,
            render_every_frame: false,
            frames: 0,
            #[cfg(feature = "egui")]
            gui_input: Default::default(),
            _saves: temporary,
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

    /// Whether the game has called [`Context::quit`], which would have closed its window.
    pub fn has_quit(&self) -> bool {
        self.headless.ctx.quit
    }

    /// Presses the window's close button, as [`Game::close_requested`] hears it, and tells
    /// whether the window would close.
    pub fn request_close(&mut self) -> bool {
        self.wake();
        self.game.close_requested(&mut self.headless.ctx)
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

    /// For a game that [redraws on demand](crate::Redraw::OnDemand), how long its window would
    /// wait before its next frame, in seconds: 0 if it wants one now, as after input, or `None`
    /// if it waits for input or a [`Waker`](crate::Waker). [`Simulation::step`] runs a frame
    /// whatever this says.
    ///
    /// The wait is in real time, while delays asked for in a frame are in game time, which a
    /// simulation runs through faster: a frame asked for a second from now is a second away in
    /// real time, however many frames run before it.
    pub fn next_frame_in(&self) -> Option<f32> {
        let due = self.headless.ctx.redraws.due()?;
        Some(due.saturating_duration_since(Instant::now()).as_secs_f32())
    }

    /// Asks for a frame, as input does in a window that redraws on demand.
    fn wake(&self) {
        self.headless.ctx.waker().wake();
    }

    /// Runs one frame: updates the game, runs its interface, then draws it.
    pub fn step(&mut self) {
        let start = Instant::now();
        self.headless.ctx.redraws.begin_frame();
        #[cfg(feature = "egui")]
        self.set_gui_input(true);
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

    /// Presses `key`. The game sees it unless the interface wants it, as a text field with focus
    /// does, or it's the [debug key](WindowConfig::debug_key).
    pub fn key_down(&mut self, key: Key) {
        self.wake();
        #[cfg(feature = "egui")]
        {
            let gui = &mut self.headless.ctx.gui;
            if gui.debug_key_down(key) {
                return;
            }
            self.gui_input.key(key, true);
            if gui.wants_keyboard() {
                return;
            }
        }
        self.headless.ctx.input.key_down(key);
    }

    pub fn key_up(&mut self, key: Key) {
        self.wake();
        #[cfg(feature = "egui")]
        self.gui_input.key(key, false);
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
        self.wake();
        self.headless.ctx.input.set_mouse_position(Some([x, y]));
        #[cfg(feature = "egui")]
        {
            let [x, y] = self.headless.screen.to_output([x, y], self.output, 1.0);
            let pixels_per_point = self.headless.ctx.gui.egui.pixels_per_point();
            self.gui_input
                .move_pointer(egui::pos2(x as f32, y as f32) / pixels_per_point);
        }
    }

    /// Presses `button` where the cursor is. The game sees it unless the interface wants it, as
    /// when the cursor is over one of its windows.
    pub fn mouse_down(&mut self, button: MouseButton) {
        self.wake();
        #[cfg(feature = "egui")]
        {
            self.gui_input.button(button, true);
            let gui = &self.headless.ctx.gui;
            if self
                .gui_input
                .pointer()
                .is_some_and(|pos| gui.wants_pointer_at(pos))
            {
                return;
            }
        }
        self.headless.ctx.input.mouse_down(button);
    }

    pub fn mouse_up(&mut self, button: MouseButton) {
        self.wake();
        #[cfg(feature = "egui")]
        self.gui_input.button(button, false);
        self.headless.ctx.input.mouse_up(button);
    }

    /// Connects a new gamepad called `name`, and returns its number: 1 for the first, and one
    /// more for each after.
    pub fn connect_gamepad(&mut self, name: &str) -> u32 {
        self.wake();
        let input = &mut self.headless.ctx.input;
        let number = input
            .matching_gamepads(None)
            .count()
            .try_into()
            .map_or(u32::MAX, |count: u32| count + 1);
        input.connect_gamepad(number, name.to_owned());
        number
    }

    /// Disconnects gamepad `number`, releasing everything held on it.
    pub fn disconnect_gamepad(&mut self, gamepad: u32) {
        self.wake();
        self.headless.ctx.input.disconnect_gamepad(gamepad);
    }

    /// Pushes `button` on `gamepad` all the way. A stick's direction, such as
    /// [`GamepadButton::LeftStickLeft`], pushes the stick that way.
    pub fn button_down(&mut self, gamepad: u32, button: GamepadButton) {
        self.push_button(gamepad, button, 1.0);
    }

    pub fn button_up(&mut self, gamepad: u32, button: GamepadButton) {
        self.push_button(gamepad, button, 0.0);
    }

    /// Pushes `button` on `gamepad` `strength` of the way, from 0 to 1, as a trigger can be. It's
    /// held from halfway, until it falls back below 0.4.
    pub fn push_button(&mut self, gamepad: u32, button: GamepadButton, strength: f32) {
        self.wake();
        self.headless
            .ctx
            .input
            .set_gamepad_button(gamepad, button, strength);
    }

    /// Pushes `button` on `gamepad`, runs one frame, and releases it, as [`Simulation::tap`]
    /// does with a key.
    pub fn tap_button(&mut self, gamepad: u32, button: GamepadButton) {
        self.button_down(gamepad, button);
        self.step();
        self.button_up(gamepad, button);
    }

    /// Holds `button` on `gamepad` for `seconds` of game time, rounded to whole frames.
    pub fn hold_button(&mut self, gamepad: u32, button: GamepadButton, seconds: f32) {
        self.hold_button_frames(gamepad, button, Self::frames_in(seconds));
    }

    /// Holds `button` on `gamepad` for `frames` frames.
    pub fn hold_button_frames(&mut self, gamepad: u32, button: GamepadButton, frames: u32) {
        self.button_down(gamepad, button);
        self.run_frames(frames);
        self.button_up(gamepad, button);
    }

    /// Moves `stick` on `gamepad` to `x`, `y`, each from -1 to 1, with `y` down: `0, 0` is its
    /// middle and `-1, 0` all the way left. The game sees it from a dead zone around the middle.
    pub fn move_stick(&mut self, gamepad: u32, stick: Stick, x: f32, y: f32) {
        self.wake();
        let input = &mut self.headless.ctx.input;
        input.set_gamepad_axis(gamepad, stick, Axis::X, x);
        input.set_gamepad_axis(gamepad, stick, Axis::Y, y);
    }

    /// Types `text` into the interface, as into whichever text field has focus. The game's own
    /// input doesn't see typing.
    #[cfg(feature = "egui")]
    pub fn type_text(&mut self, text: &str) {
        self.wake();
        self.gui_input.text(text);
    }

    /// Turns the mouse wheel by `x`, `y` points of the interface, where it scrolls whatever is
    /// under the cursor. Positive `y` scrolls up, as turning the wheel away does.
    #[cfg(feature = "egui")]
    pub fn scroll(&mut self, x: f32, y: f32) {
        self.wake();
        self.gui_input.scroll(egui::vec2(x, y));
    }

    /// Where the widgets of the interface labelled `label` were in the last frame, in canvas
    /// pixels, row by row from the top, and left to right along a row. A widget's label is its
    /// text, such as a
    /// button's, what it's [labelled by](egui::Response::labelled_by), or a text field's hint.
    /// Click the middle of one to press it.
    #[cfg(feature = "egui")]
    pub fn gui_rects(&self, label: &str) -> Vec<crate::math::Rect> {
        let pixels_per_point = self.headless.ctx.gui.egui.pixels_per_point();
        let to_canvas = |pos: egui::Pos2| {
            let pos = pos * pixels_per_point;
            self.headless
                .screen
                .to_canvas([pos.x.into(), pos.y.into()], self.output, 1.0)
        };
        self.headless
            .ctx
            .gui
            .labelled(label)
            .into_iter()
            .map(|rect| crate::gui::rect_to_canvas(rect, to_canvas))
            .collect()
    }

    /// The first of [`Simulation::gui_rects`], if there's any.
    #[cfg(feature = "egui")]
    pub fn gui_rect(&self, label: &str) -> Option<crate::math::Rect> {
        self.gui_rects(label).into_iter().next()
    }

    /// Clicks the middle of the first widget labelled `label` with the left button, as
    /// [`Simulation::click`] does, then runs another frame, in which egui sees the release that
    /// completes the click: a button has been pressed when this returns.
    ///
    /// # Panics
    ///
    /// If the last frame laid out no such widget.
    #[cfg(feature = "egui")]
    pub fn click_gui(&mut self, label: &str) {
        let Some(rect) = self.gui_rect(label) else {
            panic!("the interface has no widget labelled `{label}`");
        };
        self.click(
            MouseButton::Left,
            rect.x + rect.w / 2.0,
            rect.y + rect.h / 2.0,
        );
        self.step();
    }

    /// Gives the interface its input for the next pass: the screen, the time, and if `events`,
    /// the input gathered since the last frame.
    #[cfg(feature = "egui")]
    fn set_gui_input(&mut self, events: bool) {
        let gui = &mut self.headless.ctx.gui;
        let pixels_per_point = gui.egui.pixels_per_point();
        let size = egui::vec2(self.output[0] as f32, self.output[1] as f32) / pixels_per_point;
        let time = self.frames as f64 * f64::from(Self::FRAME_TIME);
        let max_texture_side = self
            .headless
            .ctx
            .gpu
            .device
            .limits()
            .max_texture_dimension_2d as usize;
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            max_texture_side: Some(max_texture_side),
            time: Some(time),
            ..Default::default()
        };
        if events {
            input.events = self.gui_input.take_events();
        }
        gui.set_input(input);
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
        self.wake();
        self.output = [width, height];
        self.drawn = false;
    }

    /// Renders the latest frame as the window would show it. Before the first frame, or after a
    /// resize, the game draws again first, without updating.
    pub fn screenshot(&mut self) -> Result<Image> {
        if !self.drawn {
            #[cfg(feature = "egui")]
            {
                // Laid out again for the current size, keeping the input for the next frame.
                self.set_gui_input(false);
                crate::gui::run_pass(&mut self.headless.ctx, &mut self.game);
            }
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
