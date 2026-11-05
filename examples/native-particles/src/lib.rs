//! An example native library for Wingine's Luau runtime: bursts of particles, drawn as single
//! pixels that fly out, slow down and fade, and bursts of sparks in 3D views, which fly up, fall
//! and shrink.
//!
//! A game lists it in its `conf.luau` as `libraries = { "particles" }`, with the built library in
//! its `native` folder, then:
//!
//! ```lua
//! local sparks = particles.new_emitter("#f2c14e")
//! sparks:burst(160, 90, 24)
//! sparks:burst_3d(vector.create(0, 0.5, 0), 16)
//!
//! function draw()
//!     graphics.clear("#000000")
//!     particles.draw()
//!     graphics.begin_3d(view)
//!     particles.draw_3d()
//! end
//! ```
//!
//! `particles.count()` returns how many particles and sparks are alive, `emitter:bursts()` how
//! many bursts an emitter made, and `particles.wait_until_gone()`, written in Luau, waits until
//! none are left. `particles.draw_3d()` draws the sparks as billboards in the 3D view the game
//! began last.

use wingine_native::{
    BillboardDraw, Class, Color, Context, Library, Rect, Registry, Result, Size, Sprite, Vec3,
};

/// How long a particle lives, in seconds.
const LIFETIME: f32 = 0.75;
/// How fast particles fly out at most, in canvas pixels per second.
const SPEED: f32 = 60.0;
/// How much of its speed a particle loses each second.
const DRAG: f32 = 2.0;
/// How fast sparks fly out at most, in world units per second.
const SPARK_SPEED: f32 = 2.0;
/// How fast sparks fall, in world units per second each second.
const GRAVITY: f32 = 4.0;
/// How wide and tall a spark is as it starts, in world units.
const SPARK_SIZE: f32 = 0.08;

/// The particles alive in a game.
struct Particles {
    live: Vec<Particle>,
    sparks: Vec<Spark>,
    /// For scattering particles, from a xorshift generator.
    seed: u32,
    /// A white pixel, which sparks show tinted.
    white: Sprite,
}

struct Particle {
    position: [f32; 2],
    velocity: [f32; 2],
    age: f32,
    color: Color,
}

/// A particle in 3D.
struct Spark {
    position: [f32; 3],
    velocity: [f32; 3],
    age: f32,
    color: Color,
}

/// Bursts of particles of one color, as scripts hold them.
struct Emitter {
    color: Color,
    bursts: u32,
}

impl Class for Emitter {
    const NAME: &str = "Emitter";
}

impl Particles {
    fn burst(&mut self, position: [f32; 2], count: u32, color: Color) {
        for _ in 0..count {
            let angle = self.random() * std::f32::consts::TAU;
            let speed = SPEED * (0.5 + self.random() * 0.5);
            self.live.push(Particle {
                position,
                velocity: [angle.cos() * speed, angle.sin() * speed],
                age: 0.0,
                color,
            });
        }
    }

    /// Sparks flying out and up from `position`.
    fn burst_3d(&mut self, position: [f32; 3], count: u32, color: Color) {
        for _ in 0..count {
            let angle = self.random() * std::f32::consts::TAU;
            let up = 0.3 + self.random() * 0.7;
            let across = (1.0 - up * up).sqrt();
            let speed = SPARK_SPEED * (0.5 + self.random() * 0.5);
            self.sparks.push(Spark {
                position,
                velocity: [
                    angle.cos() * across * speed,
                    up * speed,
                    angle.sin() * across * speed,
                ],
                age: 0.0,
                color,
            });
        }
    }

    /// A number from 0 up to 1.
    fn random(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / (1 << 24) as f32
    }

    fn advance(&mut self, dt: f32) {
        let drag = (1.0 - DRAG * dt).max(0.0);
        for particle in &mut self.live {
            particle.age += dt;
            for axis in 0..2 {
                particle.position[axis] += particle.velocity[axis] * dt;
                particle.velocity[axis] *= drag;
            }
        }
        self.live.retain(|particle| particle.age < LIFETIME);
        for spark in &mut self.sparks {
            spark.age += dt;
            spark.velocity[1] -= GRAVITY * dt;
            for axis in 0..3 {
                spark.position[axis] += spark.velocity[axis] * dt;
            }
        }
        self.sparks.retain(|spark| spark.age < LIFETIME);
    }

    /// The sparks as billboards, shrinking as they age.
    fn spark_draws(&self) -> Vec<BillboardDraw> {
        self.sparks
            .iter()
            .map(|spark| {
                let size = SPARK_SIZE * (1.0 - spark.age / LIFETIME);
                let [x, y, z] = spark.position;
                BillboardDraw {
                    tint: spark.color,
                    ..BillboardDraw::new(Vec3::new(x, y, z), Size::new(size, size))
                }
            })
            .collect()
    }
}

impl Library for Particles {
    const NAME: &str = "particles";

    fn new(ctx: &mut Context) -> Result<Self> {
        Ok(Self {
            live: Vec::new(),
            sparks: Vec::new(),
            seed: 0x9e37_79b9,
            white: ctx.create_sprite(1, 1, &[255; 4])?,
        })
    }

    fn register(registry: &mut Registry<Self>) {
        registry
            .function("new_emitter", |_, _, color: Color| {
                Ok(Emitter { color, bursts: 0 })
            })
            .function("count", |particles, _, ()| {
                Ok(particles.live.len() + particles.sparks.len())
            })
            .function("draw", |particles, call, ()| {
                let canvas = call.canvas()?;
                for particle in &particles.live {
                    let fade = 1.0 - particle.age / LIFETIME;
                    let color = Color {
                        a: particle.color.a * fade,
                        ..particle.color
                    };
                    let [x, y] = particle.position;
                    canvas.fill_rect(Rect::new(x.floor(), y.floor(), 1.0, 1.0), color);
                }
                Ok(())
            })
            .function("draw_3d", |particles, call, ()| {
                let draws = particles.spark_draws();
                call.canvas()?.draw_billboards(&particles.white, &draws)
            })
            .after_update(|particles, _, dt| {
                particles.advance(dt);
                Ok(())
            })
            .luau(
                "wait",
                "local particles = ...
                function particles.wait_until_gone()
                    wait_until(function() return particles.count() == 0 end)
                end",
            );
        registry
            .class::<Emitter>()
            .method(
                "burst",
                |particles, emitter, _, (x, y, count): (f32, f32, u32)| {
                    particles.burst([x, y], count, emitter.color);
                    emitter.bursts += 1;
                    Ok(())
                },
            )
            .method(
                "burst_3d",
                |particles, emitter, _, (position, count): (Vec3, u32)| {
                    let position = [position.x, position.y, position.z];
                    particles.burst_3d(position, count, emitter.color);
                    emitter.bursts += 1;
                    Ok(())
                },
            )
            .method("bursts", |_, emitter, _, ()| Ok(emitter.bursts));
    }
}

wingine_native::export!(Particles);
