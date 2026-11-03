use std::{
    fmt,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy},
    keyboard::PhysicalKey,
    window::{Window, WindowId},
};

use crate::{
    Context, Error, FrameStats, Result,
    audio::Audio,
    gamepads::Gamepads,
    graphics::{
        Canvas, Resolution,
        gpu::Gpu,
        renderer::Renderer,
        screen::Screen,
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
#[derive(Clone, Debug)]
pub struct WindowConfig {
    pub title: String,
    /// Initial width in logical pixels.
    pub width: u32,
    /// Initial height in logical pixels.
    pub height: u32,
    pub resizable: bool,
    /// The size of the canvas the game draws on.
    pub resolution: Resolution,
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
            width: 1280,
            height: 720,
            resizable: true,
            resolution: Resolution::Window,
            vsync: true,
            update_rate: None,
            debug_key: None,
            redraw: Redraw::Continuously,
            identity: "wingine".into(),
            save_folder: None,
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
    /// first frame after a pause updates by its whole time, a quarter second at most.
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
/// `_` and `-`.
pub fn run<G, F>(config: WindowConfig, init: F) -> Result<()>
where
    G: Game,
    F: FnOnce(&mut Context) -> Result<G>,
{
    if let Some(script) = std::env::var_os(crate::script::ENV_VAR) {
        return crate::script::run(&config, init, std::path::Path::new(&script));
    }

    let event_loop = EventLoop::new().map_err(|err| Error::EventLoop(err.into()))?;
    // Frames are driven by requesting a redraw after each one, paced by vsync, or on demand.
    // Waiting (rather than polling) lets the loop sleep when no frame is wanted, e.g. while
    // minimized.
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App {
        proxy: event_loop.create_proxy(),
        config,
        init: Some(init),
        state: None,
        error: None,
    };
    event_loop
        .run_app(&mut app)
        .map_err(|err| Error::EventLoop(err.into()))?;
    app.error.map_or(Ok(()), Err)
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
}

impl<G, F> ApplicationHandler for App<G, F>
where
    G: Game,
    F: FnOnce(&mut Context) -> Result<G>,
{
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let Some(init) = self.init.take() else {
            return;
        };
        match State::new(event_loop, &self.config, self.proxy.clone(), init) {
            Ok(state) => {
                state.window.request_redraw();
                self.state = Some(state);
            }
            Err(err) => {
                self.error = Some(err);
                event_loop.exit();
            }
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        // Whether egui wants the event to itself, such as a click on one of its windows.
        #[cfg(feature = "egui")]
        let to_gui = state
            .gui_window
            .on_window_event(&state.window, &event)
            .consumed;
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
                state.window.request_redraw();
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => state.key_event(&event, is_synthetic, to_gui),
            WindowEvent::CursorMoved { position, .. } => {
                state.cursor = Some([position.x, position.y])
            }
            WindowEvent::CursorLeft { .. } => state.cursor = None,
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
    surface: WindowSurface,
    screen: Screen,
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
    /// Turns window events into egui's input, and acts on what egui asks of the window.
    #[cfg(feature = "egui")]
    gui_window: egui_winit::State,
}

impl<G: Game> State<G> {
    fn new(
        event_loop: &ActiveEventLoop,
        config: &WindowConfig,
        proxy: EventLoopProxy<()>,
        init: impl FnOnce(&mut Context) -> Result<G>,
    ) -> Result<Self> {
        let timestep = Timestep::new(config.update_rate);
        let attributes = Window::default_attributes()
            .with_title(&config.title)
            .with_inner_size(LogicalSize::new(config.width, config.height))
            .with_resizable(config.resizable);
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
        let gpu = pollster::block_on(Gpu::new(instance, Some(&surface)))?;
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
        // The game is built knowing the canvas size, as it is in a `Simulation`.
        let screen = Screen::new(config.resolution);
        let size = window.inner_size();
        ctx.canvas_size = screen.canvas_size([size.width, size.height], window.scale_factor());
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
            surface,
            screen,
            ctx,
            canvas: Canvas::new(),
            game,
            timestep,
            redraw: config.redraw,
            last_frame: Instant::now(),
            stats: FrameStats::default(),
            cursor: None,
            gamepads,
            #[cfg(feature = "egui")]
            gui_window,
        })
    }

    /// Passes a key event to the game, unless egui wanted it (`to_gui`) or it's the debug key.
    fn key_event(&mut self, event: &KeyEvent, is_synthetic: bool, to_gui: bool) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        let Some(key) = Key::from_winit(code) else {
            return;
        };
        match event.state {
            // Synthetic presses report keys already held when the window gains focus, such as
            // the Alt of an Alt+Tab. They weren't pressed for this game, so they're ignored.
            ElementState::Pressed if !is_synthetic && !to_gui => {
                #[cfg(feature = "egui")]
                if (event.repeat && self.ctx.gui.debug_key == Some(key))
                    || (!event.repeat && self.ctx.gui.debug_key_down(key))
                {
                    return;
                }
                self.ctx.input.key_down(key);
            }
            ElementState::Pressed => {}
            ElementState::Released => self.ctx.input.key_up(key),
        }
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
            .map(|cursor| self.screen.to_canvas(cursor, output, scale_factor));
        self.ctx.input.set_mouse_position(mouse_position);
        if let Some(gamepads) = &mut self.gamepads {
            gamepads.poll(&mut self.ctx.input, self.focused);
        }
        let canvas_size = self.screen.canvas_size(output, scale_factor);

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
        self.gui_window.handle_platform_output(
            &self.window,
            std::mem::take(&mut self.ctx.gui.platform_output),
        );

        let Context {
            gpu,
            renderer,
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
                self.screen.render(
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
}
