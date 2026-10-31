//! What drawing a sprite costs: from Rust, from Luau, and through a native library. Run with
//! `cargo bench`.
//!
//! Each case draws [`SPRITES`] sprites a frame in a [`Simulation`], and reports the median time
//! `draw` took over [`FRAMES`] frames, per sprite, from the frame stats. A last case renders the
//! frames too, for what rendering that many quads costs.

use std::{fs, path::PathBuf};

use wingine::{
    Context, FrameStats, Game, WindowConfig,
    graphics::{Canvas, Color, Sprite},
    headless::Simulation,
    math::Rect,
    runtime::{self, LuauGame},
};
use wingine_native as native;

const SPRITES: u32 = 10_000;
/// Frames run before measuring, so caches and buffers have warmed up.
const WARMUP: u32 = 30;
const FRAMES: u32 = 120;
/// The sprite's size, in texels.
const SIZE: u32 = 8;

/// A 320x180 canvas, with sprites spread over it.
fn config() -> WindowConfig {
    WindowConfig {
        width: 320,
        height: 180,
        ..Default::default()
    }
}

/// Where the `i`th sprite goes, as the Luau cases place it too.
fn position(i: u32) -> [f32; 2] {
    [(i * 7 % 320) as f32, (i * 13 % 180) as f32]
}

fn main() {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("drawing-bench");
    fs::create_dir_all(&folder).unwrap();
    let image = folder.join("sprite.png");
    let pixels = [0xff; (SIZE * SIZE * 4) as usize];
    image::save_buffer(&image, &pixels, SIZE, SIZE, image::ExtendedColorType::Rgba8).unwrap();
    runtime::link_library("bench", native::library::<Bench>());

    let mut simulation = Simulation::new(&config(), |ctx| RustGame::load(ctx, &image, false))
        .expect("a GPU adapter is available");
    println!(
        "Drawing {SPRITES} sprites a frame on {}, median of {FRAMES} frames:",
        simulation.adapter_name()
    );
    report("Rust: Canvas::draw_sprite", draw_time(&mut simulation));
    let mut simulation =
        Simulation::new(&config(), |ctx| RustGame::load(ctx, &image, true)).unwrap();
    report("Rust: Canvas::fill_rect", draw_time(&mut simulation));

    let luau = |name: &str, draw: String| {
        let game = folder.join(name);
        fs::create_dir_all(&game).unwrap();
        fs::copy(&image, game.join("sprite.png")).unwrap();
        fs::write(game.join("conf.luau"), "return { libraries = { 'bench' } }").unwrap();
        fs::write(game.join("main.luau"), luau_game(&draw)).unwrap();
        let mut simulation = Simulation::new(&config(), |ctx| LuauGame::load(ctx, &game)).unwrap();
        let time = draw_time(&mut simulation);
        if let Some(err) = simulation.game().error() {
            panic!("{name}: {err}");
        }
        time
    };
    report(
        "Native: one call drawing every sprite through the ABI",
        luau("native-many", "bench.draw_many(SPRITES)".into()),
    );
    report(
        "Luau: graphics.draw_sprite(sprite, x, y)",
        luau("draw-sprite", each("graphics.draw_sprite(sprite, x, y)")),
    );
    report(
        "Luau: graphics.draw_sprite(sprite, x, y, \"#ff8040\")",
        luau(
            "draw-sprite-hex",
            each("graphics.draw_sprite(sprite, x, y, '#ff8040')"),
        ),
    );
    report(
        "Luau: graphics.draw_sprite(sprite, x, y, { r, g, b })",
        luau(
            "draw-sprite-table",
            each("graphics.draw_sprite(sprite, x, y, TINT)"),
        ),
    );
    report(
        "Luau: graphics.fill_rect(x, y, 8, 8, \"#ff8040\")",
        luau(
            "fill-rect",
            each("graphics.fill_rect(x, y, 8, 8, '#ff8040')"),
        ),
    );
    report(
        "Luau: a native library function per sprite",
        luau("native-one", each("bench.draw_one(x, y)")),
    );
    report(
        "Luau: an empty Luau function per sprite",
        luau("luau-call", each("nothing(sprite, x, y)")),
    );

    let mut simulation =
        Simulation::new(&config(), |ctx| RustGame::load(ctx, &image, false)).unwrap();
    simulation.set_render_every_frame(true);
    let render = median(&mut simulation, |stats| stats.render);
    let frame = median(&mut simulation, |stats| stats.frame);
    println!(
        "Rendering {SPRITES} sprites: {:.3} ms on the CPU, {:.3} ms for the whole frame waiting \
         for the GPU",
        render * 1000.0,
        frame * 1000.0
    );
    if cfg!(debug_assertions) {
        println!("(a debug build: run `cargo bench` for one built for speed)");
    }
}

fn report(case: &str, draw: f32) {
    let per_sprite = draw / SPRITES as f32 * 1e9;
    println!(
        "  {case:<56}{per_sprite:>8.1} ns a sprite ({:.3} ms a frame)",
        draw * 1000.0
    );
}

/// The median time the game took to draw a frame.
fn draw_time<G: Game>(simulation: &mut Simulation<G>) -> f32 {
    median(simulation, |stats| stats.draw)
}

/// The median of `stat` over [`FRAMES`] frames, after warming up.
fn median<G: Game>(simulation: &mut Simulation<G>, stat: fn(&FrameStats) -> f32) -> f32 {
    simulation.run_frames(WARMUP);
    let mut values: Vec<f32> = (0..FRAMES)
        .map(|_| {
            simulation.step();
            stat(&simulation.context().frame_stats())
        })
        .collect();
    values.sort_by(f32::total_cmp);
    values[values.len() / 2]
}

/// A Luau loop running `line` for each sprite, with `x` and `y` its position.
fn each(line: &str) -> String {
    format!(
        "for i = 0, SPRITES - 1 do
            local x, y = (i * 7) % 320, (i * 13) % 180
            {line}
        end"
    )
}

/// A Luau game whose `draw` runs `draw`.
fn luau_game(draw: &str) -> String {
    format!(
        "local SPRITES = {SPRITES}
        local sprite = graphics.load_sprite_sheet('sprite.png', {SIZE}, {SIZE}):sprite(0, 0)
        local TINT = {{ r = 1, g = 0.5, b = 0.25 }}
        local calls = 0
        local function nothing(sprite, x, y)
            calls += 1
        end
        bench.use_sprite(sprite)

        function draw()
            {draw}
        end"
    )
}

/// Draws [`SPRITES`] sprites, or rectangles the size of one.
struct RustGame {
    sprite: Sprite,
    rects: bool,
}

impl RustGame {
    fn load(ctx: &mut Context, image: &std::path::Path, rects: bool) -> wingine::Result<Self> {
        let sprite = ctx.load_texture(image)?.sprite();
        Ok(Self { sprite, rects })
    }
}

impl Game for RustGame {
    fn draw(&mut self, canvas: &mut Canvas) {
        let tint = Color::rgb8(0xff, 0x80, 0x40);
        for i in 0..SPRITES {
            let [x, y] = position(i);
            let dest = Rect::new(x, y, SIZE as f32, SIZE as f32);
            if self.rects {
                canvas.fill_rect(dest, tint);
            } else {
                canvas.draw_sprite(&self.sprite, dest, Color::WHITE);
            }
        }
    }
}

/// A native library that draws the sprite it's given, one at a time or many at once.
#[derive(Default)]
struct Bench {
    sprite: Option<native::Sprite>,
}

impl native::Library for Bench {
    const NAME: &str = "bench";

    fn new(_: &mut native::Context) -> native::Result<Self> {
        Ok(Self::default())
    }

    fn register(registry: &mut native::Registry<Self>) {
        registry
            .function("use_sprite", |bench, _, sprite: native::Sprite| {
                bench.sprite = Some(sprite);
                Ok(())
            })
            .function("draw_one", |bench, call, (x, y): (f32, f32)| {
                let sprite = bench.sprite()?;
                call.canvas()?
                    .draw_sprite(sprite, dest([x, y]), native::Color::WHITE);
                Ok(())
            })
            .function("draw_many", |bench, call, count: u32| {
                let sprite = bench.sprite()?;
                let canvas = call.canvas()?;
                for i in 0..count {
                    canvas.draw_sprite(sprite, dest(position(i)), native::Color::WHITE);
                }
                Ok(())
            });
    }
}

impl Bench {
    fn sprite(&self) -> native::Result<&native::Sprite> {
        self.sprite
            .as_ref()
            .ok_or_else(|| native::Error::new("no sprite to draw"))
    }
}

fn dest([x, y]: [f32; 2]) -> native::Rect {
    native::Rect::new(x, y, SIZE as f32, SIZE as f32)
}
