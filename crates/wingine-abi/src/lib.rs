//! The binary interface between Wingine and the native libraries its Luau runtime loads.
//!
//! Libraries don't use this crate directly: they're written with `wingine-native`, which wraps
//! it. Everything here crosses between separately compiled binaries, so it's `#[repr(C)]` and
//! changes only with the ABI version. Additions bump [`ABI_MINOR`]; anything else bumps
//! [`ABI_MAJOR`]. The engine loads a library built for the same major version and the same or an
//! older minor version.
//!
//! A library exports one function, [`ENTRY_SYMBOL`], which returns a static [`Library`]. The
//! engine checks its version, then calls its `register` once per process with the [`Host`]: a
//! table of everything the engine offers, as function pointers, so libraries link nothing from
//! the engine. `register` describes the library through the registration functions: its global
//! functions, its classes of objects and their methods, and the callbacks the engine makes for
//! each game, from creating the library's state for the game to hooks around `update` and
//! `draw`.
//!
//! The rules every callback follows:
//!
//! - Pointers the engine passes in, and strings and objects reached through them, are valid only
//!   until the callback returns. Pointers a library passes to the engine are valid only for that
//!   call; the engine copies what it keeps.
//! - Memory is freed by whoever allocated it, since each binary has its own allocator.
//! - A callback that fails sets its error with [`Host::set_error`] and returns [`Status::ERROR`].
//!   Functions the engine provides fail the same way, and [`Host::error`] reads their error.
//! - Panics must not unwind into the engine.
//! - A game's callbacks run on the thread running that game. Several games may run in one
//!   process, on different threads, so whatever a library keeps outside its per-game state must
//!   be thread-safe, including the `data` passed with each registered callback.

#![no_std]

use core::{
    ffi::c_void,
    marker::{PhantomData, PhantomPinned},
};

pub const ABI_MAJOR: u16 = 0;
/// 1: the first version. 2: [`Host::draw_sprites`].
pub const ABI_MINOR: u16 = 2;

/// The name of the function a library exports, an [`Entry`].
pub const ENTRY_SYMBOL: &[u8] = b"wingine_library\0";

/// The function a library exports as [`ENTRY_SYMBOL`].
pub type Entry = unsafe extern "C" fn() -> *const Library;

/// How a library describes itself.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Library {
    /// The ABI version the library was built for.
    pub abi_major: u16,
    pub abi_minor: u16,
    /// The name of the library's global table in Luau: a Luau identifier.
    pub name: Str,
    /// Registers the library's functions, classes and callbacks, once per process.
    pub register: unsafe extern "C" fn(host: *const Host, registry: *mut Registry) -> Status,
}

// The name is a static string.
unsafe impl Send for Library {}
unsafe impl Sync for Library {}

/// Whether a call succeeded.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status(pub u32);

impl Status {
    pub const OK: Self = Self(0);
    pub const ERROR: Self = Self(1);
}

/// When a hook runs: before or after the game's own `update` or `draw`.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Phase(pub u32);

impl Phase {
    pub const BEFORE: Self = Self(0);
    pub const AFTER: Self = Self(1);
}

/// How important a logged message is, as the `log` crate's levels.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Level(pub u32);

impl Level {
    pub const ERROR: Self = Self(1);
    pub const WARN: Self = Self(2);
    pub const INFO: Self = Self(3);
    pub const DEBUG: Self = Self(4);
    pub const TRACE: Self = Self(5);
}

/// A borrowed UTF-8 string, not NUL-terminated.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Str {
    pub ptr: *const u8,
    pub len: usize,
}

impl Str {
    pub const fn new(text: &str) -> Self {
        Self {
            ptr: text.as_ptr(),
            len: text.len(),
        }
    }

    /// The string's bytes, which should be UTF-8.
    ///
    /// # Safety
    ///
    /// `ptr` must point to `len` bytes that stay valid and unchanged for `'a`.
    pub unsafe fn as_bytes<'a>(self) -> &'a [u8] {
        if self.len == 0 {
            return &[];
        }
        // SAFETY: the caller's promise.
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl Default for Str {
    fn default() -> Self {
        Self::new("")
    }
}

/// An RGBA color with sRGB components from 0 to 1, as Wingine's `Color`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Color {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl Color {
    pub const TRANSPARENT: Self = Self::rgba(0.0, 0.0, 0.0, 0.0);
    pub const BLACK: Self = Self::rgb(0.0, 0.0, 0.0);
    pub const WHITE: Self = Self::rgb(1.0, 1.0, 1.0);

    pub const fn rgb(r: f32, g: f32, b: f32) -> Self {
        Self::rgba(r, g, b, 1.0)
    }

    pub const fn rgba(r: f32, g: f32, b: f32, a: f32) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb8(r: u8, g: u8, b: u8) -> Self {
        Self::rgba8(r, g, b, 255)
    }

    pub const fn rgba8(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self::rgba(
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            a as f32 / 255.0,
        )
    }
}

/// A rectangle whose top-left corner is `x`, `y`, as Wingine's `Rect`.
#[repr(C)]
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
}

/// A sprite as [`Host::draw_sprites`] draws it: stretched over `dest`, then rotated by `angle`
/// radians about the center of `dest`, its colors multiplied by `tint`. Positive angles turn
/// clockwise, since `y` grows downward.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteDraw {
    pub dest: Rect,
    pub angle: f32,
    pub tint: Color,
}

impl SpriteDraw {
    /// A sprite stretched over `dest`, unturned and untinted.
    pub const fn new(dest: Rect) -> Self {
        Self {
            dest,
            angle: 0.0,
            tint: Color::WHITE,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

macro_rules! opaque {
    ($($(#[$doc:meta])* $name:ident;)*) => {
        $(
            $(#[$doc])*
            #[repr(C)]
            pub struct $name {
                _private: [u8; 0],
                _marker: PhantomData<(*mut u8, PhantomPinned)>,
            }
        )*
    };
}

opaque! {
    /// The engine's services, lent while a game loads and updates.
    Context;
    /// What a game draws on, lent while it draws.
    Canvas;
    /// A call from a script: its arguments, results and the services lent for it.
    Call;
    /// What a library registers, while it registers.
    Registry;
    /// A class of objects scripts hold, registered by a library.
    Class;
    /// An image or part of one to draw, as Wingine's `Sprite`.
    Sprite;
}

/// Creates a library's state for a game, before its `main.luau` runs, storing it in `instance`.
pub type Create = unsafe extern "C" fn(
    data: *mut c_void,
    ctx: *mut Context,
    instance: *mut *mut c_void,
) -> Status;
/// Frees a library's state for a game, when the game ends.
pub type Destroy = unsafe extern "C" fn(data: *mut c_void, instance: *mut c_void);
/// Runs around each of the game's updates, with the `dt` it updates by in seconds.
pub type UpdateHook = unsafe extern "C" fn(
    data: *mut c_void,
    instance: *mut c_void,
    ctx: *mut Context,
    dt: f32,
) -> Status;
/// Runs around the game's `draw`.
pub type DrawHook =
    unsafe extern "C" fn(data: *mut c_void, instance: *mut c_void, canvas: *mut Canvas) -> Status;
/// A function in the library's table, called by scripts.
pub type Function =
    unsafe extern "C" fn(data: *mut c_void, instance: *mut c_void, call: *mut Call) -> Status;
/// A method of a class, called by scripts on `object`. Its arguments don't include the object.
pub type Method = unsafe extern "C" fn(
    data: *mut c_void,
    instance: *mut c_void,
    object: *mut c_void,
    call: *mut Call,
) -> Status;
/// Frees an object of a class, once scripts no longer hold it. It may run after the library's
/// state for the game is destroyed.
pub type DropObject = unsafe extern "C" fn(object: *mut c_void);

/// Everything the engine offers libraries.
///
/// Registration functions work only inside `register`. Argument indices count from 0, and an
/// index past the last argument reads as `nil`. Arguments convert as they do for the engine's own
/// functions, so a color may be a hex string or a table, and a failed conversion's error names
/// the argument as Luau counts it.
#[repr(C)]
pub struct Host {
    pub abi_major: u16,
    pub abi_minor: u16,

    /// Sets the error a failing callback returns [`Status::ERROR`] with.
    pub set_error: unsafe extern "C" fn(message: Str),
    /// The error the last failing engine function set, valid until the next engine call.
    pub error: unsafe extern "C" fn() -> Str,
    pub log: unsafe extern "C" fn(level: Level, message: Str),

    /// Sets how the library's state is created and destroyed for each game.
    pub set_instance: unsafe extern "C" fn(
        registry: *mut Registry,
        create: Create,
        destroy: Destroy,
        data: *mut c_void,
    ),
    pub add_update_hook: unsafe extern "C" fn(
        registry: *mut Registry,
        phase: Phase,
        hook: UpdateHook,
        data: *mut c_void,
    ),
    pub add_draw_hook: unsafe extern "C" fn(
        registry: *mut Registry,
        phase: Phase,
        hook: DrawHook,
        data: *mut c_void,
    ),
    pub add_function: unsafe extern "C" fn(
        registry: *mut Registry,
        name: Str,
        function: Function,
        data: *mut c_void,
    ),
    /// Adds a class, whose objects scripts hold as userdata named `name`.
    pub add_class:
        unsafe extern "C" fn(registry: *mut Registry, name: Str, drop: DropObject) -> *mut Class,
    pub add_method:
        unsafe extern "C" fn(class: *mut Class, name: Str, method: Method, data: *mut c_void),
    /// Adds Luau that runs as the library loads, once its functions are in its table, which the
    /// chunk receives as `...`. It may add functions written in Luau, such as ones that wait.
    pub add_luau: unsafe extern "C" fn(registry: *mut Registry, name: Str, source: Str),

    pub arg_count: unsafe extern "C" fn(call: *mut Call) -> usize,
    pub arg_is_nil: unsafe extern "C" fn(call: *mut Call, index: usize) -> bool,
    pub arg_boolean: unsafe extern "C" fn(call: *mut Call, index: usize, out: *mut bool) -> Status,
    pub arg_number: unsafe extern "C" fn(call: *mut Call, index: usize, out: *mut f64) -> Status,
    /// Reads a whole number from `min` to `max`, failing for one with a fractional part.
    pub arg_integer: unsafe extern "C" fn(
        call: *mut Call,
        index: usize,
        min: i64,
        max: i64,
        out: *mut i64,
    ) -> Status,
    pub arg_string: unsafe extern "C" fn(call: *mut Call, index: usize, out: *mut Str) -> Status,
    pub arg_color: unsafe extern "C" fn(call: *mut Call, index: usize, out: *mut Color) -> Status,
    pub arg_rect: unsafe extern "C" fn(call: *mut Call, index: usize, out: *mut Rect) -> Status,
    pub arg_sprite:
        unsafe extern "C" fn(call: *mut Call, index: usize, out: *mut *const Sprite) -> Status,
    /// Reads an object of `class`, borrowing it until the call returns: exclusively if
    /// `exclusive`, which fails if anything else borrows it, such as a method running on it.
    pub arg_object: unsafe extern "C" fn(
        call: *mut Call,
        index: usize,
        class: *const Class,
        exclusive: bool,
        out: *mut *mut c_void,
    ) -> Status,
    pub push_nil: unsafe extern "C" fn(call: *mut Call),
    pub push_boolean: unsafe extern "C" fn(call: *mut Call, value: bool),
    pub push_number: unsafe extern "C" fn(call: *mut Call, value: f64),
    pub push_integer: unsafe extern "C" fn(call: *mut Call, value: i64),
    pub push_string: unsafe extern "C" fn(call: *mut Call, value: Str),
    pub push_sprite: unsafe extern "C" fn(call: *mut Call, sprite: *const Sprite),
    /// Returns a new object of `class` to scripts, which own it from then on.
    pub push_object:
        unsafe extern "C" fn(call: *mut Call, class: *const Class, object: *mut c_void),
    /// The context, if the engine lent it for the call: while the game loads and in `update`.
    pub call_context: unsafe extern "C" fn(call: *mut Call) -> *mut Context,
    /// The canvas, if the engine lent it for the call: in `draw`.
    pub call_canvas: unsafe extern "C" fn(call: *mut Call) -> *mut Canvas,

    /// The size of the canvas the frame will be drawn on.
    pub context_canvas_size: unsafe extern "C" fn(ctx: *mut Context) -> Size,
    pub action_held: unsafe extern "C" fn(ctx: *mut Context, action: Str) -> bool,
    pub action_pressed: unsafe extern "C" fn(ctx: *mut Context, action: Str) -> bool,
    pub action_released: unsafe extern "C" fn(ctx: *mut Context, action: Str) -> bool,
    /// Writes the cursor's position in canvas pixels to `out`, and returns whether it's over the
    /// window.
    pub mouse_position: unsafe extern "C" fn(ctx: *mut Context, out: *mut Point) -> bool,
    /// Creates a sprite from `width` by `height` RGBA pixels (sRGB), row by row from the top.
    /// Returns null if the size is empty or too large.
    pub create_sprite: unsafe extern "C" fn(
        ctx: *mut Context,
        width: u32,
        height: u32,
        rgba: *const u8,
    ) -> *mut Sprite,

    pub canvas_size: unsafe extern "C" fn(canvas: *mut Canvas) -> Size,
    pub fill_rect: unsafe extern "C" fn(canvas: *mut Canvas, rect: Rect, color: Color),
    pub draw_sprite:
        unsafe extern "C" fn(canvas: *mut Canvas, sprite: *const Sprite, dest: Rect, tint: Color),

    /// A copy of `sprite` the library owns, until it frees it with `free_sprite`.
    pub clone_sprite: unsafe extern "C" fn(sprite: *const Sprite) -> *mut Sprite,
    pub free_sprite: unsafe extern "C" fn(sprite: *mut Sprite),
    /// The sprite's size in texels.
    pub sprite_size: unsafe extern "C" fn(sprite: *const Sprite) -> Size,

    /// Draws `sprite` as each of the `count` draws at `draws` says, in order: many sprites for
    /// the cost of one call. Since 0.2.
    pub draw_sprites: unsafe extern "C" fn(
        canvas: *mut Canvas,
        sprite: *const Sprite,
        draws: *const SpriteDraw,
        count: usize,
    ),
}
