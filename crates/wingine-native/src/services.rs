//! The engine's services: the context, the canvas, and sprites.

use std::ptr::NonNull;

use crate::{Color, Error, Rect, Result, SpriteDraw, abi, host};

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

    /// Whether any of the keys bound to `action` is held.
    pub fn held(&self, action: &str) -> bool {
        // SAFETY: lent for as long as `self` is, and the name is valid for the call.
        unsafe { (host().action_held)(self.raw, abi::Str::new(action)) }
    }

    /// Whether any of the keys bound to `action` was pressed this frame.
    pub fn pressed(&self, action: &str) -> bool {
        // SAFETY: lent for as long as `self` is, and the name is valid for the call.
        unsafe { (host().action_pressed)(self.raw, abi::Str::new(action)) }
    }

    /// Whether any of the keys bound to `action` was released this frame.
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
