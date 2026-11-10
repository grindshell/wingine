//! Physics, by [Rapier](https://rapier.rs): bodies and characters, in 2D and in 3D.
//!
//! In 2D, a [`World`] holds colliders: fixed rectangles placed by [`Affine`](crate::math::Affine)s,
//! as scene nodes are ([`Scene::placement`](crate::scene::Scene::placement)), so a collider can
//! cover a node as it's drawn, turned, stretched and mirrored; tiles' boxes; and bodies, fixed,
//! dynamic or kinematic, which [`World::step`] moves through time. All of a fixed rectangle is
//! solid ([`Solid::Whole`]), or only its edges ([`Solid::Edges`]), which keep what's inside in, as
//! a fence around a field does, or none of it, a sensor ([`Solid::Sensor`]), an area that tells
//! what's in it. [`World::slide`] moves a box among them, as a character walks around a garden
//! seen from above, [`World::walk`] walks a character seen from the side, up steps and slopes and
//! along with what it stands on, pushing what's in its way, and rays, overlaps and sensors tell
//! where things are. Coordinates are whatever the colliders are placed in, such as a scene's, with
//! `y` down.
//!
//! In 3D, a [`World3d`] holds bodies, fixed, dynamic or kinematic, which [`World3d::step`] moves
//! through time, and [`World3d::walk`] walks characters among them, up stairs and slopes, pushing
//! what's in their way. Its coordinates are a 3D view's world's, with `y` up.
//!
//! Rapier is built with its `enhanced-determinism`, and angles are turned to rotations and back
//! with `libm`, so the same colliders, bodies and moves give the same results to the bit on every
//! platform. Its types stay inside the engine.

mod world2d;
mod world3d;

/// The longest step a world takes at once, in seconds: longer ones are taken in parts.
const STEP: f32 = 1.0 / 60.0;

/// How much longer than a [`STEP`] a step may be, as a share of one, and still be taken whole: a
/// 59.94 Hz display's frames, and vsync's jitter, would take twice the work otherwise.
const STEP_SLACK: f32 = 0.05;

/// The most parts a step is taken in, so a long hitch doesn't take longer to catch up with.
const MAX_STEPS: u32 = 16;

/// How many equal parts a world takes a step of `dt` seconds in: as many as there are
/// [`STEP`]s in it, give or take [`STEP_SLACK`], from 1 to [`MAX_STEPS`].
fn parts(dt: f32) -> u32 {
    (dt / STEP - STEP_SLACK).ceil().clamp(1.0, MAX_STEPS as f32) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_split_only_when_well_past_a_sixtieth() {
        for (dt, expected) in [
            (1.0 / 60.0, 1),
            (1.0 / 59.94, 1),
            (1.04 / 60.0, 1),
            (1.06 / 60.0, 2),
            (2.0 / 60.0, 2),
            (2.04 / 60.0, 2),
            (1.0 / 30.0, 2),
            (1.0 / 20.0, 3),
            (0.001, 1),
            (10.0, MAX_STEPS),
        ] {
            assert_eq!(parts(dt), expected, "{dt}");
        }
    }
}

pub use world2d::{Body, Character, ColliderId, Pose, RayHit, Shape, Solid, Walked, World};
pub use world3d::{Body3d, BodyId, Character3d, Pose3d, RayHit3d, Shape3d, Walked3d, World3d};

/// How a body moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BodyKind {
    /// It doesn't: the ground, walls, and what stands still.
    #[default]
    Fixed,
    /// As it's pushed: it falls, tumbles, bounces and rests on what's below it.
    Dynamic,
    /// Where it's put, pushing dynamic bodies out of its way as it goes, as a lift or a door
    /// does, or a character.
    Kinematic,
}

/// What a body's surface is like.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Material {
    /// How heavy each unit of its area, in 2D, or its volume, in 3D, is, which with its shape
    /// makes its mass.
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

/// Fails unless a sensor can be of `kind`: an area doesn't fall.
fn check_sensor_kind(kind: BodyKind) -> Result<(), String> {
    if kind == BodyKind::Dynamic {
        return Err("a sensor doesn't fall: make it fixed or kinematic".into());
    }
    Ok(())
}
