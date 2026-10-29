use bytemuck::{Pod, Zeroable};

use super::Color;
use crate::math::Rect;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub position: [f32; 2],
    /// Linear-space RGBA.
    pub color: [f32; 4],
}

/// Records what a game draws during a frame.
///
/// Coordinates are in logical pixels (physical pixels divided by the window's scale factor), with
/// the origin at the top-left of the window and `y` growing downward. Shapes are drawn in the order
/// they are submitted, later shapes on top.
pub struct Canvas {
    pub(crate) clear_color: Color,
    pub(crate) vertices: Vec<Vertex>,
    pub(crate) indices: Vec<u32>,
    width: f32,
    height: f32,
}

impl Canvas {
    pub(crate) fn new() -> Self {
        Self {
            clear_color: Color::BLACK,
            vertices: Vec::new(),
            indices: Vec::new(),
            width: 0.0,
            height: 0.0,
        }
    }

    /// Resets the canvas for a new frame, keeping buffer allocations.
    pub(crate) fn begin(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
        self.clear(Color::BLACK);
    }

    /// Width of the drawable area in logical pixels.
    pub fn width(&self) -> f32 {
        self.width
    }

    /// Height of the drawable area in logical pixels.
    pub fn height(&self) -> f32 {
        self.height
    }

    /// Fills the whole canvas with `color`, discarding anything drawn so far this frame.
    pub fn clear(&mut self, color: Color) {
        self.clear_color = color;
        self.vertices.clear();
        self.indices.clear();
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        let base = self.vertices.len() as u32;
        let color = color.to_linear();
        let corners = [
            [rect.x, rect.y],
            [rect.right(), rect.y],
            [rect.right(), rect.bottom()],
            [rect.x, rect.bottom()],
        ];
        self.vertices
            .extend(corners.map(|position| Vertex { position, color }));
        self.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_rect_emits_one_quad() {
        let mut canvas = Canvas::new();
        canvas.fill_rect(Rect::new(0.0, 0.0, 1.0, 1.0), Color::WHITE);
        canvas.fill_rect(Rect::new(2.0, 2.0, 1.0, 1.0), Color::WHITE);
        assert_eq!(canvas.vertices.len(), 8);
        assert_eq!(canvas.indices[6..], [4, 5, 6, 4, 6, 7]);
    }

    #[test]
    fn clear_discards_earlier_draws() {
        let mut canvas = Canvas::new();
        canvas.fill_rect(Rect::new(0.0, 0.0, 1.0, 1.0), Color::WHITE);
        canvas.clear(Color::RED);
        assert!(canvas.vertices.is_empty());
        assert!(canvas.indices.is_empty());
        assert_eq!(canvas.clear_color, Color::RED);
    }
}
