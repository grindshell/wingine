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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlapping_needs_shared_area() {
        let rect = Rect::new(0.0, 0.0, 4.0, 4.0);
        assert!(rect.overlaps(&Rect::new(3.0, 3.0, 4.0, 4.0)));
        assert!(rect.overlaps(&Rect::new(1.0, 1.0, 1.0, 1.0)), "inside");
        assert!(!rect.overlaps(&Rect::new(4.0, 0.0, 4.0, 4.0)), "touching");
        assert!(!rect.overlaps(&Rect::new(0.0, 5.0, 4.0, 4.0)));
    }
}
