//! Draws a tile-sized checkerboard with a square sliding across it.
//!
//! Run with `cargo run --example shapes`.

use wingine::{
    Game, WindowConfig,
    graphics::{Canvas, Color},
    math::Rect,
};

const TILE: f32 = 32.0;

#[derive(Default)]
struct Shapes {
    time: f32,
}

impl Game for Shapes {
    fn update(&mut self, dt: f32) {
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

        let center_x = canvas.width() / 2.0;
        let center_y = canvas.height() / 2.0;
        let x = center_x + self.time.sin() * canvas.width() / 3.0;
        canvas.fill_rect(
            Rect::new(x - TILE / 2.0, center_y - TILE / 2.0, TILE, TILE),
            Color::rgb8(0xe0, 0x6c, 0x4a),
        );

        // Translucent overlay, to show alpha blending.
        canvas.fill_rect(
            Rect::new(
                center_x - 2.0 * TILE,
                center_y - TILE,
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
        Shapes::default(),
    )
}
