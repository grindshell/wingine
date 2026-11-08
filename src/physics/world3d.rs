//! Physics in 3D, by Rapier: bodies that fall, tumble and rest on each other, stepped through
//! time, characters walking among them, and what's where: rays, overlaps and sensors. See
//! [`World3d`].

use rapier3d::{
    parry::{
        bounding_volume::Aabb,
        query::{Ray, ShapeCastHit, ShapeCastOptions},
        shape::{Cylinder, FeatureId, Shape, TriMeshFlags},
    },
    prelude::{
        ActiveCollisionTypes, Collider, ColliderBuilder, ColliderHandle, PhysicsWorld, Pose,
        QueryFilter, RigidBodyBuilder, RigidBodyHandle, SharedShape,
    },
};

use crate::math::Bounds3;

/// Rapier's vectors and rotations, which are glam's.
type Vector = rapier3d::prelude::Vector;
type Rotation = rapier3d::math::Rotation;

/// The gap a walking character keeps from what it stops at, in the world's units, so that one
/// standing on the ground isn't touching it, and walks along it without hitting it.
const GAP: f32 = 0.002;

/// How far below a character the ground may be for it to stand on it, unless the ground sank.
const REACH: f32 = GAP * 4.0;

/// The longest step the world takes at once, in seconds: longer ones are taken in parts.
const STEP: f32 = 1.0 / 60.0;

/// The most parts a step is taken in, so a long hitch doesn't take longer to catch up with.
const MAX_STEPS: u32 = 16;

/// The most things a character slides off in one part of its walk.
const MAX_HITS: usize = 4;

/// How a body moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BodyKind {
    /// It doesn't: the ground, walls, and what stands still. Only fixed bodies can be shaped by
    /// their triangles.
    #[default]
    Fixed,
    /// As it's pushed: it falls, tumbles, bounces and rests on what's below it.
    Dynamic,
    /// Where it's put, pushing dynamic bodies out of its way as it goes, as a lift or a door
    /// does, or a character.
    Kinematic,
}

/// A body's shape, in its own coordinates.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape3d {
    /// Triangles, solid along their surfaces, which only fixed bodies are shaped by: what moves
    /// is convex.
    Triangles {
        points: Vec<[f32; 3]>,
        /// Three indices into `points` for each triangle.
        triangles: Vec<[u32; 3]>,
    },
    /// The smallest convex shape holding every point, as cling film would wrap them.
    Hull(Vec<[f32; 3]>),
    /// A box along the body's axes.
    Box(Bounds3),
    Ball {
        center: [f32; 3],
        radius: f32,
    },
    /// A cylinder standing upright on the origin, as a character does: `radius` round and
    /// `height` tall.
    Cylinder {
        radius: f32,
        height: f32,
    },
}

/// What a body's surface is like.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// How heavy each unit of its volume is, which with its shape makes its mass.
    pub density: f32,
    /// How much it grips what slides along it: 0 for ice, 1 for rubber.
    pub friction: f32,
    /// How much it bounces: 0 for not at all, 1 for back as fast as it hit.
    pub bounce: f32,
}

impl Default for Material {
    fn default() -> Self {
        Self {
            density: 1.0,
            friction: 0.5,
            bounce: 0.0,
        }
    }
}

/// Where a body is and how it's turned, as a node placed in 3D is: turned by `yaw`, `pitch` and
/// `roll`, radians about `y`, `x` and `z`, composed as rolled, pitched, then turned, then moved
/// to `position`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pose3d {
    pub position: [f32; 3],
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
}

impl Pose3d {
    /// At `position`, unturned.
    pub const fn at(position: [f32; 3]) -> Self {
        Self {
            position,
            yaw: 0.0,
            pitch: 0.0,
            roll: 0.0,
        }
    }

    /// As Rapier has it. Its rotation is worked out with `libm`, which gives the same bits on
    /// every platform, as the rest of the world's arithmetic does.
    fn to_rapier(self) -> Pose {
        let half = |angle: f32| (libm::sinf(angle / 2.0), libm::cosf(angle / 2.0));
        let (sy, cy) = half(self.yaw);
        let (sx, cx) = half(self.pitch);
        let (sz, cz) = half(self.roll);
        let rotation = Rotation::from_xyzw(0.0, sy, 0.0, cy)
            * Rotation::from_xyzw(sx, 0.0, 0.0, cx)
            * Rotation::from_xyzw(0.0, 0.0, sz, cz);
        Pose::from_parts(Vector::from(self.position), rotation)
    }

    /// From how Rapier has it.
    fn from_rapier(pose: &Pose) -> Self {
        // The rotation's columns, x, y and z: yaw * pitch * roll is
        //   [ cy cz + sy sx sz,  sy sx cz - cy sz,  sy cx ]
        //   [ cx sz,             cx cz,             -sx   ]
        //   [ cy sx sz - sy cz,  sy sz + cy sx cz,  cy cx ]
        let matrix = rapier3d::math::Matrix::from_quat(pose.rotation);
        let [x, y, z] = [matrix.x_axis, matrix.y_axis, matrix.z_axis];
        let pitch = libm::asinf((-z.y).clamp(-1.0, 1.0));
        let (yaw, roll) = if z.x * z.x + z.z * z.z > 1.0e-12 {
            (libm::atan2f(z.x, z.z), libm::atan2f(x.y, y.y))
        } else {
            // Pitched straight up or down, where turning and rolling are the same: all turning.
            (libm::atan2f(-x.z, x.x), 0.0)
        };
        Self {
            position: pose.translation.into(),
            yaw,
            pitch,
            roll,
        }
    }
}

/// A body to add to a [`World3d`].
#[derive(Clone, Debug, PartialEq)]
pub struct Body3d {
    pub kind: BodyKind,
    pub shape: Shape3d,
    pub pose: Pose3d,
    pub material: Material,
    /// Whether it's a sensor: an area that nothing bumps into, which [`World3d::inside`] tells
    /// what's in. A sensor is fixed or kinematic, and solid all through, so not triangles.
    pub sensor: bool,
}

/// A body in a [`World3d`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BodyId(RigidBodyHandle);

/// Where a ray hit a body: see [`World3d::cast_ray`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit3d {
    pub body: BodyId,
    /// How far along the ray it hit, in lengths of the ray's direction.
    pub distance: f32,
    /// Where it hit, in the world.
    pub point: [f32; 3],
    /// The way out of the body where it hit, a vector of length 1 facing back along the ray, or
    /// zero if the ray started inside the body.
    pub normal: [f32; 3],
}

/// A character walking with [`World3d::walk`]: an upright cylinder, standing on its flat feet, so
/// it stands on the edge of a step as a box would.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Character {
    /// How far it reaches out from its middle.
    pub radius: f32,
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
    /// Its cylinder, with its middle at the origin.
    fn shape(&self) -> Cylinder {
        Cylinder::new(self.height / 2.0, self.radius)
    }
}

/// How a character's walk went: see [`World3d::walk`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Walked {
    /// How far it moved.
    pub moved: [f32; 3],
    /// Whether it ended standing on something it can stand on.
    pub grounded: bool,
}

/// Bodies in 3D, which [`World3d::step`] moves through time, and characters walking among them.
///
/// Its coordinates are a 3D view's world's: right-handed, with `y` up, as glTF's are, and its
/// units the view's, which Rapier tunes itself for as metres. Bodies are fixed, dynamic or
/// kinematic ([`BodyKind`]), each a [`Shape3d`] placed by a [`Pose3d`]. Characters walk with
/// [`World3d::walk`], which moves a cylinder as a kinematic body is moved, stopping at what's solid
/// and sliding along it, stepping up onto stairs, walking up and down slopes, and pushing dynamic
/// bodies out of the way.
///
/// Scripts ask where things are by rays ([`World3d::cast_ray`]), by shapes overlapping bodies
/// ([`World3d::overlapping`]), and by sensors ([`Body3d::sensor`]), areas nothing bumps into that
/// [`World3d::inside`] tells what's in. They find bodies where they are now, added or moved since
/// the last step too, but kinematic bodies that are going somewhere over the next step where they
/// are until then. Sensors are never hit or overlapped.
///
/// Rapier is built with its `enhanced-determinism`, and poses are turned to rotations with
/// `libm`, so the same bodies, steps and walks give the same results to the bit on every
/// platform.
pub struct World3d {
    rapier: PhysicsWorld,
    /// The colliders added or moved since the last step, which Rapier's queries don't find
    /// where they are until they're refreshed.
    fresh: Vec<ColliderHandle>,
    /// How long the last step was, which characters take to have been how long their walks took,
    /// for how fast they push what they walk into.
    last_step: f32,
}

impl Default for World3d {
    fn default() -> Self {
        Self::new()
    }
}

impl World3d {
    /// A world with no bodies, and gravity pulling down at 9.81 units a second each second, as
    /// Earth's does in metres.
    pub fn new() -> Self {
        Self {
            rapier: PhysicsWorld::new(),
            fresh: Vec::new(),
            last_step: STEP,
        }
    }

    /// How fast dynamic bodies falling speed up each second, as a vector.
    pub fn gravity(&self) -> [f32; 3] {
        self.rapier.gravity.into()
    }

    pub fn set_gravity(&mut self, gravity: [f32; 3]) {
        self.rapier.gravity = Vector::from(gravity);
    }

    /// Adds `body`, or fails if its shape can't be made, or its material isn't one: triangles on
    /// a body that moves or a sensor, a hull or triangles with nothing inside, sizes that aren't
    /// finite, a density, friction or bounce less than 0, a dynamic body with a density of 0, or a
    /// dynamic sensor.
    pub fn add(&mut self, body: &Body3d) -> Result<BodyId, String> {
        check_material(&body.material, body.kind)?;
        if body.sensor {
            check_sensor(body.kind, &body.shape)?;
        }
        let collider = collider(&body.shape, body.kind)?
            .density(body.material.density)
            .friction(body.material.friction)
            .restitution(body.material.bounce);
        let collider = sensed(collider, body.sensor);
        let builder = match body.kind {
            BodyKind::Fixed => RigidBodyBuilder::fixed(),
            BodyKind::Dynamic => RigidBodyBuilder::dynamic(),
            // Never asleep: Rapier expects what touches a body that's awake to be awake too,
            // and doesn't wake a kinematic body that dynamic ones bump into and leave.
            BodyKind::Kinematic => RigidBodyBuilder::kinematic_position_based().can_sleep(false),
        };
        let (handle, collider) = self
            .rapier
            .insert(builder.pose(body.pose.to_rapier()), collider);
        self.fresh.push(collider);
        Ok(BodyId(handle))
    }

    /// Reshapes the body `id`, keeping its material, and if it's a sensor, keeping it one, as
    /// [`World3d::add`] shapes one. Does nothing to one that's been removed.
    pub fn set_shape(&mut self, id: BodyId, shape: &Shape3d) -> Result<(), String> {
        let Some(kind) = self.kind(id) else {
            return Ok(());
        };
        let sensor = self.is_sensor(id);
        if sensor {
            check_sensor(kind, shape)?;
        }
        let made = collider(shape, kind)?;
        let old = self.rapier.bodies[id.0].colliders().to_vec();
        let material = old.first().map_or_else(Material::default, |&handle| {
            let old = &self.rapier.colliders[handle];
            Material {
                density: old.density(),
                friction: old.friction(),
                bounce: old.restitution(),
            }
        });
        for handle in old {
            self.rapier.remove_collider(handle);
        }
        let made = made
            .density(material.density)
            .friction(material.friction)
            .restitution(material.bounce);
        let collider = self
            .rapier
            .insert_collider(sensed(made, sensor), Some(id.0));
        self.fresh.push(collider);
        Ok(())
    }

    /// Removes the body `id`, if it hasn't been already.
    pub fn remove(&mut self, id: BodyId) {
        self.rapier.remove_body(id.0);
    }

    /// How many bodies there are.
    pub fn len(&self) -> usize {
        self.rapier.bodies.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Whether the body `id` is a sensor: see [`Body3d::sensor`].
    pub fn is_sensor(&self, id: BodyId) -> bool {
        self.rapier
            .bodies
            .get(id.0)
            .and_then(|body| body.colliders().first())
            .is_some_and(|&collider| self.rapier.colliders[collider].is_sensor())
    }

    pub fn kind(&self, id: BodyId) -> Option<BodyKind> {
        let body = self.rapier.bodies.get(id.0)?;
        Some(if body.is_dynamic() {
            BodyKind::Dynamic
        } else if body.is_kinematic() {
            BodyKind::Kinematic
        } else {
            BodyKind::Fixed
        })
    }

    /// Where the body `id` is, or `None` if it's been removed.
    pub fn pose(&self, id: BodyId) -> Option<Pose3d> {
        Some(Pose3d::from_rapier(
            self.rapier.bodies.get(id.0)?.position(),
        ))
    }

    /// Puts the body `id` at `pose`: a fixed or dynamic body, or a sensor, at once, keeping a
    /// dynamic one's velocity, and a kinematic one over the next step, pushing what's in its way.
    /// See [`World3d::put`] for putting a kinematic one there at once.
    pub fn set_pose(&mut self, id: BodyId, pose: Pose3d) {
        // A sensor pushes nothing, so it needn't sweep there.
        let sensor = self.is_sensor(id);
        match self.rapier.bodies.get_mut(id.0) {
            Some(body) if body.is_kinematic() && !sensor => {
                body.set_next_kinematic_position(pose.to_rapier());
                self.wake(id.0);
            }
            Some(_) => self.put(id, pose),
            None => {}
        }
    }

    /// Puts the body `id` at `pose` at once, whatever its kind, keeping a dynamic one's
    /// velocity: a kinematic one doesn't go there over the next step, sweeping through what's
    /// between, as a character starting over shouldn't.
    pub fn put(&mut self, id: BodyId, pose: Pose3d) {
        let Some(body) = self.rapier.bodies.get_mut(id.0) else {
            return;
        };
        // Where it is and where it's going next, so a kinematic one stays put.
        body.set_position(pose.to_rapier(), false);
        self.fresh.extend_from_slice(body.colliders());
        self.wake(id.0);
    }

    /// How fast the body `id` is moving, in units a second: a kinematic one as fast as it moved
    /// in the last step.
    pub fn velocity(&self, id: BodyId) -> [f32; 3] {
        self.rapier
            .bodies
            .get(id.0)
            .map_or([0.0; 3], |body| body.linvel().into())
    }

    /// Sets how fast the dynamic body `id` is moving. Does nothing to bodies of other kinds.
    pub fn set_velocity(&mut self, id: BodyId, velocity: [f32; 3]) {
        if let Some(body) = self.rapier.bodies.get_mut(id.0)
            && body.is_dynamic()
        {
            body.set_linvel(Vector::from(velocity), false);
            self.wake(id.0);
        }
    }

    /// Pushes the dynamic body `id` through its center of mass by `impulse`, which changes its
    /// velocity by the impulse over its mass. Does nothing to bodies of other kinds.
    pub fn push(&mut self, id: BodyId, impulse: [f32; 3]) {
        if let Some(body) = self.rapier.bodies.get_mut(id.0)
            && body.is_dynamic()
        {
            body.apply_impulse(Vector::from(impulse), false);
            self.wake(id.0);
        }
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
            let aabb = collider.compute_aabb();
            self.rapier
                .broad_phase
                .set_aabb(&self.rapier.integration_parameters, handle, aabb);
        }
    }

    /// Wakes the body `handle`, which Rapier must be told of, not the body, to simulate it again.
    fn wake(&mut self, handle: RigidBodyHandle) {
        if self.rapier.bodies.contains(handle) {
            self.rapier.wake_up(handle, true);
        }
    }

    /// Whether the body `id` is at rest: a dynamic body that has been still for a couple of
    /// seconds, which the world then leaves alone until something touches it. Fixed bodies
    /// always are, and kinematic ones while they aren't being moved or turned.
    pub fn is_resting(&self, id: BodyId) -> bool {
        // Rounding leaves a turned one that has stopped still moving by a hair.
        const STILL: f32 = 1.0e-4;
        self.rapier.bodies.get(id.0).is_none_or(|body| {
            let still = body.linvel().length() < STILL && body.angvel().length() < STILL;
            body.is_fixed() || body.is_sleeping() || (body.is_kinematic() && still)
        })
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
        let moving: Vec<(RigidBodyHandle, Pose, Pose)> = if parts > 1 {
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
    /// whether it's on the ground. The body `own`, if given, is the character's own, which isn't
    /// in its way.
    ///
    /// On the ground, and not rising, it walks across by the motion's `x` and `z`: up stairs no
    /// higher than its [`Character::step`], up and down slopes no steeper than its
    /// [`Character::slope`], sliding along walls and steeper slopes, and staying on the ground as
    /// it goes down steps, and with what it stands on as that goes down, as a lift does, unless it
    /// walks off a ledge, where it falls by the motion's `y`. Off
    /// the ground, or rising, it moves as `motion` says, landing on what it can stand on, sliding
    /// down what's too steep, and stopping at ceilings. Either way, it stops [`GAP`] short of
    /// what it hits.
    ///
    /// Dynamic bodies it walks into are pushed along at the speed it walks, taken over the last
    /// step's time, as hard as its mass and theirs make a collision: a heavy character pushes a
    /// light crate at its own speed, and a light one a heavy crate slowly.
    pub fn walk(
        &mut self,
        character: &Character,
        feet: [f32; 3],
        motion: [f32; 3],
        own: Option<BodyId>,
    ) -> Walked {
        self.refresh();
        let mut walk = Walk {
            world: &self.rapier,
            character,
            shape: character.shape(),
            middle: Vector::from(feet) + Vector::Y * (character.height / 2.0),
            walkable: libm::cosf(character.slope),
            own: own.map(|own| own.0),
            last_step: self.last_step,
            pushes: Vec::new(),
        };
        let [x, y, z] = motion;
        let across = Vector::new(x, 0.0, z);
        let start = walk.middle;
        let ground = if y <= 0.0 { walk.ground() } else { None };
        let grounded = match ground {
            Some(below) => {
                // Down with what it stands on, if that sank away from under it.
                if below > REACH {
                    walk.middle.y -= below - GAP;
                }
                walk.walk_across(across, -y)
            }
            None => walk.slide(Vector::new(x, y, z), Slide::Free).landed,
        };
        let moved = walk.middle - start;
        let pushes = std::mem::take(&mut walk.pushes);
        // Pushed at the speed it walked into them.
        let dt = self.last_step;
        for push in pushes {
            self.push_away(character, push, across / dt);
        }
        Walked {
            moved: moved.into(),
            grounded,
        }
    }

    /// Casts a ray from `origin` along `direction`, as far as `max_distance` lengths of it, and
    /// returns where it first hits a body, other than those in `except` and sensors, or `None`
    /// if it hits nothing. It hits bodies by their shapes: triangles where they are, and a box,
    /// a hull or a ball anywhere on or inside it, so one starting inside a body hits it at 0.
    pub fn cast_ray(
        &mut self,
        origin: [f32; 3],
        direction: [f32; 3],
        max_distance: f32,
        except: &[BodyId],
    ) -> Option<RayHit3d> {
        self.refresh();
        let direction = Vector::from(direction);
        let ray = Ray::new(Vector::from(origin), direction);
        let excepted = |_: ColliderHandle, collider: &Collider| !excepts(except, collider);
        let filter = QueryFilter::default()
            .exclude_sensors()
            .predicate(&excepted);
        let queries = self.rapier.query_pipeline_with_filter(filter);
        let (collider, hit) = queries.cast_ray_and_get_normal(&ray, max_distance, true)?;
        let body = self.rapier.colliders[collider].parent()?;
        // Back toward where the ray came from, whichever side of a triangle it hit; and none from
        // inside, where Rapier gives boxes none but some shapes one.
        let normal = if hit.time_of_impact <= 0.0 {
            Vector::ZERO
        } else if hit.normal.dot(direction) > 0.0 {
            -hit.normal
        } else {
            hit.normal
        };
        Some(RayHit3d {
            body: BodyId(body),
            distance: hit.time_of_impact,
            point: ray.point_at(hit.time_of_impact).into(),
            normal: square(normal).into(),
        })
    }

    /// Returns the bodies that `shape`, in the world's coordinates, touches or overlaps, other
    /// than those in `except` and sensors, in no particular order, or fails if the shape can't be
    /// made, as [`World3d::add`] fails. Triangles touch only what touches them, not what they're
    /// around.
    pub fn overlapping(
        &mut self,
        shape: &Shape3d,
        except: &[BodyId],
    ) -> Result<Vec<BodyId>, String> {
        let made = collider(shape, BodyKind::Fixed)?;
        self.refresh();
        Ok(self.touching(made.position, &*made.shape, except))
    }

    /// Returns the bodies that overlap or touch the body `id`, a sensor's or any other's, other
    /// than sensors, in no particular order: for a sensor, what's in it.
    pub fn inside(&mut self, id: BodyId) -> Vec<BodyId> {
        self.refresh();
        let Some(body) = self.rapier.bodies.get(id.0) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for &handle in body.colliders() {
            let collider = &self.rapier.colliders[handle];
            for touched in self.touching(*collider.position(), collider.shape(), &[id]) {
                if !found.contains(&touched) {
                    found.push(touched);
                }
            }
        }
        found
    }

    /// The bodies `shape` at `pose` touches, other than those in `except` and sensors.
    fn touching(&self, pose: Pose, shape: &dyn Shape, except: &[BodyId]) -> Vec<BodyId> {
        let excepted = |_: ColliderHandle, collider: &Collider| !excepts(except, collider);
        let filter = QueryFilter::default()
            .exclude_sensors()
            .predicate(&excepted);
        let queries = self.rapier.query_pipeline_with_filter(filter);
        let mut found = Vec::new();
        for (_, collider) in queries.intersect_shape(pose, shape) {
            if let Some(body) = collider.parent().map(BodyId)
                && !found.contains(&body)
            {
                found.push(body);
            }
        }
        found
    }

    /// Pushes what a character walking at `velocity` walked into, as a collision of the two
    /// would, along the ground.
    fn push_away(&mut self, character: &Character, push: Push, velocity: Vector) {
        let Some(body) = self.rapier.bodies.get_mut(push.body) else {
            return;
        };
        // Into it, along the ground.
        let into = Vector::new(-push.normal.x, 0.0, -push.normal.z);
        let length = into.length();
        if length < 1.0e-6 {
            return;
        }
        let into = into / length;
        // At the height of its center of mass, so it slides rather than tipping over, though it
        // turns when pushed off center.
        let point = push.point.with_y(body.center_of_mass().y);
        let wanted = velocity.dot(into) - body.velocity_at_point(point).dot(into);
        if wanted <= 0.0 {
            return;
        }
        let mass = body.mass();
        let reduced = mass * character.mass / (mass + character.mass);
        body.apply_impulse_at_point(into * (wanted * reduced), point, false);
        self.wake(push.body);
    }
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
    shape: Cylinder,
    /// Where the cylinder's middle is.
    middle: Vector,
    /// The least a ground's normal points up, as the cosine of the steepest slope.
    walkable: f32,
    own: Option<RigidBodyHandle>,
    /// How long the world's last step was, over which what it stands on may have sunk.
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
    /// Casts the cylinder from where it is along `direction`, a unit vector, up to `distance`.
    fn cast(&self, direction: Vector, distance: f32) -> Option<Hit> {
        let filter = match self.own {
            Some(own) => QueryFilter::default().exclude_rigid_body(own),
            None => QueryFilter::default(),
        }
        .exclude_sensors();
        let queries = self.world.query_pipeline_with_filter(filter);
        let options = ShapeCastOptions {
            max_time_of_impact: distance,
            target_distance: 0.0,
            stop_at_penetration: false,
            compute_impact_geometry_on_penetration: true,
        };
        let at = Pose::from_translation(self.middle);
        let (collider, hit) = queries.cast_shape(&at, direction, &self.shape, options)?;
        let normal = true_normal(&hit, &self.world.colliders[collider], &self.shape);
        Some(Hit {
            distance: hit.time_of_impact,
            collider,
            normal,
            point: hit.witness1,
        })
    }

    /// How far below the character the ground it stands on is, if it stands on something it can
    /// stand on: just below it, a gap away, or as much farther as that sank in the last step,
    /// as a lift going down does, up to a step.
    fn ground(&self) -> Option<f32> {
        if let Some(hit) = self.cast(Vector::NEG_Y, REACH) {
            return (hit.normal.y >= self.walkable).then_some(hit.distance);
        }
        let hit = self.cast(Vector::NEG_Y, REACH + self.character.step)?;
        let standing = hit.normal.y >= self.walkable;
        (standing && hit.distance <= REACH + self.sank(&hit)).then_some(hit.distance)
    }

    /// How far down what `hit` hit moved where it was hit, in the last step.
    fn sank(&self, hit: &Hit) -> f32 {
        let Some(parent) = self.world.colliders[hit.collider].parent() else {
            return 0.0;
        };
        let falling = -self.world.bodies[parent].velocity_at_point(hit.point).y;
        falling.max(0.0) * self.last_step
    }

    /// Whether `hit` is a dynamic body's, and if so, notes that it was pushed.
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
    fn walk_across(&mut self, across: Vector, fall: f32) -> bool {
        // Standing still, it stays where it stands.
        if across == Vector::ZERO {
            return true;
        }
        let start = self.middle;
        let gone = |middle: Vector| (middle - start).with_y(0.0).length();
        // Along the ground, up slopes it can walk up.
        let slid = self.slide(across, Slide::Along);
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
        match self.cast(Vector::NEG_Y, self.character.step + fall) {
            Some(hit) if hit.normal.y >= self.walkable => {
                self.middle.y -= hit.distance - GAP;
                true
            }
            // Too steep to stand on, but there without stepping down: it falls onto it.
            Some(hit) if hit.distance <= fall => {
                self.middle.y -= hit.distance - GAP;
                false
            }
            // Off a ledge.
            _ => {
                self.middle.y -= fall;
                false
            }
        }
    }

    /// Steps up as high as a step, or a ceiling, lets it, across by `across` sliding along
    /// whatever's still in the way, and down onto what it can stand on, and returns where its
    /// middle ends up, or `None` if there's nothing there to stand on within a step down.
    fn step_over(&mut self, across: Vector) -> Option<Vector> {
        let rise = match self.cast(Vector::Y, self.character.step) {
            Some(hit) => (hit.distance - GAP).max(0.0),
            None => self.character.step,
        };
        self.middle.y += rise;
        self.slide(across, Slide::Upright);
        let hit = self
            .cast(Vector::NEG_Y, rise + self.character.step)
            .filter(|hit| hit.normal.y >= self.walkable)?;
        self.middle.y -= hit.distance - GAP;
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
            let standing = hit.normal.y >= self.walkable;
            if how == Slide::Free && standing && left.y < 0.0 {
                // Landed: the rest of the way is across the ground.
                slid.landed = true;
                left.y = 0.0;
            }
            let normal = if how == Slide::Upright || (how == Slide::Along && !standing) {
                let upright = hit.normal.with_y(0.0);
                let length = upright.length();
                if length < 1.0e-6 {
                    break;
                }
                upright / length
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

/// The normal out of what a character's cylinder hit, toward the cylinder, from where they
/// touched: that of a face of `collider` there, or where the cylinder touched it with its flat
/// bottom or top or its side, or as Rapier has it, `hit.normal1`, where the two touched by edges.
///
/// Rapier works out where shapes touch by iterating (GJK), which finds the normal there only to
/// within a hundredth of a radian, and a character pushed at a wall would creep along it, and
/// one standing on the ground slide off it, by that much each time.
fn true_normal(hit: &ShapeCastHit, collider: &Collider, cylinder: &Cylinder) -> Vector {
    // Within a few degrees of a face it touched: where it touched by an edge, the normal is far
    // from either face's.
    const FACE: f32 = 0.995;
    // How near the cylinder's edges a point counts as on them, and how near the faces of what it
    // touched.
    const EDGE: f32 = 1.0e-3;
    let pose = collider.position();
    let shape = collider.shape();
    let local = pose.inverse_transform_point(hit.witness1);
    let normal = pose.rotation.inverse() * hit.normal1;
    // The faces it may have touched, in the collider's coordinates.
    let faces: Vec<Vector> = if let Some(triangles) = shape.as_trimesh() {
        // The triangles there, from either side: where it touched by an edge between triangles,
        // they may be of one flat face.
        let near = Aabb::new(local - Vector::splat(EDGE), local + Vector::splat(EDGE));
        triangles
            .bvh()
            .intersect_aabb(&near)
            .filter_map(|i| triangles.triangle(i).normal())
            .map(|face| face * face.dot(normal).signum())
            .collect()
    } else if shape.as_cuboid().is_some() {
        vec![
            Vector::X,
            Vector::NEG_X,
            Vector::Y,
            Vector::NEG_Y,
            Vector::Z,
            Vector::NEG_Z,
        ]
    } else {
        let (projection, feature) = shape.project_local_point_and_get_feature(local);
        match feature {
            FeatureId::Face(_) => shape
                .feature_normal_at_point(projection.subshape, feature, projection.point)
                .into_iter()
                .collect(),
            _ => Vec::new(),
        }
    };
    let nearest = faces
        .into_iter()
        .max_by(|a, b| a.dot(normal).total_cmp(&b.dot(normal)));
    if let Some(face) = nearest.filter(|face| face.dot(normal) > FACE) {
        return square(pose.rotation * face);
    }
    // Where it touched the cylinder, from its middle: it isn't turned.
    let point = hit.witness2;
    let out = point.with_y(0.0);
    let inside = out.length() < cylinder.radius - EDGE;
    let level = point.y.abs() < cylinder.half_height - EDGE;
    match () {
        _ if inside && point.y < 0.0 => Vector::Y,
        _ if inside => Vector::NEG_Y,
        _ if level => -out / out.length(),
        _ => hit.normal1,
    }
}

/// The pose `t` of the way from `from` to `to`, turned along the way by normalizing the
/// rotations' blend, which takes only a square root, so it gives the same bits everywhere.
fn between(from: Pose, to: Pose, t: f32) -> Pose {
    let translation = from.translation + (to.translation - from.translation) * t;
    // Unturned, it stays exactly as it was, rather than as normalizing rounds it.
    let rotation = if from.rotation == to.rotation {
        from.rotation
    } else {
        from.rotation.lerp(to.rotation, t)
    };
    Pose::from_parts(translation, rotation)
}

/// `normal`, along the world's axis it's within rounding of, if it is: a face turned a quarter
/// turn by sines that aren't quite 1 and 0.
fn square(normal: Vector) -> Vector {
    const ROUNDING: f32 = 1.0e-5;
    let axes = [Vector::X, Vector::Y, Vector::Z];
    axes.into_iter()
        .flat_map(|axis| [axis, -axis])
        .find(|axis| (normal - *axis).abs().max_element() < ROUNDING)
        .unwrap_or(normal)
}

/// Whether `collider` is of one of the bodies `except`.
fn excepts(except: &[BodyId], collider: &Collider) -> bool {
    collider
        .parent()
        .is_some_and(|body| except.contains(&BodyId(body)))
}

/// Fails unless a sensor can be of `kind` and `shape`: an area doesn't fall, and is solid all
/// through, which triangles aren't.
fn check_sensor(kind: BodyKind, shape: &Shape3d) -> Result<(), String> {
    if kind == BodyKind::Dynamic {
        return Err("a sensor doesn't fall: make it fixed or kinematic".into());
    }
    if matches!(shape, Shape3d::Triangles { .. }) {
        return Err(
            "a sensor is an area, solid all through, which triangles aren't: give it a hull, a \
             box or a ball"
                .into(),
        );
    }
    Ok(())
}

/// `collider`, made a sensor if `sensor` says so: one that nothing bumps into, and that the
/// world's steps leave alone, since what's in it is asked for when it's wanted.
fn sensed(collider: ColliderBuilder, sensor: bool) -> ColliderBuilder {
    if sensor {
        collider
            .sensor(true)
            .active_collision_types(ActiveCollisionTypes::empty())
    } else {
        collider
    }
}

/// Fails unless `material` is one a body of `kind` can be of: what falls must weigh something.
fn check_material(material: &Material, kind: BodyKind) -> Result<(), String> {
    let Material {
        density,
        friction,
        bounce,
    } = *material;
    for (name, value) in [
        ("density", density),
        ("friction", friction),
        ("bounce", bounce),
    ] {
        if !(value.is_finite() && value >= 0.0) {
            return Err(format!("a body's {name} must be 0 or more, not {value}"));
        }
    }
    if kind == BodyKind::Dynamic && density == 0.0 {
        return Err("a dynamic body's density must be more than 0, so it weighs something".into());
    }
    Ok(())
}

/// A collider of `shape`, for a body of `kind`.
fn collider(shape: &Shape3d, kind: BodyKind) -> Result<ColliderBuilder, String> {
    let finite = |values: &[f32]| values.iter().all(|value| value.is_finite());
    let point = |point: &[f32; 3]| Vector::from(*point);
    let collider = match shape {
        Shape3d::Triangles { points, triangles } => {
            if kind != BodyKind::Fixed {
                return Err(
                    "only fixed bodies are shaped by their triangles: give one that moves a \
                     hull, a box or a ball"
                        .into(),
                );
            }
            if !points.iter().all(|point| finite(point)) {
                return Err("its triangles' corners must be finite".into());
            }
            let count = points.len() as u32;
            if let Some(index) = triangles.iter().flatten().find(|&&index| index >= count) {
                return Err(format!(
                    "index {index} is past the last of its {count} corners"
                ));
            }
            // Without triangles with no area, which have no side to be solid on.
            let corners = |triangle: &[u32; 3]| triangle.map(|i| point(&points[i as usize]));
            let triangles: Vec<[u32; 3]> = triangles
                .iter()
                .copied()
                .filter(|triangle| {
                    let [a, b, c] = corners(triangle);
                    (b - a).cross(c - a).length_squared() > 0.0
                })
                .collect();
            if triangles.is_empty() {
                return Err("it has no triangles with any area".into());
            }
            let points: Vec<Vector> = points.iter().map(point).collect();
            // Smoothed over the edges between its triangles, so what slides across a floor of
            // many doesn't catch on them, if its triangles join up well enough for that.
            let shape = SharedShape::trimesh_with_flags(
                points.clone(),
                triangles.clone(),
                TriMeshFlags::FIX_INTERNAL_EDGES,
            )
            .or_else(|_| {
                SharedShape::trimesh_with_flags(
                    points,
                    triangles,
                    TriMeshFlags::MERGE_DUPLICATE_VERTICES,
                )
            })
            .map_err(|err| format!("its triangles can't be made solid: {err}"))?;
            ColliderBuilder::new(shape)
        }
        Shape3d::Hull(points) => {
            if !points.iter().all(|point| finite(point)) {
                return Err("its corners must be finite".into());
            }
            let points: Vec<Vector> = points.iter().map(point).collect();
            let shape = SharedShape::convex_hull(&points)
                .ok_or("it has too few corners to make a shape of")?;
            ColliderBuilder::new(shape)
        }
        Shape3d::Box(bounds) => {
            let half = Vector::from(bounds.size()) / 2.0;
            if !(finite(&bounds.min) && finite(&bounds.max)) {
                return Err("a box's corners must be finite".into());
            }
            if half.min_element() < 0.0 {
                return Err("a box's corners must be its least and its most".into());
            }
            ColliderBuilder::new(SharedShape::cuboid(half.x, half.y, half.z))
                .position(Pose::from_translation(Vector::from(bounds.center())))
        }
        Shape3d::Ball { center, radius } => {
            if !(finite(center) && radius.is_finite() && *radius > 0.0) {
                return Err(format!("a ball needs a radius more than 0, not {radius}"));
            }
            ColliderBuilder::new(SharedShape::ball(*radius))
                .position(Pose::from_translation(point(center)))
        }
        Shape3d::Cylinder { radius, height } => {
            if !(radius.is_finite() && height.is_finite() && *radius > 0.0 && *height > 0.0) {
                return Err(format!(
                    "a cylinder needs a radius and a height more than 0, not {radius} and {height}"
                ));
            }
            ColliderBuilder::new(SharedShape::cylinder(height / 2.0, *radius))
                .position(Pose::from_translation(Vector::Y * (height / 2.0)))
        }
    };
    // What's flat weighs nothing, which only what doesn't fall can.
    let mass = collider.shape.mass_properties(1.0).mass();
    if kind == BodyKind::Dynamic && (mass.is_nan() || mass <= 0.0) {
        return Err("it's flat, with nothing inside to weigh: give what falls some depth".into());
    }
    Ok(collider)
}

#[cfg(test)]
mod tests {
    use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, FRAC_PI_6};

    use super::*;

    const FRAME: f32 = 1.0 / 60.0;

    fn bounds(min: [f32; 3], max: [f32; 3]) -> Bounds3 {
        Bounds3 { min, max }
    }

    fn fixed(shape: Shape3d) -> Body3d {
        Body3d {
            kind: BodyKind::Fixed,
            shape,
            pose: Pose3d::default(),
            material: Material::default(),
            sensor: false,
        }
    }

    /// A world with ground whose top is at `y` 0, 100 across each way.
    fn ground() -> World3d {
        let mut world = World3d::new();
        let ground = bounds([-50.0, -1.0, -50.0], [50.0, 0.0, 50.0]);
        world.add(&fixed(Shape3d::Box(ground))).unwrap();
        world
    }

    /// A box half a unit across each way, standing on its bottom's middle.
    fn crate_at(position: [f32; 3]) -> Body3d {
        Body3d {
            kind: BodyKind::Dynamic,
            shape: Shape3d::Box(bounds([-0.25, 0.0, -0.25], [0.25, 0.5, 0.25])),
            pose: Pose3d::at(position),
            material: Material::default(),
            sensor: false,
        }
    }

    const WALKER: Character = Character {
        radius: 0.2,
        height: 1.0,
        step: 0.3,
        slope: FRAC_PI_4,
        mass: 1.0,
    };

    /// Walks `WALKER` from `feet` by `motion` each frame for `frames`, stepping the world, and
    /// returns where its feet end up and whether it ended on the ground.
    fn walk_for(
        world: &mut World3d,
        mut feet: [f32; 3],
        motion: [f32; 3],
        frames: u32,
    ) -> ([f32; 3], bool) {
        let mut grounded = false;
        for _ in 0..frames {
            let walked = world.walk(&WALKER, feet, motion, None);
            feet = add(feet, walked.moved);
            grounded = walked.grounded;
            world.step(FRAME);
        }
        (feet, grounded)
    }

    fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
        [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
    }

    fn assert_near(actual: [f32; 3], expected: [f32; 3], tolerance: f32) {
        let off = (0..3)
            .map(|i| (actual[i] - expected[i]).abs())
            .fold(0.0, f32::max);
        assert!(off <= tolerance, "{actual:?}, not {expected:?}");
    }

    #[test]
    fn poses_turn_as_nodes_do_and_back() {
        for (yaw, pitch, roll) in [
            (0.0, 0.0, 0.0),
            (FRAC_PI_2, 0.0, 0.0),
            (0.3, -0.7, 1.1),
            (-2.5, 0.4, -3.0),
            (1.0, FRAC_PI_2, 0.0),
        ] {
            let pose = Pose3d {
                position: [1.0, 2.0, 3.0],
                yaw,
                pitch,
                roll,
            };
            let rapier = pose.to_rapier();
            // As a node turns: rolled, pitched, then turned.
            let turned = crate::math::Affine3::from_rotation_y(yaw)
                * crate::math::Affine3::from_rotation_x(pitch)
                * crate::math::Affine3::from_rotation_z(roll);
            for axis in [Vector::X, Vector::Y, Vector::Z] {
                let expected = Vector::from(turned.apply_vector(axis.into()));
                assert!(
                    (rapier.rotation * axis - expected).length() < 1.0e-5,
                    "{yaw}, {pitch}, {roll}"
                );
            }
            // Back again, turned the same way, if not by the same angles.
            let back = Pose3d::from_rapier(&rapier).to_rapier();
            assert!(back.rotation.angle_between(rapier.rotation) < 1.0e-3);
            assert_eq!(back.translation, rapier.translation);
        }
    }

    #[test]
    fn dynamic_bodies_fall_and_rest_on_what_is_below() {
        let mut world = ground();
        let low = world.add(&crate_at([0.0, 2.0, 0.0])).unwrap();
        let high = world.add(&crate_at([0.1, 3.0, 0.0])).unwrap();
        for _ in 0..240 {
            world.step(FRAME);
        }
        // Stacked, with the ground's top and the lower crate's at 0 and 0.5.
        let low_pose = world.pose(low).unwrap();
        let high_pose = world.pose(high).unwrap();
        assert!(low_pose.position[1].abs() < 0.02, "{low_pose:?}");
        assert!((high_pose.position[1] - 0.5).abs() < 0.02, "{high_pose:?}");
        assert!(world.is_resting(low) && world.is_resting(high));
        assert!(world.velocity(high)[1].abs() < 0.01);
    }

    #[test]
    fn steps_are_taken_in_equal_parts() {
        let run = |dt: f32, steps: u32| {
            let mut world = ground();
            let id = world.add(&crate_at([0.0, 2.0, 0.0])).unwrap();
            world.push(id, [0.3, 0.0, 0.1]);
            for _ in 0..steps {
                world.step(dt);
            }
            world.pose(id).unwrap()
        };
        assert_eq!(run(FRAME * 2.0, 30), run(FRAME, 60));
        // Nothing, or something that isn't a time, doesn't step.
        let mut world = ground();
        let id = world.add(&crate_at([0.0, 2.0, 0.0])).unwrap();
        for dt in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            world.step(dt);
        }
        assert_eq!(world.pose(id).unwrap().position, [0.0, 2.0, 0.0]);
    }

    #[test]
    fn kinematic_bodies_move_where_they_are_put_and_push() {
        let mut world = ground();
        let pusher = world
            .add(&Body3d {
                kind: BodyKind::Kinematic,
                ..crate_at([-1.0, 0.0, 0.0])
            })
            .unwrap();
        let pushed = world.add(&crate_at([0.0, 0.0, 0.0])).unwrap();
        for frame in 1..=60 {
            let x = -1.0 + frame as f32 / 30.0;
            world.set_pose(pusher, Pose3d::at([x, 0.0, 0.0]));
            world.step(FRAME);
        }
        assert_eq!(world.pose(pusher).unwrap().position, [1.0, 0.0, 0.0]);
        // Shoved ahead of it.
        assert!(world.pose(pushed).unwrap().position[0] > 1.4);
    }

    #[test]
    fn kinematic_bodies_go_evenly_through_steps_taken_in_parts() {
        let mut world = ground();
        let id = world
            .add(&Body3d {
                kind: BodyKind::Kinematic,
                ..crate_at([0.0; 3])
            })
            .unwrap();
        world.set_pose(id, Pose3d::at([1.0, 0.0, 0.0]));
        // Across a unit in two sixtieths, taken in two parts: at 30 units a second throughout.
        world.step(FRAME * 2.0);
        assert_eq!(world.pose(id).unwrap().position, [1.0, 0.0, 0.0]);
        let [vx, vy, vz] = world.velocity(id);
        assert!(
            (vx - 30.0).abs() < 1.0e-3 && vy == 0.0 && vz == 0.0,
            "{vx}, {vy}, {vz}"
        );
        assert!(!world.is_resting(id));
        // Turning as it goes, it ends turned as it was put.
        let turned = Pose3d {
            yaw: 1.0,
            ..Pose3d::at([2.0, 0.0, 0.0])
        };
        world.set_pose(id, turned);
        world.step(FRAME * 3.0);
        let pose = world.pose(id).unwrap();
        assert_eq!(pose.position, turned.position);
        assert!((pose.yaw - 1.0).abs() < 1.0e-5, "{pose:?}");
        // Turning where it is, it isn't at rest, and once it stops, it is.
        world.set_pose(id, Pose3d { yaw: 2.0, ..turned });
        world.step(FRAME);
        assert!(!world.is_resting(id));
        world.step(FRAME);
        assert!(world.is_resting(id));
    }

    #[test]
    fn kinematic_bodies_are_put_at_once() {
        let mut world = ground();
        let id = world
            .add(&Body3d {
                kind: BodyKind::Kinematic,
                ..crate_at([-0.5, 0.0, 0.0])
            })
            .unwrap();
        // A crate a hair from its right, which going there over a step would fling.
        let beside = world.add(&crate_at([0.001, 0.0, 0.0])).unwrap();
        for _ in 0..30 {
            world.step(FRAME);
        }
        world.put(id, Pose3d::at([3.0, 0.0, 0.0]));
        assert_eq!(world.pose(id).unwrap().position, [3.0, 0.0, 0.0]);
        world.step(FRAME);
        assert_eq!(world.pose(id).unwrap().position, [3.0, 0.0, 0.0]);
        assert_eq!(world.velocity(id), [0.0; 3]);
        let [vx, _, _] = world.velocity(beside);
        assert!(vx.abs() < 0.1, "flung at {vx}");
    }

    #[test]
    fn materials_that_cant_be_say_why() {
        let mut world = World3d::new();
        let with = |kind, material| Body3d {
            kind,
            material,
            ..crate_at([0.0; 3])
        };
        let weightless = Material {
            density: 0.0,
            ..Material::default()
        };
        let err = world.add(&with(BodyKind::Dynamic, weightless)).unwrap_err();
        assert!(err.contains("density must be more than 0"), "{err}");
        // What doesn't fall needn't weigh anything.
        assert!(world.add(&with(BodyKind::Fixed, weightless)).is_ok());
        assert!(world.add(&with(BodyKind::Kinematic, weightless)).is_ok());
        for (material, name) in [
            (
                Material {
                    density: -1.0,
                    ..Material::default()
                },
                "density",
            ),
            (
                Material {
                    friction: -0.5,
                    ..Material::default()
                },
                "friction",
            ),
            (
                Material {
                    bounce: f32::NAN,
                    ..Material::default()
                },
                "bounce",
            ),
        ] {
            let err = world.add(&with(BodyKind::Fixed, material)).unwrap_err();
            assert!(err.contains(&format!("{name} must be 0 or more")), "{err}");
        }
        assert_eq!(world.len(), 2);
    }

    #[test]
    fn characters_go_down_with_what_they_stand_on() {
        let mut world = ground();
        // A lift whose top starts at 1, going down a unit a second.
        let lift = world
            .add(&Body3d {
                kind: BodyKind::Kinematic,
                shape: Shape3d::Box(bounds([-1.0, -0.2, -1.0], [1.0, 0.0, 1.0])),
                pose: Pose3d::at([0.0, 1.0, 0.0]),
                material: Material::default(),
                sensor: false,
            })
            .unwrap();
        let mut feet = [0.0, 1.0 + GAP, 0.0];
        for frame in 1..=30 {
            world.set_pose(lift, Pose3d::at([0.0, 1.0 - frame as f32 / 60.0, 0.0]));
            // Standing still, then walking, pulled down as a game's gravity reset on the
            // ground pulls it.
            let x = if frame <= 15 { 0.0 } else { 0.01 };
            let walked = world.walk(&WALKER, feet, [x, -0.005, 0.0], None);
            assert!(walked.grounded, "left the lift at frame {frame}: {feet:?}");
            feet = add(feet, walked.moved);
            world.step(FRAME);
        }
        // A gap above the lift's top as it was for the last walk, before the last step.
        assert_near(feet, [0.15, 1.0 - 29.0 / 60.0 + GAP, 0.0], 1.0e-4);
    }

    #[test]
    fn bodies_are_reshaped_and_removed() {
        let mut world = ground();
        let id = world.add(&crate_at([0.0, 0.0, 0.0])).unwrap();
        // Twice as tall, so another lands on it higher.
        let tall = Shape3d::Box(bounds([-0.25, 0.0, -0.25], [0.25, 1.0, 0.25]));
        world.set_shape(id, &tall).unwrap();
        let other = world.add(&crate_at([0.0, 2.0, 0.0])).unwrap();
        for _ in 0..120 {
            world.step(FRAME);
        }
        assert!((world.pose(other).unwrap().position[1] - 1.0).abs() < 0.02);

        world.remove(id);
        assert_eq!(world.len(), 2);
        assert_eq!(world.pose(id), None);
        for _ in 0..120 {
            world.step(FRAME);
        }
        assert!(world.pose(other).unwrap().position[1].abs() < 0.02);
        // Removing it again does nothing, nor does anything else.
        world.remove(id);
        world.set_shape(id, &tall).unwrap();
        world.set_pose(id, Pose3d::default());
        world.push(id, [1.0, 0.0, 0.0]);
        assert_eq!(world.velocity(id), [0.0; 3]);
        assert_eq!(world.len(), 2);
    }

    #[test]
    fn shapes_that_cant_be_made_say_why() {
        let mut world = World3d::new();
        let triangles = Shape3d::Triangles {
            points: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            triangles: vec![[0, 2, 1]],
        };
        let moving = Body3d {
            kind: BodyKind::Dynamic,
            ..fixed(triangles.clone())
        };
        assert!(
            world
                .add(&moving)
                .unwrap_err()
                .contains("only fixed bodies")
        );
        assert!(world.add(&fixed(triangles)).is_ok());
        // What's flat can be fixed, but not fall, weighing nothing.
        let flat = Shape3d::Hull(vec![
            [0.0; 3],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
        ]);
        assert!(world.add(&fixed(flat.clone())).is_ok());
        let falling = |shape| Body3d {
            kind: BodyKind::Dynamic,
            ..fixed(shape)
        };
        assert!(world.add(&falling(flat)).unwrap_err().contains("flat"));
        let thin = Shape3d::Box(bounds([0.0; 3], [1.0, 0.0, 1.0]));
        assert!(world.add(&fixed(thin.clone())).is_ok());
        assert!(world.add(&falling(thin)).unwrap_err().contains("flat"));
        let inside_out = Shape3d::Box(bounds([1.0; 3], [0.0; 3]));
        assert!(world.add(&fixed(inside_out)).unwrap_err().contains("box"));
        let out_of_range = Shape3d::Triangles {
            points: vec![[0.0; 3]],
            triangles: vec![[0, 1, 2]],
        };
        assert!(
            world
                .add(&fixed(out_of_range))
                .unwrap_err()
                .contains("index 1")
        );
        let ball = Shape3d::Ball {
            center: [0.0; 3],
            radius: f32::NAN,
        };
        assert!(world.add(&fixed(ball)).is_err());
        assert_eq!(world.len(), 3);
    }

    #[test]
    fn characters_walk_across_the_ground() {
        let mut world = ground();
        let (feet, grounded) = walk_for(&mut world, [0.0, GAP, 0.0], [0.05, -0.01, 0.0], 60);
        assert!(grounded);
        assert_near(feet, [3.0, GAP, 0.0], 1.0e-4);
        // Standing still, nothing moves it.
        let walked = world.walk(&WALKER, feet, [0.0, -0.01, 0.0], None);
        assert_eq!(walked.moved, [0.0; 3]);
        assert!(walked.grounded);
    }

    #[test]
    fn walls_stop_characters_without_their_creeping_along() {
        let mut world = ground();
        let wall = bounds([1.0, 0.0, -5.0], [1.5, 2.0, 5.0]);
        world.add(&fixed(Shape3d::Box(wall))).unwrap();
        let (feet, _) = walk_for(&mut world, [0.0, GAP, 0.0], [0.05, -0.01, 0.0], 600);
        // Its side, a radius from its middle, a gap from the wall.
        assert_near(feet, [1.0 - WALKER.radius - GAP, GAP, 0.0], 1.0e-4);
        assert_eq!(feet[2], 0.0, "crept along the wall");

        // A wall turned a twelfth of a turn, walked at straight on.
        let mut world = ground();
        let turned = Body3d {
            pose: Pose3d {
                yaw: FRAC_PI_6,
                ..Pose3d::default()
            },
            ..fixed(Shape3d::Box(bounds([1.0, 0.0, -5.0], [1.5, 2.0, 5.0])))
        };
        world.add(&turned).unwrap();
        // Its normal, turned from -x by the yaw.
        let (sin, cos) = FRAC_PI_6.sin_cos();
        let into = [cos * 0.05, 0.0, -sin * 0.05];
        let (feet, _) = walk_for(&mut world, [0.0, GAP, 0.0], into, 600);
        let along = feet[0] * sin + feet[2] * cos;
        assert!(along.abs() < 1.0e-4, "crept {along} along the turned wall");
    }

    #[test]
    fn walls_of_triangles_stop_characters_without_their_creeping_along() {
        // Upright, facing -x at 1, in two triangles whose shared edge runs across where the
        // character touches it.
        let mut world = ground();
        let points = vec![
            [1.0, 0.0, -5.0],
            [1.0, 0.0, 5.0],
            [1.0, 2.0, 5.0],
            [1.0, 2.0, -5.0],
        ];
        let triangles = vec![[0, 1, 2], [0, 2, 3]];
        world
            .add(&fixed(Shape3d::Triangles { points, triangles }))
            .unwrap();
        let (feet, _) = walk_for(&mut world, [0.0, GAP, 0.3], [0.05, -0.01, 0.0], 600);
        assert_near(feet, [1.0 - WALKER.radius - GAP, GAP, 0.3], 1.0e-4);
        assert_eq!(feet[2], 0.3, "crept along the wall");
    }

    /// A hash of the bits of every body's pose.
    fn hash_of(world: &World3d, ids: &[BodyId]) -> u64 {
        let numbers = ids.iter().flat_map(|&id| {
            let pose = world.pose(id).unwrap();
            let [x, y, z] = pose.position;
            [x, y, z, pose.yaw, pose.pitch, pose.roll]
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
        // A slope of triangles to tumble down.
        let points = vec![
            [2.0, 0.0, -2.0],
            [2.0, 0.0, 2.0],
            [4.0, 1.5, 2.0],
            [4.0, 1.5, -2.0],
        ];
        world
            .add(&fixed(Shape3d::Triangles {
                points,
                triangles: vec![[0, 1, 2], [0, 2, 3]],
            }))
            .unwrap();
        // A pile of crates, turned, and balls, dropped on it and around it.
        let mut ids = Vec::new();
        for i in 0..12 {
            let f = i as f32;
            let mut body = crate_at([
                1.0 + (f * 0.37) % 2.5,
                1.0 + f * 0.6,
                (f * 0.53) % 1.5 - 0.75,
            ]);
            body.pose.yaw = f * 0.4;
            body.pose.roll = f * 0.1;
            if i % 3 == 0 {
                body.shape = Shape3d::Ball {
                    center: [0.0, 0.25, 0.0],
                    radius: 0.25,
                };
            }
            ids.push(world.add(&body).unwrap());
        }
        // And a character walking into them, there and back.
        let mut feet = [-2.0, GAP, 0.1];
        for frame in 0..360 {
            let x = if frame < 200 { 0.04 } else { -0.03 };
            let walked = world.walk(&WALKER, feet, [x, -0.01, 0.005], None);
            feet = add(feet, walked.moved);
            world.step(FRAME);
        }
        let hash = hash_of(&world, &ids);
        // Rapier's enhanced determinism, and `libm` for the rest, make this the same on Windows and
        // Linux, debug and release. A change to how bodies or characters move changes it.
        assert_eq!(hash, 0xd402_7fd3_701d_6e47, "{hash:#x}");
    }

    #[test]
    fn characters_slide_along_walls() {
        let mut world = ground();
        let wall = bounds([1.0, 0.0, -5.0], [1.5, 2.0, 5.0]);
        world.add(&fixed(Shape3d::Box(wall))).unwrap();
        let (feet, _) = walk_for(&mut world, [0.0, GAP, 0.0], [0.05, -0.01, 0.05], 40);
        assert_near(feet, [1.0 - WALKER.radius - GAP, GAP, 2.0], 1.0e-3);
    }

    #[test]
    fn characters_step_up_onto_what_is_low_enough() {
        let mut world = ground();
        // A stair 0.25 high, then a wall 0.5 higher.
        let stair = bounds([1.0, 0.0, -5.0], [3.0, 0.25, 5.0]);
        world.add(&fixed(Shape3d::Box(stair))).unwrap();
        let wall = bounds([2.0, 0.25, -5.0], [3.0, 0.75, 5.0]);
        world.add(&fixed(Shape3d::Box(wall))).unwrap();
        let (feet, grounded) = walk_for(&mut world, [0.0, GAP, 0.0], [0.05, -0.01, 0.0], 60);
        assert!(grounded);
        assert_near(feet, [2.0 - WALKER.radius - GAP, 0.25 + GAP, 0.0], 1.0e-3);
        // And down again.
        let (feet, grounded) = walk_for(&mut world, feet, [-0.05, -0.01, 0.0], 60);
        assert!(grounded);
        assert_near(feet, [feet[0], GAP, 0.0], 1.0e-3);
    }

    /// A wedge rising along `x` from 0 at `x` 0 to `height` at `x` 1, 10 deep.
    fn wedge(height: f32) -> Shape3d {
        let mut points = Vec::new();
        for z in [-5.0, 5.0] {
            points.extend([[0.0, 0.0, z], [1.0, 0.0, z], [1.0, height, z]]);
        }
        Shape3d::Hull(points)
    }

    #[test]
    fn characters_walk_up_slopes_gentle_enough() {
        // Rising by 0.5 across 1: about 27 degrees.
        let mut world = ground();
        world.add(&fixed(wedge(0.5))).unwrap();
        let (feet, grounded) = walk_for(&mut world, [-1.0, GAP, 0.0], [0.02, -0.01, 0.0], 60);
        assert!(grounded);
        assert!(feet[0] > 0.0 && feet[1] > 0.1, "{feet:?}");
        // And down again, on the ground all the way.
        let mut feet = feet;
        for _ in 0..60 {
            let walked = world.walk(&WALKER, feet, [-0.02, -0.01, 0.0], None);
            assert!(walked.grounded, "left the ground at {feet:?}");
            feet = add(feet, walked.moved);
            world.step(FRAME);
        }

        // Rising by 2 across 1, about 63 degrees: as a wall.
        let mut world = ground();
        world.add(&fixed(wedge(2.0))).unwrap();
        let (feet, _) = walk_for(&mut world, [-1.0, GAP, 0.0], [0.02, -0.01, 0.0], 120);
        assert!(feet[1] < WALKER.step + 0.01, "{feet:?}");
        assert!(feet[0] < 0.2, "{feet:?}");
    }

    #[test]
    fn characters_fall_off_ledges_and_land() {
        let mut world = ground();
        let ledge = bounds([-2.0, 0.0, -2.0], [0.0, 1.0, 2.0]);
        world.add(&fixed(Shape3d::Box(ledge))).unwrap();
        // Off the ledge's edge: it leaves the ground, falling as it's told to.
        let walked = world.walk(&WALKER, [-0.1, 1.0 + GAP, 0.0], [0.4, -0.05, 0.0], None);
        assert!(!walked.grounded);
        assert_near(walked.moved, [0.4, -0.05, 0.0], 1.0e-5);
        // And lands on the ground below.
        let (feet, grounded) = walk_for(&mut world, [0.3, 0.95, 0.0], [0.0, -0.1, 0.0], 20);
        assert!(grounded);
        assert_near(feet, [0.3, GAP, 0.0], 1.0e-4);
    }

    #[test]
    fn rising_characters_stop_at_ceilings() {
        let mut world = ground();
        let ceiling = bounds([-2.0, 2.0, -2.0], [2.0, 3.0, 2.0]);
        world.add(&fixed(Shape3d::Box(ceiling))).unwrap();
        let walked = world.walk(&WALKER, [0.0, GAP, 0.0], [0.0, 1.5, 0.0], None);
        assert!(!walked.grounded);
        assert_near(walked.moved, [0.0, 1.0 - 2.0 * GAP, 0.0], 1.0e-4);
    }

    #[test]
    fn characters_push_what_they_walk_into() {
        let mut world = ground();
        let pushed = world.add(&crate_at([1.0, 0.0, 0.0])).unwrap();
        for _ in 0..60 {
            world.step(FRAME);
        }
        let (feet, _) = walk_for(&mut world, [0.0, GAP, 0.0], [0.03, -0.01, 0.0], 60);
        let pose = world.pose(pushed).unwrap();
        // Pushed along ahead of the character, which kept walking.
        assert!(pose.position[0] > 2.0, "{pose:?}");
        assert!(feet[0] > 1.0, "{feet:?}");
        assert!(pose.position[2].abs() < 0.05, "{pose:?}");
    }

    #[test]
    fn a_character_is_not_in_its_own_way() {
        let mut world = ground();
        let own = world
            .add(&Body3d {
                kind: BodyKind::Kinematic,
                shape: Shape3d::Cylinder {
                    radius: WALKER.radius,
                    height: WALKER.height,
                },
                pose: Pose3d::at([0.0, GAP, 0.0]),
                material: Material::default(),
                sensor: false,
            })
            .unwrap();
        let walked = world.walk(&WALKER, [0.0, GAP, 0.0], [0.5, 0.0, 0.0], Some(own));
        assert_near(walked.moved, [0.5, 0.0, 0.0], 1.0e-4);
        // Others are, as it is to them.
        let walked = world.walk(&WALKER, [-1.0, GAP, 0.0], [1.0, 0.0, 0.0], None);
        assert_near(
            walked.moved,
            [1.0 - 2.0 * WALKER.radius - GAP, 0.0, 0.0],
            1.0e-3,
        );
    }

    /// A world with ground whose top is at `y` 0, as [`ground`] makes, and the ground's body.
    fn with_ground() -> (World3d, BodyId) {
        let mut world = World3d::new();
        let ground = bounds([-50.0, -1.0, -50.0], [50.0, 0.0, 50.0]);
        let id = world.add(&fixed(Shape3d::Box(ground))).unwrap();
        (world, id)
    }

    fn sensor_body(kind: BodyKind, shape: Shape3d) -> Body3d {
        Body3d {
            kind,
            sensor: true,
            ..fixed(shape)
        }
    }

    fn set(bodies: Vec<BodyId>) -> std::collections::HashSet<BodyId> {
        bodies.into_iter().collect()
    }

    #[test]
    fn rays_hit_the_nearest_body_where_it_is() {
        // Bodies added since the last step, as these are, are where they are.
        let (mut world, ground) = with_ground();
        let block = world
            .add(&fixed(Shape3d::Box(bounds(
                [2.0, 0.0, -1.0],
                [3.0, 1.0, 1.0],
            ))))
            .unwrap();
        let hit = world.cast_ray([0.0, 5.0, 0.0], [0.0, -1.0, 0.0], f32::MAX, &[]);
        let hit = hit.expect("the ray hit the ground");
        assert_eq!(hit.body, ground);
        assert_near(hit.point, [0.0, 0.0, 0.0], 1.0e-5);
        assert!((hit.distance - 5.0).abs() < 1.0e-5, "{hit:?}");
        assert_eq!(hit.normal, [0.0, 1.0, 0.0]);
        // Across to the block, in lengths of the direction.
        let hit = world.cast_ray([0.0, 0.5, 0.0], [2.0, 0.0, 0.0], f32::MAX, &[]);
        let hit = hit.expect("the ray hit the block");
        assert_eq!(hit.body, block);
        assert!((hit.distance - 1.0).abs() < 1.0e-5, "{hit:?}");
        assert_eq!(hit.normal, [-1.0, 0.0, 0.0]);
        // Not as far as it, or up into nothing.
        assert_eq!(
            world.cast_ray([0.0, 0.5, 0.0], [1.0, 0.0, 0.0], 1.5, &[]),
            None
        );
        assert_eq!(
            world.cast_ray([0.0, 0.5, 0.0], [0.0, 1.0, 0.0], f32::MAX, &[]),
            None
        );
    }

    #[test]
    fn rays_hit_triangles_where_they_are_not_the_box_around_them() {
        // A slope of two triangles, rising from 0 at `x` 0 to 1 at `x` 1.
        let mut world = World3d::new();
        let points = vec![
            [0.0, 0.0, -5.0],
            [0.0, 0.0, 5.0],
            [1.0, 1.0, 5.0],
            [1.0, 1.0, -5.0],
        ];
        let triangles = vec![[0, 1, 2], [0, 2, 3]];
        world
            .add(&fixed(Shape3d::Triangles { points, triangles }))
            .unwrap();
        let hit = world.cast_ray([0.25, 5.0, 0.0], [0.0, -1.0, 0.0], f32::MAX, &[]);
        let hit = hit.expect("the ray hit the slope");
        assert_near(hit.point, [0.25, 0.25, 0.0], 1.0e-5);
        let half = std::f32::consts::FRAC_1_SQRT_2;
        assert_near(hit.normal, [-half, half, 0.0], 1.0e-5);
        // From under it, it faces back down along the ray.
        let hit = world.cast_ray([0.75, -1.0, 0.0], [0.0, 1.0, 0.0], f32::MAX, &[]);
        let hit = hit.expect("the ray hit the slope from below");
        assert_near(hit.point, [0.75, 0.75, 0.0], 1.0e-5);
        assert_near(hit.normal, [half, -half, 0.0], 1.0e-5);
    }

    #[test]
    fn turned_bodies_are_hit_on_their_turned_faces() {
        // A wall turned a twelfth of a turn about the origin.
        let mut world = World3d::new();
        let turned = Body3d {
            pose: Pose3d {
                yaw: FRAC_PI_6,
                ..Pose3d::default()
            },
            ..fixed(Shape3d::Box(bounds([1.0, 0.0, -5.0], [1.5, 2.0, 5.0])))
        };
        world.add(&turned).unwrap();
        let hit = world.cast_ray([0.0, 1.0, 0.0], [1.0, 0.0, 0.0], f32::MAX, &[]);
        let hit = hit.expect("the ray hit the wall");
        // Its face's normal, -x turned by the yaw, and the face a unit from the origin.
        let (sin, cos) = FRAC_PI_6.sin_cos();
        assert_near(hit.normal, [-cos, 0.0, sin], 1.0e-5);
        assert!((hit.distance - 1.0 / cos).abs() < 1.0e-4, "{hit:?}");
    }

    #[test]
    fn rays_go_through_what_they_are_told_to_and_sensors() {
        let (mut world, ground) = with_ground();
        let block = world
            .add(&fixed(Shape3d::Box(bounds(
                [-1.0, 1.0, -1.0],
                [1.0, 2.0, 1.0],
            ))))
            .unwrap();
        let area = bounds([-1.0, 3.0, -1.0], [1.0, 4.0, 1.0]);
        world
            .add(&sensor_body(BodyKind::Fixed, Shape3d::Box(area)))
            .unwrap();
        let down = |world: &mut World3d, except: &[BodyId]| {
            world.cast_ray([0.0, 5.0, 0.0], [0.0, -1.0, 0.0], f32::MAX, except)
        };
        let hit = down(&mut world, &[]).unwrap();
        assert_eq!(hit.body, block);
        assert!((hit.distance - 3.0).abs() < 1.0e-5, "{hit:?}");
        assert_eq!(down(&mut world, &[block]).unwrap().body, ground);
        assert_eq!(down(&mut world, &[block, ground]), None);
    }

    #[test]
    fn rays_starting_inside_a_body_hit_it_at_once() {
        let mut world = World3d::new();
        let block = world
            .add(&fixed(Shape3d::Box(bounds([-1.0; 3], [1.0; 3]))))
            .unwrap();
        let hit = world.cast_ray([0.0; 3], [1.0, 0.0, 0.0], f32::MAX, &[]);
        assert_eq!(
            hit,
            Some(RayHit3d {
                body: block,
                distance: 0.0,
                point: [0.0; 3],
                normal: [0.0; 3],
            })
        );
        // A cylinder too, whose normal Rapier would make up.
        let cylinder = Shape3d::Cylinder {
            radius: 0.5,
            height: 1.0,
        };
        let upright = world
            .add(&Body3d {
                pose: Pose3d::at([5.0, 0.0, 0.0]),
                ..fixed(cylinder)
            })
            .unwrap();
        let hit = world.cast_ray([5.0, 0.5, 0.0], [1.0, 0.0, 0.0], f32::MAX, &[]);
        let hit = hit.expect("the ray hit the cylinder");
        assert_eq!(hit.body, upright);
        assert_eq!((hit.distance, hit.normal), (0.0, [0.0; 3]));
    }

    #[test]
    fn overlaps_find_what_a_shape_touches() {
        let (mut world, ground) = with_ground();
        let near = world.add(&crate_at([0.0, 0.0, 0.0])).unwrap();
        let far = world
            .add(&fixed(Shape3d::Box(bounds(
                [2.0, 0.0, -0.5],
                [3.0, 1.0, 0.5],
            ))))
            .unwrap();
        let area = bounds([-0.1, 0.6, -0.1], [0.1, 0.7, 0.1]);
        world
            .add(&sensor_body(BodyKind::Fixed, Shape3d::Box(area)))
            .unwrap();
        let ball = |center, radius| Shape3d::Ball { center, radius };
        // Just over the crate's top, at 0.5, and just short of it, not counting the sensor.
        let over = world.overlapping(&ball([0.0, 0.75, 0.0], 0.3), &[]);
        assert_eq!(over.unwrap(), vec![near]);
        let short = world.overlapping(&ball([0.0, 0.75, 0.0], 0.2), &[]);
        assert_eq!(short.unwrap(), vec![]);
        // Everything, the ground too, or all but what it's told.
        let all = world
            .overlapping(&ball([1.0, 0.25, 0.0], 1.1), &[])
            .unwrap();
        assert_eq!(set(all), set(vec![ground, near, far]));
        let all_but = world
            .overlapping(&ball([1.0, 0.25, 0.0], 1.1), &[ground, near])
            .unwrap();
        assert_eq!(all_but, vec![far]);
        // A box.
        let around = Shape3d::Box(bounds([1.5, 0.1, -1.0], [2.5, 2.0, 1.0]));
        assert_eq!(world.overlapping(&around, &[]).unwrap(), vec![far]);
        let flat = Shape3d::Ball {
            center: [0.0; 3],
            radius: 0.0,
        };
        assert!(world.overlapping(&flat, &[]).is_err());
    }

    #[test]
    fn sensors_tell_what_is_in_them_and_stop_nothing() {
        let mut world = ground();
        let area = bounds([-1.0, 1.0, -1.0], [1.0, 2.0, 1.0]);
        let sensor = world
            .add(&sensor_body(BodyKind::Fixed, Shape3d::Box(area)))
            .unwrap();
        assert!(world.is_sensor(sensor));
        let falling = world.add(&crate_at([0.0, 3.0, 0.0])).unwrap();
        assert!(!world.is_sensor(falling));
        // The crate falls through it, in it on the way, and comes to rest on the ground.
        let mut was_in = false;
        for _ in 0..180 {
            world.step(FRAME);
            was_in |= world.inside(sensor) == vec![falling];
        }
        assert!(was_in);
        assert!(world.pose(falling).unwrap().position[1].abs() < 0.02);
        assert_eq!(world.inside(sensor), vec![]);

        // A character walks through one.
        let mut world = ground();
        let area = bounds([0.5, 0.0, -1.0], [1.0, 2.0, 1.0]);
        world
            .add(&sensor_body(BodyKind::Fixed, Shape3d::Box(area)))
            .unwrap();
        let (feet, _) = walk_for(&mut world, [0.0, GAP, 0.0], [0.05, -0.01, 0.0], 60);
        assert_near(feet, [3.0, GAP, 0.0], 1.0e-3);
    }

    #[test]
    fn kinematic_sensors_are_put_at_once() {
        let mut world = World3d::new();
        let ball = Shape3d::Ball {
            center: [0.0; 3],
            radius: 0.5,
        };
        let sensor = world.add(&sensor_body(BodyKind::Kinematic, ball)).unwrap();
        let block = world
            .add(&fixed(Shape3d::Box(bounds(
                [4.8, -1.0, -1.0],
                [6.0, 1.0, 1.0],
            ))))
            .unwrap();
        assert_eq!(world.inside(sensor), vec![]);
        world.set_pose(sensor, Pose3d::at([5.0, 0.0, 0.0]));
        assert_eq!(world.pose(sensor), Some(Pose3d::at([5.0, 0.0, 0.0])));
        assert_eq!(world.inside(sensor), vec![block]);
    }

    #[test]
    fn sensors_that_cant_be_say_why() {
        let mut world = World3d::new();
        let ball = Shape3d::Ball {
            center: [0.0; 3],
            radius: 0.5,
        };
        let falling = world.add(&sensor_body(BodyKind::Dynamic, ball.clone()));
        assert!(falling.unwrap_err().contains("doesn't fall"));
        let triangles = Shape3d::Triangles {
            points: vec![[0.0; 3], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            triangles: vec![[0, 1, 2]],
        };
        let surface = world.add(&sensor_body(BodyKind::Fixed, triangles.clone()));
        assert!(surface.unwrap_err().contains("triangles"));
        // Reshaped, it stays a sensor, and can't become triangles.
        let id = world.add(&sensor_body(BodyKind::Fixed, ball)).unwrap();
        let cube = Shape3d::Box(bounds([-1.0; 3], [1.0; 3]));
        world.set_shape(id, &cube).unwrap();
        assert!(world.is_sensor(id));
        assert!(world.set_shape(id, &triangles).is_err());
    }
}
