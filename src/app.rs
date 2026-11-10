use std::{
    fmt,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use web_time::Instant;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, Ime, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::{self, ModifiersState, NamedKey, PhysicalKey},
    monitor::{MonitorHandle, VideoModeHandle},
    window::{Window, WindowId},
};

use crate::{
    Context, Error, FrameStats, Result, WindowIcon,
    audio::Audio,
    gamepads::Gamepads,
    graphics::{
        Canvas, Resolution, Scaling,
        gpu::Gpu,
        renderer::Renderer,
        surface::{Acquired, WindowSurface},
    },
    input::{Key, MouseButton},
    save::Saves,
};

/// Longest frame time the game updates for, so a stall (dragging the window, a breakpoint, being
/// minimized) doesn't make the game jump forward.
const MAX_FRAME_TIME: f32 = 0.25;

/// A game driven by the engine's main loop.
pub trait Game {
    /// Advances the game by `dt` seconds. Called before each [`Game::draw`]: once a frame, or
    /// with an [update rate](WindowConfig::update_rate), as many times as the frame's time holds.
    fn update(&mut self, _ctx: &mut Context, _dt: f32) {}

    /// Lays out the game's egui interface, drawn over the frame: debug panels, or a tool's whole
    /// window. Called once a frame, after the updates and before [`Game::draw`], with `ui`
    /// covering the window. See the [`gui`](crate::gui) module.
    #[cfg(feature = "egui")]
    fn gui(&mut self, ctx: &mut Context, _ui: &mut egui::Ui) {
        // Frames needn't run egui at all, unless the debug window is open.
        ctx.gui.game_has_gui = false;
    }

    /// Draws the current frame.
    fn draw(&mut self, canvas: &mut Canvas);

    /// Called when the window's close button is pressed. Returning false keeps the window open,
    /// as a tool does to ask about unsaved work first; [`Context::quit`] closes it later.
    fn close_requested(&mut self, _ctx: &mut Context) -> bool {
        true
    }
}

/// Settings for the game window.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowConfig {
    pub title: String,
    /// The picture the window shows in its title bar and taskbar button, where the system shows
    /// windows' own. `None`, the default, leaves the system's.
    pub icon: Option<WindowIcon>,
    /// Initial width in logical pixels.
    pub width: u32,
    /// Initial height in logical pixels.
    pub height: u32,
    pub resizable: bool,
    /// Whether the window opens covering the screen. The game changes it with
    /// [`Context::set_fullscreen`].
    pub fullscreen: Fullscreen,
    /// The size of the canvas the game draws on.
    pub resolution: Resolution,
    /// How a fixed resolution fills the window, and the screen when the window covers it. The
    /// game changes it with [`Context::set_scaling`]. Without a fixed resolution, it changes
    /// nothing.
    pub scaling: Scaling,
    /// Whether frames wait for the display's refresh. Without vsync, frames come as fast as the
    /// game can make them (and may tear), which shows how much time a frame really takes.
    pub vsync: bool,
    /// How many times a second the game updates, each time by the same `dt`, however often
    /// frames come, so it plays the same at any frame rate. A frame runs as many updates as its
    /// time holds, possibly none, then draws. `None`, the default, updates once a frame by the
    /// frame's time. A rate must be above 0.
    pub update_rate: Option<f32>,
    /// A key that shows and hides the engine's window of frame stats, drawn with egui, and never
    /// reaches the game. `None`, the default, has no such key. Without the `egui` feature, it
    /// does nothing.
    pub debug_key: Option<Key>,
    /// When frames come: one after another, as games want, or only when something may have
    /// changed, as tools want.
    pub redraw: Redraw,
    /// Names the folder the game's [saves](crate::save) are kept in, among applications' data,
    /// so each game should have its own. Letters, digits, `_` and `-` only.
    pub identity: String,
    /// Where the game's saves are kept instead, if anywhere: a
    /// [`Simulation`](crate::headless::Simulation) without one keeps them in a temporary folder
    /// of its own.
    pub save_folder: Option<PathBuf>,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Wingine".into(),
            icon: None,
            width: 1280,
            height: 720,
            resizable: true,
            fullscreen: Fullscreen::Off,
            resolution: Resolution::Window,
            scaling: Scaling::Whole,
            vsync: true,
            update_rate: None,
            debug_key: None,
            redraw: Redraw::Continuously,
            identity: "wingine".into(),
            save_folder: None,
        }
    }
}

/// Whether a window covers the screen.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fullscreen {
    /// A window of its own size, with the system's title bar and borders.
    #[default]
    Off,
    /// A window without borders covering the screen it's on, at the screen's own resolution.
    /// The system goes on showing other windows and notifications over it, and switches away
    /// from it at once, so it's what players mostly want.
    Borderless,
    /// The screen given over to the game, as old games had it, at a video mode of the screen's:
    /// its own size and refresh rate, if it has a mode of them, or else the largest and fastest.
    /// Switching to it and away from it can take a moment, as the display changes mode. Where
    /// the system has no such thing, as on Wayland and the web, it's borderless.
    Exclusive,
}

impl Fullscreen {
    /// The mode's name in Luau and in `conf.luau`: `"borderless"` or `"exclusive"`, or none
    /// for a window of its own size, which is nil there.
    pub fn name(self) -> Option<&'static str> {
        match self {
            Self::Off => None,
            Self::Borderless => Some("borderless"),
            Self::Exclusive => Some("exclusive"),
        }
    }
}

/// When a window's frames come.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Redraw {
    /// One after another, paced by vsync, as games want.
    #[default]
    Continuously,
    /// Only when something may have changed what's shown, as tools want, so they use no power
    /// while nothing happens: on input from the keyboard and mouse, when the window changes, when
    /// egui asks (as its animations and blinking text cursors do), and when the game asks, with
    /// [`Context::request_redraw`] and [`Context::request_redraw_after`], or from another thread
    /// with a [`Waker`]. Gamepads don't ask for frames: they're read in the frames that come. The
    /// first frame after a pause updates by its whole time, a quarter second at most. With an
    /// [update rate](WindowConfig::update_rate), a frame too soon after the last for an update
    /// asks for another when one falls due, so the game still sees what the frame came for.
    OnDemand,
}

/// When a game wants its next frame: what it, egui and other threads have asked for since the
/// last frame began. A window that redraws on demand runs frames when they're due.
#[derive(Default)]
pub(crate) struct Redraws {
    /// The soonest a frame is wanted.
    due: Mutex<Option<Instant>>,
    /// Wakes the window's event loop to look at when a frame is due, if it redraws on demand.
    proxy: Mutex<Option<EventLoopProxy<()>>>,
}

impl Redraws {
    /// Wants a frame `delay` from now.
    pub fn request_after(&self, delay: Duration) {
        // A delay too long to say when is never.
        let Some(when) = Instant::now().checked_add(delay) else {
            return;
        };
        let mut due = lock(&self.due);
        if due.is_some_and(|due| due <= when) {
            return;
        }
        *due = Some(when);
        drop(due);
        if let Some(proxy) = &*lock(&self.proxy) {
            // It fails once the event loop has ended, when no frames are wanted.
            let _ = proxy.send_event(());
        }
    }

    /// When the next frame is wanted, if one is.
    pub fn due(&self) -> Option<Instant> {
        *lock(&self.due)
    }

    /// Forgets what was asked for, as a frame begins.
    pub fn begin_frame(&self) {
        *lock(&self.due) = None;
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Asks for a frame from any thread, for a game that [redraws on demand](Redraw::OnDemand): as a
/// thread does that reads what another process says, when there's something new to show. Get one
/// from [`Context::waker`]; clones ask for the same game's frames.
#[derive(Clone)]
pub struct Waker(pub(crate) Arc<Redraws>);

impl Waker {
    /// Asks for a frame as soon as there can be one. Does nothing once the window has closed.
    pub fn wake(&self) {
        self.0.request_after(Duration::ZERO);
    }
}

impl fmt::Debug for Waker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Waker").finish_non_exhaustive()
    }
}

/// Opens a window and runs a game until the window is closed.
///
/// `init` builds the game once the window and GPU are ready, so it can load assets through the
/// [`Context`]. An error from `init` closes the window and is returned from `run`.
///
/// If the `WINGINE_SCRIPT` environment variable names a script file, the game runs that script
/// headlessly instead of opening a window: see [`headless::Simulation`](crate::headless::Simulation)
/// and the script format in the `script` module docs.
///
/// The game's [saves](crate::save) are kept in a folder named by `config.identity`, unless
/// `config.save_folder` gives another.
///
/// # Panics
///
/// If `config` has an update rate that isn't above 0, or an identity that isn't letters, digits,
/// `_` and `-`, or is a name Windows keeps for a device, such as `con`.
#[cfg(not(target_arch = "wasm32"))]
pub fn run<G, F>(config: WindowConfig, init: F) -> Result<()>
where
    G: Game,
    F: FnOnce(&mut Context) -> Result<G>,
{
    if let Some(script) = std::env::var_os(crate::script::ENV_VAR) {
        return crate::script::run(&config, init, std::path::Path::new(&script));
    }

    let event_loop = event_loop()?;
    let mut app = App::new(&event_loop, config, init);
    event_loop
        .run_app(&mut app)
        .map_err(|err| Error::EventLoop(err.into()))?;
    app.error.map_or(Ok(()), Err)
}

/// Starts a game in a canvas added to the page, and returns at once, as the browser runs the
/// page's event loop and the game in it: the web's `run`. The canvas is as big as the page's
/// styles make it, in place of the window's size, and the window's title is the page's.
///
/// `init` builds the game once the browser has given the canvas a GPU, after `run` has returned,
/// so an error from it can't be returned: it's logged, and the game doesn't start.
///
/// # Panics
///
/// If `config` has an update rate that isn't above 0, or an identity that isn't letters, digits,
/// `_` and `-`, or is a name Windows keeps for a device, such as `con`.
#[cfg(target_arch = "wasm32")]
pub fn run<G, F>(config: WindowConfig, init: F) -> Result<()>
where
    G: Game + 'static,
    F: FnOnce(&mut Context) -> Result<G> + 'static,
{
    use winit::platform::web::EventLoopExtWebSys;

    let event_loop = event_loop()?;
    let app = App::new(&event_loop, config, init);
    event_loop.spawn_app(app);
    Ok(())
}

fn event_loop() -> Result<EventLoop<()>> {
    let event_loop = EventLoop::new().map_err(|err| Error::EventLoop(err.into()))?;
    // Frames are driven by requesting a redraw after each one, paced by vsync, or on demand.
    // Waiting (rather than polling) lets the loop sleep when no frame is wanted, e.g. while
    // minimized.
    event_loop.set_control_flow(ControlFlow::Wait);
    Ok(event_loop)
}

/// Runs one frame of `game`, as both the window and a [`Simulation`] do: updates it as `timestep`
/// says for a frame `elapsed` seconds long, with a canvas `canvas_size` canvas pixels in size,
/// runs its interface with the input the caller gave the GUI, then has it draw on `canvas`.
/// Returns the frame's stats so far: the caller adds rendering and the frame's whole time.
///
/// [`Simulation`]: crate::headless::Simulation
pub(crate) fn run_frame(
    game: &mut impl Game,
    ctx: &mut Context,
    canvas: &mut Canvas,
    canvas_size: [f32; 2],
    timestep: &mut Timestep,
    elapsed: f32,
) -> FrameStats {
    ctx.canvas_size = canvas_size;
    let (updates, dt) = timestep.updates(elapsed);
    // With an update rate, a frame may come too soon after the last for an update, as one a window
    // that redraws on demand runs for input may: another comes when one's due, so the game sees
    // what this one was for without waiting for more.
    if updates == 0
        && let Some(wait) = timestep.until_next()
    {
        ctx.redraws.request_after(wait);
    }
    let mut update = 0.0;
    for _ in 0..updates {
        let start = Instant::now();
        game.update(ctx, dt);
        update += start.elapsed().as_secs_f32();
        // Presses and releases last until an update has seen them.
        ctx.input.end_frame();
        ctx.audio.advance(dt);
    }
    let start = Instant::now();
    #[cfg(feature = "egui")]
    crate::gui::run_pass(ctx, game);
    canvas.begin(canvas_size[0], canvas_size[1]);
    canvas.frame_stats = ctx.frame_stats;
    game.draw(canvas);
    FrameStats {
        update,
        draw: start.elapsed().as_secs_f32(),
        quads: canvas.quad_count(),
        triangles: canvas.triangle_count(),
        draw_calls: canvas.draw_call_count(),
        ..Default::default()
    }
}

/// Turns the time frames take into updates: one a frame by the frame's time, or a fixed number a
/// second.
pub(crate) struct Timestep {
    /// The seconds each update covers, with a fixed rate.
    step: Option<f64>,
    /// The seconds of frames not yet updated for, with a fixed rate.
    carried: f64,
}

impl Timestep {
    /// How close a frame's time must be to a whole number of steps, as a fraction of a step, to
    /// count as exactly that many.
    const SNAP: f64 = 0.01;

    /// Updates `rate` times a second, or once a frame without a rate.
    ///
    /// # Panics
    ///
    /// If `rate` isn't above 0.
    pub(crate) fn new(rate: Option<f32>) -> Self {
        let step = rate.map(|rate| {
            assert!(
                rate > 0.0 && rate.is_finite(),
                "an update rate must be above 0, not {rate}"
            );
            1.0 / f64::from(rate)
        });
        Self {
            step,
            // A quarter of a step ahead, so frames that take about a step each, or half of one,
            // end well clear of when an update falls due, and jitter in their timing doesn't move
            // updates from one frame to the next.
            carried: step.map_or(0.0, |step| step / 4.0),
        }
    }

    /// How many updates to run for a frame that took `elapsed` seconds, and the `dt` of each.
    pub(crate) fn updates(&mut self, elapsed: f32) -> (u32, f32) {
        let elapsed = elapsed.min(MAX_FRAME_TIME);
        let Some(step) = self.step else {
            return (1, elapsed);
        };
        // Frames paced by a display that refreshes at about the update rate, such as 59.94 Hz
        // for 60, count as a step each. Otherwise the difference would add up until a frame had
        // no update, and the next two.
        let steps = f64::from(elapsed) / step;
        let whole = steps.round();
        let steps = if whole >= 1.0 && (steps - whole).abs() < Self::SNAP {
            whole
        } else {
            steps
        };
        self.carried += steps * step;
        let updates = (self.carried / step).floor();
        self.carried -= updates * step;
        (updates as u32, step as f32)
    }

    /// How long until the next update falls due, with a fixed rate.
    pub(crate) fn until_next(&self) -> Option<Duration> {
        let step = self.step?;
        Duration::try_from_secs_f64((step - self.carried).max(0.0)).ok()
    }
}

struct App<G, F> {
    config: WindowConfig,
    /// Wakes the event loop when a game that redraws on demand wants a frame.
    proxy: EventLoopProxy<()>,
    /// Taken on startup.
    init: Option<F>,
    /// Created once the event loop starts, since windows can't be created before then.
    state: Option<State<G>>,
    /// A startup failure, returned from [`run`] after the event loop exits.
    error: Option<Error>,
    /// The canvas on the web, while the browser finds it a GPU.
    #[cfg(target_arch = "wasm32")]
    starting: Option<Starting>,
}

/// A window on the web waiting for its GPU, which the browser gives asynchronously.
#[cfg(target_arch = "wasm32")]
struct Starting {
    window: Arc<Window>,
    /// The GPU and the window's surface, once they're ready, or why there are none.
    gpu: std::rc::Rc<std::cell::RefCell<Option<Result<GpuFound>>>>,
}

/// A GPU, and the window's surface it presents to.
#[cfg(target_arch = "wasm32")]
type GpuFound = (Gpu, wgpu::Surface<'static>);

impl<G, F> App<G, F> {
    fn new(event_loop: &EventLoop<()>, config: WindowConfig, init: F) -> Self {
        Self {
            proxy: event_loop.create_proxy(),
            config,
            init: Some(init),
            state: None,
            error: None,
            #[cfg(target_arch = "wasm32")]
            starting: None,
        }
    }

    /// Ends the game because it couldn't start.
    fn fail(&mut self, event_loop: &ActiveEventLoop, err: Error) {
        // On the web, `run` has returned already.
        #[cfg(target_arch = "wasm32")]
        log::error!("the game couldn't start: {err}");
        self.error = Some(err);
        event_loop.exit();
    }
}

impl<G, F> ApplicationHandler for App<G, F>
where
    G: Game,
    F: FnOnce(&mut Context) -> Result<G>,
{
    #[cfg(not(target_arch = "wasm32"))]
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Some(init) = self.init.take() else {
            return;
        };
        let state =
            create_window(event_loop, &self.config).and_then(|(window, instance, surface)| {
                let gpu = pollster::block_on(Gpu::new(instance, Some(&surface)))?;
                State::new(
                    event_loop,
                    &self.config,
                    self.proxy.clone(),
                    window,
                    surface,
                    gpu,
                    init,
                )
            });
        match state {
            Ok(state) => {
                state.window.request_redraw();
                self.state = Some(state);
            }
            Err(err) => self.fail(event_loop, err),
        }
    }

    /// Opens the canvas and asks the browser for a GPU, which [`App::user_event`] takes.
    #[cfg(target_arch = "wasm32")]
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.init.is_none() || self.starting.is_some() {
            return;
        }
        let (window, instance, surface) = match create_window(event_loop, &self.config) {
            Ok(created) => created,
            Err(err) => return self.fail(event_loop, err),
        };
        let gpu = std::rc::Rc::<std::cell::RefCell<Option<Result<GpuFound>>>>::default();
        let ready = std::rc::Rc::clone(&gpu);
        let proxy = self.proxy.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let found = Gpu::new(instance, Some(&surface)).await;
            *ready.borrow_mut() = Some(found.map(|gpu| (gpu, surface)));
            // It fails once the event loop has ended, when there's no game to start.
            let _ = proxy.send_event(());
        });
        self.starting = Some(Starting { window, gpu });
    }

    /// Builds the game once the browser has given its canvas a GPU. Other events only wake the
    /// loop to look at when a frame is due.
    #[cfg(target_arch = "wasm32")]
    fn user_event(&mut self, event_loop: &ActiveEventLoop, (): ()) {
        let Some(gpu) = self
            .starting
            .as_ref()
            .and_then(|s| s.gpu.borrow_mut().take())
        else {
            return;
        };
        let Starting { window, .. } = self.starting.take().expect("the canvas is starting");
        let init = self.init.take().expect("the game is built once");
        let state = gpu.and_then(|(gpu, surface)| {
            State::new(
                event_loop,
                &self.config,
                self.proxy.clone(),
                window,
                surface,
                gpu,
                init,
            )
        });
        match state {
            Ok(state) => {
                state.window.request_redraw();
                self.state = Some(state);
            }
            Err(err) => self.fail(event_loop, err),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        // Whether egui wants the event to itself, such as a click on one of its windows. A key
        // egui doesn't hear (Tab, unless one of its widgets has focus) is the game's alone.
        #[cfg(feature = "egui")]
        let to_gui = if hidden_from_gui(&event, &state.ctx.gui) {
            false
        } else {
            state
                .gui_window
                .on_window_event(&state.window, &event)
                .consumed
        };
        #[cfg(not(feature = "egui"))]
        let to_gui = false;
        if state.redraw == Redraw::OnDemand && changes_frame(&event) {
            state.window.request_redraw();
        }
        match event {
            WindowEvent::CloseRequested if state.game.close_requested(&mut state.ctx) => {
                event_loop.exit();
            }
            WindowEvent::Resized(size) => {
                state.surface.resize(&state.ctx.gpu, size);
                // Covering the screen or leaving it resizes the window, whether the game asked
                // for it or the player did, through the system.
                state.fullscreen = from_winit(state.window.fullscreen());
                state.ctx.fullscreen = state.fullscreen;
                state.window.request_redraw();
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => state.key_event(&event, is_synthetic, to_gui),
            WindowEvent::ModifiersChanged(modifiers) => state.modifiers = modifiers.state(),
            WindowEvent::Ime(ime) if !to_gui => match ime {
                Ime::Commit(text) => state.ctx.input.type_text(&text),
                Ime::Preedit(text, _) => state.ctx.input.compose(&text),
                Ime::Disabled => state.ctx.input.compose(""),
                Ime::Enabled => {}
            },
            WindowEvent::CursorMoved { position, .. } => {
                state.cursor = Some([position.x, position.y])
            }
            WindowEvent::CursorLeft { .. } => state.cursor = None,
            WindowEvent::MouseWheel { delta, .. } if !to_gui => {
                let scale_factor = state.window.scale_factor();
                state.ctx.input.turn_wheel_by(delta, scale_factor);
            }
            WindowEvent::MouseInput {
                state: button_state,
                button,
                ..
            } => {
                if let Some(button) = MouseButton::from_winit(button) {
                    match button_state {
                        ElementState::Pressed if !to_gui => state.ctx.input.mouse_down(button),
                        ElementState::Pressed => {}
                        ElementState::Released => state.ctx.input.mouse_up(button),
                    }
                }
            }
            // Releases aren't delivered while unfocused, so treat every key and button as released.
            WindowEvent::Focused(focused) => {
                state.focused = focused;
                if !focused {
                    state.ctx.input.release_all();
                }
            }
            // Frames stop while the window is hidden, so they start again here.
            WindowEvent::Occluded(false) => state.window.request_redraw(),
            WindowEvent::RedrawRequested => {
                state.frame();
                if state.ctx.quit {
                    event_loop.exit();
                } else if state.ctx.fullscreen != state.fullscreen {
                    state.set_fullscreen(state.ctx.fullscreen);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        let Some(state) = &self.state else {
            return;
        };
        if state.redraw != Redraw::OnDemand {
            return;
        }
        // A frame that's due is requested until one begins. If the window can't show one, as
        // while it's hidden, the loop waits for an event before asking again.
        let control_flow = match state.ctx.redraws.due() {
            Some(due) if due <= Instant::now() => {
                state.window.request_redraw();
                ControlFlow::Wait
            }
            Some(due) => ControlFlow::WaitUntil(due),
            None => ControlFlow::Wait,
        };
        event_loop.set_control_flow(control_flow);
    }
}

/// Whether `event` is a key egui doesn't hear (see [`Gui::hears_key`](crate::gui::Gui::hears_key)),
/// which then goes to the game alone.
#[cfg(feature = "egui")]
fn hidden_from_gui(event: &WindowEvent, gui: &crate::gui::Gui) -> bool {
    match event {
        WindowEvent::KeyboardInput { event, .. } => match event.physical_key {
            PhysicalKey::Code(code) => Key::from_winit(code).is_some_and(|key| !gui.hears_key(key)),
            PhysicalKey::Unidentified(_) => false,
        },
        _ => false,
    }
}

/// Whether `event` may change what a window that redraws on demand shows: input, and changes to
/// the window. Its own redraws and moves don't.
fn changes_frame(event: &WindowEvent) -> bool {
    !matches!(
        event,
        WindowEvent::RedrawRequested
            | WindowEvent::Moved(_)
            | WindowEvent::Destroyed
            | WindowEvent::Occluded(true)
            | WindowEvent::AxisMotion { .. }
            | WindowEvent::ActivationTokenDone { .. }
    )
}

struct State<G> {
    window: Arc<Window>,
    /// Whether the window covers the screen, as it was last made to or last said it did after a
    /// resize. The game's wish, [`Context::fullscreen`], is compared with it after each frame.
    fullscreen: Fullscreen,
    /// Whether the system can give the screen over to the game, for [`Fullscreen::Exclusive`].
    exclusive: bool,
    surface: WindowSurface,
    ctx: Context,
    canvas: Canvas,
    game: G,
    timestep: Timestep,
    redraw: Redraw,
    last_frame: Instant,
    /// The stats of the last frame, which become the context's once its whole time is known, at
    /// the start of the next.
    stats: FrameStats,
    /// Cursor position in physical window pixels, while it is over the window.
    cursor: Option<[f64; 2]>,
    /// The gamepads, unless the system has none to read.
    gamepads: Option<Gamepads>,
    /// Whether the window has the keyboard's focus, and so the gamepads'.
    focused: bool,
    /// The modifier keys held, which decide whether a key types text or makes a shortcut.
    modifiers: ModifiersState,
    /// Where the window last told the system's input method the game shows text, while it lets
    /// the game take one.
    #[cfg(not(feature = "egui"))]
    input_method: Option<crate::math::Rect>,
    /// Turns window events into egui's input, and acts on what egui asks of the window.
    #[cfg(feature = "egui")]
    gui_window: egui_winit::State,
}

/// `icon` as winit takes it, scaled up by whole numbers so that the system only ever shrinks it.
fn winit_icon(icon: &WindowIcon) -> Option<winit::window::Icon> {
    let image = icon.scaled_within(crate::icon::LARGEST);
    let (width, height) = image.dimensions();
    winit::window::Icon::from_rgba(image.into_raw(), width, height)
        .inspect_err(|err| log::warn!("couldn't give the window its icon: {err}"))
        .ok()
}

/// Opens the game's window, or on the web its canvas, and a surface on it for a GPU to present
/// frames to.
fn create_window(
    event_loop: &ActiveEventLoop,
    config: &WindowConfig,
) -> Result<(Arc<Window>, wgpu::Instance, wgpu::Surface<'static>)> {
    let icon = config.icon.as_ref().and_then(winit_icon);
    // Windows shows the window's icon small, in its title bar, and its taskbar's large.
    #[cfg(windows)]
    let taskbar_icon = icon.clone();
    let attributes = Window::default_attributes()
        .with_title(&config.title)
        .with_window_icon(icon)
        .with_resizable(config.resizable)
        .with_fullscreen(opening_fullscreen(event_loop, config));
    #[cfg(windows)]
    let attributes = {
        use winit::platform::windows::WindowAttributesExtWindows;
        attributes.with_taskbar_icon(taskbar_icon)
    };
    #[cfg(not(target_arch = "wasm32"))]
    let attributes =
        attributes.with_inner_size(winit::dpi::LogicalSize::new(config.width, config.height));
    // The page's styles size the canvas, which a size here would override.
    #[cfg(target_arch = "wasm32")]
    let attributes = {
        use winit::platform::web::WindowAttributesExtWebSys;
        attributes.with_append(true)
    };
    let window = event_loop
        .create_window(attributes)
        .map_err(|err| Error::CreateWindow(err.into()))?;
    let window = Arc::new(window);
    let instance = wgpu::Instance::new(
        wgpu::InstanceDescriptor::new_with_display_handle(Box::new(
            event_loop.owned_display_handle(),
        ))
        .with_env(),
    );
    let surface = instance
        .create_surface(window.clone())
        .map_err(|err| Error::CreateSurface(err.into()))?;
    Ok((window, instance, surface))
}

/// How the game's window covers the screen as it opens, on the main screen, as winit has it.
fn opening_fullscreen(
    event_loop: &ActiveEventLoop,
    config: &WindowConfig,
) -> Option<winit::window::Fullscreen> {
    to_winit(
        config.fullscreen,
        event_loop.primary_monitor(),
        has_exclusive_fullscreen(event_loop),
    )
}

/// Whether the system can give the screen over to a game: Wayland can't.
#[cfg(all(
    unix,
    not(target_vendor = "apple"),
    not(target_os = "android"),
    not(target_os = "emscripten"),
    not(target_os = "redox")
))]
fn has_exclusive_fullscreen(event_loop: &ActiveEventLoop) -> bool {
    use winit::platform::wayland::ActiveEventLoopExtWayland;
    !event_loop.is_wayland()
}

/// Whether the system can give the screen over to a game. Where it can't, as on the web, its
/// screens have no video modes.
#[cfg(not(all(
    unix,
    not(target_vendor = "apple"),
    not(target_os = "android"),
    not(target_os = "emscripten"),
    not(target_os = "redox")
)))]
fn has_exclusive_fullscreen(_: &ActiveEventLoop) -> bool {
    true
}

/// `fullscreen` as winit has it, on `monitor`: exclusive at the [video mode](video_mode) it picks,
/// if the system has `exclusive` fullscreen and the monitor a mode, or else borderless.
fn to_winit(
    fullscreen: Fullscreen,
    monitor: Option<MonitorHandle>,
    exclusive: bool,
) -> Option<winit::window::Fullscreen> {
    use winit::window::Fullscreen as Winit;
    match fullscreen {
        Fullscreen::Off => None,
        Fullscreen::Borderless => Some(Winit::Borderless(monitor)),
        Fullscreen::Exclusive => {
            let mode = monitor.as_ref().filter(|_| exclusive).and_then(video_mode);
            Some(mode.map_or(Winit::Borderless(monitor), Winit::Exclusive))
        }
    }
}

/// How winit says a window covers the screen.
fn from_winit(fullscreen: Option<winit::window::Fullscreen>) -> Fullscreen {
    match fullscreen {
        None => Fullscreen::Off,
        Some(winit::window::Fullscreen::Borderless(_)) => Fullscreen::Borderless,
        Some(winit::window::Fullscreen::Exclusive(_)) => Fullscreen::Exclusive,
    }
}

/// The video mode of `monitor` that exclusive fullscreen uses, as [`best_video_mode`] picks it.
fn video_mode(monitor: &MonitorHandle) -> Option<VideoModeHandle> {
    let modes: Vec<VideoModeHandle> = monitor.video_modes().collect();
    let size = monitor.size();
    let best = best_video_mode(
        modes.iter().map(|mode| {
            let size = mode.size();
            (
                [size.width, size.height],
                mode.refresh_rate_millihertz(),
                mode.bit_depth(),
            )
        }),
        [size.width, size.height],
        monitor.refresh_rate_millihertz(),
    )?;
    modes.into_iter().nth(best)
}

/// Which of a screen's video modes exclusive fullscreen uses, given each mode's size, refresh rate
/// in millihertz and bits a pixel: one of the screen's own `size`, and its `refresh` rate, as far
/// as it has them, so the display changes as little as it can; then the largest, the fastest and
/// the deepest.
fn best_video_mode(
    modes: impl IntoIterator<Item = ([u32; 2], u32, u16)>,
    size: [u32; 2],
    refresh: Option<u32>,
) -> Option<usize> {
    modes
        .into_iter()
        .enumerate()
        .max_by_key(|&(_, (mode_size, mode_refresh, depth))| {
            (
                mode_size == size,
                Some(mode_refresh) == refresh,
                u64::from(mode_size[0]) * u64::from(mode_size[1]),
                mode_refresh,
                depth,
            )
        })
        .map(|(index, _)| index)
}

impl<G: Game> State<G> {
    /// Builds the game in `window`, which [`create_window`] opened with `surface`, once `gpu` has
    /// been found for it.
    fn new(
        event_loop: &ActiveEventLoop,
        config: &WindowConfig,
        proxy: EventLoopProxy<()>,
        window: Arc<Window>,
        surface: wgpu::Surface<'static>,
        gpu: Gpu,
        init: impl FnOnce(&mut Context) -> Result<G>,
    ) -> Result<Self> {
        let timestep = Timestep::new(config.update_rate);
        if let Some(problem) = config.resolution.problem(&gpu.device) {
            return Err(Error::WindowConfig(problem));
        }
        let surface = WindowSurface::new(&gpu, window.clone(), surface, config.vsync);
        let renderer = Renderer::new(&gpu.device, &gpu.queue, surface.view_format());
        let mut ctx = Context::new(gpu, renderer, Audio::new());
        ctx.saves = Saves::for_window(config);
        // Before the game is built, so it can see the gamepads already connected.
        let gamepads = Gamepads::new(&mut ctx.input);
        // Frames that come one after another needn't be asked for.
        if config.redraw == Redraw::OnDemand {
            *lock(&ctx.redraws.proxy) = Some(proxy);
        }
        // As the window opened, falling back from exclusive as it did.
        let fullscreen = from_winit(opening_fullscreen(event_loop, config));
        ctx.fullscreen = fullscreen;
        // The game is built knowing the canvas size, as it is in a `Simulation`.
        ctx.screen.set_resolution(config.resolution);
        ctx.screen.set_scaling(config.scaling);
        let size = window.inner_size();
        ctx.canvas_size = ctx
            .screen
            .canvas_size([size.width, size.height], window.scale_factor());
        #[cfg(feature = "egui")]
        let gui_window = {
            ctx.gui.debug_key = config.debug_key;
            let max_texture_side = ctx.gpu.device.limits().max_texture_dimension_2d as usize;
            egui_winit::State::new(
                ctx.gui.egui.clone(),
                egui::ViewportId::ROOT,
                event_loop,
                Some(window.scale_factor() as f32),
                window.theme(),
                Some(max_texture_side),
            )
        };
        let game = init(&mut ctx)?;

        Ok(Self {
            focused: window.has_focus(),
            window,
            fullscreen,
            exclusive: has_exclusive_fullscreen(event_loop),
            surface,
            ctx,
            canvas: Canvas::new(),
            game,
            timestep,
            redraw: config.redraw,
            last_frame: Instant::now(),
            stats: FrameStats::default(),
            cursor: None,
            gamepads,
            modifiers: ModifiersState::empty(),
            #[cfg(not(feature = "egui"))]
            input_method: None,
            #[cfg(feature = "egui")]
            gui_window,
        })
    }

    /// Passes a key event to the game, and what it types, unless egui wanted it (`to_gui`) or
    /// it's the debug key.
    fn key_event(&mut self, event: &KeyEvent, is_synthetic: bool, to_gui: bool) {
        let key = match event.physical_key {
            PhysicalKey::Code(code) => Key::from_winit(code),
            PhysicalKey::Unidentified(_) => None,
        };
        match event.state {
            // Synthetic presses report keys already held when the window gains focus, such as
            // the Alt of an Alt+Tab. They weren't pressed for this game, so they're ignored.
            ElementState::Pressed if !is_synthetic && !to_gui => {
                // On Windows, the keys an input method takes still come, as `Process`.
                if event.logical_key == keyboard::Key::Named(NamedKey::Process) {
                    return;
                }
                if let Some(key) = key {
                    #[cfg(feature = "egui")]
                    if (event.repeat && self.ctx.gui.debug_key == Some(key))
                        || (!event.repeat && self.ctx.gui.debug_key_down(key))
                    {
                        return;
                    }
                    if event.repeat {
                        self.ctx.input.key_repeat(key);
                    } else {
                        self.ctx.input.key_down(key);
                    }
                }
                // Keys with Control (but not Alt, as AltGr is on Windows) or Command make
                // shortcuts, which on some systems type their letter too.
                let modifiers = self.modifiers;
                let shortcut =
                    modifiers.super_key() || (modifiers.control_key() && !modifiers.alt_key());
                let text = event
                    .text
                    .as_deref()
                    .or_else(|| event.logical_key.to_text());
                if let Some(text) = text
                    && !shortcut
                {
                    self.ctx.input.type_text(text);
                }
            }
            ElementState::Pressed => {}
            ElementState::Released => {
                if let Some(key) = key {
                    self.ctx.input.key_up(key);
                }
            }
        }
    }

    /// Lets the system's input method compose text for the game while it takes one, unless egui
    /// is using it, for a text field of its own.
    #[cfg(feature = "egui")]
    fn take_input_method(&self, output: &mut egui::PlatformOutput, size: [u32; 2]) {
        let Some(area) = self.ctx.input.input_method() else {
            return;
        };
        if output.ime.is_some() {
            return;
        }
        let scale_factor = self.window.scale_factor();
        let pixels_per_point = self.ctx.gui.egui.pixels_per_point();
        let to_points = |point: [f32; 2]| {
            let [x, y] = self.ctx.screen.to_output(point, size, scale_factor);
            egui::pos2(x as f32, y as f32) / pixels_per_point
        };
        let rect = egui::Rect::from_two_pos(
            to_points([area.x, area.y]),
            to_points([area.x + area.w, area.y + area.h]),
        );
        output.ime = Some(egui::output::IMEOutput {
            purpose: egui::IMEPurpose::Normal,
            rect,
            cursor_rect: rect,
            should_interrupt_composition: false,
        });
    }

    /// Lets the system's input method compose text for the game while it takes one.
    #[cfg(not(feature = "egui"))]
    fn take_input_method(&mut self, size: [u32; 2]) {
        let area = self.ctx.input.input_method();
        if area == self.input_method {
            return;
        }
        if area.is_some() != self.input_method.is_some() {
            self.window.set_ime_allowed(area.is_some());
        }
        self.input_method = area;
        if let Some(area) = area {
            let scale_factor = self.window.scale_factor();
            let screen = &self.ctx.screen;
            let [x, y] = screen.to_output([area.x, area.y], size, scale_factor);
            let [right, bottom] =
                screen.to_output([area.x + area.w, area.y + area.h], size, scale_factor);
            self.window.set_ime_cursor_area(
                winit::dpi::PhysicalPosition::new(x, y),
                winit::dpi::PhysicalSize::new(right - x, bottom - y),
            );
        }
    }

    /// Makes the window cover the screen it's on, or not, as `fullscreen` says, noting what it
    /// became: borderless where it couldn't be exclusive.
    fn set_fullscreen(&mut self, fullscreen: Fullscreen) {
        let monitor = self
            .window
            .current_monitor()
            .or_else(|| self.window.primary_monitor());
        let winit = to_winit(fullscreen, monitor, self.exclusive);
        self.fullscreen = from_winit(winit.clone());
        self.ctx.fullscreen = self.fullscreen;
        self.window.set_fullscreen(winit);
    }

    fn frame(&mut self) {
        self.ctx.redraws.begin_frame();
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            // Minimized. The next `Resized` event requests a redraw.
            return;
        }

        let output = [size.width, size.height];
        let scale_factor = self.window.scale_factor();
        // Mapped every frame, since resizing moves the canvas under a still cursor.
        let mouse_position = self
            .cursor
            .map(|cursor| self.ctx.screen.to_canvas(cursor, output, scale_factor));
        self.ctx.input.set_mouse_position(mouse_position);
        if let Some(gamepads) = &mut self.gamepads {
            gamepads.poll(&mut self.ctx.input, self.focused);
        }
        let canvas_size = self.ctx.screen.canvas_size(output, scale_factor);

        let now = Instant::now();
        let elapsed = (now - self.last_frame).as_secs_f32();
        self.last_frame = now;
        self.ctx.frame_stats = FrameStats {
            frame: elapsed,
            ..self.stats
        };
        #[cfg(feature = "egui")]
        self.ctx
            .gui
            .set_input(self.gui_window.take_egui_input(&self.window));
        self.stats = run_frame(
            &mut self.game,
            &mut self.ctx,
            &mut self.canvas,
            canvas_size,
            &mut self.timestep,
            elapsed,
        );
        #[cfg(feature = "egui")]
        {
            let mut platform_output = std::mem::take(&mut self.ctx.gui.platform_output);
            self.take_input_method(&mut platform_output, output);
            self.gui_window
                .handle_platform_output(&self.window, platform_output);
        }
        #[cfg(not(feature = "egui"))]
        self.take_input_method(output);

        let Context {
            gpu,
            renderer,
            screen,
            #[cfg(feature = "egui")]
            gui,
            ..
        } = &mut self.ctx;
        match self.surface.acquire_frame(gpu) {
            Acquired::Frame(frame) => {
                let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
                    format: Some(self.surface.view_format()),
                    ..Default::default()
                });
                let output_size = [frame.texture.width(), frame.texture.height()];
                let start = Instant::now();
                screen.render(
                    &gpu.device,
                    &gpu.queue,
                    renderer,
                    &self.canvas,
                    &view,
                    output_size,
                );
                #[cfg(feature = "egui")]
                gui.render(&gpu.device, &gpu.queue, &frame.texture);
                self.stats.render = start.elapsed().as_secs_f32();
                self.surface.present(gpu, frame);
            }
            // Nothing was shown, so the next frame tries again.
            Acquired::Skip => {
                self.window.request_redraw();
                return;
            }
            // Hidden windows aren't paced by vsync, so asking for the next frame at once would
            // spin. `WindowEvent::Occluded(false)` asks for it instead.
            Acquired::Occluded => return,
        }
        if self.redraw == Redraw::Continuously {
            self.window.request_redraw();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How many updates each of `frames`, their times in seconds, gets at `rate`.
    fn updates(rate: f32, frames: impl IntoIterator<Item = f32>) -> Vec<u32> {
        let mut timestep = Timestep::new(Some(rate));
        frames
            .into_iter()
            .map(|elapsed| timestep.updates(elapsed).0)
            .collect()
    }

    #[test]
    fn without_a_rate_frames_update_once_by_their_time() {
        let mut timestep = Timestep::new(None);
        assert_eq!(timestep.updates(0.01), (1, 0.01));
        assert_eq!(timestep.updates(1.0), (1, MAX_FRAME_TIME));
    }

    #[test]
    fn a_rate_updates_as_often_as_frames_hold() {
        // Frames at 120, 60 and 30 Hz, updating at 60.
        assert_eq!(updates(60.0, [1.0 / 120.0; 4]), [0, 1, 0, 1]);
        assert_eq!(updates(60.0, [1.0 / 60.0; 3]), [1, 1, 1]);
        assert_eq!(updates(60.0, [1.0 / 30.0; 3]), [2, 2, 2]);
        let (_, dt) = Timestep::new(Some(60.0)).updates(0.1);
        assert!((dt - 1.0 / 60.0).abs() < 1e-9);
    }

    #[test]
    fn jitter_in_frames_at_the_rate_moves_no_updates() {
        let frames = (0..600).map(|i| 1.0 / 60.0 + if i % 2 == 0 { 0.0005 } else { -0.0005 });
        assert!(updates(60.0, frames).iter().all(|&n| n == 1));
    }

    #[test]
    fn frames_close_to_the_rate_update_once_each() {
        // A 59.94 Hz display, a little off in its timing. Updating by the frames' times, a
        // frame every 16 seconds would have no update.
        let frames = (0..6000).map(|i| 1.0 / 59.94 + if i % 3 == 0 { 0.0001 } else { -0.00005 });
        assert!(updates(60.0, frames).iter().all(|&n| n == 1));
    }

    #[test]
    fn a_stall_updates_for_a_quarter_second_at_most() {
        assert_eq!(updates(60.0, [10.0]), [15]);
    }

    #[test]
    fn exclusive_fullscreen_keeps_the_screens_own_mode_where_it_can() {
        let modes = [
            ([1280, 720], 60_000, 32),
            ([1920, 1080], 144_000, 32),
            ([1920, 1080], 60_000, 32),
            ([1920, 1080], 60_000, 16),
            ([2560, 1440], 60_000, 32),
        ];
        // Its own size and refresh rate, the deepest.
        assert_eq!(best_video_mode(modes, [1920, 1080], Some(60_000)), Some(2));
        // Its own size, the fastest, without its refresh rate.
        assert_eq!(best_video_mode(modes, [1920, 1080], None), Some(1));
        assert_eq!(best_video_mode(modes, [1920, 1080], Some(75_000)), Some(1));
        // The largest, without its own size.
        assert_eq!(best_video_mode(modes, [3840, 2160], Some(60_000)), Some(4));
        assert_eq!(best_video_mode([], [1920, 1080], Some(60_000)), None);
    }

    #[test]
    fn exclusive_fullscreen_is_borderless_without_a_mode() {
        // Without a screen, or where the system has no exclusive fullscreen.
        assert_eq!(
            from_winit(to_winit(Fullscreen::Exclusive, None, true)),
            Fullscreen::Borderless
        );
        assert_eq!(
            from_winit(to_winit(Fullscreen::Off, None, true)),
            Fullscreen::Off
        );
        assert_eq!(
            from_winit(to_winit(Fullscreen::Borderless, None, false)),
            Fullscreen::Borderless
        );
    }
}
