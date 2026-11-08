//! Physics, by [Rapier](https://rapier.rs): collision in 2D, and bodies and characters in 3D.
//!
//! In 2D, a [`World`] holds fixed colliders, and boxes moved among them stop at what's solid and
//! slide along it, as characters walk. Each collider is a rectangle placed by an [`Affine`], as a
//! scene node's is ([`Scene::placement`](crate::scene::Scene::placement)), so a collider can cover
//! a node as it's drawn: turned, stretched and mirrored. All of a collider is solid
//! ([`Solid::Whole`]), or only its edges ([`Solid::Edges`]), which keep what's inside in, as a
//! fence around a field does, or none of it, a sensor ([`Solid::Sensor`]), an area that tells
//! what's in it. [`World::slide`] moves a box among them, and rays, overlaps and sensors tell
//! where they are. Coordinates are whatever the colliders are placed in, such as a scene's, with
//! `y` down.
//!
//! In 3D, a [`World3d`] holds bodies, fixed, dynamic or kinematic, which [`World3d::step`] moves
//! through time, and [`World3d::walk`] walks characters among them, up stairs and slopes, pushing
//! what's in their way. Its coordinates are a 3D view's world's, with `y` up.
//!
//! Rapier is built with its `enhanced-determinism`, so the same colliders, bodies and moves give
//! the same results to the bit on every platform. Its types stay inside the engine.

mod world3d;

pub use world3d::{
    Body3d, BodyId, BodyKind, Character, Material, Pose3d, RayHit3d, Shape3d, Walked, World3d,
};

use rapier2d::{
    parry::{
        query::ShapeCastOptions,
        shape::{Ball, Cuboid, Shape},
    },
    prelude::{
        Collider, ColliderBuilder, ColliderHandle, PhysicsWorld, Pose, QueryFilter, Ray,
        SharedShape,
    },
};

use crate::math::{Affine, Rect};

/// Rapier's vectors, which are glam's.
type Vector = rapier2d::prelude::Vector;

/// The gap a sliding box keeps from what it stops at, in the world's units, so that a box resting
/// against something isn't touching it, and slides along it without hitting it again.
const GAP: f32 = 0.01;

/// The most things a box slides off in one move. Each takes away the part of the move going into
/// it, so in 2D, two leave nothing; more are for corners.
const MAX_HITS: usize = 4;

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

/// A collider in a [`World`].
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

/// Fixed colliders, and boxes slid among them: see the [module docs](self).
pub struct World {
    rapier: PhysicsWorld,
    /// Whether colliders have changed since Rapier last caught up with them, which it must
    /// before what it's asked about sees them.
    stale: bool,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    /// A world with no colliders.
    pub fn new() -> Self {
        Self {
            rapier: PhysicsWorld::new(),
            stale: false,
        }
    }

    /// Adds a fixed collider of `rect` placed by `transform`, all of it solid, only its edges, or
    /// none of it, a sensor. A rectangle squashed flat, with no area, is nowhere until
    /// [`World::set`] gives it some.
    pub fn add(&mut self, transform: Affine, rect: Rect, solid: Solid) -> ColliderId {
        // A stand-in shape until `set` shapes it.
        let collider = ColliderBuilder::new(SharedShape::ball(0.0)).enabled(false);
        let id = ColliderId(self.rapier.insert_collider(collider, None));
        self.set(id, transform, rect, solid);
        id
    }

    /// Moves and reshapes the collider `id`, as [`World::add`] places one. Does nothing to one
    /// that's been removed.
    pub fn set(&mut self, id: ColliderId, transform: Affine, rect: Rect, solid: Solid) {
        let Some(collider) = self.rapier.colliders.get_mut(id.0) else {
            return;
        };
        match shape(transform, rect, solid) {
            Some(shape) => {
                collider.set_shape(shape);
                collider.set_enabled(true);
            }
            None => collider.set_enabled(false),
        }
        collider.set_sensor(solid == Solid::Sensor);
        self.stale = true;
    }

    /// Whether the collider `id` is a sensor: see [`Solid::Sensor`].
    pub fn is_sensor(&self, id: ColliderId) -> bool {
        self.rapier
            .colliders
            .get(id.0)
            .is_some_and(Collider::is_sensor)
    }

    /// Brings Rapier up to date with the colliders changed since it last was, which it must be
    /// before what it's asked about sees them.
    fn catch_up(&mut self) {
        if self.stale {
            self.rapier.detect_collisions(&(), &());
            self.stale = false;
        }
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
        self.catch_up();
        let direction = Vector::from(direction);
        let ray = Ray::new(Vector::from(origin), direction);
        let excepted = |handle: ColliderHandle, _: &Collider| !except.contains(&ColliderId(handle));
        let filter = QueryFilter::default()
            .exclude_sensors()
            .predicate(&excepted);
        let queries = self.rapier.query_pipeline_with_filter(filter);
        let (handle, hit) = queries.cast_ray_and_get_normal(&ray, max_distance, true)?;
        // Rapier finds where a ray hits a polygon by iterating, as it does a box, so its normal
        // is one of the polygon's sides' within a hair.
        let normal = true_normal(hit.normal, queries.colliders[handle].shape());
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
        self.catch_up();
        self.touching(Pose::IDENTITY, &*shape, except)
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
        self.catch_up();
        let at = Pose::from_translation(Vector::from(center));
        self.touching(at, &Ball::new(radius), except)
    }

    /// Returns the colliders that overlap or touch the collider `id`, a sensor's or any other's,
    /// other than sensors, in no particular order: for a sensor, what's in it.
    pub fn inside(&mut self, id: ColliderId) -> Vec<ColliderId> {
        self.catch_up();
        let Some(collider) = self.rapier.colliders.get(id.0) else {
            return Vec::new();
        };
        if !collider.is_enabled() {
            return Vec::new();
        }
        self.touching(*collider.position(), collider.shape(), &[id])
    }

    /// The colliders `shape` at `pose` touches, other than those in `except` and sensors.
    fn touching(&self, pose: Pose, shape: &dyn Shape, except: &[ColliderId]) -> Vec<ColliderId> {
        let excepted = |handle: ColliderHandle, _: &Collider| !except.contains(&ColliderId(handle));
        let filter = QueryFilter::default()
            .exclude_sensors()
            .predicate(&excepted);
        let queries = self.rapier.query_pipeline_with_filter(filter);
        queries
            .intersect_shape(pose, shape)
            .map(|(handle, _)| ColliderId(handle))
            .collect()
    }

    /// Removes the collider `id`, if it hasn't been already.
    pub fn remove(&mut self, id: ColliderId) {
        if self.rapier.remove_collider(id.0).is_some() {
            self.stale = true;
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
    /// hit as far as `motion` goes that way. It stops [`GAP`] short of what it hits. A box that
    /// starts inside something solid moves only out of it, or along it. The collider `except`,
    /// if given, isn't in its way, as a character's own isn't, and nor are sensors.
    ///
    /// Rapier's own character controller does this for characters that walk on floors, but its
    /// casts keep their gap by rounding the box, which turns what it slides along a little, so a
    /// box pushed straight at a wall would creep along it. This casts the box as it is.
    pub fn slide(&mut self, rect: Rect, motion: [f32; 2], except: Option<ColliderId>) -> [f32; 2] {
        self.catch_up();
        let half = Vector::new(rect.w / 2.0, rect.h / 2.0);
        let shape = Cuboid::new(half);
        let start = Vector::new(rect.x, rect.y) + half;
        let filter = match except {
            Some(id) => QueryFilter::default().exclude_collider(id.0),
            None => QueryFilter::default(),
        }
        .exclude_sensors();
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
            let at = Pose::from_translation(start + moved);
            let options = ShapeCastOptions {
                max_time_of_impact: distance,
                ..options
            };
            let Some((handle, hit)) = queries.cast_shape(&at, direction, &shape, options) else {
                moved += left;
                break;
            };
            // Out of what it hit, toward the box.
            let normal = true_normal(hit.normal1, queries.colliders[handle].shape());
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

/// The normal of a side of `hit`, or of the box that hit it, which `normal` is within a hair of, or
/// `normal` itself if none is. Rapier works out where a box hits something by iterating (GJK),
/// which finds the normal there only to within about a thousandth of a radian, and a box pushed at
/// a wall would creep along it by that much each time.
fn true_normal(normal: Vector, hit: &dyn Shape) -> Vector {
    const HAIR: f32 = 2.0e-3;
    let near = |side: &Vector| normal.dot(*side) > 0.0 && normal.perp_dot(*side).abs() < HAIR;
    // What a box hits is solid inside its sides or along them, from either side.
    let theirs: Vec<Vector> = if let Some(polygon) = hit.as_convex_polygon() {
        polygon.normals().to_vec()
    } else if let Some(polyline) = hit.as_polyline() {
        let normals = polyline.segments().filter_map(|segment| segment.normal());
        normals.flat_map(|normal| [normal, -normal]).collect()
    } else {
        Vec::new()
    };
    let box_sides = [Vector::X, Vector::NEG_X, Vector::Y, Vector::NEG_Y];
    theirs
        .iter()
        .chain(&box_sides)
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

#[cfg(test)]
mod tests {
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
        let off = (actual[0] - expected[0])
            .abs()
            .max((actual[1] - expected[1]).abs());
        assert!(off < 0.02, "{actual:?}, not {expected:?}");
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
        let id = world.add(flat, rect, Solid::Whole);
        assert_eq!(
            world.slide(small_box(0.0, 0.0), [30.0, 0.0], None),
            [30.0, 0.0]
        );
        world.set(id, Affine::IDENTITY, rect, Solid::Whole);
        assert_near(
            world.slide(small_box(0.0, 0.0), [30.0, 0.0], None),
            [8.0 - GAP, 0.0],
        );
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
}
