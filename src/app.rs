use std::{sync::Arc, time::Instant};

use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

use crate::{
    Error, Result,
    graphics::{Canvas, gpu::Gpu, renderer::Renderer},
};

/// Longest frame time passed to [`Game::update`], so a stall (dragging the window, a breakpoint,
/// being minimized) doesn't make the game jump forward.
const MAX_FRAME_TIME: f32 = 0.25;

/// A game driven by the engine's main loop.
pub trait Game {
    /// Advances the game by `dt` seconds. Called once per frame, before [`Game::draw`].
    fn update(&mut self, _dt: f32) {}

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
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Wingine".into(),
            width: 1280,
            height: 720,
            resizable: true,
        }
    }
}

/// Opens a window and runs `game` until the window is closed.
pub fn run(config: WindowConfig, game: impl Game) -> Result<()> {
    let event_loop = EventLoop::new()?;
    // Frames are driven by requesting a redraw after each one, paced by vsync. Waiting (rather
    // than polling) lets the loop sleep when no frame is wanted, e.g. while minimized.
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App {
        config,
        game,
        state: None,
        error: None,
    };
    event_loop.run_app(&mut app)?;
    app.error.map_or(Ok(()), Err)
}

struct App<G> {
    config: WindowConfig,
    game: G,
    /// Created once the event loop starts, since windows can't be created before then.
    state: Option<State>,
    /// A startup failure, returned from [`run`] after the event loop exits.
    error: Option<Error>,
}

impl<G: Game> ApplicationHandler for App<G> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        match State::new(event_loop, &self.config) {
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
                state.gpu.resize(size);
                state.window.request_redraw();
            }
            WindowEvent::RedrawRequested => state.frame(&mut self.game),
            _ => {}
        }
    }
}

struct State {
    window: Arc<Window>,
    gpu: Gpu,
    renderer: Renderer,
    canvas: Canvas,
    last_frame: Instant,
}

impl State {
    fn new(event_loop: &ActiveEventLoop, config: &WindowConfig) -> Result<Self> {
        let attributes = Window::default_attributes()
            .with_title(&config.title)
            .with_inner_size(LogicalSize::new(config.width, config.height))
            .with_resizable(config.resizable);
        let window = Arc::new(event_loop.create_window(attributes)?);
        let gpu = pollster::block_on(Gpu::new(event_loop.owned_display_handle(), window.clone()))?;
        let renderer = Renderer::new(&gpu.device, gpu.view_format());

        Ok(Self {
            window,
            gpu,
            renderer,
            canvas: Canvas::new(),
            last_frame: Instant::now(),
        })
    }

    fn frame(&mut self, game: &mut impl Game) {
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            // Minimized. The next `Resized` event requests a redraw.
            return;
        }

        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f32().min(MAX_FRAME_TIME);
        self.last_frame = now;
        game.update(dt);

        let logical = size.to_logical::<f32>(self.window.scale_factor());
        self.canvas.begin(logical.width, logical.height);
        game.draw(&mut self.canvas);

        if let Some(frame) = self.gpu.acquire_frame() {
            let view = frame.texture.create_view(&wgpu::TextureViewDescriptor {
                format: Some(self.gpu.view_format()),
                ..Default::default()
            });
            self.renderer
                .render(&self.gpu.device, &self.gpu.queue, &self.canvas, &view);
            self.gpu.present(frame);
        }
        self.window.request_redraw();
    }
}
