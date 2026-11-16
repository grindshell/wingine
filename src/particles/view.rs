//! Particles in a 3D view: [`Particles3d`], an emitter whose particles fly in three dimensions,
//! with [`ParticleSettings3d`]. What it shares with the 2D emitter, its timing, its generator and
//! its colors and sizes over a life, is the 2D one's code.

use super::{ParticleSettings, Random, color_at, scale_at, slowing, steps};
use crate::{
    graphics::{Blend, Color, Mesh, Sprite},
    math::{Affine3, add3, dot3, normalize3, scale3, sub3},
};

/// What a particle in a 3D view is drawn as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Look {
    /// Its sprite, or a square, facing the camera, turned by its angle about the way the camera
    /// looks.
    #[default]
    Billboard,
    /// Its sprite, or a square, stretched back along the way it's flying, from where it is, by
    /// [`ParticleSettings3d::streak`] for each unit a second it flies, and turned about that way
    /// to face the camera: for sparks and rain.
    Streak,
    /// Its emitter's mesh, turned in 3D by its angle about an axis of its own, picked as it
    /// starts. An emitter with no mesh draws billboards.
    Mesh,
}

impl Look {
    pub const ALL: [Self; 3] = [Self::Billboard, Self::Streak, Self::Mesh];

    /// Its name in scene files and scripts, such as `"streak"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Billboard => "billboard",
            Self::Streak => "streak",
            Self::Mesh => "mesh",
        }
    }

    /// The look named `name`, if there is one.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|look| look.name() == name)
    }
}

/// What a 3D emitter sends out, how often, and how its particles move and look as they age.
///
/// `common` holds what's the same as a 2D emitter's: its rate, limit, warmup and seed, how long
/// particles live, how fast they start, how quickly they slow, how they're turned, their colors
/// and sizes over their lives, how they blend, and whether they're attached. Its `spread` is the
/// half angle of the cone particles fly in, about the emitter's `+z`: 0 sends them all along it,
/// and pi or more every way. Its `direction` and `gravity` aren't used: particles fly along `+z`,
/// as the emitter is turned, and fall by [`ParticleSettings3d::gravity`]. A particle's scale is
/// its width in world units, times its sprite's width over its height for its height.
#[derive(Clone, Debug, PartialEq)]
pub struct ParticleSettings3d {
    pub common: ParticleSettings,
    /// How much faster particles fly each second along `x`, `y` and `z`, such as `[0, -9.8, 0]`
    /// to fall.
    pub gravity: [f32; 3],
    /// What each particle is drawn as.
    pub look: Look,
    /// A streak's length, for each world unit a second it's flying, added to its width.
    pub streak: f32,
    /// Whether the view's light falls on its particles: billboards and streaks lit as billboards
    /// are, by each light's whole level, and meshes as meshes are, plus `glow` of their own
    /// colors. Unlit, they show their colors as they are, whatever lights them.
    pub lit: bool,
    /// How much of their own colors lit particles show whatever lights them.
    pub glow: Color,
    /// If it's set, how much of its speed across the plane a particle keeps as it comes back off
    /// the plane it was sent out from: the plane through where the emitter was as it started,
    /// square to its `+z`. Without it, particles pass through everything.
    pub bounce: Option<f32>,
}

impl Default for ParticleSettings3d {
    /// The 2D emitter's defaults, flying every way, unlit billboards a unit wide.
    fn default() -> Self {
        Self {
            common: ParticleSettings::default(),
            gravity: [0.0; 3],
            look: Look::Billboard,
            streak: 0.02,
            lit: false,
            glow: Color::BLACK,
            bounce: None,
        }
    }
}

impl ParticleSettings3d {
    /// Why these settings can't send particles out, if they can't: the first setting that's
    /// wrong, and what it must be.
    pub fn problem(&self) -> Option<String> {
        if let Some(problem) = self.common.problem() {
            return Some(problem);
        }
        if !self.gravity.iter().all(|value| value.is_finite()) {
            return Some("`gravity` must be finite".into());
        }
        if !(self.streak >= 0.0 && self.streak.is_finite()) {
            return Some(format!("`streak` must be 0 or more, not {}", self.streak));
        }
        if let Some(bounce) = self.bounce
            && !(bounce >= 0.0 && bounce.is_finite())
        {
            return Some(format!("`bounce` must be 0 or more, not {bounce}"));
        }
        None
    }

    /// Whether its particles draw opaque, among the view's meshes: blended normally, and never
    /// less than wholly opaque. Others are drawn blended, after them.
    pub fn is_opaque(&self) -> bool {
        self.common.blend == Blend::Normal && self.common.colors.iter().all(|color| color.a >= 1.0)
    }

    /// The color of a particle `life` of the way through its life, from 0 to 1.
    pub(crate) fn color_at(&self, life: f32) -> Color {
        color_at(&self.common.colors, life)
    }

    /// The scale of a particle `life` of the way through its life, from 0 to 1.
    pub(crate) fn scale_at(&self, life: f32) -> f32 {
        scale_at(&self.common.scales, life)
    }
}

/// A 3D emitter and the particles it has sent out that are still alive.
#[derive(Clone, Debug)]
pub struct Particles3d {
    settings: ParticleSettings3d,
    sprite: Option<Sprite>,
    mesh: Option<Mesh>,
    live: Vec<Particle3d>,
    random: Random,
    /// The share of a particle its rate has owed since it last sent one out.
    carry: f32,
    /// What places where it sends particles out from, and the box they start in, about its
    /// origin, before it's placed.
    origin: Affine3,
    size: [f32; 3],
    /// Where its origin was when it last advanced.
    last: Option<[f32; 3]>,
    /// Whether it's run its warmup.
    warmed: bool,
}

/// A particle in a 3D view: where it is and how it moves, in the emitter's coordinates if its
/// particles are attached, else in those its origin places them in.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Particle3d {
    pub position: [f32; 3],
    pub velocity: [f32; 3],
    /// Seconds since it started, less than its `lifetime`.
    pub age: f32,
    pub lifetime: f32,
    pub angle: f32,
    pub spin: f32,
    /// The axis a mesh particle turns about, of length 1.
    pub axis: [f32; 3],
    /// The plane it was sent out from, which it bounces off: a point of it, and its way out, of
    /// length 1.
    pub from: [f32; 3],
    pub out: [f32; 3],
}

impl Particles3d {
    /// An emitter with no particles yet, at the origin, sending them out from a point along
    /// `+z`. `sprite` is what each billboard or streak shows, or without one, a white square.
    /// `seed` starts its randomness unless its settings have a `seed`.
    ///
    /// # Panics
    ///
    /// If the settings have a [`problem`](ParticleSettings3d::problem).
    pub fn new(settings: ParticleSettings3d, sprite: Option<Sprite>, seed: u32) -> Self {
        Self::try_new(settings, sprite, seed).unwrap_or_else(|err| panic!("{err}"))
    }

    /// [`Particles3d::new`], or why the settings can't make one.
    pub fn try_new(
        settings: ParticleSettings3d,
        sprite: Option<Sprite>,
        seed: u32,
    ) -> Result<Self, String> {
        if let Some(problem) = settings.problem() {
            return Err(problem);
        }
        Ok(Self {
            random: Random::new(settings.common.seed.unwrap_or(seed)),
            settings,
            sprite,
            mesh: None,
            live: Vec::new(),
            carry: 0.0,
            origin: Affine3::IDENTITY,
            size: [0.0; 3],
            last: None,
            warmed: false,
        })
    }

    pub fn settings(&self) -> &ParticleSettings3d {
        &self.settings
    }

    /// Changes its settings, keeping the particles it has, which change to look and move as the
    /// new settings say. A new `seed` starts its randomness again. Fails, changing nothing, if
    /// they have a [`problem`](ParticleSettings3d::problem).
    pub fn set_settings(&mut self, settings: ParticleSettings3d) -> Result<(), String> {
        if let Some(problem) = settings.problem() {
            return Err(problem);
        }
        if let Some(seed) = settings.common.seed
            && self.settings.common.seed != Some(seed)
        {
            self.random = Random::new(seed);
        }
        self.live.truncate(settings.common.limit as usize);
        self.settings = settings;
        Ok(())
    }

    pub fn sprite(&self) -> Option<&Sprite> {
        self.sprite.as_ref()
    }

    /// Changes what its billboards and streaks show, or without a sprite, white squares.
    pub fn set_sprite(&mut self, sprite: Option<Sprite>) {
        self.sprite = sprite;
    }

    pub fn mesh(&self) -> Option<&Mesh> {
        self.mesh.as_ref()
    }

    /// Changes the mesh its particles are when they look like one ([`Look::Mesh`]).
    pub fn set_mesh(&mut self, mesh: Option<Mesh>) {
        self.mesh = mesh;
    }

    pub fn is_emitting(&self) -> bool {
        self.settings.common.emitting
    }

    /// Starts or stops sending particles out at its rate. Those it has sent live out their lives.
    pub fn set_emitting(&mut self, emitting: bool) {
        self.settings.common.emitting = emitting;
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
        self.random = Random::new(self.settings.common.seed.unwrap_or(seed));
        self.last = None;
        self.warmed = false;
    }

    /// What places its particles, if they're attached: see [`Particles3d::set_origin`].
    pub fn origin(&self) -> Affine3 {
        self.origin
    }

    /// Where it sends particles out from: from random points of a box `size` across, about its
    /// origin, placed by `origin`, which turns and stretches the way they fly too: along its
    /// `+z`. Its particles are in the coordinates `origin` places things in, unless they're
    /// attached, when they're in its own and drawn placed by `origin`.
    pub fn set_origin(&mut self, origin: Affine3, size: [f32; 3]) {
        self.origin = origin;
        self.size = size;
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

    /// Moves its particles on by `dt` seconds, as [`Particles::advance`](super::Particles::advance)
    /// does: in steps of at most [`STEP`](super::STEP), sending new ones out along the way its
    /// origin went, its warmup first the first time.
    pub fn advance(&mut self, dt: f32) {
        if !self.warmed {
            self.warmed = true;
            let warmup = self.settings.common.warmup;
            if warmup > 0.0 {
                self.last = Some(self.origin.translation);
                self.advance(warmup);
            }
        }
        let Some((steps, part)) = steps(dt) else {
            return;
        };
        let end = self.origin.translation;
        let start = self.last.unwrap_or(end);
        let at = |t: f32| [0, 1, 2].map(|i| start[i] + (end[i] - start[i]) * t);
        for step in 0..steps {
            let [from, to] = [step, step + 1].map(|n| at(n as f32 / steps as f32));
            self.step(part, from, to);
        }
        self.last = Some(end);
    }

    /// Moves particles on by `dt`, then sends out what its rate owes over it, as its origin moved
    /// from `from` to `to`.
    fn step(&mut self, dt: f32, from: [f32; 3], to: [f32; 3]) {
        let settings = &self.settings;
        let slowing = slowing(settings.common.drag, dt);
        let (gravity, bounce) = (settings.gravity, settings.bounce);
        self.live.retain_mut(|particle| {
            particle.age += dt;
            if particle.age >= particle.lifetime {
                return false;
            }
            particle.travel(dt, gravity, slowing, bounce);
            true
        });
        let common = &settings.common;
        if !common.emitting || common.rate == 0.0 {
            self.carry = 0.0;
            return;
        }
        self.carry += common.rate * dt;
        let count = self.carry as u32;
        self.carry -= count as f32;
        for i in 0..count {
            // Spread evenly through the step, each aged by what's left of it.
            let t = (i as f32 + 0.5) / count as f32;
            let at = [0, 1, 2].map(|axis| from[axis] + (to[axis] - from[axis]) * t);
            if !self.spawn(at, dt * (1.0 - t)) {
                self.carry = 0.0;
                break;
            }
        }
    }

    /// Starts a particle from a random point of its box, with its origin moved to `at`, flying
    /// within its cone, then moves it on by `age`. False, starting none, if it's at its limit.
    fn spawn(&mut self, at: [f32; 3], age: f32) -> bool {
        let settings = &self.settings;
        let common = &settings.common;
        if self.live.len() >= common.limit as usize {
            return false;
        }
        let random = &mut self.random;
        let point = self.size.map(|side| side * (random.unit() - 0.5));
        // Evenly over the cap of a sphere within `spread` of `+z`: its height is even.
        let lowest = libm::cosf(common.spread.min(std::f32::consts::PI));
        let up = 1.0 - random.unit() * (1.0 - lowest);
        let round = random.unit() * std::f32::consts::TAU;
        let (sin, cos) = libm::sincosf(round);
        let across = libm::sqrtf((1.0 - up * up).max(0.0));
        let speed = random.between(common.speed);
        let velocity = [across * cos * speed, across * sin * speed, up * speed];
        let lifetime = random.between(common.lifetime);
        let angle = random.between(common.angle);
        let spin = random.between(common.spin);
        // An axis for a mesh to turn about, evenly over the sphere.
        let height = random.unit() * 2.0 - 1.0;
        let (sin, cos) = libm::sincosf(random.unit() * std::f32::consts::TAU);
        let width = libm::sqrtf((1.0 - height * height).max(0.0));
        let axis = [width * cos, width * sin, height];
        let (position, velocity, from, out) = if common.attached {
            (point, velocity, [0.0; 3], [0.0, 0.0, 1.0])
        } else {
            let origin = Affine3 {
                translation: at,
                ..self.origin
            };
            let out = normalize3(origin.z_axis).unwrap_or([0.0, 0.0, 1.0]);
            (origin.apply(point), origin.apply_vector(velocity), at, out)
        };
        let mut particle = Particle3d {
            position,
            velocity,
            age: 0.0,
            lifetime,
            angle,
            spin,
            axis,
            from,
            out,
        };
        if age > 0.0 {
            particle.age = age;
            let slowing = slowing(common.drag, age);
            particle.travel(age, settings.gravity, slowing, settings.bounce);
        }
        self.live.push(particle);
        true
    }

    /// The particles alive, oldest first.
    pub(crate) fn live(&self) -> &[Particle3d] {
        &self.live
    }

    /// Where each particle alive is, oldest first: in the coordinates its origin places things
    /// in, or if they're attached, in the emitter's own.
    pub fn positions(&self) -> impl Iterator<Item = [f32; 3]> + '_ {
        self.live.iter().map(|particle| particle.position)
    }

    /// The longest a particle lives, in seconds.
    pub fn longest_life(&self) -> f32 {
        self.settings.common.lifetime[1]
    }
}

impl Particle3d {
    /// Moves it on by `dt` seconds, faster by `gravity` and slower by `slowing`, what's left of
    /// its speed after `dt`; and with `bounce`, back off the plane it started on.
    fn travel(&mut self, dt: f32, gravity: [f32; 3], slowing: f32, bounce: Option<f32>) {
        self.velocity = scale3(add3(self.velocity, scale3(gravity, dt)), slowing);
        self.position = add3(self.position, scale3(self.velocity, dt));
        self.angle += self.spin * dt;
        if let Some(kept) = bounce {
            let behind = dot3(sub3(self.position, self.from), self.out);
            let into = dot3(self.velocity, self.out);
            if behind < 0.0 && into < 0.0 {
                self.position = sub3(self.position, scale3(self.out, behind));
                self.velocity = sub3(self.velocity, scale3(self.out, into * (1.0 + kept)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::particles::{STEP, seed_for};

    fn emitter(settings: ParticleSettings3d) -> Particles3d {
        Particles3d::new(settings, None, 1)
    }

    fn still() -> ParticleSettings3d {
        ParticleSettings3d {
            common: ParticleSettings {
                rate: 0.0,
                lifetime: [10.0, 10.0],
                ..ParticleSettings::default()
            },
            ..ParticleSettings3d::default()
        }
    }

    fn run(particles: &mut Particles3d, steps: u32) {
        for _ in 0..steps {
            particles.advance(STEP);
        }
    }

    #[test]
    fn particles_fly_within_their_cone_along_the_emitters_z() {
        let mut particles = emitter(ParticleSettings3d {
            common: ParticleSettings {
                speed: [2.0, 2.0],
                spread: 0.5,
                ..still().common
            },
            ..still()
        });
        // Turned so its `+z` is up: a quarter turn about `x` takes `z` to `-y`, so the other way.
        particles.set_origin(
            Affine3::from_rotation_x(-std::f32::consts::FRAC_PI_2),
            [0.0; 3],
        );
        particles.emit(200);
        let lowest = libm::cosf(0.5);
        for particle in particles.live() {
            let speed = libm::sqrtf(dot3(particle.velocity, particle.velocity));
            assert!((speed - 2.0).abs() < 1e-4, "{particle:?}");
            assert!(
                particle.velocity[1] / speed >= lowest - 1e-4,
                "{particle:?}"
            );
        }
        // And every way within it: some lean each way.
        let leans = |axis: usize| {
            let live = particles.live();
            live.iter().any(|p| p.velocity[axis] > 0.3)
                && live.iter().any(|p| p.velocity[axis] < -0.3)
        };
        assert!(leans(0) && leans(2));
    }

    #[test]
    fn a_spread_of_pi_or_more_sends_particles_every_way() {
        let mut particles = emitter(ParticleSettings3d {
            common: ParticleSettings {
                speed: [1.0, 1.0],
                spread: std::f32::consts::TAU,
                ..still().common
            },
            ..still()
        });
        particles.emit(500);
        let backward = particles
            .live()
            .iter()
            .filter(|p| p.velocity[2] < 0.0)
            .count();
        assert!((200..300).contains(&backward), "{backward} of 500 backward");
    }

    #[test]
    fn gravity_pulls_in_three_dimensions_and_drag_slows() {
        let mut falling = emitter(ParticleSettings3d {
            gravity: [0.0, -10.0, 5.0],
            common: ParticleSettings {
                speed: [0.0, 0.0],
                ..still().common
            },
            ..still()
        });
        falling.emit(1);
        run(&mut falling, 60);
        let particle = falling.live()[0];
        assert!((particle.velocity[1] + 10.0).abs() < 1e-3, "{particle:?}");
        assert!((particle.velocity[2] - 5.0).abs() < 1e-3, "{particle:?}");
        assert!((-5.1..-5.0).contains(&particle.position[1]), "{particle:?}");

        let mut slowed = emitter(ParticleSettings3d {
            common: ParticleSettings {
                speed: [10.0, 10.0],
                spread: 0.0,
                drag: 1.0,
                ..still().common
            },
            ..still()
        });
        slowed.emit(1);
        run(&mut slowed, 60);
        let kept = (-1.0f32).exp() * 10.0;
        assert!((slowed.live()[0].velocity[2] - kept).abs() < 1e-3);
    }

    #[test]
    fn particles_bounce_off_the_plane_they_started_on() {
        // Facing up from the floor, at y = 1: thrown up, they come back down onto it.
        let settings = ParticleSettings3d {
            gravity: [0.0, -10.0, 0.0],
            common: ParticleSettings {
                speed: [3.0, 3.0],
                spread: 0.0,
                ..still().common
            },
            bounce: Some(0.5),
            ..still()
        };
        let up = Affine3 {
            translation: [0.0, 1.0, 0.0],
            ..Affine3::from_rotation_x(-std::f32::consts::FRAC_PI_2)
        };
        let mut bouncing = emitter(settings.clone());
        bouncing.set_origin(up, [0.0; 3]);
        bouncing.emit(1);
        let mut lowest = f32::MAX;
        let mut rose_again = false;
        let mut was_falling = false;
        for _ in 0..120 {
            bouncing.advance(STEP);
            let particle = bouncing.live()[0];
            lowest = lowest.min(particle.position[1]);
            if was_falling && particle.velocity[1] > 0.0 {
                rose_again = true;
            }
            was_falling = particle.velocity[1] < 0.0;
        }
        assert!(lowest >= 1.0 - 1e-5, "{lowest}");
        assert!(rose_again, "it bounced");

        // Without `bounce`, through the floor.
        let mut falling = emitter(ParticleSettings3d {
            bounce: None,
            ..settings
        });
        falling.set_origin(up, [0.0; 3]);
        falling.emit(1);
        run(&mut falling, 120);
        assert!(falling.live()[0].position[1] < 0.0);
    }

    #[test]
    fn particles_start_in_the_box_the_origin_places_and_attached_ones_follow() {
        let mut particles = emitter(ParticleSettings3d {
            common: ParticleSettings {
                speed: [0.0, 0.0],
                ..still().common
            },
            ..still()
        });
        particles.set_origin(Affine3::from_translation([10.0, 0.0, 0.0]), [2.0, 4.0, 6.0]);
        particles.emit(50);
        for particle in particles.live() {
            let [x, y, z] = particle.position;
            assert!(
                (9.0..=11.0).contains(&x) && (-2.0..=2.0).contains(&y),
                "{x} {y}"
            );
            assert!((-3.0..=3.0).contains(&z), "{z}");
        }
        let mut attached = emitter(ParticleSettings3d {
            common: ParticleSettings {
                speed: [0.0, 0.0],
                attached: true,
                ..still().common
            },
            ..still()
        });
        attached.set_origin(Affine3::from_translation([10.0, 0.0, 0.0]), [0.0; 3]);
        attached.emit(1);
        assert_eq!(attached.live()[0].position, [0.0; 3]);
    }

    #[test]
    fn bursts_rates_lives_and_limits_go_as_in_2d() {
        let mut particles = emitter(ParticleSettings3d {
            common: ParticleSettings {
                rate: 30.0,
                limit: 20,
                lifetime: [0.5, 0.5],
                ..ParticleSettings::default()
            },
            ..ParticleSettings3d::default()
        });
        run(&mut particles, 29);
        assert_eq!(particles.count(), 14);
        particles.emit(10);
        assert_eq!(particles.count(), 20);
        run(&mut particles, 60);
        assert!(particles.count() <= 20);
        particles.clear();
        assert_eq!(particles.count(), 0);
    }

    #[test]
    fn settings_that_cant_send_particles_out_say_why() {
        let fine = ParticleSettings3d::default;
        assert_eq!(fine().problem(), None);
        let wrong = |settings: ParticleSettings3d| settings.problem().unwrap();
        assert_eq!(
            wrong(ParticleSettings3d {
                streak: -1.0,
                ..fine()
            }),
            "`streak` must be 0 or more, not -1"
        );
        assert_eq!(
            wrong(ParticleSettings3d {
                bounce: Some(f32::NAN),
                ..fine()
            }),
            "`bounce` must be 0 or more, not NaN"
        );
        assert_eq!(
            wrong(ParticleSettings3d {
                common: ParticleSettings {
                    limit: 0,
                    ..ParticleSettings::default()
                },
                ..fine()
            }),
            "`limit` must be at least 1"
        );
        assert!(fine().is_opaque());
        let fading = ParticleSettings3d {
            common: ParticleSettings {
                colors: vec![Color::WHITE, Color::rgba(1.0, 1.0, 1.0, 0.0)],
                ..ParticleSettings::default()
            },
            ..fine()
        };
        assert!(!fading.is_opaque());
        assert_eq!(Look::from_name("streak"), Some(Look::Streak));
        assert_eq!(Look::from_name("smoke"), None);
    }

    /// FNV-1a over the bits of every particle's position, velocity, angle and axis.
    fn hash(particles: &Particles3d) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for particle in particles.live() {
            let values = [particle.position, particle.velocity, particle.axis]
                .into_iter()
                .flatten()
                .chain([particle.angle]);
            for byte in values.flat_map(|value| value.to_bits().to_le_bytes()) {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
            }
        }
        hash
    }

    /// A fountain on a turning, moving emitter, with every setting that moves particles, gives
    /// the same particles to the bit on every platform, which CI checks on Linux and Windows.
    #[test]
    fn particles_play_out_the_same_everywhere() {
        let mut particles = Particles3d::new(
            ParticleSettings3d {
                common: ParticleSettings {
                    rate: 90.0,
                    lifetime: [0.5, 1.5],
                    speed: [3.0, 8.0],
                    spread: 0.9,
                    drag: 0.4,
                    angle: [0.0, 3.0],
                    spin: [-2.0, 2.0],
                    ..ParticleSettings::default()
                },
                gravity: [0.0, -9.8, 0.0],
                bounce: Some(0.3),
                ..ParticleSettings3d::default()
            },
            None,
            seed_for("fountain"),
        );
        for frame in 0..120 {
            let t = frame as f32 / 60.0;
            let turned = Affine3::from_rotation_y(0.3 * t) * Affine3::from_rotation_x(-1.2);
            let origin = Affine3 {
                translation: [4.0 + 3.0 * t, 1.0, -2.0],
                ..turned
            };
            particles.set_origin(origin, [0.8, 0.2, 0.4]);
            particles.advance(STEP);
            if frame % 40 == 0 {
                particles.emit(12);
            }
        }
        // `libm` for the sines, roots and drag makes this the same on Windows and Linux, debug
        // and release. A change to how particles start or move changes it.
        assert_eq!(
            (particles.count(), hash(&particles)),
            (99, 0x3c08_6352_a037_3635)
        );
    }
}
