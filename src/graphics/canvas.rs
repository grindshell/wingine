use std::ops::Range;

use bytemuck::{Pod, Zeroable};

use super::{Camera, Color, Sprite, Texture, camera::Transform};
use crate::math::Rect;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    /// Linear-space RGBA.
    pub color: [f32; 4],
}

/// A run of consecutive quads that share a texture, drawn with one draw call.
pub(crate) struct Batch {
    /// `None` draws with a plain white texture, for solid fills.
    pub texture: Option<Texture>,
    pub indices: Range<u32>,
}

/// Records what a game draws during a frame.
///
/// Coordinates are canvas pixels, with the origin at the top-left and `y` growing downward. The
/// canvas is the window's size in logical pixels, or the fixed size set by
/// [`Resolution::Fixed`](super::Resolution::Fixed). With a camera set, drawing is in world
/// coordinates instead. Shapes are drawn in the order they are submitted, later shapes on top.
pub struct Canvas {
    pub(crate) clear_color: Color,
    pub(crate) vertices: Vec<Vertex>,
    pub(crate) indices: Vec<u32>,
    pub(crate) batches: Vec<Batch>,
    width: f32,
    height: f32,
    transform: Transform,
}

impl Canvas {
    pub(crate) fn new() -> Self {
        Self {
            clear_color: Color::BLACK,
            vertices: Vec::new(),
            indices: Vec::new(),
            batches: Vec::new(),
            width: 0.0,
            height: 0.0,
            transform: Transform::IDENTITY,
        }
    }

    /// Resets the canvas for a new frame, keeping buffer allocations.
    pub(crate) fn begin(&mut self, width: f32, height: f32) {
        self.width = width;
        self.height = height;
        self.transform = Transform::IDENTITY;
        self.clear(Color::BLACK);
    }

    /// Width of the drawable area in canvas pixels.
    pub fn width(&self) -> f32 {
        self.width
    }

    /// Height of the drawable area in canvas pixels.
    pub fn height(&self) -> f32 {
        self.height
    }

    /// Positions everything drawn after this call with `camera`, so coordinates are in the
    /// world. `None`, the default at the start of each frame, draws in canvas pixels, as for a
    /// HUD.
    pub fn set_camera(&mut self, camera: Option<Camera>) {
        self.transform = camera.map_or(Transform::IDENTITY, |camera| {
            camera.transform(self.width, self.height)
        });
    }

    /// The area the canvas shows, in the coordinates drawing currently uses: world coordinates
    /// with a camera set, otherwise canvas pixels. Anything drawn outside it is off-screen, so
    /// large scenes can skip it.
    pub fn visible_area(&self) -> Rect {
        self.transform.visible_area(self.width, self.height)
    }

    /// Fills the whole canvas with `color`, discarding anything drawn so far this frame.
    pub fn clear(&mut self, color: Color) {
        self.clear_color = color;
        self.vertices.clear();
        self.indices.clear();
        self.batches.clear();
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.push_quad(None, rect, ([0.0, 0.0], [1.0, 1.0]), color);
    }

    /// Draws `sprite` stretched to fill `dest`, with its colors multiplied by `tint`.
    /// Use [`Color::WHITE`] to draw the sprite unchanged.
    pub fn draw_sprite(&mut self, sprite: &Sprite, dest: Rect, tint: Color) {
        self.push_quad(Some(sprite.texture()), dest, sprite.uv_bounds(), tint);
    }

    fn push_quad(
        &mut self,
        texture: Option<&Texture>,
        dest: Rect,
        (uv_min, uv_max): ([f32; 2], [f32; 2]),
        color: Color,
    ) {
        let start = self.indices.len() as u32;
        match self.batches.last_mut() {
            Some(batch) if same_texture(batch.texture.as_ref(), texture) => batch.indices.end += 6,
            _ => self.batches.push(Batch {
                texture: texture.cloned(),
                indices: start..start + 6,
            }),
        }

        let base = self.vertices.len() as u32;
        let dest = self.transform.apply(dest);
        let color = color.to_linear();
        let corners = [
            ([dest.x, dest.y], uv_min),
            ([dest.right(), dest.y], [uv_max[0], uv_min[1]]),
            ([dest.right(), dest.bottom()], uv_max),
            ([dest.x, dest.bottom()], [uv_min[0], uv_max[1]]),
        ];
        self.vertices.extend(corners.map(|(position, uv)| Vertex {
            position,
            uv,
            color,
        }));
        self.indices.extend([0, 1, 2, 0, 2, 3].map(|i| base + i));
    }
}

fn same_texture(a: Option<&Texture>, b: Option<&Texture>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => a.same_as(b),
        _ => false,
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
    fn solid_fills_share_one_batch() {
        let mut canvas = Canvas::new();
        canvas.fill_rect(Rect::new(0.0, 0.0, 1.0, 1.0), Color::WHITE);
        canvas.fill_rect(Rect::new(2.0, 2.0, 1.0, 1.0), Color::RED);
        assert_eq!(canvas.batches.len(), 1);
        assert_eq!(canvas.batches[0].indices, 0..12);
    }

    #[test]
    fn camera_applies_until_unset() {
        let mut canvas = Canvas::new();
        canvas.begin(20.0, 20.0);
        canvas.set_camera(Some(Camera::new(100.0, 100.0)));
        canvas.fill_rect(Rect::new(100.0, 100.0, 1.0, 1.0), Color::WHITE);
        canvas.set_camera(None);
        canvas.fill_rect(Rect::new(100.0, 100.0, 1.0, 1.0), Color::WHITE);
        assert_eq!(canvas.vertices[0].position, [10.0, 10.0]);
        assert_eq!(canvas.vertices[4].position, [100.0, 100.0]);
    }

    #[test]
    fn visible_area_follows_the_camera() {
        let mut canvas = Canvas::new();
        canvas.begin(20.0, 10.0);
        assert_eq!(canvas.visible_area(), Rect::new(0.0, 0.0, 20.0, 10.0));
        canvas.set_camera(Some(Camera {
            zoom: 2.0,
            ..Camera::new(100.0, 50.0)
        }));
        assert_eq!(canvas.visible_area(), Rect::new(95.0, 47.5, 10.0, 5.0));
    }

    #[test]
    fn begin_resets_the_camera() {
        let mut canvas = Canvas::new();
        canvas.begin(20.0, 20.0);
        canvas.set_camera(Some(Camera::new(100.0, 100.0)));
        canvas.begin(20.0, 20.0);
        canvas.fill_rect(Rect::new(1.0, 1.0, 1.0, 1.0), Color::WHITE);
        assert_eq!(canvas.vertices[0].position, [1.0, 1.0]);
    }

    #[test]
    fn clear_discards_earlier_draws() {
        let mut canvas = Canvas::new();
        canvas.fill_rect(Rect::new(0.0, 0.0, 1.0, 1.0), Color::WHITE);
        canvas.clear(Color::RED);
        assert!(canvas.vertices.is_empty());
        assert!(canvas.indices.is_empty());
        assert!(canvas.batches.is_empty());
        assert_eq!(canvas.clear_color, Color::RED);
    }
}
