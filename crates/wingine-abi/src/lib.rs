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
/// 1: the first version. 2: [`Host::draw_sprites`]. 3: 3D: meshes, vectors, and views drawn
/// in, from [`Host::create_mesh`] on. 4: [`Host::set_blend`].
pub const ABI_MINOR: u16 = 4;

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

impl Size {
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

/// A point or a direction in 3D, as Luau's vectors and Wingine's `[f32; 3]`. World coordinates
/// are right-handed with `y` up.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const ONE: Self = Self::new(1.0, 1.0, 1.0);
    pub const Y: Self = Self::new(0.0, 1.0, 0.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

/// A 3D transform, as Wingine's `Affine3`: where it takes each unit axis, then the origin. A
/// point `p` goes to `x_axis * p.x + y_axis * p.y + z_axis * p.z + translation`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine3 {
    pub x_axis: Vec3,
    pub y_axis: Vec3,
    pub z_axis: Vec3,
    pub translation: Vec3,
}

impl Affine3 {
    pub const IDENTITY: Self = Self::from_scale(Vec3::ONE);

    /// Moves things by `translation`.
    pub const fn from_translation(translation: Vec3) -> Self {
        Self {
            translation,
            ..Self::IDENTITY
        }
    }

    /// Scales things along each axis about the origin.
    pub const fn from_scale(scale: Vec3) -> Self {
        Self {
            x_axis: Vec3::new(scale.x, 0.0, 0.0),
            y_axis: Vec3::new(0.0, scale.y, 0.0),
            z_axis: Vec3::new(0.0, 0.0, scale.z),
            translation: Vec3::ZERO,
        }
    }

    /// Scales things by `scale` along every axis, then moves them by `translation`.
    pub const fn from_scale_translation(scale: f32, translation: Vec3) -> Self {
        Self {
            translation,
            ..Self::from_scale(Vec3::new(scale, scale, scale))
        }
    }
}

impl Default for Affine3 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// A corner of a mesh's triangles, as Wingine's `MeshVertex`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshVertex {
    pub position: Vec3,
    /// Which way the surface faces here, for lighting. Its length doesn't matter.
    pub normal: Vec3,
    /// Where on the sprite the mesh shows this corner, from 0 at the sprite's left and top to 1
    /// at its right and bottom.
    pub u: f32,
    pub v: f32,
    /// Multiplies the sprite's colors here, as a tint does.
    pub color: Color,
}

impl MeshVertex {
    /// A white corner at `position` facing `normal`, showing the sprite's top-left.
    pub const fn new(position: Vec3, normal: Vec3) -> Self {
        Self {
            position,
            normal,
            u: 0.0,
            v: 0.0,
            color: Color::WHITE,
        }
    }
}

/// How a [`Camera3d`] projects what it sees.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Projection(pub u32);

impl Projection {
    /// Things farther away look smaller, as to an eye.
    pub const PERSPECTIVE: Self = Self(0);
    /// Things look the same size however far away they are.
    pub const ORTHOGRAPHIC: Self = Self(1);
}

/// How what's drawn mixes with what's under it, as Wingine's `Blend`. Since 0.4.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Blend(pub u32);

impl Blend {
    /// Covers what's under it as much as it's opaque, as drawing does by default.
    pub const NORMAL: Self = Self(0);
    /// Adds its color, scaled by its alpha, to what's under it.
    pub const ADD: Self = Self(1);
    /// Multiplies what's under it by its color, where it's opaque.
    pub const MULTIPLY: Self = Self(2);
    /// Brightens what's under it as much as its color is bright, never past white.
    pub const SCREEN: Self = Self(3);
}

/// A camera looking into a 3D world, as Wingine's `Camera3d`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera3d {
    pub position: Vec3,
    /// The point it looks at, which shows at the middle of the view.
    pub target: Vec3,
    /// Which way is up, which points up the view as near as it can.
    pub up: Vec3,
    pub projection: Projection,
    /// With [`Projection::PERSPECTIVE`], the angle the view takes in from its bottom edge to its
    /// top, in radians, between 0 and pi.
    pub fov_y: f32,
    /// With [`Projection::ORTHOGRAPHIC`], how many world units the view shows from its bottom
    /// edge to its top.
    pub height: f32,
    /// How near and how far from the camera things can be and still show: `near` above 0, and
    /// `far` above it.
    pub near: f32,
    pub far: f32,
}

impl Camera3d {
    /// A camera at `position` looking at `target` in perspective, `fov_y` radians of the world
    /// showing from the view's bottom edge to its top. Up is `+y`, and it sees from 0.1 to 1000
    /// world units away.
    pub const fn perspective(position: Vec3, target: Vec3, fov_y: f32) -> Self {
        Self {
            position,
            target,
            up: Vec3::Y,
            projection: Projection::PERSPECTIVE,
            fov_y,
            height: 0.0,
            near: 0.1,
            far: 1000.0,
        }
    }

    /// A camera at `position` looking at `target` without perspective, `height` world units
    /// showing from the view's bottom edge to its top. Up is `+y`, and it sees from 0.1 to 1000
    /// world units away.
    pub const fn orthographic(position: Vec3, target: Vec3, height: f32) -> Self {
        Self {
            projection: Projection::ORTHOGRAPHIC,
            fov_y: 0.0,
            height,
            ..Self::perspective(position, target, 0.0)
        }
    }
}

/// How a 3D view is lit, as Wingine's `Lighting`: by a sun whose light goes one way, and by light
/// from all around.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lighting {
    /// The way the sun's light goes. Its length doesn't matter.
    pub direction: Vec3,
    /// The sun's light on surfaces facing it.
    pub color: Color,
    /// Light on every surface.
    pub ambient: Color,
}

impl Lighting {
    /// A sun high above and a little in front, to the right, as Wingine's default.
    pub const DEFAULT: Self = Self {
        direction: Vec3::new(-0.3, -1.0, -0.5),
        color: Color::rgb(0.8, 0.8, 0.8),
        ambient: Color::rgb(0.5, 0.5, 0.5),
    };
    /// No shading: every surface shows as it is.
    pub const UNLIT: Self = Self {
        direction: Vec3::new(0.0, -1.0, 0.0),
        color: Color::BLACK,
        ambient: Color::WHITE,
    };
}

impl Default for Lighting {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// A 3D view: what it shows, where on the canvas, and how it's lit, as Wingine's `View3d`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View3d {
    pub camera: Camera3d,
    /// Where on the canvas it shows, in the coordinates drawing uses when it begins.
    pub rect: Rect,
    pub lighting: Lighting,
}

impl View3d {
    /// A view through `camera` filling `rect`, lit by [`Lighting::DEFAULT`].
    pub const fn new(camera: Camera3d, rect: Rect) -> Self {
        Self {
            camera,
            rect,
            lighting: Lighting::DEFAULT,
        }
    }
}

/// A mesh as [`Host::draw_meshes`] draws it: placed by `transform`, its colors multiplied by
/// `tint`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeshDraw {
    pub transform: Affine3,
    pub tint: Color,
}

impl MeshDraw {
    /// A mesh placed by `transform`, untinted.
    pub const fn new(transform: Affine3) -> Self {
        Self {
            transform,
            tint: Color::WHITE,
        }
    }
}

/// A sprite as [`Host::draw_billboards`] draws it: standing upright with the middle of its bottom
/// edge at `position`, `size` world units wide and tall, turned to face the camera across the
/// ground, its colors multiplied by `tint`. A negative width mirrors it.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BillboardDraw {
    pub position: Vec3,
    pub size: Size,
    pub tint: Color,
}

impl BillboardDraw {
    /// A sprite standing at `position`, `size` world units in size, untinted.
    pub const fn new(position: Vec3, size: Size) -> Self {
        Self {
            position,
            size,
            tint: Color::WHITE,
        }
    }
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
    /// Triangles on the GPU, to draw in 3D views, as Wingine's `Mesh`.
    Mesh;
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

    /// Creates a mesh of `vertex_count` vertices and `index_count` indices into them, three for
    /// each triangle, counter-clockwise seen from the side it faces, which is the only side that
    /// shows. Returns null if the indices aren't three for each triangle or reach past the
    /// vertices. Since 0.3.
    pub create_mesh: unsafe extern "C" fn(
        ctx: *mut Context,
        vertices: *const MeshVertex,
        vertex_count: usize,
        indices: *const u32,
        index_count: usize,
    ) -> *mut Mesh,
    /// A handle to `mesh` the library owns, until it frees it with `free_mesh`. Since 0.3.
    pub clone_mesh: unsafe extern "C" fn(mesh: *const Mesh) -> *mut Mesh,
    /// Since 0.3.
    pub free_mesh: unsafe extern "C" fn(mesh: *mut Mesh),
    /// Reads a mesh, borrowed until the call returns. Since 0.3.
    pub arg_mesh:
        unsafe extern "C" fn(call: *mut Call, index: usize, out: *mut *const Mesh) -> Status,
    /// Returns a mesh to scripts, which hold a handle of their own. Since 0.3.
    pub push_mesh: unsafe extern "C" fn(call: *mut Call, mesh: *const Mesh),
    /// Reads a Luau vector. Since 0.3.
    pub arg_vector: unsafe extern "C" fn(call: *mut Call, index: usize, out: *mut Vec3) -> Status,
    /// Since 0.3.
    pub push_vector: unsafe extern "C" fn(call: *mut Call, value: Vec3),
    /// Begins a 3D view over what's been drawn, as `graphics.begin_3d` does: the meshes and
    /// billboards drawn after it go in it, until anything else is drawn, which goes over it.
    /// Fails if its camera's projection, `near` or `far` aren't ones [`Camera3d`] allows. Since
    /// 0.3.
    pub begin_3d: unsafe extern "C" fn(canvas: *mut Canvas, view: *const View3d) -> Status,
    /// Draws `mesh` as each of the `count` draws at `draws` says, showing `sprite`, or white if
    /// it's null, in the view begun last, whoever began it. Fails if no view is open: if none
    /// was begun this frame, or anything else was drawn since. Since 0.3.
    pub draw_meshes: unsafe extern "C" fn(
        canvas: *mut Canvas,
        mesh: *const Mesh,
        sprite: *const Sprite,
        draws: *const MeshDraw,
        count: usize,
    ) -> Status,
    /// Draws `sprite` as a billboard for each of the `count` draws at `draws`, in the view begun
    /// last, as `draw_meshes` does. Pixels of the sprite less than half opaque aren't drawn; the
    /// rest are drawn opaque. Since 0.3.
    pub draw_billboards: unsafe extern "C" fn(
        canvas: *mut Canvas,
        sprite: *const Sprite,
        draws: *const BillboardDraw,
        count: usize,
    ) -> Status,

    /// Blends everything drawn after it as `blend` says, as `graphics.set_blend` does, until the
    /// frame ends, it's set again, or the game's `draw` returns. Fails if `blend` isn't one of
    /// [`Blend`]'s. Since 0.4.
    pub set_blend: unsafe extern "C" fn(canvas: *mut Canvas, blend: Blend) -> Status,
}
