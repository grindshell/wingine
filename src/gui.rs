//! egui interfaces drawn over a game: debug panels, and tools such as the editor.
//!
//! [`Game::gui`](crate::Game::gui) lays out the game's interface each frame with [egui], after
//! the game updates and before it draws, on a [`egui::Ui`] covering the window. The engine draws
//! the interface over the finished frame at the window's own resolution: over a fixed
//! resolution's scaled canvas and its bars too, so its text stays crisp however small the canvas.
//! Input egui wants, such as a click on one of its windows or keys typed into one of its text
//! fields, doesn't reach the game's [`Input`](crate::input::Input). Releases always do, so nothing
//! is left held. Tab reaches egui only while one of its widgets has keyboard focus, to move it to
//! the next, so a game can use Tab itself while its interface shows ([`Gui::hears_key`]).
//!
//! [`GuiCanvas`] draws with the engine into an image egui shows, such as a view of a scene in a
//! panel, and [`Context::gui_texture`] shows a texture, such as a sprite sheet. Both are sampled
//! nearest-neighbor, so pixel art stays crisp.
//!
//! [`WindowConfig::debug_key`](crate::WindowConfig::debug_key) names a key that shows and hides
//! the engine's own window of frame stats, in any game, whether or not it has an interface. The
//! key doesn't reach the game.
//!
//! In a window that [redraws on demand](crate::Redraw::OnDemand), egui's requests to repaint, from
//! its animations or from code on any thread, ask for frames.
//!
//! A [`Simulation`](crate::headless::Simulation) does the same with its simulated input, and its
//! screenshots show the interface. Tests find widgets by their labels with
//! [`Simulation::gui_rect`](crate::headless::Simulation::gui_rect).
//!
//! The engine re-exports the egui it's built with, so games use the same version.

use std::{cell::RefCell, collections::VecDeque, mem, rc::Rc};

pub use egui;
use egui::load::SizedTexture;

use crate::{
    Context, Game,
    graphics::{Canvas, Color, Texture, WeakTexture, renderer::RenderTarget},
    input::{Key, MouseButton},
    math::Rect,
};

/// How many frames the debug window's graph shows.
const GRAPH_FRAMES: usize = 120;

/// The engine's side of egui: its context, the input for its next pass, and what its last pass
/// drew until it's rendered.
pub(crate) struct Gui {
    pub egui: egui::Context,
    /// What egui renders to: the output's format without sRGB encoding, since egui blends in
    /// sRGB space.
    format: wgpu::TextureFormat,
    options: egui_wgpu::RendererOptions,
    /// Created when there's first something to draw, so games without an interface don't pay
    /// for it.
    renderer: Option<egui_wgpu::Renderer>,
    /// The input for the next pass.
    input: egui::RawInput,
    /// What the last pass drew, until it's rendered.
    primitives: Vec<egui::ClippedPrimitive>,
    pixels_per_point: f32,
    /// Changes to egui's textures not yet made, kept until the renderer exists.
    textures: egui::TexturesDelta,
    /// What the last pass asked of the window, such as a cursor or text to copy.
    pub platform_output: egui::PlatformOutput,
    /// The part of the screen the last pass left to the game, outside its panels, in points.
    free_rect: Option<egui::Rect>,
    /// The widgets the last pass laid out, by label, in points, when labels are enabled.
    labels: Vec<(String, egui::Rect)>,
    /// Engine textures shown in egui, and the IDs they're shown with.
    shown: Vec<Shown>,
    /// The IDs of dropped [`GuiCanvas`]es, to free.
    dropped: Rc<RefCell<Vec<egui::TextureId>>>,
    pub debug_key: Option<Key>,
    overlay: Overlay,
    /// Whether the game lays out an interface of its own, until [`Game::gui`]'s default says it
    /// doesn't, or a Luau game has no global `gui`, which it may define later.
    pub game_has_gui: bool,
    /// Whether the last pass had nothing to show, so passes can stop until there's something.
    idle: bool,
}

impl Gui {
    /// An interface drawn on outputs whose sRGB views have the format `output`.
    pub fn new(output: wgpu::TextureFormat) -> Self {
        Self {
            egui: egui::Context::default(),
            format: output.remove_srgb_suffix(),
            options: egui_wgpu::RendererOptions::default(),
            renderer: None,
            input: egui::RawInput::default(),
            primitives: Vec::new(),
            pixels_per_point: 1.0,
            textures: egui::TexturesDelta::default(),
            platform_output: egui::PlatformOutput::default(),
            free_rect: None,
            labels: Vec::new(),
            shown: Vec::new(),
            dropped: Rc::default(),
            debug_key: None,
            overlay: Overlay::default(),
            game_has_gui: true,
            idle: false,
        }
    }

    /// Renders the same pixels on any GPU, as screenshot tests want: without dithering, which
    /// adds noise to gradients.
    pub fn set_predictable(&mut self) {
        self.options.dithering = false;
    }

    /// Keeps the labels of the widgets each pass lays out, for [`Gui::labelled`].
    pub fn enable_labels(&mut self) {
        self.egui.enable_accesskit();
    }

    /// Sets the input for the next pass.
    pub fn set_input(&mut self, input: egui::RawInput) {
        self.input = input;
    }

    /// Shows or hides the debug window if `key` is the debug key, and tells whether it was, so
    /// the game doesn't see it.
    pub fn debug_key_down(&mut self, key: Key) -> bool {
        let is_debug_key = self.debug_key == Some(key);
        if is_debug_key {
            self.overlay.shown = !self.overlay.shown;
        }
        is_debug_key
    }

    /// Whether egui wants a press of the pointer at `pos`, in points: one over its windows or
    /// panels, or while it's dragging something. The last pass decides.
    pub fn wants_pointer_at(&self, pos: egui::Pos2) -> bool {
        if self.egui.egui_is_using_pointer() {
            return true;
        }
        match self.egui.layer_id_at(pos) {
            None => false,
            // Panels are on the background, and the space they leave is the game's.
            Some(layer) if layer.order == egui::Order::Background => {
                !self.free_rect.is_some_and(|rect| rect.contains(pos))
            }
            Some(_) => true,
        }
    }

    /// Whether egui wants keys pressed now, as a text field with focus does.
    pub fn wants_keyboard(&self) -> bool {
        self.egui.egui_wants_keyboard_input()
    }

    /// Whether egui hears `key` pressed or released: every key but Tab, and Tab only while one of
    /// its widgets has keyboard focus, to move it to the next. egui answers Tab by focusing its
    /// first widget even when none has focus, where the keys after it would then go, so a game
    /// showing an interface couldn't use Tab itself.
    pub fn hears_key(&self, key: Key) -> bool {
        key != Key::Tab || self.wants_keyboard()
    }

    /// The rectangles, in points, of the widgets labelled `label` in the last pass, row by row
    /// from the top, and left to right along a row. A widget's label is its text, such as a
    /// button's or a label's, what it's [labelled by](egui::Response::labelled_by), or a text
    /// field's hint.
    pub fn labelled(&self, label: &str) -> Vec<egui::Rect> {
        let mut rects: Vec<_> = self
            .labels
            .iter()
            .filter(|(candidate, _)| candidate == label)
            .map(|(_, rect)| *rect)
            .collect();
        // Widgets of a row differ in height, so rows are told apart by their middles: one whose
        // middle is level with the last's is on its row.
        rects.sort_by(|a, b| a.center().y.total_cmp(&b.center().y));
        let mut row = 0;
        let mut rows = Vec::with_capacity(rects.len());
        for (i, rect) in rects.iter().enumerate() {
            if i > 0 {
                let above = rects[i - 1];
                if rect.center().y - above.center().y > above.height().min(rect.height()) / 2.0 {
                    row += 1;
                }
            }
            rows.push((row, *rect));
        }
        rows.sort_by(|(a_row, a), (b_row, b)| a_row.cmp(b_row).then(a.min.x.total_cmp(&b.min.x)));
        rows.into_iter().map(|(_, rect)| rect).collect()
    }

    fn renderer(&mut self, device: &wgpu::Device) -> &mut egui_wgpu::Renderer {
        self.renderer
            .get_or_insert_with(|| egui_wgpu::Renderer::new(device, self.format, self.options))
    }

    /// Takes in what a pass drew.
    fn finish(&mut self, output: egui::FullOutput, free_rect: Option<egui::Rect>) {
        self.free_rect = free_rect;
        self.pixels_per_point = output.pixels_per_point;
        self.primitives = self.egui.tessellate(output.shapes, output.pixels_per_point);
        self.textures.append(output.textures_delta);
        self.platform_output = output.platform_output;
        if let Some(update) = &self.platform_output.accesskit_update {
            self.labels.clear();
            for (_, node) in &update.nodes {
                let Some(bounds) = node.bounds() else {
                    continue;
                };
                let rect = egui::Rect::from_min_max(
                    egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                    egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                );
                // A label's text is its value, and a text field's hint its placeholder.
                let text = (node.role() == egui::accesskit::Role::Label)
                    .then(|| node.value())
                    .flatten();
                let names = [node.label(), text, node.placeholder()];
                for name in names.into_iter().flatten() {
                    self.labels.push((name.to_owned(), rect));
                }
            }
        }
    }

    /// Draws what the last pass drew over `target`, which holds a finished frame.
    pub fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, target: &wgpu::Texture) {
        if self.renderer.is_none() && self.primitives.is_empty() {
            return;
        }
        let format = self.format;
        let pixels_per_point = self.pixels_per_point;
        let mut textures = mem::take(&mut self.textures);
        let dropped = mem::take(&mut *self.dropped.borrow_mut());
        let mut shown = mem::take(&mut self.shown);
        let primitives = mem::take(&mut self.primitives);
        let renderer = self.renderer(device);

        for id in &dropped {
            renderer.free_texture(id);
        }
        shown.retain(|shown| {
            let alive = shown.texture.is_alive();
            if !alive {
                renderer.free_texture(&shown.id);
            }
            alive
        });
        // Written with the queue's next submission, whichever that is.
        for (id, deltas) in &textures.set {
            for delta in deltas {
                renderer.update_texture(device, queue, *id, delta);
            }
        }

        // A pass that drew nothing, as one with the interface closed, has nothing to submit.
        if !primitives.is_empty() {
            let screen = egui_wgpu::ScreenDescriptor {
                size_in_pixels: [target.width(), target.height()],
                pixels_per_point,
            };
            let mut encoder = device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("gui") });
            let mut commands =
                renderer.update_buffers(device, queue, &mut encoder, &primitives, &screen);
            let view = target.create_view(&wgpu::TextureViewDescriptor {
                format: Some(format),
                ..Default::default()
            });
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("gui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            renderer.render(&mut pass, &primitives, &screen);
            drop(pass);
            commands.push(encoder.finish());
            queue.submit(commands);
        }
        for id in &textures.free {
            renderer.free_texture(id);
        }
        // Every change has been made.
        textures.clear();

        self.shown = shown;
        self.primitives = primitives;
    }
}

impl Drop for Gui {
    fn drop(&mut self) {
        // Changes to textures never made, since nothing was ever drawn, or since the last frame
        // rendered, go with the renderer.
        self.textures.clear();
    }
}

/// Runs `game`'s interface for one frame, with the input set by [`Gui::set_input`], and keeps
/// what it drew to render. Games without an interface skip it while the debug window is closed.
pub(crate) fn run_pass(ctx: &mut Context, game: &mut impl Game) {
    let overlay = &mut ctx.gui.overlay;
    if overlay.frames.len() == GRAPH_FRAMES {
        overlay.frames.pop_front();
    }
    overlay.frames.push_back(ctx.frame_stats.frame);
    let input = mem::take(&mut ctx.gui.input);
    let idle = !ctx.gui.game_has_gui && !ctx.gui.overlay.shown;
    if idle && ctx.gui.idle {
        return;
    }
    // A pass still runs on the first frame with nothing to show, so egui forgets the windows it
    // showed before and doesn't keep the mouse from the game where they were.
    ctx.gui.idle = idle;
    let egui = ctx.gui.egui.clone();
    let mut free_rect = None;
    let output = egui.run_ui(input, |ui| {
        game.gui(ctx, ui);
        // Panels the game added have taken their space by now.
        free_rect = Some(ui.available_rect_before_wrap());
        show_overlay(ctx, ui.ctx());
    });
    ctx.gui.finish(output, free_rect);
}

/// The engine's debug window: how long frames take, and how much they draw.
#[derive(Default)]
struct Overlay {
    shown: bool,
    /// The last frames' times, in seconds, oldest first.
    frames: VecDeque<f32>,
    /// The GPU adapter's name, once it's been asked for.
    adapter: Option<String>,
}

fn show_overlay(ctx: &mut Context, egui: &egui::Context) {
    let stats = ctx.frame_stats;
    let overlay = &mut ctx.gui.overlay;
    if !overlay.shown {
        return;
    }
    let adapter = overlay.adapter.get_or_insert_with(|| {
        let info = ctx.gpu.adapter.get_info();
        format!("{} ({:?})", info.name, info.backend)
    });
    let [width, height] = ctx.canvas_size;
    let ms = |seconds: f32| format!("{:.2} ms", seconds * 1000.0);
    egui::Window::new("Wingine")
        .default_pos([8.0, 8.0])
        .resizable(false)
        .show(egui, |ui| {
            egui::Grid::new("frame stats")
                .num_columns(2)
                .show(ui, |ui| {
                    let rows = [
                        ("Frame", ms(stats.frame)),
                        ("Update", ms(stats.update)),
                        ("Draw", ms(stats.draw)),
                        ("Render", ms(stats.render)),
                        ("Quads", stats.quads.to_string()),
                        ("Triangles", stats.triangles.to_string()),
                        ("Draw calls", stats.draw_calls.to_string()),
                        ("Canvas", format!("{width} x {height}")),
                    ];
                    for (name, value) in rows {
                        ui.label(name);
                        ui.monospace(value);
                        ui.end_row();
                    }
                });
            frame_graph(ui, &overlay.frames);
            ui.small(adapter.as_str());
        });
}

/// A graph of frame times, with a line at 60 fps.
fn frame_graph(ui: &mut egui::Ui, frames: &VecDeque<f32>) {
    const TARGET: f32 = 1.0 / 60.0;
    let size = egui::vec2(GRAPH_FRAMES as f32 * 2.0, 48.0);
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(rect, 2.0, visuals.extreme_bg_color);
    // Scaled so 60 fps is halfway up, and slower frames still show.
    let top = frames.iter().copied().fold(TARGET * 2.0, f32::max);
    let y = |seconds: f32| rect.bottom() - rect.height() * seconds / top;
    painter.hline(
        rect.x_range(),
        y(TARGET),
        egui::Stroke::new(1.0, visuals.weak_text_color()),
    );
    let step = rect.width() / GRAPH_FRAMES as f32;
    let points = frames
        .iter()
        .enumerate()
        .map(|(i, &seconds)| egui::pos2(rect.left() + i as f32 * step, y(seconds)))
        .collect();
    painter.add(egui::Shape::line(
        points,
        egui::Stroke::new(1.0, visuals.strong_text_color()),
    ));
}

/// A canvas the engine renders into an image for egui to show, such as a view of a scene in one
/// of its panels.
///
/// Each frame, [`GuiCanvas::begin`] a frame of the size to show, draw on the canvas it returns,
/// then [`GuiCanvas::render`] it and show the texture that returns, as with
/// `ui.image(texture)`. Its texture is kept while frames stay the same size.
pub struct GuiCanvas {
    canvas: Canvas,
    target: Option<Target>,
}

/// The texture a [`GuiCanvas`] renders into, and its ID in egui.
struct Target {
    render: RenderTarget,
    size: [u32; 2],
    id: egui::TextureId,
    /// Where its ID goes when it's dropped, to be freed.
    dropped: Rc<RefCell<Vec<egui::TextureId>>>,
}

impl Drop for Target {
    fn drop(&mut self) {
        self.dropped.borrow_mut().push(self.id);
    }
}

impl Default for GuiCanvas {
    fn default() -> Self {
        Self::new()
    }
}

impl GuiCanvas {
    pub fn new() -> Self {
        Self {
            canvas: Canvas::new(),
            target: None,
        }
    }

    /// Starts a new frame `width` by `height` pixels in size, at least 1 by 1, cleared to black,
    /// and returns the canvas to draw it on.
    pub fn begin(&mut self, width: u32, height: u32) -> &mut Canvas {
        self.canvas.begin(width.max(1) as f32, height.max(1) as f32);
        &mut self.canvas
    }

    /// The canvas of the frame begun last.
    pub fn canvas(&mut self) -> &mut Canvas {
        &mut self.canvas
    }

    /// Renders what was drawn since [`GuiCanvas::begin`], and returns the texture holding it,
    /// sized in pixels: show it at that size divided by
    /// [`egui::Context::pixels_per_point`] to show each pixel as one of the screen's. A frame
    /// wider or taller than the GPU's largest texture renders at that size, and is stretched to
    /// its own.
    pub fn render(&mut self, ctx: &mut Context) -> SizedTexture {
        let size = [self.canvas.width(), self.canvas.height()].map(|v| v as u32);
        let Context {
            gpu, renderer, gui, ..
        } = ctx;
        let device = &gpu.device;
        let limit = device.limits().max_texture_dimension_2d;
        let texture_size = size.map(|v| v.clamp(1, limit));
        if self
            .target
            .as_ref()
            .is_none_or(|target| target.size != texture_size)
        {
            let [width, height] = texture_size;
            let render = renderer.create_render_target(device, width, height);
            let gui_renderer = gui.renderer(device);
            match &mut self.target {
                Some(target) => {
                    gui_renderer.update_egui_texture_from_wgpu_texture(
                        device,
                        render.texture.gui_view(),
                        wgpu::FilterMode::Nearest,
                        target.id,
                    );
                    target.render = render;
                    target.size = texture_size;
                }
                None => {
                    let id = gui_renderer.register_native_texture(
                        device,
                        render.texture.gui_view(),
                        wgpu::FilterMode::Nearest,
                    );
                    self.target = Some(Target {
                        render,
                        size: texture_size,
                        id,
                        dropped: gui.dropped.clone(),
                    });
                }
            }
        }
        let target = self.target.as_ref().expect("created above");
        let view = &target.render.view;
        renderer.render(device, &gpu.queue, &self.canvas, view, texture_size);
        SizedTexture::new(target.id, egui::vec2(size[0] as f32, size[1] as f32))
    }
}

impl Context {
    /// `texture`, for egui to show, sized in texels. Showing the same texture again reuses its
    /// ID, until the texture is dropped.
    ///
    /// egui shows a copy of the texture as it was when first shown, with its colors multiplied
    /// by their alpha, as egui blends them.
    pub fn gui_texture(&mut self, texture: &Texture) -> SizedTexture {
        let size = egui::vec2(texture.width() as f32, texture.height() as f32);
        let shown = self
            .gui
            .shown
            .iter()
            .find(|shown| shown.texture.is_alive() && shown.texture.is(texture));
        if let Some(shown) = shown {
            return SizedTexture::new(shown.id, size);
        }
        let Context {
            gpu, renderer, gui, ..
        } = self;
        let device = &gpu.device;
        // Drawn through the quad pipeline, which multiplies colors by their alpha, onto a clear
        // target.
        let [width, height] = [texture.width(), texture.height()];
        let copy = renderer.create_render_target(device, width, height);
        let mut canvas = Canvas::new();
        canvas.begin(width as f32, height as f32);
        canvas.clear(Color::TRANSPARENT);
        canvas.draw_sprite(
            &texture.sprite(),
            Rect::new(0.0, 0.0, size.x, size.y),
            Color::WHITE,
        );
        renderer.render(device, &gpu.queue, &canvas, &copy.view, [width, height]);
        let id = gui.renderer(device).register_native_texture(
            device,
            copy.texture.gui_view(),
            wgpu::FilterMode::Nearest,
        );
        gui.shown.push(Shown {
            texture: texture.downgrade(),
            id,
            _copy: copy,
        });
        SizedTexture::new(id, size)
    }
}

/// An engine texture shown in egui, by [`Context::gui_texture`].
struct Shown {
    texture: WeakTexture,
    id: egui::TextureId,
    /// The copy egui shows, with its colors premultiplied.
    _copy: RenderTarget,
}

/// Input for egui gathered between a simulation's frames, from its simulated keys, mouse and
/// typing.
#[derive(Default)]
pub(crate) struct SimulatedInput {
    events: Vec<egui::Event>,
    /// Where the pointer is, in points.
    pointer: Option<egui::Pos2>,
    modifiers: egui::Modifiers,
}

impl SimulatedInput {
    /// Presses or releases `key`, or with `repeat`, repeats it.
    pub fn key(&mut self, key: Key, pressed: bool, repeat: bool) {
        let modifiers = &mut self.modifiers;
        match key {
            Key::LeftShift | Key::RightShift => modifiers.shift = pressed,
            // The command key is Control here on every platform, so tests run the same on all.
            Key::LeftControl | Key::RightControl => {
                modifiers.ctrl = pressed;
                modifiers.command = pressed;
            }
            Key::LeftAlt | Key::RightAlt => modifiers.alt = pressed,
            _ => {}
        }
        match egui_key(key) {
            Some(egui_key) => self.events.push(egui::Event::Key {
                key: egui_key,
                physical_key: Some(egui_key),
                pressed,
                repeat,
                modifiers: self.modifiers,
            }),
            None => self
                .events
                .push(egui::Event::ModifiersChanged(self.modifiers)),
        }
    }

    pub fn pointer(&self) -> Option<egui::Pos2> {
        self.pointer
    }

    pub fn move_pointer(&mut self, pos: egui::Pos2) {
        self.pointer = Some(pos);
        self.events.push(egui::Event::PointerMoved(pos));
    }

    pub fn button(&mut self, button: MouseButton, pressed: bool) {
        let (Some(pos), Some(button)) = (self.pointer, pointer_button(button)) else {
            return;
        };
        self.events.push(egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: self.modifiers,
        });
    }

    pub fn text(&mut self, text: &str) {
        self.events.push(egui::Event::Text(text.to_owned()));
    }

    /// Turns the mouse wheel by `delta` notches, which egui calls lines.
    pub fn scroll_lines(&mut self, delta: egui::Vec2) {
        self.events.push(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta,
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: self.modifiers,
        });
    }

    /// The events gathered since this was last called.
    pub fn take_events(&mut self) -> Vec<egui::Event> {
        mem::take(&mut self.events)
    }
}

fn pointer_button(button: MouseButton) -> Option<egui::PointerButton> {
    match button {
        MouseButton::Left => Some(egui::PointerButton::Primary),
        MouseButton::Right => Some(egui::PointerButton::Secondary),
        MouseButton::Middle => Some(egui::PointerButton::Middle),
        MouseButton::Back => Some(egui::PointerButton::Extra1),
        MouseButton::Forward => Some(egui::PointerButton::Extra2),
    }
}

/// egui's name for `key`, if it has one. egui has none for modifiers.
fn egui_key(key: Key) -> Option<egui::Key> {
    use egui::Key as E;
    let key = match key {
        Key::A => E::A,
        Key::B => E::B,
        Key::C => E::C,
        Key::D => E::D,
        Key::E => E::E,
        Key::F => E::F,
        Key::G => E::G,
        Key::H => E::H,
        Key::I => E::I,
        Key::J => E::J,
        Key::K => E::K,
        Key::L => E::L,
        Key::M => E::M,
        Key::N => E::N,
        Key::O => E::O,
        Key::P => E::P,
        Key::Q => E::Q,
        Key::R => E::R,
        Key::S => E::S,
        Key::T => E::T,
        Key::U => E::U,
        Key::V => E::V,
        Key::W => E::W,
        Key::X => E::X,
        Key::Y => E::Y,
        Key::Z => E::Z,
        Key::Digit0 | Key::Numpad0 => E::Num0,
        Key::Digit1 | Key::Numpad1 => E::Num1,
        Key::Digit2 | Key::Numpad2 => E::Num2,
        Key::Digit3 | Key::Numpad3 => E::Num3,
        Key::Digit4 | Key::Numpad4 => E::Num4,
        Key::Digit5 | Key::Numpad5 => E::Num5,
        Key::Digit6 | Key::Numpad6 => E::Num6,
        Key::Digit7 | Key::Numpad7 => E::Num7,
        Key::Digit8 | Key::Numpad8 => E::Num8,
        Key::Digit9 | Key::Numpad9 => E::Num9,
        Key::Up => E::ArrowUp,
        Key::Down => E::ArrowDown,
        Key::Left => E::ArrowLeft,
        Key::Right => E::ArrowRight,
        Key::Space => E::Space,
        Key::Enter | Key::NumpadEnter => E::Enter,
        Key::Escape => E::Escape,
        Key::Tab => E::Tab,
        Key::Backspace => E::Backspace,
        Key::Insert => E::Insert,
        Key::Delete => E::Delete,
        Key::Home => E::Home,
        Key::End => E::End,
        Key::PageUp => E::PageUp,
        Key::PageDown => E::PageDown,
        Key::Minus => E::Minus,
        Key::Equal => E::Equals,
        Key::Comma => E::Comma,
        Key::Period => E::Period,
        Key::Slash => E::Slash,
        Key::Semicolon => E::Semicolon,
        Key::Quote => E::Quote,
        Key::Backquote => E::Backtick,
        Key::Backslash => E::Backslash,
        Key::LeftBracket => E::OpenBracket,
        Key::RightBracket => E::CloseBracket,
        Key::F1 => E::F1,
        Key::F2 => E::F2,
        Key::F3 => E::F3,
        Key::F4 => E::F4,
        Key::F5 => E::F5,
        Key::F6 => E::F6,
        Key::F7 => E::F7,
        Key::F8 => E::F8,
        Key::F9 => E::F9,
        Key::F10 => E::F10,
        Key::F11 => E::F11,
        Key::F12 => E::F12,
        Key::LeftShift
        | Key::RightShift
        | Key::LeftControl
        | Key::RightControl
        | Key::LeftAlt
        | Key::RightAlt => return None,
    };
    Some(key)
}

/// A rectangle in points as one in canvas pixels, mapped through `to_canvas`.
pub(crate) fn rect_to_canvas(rect: egui::Rect, to_canvas: impl Fn(egui::Pos2) -> [f32; 2]) -> Rect {
    let [x0, y0] = to_canvas(rect.min);
    let [x1, y1] = to_canvas(rect.max);
    Rect::new(x0, y0, x1 - x0, y1 - y0)
}
