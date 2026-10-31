//! Native libraries for Wingine's Luau runtime, written in Rust.
//!
//! A native library adds a global table of functions to the Luau games that load it, and can run
//! hooks around each `update` and `draw`. It's a `cdylib` crate with a type implementing
//! [`Library`], which holds the library's state for one game, exported with [`export!`]:
//!
//! ```
//! use wingine_native::{Call, Class, Context, Library, Registry, Result};
//!
//! #[derive(Default)]
//! struct Counter {
//!     frames: u32,
//! }
//!
//! /// A tally scripts keep, as `local tally = counter.new_tally()`.
//! struct Tally(u32);
//!
//! impl Class for Tally {
//!     const NAME: &str = "Tally";
//! }
//!
//! impl Library for Counter {
//!     const NAME: &str = "counter";
//!
//!     fn new(_ctx: &mut Context) -> Result<Self> {
//!         Ok(Self::default())
//!     }
//!
//!     fn register(registry: &mut Registry<Self>) {
//!         // counter.frames()
//!         registry.function("frames", |counter, _call, ()| Ok(counter.frames));
//!         registry.function("new_tally", |_, _, start: Option<u32>| {
//!             Ok(Tally(start.unwrap_or(0)))
//!         });
//!         // tally:add(n), tally:total()
//!         registry
//!             .class::<Tally>()
//!             .method("add", |_, tally, _, n: u32| {
//!                 tally.0 += n;
//!                 Ok(())
//!             })
//!             .method("total", |_, tally, _, ()| Ok(tally.0));
//!         registry.after_update(|counter, _ctx, _dt| {
//!             counter.frames += 1;
//!             Ok(())
//!         });
//!     }
//! }
//!
//! wingine_native::export!(Counter);
//! # fn main() {}
//! ```
//!
//! A game loads it by listing it in its `conf.luau`, as `libraries = { "counter" }`, with the
//! built library in its `native` folder: `counter.dll`, `libcounter.so` or `libcounter.dylib`.
//!
//! Each game that loads the library gets state of its own, created with [`Library::new`] before
//! its `main.luau` runs, which every function and hook receives. Functions convert their
//! arguments from Luau and their results to Luau through [`FromArgs`] and [`IntoResults`], as
//! tuples or single values. An error a function returns, or a panic, is a Luau error for the
//! script that called it; one a hook returns stops the game, as an error in the script's `update`
//! or `draw` does.
//!
//! Engine services are lent as they are to scripts: a [`Context`] while the game loads and in
//! `update`, and a [`Canvas`] in `draw`. Hooks receive the one for their phase, and functions ask
//! the [`Call`] for whichever the script that called them could use.
//!
//! Each call from Luau into a library costs far more than drawing a sprite, so a library that
//! draws many sprites draws them with one function call and one [`Canvas::draw_sprites`], rather
//! than a call from Luau for each.

mod call;
mod services;

use std::{
    any::{Any, TypeId},
    ffi::c_void,
    fmt,
    marker::PhantomData,
    panic::{self, AssertUnwindSafe},
    ptr,
    sync::atomic::{AtomicPtr, Ordering},
};

#[doc(hidden)]
pub use wingine_abi as abi;
pub use wingine_abi::{Color, Rect, SpriteDraw};

pub use call::{Call, FromArg, FromArgs, IntoResult, IntoResults};
pub use services::{Canvas, Context, Sprite};

/// A native library: its state for one game, and how it registers its functions, classes and
/// hooks.
pub trait Library: Sized + 'static {
    /// The name of the library's global table in Luau.
    const NAME: &'static str;

    /// Creates the library's state for a game, before its `main.luau` runs.
    fn new(ctx: &mut Context) -> Result<Self>;

    /// Registers the library's functions, classes and hooks, once per process.
    fn register(registry: &mut Registry<Self>);
}

/// A type whose objects scripts hold, registered with [`Registry::class`]. Functions and methods
/// return new objects by returning values of the type, and read them with [`Call::object`].
pub trait Class: Sized + 'static {
    /// The name scripts see, such as in errors and from `tostring`.
    const NAME: &'static str;
}

/// Exports `$library`, a type implementing [`Library`], as the library this crate builds.
#[macro_export]
macro_rules! export {
    ($library:ty) => {
        /// The library, as the engine looks for it.
        #[unsafe(no_mangle)]
        pub extern "C" fn wingine_library() -> *const $crate::abi::Library {
            static LIBRARY: ::std::sync::OnceLock<$crate::abi::Library> =
                ::std::sync::OnceLock::new();
            LIBRARY.get_or_init($crate::library::<$library>)
        }
    };
}

/// How `L` describes itself to the engine. [`export!`] exports it from a library built on its
/// own; a program that links the library in instead passes this to
/// `wingine::runtime::link_library`.
pub fn library<L: Library>() -> abi::Library {
    abi::Library {
        abi_major: abi::ABI_MAJOR,
        abi_minor: abi::ABI_MINOR,
        name: abi::Str::new(L::NAME),
        register: register::<L>,
    }
}

/// An error from a library, which scripts see as a Luau error.
#[derive(Clone)]
pub struct Error {
    message: String,
}

impl Error {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// The error the last failing engine function set.
    fn from_engine() -> Self {
        // SAFETY: valid until the next engine call.
        let message = unsafe { (host().error)().as_bytes() };
        Self::new(String::from_utf8_lossy(message))
    }
}

/// Errors convert from any other error, so `?` works on them.
impl<E: std::error::Error> From<E> for Error {
    fn from(err: E) -> Self {
        Self::new(err.to_string())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.message, f)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// How important a logged message is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

/// Logs `message` through the engine, with the target `native`.
pub fn log(level: Level, message: &str) {
    let level = match level {
        Level::Error => abi::Level::ERROR,
        Level::Warn => abi::Level::WARN,
        Level::Info => abi::Level::INFO,
        Level::Debug => abi::Level::DEBUG,
        Level::Trace => abi::Level::TRACE,
    };
    // SAFETY: the message is valid for the call.
    unsafe { (host().log)(level, abi::Str::new(message)) };
}

/// The engine's functions, from when it first had a library register.
static HOST: AtomicPtr<abi::Host> = AtomicPtr::new(ptr::null_mut());

fn host() -> &'static abi::Host {
    let host = HOST.load(Ordering::Acquire);
    assert!(!host.is_null(), "the engine has a library register first");
    // SAFETY: the engine's host table is static.
    unsafe { &*host }
}

/// Fails with the engine's error unless `status` is OK.
fn check(status: abi::Status) -> Result<()> {
    if status == abi::Status::OK {
        Ok(())
    } else {
        Err(Error::from_engine())
    }
}

/// Runs a callback for the engine, turning its error or panic into one the engine reads.
fn guard(callback: impl FnOnce() -> Result<()>) -> abi::Status {
    let message = match panic::catch_unwind(AssertUnwindSafe(callback)) {
        Ok(Ok(())) => return abi::Status::OK,
        Ok(Err(err)) => err.message,
        Err(panic) => format!("panicked: {}", panic_message(&*panic)),
    };
    // SAFETY: the message is valid for the call.
    unsafe { (host().set_error)(abi::Str::new(&message)) };
    abi::Status::ERROR
}

fn panic_message(panic: &dyn Any) -> &str {
    if let Some(message) = panic.downcast_ref::<&str>() {
        message
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message
    } else {
        "with a value that isn't a message"
    }
}

/// The classes one registration of a library added, by type. A program can register a library
/// more than once, such as by linking it under two names, and the engine gives each registration
/// classes of its own, so games use those of the registration they loaded.
struct Classes(Vec<(TypeId, *mut abi::Class)>);

impl Classes {
    /// The class registered for `T`.
    fn get<T: Class>(&self) -> Result<*mut abi::Class> {
        self.0
            .iter()
            .find(|(type_id, _)| *type_id == TypeId::of::<T>())
            .map(|&(_, class)| class)
            .ok_or_else(|| Error::new(format!("{} isn't a registered class", T::NAME)))
    }
}

/// A library's state for a game, with the classes of the registration the game loaded.
struct Instance<L> {
    library: L,
    classes: &'static Classes,
}

/// Registers a library's functions, classes and hooks, in [`Library::register`].
pub struct Registry<L> {
    raw: *mut abi::Registry,
    classes: Vec<(TypeId, *mut abi::Class)>,
    _library: PhantomData<fn() -> L>,
}

impl<L: Library> Registry<L> {
    /// Adds `function` to the library's table as `name`, for scripts to call with arguments that
    /// convert to `A`.
    pub fn function<A, R, F>(&mut self, name: &str, function: F) -> &mut Self
    where
        A: FromArgs,
        R: IntoResults,
        F: Fn(&mut L, &mut Call, A) -> Result<R> + Send + Sync + 'static,
    {
        // SAFETY: the name is valid for the call, and the data is what the callback expects.
        unsafe {
            (host().add_function)(
                self.raw,
                abi::Str::new(name),
                call_function::<L, A, R, F>,
                data(function),
            );
        }
        self
    }

    /// Adds the class `T`, whose objects scripts hold, returning a registry for its methods.
    pub fn class<T: Class>(&mut self) -> ClassRegistry<'_, L, T> {
        // SAFETY: the name is static.
        let raw = unsafe { (host().add_class)(self.raw, abi::Str::new(T::NAME), drop_object::<T>) };
        // Adding a class twice fails the registration, so this is the only one for `T`.
        self.classes.push((TypeId::of::<T>(), raw));
        ClassRegistry {
            raw,
            _registry: PhantomData,
            _class: PhantomData,
        }
    }

    /// Runs `hook` before each of the game's updates, with the `dt` it updates by in seconds: the
    /// frame's duration, or the step of the game's update rate.
    pub fn before_update<F>(&mut self, hook: F) -> &mut Self
    where
        F: Fn(&mut L, &mut Context, f32) -> Result<()> + Send + Sync + 'static,
    {
        self.update_hook(abi::Phase::BEFORE, hook)
    }

    /// Runs `hook` after each of the game's updates and the tasks it resumes.
    pub fn after_update<F>(&mut self, hook: F) -> &mut Self
    where
        F: Fn(&mut L, &mut Context, f32) -> Result<()> + Send + Sync + 'static,
    {
        self.update_hook(abi::Phase::AFTER, hook)
    }

    /// Runs `hook` each frame before the game's `draw`.
    pub fn before_draw<F>(&mut self, hook: F) -> &mut Self
    where
        F: Fn(&mut L, &mut Canvas) -> Result<()> + Send + Sync + 'static,
    {
        self.draw_hook(abi::Phase::BEFORE, hook)
    }

    /// Runs `hook` each frame after the game's `draw`. It draws in canvas pixels, whatever camera
    /// the game set.
    pub fn after_draw<F>(&mut self, hook: F) -> &mut Self
    where
        F: Fn(&mut L, &mut Canvas) -> Result<()> + Send + Sync + 'static,
    {
        self.draw_hook(abi::Phase::AFTER, hook)
    }

    /// Adds Luau `source` that runs as the library loads, once its functions are in its table,
    /// which the chunk receives as `...`. It may add functions written in Luau, such as ones that
    /// `wait`, and functions it calls get the [`Context`], as they do while the game loads.
    /// `name` names it in errors.
    pub fn luau(&mut self, name: &str, source: &str) -> &mut Self {
        // SAFETY: the strings are valid for the call.
        unsafe { (host().add_luau)(self.raw, abi::Str::new(name), abi::Str::new(source)) };
        self
    }

    fn update_hook<F>(&mut self, phase: abi::Phase, hook: F) -> &mut Self
    where
        F: Fn(&mut L, &mut Context, f32) -> Result<()> + Send + Sync + 'static,
    {
        // SAFETY: the data is what the callback expects.
        unsafe { (host().add_update_hook)(self.raw, phase, call_update_hook::<L, F>, data(hook)) };
        self
    }

    fn draw_hook<F>(&mut self, phase: abi::Phase, hook: F) -> &mut Self
    where
        F: Fn(&mut L, &mut Canvas) -> Result<()> + Send + Sync + 'static,
    {
        // SAFETY: the data is what the callback expects.
        unsafe { (host().add_draw_hook)(self.raw, phase, call_draw_hook::<L, F>, data(hook)) };
        self
    }
}

/// Registers the methods of a class, from [`Registry::class`].
pub struct ClassRegistry<'r, L, T> {
    raw: *mut abi::Class,
    _registry: PhantomData<&'r mut Registry<L>>,
    _class: PhantomData<fn() -> T>,
}

impl<L: Library, T: Class> ClassRegistry<'_, L, T> {
    /// Adds `method` to the class as `name`, for scripts to call as `object:name(...)` with
    /// arguments that convert to `A`. It receives the library's state, then the object.
    pub fn method<A, R, F>(&mut self, name: &str, method: F) -> &mut Self
    where
        A: FromArgs,
        R: IntoResults,
        F: Fn(&mut L, &mut T, &mut Call, A) -> Result<R> + Send + Sync + 'static,
    {
        // SAFETY: the name is valid for the call, and the data is what the callback expects.
        unsafe {
            (host().add_method)(
                self.raw,
                abi::Str::new(name),
                call_method::<L, T, A, R, F>,
                data(method),
            );
        }
        self
    }
}

/// `callback` as the data the engine passes back with it. It lives as long as the process, like
/// the library.
fn data<F: Send + Sync + 'static>(callback: F) -> *mut c_void {
    Box::into_raw(Box::new(callback)).cast()
}

unsafe extern "C" fn register<L: Library>(
    host: *const abi::Host,
    registry: *mut abi::Registry,
) -> abi::Status {
    HOST.store(host.cast_mut(), Ordering::Release);
    guard(|| {
        let mut registry = Registry {
            raw: registry,
            classes: Vec::new(),
            _library: PhantomData,
        };
        L::register(&mut registry);
        // Leaked, like the callbacks, since games' state refers to it.
        let classes: &'static Classes = Box::leak(Box::new(Classes(registry.classes)));
        // SAFETY: the callbacks are for `L`, and their data is the registration's classes.
        unsafe {
            (crate::host().set_instance)(
                registry.raw,
                create::<L>,
                destroy::<L>,
                ptr::from_ref(classes).cast_mut().cast(),
            );
        }
        Ok(())
    })
}

unsafe extern "C" fn create<L: Library>(
    classes: *mut c_void,
    ctx: *mut abi::Context,
    instance: *mut *mut c_void,
) -> abi::Status {
    guard(|| {
        let library = L::new(&mut Context::new(ctx))?;
        // SAFETY: `register` leaked the classes, and passed them as the data.
        let classes = unsafe { &*classes.cast::<Classes>() };
        let state = Instance { library, classes };
        // SAFETY: the engine's promise.
        unsafe { instance.write(Box::into_raw(Box::new(state)).cast()) };
        Ok(())
    })
}

unsafe extern "C" fn destroy<L: Library>(_: *mut c_void, instance: *mut c_void) {
    // A panic dropping it is reported by the panic hook, and goes no further.
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: `create` boxed it, and the engine destroys it once.
        drop(unsafe { Box::from_raw(instance.cast::<Instance<L>>()) });
    }));
}

unsafe extern "C" fn drop_object<T: Class>(object: *mut c_void) {
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: `IntoResult` boxed it, and the engine drops it once.
        drop(unsafe { Box::from_raw(object.cast::<T>()) });
    }));
}

unsafe extern "C" fn call_function<L, A, R, F>(
    data: *mut c_void,
    instance: *mut c_void,
    call: *mut abi::Call,
) -> abi::Status
where
    L: Library,
    A: FromArgs,
    R: IntoResults,
    F: Fn(&mut L, &mut Call, A) -> Result<R>,
{
    guard(|| {
        // SAFETY: registered with this data, and the engine passes the game's state for `L`.
        let (function, instance) =
            unsafe { (&*data.cast::<F>(), &mut *instance.cast::<Instance<L>>()) };
        let mut call = Call::new(call, instance.classes);
        let args = A::from_args(&call)?;
        function(&mut instance.library, &mut call, args)?.push_all(&call)
    })
}

unsafe extern "C" fn call_method<L, T, A, R, F>(
    data: *mut c_void,
    instance: *mut c_void,
    object: *mut c_void,
    call: *mut abi::Call,
) -> abi::Status
where
    L: Library,
    T: Class,
    A: FromArgs,
    R: IntoResults,
    F: Fn(&mut L, &mut T, &mut Call, A) -> Result<R>,
{
    guard(|| {
        // SAFETY: registered with this data, the engine passes the game's state for `L`, and
        // the object is a `T` the engine borrowed exclusively for the call.
        let (method, instance, object) = unsafe {
            (
                &*data.cast::<F>(),
                &mut *instance.cast::<Instance<L>>(),
                &mut *object.cast::<T>(),
            )
        };
        let mut call = Call::new(call, instance.classes);
        let args = A::from_args(&call)?;
        method(&mut instance.library, object, &mut call, args)?.push_all(&call)
    })
}

unsafe extern "C" fn call_update_hook<L, F>(
    data: *mut c_void,
    instance: *mut c_void,
    ctx: *mut abi::Context,
    dt: f32,
) -> abi::Status
where
    L: Library,
    F: Fn(&mut L, &mut Context, f32) -> Result<()>,
{
    guard(|| {
        // SAFETY: registered with this data, and the engine passes the game's state for `L`.
        let (hook, instance) =
            unsafe { (&*data.cast::<F>(), &mut *instance.cast::<Instance<L>>()) };
        hook(&mut instance.library, &mut Context::new(ctx), dt)
    })
}

unsafe extern "C" fn call_draw_hook<L, F>(
    data: *mut c_void,
    instance: *mut c_void,
    canvas: *mut abi::Canvas,
) -> abi::Status
where
    L: Library,
    F: Fn(&mut L, &mut Canvas) -> Result<()>,
{
    guard(|| {
        // SAFETY: registered with this data, and the engine passes the game's state for `L`.
        let (hook, instance) =
            unsafe { (&*data.cast::<F>(), &mut *instance.cast::<Instance<L>>()) };
        hook(&mut instance.library, &mut Canvas::new(canvas))
    })
}
