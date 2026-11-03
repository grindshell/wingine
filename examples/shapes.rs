//! Draws a tile-sized checkerboard with a square sliding across it, a ball circling the middle on
//! the end of a line, and a translucent panel over them all.
//!
//! Run with `cargo run --example shapes`.

use wingine::{
    Context, Game, WindowConfig,
    graphics::{Canvas, Color},
    math::Rect,
};

const TILE: f32 = 32.0;
/// How far from the middle the ball circles, its radius, and how fast it goes round, in radians
/// a second.
const ORBIT: f32 = 120.0;
const BALL: f32 = 10.0;
const BALL_SPEED: f32 = 1.5;

#[derive(Default)]
struct Shapes {
    time: f32,
}

impl Game for Shapes {
    fn update(&mut self, _ctx: &mut Context, dt: f32) {
        self.time += dt;
    }

    fn draw(&mut self, canvas: &mut Canvas) {
        canvas.clear(Color::rgb8(0x1d, 0x23, 0x2b));

        let cols = (canvas.width() / TILE).ceil() as u32;
        let rows = (canvas.height() / TILE).ceil() as u32;
        for row in 0..rows {
            for col in 0..cols {
                if (row + col) % 2 == 0 {
                    let tile = Rect::new(col as f32 * TILE, row as f32 * TILE, TILE, TILE);
                    canvas.fill_rect(tile, Color::rgb8(0x2e, 0x36, 0x40));
                }
            }
        }

        let center = [canvas.width() / 2.0, canvas.height() / 2.0];

        // The ball's path, then the line out to it and the ball.
        let path = Color::rgb8(0x4a, 0x56, 0x66);
        canvas.outline_circle(center, ORBIT + 1.0, 2.0, path);
        let (sin, cos) = (self.time * BALL_SPEED).sin_cos();
        let ball = [center[0] + cos * ORBIT, center[1] + sin * ORBIT];
        canvas.draw_line(center, ball, 2.0, Color::rgb8(0x8a, 0x96, 0xa6));
        canvas.fill_circle(center, 4.0, Color::rgb8(0x8a, 0x96, 0xa6));
        canvas.fill_circle(ball, BALL, Color::rgb8(0x5a, 0xb4, 0xe0));

        let x = center[0] + self.time.sin() * canvas.width() / 3.0;
        let square = Rect::new(x - TILE / 2.0, center[1] - TILE / 2.0, TILE, TILE);
        canvas.fill_rect(square, Color::rgb8(0xe0, 0x6c, 0x4a));
        canvas.outline_rect(square, 2.0, Color::rgb8(0xf4, 0xb0, 0x8c));

        // Translucent overlay, to show alpha blending.
        canvas.fill_rect(
            Rect::new(
                center[0] - 2.0 * TILE,
                center[1] - TILE,
                4.0 * TILE,
                2.0 * TILE,
            ),
            Color::rgba(1.0, 1.0, 1.0, 0.25),
        );
    }
}

fn main() -> wingine::Result<()> {
    env_logger::init();
    wingine::run(
        WindowConfig {
            title: "Wingine - shapes".into(),
            ..Default::default()
        },
        |_| Ok(Shapes::default()),
    )
}
