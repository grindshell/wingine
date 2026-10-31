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
}
