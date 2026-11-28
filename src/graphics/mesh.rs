use std::{
    f32::consts::{PI, TAU},
    fmt,
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::Color;
use crate::math::{Bounds3, add3, scale3};

/// A corner of a mesh's triangles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshVertex {
    pub position: [f32; 3],
    /// Which way the surface faces here, for lighting. Its length doesn't matter.
    pub normal: [f32; 3],
    /// Where on the sprite the mesh is drawn with this corner shows, from `[0.0, 0.0]` at the
    /// sprite's top-left to `[1.0, 1.0]` at its bottom-right.
    pub uv: [f32; 2],
    /// Multiplies the sprite's colors here, as a tint does.
    pub color: Color,
}

impl MeshVertex {
    /// A white corner at `position`, facing `normal`, showing `uv` of the sprite.
    pub const fn new(position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            position,
            normal,
            uv,
            color: Color::WHITE,
        }
    }
}

/// The triangles of a mesh, to upload as a [`Mesh`] with
/// [`Context::create_mesh`](crate::Context::create_mesh).
///
/// The built-in shapes are centered on the origin, white, and show the whole of a sprite on each
/// face, or once around a sphere.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeshData {
    pub vertices: Vec<MeshVertex>,
    /// Three indices into `vertices` for each triangle, counter-clockwise seen from the side it
    /// faces. Triangles are drawn only from that side.
    pub indices: Vec<u32>,
}

impl MeshData {
    /// A box `size` wide, tall and deep, along `x`, `y` and `z`. Each side shows the sprite upright
    /// seen from outside, and the top shows it with its top toward `-z`.
    pub fn cuboid(size: [f32; 3]) -> Self {
        let half = size.map(|v| v / 2.0);
        // Each side's normal, then the ways the sprite's right and its top point on it.
        let sides: [[[f32; 3]; 3]; 6] = [
            [[1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 1.0, 0.0]],
            [[-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]],
            [[0.0, 1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]],
            [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
            [[0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            [[0.0, 0.0, -1.0], [-1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        ];
        let mut mesh = Self::default();
        for [normal, right, up] in sides {
            let corner = |across: f32, along: f32| {
                let point = add3(add3(normal, scale3(right, across)), scale3(up, along));
                [0, 1, 2].map(|i| point[i] * half[i])
            };
            mesh.push_quad(
                normal,
                [
                    corner(-1.0, -1.0),
                    corner(1.0, -1.0),
                    corner(1.0, 1.0),
                    corner(-1.0, 1.0),
                ],
            );
        }
        mesh
    }

    /// A cube `size` along each side: see [`MeshData::cuboid`].
    pub fn cube(size: f32) -> Self {
        Self::cuboid([size; 3])
    }

    /// A flat rectangle facing up, `size` along `x` and `z`, which shows the sprite with its top
    /// toward `-z`.
    pub fn plane(size: [f32; 2]) -> Self {
        let [x, z] = size.map(|v| v / 2.0);
        let mut mesh = Self::default();
        mesh.push_quad(
            [0.0, 1.0, 0.0],
            [[-x, 0.0, z], [x, 0.0, z], [x, 0.0, -z], [-x, 0.0, -z]],
        );
        mesh
    }

    /// The most slices around and rings from top to bottom a [`MeshData::sphere`] has: more than
    /// any sphere needs to look round, and few enough that its vertices fit in memory.
    pub const MAX_SPHERE_DIVISIONS: u32 = 512;

    /// A sphere of `radius`, made of `segments` slices around (from 3 to
    /// [`MAX_SPHERE_DIVISIONS`](Self::MAX_SPHERE_DIVISIONS)) and `rings` from top to bottom (from 2
    /// to as many). The sprite wraps once around it, its left and right edges meeting at the back,
    /// toward `-z`, its top at the top.
    pub fn sphere(radius: f32, segments: u32, rings: u32) -> Self {
        let most = Self::MAX_SPHERE_DIVISIONS;
        let [segments, rings] = [segments.clamp(3, most), rings.clamp(2, most)];
        let mut mesh = Self::default();
        for ring in 0..=rings {
            let (sin_down, cos_down) = (PI * ring as f32 / rings as f32).sin_cos();
            for segment in 0..=segments {
                // From the back, round through `-x`, the front and `+x`, so the sprite shows
                // upright from outside.
                let (sin_around, cos_around) =
                    (TAU * segment as f32 / segments as f32 - PI).sin_cos();
                let normal = [sin_down * sin_around, cos_down, sin_down * cos_around];
                mesh.vertices.push(MeshVertex::new(
                    scale3(normal, radius),
                    normal,
                    [segment as f32 / segments as f32, ring as f32 / rings as f32],
                ));
            }
        }
        let row = segments + 1;
        for ring in 0..rings {
            for segment in 0..segments {
                let above = ring * row + segment;
                let below = above + row;
                // The top and bottom rings' corners at the poles are one point.
                if ring + 1 < rings {
                    mesh.indices.extend([below, below + 1, above + 1]);
                }
                if ring > 0 {
                    mesh.indices.extend([below, above + 1, above]);
                }
            }
        }
        mesh
    }

    /// How many triangles there are.
    pub fn triangle_count(&self) -> usize {
        self.indices.len() / 3
    }

    /// The box its vertices take up, or `None` if it has none.
    pub fn bounds(&self) -> Option<Bounds3> {
        Bounds3::from_points(self.vertices.iter().map(|vertex| vertex.position))
    }

    /// The quad billboards are drawn as: 1 wide and 1 tall, facing `+z`, with the middle of its
    /// bottom edge at the origin.
    pub(crate) fn billboard() -> Self {
        let mut mesh = Self::default();
        mesh.push_quad(
            [0.0, 0.0, 1.0],
            [
                [-0.5, 0.0, 0.0],
                [0.5, 0.0, 0.0],
                [0.5, 1.0, 0.0],
                [-0.5, 1.0, 0.0],
            ],
        );
        mesh
    }

    /// Adds a quad facing `normal`, its corners counter-clockwise seen from that side from the
    /// one showing the sprite's bottom-left.
    fn push_quad(&mut self, normal: [f32; 3], corners: [[f32; 3]; 4]) {
        let first = self.vertices.len() as u32;
        let uvs = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        self.vertices.extend(
            corners
                .into_iter()
                .zip(uvs)
                .map(|(position, uv)| MeshVertex::new(position, normal, uv)),
        );
        self.indices
            .extend([0, 1, 2, 0, 2, 3].map(|corner| first + corner));
    }

    /// Why the mesh can't be drawn, if it can't.
    fn problem(&self) -> Option<String> {
        if !self.indices.len().is_multiple_of(3) {
            return Some(format!(
                "{} indices, which isn't three for each triangle",
                self.indices.len()
            ));
        }
        let vertices = self.vertices.len();
        if u32::try_from(vertices).is_err() {
            return Some(format!("{vertices} vertices, more than indices reach"));
        }
        let out_of_range = self.indices.iter().find(|&&i| i as usize >= vertices)?;
        Some(format!(
            "index {out_of_range} is past the last of {vertices} vertices"
        ))
    }
}

/// How a corner is stored on the GPU: 36 bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct GpuVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// sRGB RGBA, which the shader converts to linear space.
    pub color: [u8; 4],
}

impl GpuVertex {
    /// A corner as the GPU stores it.
    fn of(vertex: MeshVertex) -> Self {
        Self {
            position: vertex.position,
            normal: vertex.normal,
            uv: vertex.uv,
            color: vertex.color.to_rgba8(),
        }
    }
}

/// A mesh uploaded to the GPU, created with [`Context::create_mesh`](crate::Context::create_mesh)
/// and drawn with [`Canvas3d::draw_mesh`](super::Canvas3d::draw_mesh). Its corners can be moved
/// with [`Context::write_mesh`](crate::Context::write_mesh).
///
/// Cloning is cheap; clones share the same GPU buffers, so writing one writes them all.
#[derive(Clone)]
pub struct Mesh {
    inner: Arc<MeshInner>,
}

struct MeshInner {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    vertex_count: u32,
    index_count: u32,
    written: Mutex<Written>,
    /// Whether the GPU has yet to be given the vertices last written. Read for every draw call
    /// of the mesh, so it's beside the lock, not behind it.
    stale: AtomicBool,
    /// Whether it's been written since it was made, which leaves behind `made` and the box of a
    /// model it's a part of ([`Model::bounds`](super::Model::bounds)).
    rewritten: AtomicBool,
    /// The box its vertices took up as it was made, which views ask of every mesh drawn, so
    /// it's beside the lock, not behind it.
    made: Option<Bounds3>,
    #[cfg(feature = "physics")]
    geometry: Geometry,
}

/// What a mesh was last written with, and the box its vertices take up.
struct Written {
    /// The vertices last written, as the GPU stores them, kept for the next frame that draws
    /// the mesh and for its bounds, and written over by the next write, which so allocates
    /// nothing. Empty until a write.
    vertices: Vec<GpuVertex>,
    /// The box the mesh's vertices took up as it was made, or as `vertices` have it once it's
    /// been asked for.
    bounds: Option<Bounds3>,
    /// Whether `bounds` is yet to be worked out from `vertices`: it costs nearly as much as
    /// the write, and a mesh written every frame is seldom asked for it.
    unmeasured: bool,
}

/// Where a mesh's corners are and which make its triangles, kept on the CPU when physics needs
/// them, as colliders shaped like what nodes show.
#[cfg(feature = "physics")]
#[derive(Debug, Default)]
pub(crate) struct Geometry {
    pub points: Vec<[f32; 3]>,
    /// Three indices into `points` for each triangle, counter-clockwise seen from the side it
    /// faces.
    pub triangles: Vec<[u32; 3]>,
}

impl Mesh {
    pub(crate) fn new(device: &wgpu::Device, data: &MeshData) -> Result<Self, String> {
        if let Some(problem) = data.problem() {
            return Err(problem);
        }
        let vertices: Vec<GpuVertex> = data.vertices.iter().copied().map(GpuVertex::of).collect();
        let buffer = |label, contents, usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
        };
        Ok(Self {
            inner: Arc::new(MeshInner {
                // Copied to by `upload`, once the mesh is written.
                vertices: buffer(
                    "mesh vertices",
                    bytemuck::cast_slice(&vertices),
                    wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                ),
                indices: buffer(
                    "mesh indices",
                    bytemuck::cast_slice(&data.indices),
                    wgpu::BufferUsages::INDEX,
                ),
                vertex_count: data.vertices.len() as u32,
                index_count: data.indices.len() as u32,
                written: Mutex::new(Written {
                    vertices: Vec::new(),
                    bounds: data.bounds(),
                    unmeasured: false,
                }),
                stale: AtomicBool::new(false),
                rewritten: AtomicBool::new(false),
                made: data.bounds(),
                #[cfg(feature = "physics")]
                geometry: Geometry {
                    points: data.vertices.iter().map(|vertex| vertex.position).collect(),
                    triangles: data.indices.as_chunks::<3>().0.to_vec(),
                },
            }),
        })
    }

    /// Its corners and triangles, as it was made.
    #[cfg(feature = "physics")]
    pub(crate) fn geometry(&self) -> &Geometry {
        &self.inner.geometry
    }

    /// How many triangles there are.
    pub fn triangle_count(&self) -> u32 {
        self.inner.index_count / 3
    }

    /// The box its vertices take up, as it was made or last written, or `None` if it has none.
    /// After a write it's worked out the first time it's asked for.
    pub fn bounds(&self) -> Option<Bounds3> {
        if !self.is_rewritten() {
            return self.inner.made;
        }
        let mut written = self.written();
        if written.unmeasured {
            let points = written.vertices.iter().map(|vertex| vertex.position);
            written.bounds = Bounds3::from_points(points);
            written.unmeasured = false;
        }
        written.bounds
    }

    /// Writes `vertices` over the mesh's own, as many as it was made with: its triangles stay
    /// between the same corners, wherever those now are. They're kept here, and reach the GPU
    /// when a frame that draws the mesh is next rendered (`upload`), so a mesh written many
    /// times between frames, as in a simulation that renders none, uploads only the last.
    pub(crate) fn write(
        &self,
        vertices: impl ExactSizeIterator<Item = MeshVertex>,
    ) -> Result<(), String> {
        let (given, made_with) = (vertices.len(), self.inner.vertex_count as usize);
        if given != made_with {
            return Err(format!(
                "{given} vertices written to a mesh made with {made_with}"
            ));
        }
        let mut written = self.written();
        written.vertices.clear();
        written.vertices.extend(vertices.map(GpuVertex::of));
        written.unmeasured = true;
        // Set under the lock, as `upload` clears it, so a write is never taken for uploaded.
        self.inner.stale.store(true, Ordering::Release);
        self.inner.rewritten.store(true, Ordering::Relaxed);
        Ok(())
    }

    /// Whether it's been written since it was made, so its vertices may have left the box they
    /// took up then.
    pub(crate) fn is_rewritten(&self) -> bool {
        self.inner.rewritten.load(Ordering::Relaxed)
    }

    /// Gives the GPU the vertices last written, if it hasn't them yet. The renderer calls it for
    /// every mesh a frame draws, before it submits the frame, whose draws then show them.
    pub(crate) fn upload(&self, queue: &wgpu::Queue) {
        if !self.inner.stale.load(Ordering::Acquire) {
            return;
        }
        let written = self.written();
        self.inner.stale.store(false, Ordering::Release);
        if !written.vertices.is_empty() {
            let bytes = bytemuck::cast_slice(&written.vertices);
            queue.write_buffer(&self.inner.vertices, 0, bytes);
        }
    }

    /// What it was last written with. A panic while it was held left it whole enough: the next
    /// write clears it.
    fn written(&self) -> MutexGuard<'_, Written> {
        self.inner
            .written
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub(crate) fn index_count(&self) -> u32 {
        self.inner.index_count
    }

    /// What tells the mesh on the GPU from others while handles to it are kept: handles to the
    /// same mesh have the same. Never 0.
    pub(crate) fn id(&self) -> usize {
        Arc::as_ptr(&self.inner).addr()
    }

    pub(crate) fn vertex_buffer(&self) -> &wgpu::Buffer {
        &self.inner.vertices
    }

    pub(crate) fn index_buffer(&self) -> &wgpu::Buffer {
        &self.inner.indices
    }
}

impl fmt::Debug for Mesh {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mesh")
            .field("triangles", &self.triangle_count())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{cross3, dot3, sub3};

    /// The corners of each triangle.
    fn triangles(mesh: &MeshData) -> Vec<[[f32; 3]; 3]> {
        mesh.indices
            .chunks(3)
            .map(|triangle| [0, 1, 2].map(|i| mesh.vertices[triangle[i] as usize].position))
            .collect()
    }

    /// Which way a triangle faces, by its winding, scaled by its area.
    fn facing([a, b, c]: [[f32; 3]; 3]) -> [f32; 3] {
        cross3(sub3(b, a), sub3(c, a))
    }

    fn middle([a, b, c]: [[f32; 3]; 3]) -> [f32; 3] {
        [0, 1, 2].map(|i| (a[i] + b[i] + c[i]) / 3.0)
    }

    fn assert_faces_out_from_its_middle(mesh: &MeshData) {
        assert_eq!(mesh.problem(), None);
        for triangle in triangles(mesh) {
            let facing = facing(triangle);
            assert!(dot3(facing, facing) > 1e-12, "{triangle:?} has no area");
            assert!(
                dot3(facing, middle(triangle)) > 0.0,
                "{triangle:?} faces in"
            );
        }
        for vertex in &mesh.vertices {
            let length = dot3(vertex.normal, vertex.normal).sqrt();
            assert!((length - 1.0).abs() < 1e-5, "{vertex:?}");
            assert!(
                dot3(vertex.normal, vertex.position) > 0.0,
                "{vertex:?} faces in"
            );
        }
    }

    #[test]
    fn cuboids_have_six_sides_facing_out() {
        let mesh = MeshData::cuboid([2.0, 4.0, 6.0]);
        assert_eq!(mesh.vertices.len(), 24);
        assert_eq!(mesh.triangle_count(), 12);
        assert_faces_out_from_its_middle(&mesh);
        for vertex in &mesh.vertices {
            assert_eq!(vertex.position.map(f32::abs), [1.0, 2.0, 3.0]);
        }
    }

    #[test]
    fn cuboid_sides_show_the_sprite_upright() {
        let mesh = MeshData::cube(2.0);
        // On the front, the sprite's top-left is at the top-left seen from the front.
        let front = &mesh.vertices[16..20];
        assert!(front.iter().all(|vertex| vertex.normal == [0.0, 0.0, 1.0]));
        let top_left = front.iter().find(|vertex| vertex.uv == [0.0, 0.0]).unwrap();
        assert_eq!(top_left.position, [-1.0, 1.0, 1.0]);
        // On the top, its top is toward -z.
        let top = &mesh.vertices[8..12];
        assert!(top.iter().all(|vertex| vertex.normal == [0.0, 1.0, 0.0]));
        let top_left = top.iter().find(|vertex| vertex.uv == [0.0, 0.0]).unwrap();
        assert_eq!(top_left.position, [-1.0, 1.0, -1.0]);
    }

    #[test]
    fn planes_face_up() {
        let mesh = MeshData::plane([4.0, 2.0]);
        assert_eq!(mesh.problem(), None);
        assert_eq!(mesh.triangle_count(), 2);
        for triangle in triangles(&mesh) {
            let facing = facing(triangle);
            assert!(facing[1] > 0.0 && facing[0] == 0.0 && facing[2] == 0.0);
        }
        let top_left = mesh.vertices.iter().find(|v| v.uv == [0.0, 0.0]).unwrap();
        assert_eq!(top_left.position, [-2.0, 0.0, -1.0]);
    }

    #[test]
    fn spheres_close_round_and_face_out() {
        let mesh = MeshData::sphere(2.0, 8, 4);
        assert_eq!(mesh.vertices.len(), 9 * 5);
        // Two triangles for each of the 8 segments of the middle two rings, one at each pole.
        assert_eq!(mesh.triangle_count(), 8 * 2 * 2 + 8 * 2);
        assert_faces_out_from_its_middle(&mesh);
        for vertex in &mesh.vertices {
            let distance = dot3(vertex.position, vertex.position).sqrt();
            assert!((distance - 2.0).abs() < 1e-5);
        }
        // The sprite's middle faces the front.
        let front = mesh.vertices.iter().find(|v| v.uv == [0.5, 0.5]).unwrap();
        assert!((front.position[2] - 2.0).abs() < 1e-5);
        // Too few slices or rings for a shape get the fewest that make one, and too many the most.
        assert_eq!(MeshData::sphere(1.0, 0, 0).triangle_count(), 3 * 2);
        let most = MeshData::MAX_SPHERE_DIVISIONS as usize;
        let huge = MeshData::sphere(1.0, u32::MAX, u32::MAX);
        assert_eq!(huge.vertices.len(), (most + 1) * (most + 1));
        assert_eq!(huge.problem(), None);
    }

    #[test]
    fn meshes_that_cant_be_drawn_say_why() {
        let mut mesh = MeshData::cube(1.0);
        mesh.indices.pop();
        assert!(mesh.problem().unwrap().contains("35 indices"));
        let mut mesh = MeshData::cube(1.0);
        mesh.indices[5] = 24;
        assert!(mesh.problem().unwrap().contains("index 24"));
        assert_eq!(MeshData::default().problem(), None);
    }

    #[test]
    fn bounds_hold_the_vertices() {
        let bounds = MeshData::cuboid([2.0, 4.0, 6.0]).bounds().unwrap();
        assert_eq!(bounds.min, [-1.0, -2.0, -3.0]);
        assert_eq!(bounds.max, [1.0, 2.0, 3.0]);
        let flat = MeshData::plane([4.0, 2.0]).bounds().unwrap();
        assert_eq!(flat.size(), [4.0, 0.0, 2.0]);
        assert_eq!(MeshData::default().bounds(), None);
    }
}
