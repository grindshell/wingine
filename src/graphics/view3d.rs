use std::ops::Range;

use bytemuck::{Pod, Zeroable};

use super::{Camera3d, Canvas, Color, Mesh, Sprite, Texture};
use crate::math::{Affine3, Rect};

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
        let determinant = transform.determinant();
        if mesh.index_count() == 0 || !determinant.is_finite() {
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
            normal: transform.normal_matrix(),
            uv: [u0, v0, u1, v1],
            tint: tint.to_rgba8(),
        });
        canvas.mesh_draws.push(MeshDraw {
            mesh: mesh.clone(),
            texture: sprite.map(|sprite| sprite.texture().clone()),
            mirrored: determinant < 0.0,
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
}

/// A mesh recorded in a 3D view. Its transform and colors are its [`Instance`].
pub(crate) struct MeshDraw {
    pub mesh: Mesh,
    /// `None` draws with a plain white texture.
    pub texture: Option<Texture>,
    /// Whether its transform mirrors it, which turns its triangles' winding round.
    pub mirrored: bool,
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
