//! Sprites bouncing around the window, tuned from an egui panel that also shows a map of them,
//! drawn by the engine. F3 shows the engine's window of frame stats.
//!
//! Run with `cargo run --example gui`.

use wingine::{
    Context, Game, WindowConfig,
    graphics::{Canvas, Color, Sprite, SpriteSheet},
    gui::{GuiCanvas, egui},
    input::Key,
    math::Rect,
};

const SHEET: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/kenney_1-bit-pack_transparent-packed.png"
);
const SIZE: f32 = 16.0;
/// The map's size in pixels.
const MAP: [u32; 2] = [192, 108];
const TINTS: [Color; 4] = [
    Color::rgb8(0xf2, 0xc1, 0x4e),
    Color::rgb8(0x6a, 0xa8, 0x4f),
    Color::rgb8(0x4f, 0x8f, 0xd0),
    Color::rgb8(0xe0, 0x6c, 0x4a),
];

struct Ball {
    position: [f32; 2],
    /// Pixels a second, at a speed of 1.
    velocity: [f32; 2],
    tint: Color,
}

struct Bounce {
    sprite: Sprite,
    balls: Vec<Ball>,
    count: usize,
    speed: f32,
    paused: bool,
    canvas_size: [f32; 2],
    map: GuiCanvas,
    /// For placing new balls, from a xorshift generator.
    seed: u32,
}

impl Bounce {
    fn new(ctx: &mut Context) -> wingine::Result<Self> {
        let sheet = SpriteSheet::new(ctx.load_texture(SHEET)?, 16, 16);
        Ok(Self {
            sprite: sheet.sprite(29, 0),
            balls: Vec::new(),
            count: 50,
            speed: 1.0,
            paused: false,
            canvas_size: ctx.canvas_size(),
            map: GuiCanvas::new(),
            seed: 0x2545_f491,
        })
    }

    /// A number from 0 up to 1.
    fn random(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / (1 << 24) as f32
    }

    fn add_ball(&mut self) {
        let [width, height] = self.canvas_size;
        let position = [
            self.random() * (width - SIZE),
            self.random() * (height - SIZE),
        ];
        let angle = self.random() * std::f32::consts::TAU;
        let speed = 60.0 + self.random() * 120.0;
        let tint = TINTS[self.balls.len() % TINTS.len()];
        self.balls.push(Ball {
            position,
            velocity: [angle.cos() * speed, angle.sin() * speed],
            tint,
        });
    }
}

impl Game for Bounce {
    fn update(&mut self, ctx: &mut Context, dt: f32) {
        self.canvas_size = ctx.canvas_size();
        self.balls.truncate(self.count);
        while self.balls.len() < self.count {
            self.add_ball();
        }
        if self.paused {
            return;
        }
        let bounds = [self.canvas_size[0] - SIZE, self.canvas_size[1] - SIZE];
        for ball in &mut self.balls {
            let axes = ball.position.iter_mut().zip(&mut ball.velocity);
            for ((position, velocity), bound) in axes.zip(bounds) {
                *position += *velocity * self.speed * dt;
                if *position < 0.0 {
                    *position = 0.0;
                    *velocity = velocity.abs();
                } else if *position > bound {
                    *position = bound.max(0.0);
                    *velocity = -velocity.abs();
                }
            }
        }
    }

    fn gui(&mut self, ctx: &mut Context, ui: &mut egui::Ui) {
        // The map: the window's canvas scaled down, drawn by the engine into a texture.
        let canvas = self.map.begin(MAP[0], MAP[1]);
        canvas.clear(Color::rgb8(0x14, 0x18, 0x2c));
        let scale = (MAP[0] as f32 / self.canvas_size[0]).min(MAP[1] as f32 / self.canvas_size[1]);
        for ball in &self.balls {
            let [x, y] = ball.position.map(|v| (v * scale).floor());
            canvas.fill_rect(Rect::new(x, y, 2.0, 2.0), ball.tint);
        }
        let map = self.map.render(ctx);

        egui::Window::new("Bounce")
            .anchor(egui::Align2::RIGHT_TOP, [-16.0, 16.0])
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.add(egui::Slider::new(&mut self.count, 0..=2000).text("Sprites"));
                ui.add(egui::Slider::new(&mut self.speed, 0.0..=4.0).text("Speed"));
                ui.checkbox(&mut self.paused, "Paused");
                // Each of the map's pixels is one of the screen's.
                ui.image(egui::load::SizedTexture::new(
                    map.id,
                    map.size / ui.pixels_per_point(),
                ));
                ui.weak("F3 shows frame stats.");
            });
    }

    fn draw(&mut self, canvas: &mut Canvas) {
        canvas.clear(Color::rgb8(0x1d, 0x23, 0x2b));
        for ball in &self.balls {
            let [x, y] = ball.position;
            canvas.draw_sprite(&self.sprite, Rect::new(x, y, SIZE, SIZE), ball.tint);
        }
    }
}

fn main() -> wingine::Result<()> {
    env_logger::init();
    wingine::run(
        WindowConfig {
            title: "Wingine — egui".into(),
            debug_key: Some(Key::F3),
            ..Default::default()
        },
        Bounce::new,
    )
}
