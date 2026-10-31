use std::{sync::Arc, time::Instant};

use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::PhysicalKey,
    window::{Window, WindowId},
};

use crate::{
    Context, Error, FrameStats, Result,
    audio::Audio,
    graphics::{
        Canvas, Resolution,
        gpu::Gpu,
        renderer::Renderer,
        screen::Screen,
        surface::{Acquired, WindowSurface},
    },
    input::{Key, MouseButton},
};

/// Longest frame time passed to [`Game::update`], so a stall (dragging the window, a breakpoint,
/// being minimized) doesn't make the game jump forward.
const MAX_FRAME_TIME: f32 = 0.25;

/// A game driven by the engine's main loop.
pub trait Game {
    /// Advances the game by `dt` seconds. Called once per frame, before [`Game::draw`].
    fn update(&mut self, _ctx: &mut Context, _dt: f32) {}

    /// Draws the current frame.
    fn draw(&mut self, canvas: &mut Canvas);
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
        }
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
pub fn run<G, F>(config: WindowConfig, init: F) -> Result<()>
where
    G: Game,
    F: FnOnce(&mut Context) -> Result<G>,
{
    if let Some(script) = std::env::var_os(crate::script::ENV_VAR) {
        return crate::script::run(&config, init, std::path::Path::new(&script));
    }

    let event_loop = EventLoop::new().map_err(|err| Error::EventLoop(err.into()))?;
    // Frames are driven by requesting a redraw after each one, paced by vsync. Waiting (rather
    // than polling) lets the loop sleep when no frame is wanted, e.g. while minimized.
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App {
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

/// Runs one frame of `game`, as both the window and a [`Simulation`] do: updates it by `dt` with
/// a canvas `canvas_size` canvas pixels in size, then has it draw on `canvas`. Returns the
/// frame's stats so far: the caller adds rendering and the frame's whole time.
///
/// [`Simulation`]: crate::headless::Simulation
pub(crate) fn run_frame(
    game: &mut impl Game,
    ctx: &mut Context,
    canvas: &mut Canvas,
    canvas_size: [f32; 2],
    dt: f32,
) -> FrameStats {
    ctx.canvas_size = canvas_size;
    let start = Instant::now();
    game.update(ctx, dt);
    let update = start.elapsed().as_secs_f32();
    ctx.input.end_frame();
    ctx.audio.advance(dt);
    let start = Instant::now();
    canvas.begin(canvas_size[0], canvas_size[1]);
    game.draw(canvas);
    FrameStats {
        update,
        draw: start.elapsed().as_secs_f32(),
        quads: canvas.quad_count(),
        draw_calls: canvas.draw_call_count(),
        ..Default::default()
    }
}

struct App<G, F> {
    config: WindowConfig,
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
        match State::new(event_loop, &self.config, init) {
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
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                state.surface.resize(&state.ctx.gpu, size);
                state.window.request_redraw();
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => state.key_event(&event, is_synthetic),
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
                        ElementState::Pressed => state.ctx.input.mouse_down(button),
                        ElementState::Released => state.ctx.input.mouse_up(button),
                    }
                }
            }
            // Releases aren't delivered while unfocused, so treat every key and button as released.
            WindowEvent::Focused(false) => state.ctx.input.release_all(),
            // Frames stop while the window is hidden, so they start again here.
            WindowEvent::Occluded(false) => state.window.request_redraw(),
            WindowEvent::RedrawRequested => state.frame(),
            _ => {}
        }
    }
}

struct State<G> {
    window: Arc<Window>,
    surface: WindowSurface,
    screen: Screen,
    ctx: Context,
    canvas: Canvas,
    game: G,
    last_frame: Instant,
    /// The stats of the last frame, which become the context's once its whole time is known, at
    /// the start of the next.
    stats: FrameStats,
    /// Cursor position in physical window pixels, while it is over the window.
    cursor: Option<[f64; 2]>,
}

impl<G: Game> State<G> {
    fn new(
        event_loop: &ActiveEventLoop,
        config: &WindowConfig,
        init: impl FnOnce(&mut Context) -> Result<G>,
    ) -> Result<Self> {
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
        // The game is built knowing the canvas size, as it is in a `Simulation`.
        let screen = Screen::new(config.resolution);
        let size = window.inner_size();
        ctx.canvas_size = screen.canvas_size([size.width, size.height], window.scale_factor());
        let game = init(&mut ctx)?;

        Ok(Self {
            window,
            surface,
            screen,
            ctx,
            canvas: Canvas::new(),
            game,
            last_frame: Instant::now(),
            stats: FrameStats::default(),
            cursor: None,
        })
    }

    fn key_event(&mut self, event: &KeyEvent, is_synthetic: bool) {
        let PhysicalKey::Code(code) = event.physical_key else {
            return;
        };
        let Some(key) = Key::from_winit(code) else {
            return;
        };
        match event.state {
            // Synthetic presses report keys already held when the window gains focus, such as
            // the Alt of an Alt+Tab. They weren't pressed for this game, so they're ignored.
            ElementState::Pressed if !is_synthetic => self.ctx.input.key_down(key),
            ElementState::Pressed => {}
            ElementState::Released => self.ctx.input.key_up(key),
        }
    }

    fn frame(&mut self) {
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
        let canvas_size = self.screen.canvas_size(output, scale_factor);

        let now = Instant::now();
        let elapsed = (now - self.last_frame).as_secs_f32();
        let dt = elapsed.min(MAX_FRAME_TIME);
        self.last_frame = now;
        self.ctx.frame_stats = FrameStats {
            frame: elapsed,
            ..self.stats
        };
        self.stats = run_frame(
            &mut self.game,
            &mut self.ctx,
            &mut self.canvas,
            canvas_size,
            dt,
        );

        let Context { gpu, renderer, .. } = &mut self.ctx;
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
                self.stats.render = start.elapsed().as_secs_f32();
                self.surface.present(gpu, frame);
            }
            Acquired::Skip => {}
            // Hidden windows aren't paced by vsync, so asking for the next frame at once would
            // spin. `WindowEvent::Occluded(false)` asks for it instead.
            Acquired::Occluded => return,
        }
        self.window.request_redraw();
    }
}
