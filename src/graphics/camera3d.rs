use crate::math::{Affine3, Bounds3, add3, cross3, dot3, normalize3, scale3, sub3};

/// A 4x4 matrix, as columns.
pub(crate) type Mat4 = [[f32; 4]; 4];

/// How a [`Camera3d`] projects what it sees onto its view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Projection {
    /// Things farther away look smaller, as to an eye. `fov_y` is the angle the view takes in
    /// from its bottom edge to its top, in radians.
    Perspective { fov_y: f32 },
    /// Things look the same size however far away they are, as in isometric games. `height` is
    /// how many world units the view shows from its bottom edge to its top.
    Orthographic { height: f32 },
}

/// A camera looking into a 3D world, for drawing it with [`Canvas::begin_3d`].
///
/// World coordinates are right-handed with `y` up, as for [`Affine3`]. The camera sees what's
/// between its `near` and `far` planes, measured from its position along where it looks.
///
/// [`Canvas::begin_3d`]: super::Canvas::begin_3d
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera3d {
    /// Where the camera is.
    pub position: [f32; 3],
    /// The point the camera looks at, which shows at the middle of the view.
    pub target: [f32; 3],
    /// Which way is up, which points up the view as near as it can. Looking straight along it,
    /// up the view points toward `-z`, or `+z` looking up.
    pub up: [f32; 3],
    pub projection: Projection,
    /// How near to the camera things can be and still show. Keep it as far out as you can, since
    /// depth is told apart less finely the farther `far` is from it.
    pub near: f32,
    /// How far from the camera things can be and still show.
    pub far: f32,
}

impl Camera3d {
    /// A camera at `position` looking at `target`, with things farther away looking smaller, and
    /// `fov_y` radians of the world showing from the view's bottom edge to its top. Up is `+y`, and
    /// it sees from 0.1 to 1000 world units away.
    pub const fn perspective(position: [f32; 3], target: [f32; 3], fov_y: f32) -> Self {
        Self {
            position,
            target,
            up: [0.0, 1.0, 0.0],
            projection: Projection::Perspective { fov_y },
            near: 0.1,
            far: 1000.0,
        }
    }

    /// A camera at `position` looking at `target`, with things the same size however far away
    /// they are, and `height` world units showing from the view's bottom edge to its top. Up is
    /// `+y`, and it sees from 0.1 to 1000 world units away.
    pub const fn orthographic(position: [f32; 3], target: [f32; 3], height: f32) -> Self {
        Self {
            position,
            target,
            up: [0.0, 1.0, 0.0],
            projection: Projection::Orthographic { height },
            near: 0.1,
            far: 1000.0,
        }
    }

    /// What makes the camera one that can't see, if anything: a field of view not between 0 and
    /// pi, a height not above 0, or `near` not above 0 or `far` not above it.
    pub(crate) fn problem(&self) -> Option<String> {
        match self.projection {
            Projection::Perspective { fov_y } if !(fov_y > 0.0 && fov_y < std::f32::consts::PI) => {
                return Some(format!(
                    "a camera's fov must be between 0 and pi, not {fov_y}"
                ));
            }
            Projection::Orthographic { height } if !(height > 0.0 && height.is_finite()) => {
                return Some(format!("a camera's height must be above 0, not {height}"));
            }
            _ => {}
        }
        if !(self.near > 0.0 && self.far > self.near && self.far.is_finite()) {
            return Some(format!(
                "a camera's near must be above 0 and its far above that, not {} and {}",
                self.near, self.far
            ));
        }
        None
    }

    /// Where `point` in the world shows on a view `size` pixels in size, from the view's top-left
    /// corner. `None` if it's nearer than the near plane or farther than the far one, such as
    /// behind the camera. Points to the sides of the view are outside it, but still placed.
    pub fn project(&self, point: [f32; 3], size: [f32; 2]) -> Option<[f32; 2]> {
        let clip = transform(&self.view_projection(aspect(size)), point);
        if clip[3] <= 0.0 {
            return None;
        }
        let [x, y, z] = [0, 1, 2].map(|i| clip[i] / clip[3]);
        (0.0..=1.0)
            .contains(&z)
            .then(|| [(x + 1.0) / 2.0 * size[0], (1.0 - y) / 2.0 * size[1]])
    }

    /// The ray through `point` on a view `size` pixels in size, from the view's top-left corner:
    /// everything that shows there lies along it. It starts on the near plane, so points along it
    /// are in front of the camera.
    pub fn ray(&self, point: [f32; 2], size: [f32; 2]) -> Ray {
        let [x, y] = [
            point[0] / size[0].max(f32::MIN_POSITIVE) * 2.0 - 1.0,
            1.0 - point[1] / size[1].max(f32::MIN_POSITIVE) * 2.0,
        ];
        let Basis { right, up, forward } = self.basis();
        let aspect = aspect(size);
        match self.projection {
            Projection::Perspective { fov_y } => {
                let half_height = (fov_y / 2.0).tan();
                let through = add3(
                    add3(
                        scale3(right, x * half_height * aspect),
                        scale3(up, y * half_height),
                    ),
                    forward,
                );
                Ray {
                    origin: add3(self.position, scale3(through, self.near)),
                    direction: normalize3(through).unwrap_or(forward),
                }
            }
            Projection::Orthographic { height } => {
                let across = add3(
                    scale3(right, x * height / 2.0 * aspect),
                    scale3(up, y * height / 2.0),
                );
                Ray {
                    origin: add3(add3(self.position, across), scale3(forward, self.near)),
                    direction: forward,
                }
            }
        }
    }

    /// The ways a billboard's right and its front point, level with the ground: its front toward
    /// the camera across the ground, however the camera's rolled, or looking straight down or up,
    /// toward the view's bottom edge.
    pub(crate) fn billboard_axes(&self) -> ([f32; 3], [f32; 3]) {
        let Basis { up, forward, .. } = self.basis();
        let facing = normalize3([-forward[0], 0.0, -forward[2]])
            .or_else(|| normalize3([-up[0], 0.0, -up[2]]))
            .unwrap_or([0.0, 0.0, 1.0]);
        // Right, up and front, as `x`, `y` and `z` are.
        (cross3([0.0, 1.0, 0.0], facing), facing)
    }

    /// Maps world coordinates to clip space, for a view `aspect` times as wide as it's tall.
    pub(crate) fn view_projection(&self, aspect: f32) -> Mat4 {
        mul(&self.projection_matrix(aspect), &from_affine(&self.view()))
    }

    /// Maps world coordinates to the camera's, where it looks along `-z` with `+y` up the view.
    fn view(&self) -> Affine3 {
        let Basis { right, up, forward } = self.basis();
        let position = self.position;
        Affine3 {
            x_axis: [right[0], up[0], -forward[0]],
            y_axis: [right[1], up[1], -forward[1]],
            z_axis: [right[2], up[2], -forward[2]],
            translation: [
                -dot3(right, position),
                -dot3(up, position),
                dot3(forward, position),
            ],
        }
    }

    /// Which ways the view's right, its up and where it looks point in the world.
    /// The way it looks, of length 1.
    pub(crate) fn forward(&self) -> [f32; 3] {
        self.basis().forward
    }

    fn basis(&self) -> Basis {
        let forward = normalize3(sub3(self.target, self.position)).unwrap_or([0.0, 0.0, -1.0]);
        // Looking along `up`, up the view is toward `-z` looking down, or `+z` looking up.
        let fallback = if forward[1] > 0.0 {
            [0.0, 0.0, 1.0]
        } else {
            [0.0, 0.0, -1.0]
        };
        let right = [self.up, fallback, [1.0, 0.0, 0.0]]
            .into_iter()
            .find_map(|up| {
                let right = cross3(forward, up);
                // Unless `up` is too nearly the way it looks to tell which way is right.
                (dot3(right, right) > 1e-12)
                    .then(|| normalize3(right))
                    .flatten()
            })
            .unwrap_or([1.0, 0.0, 0.0]);
        Basis {
            right,
            up: cross3(right, forward),
            forward,
        }
    }

    /// Maps the camera's coordinates to clip space, with depth from 0 at the near plane to 1 at
    /// the far one.
    fn projection_matrix(&self, aspect: f32) -> Mat4 {
        let [near, far] = [self.near, self.far];
        let depth = 1.0 / (near - far);
        match self.projection {
            Projection::Perspective { fov_y } => {
                let f = 1.0 / (fov_y / 2.0).tan();
                [
                    [f / aspect, 0.0, 0.0, 0.0],
                    [0.0, f, 0.0, 0.0],
                    [0.0, 0.0, far * depth, -1.0],
                    [0.0, 0.0, near * far * depth, 0.0],
                ]
            }
            Projection::Orthographic { height } => [
                [2.0 / (height * aspect), 0.0, 0.0, 0.0],
                [0.0, 2.0 / height, 0.0, 0.0],
                [0.0, 0.0, depth, 0.0],
                [0.0, 0.0, near * depth, 1.0],
            ],
        }
    }
}

/// A camera's directions in the world, each of length 1.
struct Basis {
    right: [f32; 3],
    up: [f32; 3],
    forward: [f32; 3],
}

/// How many times wider than tall a view of `size` is, or 1 for a view with no size.
fn aspect([width, height]: [f32; 2]) -> f32 {
    let aspect = width / height;
    if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        1.0
    }
}

/// A line going one way from a point, such as what's under the cursor in a 3D view: see
/// [`Camera3d::ray`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ray {
    pub origin: [f32; 3],
    /// The way it goes, of length 1.
    pub direction: [f32; 3],
}

impl Ray {
    /// The point `distance` along the ray.
    pub fn at(&self, distance: f32) -> [f32; 3] {
        add3(self.origin, scale3(self.direction, distance))
    }

    /// Where the ray meets the plane through `point` facing `normal`, if it does ahead of its
    /// origin: for the ground at height 0, `hit_plane([0.0; 3], [0.0, 1.0, 0.0])`.
    pub fn hit_plane(&self, point: [f32; 3], normal: [f32; 3]) -> Option<[f32; 3]> {
        let toward = dot3(self.direction, normal);
        if toward.abs() < 1e-12 {
            return None;
        }
        let distance = dot3(sub3(point, self.origin), normal) / toward;
        (distance >= 0.0 && distance.is_finite()).then(|| self.at(distance))
    }

    /// How far along the ray it enters `bounds`: 0 if it starts in them, or `None` if it misses
    /// them or they're behind its origin, or anything is NaN. Picking what's under the cursor in a
    /// 3D view tries each thing's bounds and takes the nearest hit.
    pub fn hit_box(&self, bounds: &Bounds3) -> Option<f32> {
        let values = [self.origin, self.direction, bounds.min, bounds.max];
        if values.as_flattened().iter().any(|v| v.is_nan()) {
            return None;
        }
        let mut near = 0.0f32;
        let mut far = f32::INFINITY;
        for axis in 0..3 {
            let [origin, direction] = [self.origin[axis], self.direction[axis]];
            let [min, max] = [bounds.min[axis], bounds.max[axis]];
            if direction == 0.0 {
                // Going along these sides, it's between them throughout, or never.
                if origin < min || origin > max {
                    return None;
                }
                continue;
            }
            let [a, b] = [(min - origin) / direction, (max - origin) / direction];
            // From infinitely far, to infinite sides.
            if a.is_nan() || b.is_nan() {
                return None;
            }
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
        // Infinitely far from an origin infinitely far, which it never reaches.
        near.is_finite().then_some(near)
    }
}

/// `a` times `b`: `b`, then `a`.
pub(crate) fn mul(a: &Mat4, b: &Mat4) -> Mat4 {
    b.map(|column| transform4(a, column))
}

/// `m` times the column `v`.
fn transform4(m: &Mat4, v: [f32; 4]) -> [f32; 4] {
    [0, 1, 2, 3].map(|row| (0..4).map(|i| m[i][row] * v[i]).sum())
}

/// Where `m` takes the point `p`, before dividing by `w`.
fn transform(m: &Mat4, [x, y, z]: [f32; 3]) -> [f32; 4] {
    transform4(m, [x, y, z, 1.0])
}

fn from_affine(a: &Affine3) -> Mat4 {
    let [x, y, z, t] = [a.x_axis, a.y_axis, a.z_axis, a.translation];
    [
        [x[0], x[1], x[2], 0.0],
        [y[0], y[1], y[2], 0.0],
        [z[0], z[1], z[2], 0.0],
        [t[0], t[1], t[2], 1.0],
    ]
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use super::*;

    fn assert_near<const N: usize>(actual: [f32; N], expected: [f32; N]) {
        assert!(
            (0..N).all(|i| (actual[i] - expected[i]).abs() < 1e-3),
            "{actual:?} is not {expected:?}"
        );
    }

    fn ndc(camera: &Camera3d, aspect: f32, point: [f32; 3]) -> [f32; 3] {
        let clip = transform(&camera.view_projection(aspect), point);
        [0, 1, 2].map(|i| clip[i] / clip[3])
    }

    #[test]
    fn perspective_maps_the_near_and_far_planes_to_depths_0_and_1() {
        let camera = Camera3d {
            near: 1.0,
            far: 100.0,
            ..Camera3d::perspective([0.0, 0.0, 10.0], [0.0; 3], FRAC_PI_2)
        };
        assert_near(ndc(&camera, 1.0, [0.0, 0.0, 9.0]), [0.0, 0.0, 0.0]);
        assert_near(ndc(&camera, 1.0, [0.0, 0.0, -90.0]), [0.0, 0.0, 1.0]);
        // A quarter turn takes in 45 degrees either side, so 5 units up at 5 units away is the
        // top edge, and twice as wide a view takes in twice as much across.
        assert_near(
            ndc(&camera, 1.0, [0.0, 5.0, 5.0])[..2].try_into().unwrap(),
            [0.0, 1.0],
        );
        assert_near(
            ndc(&camera, 2.0, [10.0, 0.0, 5.0])[..2].try_into().unwrap(),
            [1.0, 0.0],
        );
    }

    #[test]
    fn orthographic_keeps_sizes_at_any_distance() {
        let camera = Camera3d {
            near: 1.0,
            far: 11.0,
            ..Camera3d::orthographic([0.0, 0.0, 10.0], [0.0; 3], 4.0)
        };
        assert_near(ndc(&camera, 2.0, [4.0, 2.0, 9.0]), [1.0, 1.0, 0.0]);
        assert_near(ndc(&camera, 2.0, [-4.0, -2.0, -1.0]), [-1.0, -1.0, 1.0]);
    }

    #[test]
    fn projecting_places_points_from_the_views_top_left() {
        let camera = Camera3d::perspective([0.0, 0.0, 10.0], [0.0; 3], FRAC_PI_2);
        let size = [200.0, 100.0];
        assert_near(camera.project([0.0; 3], size).unwrap(), [100.0, 50.0]);
        // Up the view is up the canvas, which counts down from its top.
        assert_near(
            camera.project([0.0, 10.0, 0.0], size).unwrap(),
            [100.0, 0.0],
        );
        assert_eq!(camera.project([0.0, 0.0, 20.0], size), None, "behind");
        assert_eq!(camera.project([0.0, 0.0, -2000.0], size), None, "too far");
    }

    #[test]
    fn rays_run_through_what_shows_under_a_point() {
        let size = [320.0, 180.0];
        for camera in [
            Camera3d::perspective([3.0, 4.0, 5.0], [0.0, 1.0, 0.0], 1.0),
            Camera3d::orthographic([3.0, 4.0, 5.0], [0.0, 1.0, 0.0], 6.0),
        ] {
            let middle = camera.ray([160.0, 90.0], size);
            let forward = normalize3(sub3(camera.target, camera.position)).unwrap();
            assert_near(middle.direction, forward);

            for point in [[0.0, 0.0], [40.0, 170.0], [300.0, 10.0]] {
                let ray = camera.ray(point, size);
                for distance in [0.01, 1.0, 50.0] {
                    assert_near(camera.project(ray.at(distance), size).unwrap(), point);
                }
            }
        }
    }

    #[test]
    fn rays_hit_the_ground_ahead_of_them() {
        let camera = Camera3d::perspective([0.0, 10.0, 10.0], [0.0; 3], 1.0);
        let ray = camera.ray([50.0, 50.0], [100.0, 100.0]);
        assert_near(ray.hit_plane([0.0; 3], [0.0, 1.0, 0.0]).unwrap(), [0.0; 3]);
        let up = Ray {
            origin: [0.0, 1.0, 0.0],
            direction: [0.0, 1.0, 0.0],
        };
        assert_eq!(up.hit_plane([0.0; 3], [0.0, 1.0, 0.0]), None);
    }

    #[test]
    fn rays_hit_boxes_where_they_enter() {
        let crate_ = Bounds3 {
            min: [-1.0, 0.0, -1.0],
            max: [1.0, 2.0, 1.0],
        };
        let ray = |origin: [f32; 3], direction: [f32; 3]| Ray {
            origin,
            direction: normalize3(direction).unwrap(),
        };
        let toward = ray([0.0, 1.0, 10.0], [0.0, 0.0, -1.0]);
        assert_eq!(toward.hit_box(&crate_), Some(9.0));
        assert_eq!(
            ray([0.0, 1.0, 10.0], [0.0, 0.0, 1.0]).hit_box(&crate_),
            None,
            "away"
        );
        assert_eq!(
            ray([3.0, 1.0, 10.0], [0.0, 0.0, -1.0]).hit_box(&crate_),
            None,
            "beside"
        );
        assert_eq!(
            ray([0.0, 1.0, 0.0], [1.0, 0.0, 0.0]).hit_box(&crate_),
            Some(0.0),
            "inside"
        );
        // From above at a slant, through the top.
        let slant = ray([0.0, 4.0, 2.0], [0.0, -1.0, -1.0]);
        let distance = slant.hit_box(&crate_).unwrap();
        assert_near(slant.at(distance), [0.0, 2.0, 0.0]);
        // Grazing past a corner.
        assert_eq!(
            ray([0.0, 5.0, 0.0], [1.0, -1.0, 0.0]).hit_box(&crate_),
            None
        );
    }

    #[test]
    fn rays_of_nan_hit_nothing() {
        let crate_ = Bounds3 {
            min: [-1.0, 0.0, -1.0],
            max: [1.0, 2.0, 1.0],
        };
        let nan = f32::NAN;
        let rays = [
            ([nan, 1.0, 0.0], [0.0, 0.0, -1.0]),
            ([0.0, 1.0, 10.0], [0.0, nan, -1.0]),
            ([0.0, 1.0, 10.0], [nan, nan, nan]),
            // Infinitely far along an axis it goes along.
            ([f32::INFINITY, 1.0, 0.0], [-1.0, 0.0, 0.0]),
        ];
        for (origin, direction) in rays {
            let ray = Ray { origin, direction };
            assert_eq!(ray.hit_box(&crate_), None, "{ray:?}");
        }
        let lost = Bounds3 {
            min: [nan; 3],
            max: [1.0; 3],
        };
        let toward = Ray {
            origin: [0.0, 1.0, 10.0],
            direction: [0.0, 0.0, -1.0],
        };
        assert_eq!(toward.hit_box(&lost), None);
    }

    #[test]
    fn billboards_face_the_camera_across_the_ground() {
        let from_front = Camera3d::perspective([0.0, 5.0, 10.0], [0.0; 3], 1.0);
        let (right, facing) = from_front.billboard_axes();
        assert_near(right, [1.0, 0.0, 0.0]);
        assert_near(facing, [0.0, 0.0, 1.0]);

        let from_right = Camera3d::perspective([10.0, 2.0, 0.0], [0.0; 3], 1.0);
        let (right, facing) = from_right.billboard_axes();
        assert_near(right, [0.0, 0.0, -1.0]);
        assert_near(facing, [1.0, 0.0, 0.0]);
        // Looking straight down, they face down the view, where its bottom edge is.
        let down = Camera3d::orthographic([0.0, 10.0, 0.0], [0.0; 3], 2.0);
        assert_near(down.billboard_axes().1, [0.0, 0.0, 1.0]);
    }

    #[test]
    fn billboards_face_the_camera_however_its_rolled() {
        let ups = [
            [0.0, 1.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, -1.0, 0.0],
            [-1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            // Nearly back the way it looks, so up the view is down.
            [0.0, 0.0, 1.0],
        ];
        for up in ups {
            let camera = Camera3d {
                up,
                ..Camera3d::orthographic([0.0, 1.0, 5.0], [0.0, 0.5, 0.0], 2.0)
            };
            let (right, facing) = camera.billboard_axes();
            assert_near(right, [1.0, 0.0, 0.0]);
            assert_near(facing, [0.0, 0.0, 1.0]);
        }
        // Looking straight down, toward the view's bottom edge, which rolling turns.
        let down = Camera3d {
            up: [1.0, 0.0, 0.0],
            ..Camera3d::orthographic([0.0, 10.0, 0.0], [0.0; 3], 2.0)
        };
        let (right, facing) = down.billboard_axes();
        assert_near(right, [0.0, 0.0, 1.0]);
        assert_near(facing, [-1.0, 0.0, 0.0]);
    }

    #[test]
    fn looking_straight_down_puts_negative_z_up_the_view() {
        let camera = Camera3d::orthographic([0.0, 10.0, 0.0], [0.0; 3], 2.0);
        let size = [100.0, 100.0];
        // Up the view, toward the canvas's top.
        let north = camera.project([0.0, 0.0, -1.0], size).unwrap();
        assert_near(north, [50.0, 0.0]);
        let east = camera.project([1.0, 0.0, 0.0], size).unwrap();
        assert_near(east, [100.0, 50.0]);

        let up = Camera3d::orthographic([0.0, -10.0, 0.0], [0.0; 3], 2.0);
        assert!(up.project([0.0; 3], size).is_some());
    }
}
