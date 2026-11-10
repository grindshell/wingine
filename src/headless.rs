//! Rendering and running games without a window: for screenshot tests, for testing game behavior
//! with simulated input, and for offline rendering.

use std::{
    collections::{HashMap, HashSet},
    path::Path,
    sync::{Condvar, Mutex, PoisonError, mpsc},
    thread::{self, ThreadId},
};

use web_time::Instant;

use crate::{
    Context, Error, Game, Result, WindowConfig,
    app::{Timestep, run_frame},
    audio::Audio,
    graphics::{Canvas, Resolution, gpu::Gpu, renderer::Renderer},
    input::{Axis, Control, GamepadButton, Key, MouseButton, Stick},
    lua::mlua::FromLuaMulti,
    runtime::LuauGame,
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
            canvas: Canvas::new(),
            target: None,
            _lease: lease,
        })
    }

    /// Sets the canvas size for later frames, as [`WindowConfig::resolution`] does for a window.
    /// [`Context::set_scaling`] scales a fixed one.
    ///
    /// [`WindowConfig::resolution`]: crate::WindowConfig::resolution
    ///
    /// # Panics
    ///
    /// If it's a fixed resolution smaller than 1x1, or larger than the GPU renders.
    pub fn set_resolution(&mut self, resolution: Resolution) {
        if let Some(problem) = resolution.problem(&self.ctx.gpu.device) {
            panic!("{problem}");
        }
        self.ctx.screen.set_resolution(resolution);
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
        let [width, height] = self.ctx.screen.canvas_size(output, 1.0);
        self.canvas.begin(width, height);
        draw(&mut self.canvas);
    }

    /// Renders the recorded canvas, and any interface, onto an `output`-sized frame and reads it
    /// back.
    fn capture(&mut self, output: [u32; 2]) -> Result<Image> {
        let Context {
            gpu,
            renderer,
            screen,
            #[cfg(feature = "egui")]
            gui,
            ..
        } = &mut self.ctx;
        let texture = create_frame(gpu, output);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        screen.render(
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
            screen,
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
        screen.render(
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

/// How a [`Simulation`] repeats a key held down, as a window's system repeats keys by the player's
/// settings: [`Simulation::set_key_repeat`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyRepeat {
    /// Seconds of game time from a key's press to its first repeat.
    pub delay: f32,
    /// Repeats a second after the first.
    pub rate: f32,
}

impl KeyRepeat {
    /// The delay in frames, which run 60 a second, so it needn't be whole.
    fn delay_frames(self) -> f64 {
        f64::from(self.delay) * 60.0
    }

    /// The frames from one repeat to the next, which needn't be whole.
    fn interval_frames(self) -> f64 {
        60.0 / f64::from(self.rate)
    }
}

impl Default for KeyRepeat {
    /// Half a second's delay, then 30 repeats a second, about as Windows and GNOME repeat keys
    /// unless told otherwise.
    fn default() -> Self {
        Self {
            delay: 0.5,
            rate: 30.0,
        }
    }
}

/// Runs a [`Game`] without a window, driven by simulated input: for tests of game behavior, and
/// for scripted runs (see the `WINGINE_SCRIPT` variable in [`run`](crate::run)).
///
/// Frames only advance when asked to, each by exactly [`Simulation::FRAME_TIME`], so runs are
/// deterministic. With an [update rate](WindowConfig::update_rate), each frame runs the updates
/// that time holds, as a window would. Input takes effect on the next frame, as events from a
/// window would: for example, a key held down is pressed on the next frame and held until
/// released. Keys held down repeat as a window's system would repeat them, at the delay and rate
/// [`Simulation::set_key_repeat`] sets. A game's actions can be pressed by name, through what
/// they're bound to ([`Simulation::action_down`]), and a Luau game's state set up and read with
/// Luau ([`Simulation::exec`] and [`Simulation::eval`]).
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
    /// How keys held down repeat, if they do.
    key_repeat: Option<KeyRepeat>,
    /// The key that repeats, the last pressed, while it's held, and the frame it repeats next,
    /// counted as `frames` counts them: infinite while keys don't repeat. A repeat due between two
    /// frames comes in the later.
    repeating: Option<(Key, f64)>,
    /// The keys held down, whether the game or the interface has them.
    keys_held: HashSet<Key>,
    /// What `action_down` pressed for each action it holds down.
    actions_held: HashMap<String, Pressed>,
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
        if let Some(problem) = config.resolution.problem(&headless.ctx.gpu.device) {
            return Err(Error::WindowConfig(problem));
        }
        headless.set_resolution(config.resolution);
        headless.ctx.set_scaling(config.scaling);
        // Noted, as the game sets it: frames keep the window's size.
        headless.ctx.fullscreen = config.fullscreen;
        let output = [config.width, config.height];
        headless.ctx.canvas_size = headless.ctx.screen.canvas_size(output, 1.0);
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
            key_repeat: Some(KeyRepeat::default()),
            repeating: None,
            keys_held: HashSet::new(),
            actions_held: HashMap::new(),
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

    /// The game and the context at once, for lending the context to the game's scripts.
    pub(crate) fn parts(&mut self) -> (&mut G, &mut Context) {
        (&mut self.game, &mut self.headless.ctx)
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

    /// How keys held down repeat from now on, or with `None`, that they don't. As a window's
    /// system does, only the last key pressed repeats, and only until it's released, even if
    /// others are still held. A key held now waits the delay from now. By default, keys repeat as
    /// [`KeyRepeat::default`] says.
    ///
    /// # Panics
    ///
    /// If the delay is below 0, or the rate isn't a finite number above 0.
    pub fn set_key_repeat(&mut self, repeat: Option<KeyRepeat>) {
        if let Some(KeyRepeat { delay, rate }) = repeat {
            assert!(
                delay >= 0.0,
                "keys can't repeat {delay} seconds after they're pressed"
            );
            assert!(
                rate > 0.0 && rate.is_finite(),
                "keys can't repeat {rate} times a second"
            );
        }
        self.key_repeat = repeat;
        let first = self.first_repeat();
        if let Some((_, next)) = &mut self.repeating {
            *next = first;
        }
    }

    /// The frame a key pressed now first repeats in, counted as `frames` counts them.
    fn first_repeat(&self) -> f64 {
        self.key_repeat.map_or(f64::INFINITY, |repeat| {
            self.frames as f64 + repeat.delay_frames()
        })
    }

    /// Repeats the key that repeats as many times as it's due to by this frame.
    fn repeat_key(&mut self) {
        let (Some(repeat), Some((key, next))) = (self.key_repeat, &mut self.repeating) else {
            return;
        };
        let key = *key;
        let interval = repeat.interval_frames();
        // A repeat due this little after a frame comes in it, so that rounding doesn't put
        // repeats at rates that divide 60 a frame late.
        let now = self.frames as f64 + 1e-3;
        let mut repeats = 0;
        while *next <= now {
            *next += interval;
            repeats += 1;
        }
        for _ in 0..repeats {
            self.send_key_down(key, true);
        }
    }

    /// Runs one frame: updates the game, runs its interface, then draws it.
    pub fn step(&mut self) {
        let start = Instant::now();
        self.headless.ctx.redraws.begin_frame();
        self.repeat_key();
        #[cfg(feature = "egui")]
        self.set_gui_input(true);
        let Self {
            headless,
            game,
            timestep,
            output,
            ..
        } = self;
        let canvas_size = headless.ctx.screen.canvas_size(*output, 1.0);
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

    /// Presses `key`, which then [repeats](Simulation::set_key_repeat) until it's released or
    /// another key is pressed. The game sees it unless the interface wants it, as a text field
    /// with focus does, or it's the [debug key](WindowConfig::debug_key), which, as in a window,
    /// does nothing while the interface wants it.
    ///
    /// The key, and each repeat, types what it would on a US keyboard, with Shift if it's held,
    /// unless Control or Alt is: [`Simulation::type_text`] types other layouts' characters.
    pub fn key_down(&mut self, key: Key) {
        self.wake();
        self.keys_held.insert(key);
        if self.repeating.is_none_or(|(repeating, _)| repeating != key) {
            self.repeating = Some((key, self.first_repeat()));
        }
        self.send_key_down(key, false);
    }

    /// Passes a press of `key`, or a repeat, and what it types, to the interface, and to the
    /// game unless the interface wants it or it's the debug key.
    fn send_key_down(&mut self, key: Key, repeat: bool) {
        let held = |keys: [Key; 2]| keys.iter().any(|key| self.keys_held.contains(key));
        let shift = held([Key::LeftShift, Key::RightShift]);
        let shortcut =
            held([Key::LeftControl, Key::RightControl]) || held([Key::LeftAlt, Key::RightAlt]);
        let text = key.us_text(shift).filter(|_| !shortcut);
        let mut buffer = [0; 4];
        let text = text.map(|c| &*c.encode_utf8(&mut buffer));
        #[cfg(feature = "egui")]
        {
            let gui = &mut self.headless.ctx.gui;
            if gui.hears_key(key) {
                self.gui_input.key(key, true, repeat);
                if let Some(text) = text {
                    self.gui_input.text(text);
                }
            }
            if gui.wants_keyboard() {
                return;
            }
            // Repeats of the debug key do nothing, as in a window.
            let debug_key = if repeat {
                gui.debug_key == Some(key)
            } else {
                gui.debug_key_down(key)
            };
            if debug_key {
                return;
            }
        }
        let input = &mut self.headless.ctx.input;
        if repeat {
            input.key_repeat(key);
        } else {
            input.key_down(key);
        }
        if let Some(text) = text {
            input.type_text(text);
        }
    }

    pub fn key_up(&mut self, key: Key) {
        self.wake();
        self.keys_held.remove(&key);
        if self
            .repeating
            .is_some_and(|(repeating, _)| repeating == key)
        {
            self.repeating = None;
        }
        #[cfg(feature = "egui")]
        if self.headless.ctx.gui.hears_key(key) {
            self.gui_input.key(key, false, false);
        }
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
            let [x, y] = self.headless.ctx.screen.to_output([x, y], self.output, 1.0);
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

    /// Presses what `action` is bound to, as the player would: its first key, as
    /// [`Simulation::key_down`] does, or if it has none, its first gamepad button, as
    /// [`Simulation::button_down`] does, on the gamepad it's bound to, or if it's bound to any, the
    /// first connected. So a test presses what the game binds, wherever the game binds it.
    ///
    /// # Panics
    ///
    /// If `action` isn't bound to a key, nor to a button of a connected gamepad.
    pub fn action_down(&mut self, action: &str) {
        if let Err(message) = self.try_action_down(action) {
            panic!("{message}");
        }
    }

    /// Presses what `action` is bound to, as [`Simulation::action_down`] does, or says why it
    /// can't.
    pub(crate) fn try_action_down(&mut self, action: &str) -> std::result::Result<(), String> {
        let pressed = self.action_control(action)?;
        match pressed {
            Pressed::Key(key) => self.key_down(key),
            Pressed::Button(gamepad, button) => self.button_down(gamepad, button),
        }
        self.actions_held.insert(action.to_owned(), pressed);
        Ok(())
    }

    /// Releases what [`Simulation::action_down`] pressed for `action`, if it's holding it down,
    /// even if the game has bound it to something else since.
    pub fn action_up(&mut self, action: &str) {
        match self.actions_held.remove(action) {
            Some(Pressed::Key(key)) => self.key_up(key),
            Some(Pressed::Button(gamepad, button)) => self.button_up(gamepad, button),
            None => {}
        }
    }

    /// Presses `action`, runs one frame, and releases it, as [`Simulation::tap`] does with a key.
    pub fn tap_action(&mut self, action: &str) {
        self.action_down(action);
        self.step();
        self.action_up(action);
    }

    /// Holds `action` down for `seconds` of game time, rounded to whole frames.
    pub fn hold_action(&mut self, action: &str, seconds: f32) {
        self.hold_action_frames(action, Self::frames_in(seconds));
    }

    /// Holds `action` down for `frames` frames.
    pub fn hold_action_frames(&mut self, action: &str, frames: u32) {
        self.action_down(action);
        self.run_frames(frames);
        self.action_up(action);
    }

    /// What [`Simulation::action_down`] presses for `action`, or why it can't.
    fn action_control(&self, action: &str) -> std::result::Result<Pressed, String> {
        let input = &self.headless.ctx.input;
        let unbound = || format!("`{action}` isn't bound to anything");
        let controls = input.bindings(action);
        let key = controls.iter().find_map(|&control| match control {
            Control::Key(key) => Some(key),
            Control::Button(_) => None,
        });
        if let Some(key) = key {
            return Ok(Pressed::Key(key));
        }
        let button = controls.iter().find_map(|&control| match control {
            Control::Key(_) => None,
            Control::Button(button) => Some(button),
        });
        let button = button.ok_or_else(unbound)?;
        let gamepad = input.bound_gamepad(action);
        let connected = match gamepad {
            Some(gamepad) => input
                .gamepad(gamepad)
                .is_some_and(|pad| pad.is_connected())
                .then_some(gamepad),
            None => input.gamepads().next().map(|(number, _)| number),
        };
        match (connected, gamepad) {
            (Some(number), _) => Ok(Pressed::Button(number, button)),
            (None, Some(gamepad)) => Err(format!(
                "`{action}` is bound only to gamepad {gamepad}'s buttons, and it isn't connected"
            )),
            (None, None) => Err(format!(
                "`{action}` is bound only to gamepads' buttons, and none is connected: connect one \
                 with `connect_gamepad`"
            )),
        }
    }

    /// Moves `stick` on `gamepad` to `x`, `y`, each from -1 to 1, with `y` down: `0, 0` is its
    /// middle and `-1, 0` all the way left. The game sees it from a dead zone around the middle.
    pub fn move_stick(&mut self, gamepad: u32, stick: Stick, x: f32, y: f32) {
        self.wake();
        let input = &mut self.headless.ctx.input;
        input.set_gamepad_axis(gamepad, stick, Axis::X, x);
        input.set_gamepad_axis(gamepad, stick, Axis::Y, y);
    }

    /// Types `text`, as a keyboard of any layout or an input method would, without pressing any
    /// keys: into the interface if it wants the keyboard, as a text field with focus does, and
    /// otherwise for the game, as [`Input::text`](crate::input::Input::text) tells. Its control
    /// characters aren't text the game sees.
    pub fn type_text(&mut self, text: &str) {
        self.wake();
        #[cfg(feature = "egui")]
        {
            self.gui_input.text(text);
            if self.headless.ctx.gui.wants_keyboard() {
                return;
            }
        }
        self.headless.ctx.input.type_text(text);
    }

    /// Has the input method compose `text`, as [`Input::composition`] tells, while the game takes
    /// one and the interface doesn't want the keyboard. [`Simulation::type_text`] then types what
    /// it becomes, and `compose("")` stops it, as when the player deletes it all.
    ///
    /// [`Input::composition`]: crate::input::Input::composition
    pub fn compose(&mut self, text: &str) {
        self.wake();
        #[cfg(feature = "egui")]
        if self.headless.ctx.gui.wants_keyboard() {
            return;
        }
        self.headless.ctx.input.compose(text);
    }

    /// Turns the mouse wheel by `x`, `y` notches where the cursor is, as
    /// [`Input::wheel`](crate::input::Input::wheel) has them: positive `y` turns it away from the
    /// player, as to scroll up, and positive `x` tilts it right. The game sees it unless the
    /// interface wants it, as when the cursor is over one of its windows, which scrolls what's
    /// under the cursor.
    pub fn scroll(&mut self, x: f32, y: f32) {
        self.wake();
        #[cfg(feature = "egui")]
        {
            // egui has winit's way round: positive `x` is tilting left.
            self.gui_input.scroll_lines(egui::vec2(-x, y));
            let gui = &self.headless.ctx.gui;
            if self
                .gui_input
                .pointer()
                .is_some_and(|pos| gui.wants_pointer_at(pos))
            {
                return;
            }
        }
        self.headless.ctx.input.turn_wheel([x, y]);
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
                .ctx
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

/// Luau games' state, for tests to set up and look at.
///
/// Each call runs a chunk of its own, between frames, lent the context as `update` is, so the
/// engine's functions work in it as they do there: loading a level, playing a sound, reading
/// input. Its globals are the game's, and it `require`s modules by their paths from the game's
/// folder, as `main.luau` does, getting the same ones the game has, such as
/// `require("./sim/rng")`. Since each is a chunk of its own, it sees fields of the game's globals
/// as they are now, even those a chunk before replaced (a chunk reads the fields of globals it
/// names as it loads).
impl Simulation<LuauGame> {
    /// Runs the Luau `code`, such as to set up a state.
    ///
    /// # Panics
    ///
    /// If it fails, with its error.
    pub fn exec(&mut self, code: &str) {
        if let Err(err) = self.try_exec(code) {
            panic!("`{code}`: {err}");
        }
    }

    /// The value of the Luau `expression`, such as `"pong.ball.vx"`, converted to `R`.
    ///
    /// # Panics
    ///
    /// If it fails, or its value doesn't convert, with its error.
    pub fn eval<R: FromLuaMulti>(&mut self, expression: &str) -> R {
        self.try_eval(expression)
            .unwrap_or_else(|err| panic!("`{expression}`: {err}"))
    }

    /// Runs the Luau `code`, as [`Simulation::exec`] does, returning its error if it fails, for
    /// a test that expects it to.
    pub fn try_exec(&mut self, code: &str) -> Result<()> {
        self.game.run_chunk(&mut self.headless.ctx, code)
    }

    /// The value of the Luau `expression`, as [`Simulation::eval`] gives it, or the error
    /// evaluating it fails with.
    pub fn try_eval<R: FromLuaMulti>(&mut self, expression: &str) -> Result<R> {
        let source = format!("return {expression}");
        self.game.run_chunk(&mut self.headless.ctx, &source)
    }

    /// Loads the game again from its folder, in its place, as a game watching its folder does
    /// when a file changes: see [`LuauGame::reload`]. An error loading it stops it, which
    /// [`LuauGame::error`] tells.
    pub fn reload(&mut self) {
        self.game.reload(&mut self.headless.ctx);
    }
}

/// A control a [`Simulation`] pressed for an action.
#[derive(Clone, Copy, Debug)]
enum Pressed {
    Key(Key),
    /// A button of a gamepad, by its number.
    Button(u32, GamepadButton),
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
///
/// Frames compare with [`Image::compare`], against each other or against PNGs saved before and
/// loaded with [`Image::load_png`], as screenshot tests do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Image {
    /// An image of `pixels`, laid out as [`Image::pixels`] returns them, or `None` if there aren't
    /// `width * height * 4` bytes of them.
    pub fn from_pixels(width: u32, height: u32, pixels: Vec<u8>) -> Option<Self> {
        let len = u64::from(width) * u64::from(height) * 4;
        (pixels.len() as u64 == len).then_some(Self {
            width,
            height,
            pixels,
        })
    }

    /// Loads a PNG file, converting its pixels to 8-bit RGBA whatever it holds, so two files of
    /// the same picture load the same.
    pub fn load_png(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let image = image::open(path)
            .map_err(|source| Error::LoadImage {
                path: path.to_owned(),
                source,
            })?
            .into_rgba8();
        let (width, height) = image.dimensions();
        Ok(Self {
            width,
            height,
            pixels: image.into_raw(),
        })
    }

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

    /// The pixel at `x`, `y`, counting from 0 at the top-left: its red, green, blue and alpha.
    ///
    /// # Panics
    ///
    /// If it's outside the image.
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        assert!(
            x < self.width && y < self.height,
            "{x}, {y} is outside a {}x{} image",
            self.width,
            self.height
        );
        let i = (y as usize * self.width as usize + x as usize) * 4;
        self.pixels[i..i + 4].try_into().unwrap()
    }

    /// How this image differs from `other`: how many pixels differ by more than `tolerance` in
    /// any channel, and the most any channel differs by. A `tolerance` of 0 compares exactly; a
    /// small one lets frames rendered on different GPUs match, since they round blending a little
    /// differently (the engine's own screenshot tests allow 2).
    pub fn compare(&self, other: &Image, tolerance: u8) -> Comparison {
        if (self.width, self.height) != (other.width, other.height) {
            return Comparison::DifferentSizes;
        }
        let mut differing = 0;
        let mut largest = 0;
        for (a, b) in self
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(other.pixels.as_chunks::<4>().0)
        {
            let difference = channel_difference(a, b);
            largest = largest.max(difference);
            differing += usize::from(difference > tolerance);
        }
        Comparison::SameSize { differing, largest }
    }

    /// This image dimmed to a third, with the pixels that differ from `other` by more than
    /// `tolerance` (as [`Image::compare`] counts them) in magenta, to show where they are, or
    /// `None` if the images are different sizes.
    pub fn diff(&self, other: &Image, tolerance: u8) -> Option<Image> {
        if (self.width, self.height) != (other.width, other.height) {
            return None;
        }
        let pixels = self
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .zip(other.pixels.as_chunks::<4>().0)
            .flat_map(|(a, b)| {
                if channel_difference(a, b) > tolerance {
                    [255, 0, 255, 255]
                } else {
                    [a[0] / 3, a[1] / 3, a[2] / 3, 255]
                }
            })
            .collect();
        Some(Image { pixels, ..*self })
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

/// How two images compare, from [`Image::compare`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Comparison {
    /// The images are the same size. `differing` of their pixels differ by more than the
    /// tolerance in some channel, and `largest` is the most any channel of any pixel differs by,
    /// within the tolerance or not.
    SameSize { differing: usize, largest: u8 },
    /// The images are different sizes, so their pixels don't compare.
    DifferentSizes,
}

impl Comparison {
    /// Whether the images are the same size and at most `pixels` of their pixels differ: 0 for
    /// images that match, or a few for scenes where GPUs disagree about some pixels, such as
    /// textures in perspective.
    pub fn differs_by_at_most(&self, pixels: usize) -> bool {
        matches!(*self, Self::SameSize { differing, .. } if differing <= pixels)
    }
}

/// The most any channel of two pixels differs by.
fn channel_difference(a: &[u8; 4], b: &[u8; 4]) -> u8 {
    a.iter()
        .zip(b)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GRAY: [u8; 4] = [90, 90, 90, 255];

    /// A 3x2 image of gray.
    fn gray() -> Image {
        Image::from_pixels(3, 2, GRAY.repeat(6)).unwrap()
    }

    /// `image` with the pixel at `x`, `y` changed to `pixel`.
    fn with_pixel(mut image: Image, x: u32, y: u32, pixel: [u8; 4]) -> Image {
        let i = ((y * image.width + x) * 4) as usize;
        image.pixels[i..i + 4].copy_from_slice(&pixel);
        image
    }

    #[test]
    fn images_hold_width_by_height_pixels() {
        assert!(Image::from_pixels(3, 2, vec![0; 23]).is_none());
        assert!(Image::from_pixels(3, 2, vec![0; 25]).is_none());
        let image = with_pixel(gray(), 2, 1, [1, 2, 3, 4]);
        assert_eq!(image.pixel(2, 1), [1, 2, 3, 4]);
        assert_eq!(image.pixel(1, 1), GRAY);
    }

    #[test]
    #[should_panic(expected = "3, 0 is outside a 3x2 image")]
    fn pixels_outside_the_image_panic() {
        gray().pixel(3, 0);
    }

    #[test]
    fn comparing_counts_pixels_beyond_the_tolerance() {
        let image = gray();
        let same = image.compare(&gray(), 0);
        assert_eq!(
            same,
            Comparison::SameSize {
                differing: 0,
                largest: 0
            }
        );
        assert!(same.differs_by_at_most(0));

        let nudged = with_pixel(gray(), 0, 0, [92, 90, 90, 255]);
        let moved = with_pixel(nudged.clone(), 1, 1, [90, 90, 90, 0]);
        let exact = image.compare(&moved, 0);
        assert_eq!(
            exact,
            Comparison::SameSize {
                differing: 2,
                largest: 255
            }
        );
        assert!(!exact.differs_by_at_most(1) && exact.differs_by_at_most(2));
        assert_eq!(
            image.compare(&moved, 2),
            Comparison::SameSize {
                differing: 1,
                largest: 255
            },
            "the nudged pixel is within it"
        );
        assert_eq!(
            image.compare(&nudged, 2),
            Comparison::SameSize {
                differing: 0,
                largest: 2
            },
            "the largest difference counts those within it"
        );

        let wider = Image::from_pixels(4, 2, GRAY.repeat(8)).unwrap();
        assert_eq!(image.compare(&wider, 255), Comparison::DifferentSizes);
        assert!(!image.compare(&wider, 255).differs_by_at_most(usize::MAX));
    }

    #[test]
    fn diffs_show_the_differing_pixels_in_magenta() {
        let moved = with_pixel(gray(), 2, 0, [200, 90, 90, 255]);
        let diff = moved.diff(&gray(), 2).unwrap();
        assert_eq!(diff.pixel(2, 0), [255, 0, 255, 255]);
        assert_eq!(diff.pixel(0, 0), [30, 30, 30, 255], "the rest is dimmed");
        let wider = Image::from_pixels(4, 2, GRAY.repeat(8)).unwrap();
        assert!(moved.diff(&wider, 0).is_none());
    }

    #[test]
    fn pngs_load_as_rgba_whatever_they_hold() {
        let folder = std::env::temp_dir().join(format!("wingine-load-png-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let opaque = folder.join("opaque.png");
        // Red, green and blue without alpha.
        image::save_buffer(&opaque, &[90; 18], 3, 2, image::ExtendedColorType::Rgb8).unwrap();
        let loaded = Image::load_png(&opaque).unwrap();
        assert_eq!(loaded, gray());
        let saved = folder.join("saved.png");
        loaded.save_png(&saved).unwrap();
        assert_eq!(Image::load_png(&saved).unwrap(), loaded);
        let err = Image::load_png(folder.join("missing.png")).unwrap_err();
        assert!(err.to_string().contains("missing.png"), "{err}");
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
