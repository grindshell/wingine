use std::ops::{Bound, Range, RangeBounds};

use bytemuck::{Pod, Zeroable};

use super::{
    Camera, Canvas3d, Color, Font, Mesh, Sprite, TextLayout, Texture, View3d,
    camera::Transform,
    font::PlacedGlyph,
    view3d::{Instance, MeshDraw, Segment3d},
};
use crate::{
    FrameStats,
    math::{Affine, Rect},
};

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
/// coordinates instead. Shapes are drawn in the order they are submitted, later shapes on top, and
/// 3D views ([`Canvas::begin_3d`]) take their place in that order too.
pub struct Canvas {
    pub(crate) clear_color: Color,
    pub(crate) vertices: Vec<Vertex>,
    pub(crate) batches: Vec<Batch>,
    /// 3D views, in the order they were drawn.
    pub(crate) segments: Vec<Segment3d>,
    /// The meshes drawn in them, in order, and how each is placed.
    pub(crate) mesh_draws: Vec<MeshDraw>,
    pub(crate) instances: Vec<Instance>,
    width: f32,
    height: f32,
    transform: Transform,
    /// The stats of the frame before, as the context has them, for Luau games, which are lent the
    /// canvas and not the context while they draw.
    pub(crate) frame_stats: FrameStats,
}

impl Canvas {
    pub(crate) fn new() -> Self {
        Self {
            clear_color: Color::BLACK,
            vertices: Vec::new(),
            batches: Vec::new(),
            segments: Vec::new(),
            mesh_draws: Vec::new(),
            instances: Vec::new(),
            width: 0.0,
            height: 0.0,
            transform: Transform::IDENTITY,
            frame_stats: FrameStats::default(),
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

    /// The draw calls rendering what was drawn so far this frame will take: one for each run of
    /// quads with the same texture, and one for each mesh.
    pub(crate) fn draw_call_count(&self) -> u32 {
        let quads = self.batches.iter().filter(|b| !b.quads.is_empty()).count();
        (quads + self.mesh_draws.len()) as u32
    }

    /// The triangles of the meshes drawn so far this frame.
    pub(crate) fn triangle_count(&self) -> u32 {
        self.mesh_draws
            .iter()
            .map(|draw| draw.mesh.as_ref().map_or(2, Mesh::triangle_count))
            .sum()
    }

    /// Fills the whole canvas with `color`, discarding anything drawn so far this frame.
    pub fn clear(&mut self, color: Color) {
        self.clear_color = color;
        self.vertices.clear();
        self.batches.clear();
        self.segments.clear();
        self.mesh_draws.clear();
        self.instances.clear();
    }

    /// Begins a 3D view, which shows what [`Canvas3d::draw_mesh`] draws in it through
    /// `view.camera`, on `view.rect` of the canvas, over what was drawn before. What's drawn on
    /// the canvas after the returned [`Canvas3d`] is dropped goes over it.
    ///
    /// Each view has a depth buffer of its own: meshes in it hide one another by how near they
    /// are, but a later view draws over an earlier one.
    pub fn begin_3d(&mut self, view: View3d) -> Canvas3d<'_> {
        let rect = self.transform.apply(view.rect);
        // Quads drawn after the view mustn't join a batch drawn before it.
        let quad = self.quad_count();
        self.batches.push(Batch {
            texture: None,
            quads: quad..quad,
        });
        let first = self.mesh_draws.len() as u32;
        self.segments.push(Segment3d {
            view: View3d { rect, ..view },
            before_batch: (self.batches.len() - 1) as u32,
            draws: first..first,
        });
        Canvas3d { canvas: self }
    }

    /// The 3D view begun last, to draw more in, unless something else has been drawn since.
    pub(crate) fn open_view(&mut self) -> Option<Canvas3d<'_>> {
        let segment = self.segments.last()?;
        let last = self.batches.len().checked_sub(1)?;
        let open = segment.before_batch as usize == last && self.batches[last].quads.is_empty();
        open.then_some(Canvas3d { canvas: self })
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.push_rect(None, rect, SOLID, color);
    }

    /// Draws the outline of `rect`, `thickness` wide inside its edges, so it covers the edge of
    /// what [`Canvas::fill_rect`] would fill. Its sides don't overlap, so a translucent outline
    /// is even all round. An outline thick enough to meet itself fills the rect.
    pub fn outline_rect(&mut self, rect: Rect, thickness: f32, color: Color) {
        if !(thickness > 0.0 && rect.w > 0.0 && rect.h > 0.0) {
            return;
        }
        if thickness * 2.0 >= rect.w.min(rect.h) {
            return self.fill_rect(rect, color);
        }
        // In canvas pixels, so the sides meet exactly, whatever the camera's zoom.
        let outer = self.transform.apply(rect);
        let thickness = thickness * self.transform.scale;
        let [left, top, right, bottom] = [outer.x, outer.y, outer.right(), outer.bottom()];
        let [inner_top, inner_bottom] = [top + thickness, bottom - thickness];
        for [x0, y0, x1, y1] in [
            [left, top, right, inner_top],
            [left, inner_top, left + thickness, inner_bottom],
            [right - thickness, inner_top, right, inner_bottom],
            [left, inner_bottom, right, bottom],
        ] {
            self.push_quad(None, [[x0, y0], [x1, y0], [x1, y1], [x0, y1]], SOLID, color);
        }
    }

    /// Draws a line `thickness` wide from `from` to `to`, centered on the line between them and
    /// ending square at them. Like everything drawn, it covers the pixels whose middles it covers,
    /// so a line 1 wide lights a row of pixels when it runs along their middles: from `[0.0, 5.5]`
    /// to `[11.0, 5.5]` lights the pixels `fill_rect` does with `Rect::new(0.0, 5.0, 11.0, 1.0)`.
    /// A line from a point to itself draws nothing.
    pub fn draw_line(&mut self, from: [f32; 2], to: [f32; 2], thickness: f32, color: Color) {
        let [dx, dy] = [to[0] - from[0], to[1] - from[1]];
        let length = (dx * dx + dy * dy).sqrt();
        if !(length > 0.0 && thickness > 0.0) {
            return;
        }
        // Half the thickness, across the line, to the left of it going from `from` to `to`.
        let across = thickness / 2.0 / length;
        let [nx, ny] = [dy * across, -dx * across];
        let corners = [
            [from[0] + nx, from[1] + ny],
            [to[0] + nx, to[1] + ny],
            [to[0] - nx, to[1] - ny],
            [from[0] - nx, from[1] - ny],
        ]
        .map(|corner| self.transform.apply_point(corner));
        self.push_quad(None, corners, SOLID, color);
    }

    /// Fills a circle of `radius` about `center`, drawn as a polygon with as many sides as its
    /// size on the canvas needs to look round.
    pub fn fill_circle(&mut self, center: [f32; 2], radius: f32, color: Color) {
        if radius.is_nan() || radius <= 0.0 {
            return;
        }
        let center = self.transform.apply_point(center);
        let radius = radius * self.transform.scale;
        let polygon = Polygon::new(radius);
        let at = |[x, y]: [f32; 2]| [center[0] + x * radius, center[1] + y * radius];
        // A fan of triangles from the middle, two to a quad, the last closing on the first corner.
        let first = polygon.first();
        let mut corner = first;
        for quad in 0..polygon.sides / 2 {
            let next = polygon.turn(corner);
            let after = if quad + 1 == polygon.sides / 2 {
                first
            } else {
                polygon.turn(next)
            };
            self.push_quad(
                None,
                [center, at(corner), at(next), at(after)],
                SOLID,
                color,
            );
            corner = after;
        }
    }

    /// Draws the outline of a circle of `radius` about `center`, `thickness` wide inside its
    /// edge, so it covers the edge of what [`Canvas::fill_circle`] would fill, as evenly as
    /// [`Canvas::outline_rect`]. An outline thick enough to meet itself fills the circle.
    pub fn outline_circle(&mut self, center: [f32; 2], radius: f32, thickness: f32, color: Color) {
        if !(radius > 0.0 && thickness > 0.0) {
            return;
        }
        if thickness >= radius {
            return self.fill_circle(center, radius, color);
        }
        let center = self.transform.apply_point(center);
        let outer = radius * self.transform.scale;
        let inner = (radius - thickness) * self.transform.scale;
        let polygon = Polygon::new(outer);
        let at = |[x, y]: [f32; 2], radius: f32| [center[0] + x * radius, center[1] + y * radius];
        let first = polygon.first();
        let mut corner = first;
        for side in 0..polygon.sides {
            let next = if side + 1 == polygon.sides {
                first
            } else {
                polygon.turn(corner)
            };
            let corners = [
                at(corner, outer),
                at(next, outer),
                at(next, inner),
                at(corner, inner),
            ];
            self.push_quad(None, corners, SOLID, color);
            corner = next;
        }
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

    /// Draws `sprite` stretched to fill `rect`, then moved by `transform`, with its colors
    /// multiplied by `tint`. `transform` turns, scales or mirrors the sprite about the origin of
    /// `rect`'s coordinates, so a sprite turning about its middle has its middle there: for a
    /// 16x16 sprite, `rect` is `Rect::new(-8.0, -8.0, 16.0, 16.0)`.
    pub fn draw_sprite_transformed(
        &mut self,
        sprite: &Sprite,
        rect: Rect,
        transform: Affine,
        tint: Color,
    ) {
        let corners = self.transformed_corners(rect, transform);
        self.push_quad(Some(sprite.texture()), corners, sprite.uv_bounds(), tint);
    }

    /// Fills `rect` moved by `transform`, as [`Canvas::draw_sprite_transformed`] places a sprite.
    pub fn fill_rect_transformed(&mut self, rect: Rect, transform: Affine, color: Color) {
        let corners = self.transformed_corners(rect, transform);
        self.push_quad(None, corners, SOLID, color);
    }

    /// The corners of `rect` moved by `transform`, in canvas pixels.
    fn transformed_corners(&self, rect: Rect, transform: Affine) -> [[f32; 2]; 4] {
        transform
            .corners(rect)
            .map(|corner| self.transform.apply_point(corner))
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

/// The texture region solid fills draw, the whole of the white texture.
const SOLID: ([f32; 2], [f32; 2]) = ([0.0, 0.0], [1.0, 1.0]);

/// The polygon a circle is drawn as, with enough sides that its edge strays from the circle's by
/// no more than an eighth of a pixel either way.
struct Polygon {
    sides: u32,
    /// How far each corner turns from the last, as its cosine and sine.
    cos: f32,
    sin: f32,
    /// How far out the corners are, for a circle of radius 1: a little outside it, so the sides'
    /// middles are as far inside.
    reach: f32,
}

impl Polygon {
    /// The polygon for a circle of `radius` canvas pixels.
    fn new(radius: f32) -> Self {
        use std::f32::consts::{PI, TAU};
        // The middle of a side is about `radius * (PI / sides)^2 / 2` inside the corners, which
        // is a quarter of a pixel at this many.
        let needed = (PI * (2.0 * radius).sqrt()).min(256.0) as u32 + 1;
        let sides = needed.clamp(8, 256).next_multiple_of(2);
        let (sin, cos) = (TAU / sides as f32).sin_cos();
        Self {
            sides,
            cos,
            sin,
            reach: 2.0 / (1.0 + (PI / sides as f32).cos()),
        }
    }

    /// The first corner, for a circle of radius 1 about the origin.
    fn first(&self) -> [f32; 2] {
        [self.reach, 0.0]
    }

    /// The corner after `corner`, clockwise.
    fn turn(&self, [x, y]: [f32; 2]) -> [f32; 2] {
        [x * self.cos - y * self.sin, x * self.sin + y * self.cos]
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

    /// The corners of each quad drawn so far.
    fn quads(canvas: &Canvas) -> Vec<[[f32; 2]; 4]> {
        canvas
            .vertices
            .chunks(4)
            .map(|quad| [0, 1, 2, 3].map(|i| quad[i].position))
            .collect()
    }

    #[test]
    fn outlines_are_four_sides_that_meet_without_overlapping() {
        let mut canvas = Canvas::new();
        canvas.outline_rect(Rect::new(10.0, 20.0, 8.0, 6.0), 2.0, Color::WHITE);
        let sides: Vec<[f32; 4]> = quads(&canvas)
            .iter()
            .map(|[top_left, _, bottom_right, _]| {
                [top_left[0], top_left[1], bottom_right[0], bottom_right[1]]
            })
            .collect();
        assert_eq!(
            sides,
            [
                [10.0, 20.0, 18.0, 22.0],
                [10.0, 22.0, 12.0, 24.0],
                [16.0, 22.0, 18.0, 24.0],
                [10.0, 24.0, 18.0, 26.0],
            ]
        );
        assert_eq!(canvas.batches.len(), 1);
    }

    #[test]
    fn outlines_thick_enough_to_meet_fill() {
        let mut canvas = Canvas::new();
        canvas.outline_rect(Rect::new(0.0, 0.0, 8.0, 4.0), 2.0, Color::WHITE);
        canvas.outline_circle([0.0, 0.0], 3.0, 3.0, Color::WHITE);
        let mut filled = Canvas::new();
        filled.fill_rect(Rect::new(0.0, 0.0, 8.0, 4.0), Color::WHITE);
        filled.fill_circle([0.0, 0.0], 3.0, Color::WHITE);
        assert_eq!(quads(&canvas), quads(&filled));
    }

    #[test]
    fn lines_run_between_their_ends_and_as_wide_as_their_thickness() {
        let mut canvas = Canvas::new();
        canvas.draw_line([0.0, 5.5], [11.0, 5.5], 1.0, Color::WHITE);
        assert_eq!(
            quads(&canvas),
            [[[0.0, 5.0], [11.0, 5.0], [11.0, 6.0], [0.0, 6.0]]]
        );

        canvas.clear(Color::BLACK);
        canvas.draw_line([0.0, 0.0], [3.0, 4.0], 10.0, Color::WHITE);
        let [a, b, c, d] = quads(&canvas)[0];
        let near =
            |p: [f32; 2], q: [f32; 2]| (p[0] - q[0]).abs() < 1e-5 && (p[1] - q[1]).abs() < 1e-5;
        assert!(near(a, [4.0, -3.0]) && near(b, [7.0, 1.0]), "{a:?} {b:?}");
        assert!(near(c, [-1.0, 7.0]) && near(d, [-4.0, 3.0]), "{c:?} {d:?}");
    }

    #[test]
    fn shapes_with_nothing_to_them_draw_nothing() {
        let mut canvas = Canvas::new();
        canvas.draw_line([1.0, 1.0], [1.0, 1.0], 1.0, Color::WHITE);
        canvas.draw_line([0.0, 0.0], [1.0, 1.0], 0.0, Color::WHITE);
        canvas.outline_rect(Rect::new(0.0, 0.0, 0.0, 4.0), 1.0, Color::WHITE);
        canvas.outline_rect(Rect::new(0.0, 0.0, 4.0, 4.0), f32::NAN, Color::WHITE);
        canvas.fill_circle([0.0, 0.0], 0.0, Color::WHITE);
        canvas.fill_circle([0.0, 0.0], -1.0, Color::WHITE);
        canvas.outline_circle([0.0, 0.0], 4.0, 0.0, Color::WHITE);
        canvas.outline_circle([0.0, 0.0], f32::NAN, 1.0, Color::WHITE);
        assert!(canvas.vertices.is_empty());
    }

    #[test]
    fn circles_are_fans_and_rings_that_close() {
        let mut canvas = Canvas::new();
        canvas.fill_circle([10.0, 20.0], 4.0, Color::WHITE);
        let fan = quads(&canvas);
        let sides = Polygon::new(4.0).sides as usize;
        assert_eq!(fan.len(), sides / 2);
        for (quad, next) in fan.iter().zip(fan.iter().cycle().skip(1)) {
            assert_eq!(quad[0], [10.0, 20.0], "from the middle");
            assert_eq!(quad[3], next[1], "each quad starts where the last ended");
        }

        canvas.clear(Color::BLACK);
        canvas.outline_circle([10.0, 20.0], 4.0, 1.0, Color::WHITE);
        let ring = quads(&canvas);
        assert_eq!(ring.len(), sides);
        for (quad, next) in ring.iter().zip(ring.iter().cycle().skip(1)) {
            assert_eq!(
                [quad[1], quad[2]],
                [next[0], next[3]],
                "each side meets the next"
            );
            let distance = |[x, y]: [f32; 2]| (x - 10.0).hypot(y - 20.0);
            assert!((distance(quad[0]) - 4.0).abs() < 0.15, "{:?}", quad[0]);
            assert!((distance(quad[3]) - 3.0).abs() < 0.15, "{:?}", quad[3]);
        }
    }

    #[test]
    fn circles_get_more_sides_the_larger_they_are_drawn() {
        assert_eq!(Polygon::new(1.0).sides, 8);
        assert_eq!(Polygon::new(100.0).sides, 46);
        assert_eq!(Polygon::new(1e9).sides, 256);
        assert_eq!(Polygon::new(f32::INFINITY).sides, 256);

        // A circle twice the size on the canvas through a zoomed camera.
        let mut canvas = Canvas::new();
        canvas.begin(100.0, 100.0);
        canvas.set_camera(Some(Camera {
            zoom: 2.0,
            ..Camera::new(0.0, 0.0)
        }));
        canvas.fill_circle([0.0, 0.0], 50.0, Color::WHITE);
        assert_eq!(quads(&canvas).len(), 46 / 2);
        assert_eq!(quads(&canvas)[0][0], [50.0, 50.0]);
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
    fn quads_after_a_3d_view_go_over_it() {
        use crate::graphics::Camera3d;

        let mut canvas = Canvas::new();
        canvas.begin(100.0, 100.0);
        canvas.fill_rect(Rect::new(0.0, 0.0, 1.0, 1.0), Color::WHITE);
        let camera = Camera3d::perspective([0.0, 0.0, 5.0], [0.0; 3], 1.0);
        let view = View3d::new(camera, Rect::new(10.0, 20.0, 30.0, 40.0));
        assert_eq!(canvas.begin_3d(view).view().rect, view.rect);
        // Solid like the one before, but in a batch of its own, after the view.
        canvas.fill_rect(Rect::new(2.0, 2.0, 1.0, 1.0), Color::WHITE);
        let batches: Vec<_> = canvas.batches.iter().map(|b| b.quads.clone()).collect();
        assert_eq!(batches, [0..1, 1..2]);
        assert_eq!(canvas.segments[0].before_batch, 1);
        assert_eq!(
            canvas.draw_call_count(),
            2,
            "a view with no meshes draws nothing"
        );

        // Its rect is placed by the canvas's camera, as everything else is.
        canvas.set_camera(Some(Camera {
            zoom: 2.0,
            ..Camera::new(50.0, 50.0)
        }));
        assert_eq!(
            canvas.begin_3d(view).view().rect,
            Rect::new(-30.0, -10.0, 60.0, 80.0)
        );

        canvas.clear(Color::BLACK);
        assert!(canvas.segments.is_empty());
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
