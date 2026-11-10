use std::ops::Range;

use bytemuck::{Pod, Zeroable};

use super::{Camera3d, Canvas, Color, Mesh, Model, Projection, Ray, Sprite, Texture};
use crate::math::{Affine3, Rect, dot3, scale3, sub3};

/// What a 3D view shows, where on the canvas, and how it's lit: see [`Canvas::begin_3d`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View3d {
    pub camera: Camera3d,
    /// Where on the canvas the view shows, in the coordinates drawing uses when it begins: the
    /// canvas's camera moves and zooms it, as it does everything else.
    pub rect: Rect,
    pub lighting: Lighting,
}

impl View3d {
    /// A view through `camera` filling `rect`, lit by [`Lighting::default`].
    pub fn new(camera: Camera3d, rect: Rect) -> Self {
        Self {
            camera,
            rect,
            lighting: Lighting::default(),
        }
    }

    /// Where `point` in the view's world shows, in the coordinates its `rect` is in, as
    /// [`Camera3d::project`] places it on the rect. `None` if it's nearer than the near plane or
    /// farther than the far one.
    pub fn project(&self, point: [f32; 3]) -> Option<[f32; 2]> {
        let rect = self.rect;
        let [x, y] = self.camera.project(point, [rect.w, rect.h])?;
        Some([rect.x + x, rect.y + y])
    }

    /// The ray through `point`, in the coordinates its `rect` is in, into the view's world, as
    /// [`Camera3d::ray`] gives it. Points outside the rect have rays too, as though it went on.
    pub fn ray(&self, [x, y]: [f32; 2]) -> Ray {
        let rect = self.rect;
        self.camera.ray([x - rect.x, y - rect.y], [rect.w, rect.h])
    }
}

/// How a 3D view is lit: by a sun, whose light comes from one direction, and by ambient light from
/// all around. Colors multiply what surfaces show, so white lights them as they are, and
/// together they light them no brighter than that.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lighting {
    /// The way the sun's light goes: `[0.0, -1.0, 0.0]` straight down. Its length doesn't matter.
    pub direction: [f32; 3],
    /// The sun's light on surfaces facing it, less on those turned partly away.
    pub color: Color,
    /// Light on every surface, even those facing away from the sun.
    pub ambient: Color,
}

impl Lighting {
    /// No shading: every surface shows as it is.
    pub const UNLIT: Self = Self {
        direction: [0.0, -1.0, 0.0],
        color: Color::BLACK,
        ambient: Color::WHITE,
    };
}

impl Default for Lighting {
    /// A sun high above and a little in front, to the right, so the tops of things are brightest,
    /// then their fronts, then their right-hand sides.
    fn default() -> Self {
        Self {
            direction: [-0.3, -1.0, -0.5],
            color: Color::rgb(0.8, 0.8, 0.8),
            ambient: Color::rgb(0.5, 0.5, 0.5),
        }
    }
}

/// Draws meshes into a 3D view on a canvas, from [`Canvas::begin_3d`]. The view is finished when
/// this is dropped, and what's drawn on the canvas after goes over it.
///
/// Meshes hide what's behind them, whatever order they're drawn in.
pub struct Canvas3d<'a> {
    pub(super) canvas: &'a mut Canvas,
}

impl Canvas3d<'_> {
    /// Draws `mesh` placed by `transform`, showing `sprite`, or white without one, multiplied by
    /// the mesh's colors and by `tint`. Pixels of the sprite less than half opaque aren't drawn,
    /// so what's behind shows through them; the rest are drawn opaque.
    pub fn draw_mesh(
        &mut self,
        mesh: &Mesh,
        transform: Affine3,
        sprite: Option<&Sprite>,
        tint: Color,
    ) {
        self.draw_sides(mesh, transform, sprite, tint, false);
    }

    /// Draws each part of `model` as [`draw_mesh`](Self::draw_mesh) would, placed in the model,
    /// and the model placed by `transform`, all multiplied by `tint`: a draw call for each part.
    /// Double-sided parts show from behind too.
    pub fn draw_model(&mut self, model: &Model, transform: Affine3, tint: Color) {
        for part in model.parts() {
            let placed = transform * part.transform;
            let sprite = part.sprite.as_ref();
            self.draw_sides(&part.mesh, placed, sprite, tint, part.double_sided);
        }
    }

    /// Draws `mesh` as [`draw_mesh`](Self::draw_mesh) does, from behind too if `double_sided`.
    fn draw_sides(
        &mut self,
        mesh: &Mesh,
        transform: Affine3,
        sprite: Option<&Sprite>,
        tint: Color,
        double_sided: bool,
    ) {
        if mesh.index_count() > 0 {
            let normal = transform.normal_matrix();
            self.push(Some(mesh), transform, normal, sprite, tint, double_sided);
        }
    }

    /// Draws `sprite` standing upright with the middle of its bottom edge at `position`, `size`
    /// world units wide and tall, turned to face the camera across the ground, as characters do
    /// in 2.5D games: it stays upright in the world however far the camera looks down, and turns
    /// with the world in the view as the camera rolls. One a camera in perspective would see from
    /// behind, as below one looking steeply down, turns around to face it. A negative width
    /// mirrors it. It's lit as the ground is, whichever way it faces, and hides what's behind it as
    /// meshes do, so pixels of the sprite less than half opaque aren't drawn.
    pub fn draw_billboard(
        &mut self,
        sprite: &Sprite,
        position: [f32; 3],
        size: [f32; 2],
        tint: Color,
    ) {
        if size[0] == 0.0 || size[1] == 0.0 {
            return;
        }
        let segment = self.canvas.segments.last().expect("begin_3d adds one");
        let (mut right, mut facing) = segment.billboard_axes;
        // A camera in perspective sees one behind it across the ground from behind, as when it
        // looks steeply down on one below it, so that one turns around to face it.
        let camera = &segment.view.camera;
        if let Projection::Perspective { .. } = camera.projection
            && dot3(sub3(camera.position, position), facing) < 0.0
        {
            right = scale3(right, -1.0);
            facing = scale3(facing, -1.0);
        }
        let transform = Affine3 {
            x_axis: scale3(right, size[0]),
            y_axis: [0.0, size[1], 0.0],
            z_axis: facing,
            translation: position,
        };
        // Whichever way the quad faces, its normal points up, as the ground's do.
        let normal = [[0.0; 3], [0.0; 3], [0.0, 1.0, 0.0]];
        self.push(None, transform, normal, Some(sprite), tint, false);
    }

    /// Records `mesh`, or the billboard quad for `None`, placed by `transform`, with `normal`
    /// moving its normals, showing from behind too if `double_sided`.
    fn push(
        &mut self,
        mesh: Option<&Mesh>,
        transform: Affine3,
        normal: [[f32; 3]; 3],
        sprite: Option<&Sprite>,
        tint: Color,
        double_sided: bool,
    ) {
        let determinant = transform.determinant();
        if !determinant.is_finite() {
            return;
        }
        let ([u0, v0], [u1, v1]) = sprite.map_or(([0.0, 0.0], [1.0, 1.0]), Sprite::uv_bounds);
        let canvas = &mut *self.canvas;
        canvas.instances.push(Instance {
            model: [
                transform.x_axis,
                transform.y_axis,
                transform.z_axis,
                transform.translation,
            ],
            normal,
            uv: [u0, v0, u1, v1],
            tint: tint.to_rgba8(),
        });
        canvas.mesh_draws.push(MeshDraw {
            mesh: mesh.cloned(),
            texture: sprite.map(|sprite| sprite.texture().clone()),
            mirrored: determinant < 0.0,
            double_sided,
        });
        let segment = canvas.segments.last_mut().expect("begin_3d adds a segment");
        segment.draws.end += 1;
    }

    /// The view being drawn, with its rect in canvas pixels.
    pub fn view(&self) -> &View3d {
        &self.canvas.segments.last().expect("begin_3d adds one").view
    }
}

/// A 3D view recorded on a canvas, drawn between its quads.
pub(crate) struct Segment3d {
    /// Its rect in canvas pixels.
    pub view: View3d,
    /// The batch of quads it's drawn before, which begins after it.
    pub before_batch: u32,
    /// Its meshes, in the canvas's `mesh_draws`.
    pub draws: Range<u32>,
    /// The ways its billboards' right and front point ([`Camera3d::billboard_axes`]), the same
    /// for all of them.
    pub billboard_axes: ([f32; 3], [f32; 3]),
}

/// A mesh recorded in a 3D view. Its transform and colors are its [`Instance`].
pub(crate) struct MeshDraw {
    /// `None` for a billboard, which draws the renderer's quad.
    pub mesh: Option<Mesh>,
    /// `None` draws with a plain white texture.
    pub texture: Option<Texture>,
    /// Whether its transform mirrors it, which turns its triangles' winding round.
    pub mirrored: bool,
    /// Whether its triangles show from behind too.
    pub double_sided: bool,
}

impl MeshDraw {
    /// Whether `next`, drawn right after this, is drawn in the same draw call, as instances of
    /// one mesh: it's the same mesh, showing the same texture, drawn the same way round.
    pub fn joins(&self, next: &MeshDraw) -> bool {
        let same_mesh = match (&self.mesh, &next.mesh) {
            (Some(mesh), Some(next)) => mesh.same_as(next),
            (mesh, next) => mesh.is_none() && next.is_none(),
        };
        let same_texture = match (&self.texture, &next.texture) {
            (Some(texture), Some(next)) => texture.same_as(next),
            (texture, next) => texture.is_none() && next.is_none(),
        };
        same_mesh
            && same_texture
            && self.mirrored == next.mirrored
            && self.double_sided == next.double_sided
    }
}

/// How a mesh drawn is placed and colored, as the GPU reads it: 104 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct Instance {
    /// The transform's axes, then its translation.
    pub model: [[f32; 3]; 4],
    /// The columns of what moves its normals.
    pub normal: [[f32; 3]; 3],
    /// The sprite's texture coordinates, the top-left's then the bottom-right's.
    pub uv: [f32; 4],
    /// sRGB RGBA.
    pub tint: [u8; 4],
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near<const N: usize>(actual: [f32; N], expected: [f32; N]) {
        assert!(
            (0..N).all(|i| (actual[i] - expected[i]).abs() < 1e-4),
            "{actual:?} is not {expected:?}"
        );
    }

    #[test]
    fn views_place_points_and_rays_where_their_rects_are() {
        // Twice as wide as it's tall, so it shows 4 units across, 10 pixels each.
        let camera = Camera3d::orthographic([0.0, 0.0, 5.0], [0.0; 3], 2.0);
        let view = View3d::new(camera, Rect::new(10.0, 20.0, 40.0, 20.0));
        assert_near(view.project([0.0; 3]).unwrap(), [30.0, 30.0]);
        assert_near(view.project([2.0, 1.0, 0.0]).unwrap(), [50.0, 20.0]);
        assert_eq!(view.project([0.0, 0.0, 10.0]), None, "behind the camera");

        let middle = view.ray([30.0, 30.0]);
        assert_near(middle.origin, [0.0, 0.0, 4.9]);
        assert_near(middle.direction, [0.0, 0.0, -1.0]);
        // Past the rect's right edge, as though the view went on.
        assert_near(view.ray([70.0, 30.0]).origin, [4.0, 0.0, 4.9]);
    }
}
