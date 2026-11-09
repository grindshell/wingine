use std::ops::Mul;

/// An axis-aligned rectangle. `(x, y)` is the top-left corner; `y` grows downward.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub const fn right(&self) -> f32 {
        self.x + self.w
    }

    pub const fn bottom(&self) -> f32 {
        self.y + self.h
    }

    /// Whether the two rectangles share any area. Rectangles that only touch along an edge don't
    /// overlap.
    pub const fn overlaps(&self, other: &Rect) -> bool {
        self.x < other.right()
            && other.x < self.right()
            && self.y < other.bottom()
            && other.y < self.bottom()
    }

    /// Whether `point` is inside, counting the top and left edges but not the bottom and right,
    /// so a point on the edge between two rectangles side by side is in exactly one.
    pub const fn contains(&self, [x, y]: [f32; 2]) -> bool {
        self.x <= x && x < self.right() && self.y <= y && y < self.bottom()
    }
}

/// A 2D affine transform: rotation, scaling and mirroring, then a translation. It moves a point
/// `[x, y]` to `x_axis * x + y_axis * y + translation`.
///
/// Composing transforms with `*` applies the right-hand one first: `parent * child` places what
/// `child` places, then moves it as `parent` does, as a node in a tree is placed within its
/// parent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    /// Where a step of 1 along `x` goes.
    pub x_axis: [f32; 2],
    /// Where a step of 1 along `y` goes.
    pub y_axis: [f32; 2],
    /// Where the origin goes.
    pub translation: [f32; 2],
}

impl Affine {
    pub const IDENTITY: Self = Self::from_translation([0.0, 0.0]);

    pub const fn from_translation(translation: [f32; 2]) -> Self {
        Self {
            x_axis: [1.0, 0.0],
            y_axis: [0.0, 1.0],
            translation,
        }
    }

    /// Scales by `scale` along each axis (negative to mirror), rotates by `rotation` radians,
    /// then moves by `translation`. Positive angles turn clockwise, since `y` grows downward.
    pub fn from_parts(translation: [f32; 2], rotation: f32, scale: [f32; 2]) -> Self {
        // Most things aren't rotated, and the sine and cosine are calls into the math library.
        let (sin, cos) = if rotation == 0.0 {
            (0.0, 1.0)
        } else {
            rotation.sin_cos()
        };
        Self::turned(translation, sin, cos, scale)
    }

    /// [`Affine::from_parts`], with the sine and cosine from `libm`, which gives the same bits on
    /// every platform, where the platform's C library's may differ in the last: for what physics
    /// is placed by, such as scene nodes, so it plays out the same everywhere. It's slower, so
    /// what's drawn many times a frame uses [`Affine::from_parts`].
    pub fn from_parts_exact(translation: [f32; 2], rotation: f32, scale: [f32; 2]) -> Self {
        let (sin, cos) = if rotation == 0.0 {
            (0.0, 1.0)
        } else {
            libm::sincosf(rotation)
        };
        Self::turned(translation, sin, cos, scale)
    }

    fn turned(translation: [f32; 2], sin: f32, cos: f32, scale: [f32; 2]) -> Self {
        Self {
            x_axis: [cos * scale[0], sin * scale[0]],
            y_axis: [-sin * scale[1], cos * scale[1]],
            translation,
        }
    }

    /// Where `point` goes.
    pub fn apply(&self, [x, y]: [f32; 2]) -> [f32; 2] {
        let [tx, ty] = self.translation;
        [
            self.x_axis[0] * x + self.y_axis[0] * y + tx,
            self.x_axis[1] * x + self.y_axis[1] * y + ty,
        ]
    }

    /// Where the step `vector` goes, which the translation doesn't move.
    fn apply_vector(&self, [x, y]: [f32; 2]) -> [f32; 2] {
        [
            self.x_axis[0] * x + self.y_axis[0] * y,
            self.x_axis[1] * x + self.y_axis[1] * y,
        ]
    }

    /// The transform that undoes this one, unless it squashes everything onto a line or a point
    /// (such as with a scale of 0), which nothing undoes.
    pub fn inverse(&self) -> Option<Self> {
        let [a, b] = self.x_axis;
        let [c, d] = self.y_axis;
        let determinant = a * d - b * c;
        if determinant == 0.0 || !determinant.is_finite() {
            return None;
        }
        let inverse = Self {
            x_axis: [d / determinant, -b / determinant],
            y_axis: [-c / determinant, a / determinant],
            translation: [0.0, 0.0],
        };
        let [x, y] = inverse.apply_vector(self.translation);
        Some(Self {
            translation: [-x, -y],
            ..inverse
        })
    }

    /// The corners of `rect` once transformed, clockwise from where its top-left corner goes (or
    /// counter-clockwise, if this mirrors).
    pub fn corners(&self, rect: Rect) -> [[f32; 2]; 4] {
        [
            [rect.x, rect.y],
            [rect.right(), rect.y],
            [rect.right(), rect.bottom()],
            [rect.x, rect.bottom()],
        ]
        .map(|corner| self.apply(corner))
    }

    /// The smallest axis-aligned rectangle holding `rect` once transformed.
    pub fn bounds(&self, rect: Rect) -> Rect {
        let corners = self.corners(rect);
        let [mut min, mut max] = [corners[0], corners[0]];
        for [x, y] in &corners[1..] {
            min = [min[0].min(*x), min[1].min(*y)];
            max = [max[0].max(*x), max[1].max(*y)];
        }
        Rect::new(min[0], min[1], max[0] - min[0], max[1] - min[1])
    }
}

impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mul for Affine {
    type Output = Self;

    /// `rhs`, then `self`.
    fn mul(self, rhs: Self) -> Self {
        Self {
            x_axis: self.apply_vector(rhs.x_axis),
            y_axis: self.apply_vector(rhs.y_axis),
            translation: self.apply(rhs.translation),
        }
    }
}

/// A 3D affine transform: rotation, scaling and mirroring, then a translation. It moves a point
/// `[x, y, z]` to `x_axis * x + y_axis * y + z_axis * z + translation`.
///
/// 3D coordinates are right-handed with `y` up: with `x` pointing right and `y` up, `z` points
/// toward the viewer. That's glTF's convention, unlike 2D drawing's, where `y` points down.
///
/// Composing transforms with `*` applies the right-hand one first, as with [`Affine`]: `parent *
/// child` places what `child` places, then moves it as `parent` does.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine3 {
    /// Where a step of 1 along `x` goes.
    pub x_axis: [f32; 3],
    /// Where a step of 1 along `y` goes.
    pub y_axis: [f32; 3],
    /// Where a step of 1 along `z` goes.
    pub z_axis: [f32; 3],
    /// Where the origin goes.
    pub translation: [f32; 3],
}

impl Affine3 {
    pub const IDENTITY: Self = Self::from_translation([0.0, 0.0, 0.0]);

    pub const fn from_translation(translation: [f32; 3]) -> Self {
        Self {
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, 1.0, 0.0],
            z_axis: [0.0, 0.0, 1.0],
            translation,
        }
    }

    /// Scales by `scale` along each axis, negative to mirror.
    pub const fn from_scale(scale: [f32; 3]) -> Self {
        Self {
            x_axis: [scale[0], 0.0, 0.0],
            y_axis: [0.0, scale[1], 0.0],
            z_axis: [0.0, 0.0, scale[2]],
            translation: [0.0, 0.0, 0.0],
        }
    }

    /// Turns by `angle` radians about the `x` axis, counter-clockwise seen from `+x`: `y` turns
    /// toward `z`.
    pub fn from_rotation_x(angle: f32) -> Self {
        let (sin, cos) = sin_cos(angle);
        Self {
            x_axis: [1.0, 0.0, 0.0],
            y_axis: [0.0, cos, sin],
            z_axis: [0.0, -sin, cos],
            translation: [0.0, 0.0, 0.0],
        }
    }

    /// Turns by `angle` radians about the `y` axis, counter-clockwise seen from above: `z` turns
    /// toward `x`.
    pub fn from_rotation_y(angle: f32) -> Self {
        let (sin, cos) = sin_cos(angle);
        Self {
            x_axis: [cos, 0.0, -sin],
            y_axis: [0.0, 1.0, 0.0],
            z_axis: [sin, 0.0, cos],
            translation: [0.0, 0.0, 0.0],
        }
    }

    /// Turns by `angle` radians about the `z` axis, counter-clockwise seen from `+z`: `x` turns
    /// toward `y`.
    pub fn from_rotation_z(angle: f32) -> Self {
        let (sin, cos) = sin_cos(angle);
        Self {
            x_axis: [cos, sin, 0.0],
            y_axis: [-sin, cos, 0.0],
            z_axis: [0.0, 0.0, 1.0],
            translation: [0.0, 0.0, 0.0],
        }
    }

    /// Where `point` goes.
    pub fn apply(&self, point: [f32; 3]) -> [f32; 3] {
        add3(self.apply_vector(point), self.translation)
    }

    /// Where the step `vector` goes, which the translation doesn't move.
    pub fn apply_vector(&self, [x, y, z]: [f32; 3]) -> [f32; 3] {
        [0, 1, 2].map(|i| self.x_axis[i] * x + self.y_axis[i] * y + self.z_axis[i] * z)
    }

    /// How much the transform scales volumes by: negative if it mirrors, 0 if it squashes
    /// everything flat.
    pub fn determinant(&self) -> f32 {
        dot3(self.x_axis, cross3(self.y_axis, self.z_axis))
    }

    /// The transform that undoes this one, unless it squashes everything flat (such as with a
    /// scale of 0), which nothing undoes.
    pub fn inverse(&self) -> Option<Self> {
        let determinant = self.determinant();
        if determinant == 0.0 || !determinant.is_finite() {
            return None;
        }
        // The inverse's rows are the cofactors' columns, over the determinant.
        let rows = self.cofactors().map(|row| scale3(row, 1.0 / determinant));
        let inverse = Self {
            x_axis: rows.map(|row| row[0]),
            y_axis: rows.map(|row| row[1]),
            z_axis: rows.map(|row| row[2]),
            translation: [0.0, 0.0, 0.0],
        };
        Some(Self {
            translation: scale3(inverse.apply_vector(self.translation), -1.0),
            ..inverse
        })
    }

    /// The columns of what moves surfaces' normals as this moves the surfaces: the inverse's
    /// transpose, scaled, so normals keep their direction but not their length. Mirroring
    /// transforms flip it, so normals still point out of what they're on.
    pub(crate) fn normal_matrix(&self) -> [[f32; 3]; 3] {
        let sign = if self.determinant() < 0.0 { -1.0 } else { 1.0 };
        self.cofactors().map(|column| scale3(column, sign))
    }

    /// The cross products of each pair of axes, which are the determinant times the rows of the
    /// inverse.
    fn cofactors(&self) -> [[f32; 3]; 3] {
        [
            cross3(self.y_axis, self.z_axis),
            cross3(self.z_axis, self.x_axis),
            cross3(self.x_axis, self.y_axis),
        ]
    }
}

impl Default for Affine3 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mul for Affine3 {
    type Output = Self;

    /// `rhs`, then `self`.
    fn mul(self, rhs: Self) -> Self {
        Self {
            x_axis: self.apply_vector(rhs.x_axis),
            y_axis: self.apply_vector(rhs.y_axis),
            z_axis: self.apply_vector(rhs.z_axis),
            translation: self.apply(rhs.translation),
        }
    }
}

/// A box with its sides along the axes, from its corner `min` to its corner `max`, such as what a
/// mesh takes up: see [`MeshData::bounds`](crate::graphics::MeshData::bounds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds3 {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl Bounds3 {
    /// The smallest box holding every one of `points`, or `None` if there are none.
    pub fn from_points(points: impl IntoIterator<Item = [f32; 3]>) -> Option<Self> {
        let mut points = points.into_iter();
        let first = points.next()?;
        Some(points.fold(Self::point(first), |bounds, point| {
            bounds.union(Self::point(point))
        }))
    }

    /// The box holding just `point`.
    const fn point(point: [f32; 3]) -> Self {
        Self {
            min: point,
            max: point,
        }
    }

    /// The smallest box holding both boxes.
    pub fn union(self, other: Self) -> Self {
        Self {
            min: [0, 1, 2].map(|i| self.min[i].min(other.min[i])),
            max: [0, 1, 2].map(|i| self.max[i].max(other.max[i])),
        }
    }

    /// The smallest box holding this one moved by `transform`, which is larger than the box
    /// moved when the transform turns it.
    pub fn transformed(&self, transform: &Affine3) -> Self {
        let [min, max] = [self.min, self.max];
        let corners = (0..8).map(|corner| {
            let pick = |axis: usize| {
                if corner & (1 << axis) == 0 {
                    min[axis]
                } else {
                    max[axis]
                }
            };
            transform.apply([pick(0), pick(1), pick(2)])
        });
        Self::from_points(corners).expect("a box has corners")
    }

    /// The point halfway between its corners.
    pub fn center(&self) -> [f32; 3] {
        [0, 1, 2].map(|i| (self.min[i] + self.max[i]) / 2.0)
    }

    /// How wide, tall and deep it is.
    pub fn size(&self) -> [f32; 3] {
        sub3(self.max, self.min)
    }

    /// Whether `point` is in it, or on its surface.
    pub fn contains(&self, point: [f32; 3]) -> bool {
        (0..3).all(|i| self.min[i] <= point[i] && point[i] <= self.max[i])
    }
}

/// The sine and cosine of `angle`, without calling into the math library for an angle of 0.
fn sin_cos(angle: f32) -> (f32, f32) {
    if angle == 0.0 {
        (0.0, 1.0)
    } else {
        angle.sin_cos()
    }
}

pub(crate) fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

pub(crate) fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub(crate) fn scale3(a: [f32; 3], scale: f32) -> [f32; 3] {
    a.map(|v| v * scale)
}

pub(crate) fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub(crate) fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// `a` scaled to a length of 1, or `None` if it has no direction to keep.
pub(crate) fn normalize3(a: [f32; 3]) -> Option<[f32; 3]> {
    let length = dot3(a, a).sqrt();
    (length > 0.0 && length.is_finite()).then(|| scale3(a, 1.0 / length))
}

#[cfg(test)]
mod tests {
    use std::f32::consts::FRAC_PI_2;

    use super::*;

    fn assert_near(actual: [f32; 2], expected: [f32; 2]) {
        assert!(
            (actual[0] - expected[0]).abs() < 1e-5 && (actual[1] - expected[1]).abs() < 1e-5,
            "{actual:?} is not {expected:?}"
        );
    }

    #[test]
    fn transforms_scale_then_rotate_then_translate() {
        let transform = Affine::from_parts([10.0, 20.0], FRAC_PI_2, [2.0, 3.0]);
        // (1, 0) scales to (2, 0), turns a quarter clockwise to (0, 2), then moves.
        assert_near(transform.apply([1.0, 0.0]), [10.0, 22.0]);
        // (0, 1) scales to (0, 3) and turns to (-3, 0).
        assert_near(transform.apply([0.0, 1.0]), [7.0, 20.0]);
    }

    #[test]
    fn exact_transforms_are_the_same_as_others_to_within_rounding() {
        for rotation in [0.0, 0.3, FRAC_PI_2, -2.5, 7.0] {
            let quick = Affine::from_parts([10.0, 20.0], rotation, [2.0, -3.0]);
            let exact = Affine::from_parts_exact([10.0, 20.0], rotation, [2.0, -3.0]);
            for point in [[1.0, 0.0], [0.0, 1.0], [-4.0, 9.0]] {
                assert_near(exact.apply(point), quick.apply(point));
            }
        }
        // Unturned, exactly.
        let unturned = Affine::from_parts_exact([1.0, 2.0], 0.0, [3.0, 4.0]);
        assert_eq!(unturned.x_axis, [3.0, 0.0]);
        assert_eq!(unturned.y_axis, [-0.0, 4.0]);
    }

    #[test]
    fn composing_applies_the_right_hand_side_first() {
        let parent = Affine::from_parts([100.0, 0.0], FRAC_PI_2, [1.0, 1.0]);
        let child = Affine::from_translation([5.0, 0.0]);
        // The child's origin is 5 along the parent's x axis, which points down.
        assert_near((parent * child).apply([0.0, 0.0]), [100.0, 5.0]);
        let point = [3.0, -7.0];
        assert_near(
            (parent * child).apply(point),
            parent.apply(child.apply(point)),
        );
    }

    #[test]
    fn inverses_undo_transforms() {
        let transform = Affine::from_parts([4.0, -6.0], 0.7, [2.0, -0.5]);
        let inverse = transform.inverse().unwrap();
        assert_near(inverse.apply(transform.apply([3.0, 9.0])), [3.0, 9.0]);
        assert_eq!(
            Affine::from_parts([1.0, 1.0], 0.0, [0.0, 1.0]).inverse(),
            None
        );
    }

    #[test]
    fn bounds_hold_every_corner() {
        let transform = Affine::from_parts([0.0, 0.0], FRAC_PI_2, [1.0, 1.0]);
        let bounds = transform.bounds(Rect::new(0.0, 0.0, 4.0, 2.0));
        assert_near([bounds.x, bounds.y], [-2.0, 0.0]);
        assert_near([bounds.w, bounds.h], [2.0, 4.0]);
    }

    fn assert_near3(actual: [f32; 3], expected: [f32; 3]) {
        assert!(
            (0..3).all(|i| (actual[i] - expected[i]).abs() < 1e-5),
            "{actual:?} is not {expected:?}"
        );
    }

    #[test]
    fn rotations_turn_counter_clockwise_about_their_axes() {
        assert_near3(
            Affine3::from_rotation_x(FRAC_PI_2).apply([0.0, 1.0, 0.0]),
            [0.0, 0.0, 1.0],
        );
        assert_near3(
            Affine3::from_rotation_y(FRAC_PI_2).apply([0.0, 0.0, 1.0]),
            [1.0, 0.0, 0.0],
        );
        assert_near3(
            Affine3::from_rotation_z(FRAC_PI_2).apply([1.0, 0.0, 0.0]),
            [0.0, 1.0, 0.0],
        );
    }

    #[test]
    fn composing_in_3d_applies_the_right_hand_side_first() {
        let parent = Affine3::from_translation([10.0, 0.0, 0.0]) * Affine3::from_rotation_y(0.4);
        let child = Affine3::from_translation([0.0, 2.0, 5.0]) * Affine3::from_scale([2.0; 3]);
        let point = [1.0, -3.0, 0.5];
        assert_near3(
            (parent * child).apply(point),
            parent.apply(child.apply(point)),
        );
        // Translating after scaling doesn't scale the translation.
        assert_near3(child.apply([0.0, 0.0, 0.0]), [0.0, 2.0, 5.0]);
    }

    #[test]
    fn inverses_undo_3d_transforms() {
        let transform = Affine3::from_translation([4.0, -6.0, 2.0])
            * Affine3::from_rotation_x(0.7)
            * Affine3::from_rotation_z(-1.2)
            * Affine3::from_scale([2.0, -0.5, 3.0]);
        let inverse = transform.inverse().unwrap();
        let point = [3.0, 9.0, -1.0];
        assert_near3(inverse.apply(transform.apply(point)), point);
        assert_near3(transform.apply(inverse.apply(point)), point);
        assert_eq!(Affine3::from_scale([1.0, 0.0, 1.0]).inverse(), None);
        assert!((Affine3::from_scale([2.0, 3.0, -1.0]).determinant() + 6.0).abs() < 1e-6);
    }

    #[test]
    fn normals_stay_perpendicular_to_what_they_are_on() {
        // A slope rising along x, stretched to rise twice as steeply.
        let stretch = Affine3::from_scale([1.0, 2.0, 1.0]);
        let along = stretch.apply_vector([1.0, 1.0, 0.0]);
        let [x, y, z] = stretch.normal_matrix();
        let normal = [-1.0, 1.0, 0.0];
        let moved = [0, 1, 2].map(|i| x[i] * normal[0] + y[i] * normal[1] + z[i] * normal[2]);
        assert!(dot3(moved, along).abs() < 1e-5, "{moved:?}");
        assert!(moved[1] > 0.0, "still facing up");

        // Mirrored, the up face of a cube still faces out of it, now down.
        let [_, y, _] = Affine3::from_scale([1.0, -1.0, 1.0]).normal_matrix();
        assert!(y[1] < 0.0);
    }

    #[test]
    fn normalizing_keeps_direction() {
        assert_near3(normalize3([0.0, 3.0, 4.0]).unwrap(), [0.0, 0.6, 0.8]);
        assert_eq!(normalize3([0.0; 3]), None);
    }

    #[test]
    fn containing_counts_the_top_and_left_edges() {
        let rect = Rect::new(0.0, 0.0, 4.0, 4.0);
        assert!(rect.contains([0.0, 0.0]));
        assert!(rect.contains([3.9, 2.0]));
        assert!(!rect.contains([4.0, 2.0]));
        assert!(!rect.contains([2.0, -0.1]));
    }

    #[test]
    fn overlapping_needs_shared_area() {
        let rect = Rect::new(0.0, 0.0, 4.0, 4.0);
        assert!(rect.overlaps(&Rect::new(3.0, 3.0, 4.0, 4.0)));
        assert!(rect.overlaps(&Rect::new(1.0, 1.0, 1.0, 1.0)), "inside");
        assert!(!rect.overlaps(&Rect::new(4.0, 0.0, 4.0, 4.0)), "touching");
        assert!(!rect.overlaps(&Rect::new(0.0, 5.0, 4.0, 4.0)));
    }

    #[test]
    fn bounds_hold_every_point() {
        let points = [[1.0, -2.0, 3.0], [-1.0, 4.0, 0.0], [0.0, 0.0, 5.0]];
        let bounds = Bounds3::from_points(points).unwrap();
        assert_eq!(bounds.min, [-1.0, -2.0, 0.0]);
        assert_eq!(bounds.max, [1.0, 4.0, 5.0]);
        assert_eq!(bounds.center(), [0.0, 1.0, 2.5]);
        assert_eq!(bounds.size(), [2.0, 6.0, 5.0]);
        assert!(points.iter().all(|&point| bounds.contains(point)));
        assert!(!bounds.contains([0.0, 0.0, -0.1]));
        assert_eq!(Bounds3::from_points([]), None);
    }

    #[test]
    fn transformed_bounds_hold_the_moved_box() {
        let unit = Bounds3 {
            min: [-1.0; 3],
            max: [1.0; 3],
        };
        let moved = unit.transformed(&Affine3::from_translation([10.0, 0.0, 0.0]));
        assert_eq!(moved.min, [9.0, -1.0, -1.0]);
        assert_eq!(moved.max, [11.0, 1.0, 1.0]);
        // Turned an eighth of a turn about y, its corners reach out to the square root of 2.
        let turned = unit.transformed(&Affine3::from_rotation_y(FRAC_PI_2 / 2.0));
        let reach = 2.0f32.sqrt();
        assert_near3(turned.min, [-reach, -1.0, -reach]);
        assert_near3(turned.max, [reach, 1.0, reach]);
        // Mirrored, it's the same box.
        let mirrored = unit.transformed(&Affine3::from_scale([-2.0, 1.0, 1.0]));
        assert_eq!(mirrored.min, [-2.0, -1.0, -1.0]);
        assert_eq!(mirrored.max, [2.0, 1.0, 1.0]);
    }
}
