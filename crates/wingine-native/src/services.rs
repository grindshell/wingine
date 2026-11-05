//! The engine's services: the context, the canvas, sprites and meshes.

use std::ptr::{self, NonNull};

use crate::{
    BillboardDraw, Color, Error, MeshDraw, MeshVertex, Rect, Result, SpriteDraw, View3d, abi,
    check, host,
};

/// The engine's services, lent while a game loads and updates: input, the canvas size, and
/// creating sprites.
pub struct Context {
    raw: *mut abi::Context,
}

impl Context {
    pub(crate) fn new(raw: *mut abi::Context) -> Self {
        Self { raw }
    }

    /// Width and height of the canvas the frame will be drawn on, in canvas pixels.
    pub fn canvas_size(&self) -> [f32; 2] {
        // SAFETY: lent for as long as `self` is.
        let size = unsafe { (host().context_canvas_size)(self.raw) };
        [size.width, size.height]
    }

    /// Whether any of the keys and gamepad buttons bound to `action` is held.
    pub fn held(&self, action: &str) -> bool {
        // SAFETY: lent for as long as `self` is, and the name is valid for the call.
        unsafe { (host().action_held)(self.raw, abi::Str::new(action)) }
    }

    /// Whether any of the keys and gamepad buttons bound to `action` was pressed this frame.
    pub fn pressed(&self, action: &str) -> bool {
        // SAFETY: lent for as long as `self` is, and the name is valid for the call.
        unsafe { (host().action_pressed)(self.raw, abi::Str::new(action)) }
    }

    /// Whether any of the keys and gamepad buttons bound to `action` was released this frame.
    pub fn released(&self, action: &str) -> bool {
        // SAFETY: lent for as long as `self` is, and the name is valid for the call.
        unsafe { (host().action_released)(self.raw, abi::Str::new(action)) }
    }

    /// The cursor's position in canvas pixels, or `None` when it's outside the window.
    pub fn mouse_position(&self) -> Option<[f32; 2]> {
        let mut point = abi::Point::default();
        // SAFETY: lent for as long as `self` is.
        let inside = unsafe { (host().mouse_position)(self.raw, &mut point) };
        inside.then_some([point.x, point.y])
    }

    /// Creates a sprite from `width` by `height` pixels: sRGB RGBA bytes, row by row from the
    /// top.
    pub fn create_sprite(&mut self, width: u32, height: u32, rgba: &[u8]) -> Result<Sprite> {
        let expected = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4));
        if expected != Some(rgba.len()) {
            return Err(Error::new(format!(
                "{width}x{height} pixels take {} bytes, not {}",
                u64::from(width) * u64::from(height) * 4,
                rgba.len()
            )));
        }
        // SAFETY: lent for as long as `self` is, and the pixels are the size the engine reads.
        let raw = unsafe { (host().create_sprite)(self.raw, width, height, rgba.as_ptr()) };
        NonNull::new(raw)
            .map(|raw| Sprite { raw })
            .ok_or_else(Error::from_engine)
    }

    /// Creates a mesh of `vertices` and `indices` into them, three for each triangle,
    /// counter-clockwise seen from the side it faces, which is the only side that shows. Fails if
    /// the indices aren't three for each triangle or reach past the vertices.
    pub fn create_mesh(&mut self, vertices: &[MeshVertex], indices: &[u32]) -> Result<Mesh> {
        // SAFETY: lent for as long as `self` is, and the slices are valid for the call.
        let raw = unsafe {
            (host().create_mesh)(
                self.raw,
                vertices.as_ptr(),
                vertices.len(),
                indices.as_ptr(),
                indices.len(),
            )
        };
        NonNull::new(raw)
            .map(|raw| Mesh { raw })
            .ok_or_else(Error::from_engine)
    }
}

/// What a game draws on, lent while it draws.
pub struct Canvas {
    raw: *mut abi::Canvas,
}

impl Canvas {
    pub(crate) fn new(raw: *mut abi::Canvas) -> Self {
        Self { raw }
    }

    /// The canvas's width and height, in canvas pixels.
    pub fn size(&self) -> [f32; 2] {
        // SAFETY: lent for as long as `self` is.
        let size = unsafe { (host().canvas_size)(self.raw) };
        [size.width, size.height]
    }

    /// Fills `rect` with `color`, in the coordinates of whatever camera the game set.
    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        // SAFETY: lent for as long as `self` is.
        unsafe { (host().fill_rect)(self.raw, rect, color) };
    }

    /// Draws `sprite` stretched over `dest`, its colors multiplied by `tint`.
    pub fn draw_sprite(&mut self, sprite: &Sprite, dest: Rect, tint: Color) {
        // SAFETY: lent for as long as `self` is, and the sprite is alive.
        unsafe { (host().draw_sprite)(self.raw, sprite.raw.as_ptr(), dest, tint) };
    }

    /// Draws `sprite` as each of `draws` says, in order, each one stretched, rotated and tinted
    /// as it says. One call draws them all, so this is how to draw many sprites.
    pub fn draw_sprites(&mut self, sprite: &Sprite, draws: &[SpriteDraw]) {
        // SAFETY: lent for as long as `self` is, the sprite is alive, and the draws are valid for
        // the call.
        unsafe {
            (host().draw_sprites)(self.raw, sprite.raw.as_ptr(), draws.as_ptr(), draws.len());
        }
    }

    /// Begins a 3D view over what's been drawn: the meshes and billboards drawn after it go in it,
    /// nearer ones hiding farther ones, until anything else is drawn, which goes over it. Fails if
    /// its camera's projection, `near` or `far` aren't ones [`Camera3d`](crate::Camera3d) allows.
    pub fn begin_3d(&mut self, view: &View3d) -> Result<()> {
        // SAFETY: lent for as long as `self` is, and the view is valid for the call.
        check(unsafe { (host().begin_3d)(self.raw, view) })
    }

    /// Draws `mesh` as each of `draws` says, showing `sprite`, or white without one, in the 3D
    /// view begun last, whether the library or the script began it. One call draws them all, a
    /// draw call for each. Fails if no view is open: if none was begun this frame, or anything
    /// else was drawn since.
    pub fn draw_meshes(
        &mut self,
        mesh: &Mesh,
        sprite: Option<&Sprite>,
        draws: &[MeshDraw],
    ) -> Result<()> {
        let sprite = sprite.map_or(ptr::null(), Sprite::as_ptr);
        // SAFETY: lent for as long as `self` is, the mesh and sprite are alive, and the draws are
        // valid for the call.
        check(unsafe {
            (host().draw_meshes)(self.raw, mesh.as_ptr(), sprite, draws.as_ptr(), draws.len())
        })
    }

    /// Draws `sprite` standing upright as each of `draws` says, facing the camera across the
    /// ground, in the 3D view begun last, as [`draw_meshes`](Self::draw_meshes) does. Pixels of
    /// the sprite less than half opaque aren't drawn; the rest are drawn opaque.
    pub fn draw_billboards(&mut self, sprite: &Sprite, draws: &[BillboardDraw]) -> Result<()> {
        // SAFETY: lent for as long as `self` is, the sprite is alive, and the draws are valid for
        // the call.
        check(unsafe {
            (host().draw_billboards)(self.raw, sprite.as_ptr(), draws.as_ptr(), draws.len())
        })
    }
}

/// Triangles on the GPU, to draw in 3D views: the engine's mesh, which scripts hold too. Clones
/// share the triangles.
pub struct Mesh {
    raw: NonNull<abi::Mesh>,
}

impl Mesh {
    /// A handle to the mesh at `mesh`, which the engine lent.
    pub(crate) fn copy(mesh: *const abi::Mesh) -> Self {
        // SAFETY: the engine lent the mesh, and returns a handle the library owns.
        let raw = unsafe { (host().clone_mesh)(mesh) };
        Self {
            raw: NonNull::new(raw).expect("copying a mesh always works"),
        }
    }

    pub(crate) fn as_ptr(&self) -> *const abi::Mesh {
        self.raw.as_ptr()
    }
}

impl Clone for Mesh {
    fn clone(&self) -> Self {
        Self::copy(self.raw.as_ptr())
    }
}

impl Drop for Mesh {
    fn drop(&mut self) {
        // SAFETY: the library owns the handle, and frees it once.
        unsafe { (host().free_mesh)(self.raw.as_ptr()) };
    }
}

/// An image, or part of one, to draw: the engine's sprite, which scripts hold too.
pub struct Sprite {
    raw: NonNull<abi::Sprite>,
}

impl Sprite {
    /// A copy of the sprite at `sprite`, which the engine lent.
    pub(crate) fn copy(sprite: *const abi::Sprite) -> Self {
        // SAFETY: the engine lent the sprite, and returns a copy the library owns.
        let raw = unsafe { (host().clone_sprite)(sprite) };
        Self {
            raw: NonNull::new(raw).expect("copying a sprite always works"),
        }
    }

    pub(crate) fn as_ptr(&self) -> *const abi::Sprite {
        self.raw.as_ptr()
    }

    /// Its width and height in texels.
    pub fn size(&self) -> [f32; 2] {
        // SAFETY: the sprite is alive.
        let size = unsafe { (host().sprite_size)(self.raw.as_ptr()) };
        [size.width, size.height]
    }
}

impl Clone for Sprite {
    fn clone(&self) -> Self {
        Self::copy(self.raw.as_ptr())
    }
}

impl Drop for Sprite {
    fn drop(&mut self) {
        // SAFETY: the library owns the sprite, and frees it once.
        unsafe { (host().free_sprite)(self.raw.as_ptr()) };
    }
}
