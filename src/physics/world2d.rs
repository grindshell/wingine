//! Physics in 2D, by Rapier: fixed colliders that cover nodes as they're drawn, tiles, bodies that
//! fall, tumble and rest on each other, stepped through time, boxes slid and characters walked
//! among them, and what's where: rays, overlaps and sensors. See [`World`].

use rapier2d::{
    math::Rotation,
    parry::{
        query::ShapeCastOptions,
        shape::{Ball, Cuboid, Shape as ParryShape},
    },
    prelude::{
        ActiveCollisionTypes, Collider, ColliderBuilder, ColliderHandle, PhysicsWorld, QueryFilter,
        Ray, RigidBodyBuilder, RigidBodyHandle, SharedShape,
    },
};

use super::{BodyKind, Material, check_material, check_sensor_kind};
use crate::math::{Affine, Rect};

/// Rapier's vectors, which are glam's, and its poses: where something is and how it's turned.
type Vector = rapier2d::prelude::Vector;
type Isometry = rapier2d::prelude::Pose;

/// The gap a sliding box or a walking character keeps from what it stops at, in the world's units,
/// so that a box resting against something isn't touching it, and slides along it without hitting
/// it again.
const GAP: f32 = 0.01;

/// How far below a character the ground may be for it to stand on it, unless the ground moved away.
const REACH: f32 = GAP * 4.0;

/// The longest step the world takes at once, in seconds: longer ones are taken in parts.
const STEP: f32 = 1.0 / 60.0;

/// The most parts a step is taken in, so a long hitch doesn't take longer to catch up with.
const MAX_STEPS: u32 = 16;

/// The most things a box slides off in one move. Each takes away the part of the move going into
/// it, so in 2D, two leave nothing; more are for corners.
const MAX_HITS: usize = 4;

/// Down the world, with `y` down.
const DOWN: Vector = Vector::Y;
const UP: Vector = Vector::NEG_Y;

/// The ways out of a box's sides, unturned.
const BOX_SIDES: [Vector; 4] = [Vector::X, Vector::NEG_X, Vector::Y, Vector::NEG_Y];

/// Which of a collider's rectangle is solid.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Solid {
    /// All of it, so nothing gets in.
    #[default]
    Whole,
    /// Only its edges, lines with no thickness: what's inside stays in, and what's outside out.
    Edges,
    /// None of it: a sensor, all of whose area boxes slide through, which [`World::inside`]
    /// tells what's in. Rays and overlaps never find sensors.
    Sensor,
}

/// A collider in a [`World`]: a fixed one's, tiles', or a body's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ColliderId(ColliderHandle);

/// Where a ray hit a collider: see [`World::cast_ray`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    pub collider: ColliderId,
    /// How far along the ray it hit, in lengths of the ray's direction.
    pub distance: f32,
    /// Where it hit.
    pub point: [f32; 2],
    /// The way out of the collider where it hit, a vector of length 1 facing back along the ray,
    /// or zero if the ray started inside the collider.
    pub normal: [f32; 2],
}

/// Where a body is and how it's turned: by `angle`, radians clockwise with `y` down, about its
/// origin, then moved to `position`, as a scene node is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose {
    pub position: [f32; 2],
    pub angle: f32,
}

impl Pose {
    /// At `position`, unturned.
    pub const fn at(position: [f32; 2]) -> Self {
        Self {
            position,
            angle: 0.0,
        }
    }

    /// As Rapier has it. Its rotation is worked out with `libm`, which gives the same bits on
    /// every platform, as the rest of the world's arithmetic does.
    fn to_rapier(self) -> Isometry {
        let (sin, cos) = libm::sincosf(self.angle);
        let rotation = Rotation::from_cos_sin_unchecked(cos, sin);
        Isometry::from_parts(Vector::from(self.position), rotation)
    }

    /// From how Rapier has it.
    fn from_rapier(pose: &Isometry) -> Self {
        Self {
            position: pose.translation.into(),
            angle: libm::atan2f(pose.rotation.im, pose.rotation.re),
        }
    }
}

/// A body's shape, in its own coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shape {
    /// A box along the body's axes.
    Box(Rect),
    Circle {
        center: [f32; 2],
        radius: f32,
    },
}

/// A body to add to a [`World`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    pub kind: BodyKind,
    pub shape: Shape,
    pub pose: Pose,
    pub material: Material,
    /// Whether it's a sensor: an area that nothing bumps into, which [`World::inside`] tells
    /// what's in. A sensor is fixed or kinematic.
    pub sensor: bool,
}

/// A character walking with [`World::walk`], seen from the side: an upright box, standing on the
/// middle of its bottom edge, which is never turned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Character {
    pub width: f32,
    /// How tall it is, from its feet to the top of its head.
    pub height: f32,
    /// The highest it steps up, as onto a stair.
    pub step: f32,
    /// The steepest slope, in radians, that it walks up and stands on, rather than slides down.
    pub slope: f32,
    /// How heavy it is, for how hard it pushes dynamic bodies it walks into.
    pub mass: f32,
}

impl Character {
    /// Its box, with its middle at the origin.
    fn shape(&self) -> Cuboid {
        Cuboid::new(Vector::new(self.width / 2.0, self.height / 2.0))
    }
}

/// How a character's walk went: see [`World::walk`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Walked {
    /// How far it moved.
    pub moved: [f32; 2],
    /// Whether it ended standing on something it can stand on.
    pub grounded: bool,
}

/// Colliders and bodies in 2D, boxes slid and characters walked among them, and what's where.
///
/// Fixed colliders cover rectangles placed by [`Affine`]s, as scene nodes are, all of each solid,
/// only its edges, or none of it ([`Solid`]), and tiles, as boxes ([`World::add_tiles`]). Bodies
/// are fixed, dynamic or kinematic ([`BodyKind`]), each a box or a circle ([`Shape`]) placed by
/// a [`Pose`], which [`World::step`] moves through time: dynamic bodies fall with
/// [`World::gravity`], tumble, bounce and come to rest, and kinematic bodies go where they're put,
/// pushing dynamic ones out of their way.
///
/// Boxes slide among them with [`World::slide`], as a character walks around a garden seen from
/// above, stopping at what's in their way, bodies too, without pushing it. Characters seen from
/// the side walk with [`World::walk`], which moves a box as a kinematic body is moved, along the
/// ground, up steps and slopes, along with what they stand on, and pushing dynamic bodies out of
/// the way.
///
/// Scripts ask where things are by rays ([`World::cast_ray`]), by shapes overlapping colliders
/// ([`World::overlapping`], [`World::overlapping_circle`]), and by sensors, areas nothing bumps
/// into that [`World::inside`] tells what's in. They find colliders where they are now, added or
/// moved since the last step too, but kinematic bodies that are going somewhere over the next
/// step where they are until then. Sensors are never hit or overlapped.
///
/// Its units are its colliders', such as a scene's pixels, with `y` down. Rapier tunes how near
/// things may get, how far they sink into each other and how slowly they come to rest by
/// [`World::set_length_unit`]. Rapier is built with its `enhanced-determinism`, and angles are
/// turned into rotations and back with `libm`, so the same colliders, bodies, steps and walks give
/// the same results to the bit on every platform.
pub struct World {
    rapier: PhysicsWorld,
    /// The colliders added or moved since the last step, which Rapier's queries don't find where
    /// they are until they're refreshed.
    fresh: Vec<ColliderHandle>,
    /// How long the last step was, which characters take to have been how long their walks took,
    /// for how fast they push what they walk into and how far what they stand on carries them.
    last_step: f32,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// A world with no colliders, and gravity pulling down at 9.81 units a second each second, as
    /// Earth's does in metres.
    pub fn new() -> Self {
        let mut rapier = PhysicsWorld::new();
        rapier.gravity = DOWN * 9.81;
        Self {
            rapier,
            fresh: Vec::new(),
            last_step: STEP,
        }
    }

    /// How fast dynamic bodies falling speed up each second, as a vector, with `y` down.
    pub fn gravity(&self) -> [f32; 2] {
        self.rapier.gravity.into()
    }

    pub fn set_gravity(&mut self, gravity: [f32; 2]) {
        self.rapier.gravity = Vector::from(gravity);
    }

    /// Sets how long most things are, in the world's units, which Rapier tunes how near things
    /// may get, how far they may sink into each other and how slowly they come to rest by: 1, as
    /// for metres, by default. A world in pixels might be 16. Does nothing unless `unit` is more
    /// than 0.
    pub fn set_length_unit(&mut self, unit: f32) {
        if unit > 0.0 && unit.is_finite() {
            self.rapier.integration_parameters.length_unit = unit;
        }
    }

    /// Adds a fixed collider of `rect` placed by `transform`, all of it solid, only its edges, or
    /// none of it, a sensor. A rectangle squashed flat, with no area, is nowhere until
    /// [`World::set`] gives it some.
    pub fn add(&mut self, transform: Affine, rect: Rect, solid: Solid) -> ColliderId {
        let id = self.add_stand_in();
        self.set(id, transform, rect, solid);
        id
    }

    /// A fixed collider that's nowhere, for shaping.
    fn add_stand_in(&mut self) -> ColliderId {
        let collider = ColliderBuilder::new(SharedShape::ball(0.0)).enabled(false);
        ColliderId(self.rapier.insert_collider(collider, None))
    }

    /// Moves and reshapes the fixed collider `id`, as [`World::add`] places one. Does nothing to
    /// one that's been removed, or a body's.
    pub fn set(&mut self, id: ColliderId, transform: Affine, rect: Rect, solid: Solid) {
        let Some(collider) = self.rapier.colliders.get_mut(id.0) else {
            return;
        };
        if collider.parent().is_some() {
            return;
        }
        match shape(transform, rect, solid) {
            Some(shape) => {
                collider.set_shape(shape);
                collider.set_enabled(true);
            }
            None => collider.set_enabled(false),
        }
        collider.set_position(Isometry::IDENTITY);
        set_sensor(collider, solid == Solid::Sensor);
        self.fresh.push(id.0);
    }

    /// Adds a fixed collider of tiles: boxes, `rects`, placed from `origin`, such as a tile
    /// map's solid tiles. Boxes with no area are left out, and with none left, it's nowhere until
    /// [`World::set_tiles`] gives it some.
    ///
    /// Boxes sharing an edge are separate, and what slides along one can catch on the next's
    /// corner, so they're best merged into as few as cover the tiles, as
    /// [`Tilemap::solid_rects`](crate::tilemap::Tilemap::solid_rects) does.
    pub fn add_tiles(&mut self, origin: [f32; 2], rects: &[Rect]) -> ColliderId {
        let id = self.add_stand_in();
        self.set_tiles(id, origin, rects);
        id
    }

    /// Moves and reshapes the collider of tiles `id`, as [`World::add_tiles`] places one. Does
    /// nothing to one that's been removed, or a body's.
    pub fn set_tiles(&mut self, id: ColliderId, origin: [f32; 2], rects: &[Rect]) {
        let Some(collider) = self.rapier.colliders.get_mut(id.0) else {
            return;
        };
        if collider.parent().is_some() {
            return;
        }
        let finite = |rect: &&Rect| {
            [rect.x, rect.y, rect.w, rect.h]
                .iter()
                .all(|n| n.is_finite())
                && rect.w > 0.0
                && rect.h > 0.0
        };
        let boxes: Vec<(Isometry, SharedShape)> = rects
            .iter()
            .filter(finite)
            .map(|rect| {
                let half = Vector::new(rect.w / 2.0, rect.h / 2.0);
                let middle = Vector::new(rect.x, rect.y) + half;
                let at = Isometry::from_translation(middle);
                (at, SharedShape::cuboid(half.x, half.y))
            })
            .collect();
        // Rapier can't make a compound of nothing.
        if boxes.is_empty() {
            collider.set_enabled(false);
        } else {
            collider.set_shape(SharedShape::compound(boxes));
            collider.set_enabled(true);
        }
        collider.set_position(Isometry::from_translation(Vector::from(origin)));
        set_sensor(collider, false);
        self.fresh.push(id.0);
    }

    /// Adds `body`, and returns its collider, or fails if its shape can't be made, or its material
    /// isn't one: sizes that aren't finite, a density, friction or bounce less than 0, a dynamic
    /// body with a density of 0, or with no area, or a dynamic sensor.
    pub fn add_body(&mut self, body: &Body) -> Result<ColliderId, String> {
        check_material(&body.material, body.kind)?;
        if body.sensor {
            check_sensor_kind(body.kind)?;
        }
        let collider = body_collider(&body.shape, body.kind)?
            .density(body.material.density)
            .friction(body.material.friction)
            .restitution(body.material.bounce);
        let collider = if body.sensor {
            collider
                .sensor(true)
                .active_collision_types(ActiveCollisionTypes::empty())
        } else {
            collider
        };
        let builder = match body.kind {
            BodyKind::Fixed => RigidBodyBuilder::fixed(),
            BodyKind::Dynamic => RigidBodyBuilder::dynamic(),
            // Never asleep: Rapier expects what touches a body that's awake to be awake too,
            // and doesn't wake a kinematic body that dynamic ones bump into and leave.
            BodyKind::Kinematic => RigidBodyBuilder::kinematic_position_based().can_sleep(false),
        };
        let (_, collider) = self
            .rapier
            .insert(builder.pose(body.pose.to_rapier()), collider);
        self.fresh.push(collider);
        Ok(ColliderId(collider))
    }

    /// Reshapes the body whose collider is `id`, keeping its material, and if it's a sensor,
    /// keeping it one, as [`World::add_body`] shapes one. Does nothing to one that's been
    /// removed, or to a collider that isn't a body's.
    pub fn set_shape(&mut self, id: ColliderId, shape: &Shape) -> Result<(), String> {
        let Some(kind) = self.body_of(id).and(self.kind(id)) else {
            return Ok(());
        };
        let made = body_collider(shape, kind)?;
        let collider = &mut self.rapier.colliders[id.0];
        collider.set_shape(made.shape);
        collider.set_position_wrt_parent(made.position);
        self.fresh.push(id.0);
        if let Some(body) = self.body_of(id) {
            self.wake(body);
        }
        Ok(())
    }

    /// Gives the collider `id`, a fixed one's, tiles', or a body's, `material`, or fails as
    /// [`World::add_body`] does if it isn't one. A body's density changes how heavy it is.
    pub fn set_material(&mut self, id: ColliderId, material: &Material) -> Result<(), String> {
        let Some(kind) = self.kind(id) else {
            return Ok(());
        };
        check_material(material, kind)?;
        let collider = &mut self.rapier.colliders[id.0];
        collider.set_density(material.density);
        collider.set_friction(material.friction);
        collider.set_restitution(material.bounce);
        if let Some(body) = self.body_of(id) {
            self.wake(body);
        }
        Ok(())
    }

    /// Whether the collider `id` is a sensor: see [`Solid::Sensor`] and [`Body::sensor`].
    pub fn is_sensor(&self, id: ColliderId) -> bool {
        self.rapier
            .colliders
            .get(id.0)
            .is_some_and(Collider::is_sensor)
    }

    /// How the collider `id` moves: a body's as its kind says, and fixed colliders and tiles not
    /// at all. `None` for one that's been removed.
    pub fn kind(&self, id: ColliderId) -> Option<BodyKind> {
        let collider = self.rapier.colliders.get(id.0)?;
        let Some(parent) = collider.parent() else {
            return Some(BodyKind::Fixed);
        };
        let body = self.rapier.bodies.get(parent)?;
        Some(if body.is_dynamic() {
            BodyKind::Dynamic
        } else if body.is_kinematic() {
            BodyKind::Kinematic
        } else {
            BodyKind::Fixed
        })
    }

    /// The body whose collider is `id`, if it's a body's.
    fn body_of(&self, id: ColliderId) -> Option<RigidBodyHandle> {
        self.rapier.colliders.get(id.0)?.parent()
    }

    /// Where the body whose collider is `id` is, or `None` if it's been removed or isn't a body's.
    pub fn pose(&self, id: ColliderId) -> Option<Pose> {
        let body = self.rapier.bodies.get(self.body_of(id)?)?;
        Some(Pose::from_rapier(body.position()))
    }

    /// Puts the body whose collider is `id` at `pose`: a fixed or dynamic body, or a sensor, at
    /// once, keeping a dynamic one's velocity, and a kinematic one over the next step, pushing
    /// what's in its way. See [`World::put`] for putting a kinematic one there at once.
    pub fn set_pose(&mut self, id: ColliderId, pose: Pose) {
        // A sensor pushes nothing, so it needn't sweep there.
        let sensor = self.is_sensor(id);
        let Some(handle) = self.body_of(id) else {
            return;
        };
        match self.rapier.bodies.get_mut(handle) {
            Some(body) if body.is_kinematic() && !sensor => {
                body.set_next_kinematic_position(pose.to_rapier());
                self.wake(handle);
            }
            Some(_) => self.put(id, pose),
            None => {}
        }
    }

    /// Puts the body whose collider is `id` at `pose` at once, whatever its kind, keeping a
    /// dynamic one's velocity: a kinematic one doesn't go there over the next step, sweeping
    /// through what's between, as a character starting over shouldn't.
    pub fn put(&mut self, id: ColliderId, pose: Pose) {
        let Some(handle) = self.body_of(id) else {
            return;
        };
        let Some(body) = self.rapier.bodies.get_mut(handle) else {
            return;
        };
        // Where it is and where it's going next, so a kinematic one stays put.
        body.set_position(pose.to_rapier(), false);
        self.fresh.extend_from_slice(body.colliders());
        self.wake(handle);
    }

    /// How fast the body whose collider is `id` is moving, in units a second: a kinematic one as
    /// fast as it moved in the last step. Zero for colliders that aren't bodies'.
    pub fn velocity(&self, id: ColliderId) -> [f32; 2] {
        self.body_of(id)
            .and_then(|handle| self.rapier.bodies.get(handle))
            .map_or([0.0; 2], |body| body.linvel().into())
    }

    /// Sets how fast the dynamic body whose collider is `id` is moving. Does nothing to bodies of
    /// other kinds.
    pub fn set_velocity(&mut self, id: ColliderId, velocity: [f32; 2]) {
        let Some(handle) = self.body_of(id) else {
            return;
        };
        if let Some(body) = self.rapier.bodies.get_mut(handle)
            && body.is_dynamic()
        {
            body.set_linvel(Vector::from(velocity), false);
            self.wake(handle);
        }
    }

    /// Pushes the dynamic body whose collider is `id` through its center of mass by `impulse`,
    /// which changes its velocity by the impulse over its mass. Does nothing to bodies of other
    /// kinds.
    pub fn push(&mut self, id: ColliderId, impulse: [f32; 2]) {
        let Some(handle) = self.body_of(id) else {
            return;
        };
        if let Some(body) = self.rapier.bodies.get_mut(handle)
            && body.is_dynamic()
        {
            body.apply_impulse(Vector::from(impulse), false);
            self.wake(handle);
        }
    }

    /// Whether the body whose collider is `id` is at rest: a dynamic body that has been still for
    /// a couple of seconds, which the world then leaves alone until something touches it. Fixed
    /// colliders and bodies always are, and kinematic ones while they aren't being moved or turned.
    pub fn is_resting(&self, id: ColliderId) -> bool {
        // Rounding leaves a turned one that has stopped still moving by a hair, as long as most
        // things are long.
        const STILL: f32 = 1.0e-4;
        let Some(body) = self
            .body_of(id)
            .and_then(|handle| self.rapier.bodies.get(handle))
        else {
            return true;
        };
        let unit = self.rapier.integration_parameters.length_unit;
        let still = body.linvel().length() < STILL * unit && body.angvel().abs() < STILL;
        body.is_fixed() || body.is_sleeping() || (body.is_kinematic() && still)
    }

    /// Puts the colliders added or moved since the last step where Rapier's queries find them.
    ///
    /// Rapier catches up with them as it steps, and its `detect_collisions` would before then,
    /// but that forgets bodies added since its last step as the step that follows looks for them,
    /// so it never simulates them.
    fn refresh(&mut self) {
        for handle in self.fresh.drain(..) {
            let Some(collider) = self.rapier.colliders.get_mut(handle) else {
                continue;
            };
            if let (Some(parent), Some(offset)) =
                (collider.parent(), collider.position_wrt_parent())
                && let Some(body) = self.rapier.bodies.get(parent)
            {
                let pose = *body.position() * *offset;
                collider.set_position(pose);
            }
            // Queries skip what's disabled, wherever it was.
            if collider.is_enabled() {
                let aabb = collider.compute_aabb();
                self.rapier
                    .broad_phase
                    .set_aabb(&self.rapier.integration_parameters, handle, aabb);
            }
        }
    }

    /// Wakes the body `handle`, which Rapier must be told of, not the body, to simulate it again.
    fn wake(&mut self, handle: RigidBodyHandle) {
        if self.rapier.bodies.contains(handle) {
            self.rapier.wake_up(handle, true);
        }
    }

    /// Moves the world on by `dt` seconds: dynamic bodies fall, tumble and come to rest, and
    /// kinematic ones move where they've been put. Steps longer than a sixtieth of a second are
    /// taken in equal parts, up to 16, so bodies don't pass through each other, and kinematic
    /// bodies go an equal part of the way in each.
    pub fn step(&mut self, dt: f32) {
        if !(dt > 0.0 && dt.is_finite()) {
            return;
        }
        // A whole number of sixtieths, give or take a little, is that many.
        let parts = (dt / STEP - 1.0e-3).ceil().clamp(1.0, MAX_STEPS as f32) as u32;
        self.rapier.integration_parameters.dt = dt / parts as f32;
        // Rapier moves a kinematic body all the way to where it's been put in its next step, and
        // keeps it still in the steps after, so in parts it would go too fast, then stop.
        let moving: Vec<(RigidBodyHandle, Isometry, Isometry)> = if parts > 1 {
            self.rapier
                .bodies
                .iter()
                .filter(|(_, body)| body.is_kinematic() && body.position() != body.next_position())
                .map(|(handle, body)| (handle, *body.position(), *body.next_position()))
                .collect()
        } else {
            Vec::new()
        };
        for part in 1..=parts {
            for &(handle, from, to) in &moving {
                let pose = if part == parts {
                    to
                } else {
                    between(from, to, part as f32 / parts as f32)
                };
                if let Some(body) = self.rapier.bodies.get_mut(handle) {
                    body.set_next_kinematic_position(pose);
                }
            }
            self.rapier.step();
        }
        self.fresh.clear();
        self.last_step = dt;
    }

    /// Walks `character`, standing at `feet`, by up to `motion`, and returns how far it got and
    /// whether it's on the ground. The collider `own`, if given, is the character's own, which
    /// isn't in its way. `y` is down, so a character falls by a `motion` whose `y` is more than 0.
    ///
    /// On the ground, and not rising, it's first carried along with what it stands on, by as far
    /// as that moved where it stands in the last step, across, up or down, as a lift or a moving
    /// platform carries it, then walks across by the motion's `x`: up steps no higher than its
    /// [`Character::step`], up and down slopes no steeper than its [`Character::slope`], sliding
    /// along walls and steeper slopes, and staying on the ground as it goes down steps, unless it
    /// walks off a ledge, where it falls by the motion's `y`. Off the ground, or rising, it moves
    /// as `motion` says, landing on what it can stand on, sliding down what's too steep, and
    /// stopping at ceilings. Either way, it stops [`GAP`] short of what it hits.
    ///
    /// Dynamic bodies it walks into are pushed along at the speed it walks, taken over the last
    /// step's time, as hard as its mass and theirs make a collision: a heavy character pushes a
    /// light crate at its own speed, and a light one a heavy crate slowly.
    pub fn walk(
        &mut self,
        character: &Character,
        feet: [f32; 2],
        motion: [f32; 2],
        own: Option<ColliderId>,
    ) -> Walked {
        self.refresh();
        let mut walk = Walk {
            world: &self.rapier,
            character,
            shape: character.shape(),
            middle: Vector::from(feet) + UP * (character.height / 2.0),
            walkable: libm::cosf(character.slope),
            own: own.map(|own| own.0),
            last_step: self.last_step,
            pushes: Vec::new(),
        };
        let [x, y] = motion;
        let start = walk.middle;
        let ground = if y >= 0.0 { walk.ground() } else { None };
        let grounded = match ground {
            Some(ground) => {
                let carried = walk.carried(&ground);
                if carried != Vector::ZERO {
                    walk.slide(carried, Slide::Free);
                    // Carrying it pushes nothing: what carries it does, if it's in the way.
                    walk.pushes.clear();
                }
                walk.walk_across(x, y)
            }
            None => walk.slide(Vector::new(x, y), Slide::Free).landed,
        };
        let moved = walk.middle - start;
        let pushes = std::mem::take(&mut walk.pushes);
        // Pushed at the speed it walked into them.
        let dt = self.last_step;
        for push in pushes {
            self.push_away(character, push, Vector::new(x / dt, 0.0));
        }
        Walked {
            moved: moved.into(),
            grounded,
        }
    }

    /// Pushes what a character walking at `velocity` walked into, as a collision of the two
    /// would, along the ground.
    fn push_away(&mut self, character: &Character, push: Push, velocity: Vector) {
        let Some(body) = self.rapier.bodies.get_mut(push.body) else {
            return;
        };
        // Into it, along the ground.
        let into = Vector::new(-push.normal.x, 0.0);
        let length = into.length();
        if length < 1.0e-6 {
            return;
        }
        let into = into / length;
        // At the height of its center of mass, so it slides rather than tipping over, though it
        // turns when pushed off center.
        let point = Vector::new(push.point.x, body.center_of_mass().y);
        let wanted = velocity.dot(into) - body.velocity_at_point(point).dot(into);
        if wanted <= 0.0 {
            return;
        }
        let mass = body.mass();
        let reduced = mass * character.mass / (mass + character.mass);
        body.apply_impulse_at_point(into * (wanted * reduced), point, false);
        self.wake(push.body);
    }

    /// Casts a ray from `origin` along `direction`, as far as `max_distance` lengths of it, and
    /// returns where it first hits a collider, other than those in `except` and sensors, or
    /// `None` if it hits nothing. A ray starting inside a collider solid all over hits it at 0,
    /// and edges are hit from either side.
    pub fn cast_ray(
        &mut self,
        origin: [f32; 2],
        direction: [f32; 2],
        max_distance: f32,
        except: &[ColliderId],
    ) -> Option<RayHit> {
        self.refresh();
        let direction = Vector::from(direction);
        let ray = Ray::new(Vector::from(origin), direction);
        let excepted = |handle: ColliderHandle, collider: &Collider| {
            collider.is_enabled() && !except.contains(&ColliderId(handle))
        };
        let filter = QueryFilter::default()
            .exclude_sensors()
            .predicate(&excepted);
        let queries = self.rapier.query_pipeline_with_filter(filter);
        let (handle, hit) = queries.cast_ray_and_get_normal(&ray, max_distance, true)?;
        // Rapier finds where a ray hits a polygon by iterating, as it does a box, so its normal
        // is one of the polygon's sides' within a hair.
        let normal = true_normal(hit.normal, &queries.colliders[handle]);
        // Back toward where the ray came from, whichever side of an edge it hit, and none from
        // inside.
        let normal = if hit.time_of_impact <= 0.0 {
            Vector::ZERO
        } else if normal.dot(direction) > 0.0 {
            -normal
        } else {
            normal
        };
        Some(RayHit {
            collider: ColliderId(handle),
            distance: hit.time_of_impact,
            point: ray.point_at(hit.time_of_impact).into(),
            normal: normal.into(),
        })
    }

    /// Returns the colliders that `rect` placed by `transform`, as [`World::add`] places one,
    /// touches or overlaps, other than those in `except` and sensors, in no particular order:
    /// none for a rectangle with no area. Edges count only where it touches them, not if it's
    /// inside them.
    pub fn overlapping(
        &mut self,
        transform: Affine,
        rect: Rect,
        except: &[ColliderId],
    ) -> Vec<ColliderId> {
        let Some(shape) = shape(transform, rect, Solid::Whole) else {
            return Vec::new();
        };
        self.refresh();
        self.touching(Isometry::IDENTITY, &*shape, except)
    }

    /// Returns the colliders that the circle around `center` touches or overlaps, other than
    /// those in `except` and sensors, in no particular order: none for a radius that isn't more
    /// than 0.
    pub fn overlapping_circle(
        &mut self,
        center: [f32; 2],
        radius: f32,
        except: &[ColliderId],
    ) -> Vec<ColliderId> {
        if !(radius > 0.0 && radius.is_finite()) {
            return Vec::new();
        }
        self.refresh();
        let at = Isometry::from_translation(Vector::from(center));
        self.touching(at, &Ball::new(radius), except)
    }

    /// Returns the colliders that overlap or touch the collider `id`, a sensor's or any other's,
    /// other than sensors, in no particular order: for a sensor, what's in it.
    pub fn inside(&mut self, id: ColliderId) -> Vec<ColliderId> {
        self.refresh();
        let Some(collider) = self.rapier.colliders.get(id.0) else {
            return Vec::new();
        };
        if !collider.is_enabled() {
            return Vec::new();
        }
        self.touching(*collider.position(), collider.shape(), &[id])
    }

    /// The colliders `shape` at `pose` touches, other than those in `except` and sensors.
    fn touching(
        &self,
        pose: Isometry,
        shape: &dyn ParryShape,
        except: &[ColliderId],
    ) -> Vec<ColliderId> {
        let excepted = |handle: ColliderHandle, collider: &Collider| {
            collider.is_enabled() && !except.contains(&ColliderId(handle))
        };
        let filter = QueryFilter::default()
            .exclude_sensors()
            .predicate(&excepted);
        let queries = self.rapier.query_pipeline_with_filter(filter);
        queries
            .intersect_shape(pose, shape)
            .map(|(handle, _)| ColliderId(handle))
            .collect()
    }

    /// Removes the collider `id`, and if it's a body's, the body, if it hasn't been already.
    pub fn remove(&mut self, id: ColliderId) {
        match self.body_of(id) {
            Some(body) => {
                self.rapier.remove_body(body);
            }
            None => {
                self.rapier.remove_collider(id.0);
            }
        }
    }

    /// How many colliders there are.
    pub fn len(&self) -> usize {
        self.rapier.colliders.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Moves the box `rect` by up to `motion`, as a character walks, and returns how far it got:
    /// all of `motion` unless it hits something solid, where it stops, then slides along what it
    /// hit as far as `motion` goes that way. It stops [`GAP`] short of what it hits, bodies too,
    /// which it doesn't push. A box that starts inside something solid moves only out of it, or
    /// along it. The collider `except`, if given, isn't in its way, as a character's own isn't,
    /// and nor are sensors.
    ///
    /// Rapier's own character controller does this for characters that walk on floors, but its
    /// casts keep their gap by rounding the box, which turns what it slides along a little, so a
    /// box pushed straight at a wall would creep along it. This casts the box as it is.
    pub fn slide(&mut self, rect: Rect, motion: [f32; 2], except: Option<ColliderId>) -> [f32; 2] {
        self.refresh();
        let half = Vector::new(rect.w / 2.0, rect.h / 2.0);
        let shape = Cuboid::new(half);
        let start = Vector::new(rect.x, rect.y) + half;
        let usable = |handle: ColliderHandle, collider: &Collider| {
            collider.is_enabled() && except != Some(ColliderId(handle))
        };
        let filter = QueryFilter::default().exclude_sensors().predicate(&usable);
        let queries = self.rapier.query_pipeline_with_filter(filter);
        let options = ShapeCastOptions {
            target_distance: 0.0,
            stop_at_penetration: false,
            compute_impact_geometry_on_penetration: true,
            ..ShapeCastOptions::default()
        };
        let mut moved = Vector::ZERO;
        let mut left = Vector::from(motion);
        for _ in 0..MAX_HITS {
            let distance = left.length();
            if distance == 0.0 {
                break;
            }
            let direction = left / distance;
            let at = Isometry::from_translation(start + moved);
            let options = ShapeCastOptions {
                max_time_of_impact: distance,
                ..options
            };
            let Some((handle, hit)) = queries.cast_shape(&at, direction, &shape, options) else {
                moved += left;
                break;
            };
            // Out of what it hit, toward the box.
            let normal = true_normal(hit.normal1, &queries.colliders[handle]);
            // To where it touches, then back out of it by the gap.
            moved += direction * hit.time_of_impact + normal * GAP;
            left -= direction * hit.time_of_impact;
            // The rest of the way, without the part going into what it hit.
            let into = left.dot(normal);
            if into < 0.0 {
                left -= normal * into;
            }
        }
        moved.into()
    }
}

/// Makes `collider` a sensor, or not, as `sensor` says: one that nothing bumps into, and that the
/// world's steps leave alone, since what's in it is asked for when it's wanted.
fn set_sensor(collider: &mut Collider, sensor: bool) {
    collider.set_sensor(sensor);
    collider.set_active_collision_types(if sensor {
        ActiveCollisionTypes::empty()
    } else {
        ActiveCollisionTypes::default()
    });
}

/// A dynamic body a character walked into, where, and which way out of it.
struct Push {
    body: RigidBodyHandle,
    point: Vector,
    normal: Vector,
}

/// A character's walk in progress.
struct Walk<'a> {
    world: &'a PhysicsWorld,
    character: &'a Character,
    shape: Cuboid,
    /// Where the box's middle is.
    middle: Vector,
    /// The least a ground's normal points up, as the cosine of the steepest slope.
    walkable: f32,
    own: Option<ColliderHandle>,
    /// How long the world's last step was, over which what it stands on may have moved.
    last_step: f32,
    pushes: Vec<Push>,
}

/// Where a cast hit something: how far along it went, the collider, and the normal out of it.
struct Hit {
    distance: f32,
    collider: ColliderHandle,
    normal: Vector,
    point: Vector,
}

impl Walk<'_> {
    /// Casts the box from where it is along `direction`, a unit vector, up to `distance`.
    fn cast(&self, direction: Vector, distance: f32) -> Option<Hit> {
        let own = self.own;
        let usable = |handle: ColliderHandle, collider: &Collider| {
            collider.is_enabled() && own != Some(handle)
        };
        let filter = QueryFilter::default().exclude_sensors().predicate(&usable);
        let queries = self.world.query_pipeline_with_filter(filter);
        let options = ShapeCastOptions {
            max_time_of_impact: distance,
            target_distance: 0.0,
            stop_at_penetration: false,
            compute_impact_geometry_on_penetration: true,
        };
        let at = Isometry::from_translation(self.middle);
        let (collider, hit) = queries.cast_shape(&at, direction, &self.shape, options)?;
        let normal = true_normal(hit.normal1, &queries.colliders[collider]);
        Some(Hit {
            distance: hit.time_of_impact,
            collider,
            normal,
            point: hit.witness1,
        })
    }

    /// Whether what `hit` hit, there, is something the character can stand on.
    fn standing(&self, hit: &Hit) -> bool {
        -hit.normal.y >= self.walkable
    }

    /// What the character stands on, if it stands on something it can stand on: just below it, a
    /// gap away, or as much farther as that moved down in the last step, as a lift going down
    /// does, up to a step.
    fn ground(&self) -> Option<Hit> {
        if let Some(hit) = self.cast(DOWN, REACH) {
            return self.standing(&hit).then_some(hit);
        }
        let hit = self.cast(DOWN, REACH + self.character.step)?;
        let sank = self.carried(&hit).y.max(0.0);
        (self.standing(&hit) && hit.distance <= REACH + sank).then_some(hit)
    }

    /// How far what `hit` hit moved where it was hit, in the last step.
    fn carried(&self, hit: &Hit) -> Vector {
        let Some(parent) = self.world.colliders[hit.collider].parent() else {
            return Vector::ZERO;
        };
        self.world.bodies[parent].velocity_at_point(hit.point) * self.last_step
    }

    /// Notes that `hit` pushed what it hit, if that's a dynamic body.
    fn note_push(&mut self, hit: &Hit) {
        let Some(body) = self.world.colliders[hit.collider].parent() else {
            return;
        };
        if self.world.bodies[body].is_dynamic() {
            self.pushes.push(Push {
                body,
                point: hit.point,
                normal: hit.normal,
            });
        }
    }

    /// Walks across the ground by `across`, along it, stepping up onto what's low enough and down
    /// with the ground, or falling by `fall` off a ledge. Returns whether it ended on the ground.
    fn walk_across(&mut self, across: f32, fall: f32) -> bool {
        // Standing still, it stays where it stands.
        if across == 0.0 {
            return true;
        }
        let start = self.middle;
        let gone = |middle: Vector| (middle.x - start.x).abs();
        // Along the ground, up slopes it can walk up.
        let slid = self.slide(Vector::new(across, 0.0), Slide::Along);
        let low = (self.middle, std::mem::take(&mut self.pushes));
        // Something was in the way: over it, if it's low enough, and that goes farther.
        if slid.hit {
            self.middle = start;
            let high = self.step_over(across);
            if high.is_some_and(|middle| gone(middle) > gone(low.0)) {
                return true;
            }
        }
        (self.middle, self.pushes) = low;
        // Down to the ground, as far as a step, to follow it down slopes and steps, or as far as
        // it falls.
        match self.cast(DOWN, self.character.step + fall) {
            Some(hit) if self.standing(&hit) => {
                self.middle.y += hit.distance - GAP;
                true
            }
            // Too steep to stand on, but there without stepping down: it falls onto it.
            Some(hit) if hit.distance <= fall => {
                self.middle.y += hit.distance - GAP;
                false
            }
            // Off a ledge.
            _ => {
                self.middle.y += fall;
                false
            }
        }
    }

    /// Steps up as high as a step, or a ceiling, lets it, across by `across` sliding along
    /// whatever's still in the way, and down onto what it can stand on, and returns where its
    /// middle ends up, or `None` if there's nothing there to stand on within a step down.
    fn step_over(&mut self, across: f32) -> Option<Vector> {
        let rise = match self.cast(UP, self.character.step) {
            Some(hit) => (hit.distance - GAP).max(0.0),
            None => self.character.step,
        };
        self.middle.y -= rise;
        self.slide(Vector::new(across, 0.0), Slide::Upright);
        let hit = self
            .cast(DOWN, rise + self.character.step)
            .filter(|hit| self.standing(hit))?;
        self.middle.y += hit.distance - GAP;
        Some(self.middle)
    }

    /// Moves by `motion`, stopping at what it hits and sliding along it as `how` says.
    fn slide(&mut self, motion: Vector, how: Slide) -> Slid {
        let mut left = motion;
        let mut slid = Slid::default();
        for _ in 0..MAX_HITS {
            let distance = left.length();
            if distance == 0.0 {
                break;
            }
            let direction = left / distance;
            let Some(hit) = self.cast(direction, distance) else {
                self.middle += left;
                break;
            };
            self.note_push(&hit);
            slid.hit = true;
            // To where it touches, then back out of it by the gap.
            self.middle += direction * hit.distance + hit.normal * GAP;
            left -= direction * hit.distance;
            let standing = self.standing(&hit);
            if how == Slide::Free && standing && left.y > 0.0 {
                // Landed: the rest of the way is across the ground.
                slid.landed = true;
                left.y = 0.0;
            }
            let normal = if how == Slide::Upright || (how == Slide::Along && !standing) {
                if hit.normal.x.abs() < 1.0e-6 {
                    break;
                }
                Vector::new(hit.normal.x.signum(), 0.0)
            } else {
                hit.normal
            };
            // The rest of the way, without the part going into what it hit.
            let into = left.dot(normal);
            if into < 0.0 {
                left -= normal * into;
            }
        }
        slid
    }
}

/// How a slide went.
#[derive(Clone, Copy, Default)]
struct Slid {
    /// Whether it hit anything.
    hit: bool,
    /// Whether it landed on something to stand on.
    landed: bool,
}

/// How a character slides along what it hits.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Slide {
    /// Along all of it, as one off the ground does.
    Free,
    /// Along the ground, up slopes no steeper than it walks up, and along walls and steeper
    /// slopes as though they were upright, rather than climbing them.
    Along,
    /// Along everything as though it were upright, as one stepping over something does.
    Upright,
}

/// The pose `t` of the way from `from` to `to`, turned along the way by normalizing the
/// rotations' blend, which takes only a square root, so it gives the same bits everywhere.
fn between(from: Isometry, to: Isometry, t: f32) -> Isometry {
    let translation = from.translation + (to.translation - from.translation) * t;
    // Unturned, it stays exactly as it was, rather than as normalizing rounds it.
    let rotation = if from.rotation == to.rotation {
        from.rotation
    } else {
        let re = from.rotation.re + (to.rotation.re - from.rotation.re) * t;
        let im = from.rotation.im + (to.rotation.im - from.rotation.im) * t;
        let length = (re * re + im * im).sqrt();
        Rotation::from_cos_sin_unchecked(re / length, im / length)
    };
    Isometry::from_parts(translation, rotation)
}

/// The normal of a side of what `hit` is, or of the box that hit it, which `normal` is within a
/// hair of, or `normal` itself if none is. Rapier works out where a box hits something by
/// iterating (GJK), which finds the normal there only to within about a thousandth of a radian,
/// and a box pushed at a wall would creep along it by that much each time.
fn true_normal(normal: Vector, hit: &Collider) -> Vector {
    const HAIR: f32 = 2.0e-3;
    let near = |side: &Vector| normal.dot(*side) > 0.0 && normal.perp_dot(*side).abs() < HAIR;
    let rotation = hit.position().rotation;
    let shape = hit.shape();
    // What a box hits is solid inside its sides or along them, from either side.
    let theirs: Vec<Vector> = if let Some(polygon) = shape.as_convex_polygon() {
        polygon
            .normals()
            .iter()
            .map(|&side| rotation * side)
            .collect()
    } else if let Some(polyline) = shape.as_polyline() {
        let normals = polyline.segments().filter_map(|segment| segment.normal());
        normals
            .flat_map(|side| [rotation * side, -(rotation * side)])
            .collect()
    } else if shape.as_cuboid().is_some() || shape.as_compound().is_some() {
        // A box's sides, or tiles', which are boxes along the collider's axes.
        BOX_SIDES.iter().map(|&side| rotation * side).collect()
    } else {
        Vec::new()
    };
    theirs
        .iter()
        .chain(&BOX_SIDES)
        .find(|side| near(side))
        .copied()
        .unwrap_or(normal)
}

/// The shape of `rect` placed by `transform`, in the world's coordinates, or `None` for one with
/// no area.
fn shape(transform: Affine, rect: Rect, solid: Solid) -> Option<SharedShape> {
    let corners = transform.corners(rect).map(Vector::from);
    let area = (corners[1] - corners[0]).perp_dot(corners[3] - corners[0]);
    if area == 0.0 || !area.is_finite() {
        return None;
    }
    match solid {
        Solid::Whole | Solid::Sensor => SharedShape::convex_hull(&corners),
        Solid::Edges => Some(SharedShape::polyline(
            corners.to_vec(),
            Some(vec![[0, 1], [1, 2], [2, 3], [3, 0]]),
        )),
    }
}

/// A collider of `shape`, for a body of `kind`.
fn body_collider(shape: &Shape, kind: BodyKind) -> Result<ColliderBuilder, String> {
    let finite = |values: &[f32]| values.iter().all(|value| value.is_finite());
    let (collider, area) = match *shape {
        Shape::Box(rect) => {
            if !finite(&[rect.x, rect.y, rect.w, rect.h]) {
                return Err("a box's corners must be finite".into());
            }
            if rect.w < 0.0 || rect.h < 0.0 {
                return Err(format!(
                    "a box must be 0 or more wide and tall, not {} by {}",
                    rect.w, rect.h
                ));
            }
            let half = Vector::new(rect.w / 2.0, rect.h / 2.0);
            let middle = Vector::new(rect.x, rect.y) + half;
            let collider = ColliderBuilder::new(SharedShape::cuboid(half.x, half.y))
                .position(Isometry::from_translation(middle));
            (collider, rect.w * rect.h)
        }
        Shape::Circle { center, radius } => {
            if !(finite(&center) && radius.is_finite() && radius > 0.0) {
                return Err(format!("a circle needs a radius more than 0, not {radius}"));
            }
            let collider = ColliderBuilder::new(SharedShape::ball(radius))
                .position(Isometry::from_translation(Vector::from(center)));
            (collider, radius * radius)
        }
    };
    // What's flat weighs nothing, which only what doesn't fall can.
    if kind == BodyKind::Dynamic && area <= 0.0 {
        return Err("it's flat, with nothing inside to weigh: give what falls some area".into());
    }
    Ok(collider)
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_4;

    use super::*;

    /// A 2x2 box with its top-left corner at `x`, `y`.
    fn small_box(x: f32, y: f32) -> Rect {
        Rect::new(x, y, 2.0, 2.0)
    }

    /// A wall from 10 to 20 across, and from 0 to 100 down.
    fn walled() -> World {
        let mut world = World::new();
        world.add(
            Affine::IDENTITY,
            Rect::new(10.0, 0.0, 10.0, 100.0),
            Solid::Whole,
        );
        world
    }

    fn assert_near(actual: [f32; 2], expected: [f32; 2]) {
        assert_within(actual, expected, 0.02);
    }

    fn assert_within(actual: [f32; 2], expected: [f32; 2], tolerance: f32) {
        let off = (actual[0] - expected[0])
            .abs()
            .max((actual[1] - expected[1]).abs());
        assert!(off <= tolerance, "{actual:?}, not {expected:?}");
    }

    #[test]
    fn a_box_moves_all_the_way_when_nothing_is_in_its_way() {
        let mut world = walled();
        assert_eq!(
            world.slide(small_box(0.0, 50.0), [5.0, -3.25], None),
            [5.0, -3.25]
        );
        assert_eq!(
            world.slide(small_box(30.0, 50.0), [-8.0, 0.0], None),
            [-8.0, 0.0]
        );
    }

    #[test]
    fn a_box_stops_at_what_is_solid() {
        let mut world = walled();
        // Its right edge, at 2, reaches the wall at 10 after 8.
        assert_near(
            world.slide(small_box(0.0, 50.0), [20.0, 0.0], None),
            [8.0 - GAP, 0.0],
        );
        // From the other side.
        assert_near(
            world.slide(small_box(25.0, 50.0), [-20.0, 0.0], None),
            [-5.0 + GAP, 0.0],
        );
    }

    #[test]
    fn pushing_straight_at_something_never_creeps_along_it() {
        let mut world = walled();
        let edges = Rect::new(-50.0, -50.0, 100.0, 200.0);
        world.add(Affine::IDENTITY, edges, Solid::Edges);
        for (start, motion) in [
            ([0.0, 50.0], [1.0, 0.0]),
            ([25.0, 50.0], [-1.0, 0.0]),
            ([15.0, 110.0], [0.0, -1.0]),
            ([-45.0, 0.0], [-1.0, 0.0]),
            ([0.0, 145.0], [0.0, 1.0]),
        ] {
            let mut at = small_box(start[0], start[1]);
            for _ in 0..600 {
                let [dx, dy] = world.slide(at, motion, None);
                at.x += dx;
                at.y += dy;
            }
            let across = if motion[0] == 0.0 { at.x } else { at.y };
            let along = if motion[0] == 0.0 { start[0] } else { start[1] };
            assert_eq!(across, along, "pushed {motion:?} from {start:?} to {at:?}");
        }

        // A wall turned a twelfth of a turn, pushed at straight on from above its upper side.
        let mut world = World::new();
        let angle = std::f32::consts::FRAC_PI_6;
        let turned = Affine::from_parts([100.0, 100.0], angle, [1.0, 1.0]);
        world.add(turned, Rect::new(-50.0, -5.0, 100.0, 10.0), Solid::Whole);
        let (along, out) = ([angle.cos(), angle.sin()], [angle.sin(), -angle.cos()]);
        let mut at = Rect::new(
            100.0 + out[0] * 20.0 - 1.0,
            100.0 + out[1] * 20.0 - 1.0,
            2.0,
            2.0,
        );
        let start = at.x * along[0] + at.y * along[1];
        for _ in 0..600 {
            let [dx, dy] = world.slide(at, [-out[0], -out[1]], None);
            at.x += dx;
            at.y += dy;
        }
        let crept = at.x * along[0] + at.y * along[1] - start;
        assert!(crept.abs() < 1.0e-3, "crept {crept} along the turned wall");
        // And it reached the wall, 5 out from its middle, with its nearest corner: the box's middle
        // is as far again as the box reaches that way.
        let reached = (at.x + 1.0 - 100.0) * out[0] + (at.y + 1.0 - 100.0) * out[1];
        let expected = 5.0 + GAP + out[0].abs() + out[1].abs();
        assert!(
            (reached - expected).abs() < 0.01,
            "{reached}, not {expected}"
        );
    }

    #[test]
    fn a_box_slides_along_what_it_hits() {
        let mut world = walled();
        // Against the wall, the part along it carries on.
        assert_near(
            world.slide(small_box(0.0, 50.0), [12.0, 12.0], None),
            [8.0 - GAP, 12.0],
        );
        // Around its end, once past it.
        assert_near(
            world.slide(small_box(0.0, 90.0), [12.0, 20.0], None),
            [12.0, 20.0],
        );
    }

    #[test]
    fn turned_colliders_are_slid_along() {
        // A square 10 wide turned to a diamond, its corners 7.07 from its middle at 50, 50.
        let mut world = World::new();
        let turned = Affine::from_parts([50.0, 50.0], std::f32::consts::FRAC_PI_4, [1.0, 1.0]);
        world.add(turned, Rect::new(-5.0, -5.0, 10.0, 10.0), Solid::Whole);
        // Moving right into its upper left side, it's deflected up the slope, so it ends up
        // higher and short of where it was going across.
        let [x, y] = world.slide(Rect::new(40.0, 44.0, 1.0, 1.0), [10.0, 0.0], None);
        assert!(x > 0.0 && x < 10.0, "{x}");
        assert!(y < 0.0, "{y}");
    }

    #[test]
    fn edges_keep_a_box_inside() {
        let mut world = World::new();
        world.add(
            Affine::IDENTITY,
            Rect::new(0.0, 0.0, 100.0, 50.0),
            Solid::Edges,
        );
        assert_near(
            world.slide(small_box(10.0, 10.0), [-20.0, 0.0], None),
            [-10.0 + GAP, 0.0],
        );
        assert_near(
            world.slide(small_box(10.0, 10.0), [0.0, 60.0], None),
            [0.0, 38.0 - GAP],
        );
        // Inside, nothing's in the way.
        assert_eq!(
            world.slide(small_box(10.0, 10.0), [50.0, 20.0], None),
            [50.0, 20.0]
        );
    }

    #[test]
    fn colliders_move_and_go() {
        let mut world = walled();
        let id = world.add(
            Affine::IDENTITY,
            Rect::new(0.0, 20.0, 10.0, 5.0),
            Solid::Whole,
        );
        assert_near(
            world.slide(small_box(2.0, 10.0), [0.0, 20.0], None),
            [0.0, 8.0 - GAP],
        );

        world.set(
            id,
            Affine::from_translation([0.0, 10.0]),
            Rect::new(0.0, 20.0, 10.0, 5.0),
            Solid::Whole,
        );
        assert_near(
            world.slide(small_box(2.0, 10.0), [0.0, 20.0], None),
            [0.0, 18.0 - GAP],
        );

        world.remove(id);
        assert_eq!(world.len(), 1);
        assert_eq!(
            world.slide(small_box(2.0, 10.0), [0.0, 20.0], None),
            [0.0, 20.0]
        );
        // Removing it again does nothing, nor does setting it.
        world.remove(id);
        world.set(
            id,
            Affine::IDENTITY,
            Rect::new(0.0, 0.0, 1.0, 1.0),
            Solid::Whole,
        );
        assert_eq!(world.len(), 1);
    }

    #[test]
    fn a_box_is_not_in_its_own_way() {
        let mut world = walled();
        let own = world.add(Affine::IDENTITY, small_box(0.0, 50.0), Solid::Whole);
        let moved = world.slide(small_box(0.0, 50.0), [-5.0, 3.0], Some(own));
        assert_eq!(moved, [-5.0, 3.0]);
        // The rest still are.
        let moved = world.slide(small_box(0.0, 50.0), [20.0, 0.0], Some(own));
        assert_near(moved, [8.0 - GAP, 0.0]);
    }

    #[test]
    fn a_collider_squashed_flat_is_solid_nowhere() {
        let mut world = World::new();
        let flat = Affine::from_parts([0.0, 0.0], 0.0, [1.0, 0.0]);
        let rect = Rect::new(10.0, 0.0, 10.0, 100.0);
        let id = world.add(Affine::IDENTITY, rect, Solid::Whole);
        // Solid, then squashed after it was found once, it's found no more.
        assert_near(
            world.slide(small_box(0.0, 0.0), [30.0, 0.0], None),
            [8.0 - GAP, 0.0],
        );
        world.set(id, flat, rect, Solid::Whole);
        assert_eq!(
            world.slide(small_box(0.0, 0.0), [30.0, 0.0], None),
            [30.0, 0.0]
        );
        assert_eq!(world.cast_ray([0.0, 1.0], [1.0, 0.0], f32::MAX, &[]), None);
        world.set(id, Affine::IDENTITY, rect, Solid::Whole);
        assert_near(
            world.slide(small_box(0.0, 0.0), [30.0, 0.0], None),
            [8.0 - GAP, 0.0],
        );
        let flat_from_the_start = world.add(flat, rect, Solid::Whole);
        world.remove(id);
        assert_eq!(
            world.slide(small_box(0.0, 0.0), [30.0, 0.0], None),
            [30.0, 0.0]
        );
        assert!(!world.is_sensor(flat_from_the_start));
    }

    fn set(ids: Vec<ColliderId>) -> std::collections::HashSet<ColliderId> {
        ids.into_iter().collect()
    }

    #[test]
    fn rays_hit_the_nearest_collider() {
        // Colliders added since anything was last asked, as these are, are where they are.
        let mut world = World::new();
        let wall = Rect::new(10.0, 0.0, 10.0, 100.0);
        let near = world.add(Affine::IDENTITY, wall, Solid::Whole);
        let far = world.add(
            Affine::IDENTITY,
            Rect::new(30.0, 0.0, 10.0, 100.0),
            Solid::Whole,
        );
        // In lengths of its direction.
        let hit = world.cast_ray([0.0, 50.0], [2.0, 0.0], f32::MAX, &[]);
        let hit = hit.expect("the ray hit the wall");
        assert_eq!(hit.collider, near);
        assert!((hit.distance - 5.0).abs() < 1.0e-3, "{hit:?}");
        assert_near(hit.point, [10.0, 50.0]);
        assert_eq!(hit.normal, [-1.0, 0.0]);
        // Past it, or not as far as it.
        let hit = world.cast_ray([0.0, 50.0], [1.0, 0.0], f32::MAX, &[near]);
        assert_eq!(hit.map(|hit| hit.collider), Some(far));
        assert_eq!(world.cast_ray([0.0, 50.0], [1.0, 0.0], 9.0, &[]), None);
        assert_eq!(
            world.cast_ray([0.0, 50.0], [-1.0, 0.0], f32::MAX, &[]),
            None
        );
        // From inside, at once, with no normal.
        let hit = world
            .cast_ray([15.0, 50.0], [1.0, 0.0], f32::MAX, &[])
            .unwrap();
        assert_eq!(
            (hit.collider, hit.distance, hit.normal),
            (near, 0.0, [0.0, 0.0])
        );
    }

    #[test]
    fn rays_hit_turned_colliders_on_their_turned_sides_and_edges_from_inside() {
        // A square 10 wide turned to a diamond, its corners 7.07 from its middle at 50, 50.
        let mut world = World::new();
        let turned = Affine::from_parts([50.0, 50.0], std::f32::consts::FRAC_PI_4, [1.0, 1.0]);
        world.add(turned, Rect::new(-5.0, -5.0, 10.0, 10.0), Solid::Whole);
        // Above its left corner, onto its upper left side.
        let hit = world
            .cast_ray([0.0, 48.0], [1.0, 0.0], f32::MAX, &[])
            .unwrap();
        let half = std::f32::consts::FRAC_1_SQRT_2;
        assert!(
            (hit.distance - (50.0 - 10.0 * half + 2.0)).abs() < 1.0e-3,
            "{hit:?}"
        );
        let [x, y] = hit.normal;
        assert!(
            (x + half).abs() < 1.0e-5 && (y.abs() - half).abs() < 1.0e-5,
            "{hit:?}"
        );

        // A fence, hit from inside it, facing back in.
        let mut world = World::new();
        let field = Rect::new(0.0, 0.0, 100.0, 50.0);
        world.add(Affine::IDENTITY, field, Solid::Edges);
        let hit = world
            .cast_ray([50.0, 25.0], [0.0, 1.0], f32::MAX, &[])
            .unwrap();
        assert_eq!((hit.distance, hit.normal), (25.0, [0.0, -1.0]));
    }

    #[test]
    fn rays_go_through_sensors_and_flat_colliders() {
        let mut world = walled();
        let area = Rect::new(4.0, 0.0, 2.0, 100.0);
        world.add(Affine::IDENTITY, area, Solid::Sensor);
        let flat = Affine::from_parts([0.0, 0.0], 0.0, [0.0, 1.0]);
        world.add(flat, Rect::new(7.0, 0.0, 2.0, 100.0), Solid::Whole);
        let hit = world
            .cast_ray([0.0, 50.0], [1.0, 0.0], f32::MAX, &[])
            .unwrap();
        assert!((hit.distance - 10.0).abs() < 1.0e-3, "{hit:?}");
    }

    #[test]
    fn overlaps_find_what_they_touch() {
        let mut world = walled();
        let wall = ColliderId(world.rapier.colliders.iter().next().unwrap().0);
        let fence = world.add(
            Affine::IDENTITY,
            Rect::new(-50.0, -50.0, 100.0, 200.0),
            Solid::Edges,
        );
        world.add(
            Affine::IDENTITY,
            Rect::new(0.0, 0.0, 9.0, 9.0),
            Solid::Sensor,
        );
        // Reaching the wall's side at 10, and short of it; the sensor it's in doesn't count, nor
        // the fence around it.
        let reaching = world.overlapping_circle([7.0, 5.0], 3.5, &[]);
        assert_eq!(reaching, vec![wall]);
        assert_eq!(world.overlapping_circle([7.0, 5.0], 2.5, &[]), vec![]);
        // The wall's top, at 0, and the fence's edge, at -50.
        let both = world.overlapping_circle([10.0, -25.0], 26.0, &[]);
        assert_eq!(set(both), set(vec![wall, fence]));
        let but = world.overlapping_circle([10.0, -25.0], 26.0, &[wall]);
        assert_eq!(but, vec![fence]);
        // A rectangle turned a quarter turn about its corner, reaching into the wall.
        let turned = Affine::from_parts([15.0, 60.0], std::f32::consts::FRAC_PI_2, [1.0, 1.0]);
        let found = world.overlapping(turned, Rect::new(0.0, 0.0, 4.0, 10.0), &[]);
        assert_eq!(found, vec![wall]);
        let flat = Rect::new(0.0, 60.0, 15.0, 0.0);
        assert_eq!(world.overlapping(Affine::IDENTITY, flat, &[]), vec![]);
        assert_eq!(world.overlapping_circle([12.0, 5.0], 0.0, &[]), vec![]);
    }

    #[test]
    fn sensors_tell_what_is_in_them_and_stop_nothing() {
        let mut world = World::new();
        let area = Rect::new(0.0, 0.0, 20.0, 20.0);
        let sensor = world.add(Affine::IDENTITY, area, Solid::Sensor);
        assert!(world.is_sensor(sensor));
        // A box slides through it.
        assert_eq!(
            world.slide(small_box(-10.0, 5.0), [40.0, 0.0], None),
            [40.0, 0.0]
        );
        assert_eq!(world.inside(sensor), vec![]);
        // What's in it, even partly, other than sensors.
        let crate_in = world.add(Affine::IDENTITY, small_box(19.0, 5.0), Solid::Whole);
        assert!(!world.is_sensor(crate_in));
        world.add(Affine::IDENTITY, small_box(5.0, 5.0), Solid::Sensor);
        world.add(Affine::IDENTITY, small_box(30.0, 5.0), Solid::Whole);
        assert_eq!(world.inside(sensor), vec![crate_in]);
        // Moved away, the crate isn't; set again, the sensor stays one.
        world.set(
            crate_in,
            Affine::from_translation([10.0, 0.0]),
            small_box(19.0, 5.0),
            Solid::Whole,
        );
        assert_eq!(world.inside(sensor), vec![]);
        world.set(
            sensor,
            Affine::IDENTITY,
            Rect::new(0.0, 0.0, 40.0, 20.0),
            Solid::Sensor,
        );
        assert!(world.is_sensor(sensor));
        assert_eq!(world.inside(sensor).len(), 2);
        // A collider made solid again is in the way.
        world.set(sensor, Affine::IDENTITY, area, Solid::Whole);
        assert!(!world.is_sensor(sensor));
        assert_near(
            world.slide(small_box(-10.0, 5.0), [40.0, 0.0], None),
            [8.0 - GAP, 0.0],
        );
    }

    #[test]
    fn tiles_are_solid_where_their_boxes_are() {
        let mut world = World::new();
        // Two boxes, from 10 to 20 and from 40 to 50 across, from an origin of 100, 0.
        let boxes = [
            Rect::new(10.0, 0.0, 10.0, 10.0),
            Rect::new(40.0, 0.0, 10.0, 10.0),
        ];
        let tiles = world.add_tiles([100.0, 0.0], &boxes);
        assert_eq!(world.kind(tiles), Some(BodyKind::Fixed));
        let hit = world
            .cast_ray([0.0, 5.0], [1.0, 0.0], f32::MAX, &[])
            .unwrap();
        assert_eq!((hit.collider, hit.normal), (tiles, [-1.0, 0.0]));
        assert!((hit.distance - 110.0).abs() < 1.0e-3, "{hit:?}");
        // Between them, nothing is solid.
        assert_eq!(world.overlapping_circle([130.0, 5.0], 4.0, &[]), vec![]);
        assert_eq!(
            world.overlapping_circle([145.0, 5.0], 4.0, &[]),
            vec![tiles]
        );
        assert_near(
            world.slide(small_box(0.0, 4.0), [200.0, 0.0], None),
            [108.0 - GAP, 0.0],
        );
        // Moved, and with none, nowhere; boxes with no area don't count.
        world.set_tiles(tiles, [0.0, 0.0], &boxes);
        let hit = world
            .cast_ray([0.0, 5.0], [1.0, 0.0], f32::MAX, &[])
            .unwrap();
        assert!((hit.distance - 10.0).abs() < 1.0e-3, "{hit:?}");
        world.set_tiles(tiles, [0.0, 0.0], &[Rect::new(0.0, 0.0, 0.0, 10.0)]);
        assert_eq!(world.cast_ray([0.0, 5.0], [1.0, 0.0], f32::MAX, &[]), None);
        let none = world.add_tiles([0.0, 0.0], &[]);
        assert_eq!(world.overlapping_circle([0.0, 0.0], 100.0, &[]), vec![]);
        world.set_tiles(none, [0.0, 0.0], &boxes);
        assert_eq!(world.overlapping_circle([15.0, 5.0], 1.0, &[]), vec![none]);
    }

    // Bodies and characters, in pixels, as games have them: a metre is 16, gravity pulls down at
    // 160 a second each second, and a crate 16 across, at a density of a 256th, weighs 1.

    const FRAME: f32 = 1.0 / 60.0;

    /// A world with ground whose top is at `y` 0, 1600 across.
    fn ground() -> World {
        let mut world = World::new();
        world.set_length_unit(16.0);
        world.set_gravity([0.0, 160.0]);
        let ground = Rect::new(-800.0, 0.0, 1600.0, 16.0);
        world.add(Affine::IDENTITY, ground, Solid::Whole);
        world
    }

    /// A crate 16 across, standing on the middle of its bottom.
    fn crate_at(position: [f32; 2]) -> Body {
        Body {
            kind: BodyKind::Dynamic,
            shape: Shape::Box(Rect::new(-8.0, -16.0, 16.0, 16.0)),
            pose: Pose::at(position),
            material: Material {
                density: 1.0 / 256.0,
                ..Material::default()
            },
            sensor: false,
        }
    }

    /// A ball 16 across, its middle at `position`.
    fn ball_at(position: [f32; 2]) -> Body {
        Body {
            shape: Shape::Circle {
                center: [0.0, 0.0],
                radius: 8.0,
            },
            ..crate_at(position)
        }
    }

    fn fixed(shape: Shape, position: [f32; 2]) -> Body {
        Body {
            kind: BodyKind::Fixed,
            shape,
            pose: Pose::at(position),
            material: Material::default(),
            sensor: false,
        }
    }

    const WALKER: Character = Character {
        width: 8.0,
        height: 14.0,
        step: 4.0,
        slope: FRAC_PI_4,
        mass: 1.0,
    };

    /// Walks `WALKER` from `feet` by `motion` each frame for `frames`, stepping the world, and
    /// returns where its feet end up and whether it ended on the ground.
    fn walk_for(
        world: &mut World,
        mut feet: [f32; 2],
        motion: [f32; 2],
        frames: u32,
    ) -> ([f32; 2], bool) {
        let mut grounded = false;
        for _ in 0..frames {
            let walked = world.walk(&WALKER, feet, motion, None);
            feet = add(feet, walked.moved);
            grounded = walked.grounded;
            world.step(FRAME);
        }
        (feet, grounded)
    }

    fn add(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
        [a[0] + b[0], a[1] + b[1]]
    }

    #[test]
    fn poses_turn_as_nodes_do_and_back() {
        for angle in [0.0, 0.5, -2.0, 3.0] {
            let pose = Pose {
                position: [1.0, 2.0],
                angle,
            };
            let rapier = pose.to_rapier();
            // As a node turns: `x` toward `y`, clockwise with `y` down.
            let turned = Affine::from_parts([1.0, 2.0], angle, [1.0, 1.0]);
            let expected = Vector::from(turned.apply([1.0, 0.0]));
            assert!((rapier * Vector::X - expected).length() < 1.0e-5, "{angle}");
            let back = Pose::from_rapier(&rapier);
            assert!((back.angle - angle).abs() < 1.0e-6, "{back:?}");
            assert_eq!(back.position, pose.position);
        }
        // Unturned, exactly.
        assert_eq!(
            Pose::from_rapier(&Pose::at([3.0, 4.0]).to_rapier()).angle,
            0.0
        );
    }

    #[test]
    fn dynamic_bodies_fall_and_rest_on_what_is_below() {
        let mut world = ground();
        let low = world.add_body(&crate_at([0.0, -40.0])).unwrap();
        let high = world.add_body(&crate_at([2.0, -80.0])).unwrap();
        assert_eq!(world.kind(low), Some(BodyKind::Dynamic));
        for _ in 0..360 {
            world.step(FRAME);
        }
        // Stacked, with the ground's top and the lower crate's at 0 and -16.
        let low_pose = world.pose(low).unwrap();
        let high_pose = world.pose(high).unwrap();
        assert!(low_pose.position[1].abs() < 0.2, "{low_pose:?}");
        assert!((high_pose.position[1] + 16.0).abs() < 0.2, "{high_pose:?}");
        assert!(world.is_resting(low) && world.is_resting(high));
        assert!(world.velocity(high)[1].abs() < 0.1);
    }

    #[test]
    fn circles_roll_down_slopes() {
        let mut world = ground();
        // A slope falling to the right by half a radian, from its top at 0, -40.
        let slope = Affine::from_parts([0.0, -40.0], 0.5, [1.0, 1.0]);
        world.add(slope, Rect::new(0.0, 0.0, 200.0, 10.0), Solid::Whole);
        let ball = world.add_body(&ball_at([10.0, -60.0])).unwrap();
        for _ in 0..120 {
            world.step(FRAME);
        }
        let pose = world.pose(ball).unwrap();
        // Down the slope, turning clockwise as it goes.
        assert!(pose.position[0] > 30.0, "{pose:?}");
        assert!(pose.angle != 0.0, "{pose:?}");
        assert!(world.velocity(ball)[0] > 0.0);
    }

    #[test]
    fn steps_are_taken_in_equal_parts() {
        let run = |dt: f32, steps: u32| {
            let mut world = ground();
            let id = world.add_body(&crate_at([0.0, -40.0])).unwrap();
            world.push(id, [3.0, -1.0]);
            for _ in 0..steps {
                world.step(dt);
            }
            world.pose(id).unwrap()
        };
        assert_eq!(run(FRAME * 2.0, 30), run(FRAME, 60));
        // Nothing, or something that isn't a time, doesn't step.
        let mut world = ground();
        let id = world.add_body(&crate_at([0.0, -40.0])).unwrap();
        for dt in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            world.step(dt);
        }
        assert_eq!(world.pose(id).unwrap().position, [0.0, -40.0]);
    }

    #[test]
    fn kinematic_bodies_move_where_they_are_put_and_push() {
        let mut world = ground();
        let pusher = world
            .add_body(&Body {
                kind: BodyKind::Kinematic,
                ..crate_at([-20.0, 0.0])
            })
            .unwrap();
        let pushed = world.add_body(&crate_at([0.0, 0.0])).unwrap();
        for frame in 1..=60 {
            let x = -20.0 + frame as f32 / 2.0;
            world.set_pose(pusher, Pose::at([x, 0.0]));
            world.step(FRAME);
        }
        assert_eq!(world.pose(pusher).unwrap().position, [10.0, 0.0]);
        // Shoved ahead of it.
        assert!(world.pose(pushed).unwrap().position[0] > 17.0);
    }

    #[test]
    fn kinematic_bodies_go_evenly_through_steps_taken_in_parts() {
        let mut world = ground();
        let id = world
            .add_body(&Body {
                kind: BodyKind::Kinematic,
                ..crate_at([0.0; 2])
            })
            .unwrap();
        world.set_pose(id, Pose::at([16.0, 0.0]));
        // Across 16 in two sixtieths, taken in two parts: at 480 a second throughout.
        world.step(FRAME * 2.0);
        assert_eq!(world.pose(id).unwrap().position, [16.0, 0.0]);
        let [vx, vy] = world.velocity(id);
        assert!((vx - 480.0).abs() < 1.0e-2 && vy == 0.0, "{vx}, {vy}");
        assert!(!world.is_resting(id));
        // Turning as it goes, it ends turned as it was put.
        let turned = Pose {
            position: [32.0, 0.0],
            angle: 1.0,
        };
        world.set_pose(id, turned);
        world.step(FRAME * 3.0);
        let pose = world.pose(id).unwrap();
        assert_eq!(pose.position, turned.position);
        assert!((pose.angle - 1.0).abs() < 1.0e-5, "{pose:?}");
        world.step(FRAME);
        assert!(world.is_resting(id), "{:?}", world.velocity(id));
    }

    #[test]
    fn kinematic_bodies_are_put_at_once() {
        let mut world = ground();
        let id = world
            .add_body(&Body {
                kind: BodyKind::Kinematic,
                ..crate_at([-8.0, 0.0])
            })
            .unwrap();
        // A crate a hair from its right, which going there over a step would fling.
        let beside = world.add_body(&crate_at([8.02, 0.0])).unwrap();
        for _ in 0..30 {
            world.step(FRAME);
        }
        world.put(id, Pose::at([48.0, 0.0]));
        assert_eq!(world.pose(id).unwrap().position, [48.0, 0.0]);
        world.step(FRAME);
        assert_eq!(world.pose(id).unwrap().position, [48.0, 0.0]);
        assert_eq!(world.velocity(id), [0.0; 2]);
        let [vx, _] = world.velocity(beside);
        assert!(vx.abs() < 1.0, "flung at {vx}");
    }

    #[test]
    fn bodies_are_reshaped_and_removed() {
        let mut world = ground();
        let id = world.add_body(&crate_at([0.0, 0.0])).unwrap();
        // Twice as tall, so another lands on it higher.
        let tall = Shape::Box(Rect::new(-8.0, -32.0, 16.0, 32.0));
        world.set_shape(id, &tall).unwrap();
        let other = world.add_body(&crate_at([0.0, -64.0])).unwrap();
        for _ in 0..120 {
            world.step(FRAME);
        }
        assert!((world.pose(other).unwrap().position[1] + 32.0).abs() < 0.2);

        world.remove(id);
        assert_eq!(world.len(), 2);
        assert_eq!(world.pose(id), None);
        for _ in 0..120 {
            world.step(FRAME);
        }
        assert!(world.pose(other).unwrap().position[1].abs() < 0.2);
        // Removing it again does nothing, nor does anything else.
        world.remove(id);
        world.set_shape(id, &tall).unwrap();
        world.set_pose(id, Pose::default());
        world.push(id, [1.0, 0.0]);
        assert_eq!(world.velocity(id), [0.0; 2]);
        assert_eq!(world.kind(id), None);
        assert_eq!(world.len(), 2);
        // Fixed colliders aren't bodies, so they have no pose and don't move.
        let wall = world.add(Affine::IDENTITY, small_box(0.0, 0.0), Solid::Whole);
        assert_eq!((world.pose(wall), world.velocity(wall)), (None, [0.0; 2]));
        assert!(world.is_resting(wall));
    }

    #[test]
    fn bodies_that_cant_be_say_why() {
        let mut world = World::new();
        let with = |kind, material| Body {
            kind,
            material,
            ..crate_at([0.0; 2])
        };
        let weightless = Material {
            density: 0.0,
            ..Material::default()
        };
        let err = world
            .add_body(&with(BodyKind::Dynamic, weightless))
            .unwrap_err();
        assert!(err.contains("density must be more than 0"), "{err}");
        // What doesn't fall needn't weigh anything.
        assert!(world.add_body(&with(BodyKind::Fixed, weightless)).is_ok());
        assert!(
            world
                .add_body(&with(BodyKind::Kinematic, weightless))
                .is_ok()
        );
        let rough = Material {
            friction: -0.5,
            ..Material::default()
        };
        let err = world.add_body(&with(BodyKind::Fixed, rough)).unwrap_err();
        assert!(err.contains("friction must be 0 or more"), "{err}");
        // What's flat can be fixed, but not fall.
        let flat = Shape::Box(Rect::new(0.0, 0.0, 16.0, 0.0));
        assert!(world.add_body(&fixed(flat, [0.0; 2])).is_ok());
        let falling = Body {
            shape: flat,
            ..crate_at([0.0; 2])
        };
        assert!(world.add_body(&falling).unwrap_err().contains("flat"));
        let inside_out = Shape::Box(Rect::new(0.0, 0.0, -1.0, 1.0));
        assert!(world.add_body(&fixed(inside_out, [0.0; 2])).is_err());
        let pointless = Shape::Circle {
            center: [0.0; 2],
            radius: 0.0,
        };
        assert!(world.add_body(&fixed(pointless, [0.0; 2])).is_err());
        // A sensor doesn't fall.
        let falling_sensor = Body {
            sensor: true,
            ..crate_at([0.0; 2])
        };
        let err = world.add_body(&falling_sensor).unwrap_err();
        assert!(err.contains("doesn't fall"), "{err}");
        assert_eq!(world.len(), 3);
    }

    #[test]
    fn body_sensors_tell_what_is_in_them_and_stop_nothing() {
        let mut world = ground();
        let area = Shape::Box(Rect::new(-16.0, -16.0, 32.0, 16.0));
        let sensor = world
            .add_body(&Body {
                kind: BodyKind::Kinematic,
                sensor: true,
                ..fixed(area, [0.0, -40.0])
            })
            .unwrap();
        assert!(world.is_sensor(sensor));
        let falling = world.add_body(&crate_at([0.0, -80.0])).unwrap();
        // The crate falls through it, in it on the way, and comes to rest on the ground.
        let mut was_in = false;
        for _ in 0..180 {
            world.step(FRAME);
            was_in |= world.inside(sensor) == vec![falling];
        }
        assert!(was_in);
        assert!(world.pose(falling).unwrap().position[1].abs() < 0.2);
        // Put over it, at once, it holds it.
        world.set_pose(sensor, Pose::at([0.0, 8.0]));
        assert_eq!(world.inside(sensor).len(), 2);
    }

    #[test]
    fn characters_walk_across_the_ground() {
        let mut world = ground();
        let (feet, grounded) = walk_for(&mut world, [0.0, -GAP], [1.0, 0.2], 60);
        assert!(grounded);
        assert_within(feet, [60.0, -GAP], 1.0e-3);
        // Standing still, nothing moves it.
        let walked = world.walk(&WALKER, feet, [0.0, 0.2], None);
        assert_eq!(walked.moved, [0.0; 2]);
        assert!(walked.grounded);
    }

    #[test]
    fn walls_stop_characters() {
        let mut world = ground();
        world.add(
            Affine::IDENTITY,
            Rect::new(40.0, -32.0, 8.0, 32.0),
            Solid::Whole,
        );
        let (feet, grounded) = walk_for(&mut world, [0.0, -GAP], [1.0, 0.2], 600);
        // Its side, half its width from its middle, a gap from the wall.
        assert!(grounded);
        assert_within(feet, [40.0 - WALKER.width / 2.0 - GAP, -GAP], 1.0e-3);
    }

    #[test]
    fn characters_step_up_onto_what_is_low_enough() {
        let mut world = ground();
        // A stair 3 high, then a wall 8 higher.
        let stair = Rect::new(40.0, -3.0, 40.0, 3.0);
        world.add(Affine::IDENTITY, stair, Solid::Whole);
        let wall = Rect::new(60.0, -11.0, 20.0, 8.0);
        world.add(Affine::IDENTITY, wall, Solid::Whole);
        let (feet, grounded) = walk_for(&mut world, [0.0, -GAP], [1.0, 0.2], 90);
        assert!(grounded);
        assert_within(feet, [60.0 - WALKER.width / 2.0 - GAP, -3.0 - GAP], 1.0e-2);
        // And down again.
        let (feet, grounded) = walk_for(&mut world, feet, [-1.0, 0.2], 60);
        assert!(grounded);
        assert_within(feet, [feet[0], -GAP], 1.0e-2);
    }

    #[test]
    fn characters_walk_up_slopes_gentle_enough() {
        // Rising to the right from 0, 0, about 17 degrees.
        let mut world = ground();
        let gentle = Affine::from_parts([0.0, 0.0], -0.3, [1.0, 1.0]);
        world.add(gentle, Rect::new(0.0, 0.0, 200.0, 40.0), Solid::Whole);
        let (feet, grounded) = walk_for(&mut world, [-20.0, -GAP], [1.0, 0.2], 60);
        assert!(grounded);
        assert!(feet[0] > 30.0 && feet[1] < -8.0, "{feet:?}");
        // And down again, on the ground all the way.
        let mut feet = feet;
        for _ in 0..60 {
            let walked = world.walk(&WALKER, feet, [-1.0, 0.2], None);
            assert!(walked.grounded, "left the ground at {feet:?}");
            feet = add(feet, walked.moved);
            world.step(FRAME);
        }

        // Rising at about 69 degrees: as a wall.
        let mut world = ground();
        let steep = Affine::from_parts([0.0, 0.0], -1.2, [1.0, 1.0]);
        world.add(steep, Rect::new(0.0, 0.0, 200.0, 40.0), Solid::Whole);
        let (feet, _) = walk_for(&mut world, [-20.0, -GAP], [1.0, 0.2], 120);
        assert!(feet[1] > -WALKER.step - 0.1, "{feet:?}");
        assert!(feet[0] < 5.0, "{feet:?}");
    }

    #[test]
    fn characters_fall_off_ledges_and_land() {
        let mut world = ground();
        let ledge = Rect::new(-100.0, -30.0, 100.0, 30.0);
        world.add(Affine::IDENTITY, ledge, Solid::Whole);
        // Off the ledge's edge: it leaves the ground, falling as it's told to.
        let walked = world.walk(&WALKER, [-1.0, -30.0 - GAP], [8.0, 2.0], None);
        assert!(!walked.grounded);
        assert_within(walked.moved, [8.0, 2.0], 1.0e-4);
        // And lands on the ground below.
        let (feet, grounded) = walk_for(&mut world, [7.0, -28.0], [0.0, 3.0], 20);
        assert!(grounded);
        assert_within(feet, [7.0, -GAP], 1.0e-3);
    }

    #[test]
    fn rising_characters_stop_at_ceilings() {
        let mut world = ground();
        let ceiling = Rect::new(-50.0, -40.0, 100.0, 10.0);
        world.add(Affine::IDENTITY, ceiling, Solid::Whole);
        let walked = world.walk(&WALKER, [0.0, -GAP], [0.0, -30.0], None);
        assert!(!walked.grounded);
        assert_within(
            walked.moved,
            [0.0, -(30.0 - WALKER.height - 2.0 * GAP)],
            1.0e-3,
        );
    }

    #[test]
    fn characters_push_what_they_walk_into() {
        let mut world = ground();
        let pushed = world.add_body(&crate_at([30.0, 0.0])).unwrap();
        for _ in 0..60 {
            world.step(FRAME);
        }
        let (feet, _) = walk_for(&mut world, [0.0, -GAP], [1.0, 0.2], 60);
        let pose = world.pose(pushed).unwrap();
        // Pushed along ahead of the character, which kept walking.
        assert!(pose.position[0] > 45.0, "{pose:?}");
        assert!(feet[0] > 25.0, "{feet:?}");
        assert!(
            pose.angle.abs() < 0.1 && pose.position[1].abs() < 0.5,
            "{pose:?}"
        );
    }

    #[test]
    fn characters_ride_what_they_stand_on() {
        let mut world = ground();
        // A platform 40 across, its top at its position.
        let platform = world
            .add_body(&Body {
                kind: BodyKind::Kinematic,
                ..fixed(Shape::Box(Rect::new(-20.0, 0.0, 40.0, 4.0)), [0.0, -20.0])
            })
            .unwrap();
        let mut feet = [0.0, -20.0 - GAP];
        let mut at = [0.0, -20.0];
        // Across, then up, then down, a pixel every other frame; the character goes as the
        // platform went in the last step, so a frame behind it.
        let moves = [[0.5, 0.0]; 60]
            .into_iter()
            .chain([[0.0, -0.5]; 30])
            .chain([[0.0, 0.5]; 30]);
        let mut last = [0.0, 0.0];
        for (frame, by) in moves.enumerate() {
            at = add(at, by);
            world.set_pose(platform, Pose::at(at));
            let walked = world.walk(&WALKER, feet, [0.0, 0.2], None);
            assert!(
                walked.grounded,
                "left the platform at frame {frame}: {feet:?}"
            );
            feet = add(feet, walked.moved);
            world.step(FRAME);
            let behind = [at[0] - by[0], at[1] - by[1] - GAP];
            if frame > 0 && by == last {
                assert_within(feet, behind, 1.0e-2);
            }
            last = by;
        }
    }

    #[test]
    fn a_character_is_not_in_its_own_way() {
        let mut world = ground();
        let own = world
            .add_body(&Body {
                kind: BodyKind::Kinematic,
                ..fixed(Shape::Box(Rect::new(-4.0, -14.0, 8.0, 14.0)), [0.0, -GAP])
            })
            .unwrap();
        let walked = world.walk(&WALKER, [0.0, -GAP], [8.0, 0.0], Some(own));
        assert_within(walked.moved, [8.0, 0.0], 1.0e-3);
        // Others are, as it is to them.
        let walked = world.walk(&WALKER, [-16.0, -GAP], [16.0, 0.0], None);
        assert_within(walked.moved, [16.0 - WALKER.width - GAP, 0.0], 1.0e-3);
    }

    /// A hash of the bits of every body's pose.
    fn hash_of(world: &World, ids: &[ColliderId]) -> u64 {
        let numbers = ids.iter().flat_map(|&id| {
            let pose = world.pose(id).unwrap();
            let [x, y] = pose.position;
            [x, y, pose.angle]
        });
        // FNV-1a.
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for byte in numbers.flat_map(|n| n.to_bits().to_le_bytes()) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
        hash
    }

    #[test]
    fn the_same_bodies_and_walks_end_the_same_to_the_bit_on_every_platform() {
        let mut world = ground();
        // A slope to tumble down.
        let slope = Affine::from_parts([40.0, -40.0], 0.6, [1.0, 1.0]);
        world.add(slope, Rect::new(0.0, 0.0, 120.0, 10.0), Solid::Whole);
        // A pile of crates, turned, and balls, dropped on it and around it.
        let mut ids = Vec::new();
        for i in 0..12 {
            let f = i as f32;
            let mut body = crate_at([20.0 + (f * 37.0) % 80.0, -60.0 - f * 20.0]);
            body.pose.angle = f * 0.4;
            if i % 3 == 0 {
                body.shape = Shape::Circle {
                    center: [0.0, -8.0],
                    radius: 8.0,
                };
            }
            ids.push(world.add_body(&body).unwrap());
        }
        // And a character walking into them, there and back.
        let mut feet = [-60.0, -GAP];
        for frame in 0..360 {
            let x = if frame < 200 { 1.0 } else { -0.8 };
            let walked = world.walk(&WALKER, feet, [x, 0.2], None);
            feet = add(feet, walked.moved);
            world.step(FRAME);
        }
        let hash = hash_of(&world, &ids);
        // Rapier's enhanced determinism, and `libm` for the rest, make this the same on Windows and
        // Linux, debug and release. A change to how bodies or characters move changes it.
        assert_eq!(hash, 0xf990_1651_a7ae_86e2, "{hash:#x}");
    }
}
