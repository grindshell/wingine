//! Particles: many small sprites, or squares, that an emitter sends out, which fly, turn, change
//! color and size as they age, and vanish when their lives end: for fire, smoke, sparks, dust and
//! fireworks.
//!
//! A [`Particles`] emitter is plain data with [`ParticleSettings`], kept and advanced by whoever
//! owns it: a game, a scene's `"particles"` node, or a script's emitter. Nothing advances it but
//! [`Particles::advance`], so particles stop when a game pauses, and tests step them as they like.
//! [`Canvas::draw_particles`](crate::graphics::Canvas::draw_particles) draws them.
//!
//! Each emitter has a random number generator of its own, seeded by its settings' `seed`, or by
//! the seed it's made with. Starting particles takes its sines from `libm`, so an emitter given
//! the same steps gives the same particles to the bit on every platform.

use crate::{
    graphics::{Blend, Color, Sprite},
    math::{Affine, Rect},
};

/// The longest step particles move in at once, in seconds: longer ones are split into steps this
/// long or shorter, so particles move alike at any frame rate.
pub const STEP: f32 = 1.0 / 60.0;

/// What an emitter sends out, how often, and how its particles move and look as they age.
///
/// Ranges, such as `lifetime`, are the least and the most a particle gets, each getting a value
/// picked at random between them as it starts: give both the same for a value every particle
/// gets. Lists over a particle's life, `colors` and `scales`, are spread evenly from its start to
/// its end, changing smoothly from each to the next.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleSettings {
    /// Whether it sends particles out at its `rate`. Bursts ([`Particles::emit`]) send them out
    /// either way.
    pub emitting: bool,
    /// Particles a second, while emitting.
    pub rate: f32,
    /// The most particles alive at once: while it has this many, it sends no more out.
    pub limit: u32,
    /// Seconds it runs for as it first advances, before the step it's advanced by, so a fire is
    /// already burning when a scene starts.
    pub warmup: f32,
    /// Where its randomness starts, so two emitters alike look different. Without one, it's the
    /// seed the emitter is made with.
    pub seed: Option<u32>,
    /// Seconds a particle lives, above 0.
    pub lifetime: [f32; 2],
    /// How fast a particle starts, in units a second.
    pub speed: [f32; 2],
    /// The way particles fly, in radians clockwise from right, as the emitter is turned: `-PI / 2`
    /// is up.
    pub direction: f32,
    /// How many radians wide the ways particles fly are, centered on `direction`: 0 sends every
    /// particle the same way, and a full turn, `2 * PI`, sends them every way.
    pub spread: f32,
    /// How much faster particles fly each second along `x` and `y`, such as `[0, 98]` to fall.
    pub gravity: [f32; 2],
    /// How quickly particles slow down: each second, their speed is `e^-drag` of what it was, so
    /// at 1 they keep about a third of it. 0 for none.
    pub drag: f32,
    /// How far a particle is turned as it starts, in radians, clockwise.
    pub angle: [f32; 2],
    /// How fast a particle turns, in radians a second, clockwise.
    pub spin: [f32; 2],
    /// A particle's colors over its life, multiplying its sprite's, at least one.
    pub colors: Vec<Color>,
    /// A particle's sizes over its life, at least one, each 0 or more: times its sprite's size,
    /// or without one, its width in units.
    pub scales: Vec<f32>,
    /// How particles mix with what's under them.
    pub blend: Blend,
    /// Whether particles move with the emitter after they've left it, as a fountain's spray
    /// does when the fountain moves. By default they stay where they were sent out, so a moving
    /// emitter leaves a trail.
    pub attached: bool,
}

impl Default for ParticleSettings {
    /// Ten white particles a second, each living a second, flying out every way at 20 units a
    /// second, a unit wide.
    fn default() -> Self {
        Self {
            emitting: true,
            rate: 10.0,
            limit: 500,
            warmup: 0.0,
            seed: None,
            lifetime: [1.0, 1.0],
            speed: [20.0, 20.0],
            direction: 0.0,
            spread: std::f32::consts::TAU,
            gravity: [0.0, 0.0],
            drag: 0.0,
            angle: [0.0, 0.0],
            spin: [0.0, 0.0],
            colors: vec![Color::WHITE],
            scales: vec![1.0],
            blend: Blend::Normal,
            attached: false,
        }
    }
}

impl ParticleSettings {
    /// Why these settings can't send particles out, if they can't: the first setting that's
    /// wrong, and what it must be.
    pub fn problem(&self) -> Option<String> {
        let finite = |values: &[f32]| values.iter().all(|value| value.is_finite());
        let range = |[least, most]: [f32; 2]| finite(&[least, most]) && least <= most;
        if !(self.rate >= 0.0 && self.rate.is_finite()) {
            return Some(format!("`rate` must be 0 or more, not {}", self.rate));
        }
        if self.limit == 0 {
            return Some("`limit` must be at least 1".into());
        }
        if !(self.warmup >= 0.0 && self.warmup.is_finite()) {
            return Some(format!("`warmup` must be 0 or more, not {}", self.warmup));
        }
        let ranges = [
            ("lifetime", self.lifetime),
            ("speed", self.speed),
            ("angle", self.angle),
            ("spin", self.spin),
        ];
        for (name, values) in ranges {
            if !range(values) {
                let [least, most] = values;
                return Some(format!(
                    "`{name}` must be finite, and its least no more than its most, not \
                     {{ {least}, {most} }}"
                ));
            }
        }
        if self.lifetime[0] <= 0.0 {
            return Some(format!(
                "`lifetime` must be above 0, not {}",
                self.lifetime[0]
            ));
        }
        if !finite(&[self.direction, self.gravity[0], self.gravity[1]]) {
            return Some("`direction` and `gravity` must be finite".into());
        }
        if !(self.spread >= 0.0 && self.spread.is_finite()) {
            return Some(format!("`spread` must be 0 or more, not {}", self.spread));
        }
        if !(self.drag >= 0.0 && self.drag.is_finite()) {
            return Some(format!("`drag` must be 0 or more, not {}", self.drag));
        }
        if self.colors.is_empty() {
            return Some("`colors` must have at least one color".into());
        }
        if self.scales.is_empty() {
            return Some("`scales` must have at least one scale".into());
        }
        if let Some(scale) = self
            .scales
            .iter()
            .find(|scale| !(**scale >= 0.0 && scale.is_finite()))
        {
            return Some(format!("`scales` must be 0 or more, not {scale}"));
        }
        None
    }

    /// The color of a particle `life` of the way through its life, from 0 to 1.
    pub(crate) fn color_at(&self, life: f32) -> Color {
        let (i, f) = along(self.colors.len(), life);
        let a = self.colors[i];
        let Some(b) = self.colors.get(i + 1) else {
            return a;
        };
        let mix = |a: f32, b: f32| a + (b - a) * f;
        Color::rgba(mix(a.r, b.r), mix(a.g, b.g), mix(a.b, b.b), mix(a.a, b.a))
    }

    /// The scale of a particle `life` of the way through its life, from 0 to 1.
    pub(crate) fn scale_at(&self, life: f32) -> f32 {
        let (i, f) = along(self.scales.len(), life);
        let a = self.scales[i];
        match self.scales.get(i + 1) {
            Some(b) => a + (b - a) * f,
            None => a,
        }
    }
}

/// Where `life`, from 0 to 1, falls among `count` values spread evenly over it: the one before it
/// and how far it is toward the next.
fn along(count: usize, life: f32) -> (usize, f32) {
    if count < 2 {
        return (0, 0.0);
    }
    let last = (count - 1) as f32;
    let at = (life.clamp(0.0, 1.0) * last).min(last);
    // Not `floor`, a call into the math library: `at` is 0 or more, so a cast rounds it down.
    let i = (at as usize).min(count - 2);
    (i, at - i as f32)
}

/// An emitter and the particles it has sent out that are still alive.
#[derive(Clone, Debug)]
pub struct Particles {
    settings: ParticleSettings,
    sprite: Option<Sprite>,
    live: Vec<Particle>,
    random: Random,
    /// The share of a particle its rate has owed since it last sent one out.
    carry: f32,
    /// What places where it sends particles out from, `area`, in the coordinates its particles
    /// are in.
    origin: Affine,
    area: Rect,
    /// Where its origin was when it last advanced, so particles it sends out as it moves are
    /// spread along the way it went.
    last: Option<[f32; 2]>,
    /// Whether it's run its warmup.
    warmed: bool,
}

/// A particle: where it is and how it moves, in the emitter's coordinates if its particles are
/// attached, else in those its origin places them in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Particle {
    pub position: [f32; 2],
    pub velocity: [f32; 2],
    /// Seconds since it started, less than its `lifetime`.
    pub age: f32,
    pub lifetime: f32,
    pub angle: f32,
    pub spin: f32,
}

impl Particles {
    /// An emitter with no particles yet, at the origin, sending them out from a point. `sprite`
    /// is what each particle shows, or without one, a white square. `seed` starts its randomness
    /// unless its settings have a `seed`.
    ///
    /// # Panics
    ///
    /// If the settings have a [`problem`](ParticleSettings::problem).
    pub fn new(settings: ParticleSettings, sprite: Option<Sprite>, seed: u32) -> Self {
        Self::try_new(settings, sprite, seed).unwrap_or_else(|err| panic!("{err}"))
    }

    /// [`Particles::new`], or why the settings can't make one.
    pub fn try_new(
        settings: ParticleSettings,
        sprite: Option<Sprite>,
        seed: u32,
    ) -> Result<Self, String> {
        if let Some(problem) = settings.problem() {
            return Err(problem);
        }
        Ok(Self {
            random: Random::new(settings.seed.unwrap_or(seed)),
            settings,
            sprite,
            live: Vec::new(),
            carry: 0.0,
            origin: Affine::IDENTITY,
            area: Rect::default(),
            last: None,
            warmed: false,
        })
    }

    pub fn settings(&self) -> &ParticleSettings {
        &self.settings
    }

    /// Changes its settings, keeping the particles it has, which change to look and move as the
    /// new settings say. A new `seed` starts its randomness again. Fails, changing nothing, if
    /// they have a [`problem`](ParticleSettings::problem).
    pub fn set_settings(&mut self, settings: ParticleSettings) -> Result<(), String> {
        if let Some(problem) = settings.problem() {
            return Err(problem);
        }
        if let Some(seed) = settings.seed
            && self.settings.seed != Some(seed)
        {
            self.random = Random::new(seed);
        }
        let limit = settings.limit as usize;
        self.live.truncate(limit);
        self.settings = settings;
        Ok(())
    }

    pub fn sprite(&self) -> Option<&Sprite> {
        self.sprite.as_ref()
    }

    /// Changes what its particles show, or without a sprite, white squares.
    pub fn set_sprite(&mut self, sprite: Option<Sprite>) {
        self.sprite = sprite;
    }

    pub fn is_emitting(&self) -> bool {
        self.settings.emitting
    }

    /// Starts or stops sending particles out at its rate. Those it has sent live out their lives.
    pub fn set_emitting(&mut self, emitting: bool) {
        self.settings.emitting = emitting;
    }

    /// How many particles are alive.
    pub fn count(&self) -> usize {
        self.live.len()
    }

    /// Ends every particle's life at once.
    pub fn clear(&mut self) {
        self.live.clear();
        self.carry = 0.0;
    }

    /// Starts over as it was made: no particles, its warmup to run again, and its randomness
    /// from `seed`, unless its settings have a `seed`.
    pub(crate) fn restart(&mut self, seed: u32) {
        self.clear();
        self.random = Random::new(self.settings.seed.unwrap_or(seed));
        self.last = None;
        self.warmed = false;
    }

    /// What places its particles, if they're attached: see [`Particles::set_origin`].
    pub fn origin(&self) -> Affine {
        self.origin
    }

    /// Where it sends particles out from: from random points of `area`, placed by `origin`,
    /// which turns and stretches the way they fly too. Its particles are in the coordinates
    /// `origin` places things in, unless they're attached, when they're in `area`'s and drawn placed
    /// by `origin`.
    pub fn set_origin(&mut self, origin: Affine, area: Rect) {
        self.origin = origin;
        self.area = area;
    }

    /// Sends `count` particles out at once from its origin, emitting or not, as many as its
    /// limit leaves room for.
    pub fn emit(&mut self, count: u32) {
        let origin = self.origin.translation;
        for _ in 0..count {
            if !self.spawn(origin, 0.0) {
                break;
            }
        }
    }

    /// Moves its particles on by `dt` seconds, ending those whose lives are over, and sends new
    /// ones out at its rate while emitting, in steps of at most [`STEP`]. Particles sent out as
    /// its origin moved since it last advanced are spread along the way it went. The first time
    /// it advances, it first runs its `warmup`.
    pub fn advance(&mut self, dt: f32) {
        if !self.warmed {
            self.warmed = true;
            let warmup = self.settings.warmup;
            if warmup > 0.0 {
                self.last = Some(self.origin.translation);
                self.advance(warmup);
            }
        }
        if !(dt > 0.0 && dt.is_finite()) {
            return;
        }
        // Not `ceil`, a call into the math library; a hair over a whole number of steps, from
        // rounding, isn't another step.
        let steps = ((dt / STEP - 1e-3) as u32 + 1).max(1);
        let part = dt / steps as f32;
        let end = self.origin.translation;
        let start = self.last.unwrap_or(end);
        let at = |t: f32| [0, 1].map(|i| start[i] + (end[i] - start[i]) * t);
        for step in 0..steps {
            let [from, to] = [step, step + 1].map(|n| at(n as f32 / steps as f32));
            self.step(part, from, to);
        }
        self.last = Some(end);
    }

    /// Moves particles on by `dt`, then sends out what its rate owes over it, as its origin moved
    /// from `from` to `to`.
    fn step(&mut self, dt: f32, from: [f32; 2], to: [f32; 2]) {
        let settings = &self.settings;
        let slowing = slowing(settings.drag, dt);
        let gravity = settings.gravity;
        self.live.retain_mut(|particle| {
            particle.age += dt;
            if particle.age >= particle.lifetime {
                return false;
            }
            particle.travel(dt, gravity, slowing);
            true
        });
        if !settings.emitting || settings.rate == 0.0 {
            self.carry = 0.0;
            return;
        }
        self.carry += settings.rate * dt;
        let count = self.carry as u32;
        self.carry -= count as f32;
        for i in 0..count {
            // Spread evenly through the step, each aged by what's left of it.
            let t = (i as f32 + 0.5) / count as f32;
            let at = [0, 1].map(|axis| from[axis] + (to[axis] - from[axis]) * t);
            if !self.spawn(at, dt * (1.0 - t)) {
                self.carry = 0.0;
                break;
            }
        }
    }

    /// Starts a particle from a random point of its area, with its origin moved to `at`, then
    /// moves it on by `age`. False, starting none, if it's at its limit.
    fn spawn(&mut self, at: [f32; 2], age: f32) -> bool {
        if self.live.len() >= self.settings.limit as usize {
            return false;
        }
        let settings = &self.settings;
        let random = &mut self.random;
        let area = self.area;
        let point = [
            area.x + area.w * random.unit(),
            area.y + area.h * random.unit(),
        ];
        let way = settings.direction + settings.spread * (random.unit() - 0.5);
        let (sin, cos) = libm::sincosf(way);
        let speed = random.between(settings.speed);
        let velocity = [cos * speed, sin * speed];
        let lifetime = random.between(settings.lifetime);
        let mut angle = random.between(settings.angle);
        let spin = random.between(settings.spin);
        let (position, velocity) = if settings.attached {
            (point, velocity)
        } else {
            let origin = Affine {
                translation: at,
                ..self.origin
            };
            let [x, y] = origin.x_axis;
            if y != 0.0 || x < 0.0 {
                angle += libm::atan2f(y, x);
            }
            (origin.apply(point), origin.apply_vector(velocity))
        };
        let mut particle = Particle {
            position,
            velocity,
            age: 0.0,
            lifetime,
            angle,
            spin,
        };
        if age > 0.0 {
            particle.age = age;
            particle.travel(age, settings.gravity, slowing(settings.drag, age));
        }
        self.live.push(particle);
        true
    }

    /// The particles alive, oldest first.
    pub(crate) fn live(&self) -> &[Particle] {
        &self.live
    }

    /// Where each particle alive is, oldest first: in the coordinates its origin places things
    /// in, or if they're attached, in the emitter's own.
    pub fn positions(&self) -> impl Iterator<Item = [f32; 2]> + '_ {
        self.live.iter().map(|particle| particle.position)
    }

    /// The longest a particle lives, in seconds.
    pub fn longest_life(&self) -> f32 {
        self.settings.lifetime[1]
    }
}

impl Particle {
    /// Moves it on by `dt` seconds, faster by `gravity` and slower by `slowing`, what's left of
    /// its speed after `dt`.
    fn travel(&mut self, dt: f32, gravity: [f32; 2], slowing: f32) {
        let axes = self
            .velocity
            .iter_mut()
            .zip(&mut self.position)
            .zip(gravity);
        for ((velocity, position), gravity) in axes {
            *velocity = (*velocity + gravity * dt) * slowing;
            *position += *velocity * dt;
        }
        self.angle += self.spin * dt;
    }
}

/// How much of its speed a particle keeps over `dt` seconds with `drag`.
fn slowing(drag: f32, dt: f32) -> f32 {
    if drag == 0.0 {
        1.0
    } else {
        libm::expf(-drag * dt)
    }
}

/// A xorshift generator, small and quick, and the same on every platform.
#[derive(Clone, Debug)]
struct Random(u32);

impl Random {
    fn new(seed: u32) -> Self {
        // Mixed, so seeds near one another start far apart, and never 0, where xorshift stays.
        let mut z = seed.wrapping_add(0x9e37_79b9);
        z = (z ^ (z >> 16)).wrapping_mul(0x85eb_ca6b);
        z = (z ^ (z >> 13)).wrapping_mul(0xc2b2_ae35);
        z ^= z >> 16;
        Self(if z == 0 { 0x9e37_79b9 } else { z })
    }

    /// A number from 0 up to 1.
    fn unit(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x >> 8) as f32 / (1 << 24) as f32
    }

    /// A number from `least` up to `most`, or `least` if they're the same.
    fn between(&mut self, [least, most]: [f32; 2]) -> f32 {
        if least == most {
            least
        } else {
            least + (most - least) * self.unit()
        }
    }
}

/// The seed an emitter named `name`, such as a scene node's path, starts from, so emitters with
/// different names differ but each is the same every time: its FNV-1a hash.
pub(crate) fn seed_for(name: &str) -> u32 {
    name.bytes().fold(0x811c_9dc5, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emitter(settings: ParticleSettings) -> Particles {
        Particles::new(settings, None, 1)
    }

    /// Advances `particles` `steps` times by [`STEP`].
    fn run(particles: &mut Particles, steps: u32) {
        for _ in 0..steps {
            particles.advance(STEP);
        }
    }

    #[test]
    fn a_rate_sends_particles_out_evenly_carrying_what_it_owes() {
        let mut particles = emitter(ParticleSettings {
            rate: 30.0,
            lifetime: [10.0, 10.0],
            ..ParticleSettings::default()
        });
        run(&mut particles, 60);
        assert_eq!(particles.count(), 30);
        // Half a particle a step: none, then one.
        let mut counts = Vec::new();
        for _ in 0..4 {
            let before = particles.count();
            particles.advance(STEP);
            counts.push(particles.count() - before);
        }
        assert_eq!(counts, [0, 1, 0, 1]);
    }

    #[test]
    fn long_steps_are_split_and_spread_their_particles() {
        let settings = ParticleSettings {
            rate: 60.0,
            lifetime: [10.0, 10.0],
            spread: 0.0,
            ..ParticleSettings::default()
        };
        let mut once = emitter(settings.clone());
        once.advance(0.5);
        let mut stepped = emitter(settings);
        run(&mut stepped, 30);
        assert_eq!(once.count(), 30);
        assert_eq!(once.live(), stepped.live());
        // Each a step older than the next, flying right.
        let xs: Vec<f32> = once.live().iter().map(|p| p.position[0]).collect();
        assert!(xs.windows(2).all(|pair| pair[0] > pair[1]), "{xs:?}");
    }

    #[test]
    fn particles_end_with_their_lives() {
        let mut particles = emitter(ParticleSettings {
            rate: 0.0,
            lifetime: [0.5, 0.5],
            ..ParticleSettings::default()
        });
        particles.emit(8);
        assert_eq!(particles.count(), 8);
        run(&mut particles, 29);
        assert_eq!(particles.count(), 8);
        run(&mut particles, 2);
        assert_eq!(particles.count(), 0);
    }

    #[test]
    fn bursts_and_rates_stop_at_the_limit() {
        let mut particles = emitter(ParticleSettings {
            rate: 600.0,
            limit: 20,
            lifetime: [10.0, 10.0],
            ..ParticleSettings::default()
        });
        particles.emit(15);
        run(&mut particles, 10);
        assert_eq!(particles.count(), 20);
        particles.emit(5);
        assert_eq!(particles.count(), 20);
    }

    #[test]
    fn emitters_that_arent_emitting_only_burst() {
        let mut particles = emitter(ParticleSettings {
            emitting: false,
            ..ParticleSettings::default()
        });
        run(&mut particles, 60);
        assert_eq!(particles.count(), 0);
        particles.emit(3);
        assert_eq!(particles.count(), 3);
        // Ten a second: one in a tenth of a second and a little more.
        particles.set_emitting(true);
        run(&mut particles, 9);
        assert_eq!(particles.count(), 4);
        particles.clear();
        assert_eq!(particles.count(), 0);
    }

    #[test]
    fn gravity_pulls_and_drag_slows() {
        let still = ParticleSettings {
            rate: 0.0,
            speed: [0.0, 0.0],
            lifetime: [10.0, 10.0],
            ..ParticleSettings::default()
        };
        let mut falling = emitter(ParticleSettings {
            gravity: [0.0, 60.0],
            ..still.clone()
        });
        falling.emit(1);
        run(&mut falling, 60);
        let particle = falling.live()[0];
        assert!((particle.velocity[1] - 60.0).abs() < 1e-3, "{particle:?}");
        // A step of 1/60 s: about half of 60 for the second it fell, a little more.
        assert!((30.0..30.6).contains(&particle.position[1]), "{particle:?}");

        let mut slowed = emitter(ParticleSettings {
            speed: [10.0, 10.0],
            spread: 0.0,
            drag: 1.0,
            ..still
        });
        slowed.emit(1);
        run(&mut slowed, 60);
        let particle = slowed.live()[0];
        let kept = (-1.0f32).exp() * 10.0;
        assert!((particle.velocity[0] - kept).abs() < 1e-3, "{particle:?}");
    }

    #[test]
    fn particles_start_in_the_area_the_origin_places() {
        let mut particles = emitter(ParticleSettings {
            rate: 0.0,
            speed: [0.0, 0.0],
            ..ParticleSettings::default()
        });
        let turned = Affine::from_parts_exact([100.0, 50.0], std::f32::consts::FRAC_PI_2, [1.0; 2]);
        particles.set_origin(turned, Rect::new(0.0, -2.0, 10.0, 4.0));
        particles.emit(50);
        for particle in particles.live() {
            // Turned a quarter, the area runs down from the origin, 4 wide.
            let [x, y] = particle.position;
            assert!((98.0..=102.0).contains(&x), "{x}");
            assert!((50.0..=60.0).contains(&y), "{y}");
        }
    }

    #[test]
    fn particles_left_behind_stay_and_attached_ones_follow() {
        let settings = ParticleSettings {
            rate: 0.0,
            speed: [0.0, 0.0],
            lifetime: [10.0, 10.0],
            ..ParticleSettings::default()
        };
        let mut behind = emitter(settings.clone());
        let mut attached = emitter(ParticleSettings {
            attached: true,
            ..settings
        });
        for particles in [&mut behind, &mut attached] {
            particles.set_origin(Affine::from_translation([10.0, 0.0]), Rect::default());
            particles.emit(1);
            particles.set_origin(Affine::from_translation([50.0, 0.0]), Rect::default());
            particles.advance(STEP);
        }
        assert_eq!(behind.live()[0].position, [10.0, 0.0]);
        // In the emitter's coordinates, drawn where it is.
        assert_eq!(attached.live()[0].position, [0.0, 0.0]);
        assert_eq!(attached.origin().translation, [50.0, 0.0]);
    }

    #[test]
    fn particles_sent_out_as_the_origin_moves_are_spread_along_its_way() {
        let mut particles = emitter(ParticleSettings {
            rate: 240.0,
            speed: [0.0, 0.0],
            lifetime: [10.0, 10.0],
            ..ParticleSettings::default()
        });
        particles.advance(STEP);
        particles.live.clear();
        particles.set_origin(Affine::from_translation([40.0, 0.0]), Rect::default());
        particles.advance(STEP);
        let xs: Vec<f32> = particles.live().iter().map(|p| p.position[0]).collect();
        assert_eq!(xs, [5.0, 15.0, 25.0, 35.0]);
    }

    #[test]
    fn colors_and_scales_change_evenly_over_a_life() {
        let settings = ParticleSettings {
            colors: vec![Color::WHITE, Color::rgba(1.0, 0.0, 0.0, 0.0), Color::BLACK],
            scales: vec![4.0, 2.0],
            ..ParticleSettings::default()
        };
        assert_eq!(settings.color_at(0.0), Color::WHITE);
        assert_eq!(settings.color_at(0.25), Color::rgba(1.0, 0.5, 0.5, 0.5));
        assert_eq!(settings.color_at(0.5), Color::rgba(1.0, 0.0, 0.0, 0.0));
        assert_eq!(settings.color_at(1.0), Color::BLACK);
        assert_eq!(settings.scale_at(0.0), 4.0);
        assert_eq!(settings.scale_at(0.75), 2.5);
        assert_eq!(settings.scale_at(1.0), 2.0);
        let one = ParticleSettings::default();
        assert_eq!(one.color_at(0.7), Color::WHITE);
        assert_eq!(one.scale_at(0.7), 1.0);
    }

    #[test]
    fn warmup_runs_as_it_first_advances() {
        let settings = ParticleSettings {
            rate: 30.0,
            lifetime: [10.0, 10.0],
            ..ParticleSettings::default()
        };
        let mut warm = emitter(ParticleSettings {
            warmup: 1.0,
            ..settings.clone()
        });
        let mut cold = emitter(settings);
        assert_eq!(warm.count(), 0);
        warm.advance(STEP);
        run(&mut cold, 61);
        assert_eq!(warm.count(), cold.count());
        assert_eq!(warm.live(), cold.live());
    }

    #[test]
    fn seeds_start_the_randomness() {
        let settings = ParticleSettings {
            lifetime: [1.0, 2.0],
            speed: [10.0, 40.0],
            ..ParticleSettings::default()
        };
        let made = |settings: &ParticleSettings, seed| {
            let mut particles = Particles::new(settings.clone(), None, seed);
            particles.emit(4);
            particles.live().to_vec()
        };
        assert_eq!(made(&settings, 1), made(&settings, 1));
        assert_ne!(made(&settings, 1), made(&settings, 2));
        let seeded = ParticleSettings {
            seed: Some(7),
            ..settings
        };
        assert_eq!(made(&seeded, 1), made(&seeded, 2));
        assert_ne!(seed_for("fire/sparks"), seed_for("fire/smoke"));
    }

    #[test]
    fn settings_that_cant_send_particles_out_say_why() {
        let wrong = |settings: ParticleSettings| settings.problem().unwrap();
        let fine = ParticleSettings::default;
        assert_eq!(fine().problem(), None);
        assert_eq!(
            wrong(ParticleSettings {
                rate: -1.0,
                ..fine()
            }),
            "`rate` must be 0 or more, not -1"
        );
        assert_eq!(
            wrong(ParticleSettings {
                lifetime: [2.0, 1.0],
                ..fine()
            }),
            "`lifetime` must be finite, and its least no more than its most, not { 2, 1 }"
        );
        assert_eq!(
            wrong(ParticleSettings {
                lifetime: [0.0, 1.0],
                ..fine()
            }),
            "`lifetime` must be above 0, not 0"
        );
        assert_eq!(
            wrong(ParticleSettings {
                colors: Vec::new(),
                ..fine()
            }),
            "`colors` must have at least one color"
        );
        assert_eq!(
            wrong(ParticleSettings {
                scales: vec![1.0, -1.0],
                ..fine()
            }),
            "`scales` must be 0 or more, not -1"
        );
        assert_eq!(
            wrong(ParticleSettings { limit: 0, ..fine() }),
            "`limit` must be at least 1"
        );
        assert!(Particles::try_new(ParticleSettings { limit: 0, ..fine() }, None, 1).is_err());
        let mut particles = emitter(fine());
        assert!(
            particles
                .set_settings(ParticleSettings {
                    drag: f32::NAN,
                    ..fine()
                })
                .is_err()
        );
        assert_eq!(particles.settings(), &fine());
    }

    /// FNV-1a over the bits of every particle's position, velocity and angle.
    fn hash(particles: &Particles) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for particle in particles.live() {
            let values = [
                particle.position[0],
                particle.position[1],
                particle.velocity[0],
                particle.velocity[1],
                particle.angle,
            ];
            for byte in values
                .iter()
                .flat_map(|value| value.to_bits().to_le_bytes())
            {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
            }
        }
        hash
    }

    /// A fountain on a turning, moving emitter, with every setting that moves particles, gives
    /// the same particles to the bit on every platform, which CI checks on Linux and Windows.
    #[test]
    fn particles_play_out_the_same_everywhere() {
        let mut particles = Particles::new(
            ParticleSettings {
                rate: 90.0,
                lifetime: [0.5, 1.5],
                speed: [30.0, 80.0],
                direction: -1.2,
                spread: 0.9,
                gravity: [0.0, 98.0],
                drag: 0.4,
                angle: [0.0, 3.0],
                spin: [-2.0, 2.0],
                ..ParticleSettings::default()
            },
            None,
            seed_for("fountain"),
        );
        for frame in 0..120 {
            let t = frame as f32 / 60.0;
            let origin = Affine::from_parts_exact([40.0 + 30.0 * t, 90.0], 0.3 * t, [1.0, 1.0]);
            particles.set_origin(origin, Rect::new(-4.0, -1.0, 8.0, 2.0));
            particles.advance(STEP);
            if frame % 40 == 0 {
                particles.emit(12);
            }
        }
        // `libm` for the sines and the drag makes this the same on Windows and Linux, debug and
        // release. A change to how particles start or move changes it.
        assert_eq!(
            (particles.count(), hash(&particles)),
            (101, 0x33d2_7f24_627d_e7e1)
        );
    }
}
