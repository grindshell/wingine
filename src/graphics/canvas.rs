use std::ops::{Bound, Range, RangeBounds};

use bytemuck::{Pod, Zeroable};

use super::{
    Camera, Color, Font, Sprite, TextLayout, Texture, camera::Transform, font::PlacedGlyph,
};
use crate::math::Rect;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    /// sRGB RGBA, which the shader converts to linear space.
    pub color: [u8; 4],
}

/// A run of consecutive quads that share a texture, drawn with one draw call.
pub(crate) struct Batch {
    /// `None` draws with a plain white texture, for solid fills.
    pub texture: Option<Texture>,
    /// Its quads, counting from the frame's first. Each quad is four vertices.
    pub quads: Range<u32>,
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

    /// The quads drawn so far this frame.
    pub(crate) fn quad_count(&self) -> u32 {
        (self.vertices.len() / 4) as u32
    }

    /// The draw calls rendering the quads drawn so far this frame will take.
    pub(crate) fn draw_call_count(&self) -> u32 {
        self.batches.len() as u32
    }

    /// Fills the whole canvas with `color`, discarding anything drawn so far this frame.
    pub fn clear(&mut self, color: Color) {
        self.clear_color = color;
        self.vertices.clear();
        self.batches.clear();
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.push_rect(None, rect, ([0.0, 0.0], [1.0, 1.0]), color);
    }

    /// Draws `sprite` stretched to fill `dest`, with its colors multiplied by `tint`.
    /// Use [`Color::WHITE`] to draw the sprite unchanged.
    pub fn draw_sprite(&mut self, sprite: &Sprite, dest: Rect, tint: Color) {
        self.push_rect(Some(sprite.texture()), dest, sprite.uv_bounds(), tint);
    }

    /// Draws `sprite` stretched to fill `dest`, then rotated by `angle` radians about the center
    /// of `dest`, with its colors multiplied by `tint`. Positive angles turn clockwise, since `y`
    /// grows downward. At an angle of 0 it draws exactly as [`Canvas::draw_sprite`] does.
    pub fn draw_sprite_rotated(&mut self, sprite: &Sprite, dest: Rect, angle: f32, tint: Color) {
        if angle == 0.0 {
            return self.draw_sprite(sprite, dest, tint);
        }
        let corners = rotated_corners(self.transform.apply(dest), angle);
        self.push_quad(Some(sprite.texture()), corners, sprite.uv_bounds(), tint);
    }

    /// Draws `text` in `font` with its top-left corner at `position`, without wrapping; `\n`
    /// starts a new line. The position is rounded to whole units, so pixel fonts stay crisp.
    pub fn draw_text(&mut self, font: &Font, text: &str, position: [f32; 2], color: Color) {
        let origin = position.map(f32::round);
        font.place_glyphs(text, |glyph| {
            self.push_glyph(font.texture(), glyph, origin, color);
        });
    }

    /// Draws the glyphs of `layout` in the range `glyphs` (`..` for all of them), with the
    /// layout's top-left corner at `position`, rounded as for [`Canvas::draw_text`]. Drawing a
    /// growing range, such as `..shown`, reveals text a little at a time. The range is cut to the
    /// glyphs there are.
    pub fn draw_text_layout(
        &mut self,
        layout: &TextLayout,
        glyphs: impl RangeBounds<usize>,
        position: [f32; 2],
        color: Color,
    ) {
        let origin = position.map(f32::round);
        let range = clamp_range(glyphs, layout.glyph_count());
        for glyph in &layout.glyphs()[range] {
            self.push_glyph(layout.font().texture(), *glyph, origin, color);
        }
    }

    fn push_glyph(
        &mut self,
        texture: &Texture,
        glyph: PlacedGlyph,
        [x, y]: [f32; 2],
        color: Color,
    ) {
        let rect = glyph.rect;
        let dest = Rect::new(x + rect.x, y + rect.y, rect.w, rect.h);
        self.push_rect(Some(texture), dest, glyph.uv, color);
    }

    /// Records a quad covering `dest`, in the coordinates drawing currently uses.
    fn push_rect(
        &mut self,
        texture: Option<&Texture>,
        dest: Rect,
        uv: ([f32; 2], [f32; 2]),
        color: Color,
    ) {
        let dest = self.transform.apply(dest);
        let corners = [
            [dest.x, dest.y],
            [dest.right(), dest.y],
            [dest.right(), dest.bottom()],
            [dest.x, dest.bottom()],
        ];
        self.push_quad(texture, corners, uv, color);
    }

    /// Records a quad with `corners` in canvas pixels, clockwise from the one that shows the
    /// top-left of the texture region `uv_min..uv_max`.
    fn push_quad(
        &mut self,
        texture: Option<&Texture>,
        corners: [[f32; 2]; 4],
        (uv_min, uv_max): ([f32; 2], [f32; 2]),
        color: Color,
    ) {
        let quad = self.quad_count();
        match self.batches.last_mut() {
            Some(batch) if same_texture(batch.texture.as_ref(), texture) => batch.quads.end += 1,
            _ => self.batches.push(Batch {
                texture: texture.cloned(),
                quads: quad..quad + 1,
            }),
        }

        let color = color.to_rgba8();
        let uvs = [
            uv_min,
            [uv_max[0], uv_min[1]],
            uv_max,
            [uv_min[0], uv_max[1]],
        ];
        self.vertices
            .extend(corners.into_iter().zip(uvs).map(|(position, uv)| Vertex {
                position,
                uv,
                color,
            }));
    }
}

/// The corners of `rect` rotated by `angle` radians about its center, clockwise from the one
/// that was its top-left.
fn rotated_corners(rect: Rect, angle: f32) -> [[f32; 2]; 4] {
    let (sin, cos) = angle.sin_cos();
    let [half_w, half_h] = [rect.w / 2.0, rect.h / 2.0];
    let center = [rect.x + half_w, rect.y + half_h];
    [
        [-half_w, -half_h],
        [half_w, -half_h],
        [half_w, half_h],
        [-half_w, half_h],
    ]
    .map(|[x, y]| [center[0] + x * cos - y * sin, center[1] + x * sin + y * cos])
}

/// `range` cut to fit `0..len`.
fn clamp_range(range: impl RangeBounds<usize>, len: usize) -> Range<usize> {
    let start = match range.start_bound() {
        Bound::Included(&start) => start,
        Bound::Excluded(&start) => start.saturating_add(1),
        Bound::Unbounded => 0,
    };
    let end = match range.end_bound() {
        Bound::Included(&end) => end.saturating_add(1),
        Bound::Excluded(&end) => end,
        Bound::Unbounded => len,
    };
    let end = end.min(len);
    start.min(end)..end
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
        assert_eq!(canvas.vertices[4].position, [2.0, 2.0]);
        assert_eq!(canvas.vertices[6].position, [3.0, 3.0]);
    }

    #[test]
    fn solid_fills_share_one_batch() {
        let mut canvas = Canvas::new();
        canvas.fill_rect(Rect::new(0.0, 0.0, 1.0, 1.0), Color::WHITE);
        canvas.fill_rect(Rect::new(2.0, 2.0, 1.0, 1.0), Color::RED);
        assert_eq!(canvas.batches.len(), 1);
        assert_eq!(canvas.batches[0].quads, 0..2);
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
    fn rotation_turns_corners_clockwise_about_the_center() {
        let corners = rotated_corners(Rect::new(0.0, 0.0, 4.0, 2.0), std::f32::consts::FRAC_PI_2);
        // The 4x2 rect stands up as 2x4 about its center (2, 1): its top-left corner goes to the
        // top-right.
        let expected = [[3.0, -1.0], [3.0, 3.0], [1.0, 3.0], [1.0, -1.0]];
        for (corner, expected) in corners.iter().zip(expected) {
            assert!(
                (corner[0] - expected[0]).abs() < 1e-5 && (corner[1] - expected[1]).abs() < 1e-5,
                "{corners:?}"
            );
        }
    }

    #[test]
    fn ranges_are_cut_to_fit() {
        assert_eq!(clamp_range(.., 5), 0..5);
        assert_eq!(clamp_range(..3, 5), 0..3);
        assert_eq!(clamp_range(..9, 5), 0..5);
        assert_eq!(clamp_range(2..=3, 5), 2..4);
        assert_eq!(clamp_range(7.., 5), 5..5);
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
        assert!(canvas.batches.is_empty());
        assert_eq!(canvas.clear_color, Color::RED);
    }
}
