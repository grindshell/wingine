//! Points and boxes on the plane, and what one box answers by itself.
//!
//! Only `+`, `-`, `*`, `/` and `sqrt`, so every answer is the same to the bit on every platform.

use std::ops::{Add, Mul, Sub};

/// A point or a direction on the plane. A game on the ground of a 3D view gives its `x` and `z`
/// as `x` and `y`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// How long it is, as a direction: how far from the origin, as a point.
    pub fn length(self) -> f32 {
        (self.x * self.x + self.y * self.y).sqrt()
    }
}

impl Add for Point {
    type Output = Point;
    fn add(self, other: Point) -> Point {
        Point::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for Point {
    type Output = Point;
    fn sub(self, other: Point) -> Point {
        Point::new(self.x - other.x, self.y - other.y)
    }
}

impl Mul<f32> for Point {
    type Output = Point;
    fn mul(self, scale: f32) -> Point {
        Point::new(self.x * scale, self.y * scale)
    }
}

/// A box with its sides along the axes, from its least corner to its greatest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
    pub min: Point,
    pub max: Point,
}

impl Area {
    /// The box between two opposite corners, whichever two they are.
    pub fn from_corners(a: Point, b: Point) -> Self {
        Self {
            min: Point::new(a.x.min(b.x), a.y.min(b.y)),
            max: Point::new(a.x.max(b.x), a.y.max(b.y)),
        }
    }

    /// Whether `point` is in it, or on its edge.
    pub fn contains(self, point: Point) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }

    /// Whether `point` is inside it. Its edge isn't inside.
    pub fn surrounds(self, point: Point) -> bool {
        point.x > self.min.x && point.x < self.max.x && point.y > self.min.y && point.y < self.max.y
    }

    /// Whether it and `other` share any point, an edge's included.
    pub fn overlaps(self, other: Area) -> bool {
        self.min.x <= other.max.x
            && other.min.x <= self.max.x
            && self.min.y <= other.max.y
            && other.min.y <= self.max.y
    }

    /// It, wider by `margin` on every side.
    pub fn grown(self, margin: f32) -> Area {
        Area {
            min: Point::new(self.min.x - margin, self.min.y - margin),
            max: Point::new(self.max.x + margin, self.max.y + margin),
        }
    }

    /// How far `point` is from it: 0 inside or on its edge.
    pub fn distance(self, point: Point) -> f32 {
        let x = (self.min.x - point.x).max(point.x - self.max.x).max(0.0);
        let y = (self.min.y - point.y).max(point.y - self.max.y).max(0.0);
        (x * x + y * y).sqrt()
    }

    /// Where the way from `a` to `b` first enters it, grown by `margin`, as a share of the way
    /// from `a`, 0 if `a` is inside: none if the way never passes through its inside. A way
    /// along its edge doesn't.
    pub fn entry(self, a: Point, b: Point, margin: f32) -> Option<f32> {
        let Area { min, max } = self.grown(margin);
        let mut enter = 0.0_f32;
        let mut exit = 1.0_f32;
        for (from, delta, low, high) in [
            (a.x, b.x - a.x, min.x, max.x),
            (a.y, b.y - a.y, min.y, max.y),
        ] {
            if delta == 0.0 {
                if from <= low || from >= high {
                    return None;
                }
            } else {
                let (t0, t1) = ((low - from) / delta, (high - from) / delta);
                let (near, far) = if t0 < t1 { (t0, t1) } else { (t1, t0) };
                enter = enter.max(near);
                exit = exit.min(far);
                if enter >= exit {
                    return None;
                }
            }
        }
        Some(enter)
    }

    /// Moves `point` out of it, grown by `margin`, if it's inside: across the side it's nearest,
    /// onto the edge, which isn't inside. Returns that side's outward direction.
    pub fn push_out(self, point: &mut Point, margin: f32) -> Option<Point> {
        let grown = self.grown(margin);
        if !grown.surrounds(*point) {
            return None;
        }
        let Area {
            min: low,
            max: high,
        } = grown;
        let sides = [
            (point.x - low.x, Point::new(-1.0, 0.0)),
            (high.x - point.x, Point::new(1.0, 0.0)),
            (point.y - low.y, Point::new(0.0, -1.0)),
            (high.y - point.y, Point::new(0.0, 1.0)),
        ];
        let mut nearest = sides[0];
        for side in &sides[1..] {
            if side.0 < nearest.0 {
                nearest = *side;
            }
        }
        // Onto the edge itself, rather than by a sum that may round short of it.
        let out = nearest.1;
        if out.x < 0.0 {
            point.x = low.x;
        } else if out.x > 0.0 {
            point.x = high.x;
        } else if out.y < 0.0 {
            point.y = low.y;
        } else {
            point.y = high.y;
        }
        Some(out)
    }

    /// Its corners, grown by `margin`, round it from the least: the least, then along `x`, the
    /// greatest, and along `y`.
    pub fn corners(self, margin: f32) -> [Point; 4] {
        let Area {
            min: low,
            max: high,
        } = self.grown(margin);
        [
            low,
            Point::new(high.x, low.y),
            high,
            Point::new(low.x, high.y),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wall 10 across and 1 deep, from the origin.
    fn wall() -> Area {
        Area::from_corners(Point::new(-5.0, 0.0), Point::new(5.0, 1.0))
    }

    #[test]
    fn a_way_enters_a_box_only_through_its_inside() {
        let wall = wall();
        assert_eq!(
            wall.entry(Point::new(0.0, -2.0), Point::new(0.0, 3.0), 0.0),
            Some(0.4)
        );
        assert_eq!(
            wall.entry(Point::new(-8.0, -2.0), Point::new(-6.0, 3.0), 0.0),
            None
        );
        // Along its edge isn't through it, but grown, it is.
        let (a, b) = (Point::new(-8.0, 1.0), Point::new(8.0, 1.0));
        assert_eq!(wall.entry(a, b, 0.0), None);
        assert!(wall.entry(a, b, 0.1).is_some());
        // From inside, it's entered at once.
        assert_eq!(
            wall.entry(Point::new(0.0, 0.5), Point::new(0.0, 9.0), 0.0),
            Some(0.0)
        );
    }

    #[test]
    fn a_point_inside_is_pushed_out_across_the_nearest_side() {
        let wall = wall();
        let mut point = Point::new(2.0, 0.8);
        assert_eq!(wall.push_out(&mut point, 0.4), Some(Point::new(0.0, 1.0)));
        assert_eq!(point, Point::new(2.0, 1.4));
        assert!(!wall.grown(0.4).surrounds(point));
        assert_eq!(wall.push_out(&mut point, 0.4), None);
    }

    #[test]
    fn distance_is_to_the_nearest_edge_and_nothing_inside() {
        let wall = wall();
        assert_eq!(wall.distance(Point::new(0.0, 0.5)), 0.0);
        assert_eq!(wall.distance(Point::new(0.0, 3.0)), 2.0);
        assert_eq!(wall.distance(Point::new(8.0, 5.0)), 5.0);
    }
}
