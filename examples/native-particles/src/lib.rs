//! An example native library for Wingine's Luau runtime: bursts of particles, drawn as single
//! pixels that fly out, slow down and fade.
//!
//! A game lists it in its `conf.luau` as `libraries = { "particles" }`, with the built library in
//! its `native` folder, then:
//!
//! ```lua
//! local sparks = particles.new_emitter("#f2c14e")
//! sparks:burst(160, 90, 24)
//!
//! function draw()
//!     graphics.clear("#000000")
//!     particles.draw()
//! end
//! ```
//!
//! `particles.count()` returns how many particles are alive, `emitter:bursts()` how many bursts
//! an emitter made, and `particles.wait_until_gone()`, written in Luau, waits until no particles
//! are left.

use wingine_native::{Class, Color, Context, Library, Rect, Registry, Result};

/// How long a particle lives, in seconds.
const LIFETIME: f32 = 0.75;
/// How fast particles fly out at most, in canvas pixels per second.
const SPEED: f32 = 60.0;
/// How much of its speed a particle loses each second.
const DRAG: f32 = 2.0;

/// The particles alive in a game.
struct Particles {
    live: Vec<Particle>,
    /// For scattering particles, from a xorshift generator.
    seed: u32,
}

struct Particle {
    position: [f32; 2],
    velocity: [f32; 2],
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
    }
}

impl Library for Particles {
    const NAME: &str = "particles";

    fn new(_ctx: &mut Context) -> Result<Self> {
        Ok(Self {
            live: Vec::new(),
            seed: 0x9e37_79b9,
        })
    }

    fn register(registry: &mut Registry<Self>) {
        registry
            .function("new_emitter", |_, _, color: Color| {
                Ok(Emitter { color, bursts: 0 })
            })
            .function("count", |particles, _, ()| Ok(particles.live.len()))
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
            .method("bursts", |_, emitter, _, ()| Ok(emitter.bursts));
    }
}

wingine_native::export!(Particles);
