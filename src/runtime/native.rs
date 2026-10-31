//! Native libraries: compiled code that adds a table of functions to a game's Luau, and hooks
//! around its `update` and `draw`. The interface they're built against is the `wingine-abi`
//! crate, whose docs give the rules both sides follow.
//!
//! A library is registered once per process, the first time a game loads it, and never unloaded,
//! so what it registered lives as long as the process. Each game that loads it then gets the
//! library's state of its own, and its own copies of the library's functions in its VM.

use std::{
    cell::RefCell,
    collections::HashMap,
    env::consts::{DLL_PREFIX, DLL_SUFFIX},
    ffi::c_void,
    io,
    path::{Path, PathBuf},
    ptr,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use mlua::IntoLua;
use wingine_abi as abi;

use super::{CANVAS, CONTEXT, LibraryConfig, lend, with_lent};
use crate::{
    Context, Error, Result,
    graphics::{Canvas, Color, Sprite},
    math::Rect,
};

/// Whether `name` can name a library in a game's settings: it's part of a file name, so it may
/// not hold path separators or dots.
pub(super) fn is_library_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Libraries linked into this program, by the name games list them by.
static LINKED: Mutex<Vec<(String, abi::Library)>> = Mutex::new(Vec::new());

/// Libraries registered in this process, by where they came from.
static REGISTERED: Mutex<Vec<(Source, &'static Library)>> = Mutex::new(Vec::new());

#[derive(Clone, Debug, PartialEq, Eq)]
enum Source {
    Linked(String),
    /// The library's file, by its canonical path.
    File(PathBuf),
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // Nothing panics while holding these locks in a way that leaves them inconsistent.
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Makes `library`, linked into this program rather than loaded from a file, the library `name`
/// for every game that lists it in its `libraries`, in place of a file in its `native` folder.
/// Libraries written with `wingine-native` provide theirs with `wingine_native::library`.
///
/// Games that already loaded a library by that name keep the one they loaded.
pub fn link_library(name: &str, library: abi::Library) {
    let mut linked = lock(&LINKED);
    linked.retain(|(linked, _)| linked != name);
    linked.push((name.to_owned(), library));
    let source = Source::Linked(name.to_owned());
    lock(&REGISTERED).retain(|(registered, _)| *registered != source);
}

/// Where the game in `folder` keeps the library `name`.
fn library_path(folder: &Path, name: &str) -> PathBuf {
    folder
        .join("native")
        .join(format!("{DLL_PREFIX}{name}{DLL_SUFFIX}"))
}

/// The library `name` for the game in `folder`: linked into this program, or loaded from the
/// game's `native` folder, and registered if it hasn't been yet. `None` if there's no such
/// library.
fn find(folder: &Path, name: &str) -> Result<Option<&'static Library>, String> {
    let linked = lock(&LINKED)
        .iter()
        .find(|(linked, _)| linked == name)
        .map(|(_, library)| *library);
    let source = match linked {
        Some(_) => Source::Linked(name.to_owned()),
        None => {
            let path = library_path(folder, name);
            match path.canonicalize() {
                Ok(path) => Source::File(path),
                Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
                Err(err) => return Err(format!("couldn't open {}: {err}", path.display())),
            }
        }
    };
    // Held while registering, so two games loading the library at once register it once.
    let mut registered = lock(&REGISTERED);
    if let Some((_, library)) = registered.iter().find(|(s, _)| *s == source) {
        return Ok(Some(library));
    }
    let descriptor = match (&source, linked) {
        (_, Some(library)) => library,
        (Source::File(path), None) => open(path)?,
        (Source::Linked(_), None) => unreachable!("linked libraries have a descriptor"),
    };
    let library: &'static Library = Box::leak(Box::new(register(descriptor)?));
    registered.push((source, library));
    Ok(Some(library))
}

/// Loads the library file at `path`, which stays loaded until the process ends, and returns how
/// it describes itself.
fn open(path: &Path) -> Result<abi::Library, String> {
    // SAFETY: loading a library runs its initialization code, and calling its entry point trusts
    // that it follows the ABI. Games list the libraries they trust.
    unsafe {
        let library = libloading::Library::new(path).map_err(|err| err.to_string())?;
        let entry = *library.get::<abi::Entry>(abi::ENTRY_SYMBOL).map_err(|_| {
            format!(
                "{} has no `wingine_library` function: was it built with \
                     `wingine_native::export!`?",
                path.display()
            )
        })?;
        let descriptor = entry();
        if descriptor.is_null() {
            return Err("its `wingine_library` returned nothing".into());
        }
        // Never unloaded, so the functions it registers stay valid.
        std::mem::forget(library);
        Ok(*descriptor)
    }
}

/// What a library registered.
struct Library {
    /// Its global table's name.
    name: String,
    instance: Option<Callback<(abi::Create, abi::Destroy)>>,
    update_hooks: Vec<(abi::Phase, Callback<abi::UpdateHook>)>,
    draw_hooks: Vec<(abi::Phase, Callback<abi::DrawHook>)>,
    functions: Vec<(String, Callback<abi::Function>)>,
    classes: Vec<&'static Class>,
    /// Chunks of Luau to run as it loads, with their names.
    luau: Vec<(String, String)>,
}

/// A class of objects a library registered.
struct Class {
    name: String,
    drop: abi::DropObject,
    methods: Vec<(String, Callback<abi::Method>)>,
    /// A mistake registering its methods, which fails the library's registration.
    error: Option<String>,
}

/// A library's callback, and the data it's called with.
#[derive(Clone, Copy)]
struct Callback<F> {
    function: F,
    data: *mut c_void,
}

// SAFETY: the ABI requires registered callbacks to work from any thread a game runs on, with
// their data.
unsafe impl<F> Send for Callback<F> {}
unsafe impl<F> Sync for Callback<F> {}

/// A library while it registers, which the engine's registration functions add to.
struct Registration {
    library: Library,
    /// Its classes, which it may add methods to until it finishes registering.
    classes: Vec<*mut Class>,
    /// The first mistake it made, which fails registration.
    error: Option<String>,
}

impl Registration {
    fn fail(&mut self, message: String) {
        self.error.get_or_insert(message);
    }
}

/// Checks `descriptor`'s version, then has the library register its functions, classes and
/// hooks.
fn register(descriptor: abi::Library) -> Result<Library, String> {
    if descriptor.abi_major != abi::ABI_MAJOR || descriptor.abi_minor > abi::ABI_MINOR {
        return Err(format!(
            "it was built for version {}.{} of the engine's interface, which this engine \
             ({}.{}) can't load",
            descriptor.abi_major,
            descriptor.abi_minor,
            abi::ABI_MAJOR,
            abi::ABI_MINOR
        ));
    }
    // SAFETY: the name is static.
    let name = unsafe { text(descriptor.name) }?;
    if !is_identifier(name) {
        return Err(format!("`{name}` can't name a Luau table"));
    }
    let mut registration = Registration {
        library: Library {
            name: name.to_owned(),
            instance: None,
            update_hooks: Vec::new(),
            draw_hooks: Vec::new(),
            functions: Vec::new(),
            classes: Vec::new(),
            luau: Vec::new(),
        },
        classes: Vec::new(),
        error: None,
    };
    clear_error();
    // SAFETY: the registry is the registration, as the engine's registration functions expect.
    let status = unsafe { (descriptor.register)(&HOST, (&raw mut registration).cast()) };
    if status != abi::Status::OK {
        return Err(take_error());
    }
    // SAFETY: the classes were leaked, and nothing changes them after registration.
    registration.library.classes = registration
        .classes
        .iter()
        .map(|&class| unsafe { &*class })
        .collect();
    let class_error = registration.library.classes.iter().find_map(|class| {
        let err = class.error.as_ref()?;
        Some(format!("class `{}`: {err}", class.name))
    });
    if let Some(err) = registration.error.or(class_error) {
        return Err(err);
    }
    Ok(registration.library)
}

/// Whether `name` is a Luau identifier, which scripts can name a global table by.
fn is_identifier(name: &str) -> bool {
    const KEYWORDS: [&str; 21] = [
        "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in",
        "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
    ];
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !KEYWORDS.contains(&name)
}

/// `text` as a string, if it's UTF-8.
///
/// # Safety
///
/// `text` must be valid for `'a`.
unsafe fn text<'a>(text: abi::Str) -> Result<&'a str, String> {
    // SAFETY: the caller's promise.
    let bytes = unsafe { text.as_bytes() };
    std::str::from_utf8(bytes)
        .map_err(|_| format!("{:?} isn't UTF-8", String::from_utf8_lossy(bytes)))
}

/// The native libraries a game loaded, with their state for it.
#[derive(Default)]
pub(crate) struct Natives {
    instances: Vec<Instance>,
}

/// A library loaded for a game, with its state for the game.
struct Instance {
    /// What the game calls it.
    name: String,
    library: &'static Library,
    state: *mut c_void,
}

impl Natives {
    /// Loads `libraries`, in order, for the game in `folder`, creating their state for it and
    /// adding their tables to its VM. Optional libraries that aren't there are left out.
    pub(crate) fn load(
        lua: &mlua::Lua,
        ctx: &mut Context,
        folder: &Path,
        libraries: &[LibraryConfig],
    ) -> Result<Self> {
        let mut natives = Self::default();
        for LibraryConfig { name, optional } in libraries {
            let fail = |message: String| Error::NativeLibrary {
                name: name.clone(),
                message,
            };
            let library = match find(folder, name).map_err(fail)? {
                Some(library) => library,
                None => {
                    let path = library_path(folder, name);
                    if *optional {
                        log::info!("no {}: running without `{name}`", path.display());
                        continue;
                    }
                    return Err(fail(format!("couldn't find {}", path.display())));
                }
            };
            if !lua.globals().get::<mlua::Value>(&*library.name)?.is_nil() {
                return Err(fail(format!(
                    "its table `{}` is taken by the engine or another library",
                    library.name
                )));
            }
            let mut state = ptr::null_mut();
            if let Some(instance) = library.instance {
                let (create, _) = instance.function;
                clear_error();
                // SAFETY: the library's own callback, called as the ABI says.
                let status =
                    unsafe { create(instance.data, ptr::from_mut(ctx).cast(), &mut state) };
                if status != abi::Status::OK {
                    return Err(fail(take_error()));
                }
            }
            natives.instances.push(Instance {
                name: name.clone(),
                library,
                state,
            });
            // The context is lent for the library's Luau, as it is for the game's `main.luau`.
            lend(lua, CONTEXT, ctx, || Ok(install(lua, library, state)))?
                .map_err(|err| fail(err.to_string()))?;
        }
        Ok(natives)
    }

    /// Runs the libraries' hooks for `phase` around the game's `update`, in the order they
    /// loaded.
    pub(crate) fn update(&self, phase: abi::Phase, ctx: &mut Context, dt: f32) -> Result<()> {
        for instance in &self.instances {
            for (_, hook) in instance
                .library
                .update_hooks
                .iter()
                .filter(|(p, _)| *p == phase)
            {
                clear_error();
                // SAFETY: the library's own callback, called as the ABI says.
                let status = unsafe {
                    (hook.function)(hook.data, instance.state, ptr::from_mut(ctx).cast(), dt)
                };
                instance.check(status)?;
            }
        }
        Ok(())
    }

    /// Runs the libraries' hooks for `phase` around the game's `draw`, in the order they loaded.
    pub(crate) fn draw(&self, phase: abi::Phase, canvas: &mut Canvas) -> Result<()> {
        for instance in &self.instances {
            for (_, hook) in instance
                .library
                .draw_hooks
                .iter()
                .filter(|(p, _)| *p == phase)
            {
                clear_error();
                // SAFETY: the library's own callback, called as the ABI says.
                let status = unsafe {
                    (hook.function)(hook.data, instance.state, ptr::from_mut(canvas).cast())
                };
                instance.check(status)?;
            }
        }
        Ok(())
    }
}

impl Instance {
    fn check(&self, status: abi::Status) -> Result<()> {
        if status == abi::Status::OK {
            return Ok(());
        }
        Err(Error::NativeLibrary {
            name: self.name.clone(),
            message: take_error(),
        })
    }
}

impl Drop for Natives {
    fn drop(&mut self) {
        // In the reverse of the order they loaded.
        while let Some(instance) = self.instances.pop() {
            if let Some(callback) = instance.library.instance {
                let (_, destroy) = callback.function;
                // SAFETY: the library's own callback, called as the ABI says, once.
                unsafe { destroy(callback.data, instance.state) };
            }
        }
    }
}

/// The methods of each class in a VM, by the class's address.
#[derive(Default)]
struct ClassMethods(HashMap<usize, mlua::Table>);

fn class_key(class: &Class) -> usize {
    ptr::from_ref(class) as usize
}

/// Adds `library`'s table to `lua`, with functions that call it with its state for the game.
fn install(lua: &mlua::Lua, library: &'static Library, state: *mut c_void) -> mlua::Result<()> {
    let table = lua.create_table()?;
    for (name, callback) in &library.functions {
        let label = format!("{}.{name}", library.name);
        let callback = *callback;
        let function = lua.create_function(move |lua, args: mlua::Variadic<mlua::Value>| {
            invoke(lua, &label, &args, 1, |call| {
                // SAFETY: the library's own callback, called as the ABI says.
                unsafe { (callback.function)(callback.data, state, call) }
            })
        })?;
        table.set(name.as_str(), function)?;
    }

    if lua.app_data_ref::<ClassMethods>().is_none() {
        lua.set_app_data(ClassMethods::default());
    }
    for &class in &library.classes {
        let methods = lua.create_table()?;
        for (name, callback) in &class.methods {
            let label = format!("{}:{name}", class.name);
            let callback = *callback;
            let method = lua.create_function(move |lua, args: mlua::Variadic<mlua::Value>| {
                let bad_self = |message: String| mlua::Error::BadArgument {
                    to: Some(label.clone()),
                    pos: 1,
                    name: Some("self".into()),
                    cause: Arc::new(mlua::Error::runtime(message)),
                };
                let expected =
                    || bad_self(format!("expected {}; call methods with `:`", class.name));
                let Some(mlua::Value::UserData(this)) = args.first() else {
                    return Err(expected());
                };
                if !this.is::<NativeObject>() {
                    return Err(expected());
                }
                // Borrowed until the method returns, so it may change the object.
                let object = this
                    .borrow_mut::<NativeObject>()
                    .map_err(|err| bad_self(err.to_string()))?;
                if !ptr::eq(object.class, class) {
                    let message = format!("expected {}, got {}", class.name, object.class.name);
                    return Err(bad_self(message));
                }
                let pointer = object.object;
                invoke(lua, &label, &args[1..], 2, |call| {
                    // SAFETY: the library's own callback, called as the ABI says.
                    unsafe { (callback.function)(callback.data, state, pointer, call) }
                })
            })?;
            methods.set(name.as_str(), method)?;
        }
        methods.set_readonly(true);
        lua.app_data_mut::<ClassMethods>()
            .expect("added above")
            .0
            .insert(class_key(class), methods);
    }

    for (name, source) in &library.luau {
        lua.load(source.as_str())
            .set_name(format!("={}/{name}", library.name))
            .call::<()>(table.clone())?;
    }
    table.set_readonly(true);
    lua.globals().set(library.name.as_str(), table)
}

/// An object of a library's class, which scripts hold.
struct NativeObject {
    class: &'static Class,
    object: *mut c_void,
}

impl Drop for NativeObject {
    fn drop(&mut self) {
        // SAFETY: the library's own callback, called as the ABI says, once.
        unsafe { (self.class.drop)(self.object) }
    }
}

impl mlua::UserData for NativeObject {
    fn add_methods<M: mlua::UserDataMethods<Self>>(methods: &mut M) {
        methods.add_meta_method(mlua::MetaMethod::Index, |lua, this, key: mlua::Value| {
            let classes = lua.app_data_ref::<ClassMethods>();
            match classes.and_then(|classes| classes.0.get(&class_key(this.class)).cloned()) {
                Some(methods) => methods.get::<mlua::Value>(key),
                None => Ok(mlua::Value::Nil),
            }
        });
        methods.add_meta_method(mlua::MetaMethod::ToString, |_, this, ()| {
            Ok(this.class.name.clone())
        });
    }
}

/// A call from a script to a library, while the library handles it.
struct Call<'a> {
    lua: &'a mlua::Lua,
    /// The function, as scripts call it, and the position Luau counts its first argument at.
    function: &'a str,
    first_position: usize,
    args: &'a [mlua::Value],
    context: *mut Context,
    canvas: *mut Canvas,
    results: Vec<mlua::Value>,
    /// Strings, sprites and objects the library reads from the arguments, borrowed until the call
    /// returns.
    borrowed: Vec<Borrowed>,
    /// The first error from returning a result.
    error: Option<mlua::Error>,
}

#[allow(dead_code, reason = "held only to keep what they borrow alive")]
enum Borrowed {
    String(mlua::BorrowedStr),
    Sprite(mlua::UserDataRef<Sprite>),
    Object(mlua::UserDataRef<NativeObject>),
    ObjectMut(mlua::UserDataRefMut<NativeObject>),
}

impl Call<'_> {
    fn arg(&self, index: usize) -> mlua::Value {
        self.args.get(index).cloned().unwrap_or(mlua::Value::Nil)
    }

    /// Converts the argument at `index` to `T`, or sets the error saying why it can't.
    fn convert<T: mlua::FromLua>(&self, index: usize) -> Option<T> {
        self.lua
            .unpack(self.arg(index))
            .map_err(|err| self.bad_argument(index, err))
            .ok()
    }

    /// Sets the error for an argument that isn't what the library expects.
    fn bad_argument(&self, index: usize, cause: mlua::Error) {
        let err = mlua::Error::BadArgument {
            to: Some(self.function.to_owned()),
            pos: self.first_position + index,
            name: None,
            cause: Arc::new(cause),
        };
        set_error(err.to_string());
    }

    fn push(&mut self, value: mlua::Result<mlua::Value>) {
        match value {
            Ok(value) => self.results.push(value),
            Err(err) => {
                self.error.get_or_insert(err);
            }
        }
    }
}

/// Calls a library's function or method for scripts, with the arguments `args`, lending it the
/// context or canvas if the engine lent the scripts either. `first_position` is where Luau counts
/// the first of `args`, for error messages.
fn invoke(
    lua: &mlua::Lua,
    function: &str,
    args: &[mlua::Value],
    first_position: usize,
    callback: impl FnOnce(*mut abi::Call) -> abi::Status,
) -> mlua::Result<mlua::MultiValue> {
    let mut callback = Some(callback);
    let mut run = |context: *mut Context, canvas: *mut Canvas| {
        let mut call = Call {
            lua,
            function,
            first_position,
            args,
            context,
            canvas,
            results: Vec::new(),
            borrowed: Vec::new(),
            error: None,
        };
        let callback = callback.take().expect("runs once");
        clear_error();
        if callback((&raw mut call).cast()) != abi::Status::OK {
            return Err(mlua::Error::runtime(take_error()));
        }
        match call.error.take() {
            Some(err) => Err(err),
            None => Ok(mlua::MultiValue::from_vec(std::mem::take(
                &mut call.results,
            ))),
        }
    };
    let lent = with_lent(lua, CONTEXT, |ctx: &mut Context| {
        run(ptr::from_mut(ctx), ptr::null_mut())
    })?;
    if let Some(result) = lent {
        return result;
    }
    let lent = with_lent(lua, CANVAS, |canvas: &mut Canvas| {
        run(ptr::null_mut(), ptr::from_mut(canvas))
    })?;
    if let Some(result) = lent {
        return result;
    }
    run(ptr::null_mut(), ptr::null_mut())
}

thread_local! {
    /// The error the last failing callback or engine function set.
    static ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

fn set_error(message: String) {
    ERROR.set(Some(message));
}

fn clear_error() {
    ERROR.set(None);
}

fn take_error() -> String {
    ERROR
        .take()
        .unwrap_or_else(|| "it failed without saying why".into())
}

/// Everything the engine offers libraries.
static HOST: abi::Host = abi::Host {
    abi_major: abi::ABI_MAJOR,
    abi_minor: abi::ABI_MINOR,
    set_error: host::set_error,
    error: host::error,
    log: host::log,
    set_instance: host::set_instance,
    add_update_hook: host::add_update_hook,
    add_draw_hook: host::add_draw_hook,
    add_function: host::add_function,
    add_class: host::add_class,
    add_method: host::add_method,
    add_luau: host::add_luau,
    arg_count: host::arg_count,
    arg_is_nil: host::arg_is_nil,
    arg_boolean: host::arg_boolean,
    arg_number: host::arg_number,
    arg_integer: host::arg_integer,
    arg_string: host::arg_string,
    arg_color: host::arg_color,
    arg_rect: host::arg_rect,
    arg_sprite: host::arg_sprite,
    arg_object: host::arg_object,
    push_nil: host::push_nil,
    push_boolean: host::push_boolean,
    push_number: host::push_number,
    push_integer: host::push_integer,
    push_string: host::push_string,
    push_sprite: host::push_sprite,
    push_object: host::push_object,
    call_context: host::call_context,
    call_canvas: host::call_canvas,
    context_canvas_size: host::context_canvas_size,
    action_held: host::action_held,
    action_pressed: host::action_pressed,
    action_released: host::action_released,
    mouse_position: host::mouse_position,
    create_sprite: host::create_sprite,
    canvas_size: host::canvas_size,
    fill_rect: host::fill_rect,
    draw_sprite: host::draw_sprite,
    clone_sprite: host::clone_sprite,
    free_sprite: host::free_sprite,
    sprite_size: host::sprite_size,
    draw_sprites: host::draw_sprites,
};

/// The functions in [`HOST`]. Each trusts the library to pass what the ABI says: pointers the
/// engine handed it, still valid, and registration functions only while it registers.
mod host {
    use super::*;
    use crate::input::Input;

    /// Writes `value` to `out` if there is one; otherwise its error is set already.
    unsafe fn output<T>(value: Option<T>, out: *mut T) -> abi::Status {
        match value {
            Some(value) => {
                // SAFETY: the caller's promise.
                unsafe { out.write(value) };
                abi::Status::OK
            }
            None => abi::Status::ERROR,
        }
    }

    /// `text` as a string, with anything that isn't UTF-8 replaced.
    unsafe fn lossy(text: abi::Str) -> String {
        // SAFETY: the caller's promise.
        String::from_utf8_lossy(unsafe { text.as_bytes() }).into_owned()
    }

    unsafe fn registration_mut<'a>(registry: *mut abi::Registry) -> &'a mut Registration {
        // SAFETY: the caller's promise.
        unsafe { &mut *registry.cast() }
    }

    unsafe fn call_mut<'a>(call: *mut abi::Call) -> &'a mut Call<'a> {
        // SAFETY: the caller's promise.
        unsafe { &mut *call.cast() }
    }

    unsafe fn context_mut<'a>(ctx: *mut abi::Context) -> &'a mut Context {
        // SAFETY: the caller's promise.
        unsafe { &mut *ctx.cast() }
    }

    unsafe fn canvas_mut<'a>(canvas: *mut abi::Canvas) -> &'a mut Canvas {
        // SAFETY: the caller's promise.
        unsafe { &mut *canvas.cast() }
    }

    unsafe fn sprite_ref<'a>(sprite: *const abi::Sprite) -> &'a Sprite {
        // SAFETY: the caller's promise.
        unsafe { &*sprite.cast() }
    }

    fn size([width, height]: [f32; 2]) -> abi::Size {
        abi::Size { width, height }
    }

    fn color(color: abi::Color) -> Color {
        Color::rgba(color.r, color.g, color.b, color.a)
    }

    fn rect(rect: abi::Rect) -> Rect {
        Rect::new(rect.x, rect.y, rect.w, rect.h)
    }

    pub(super) unsafe extern "C" fn set_error(message: abi::Str) {
        // SAFETY: the library's promise.
        super::set_error(unsafe { lossy(message) });
    }

    pub(super) unsafe extern "C" fn error() -> abi::Str {
        // The string stays put until the error is set again.
        ERROR.with_borrow(|error| abi::Str::new(error.as_deref().unwrap_or("")))
    }

    pub(super) unsafe extern "C" fn log(level: abi::Level, message: abi::Str) {
        let level = match level {
            abi::Level::ERROR => log::Level::Error,
            abi::Level::WARN => log::Level::Warn,
            abi::Level::DEBUG => log::Level::Debug,
            abi::Level::TRACE => log::Level::Trace,
            _ => log::Level::Info,
        };
        // SAFETY: the library's promise.
        log::log!(target: "native", level, "{}", unsafe { lossy(message) });
    }

    pub(super) unsafe extern "C" fn set_instance(
        registry: *mut abi::Registry,
        create: abi::Create,
        destroy: abi::Destroy,
        data: *mut c_void,
    ) {
        // SAFETY: the library's promise.
        let registration = unsafe { registration_mut(registry) };
        registration.library.instance = Some(Callback {
            function: (create, destroy),
            data,
        });
    }

    fn check_phase(registration: &mut Registration, phase: abi::Phase) -> bool {
        let valid = matches!(phase, abi::Phase::BEFORE | abi::Phase::AFTER);
        if !valid {
            registration.fail(format!("there's no phase {}", phase.0));
        }
        valid
    }

    pub(super) unsafe extern "C" fn add_update_hook(
        registry: *mut abi::Registry,
        phase: abi::Phase,
        hook: abi::UpdateHook,
        data: *mut c_void,
    ) {
        // SAFETY: the library's promise.
        let registration = unsafe { registration_mut(registry) };
        if check_phase(registration, phase) {
            let hook = Callback {
                function: hook,
                data,
            };
            registration.library.update_hooks.push((phase, hook));
        }
    }

    pub(super) unsafe extern "C" fn add_draw_hook(
        registry: *mut abi::Registry,
        phase: abi::Phase,
        hook: abi::DrawHook,
        data: *mut c_void,
    ) {
        // SAFETY: the library's promise.
        let registration = unsafe { registration_mut(registry) };
        if check_phase(registration, phase) {
            let hook = Callback {
                function: hook,
                data,
            };
            registration.library.draw_hooks.push((phase, hook));
        }
    }

    /// The name of something a library registers, unless `taken` says it registered it already.
    unsafe fn new_name(name: abi::Str, taken: impl Fn(&str) -> bool) -> Result<String, String> {
        // SAFETY: the caller's promise.
        let name = unsafe { text(name) }?;
        if taken(name) {
            return Err(format!("it registered `{name}` twice"));
        }
        Ok(name.to_owned())
    }

    pub(super) unsafe extern "C" fn add_function(
        registry: *mut abi::Registry,
        name: abi::Str,
        function: abi::Function,
        data: *mut c_void,
    ) {
        // SAFETY: the library's promise.
        let registration = unsafe { registration_mut(registry) };
        let functions = &registration.library.functions;
        // SAFETY: the library's promise.
        match unsafe { new_name(name, |name| functions.iter().any(|(f, _)| f == name)) } {
            Ok(name) => {
                let function = Callback { function, data };
                registration.library.functions.push((name, function));
            }
            Err(err) => registration.fail(err),
        }
    }

    pub(super) unsafe extern "C" fn add_class(
        registry: *mut abi::Registry,
        name: abi::Str,
        drop: abi::DropObject,
    ) -> *mut abi::Class {
        // SAFETY: the library's promise.
        let registration = unsafe { registration_mut(registry) };
        let classes = &registration.classes;
        // SAFETY: the library's promise, and the classes are alive until the library is.
        let name = unsafe {
            new_name(name, |name| {
                classes.iter().any(|&class| (*class).name == name)
            })
        };
        let name = name.unwrap_or_else(|err| {
            registration.fail(err);
            String::new()
        });
        // Leaked, since it lives as long as the library.
        let class = Box::into_raw(Box::new(Class {
            name,
            drop,
            methods: Vec::new(),
            error: None,
        }));
        registration.classes.push(class);
        class.cast()
    }

    pub(super) unsafe extern "C" fn add_method(
        class: *mut abi::Class,
        name: abi::Str,
        method: abi::Method,
        data: *mut c_void,
    ) {
        // SAFETY: the library's promise.
        let class = unsafe { &mut *class.cast::<Class>() };
        let methods = &class.methods;
        // SAFETY: the library's promise.
        match unsafe { new_name(name, |name| methods.iter().any(|(m, _)| m == name)) } {
            Ok(name) => {
                let method = Callback {
                    function: method,
                    data,
                };
                class.methods.push((name, method));
            }
            Err(err) => {
                class.error.get_or_insert(err);
            }
        }
    }

    pub(super) unsafe extern "C" fn add_luau(
        registry: *mut abi::Registry,
        name: abi::Str,
        source: abi::Str,
    ) {
        // SAFETY: the library's promise.
        let registration = unsafe { registration_mut(registry) };
        // SAFETY: the library's promise.
        let (name, source) = unsafe { (lossy(name), lossy(source)) };
        registration.library.luau.push((name, source));
    }

    pub(super) unsafe extern "C" fn arg_count(call: *mut abi::Call) -> usize {
        // SAFETY: the library's promise.
        unsafe { call_mut(call) }.args.len()
    }

    pub(super) unsafe extern "C" fn arg_is_nil(call: *mut abi::Call, index: usize) -> bool {
        // SAFETY: the library's promise.
        unsafe { call_mut(call) }.arg(index).is_nil()
    }

    pub(super) unsafe extern "C" fn arg_boolean(
        call: *mut abi::Call,
        index: usize,
        out: *mut bool,
    ) -> abi::Status {
        // SAFETY: the library's promise.
        unsafe { output(call_mut(call).convert(index), out) }
    }

    pub(super) unsafe extern "C" fn arg_number(
        call: *mut abi::Call,
        index: usize,
        out: *mut f64,
    ) -> abi::Status {
        // SAFETY: the library's promise.
        unsafe { output(call_mut(call).convert(index), out) }
    }

    pub(super) unsafe extern "C" fn arg_integer(
        call: *mut abi::Call,
        index: usize,
        min: i64,
        max: i64,
        out: *mut i64,
    ) -> abi::Status {
        // SAFETY: the library's promise.
        let call = unsafe { call_mut(call) };
        let value = call.convert::<i64>(index).filter(|value| {
            let in_range = (min..=max).contains(value);
            if !in_range {
                let message = format!("{value} is out of range: expected {min} to {max}");
                let err = conversion(&call.arg(index), "integer", Some(message));
                call.bad_argument(index, err);
            }
            in_range
        });
        // SAFETY: the library's promise.
        unsafe { output(value, out) }
    }

    pub(super) unsafe extern "C" fn arg_string(
        call: *mut abi::Call,
        index: usize,
        out: *mut abi::Str,
    ) -> abi::Status {
        // SAFETY: the library's promise.
        let call = unsafe { call_mut(call) };
        let string = call.convert::<mlua::LuaString>(index).and_then(|string| {
            string
                .to_str()
                .map_err(|err| call.bad_argument(index, err))
                .ok()
        });
        let text = string.map(|string| {
            let text = abi::Str::new(&string);
            call.borrowed.push(Borrowed::String(string));
            text
        });
        // SAFETY: the library's promise.
        unsafe { output(text, out) }
    }

    pub(super) unsafe extern "C" fn arg_color(
        call: *mut abi::Call,
        index: usize,
        out: *mut abi::Color,
    ) -> abi::Status {
        // SAFETY: the library's promise.
        let color = unsafe { call_mut(call) }.convert::<Color>(index);
        let color = color.map(|Color { r, g, b, a }| abi::Color { r, g, b, a });
        // SAFETY: the library's promise.
        unsafe { output(color, out) }
    }

    pub(super) unsafe extern "C" fn arg_rect(
        call: *mut abi::Call,
        index: usize,
        out: *mut abi::Rect,
    ) -> abi::Status {
        // SAFETY: the library's promise.
        let rect = unsafe { call_mut(call) }.convert::<Rect>(index);
        let rect = rect.map(|Rect { x, y, w, h }| abi::Rect { x, y, w, h });
        // SAFETY: the library's promise.
        unsafe { output(rect, out) }
    }

    pub(super) unsafe extern "C" fn arg_sprite(
        call: *mut abi::Call,
        index: usize,
        out: *mut *const abi::Sprite,
    ) -> abi::Status {
        // SAFETY: the library's promise.
        let call = unsafe { call_mut(call) };
        let sprite = call
            .convert::<mlua::UserDataRef<Sprite>>(index)
            .map(|sprite| {
                let pointer = ptr::from_ref::<Sprite>(&sprite).cast();
                call.borrowed.push(Borrowed::Sprite(sprite));
                pointer
            });
        // SAFETY: the library's promise.
        unsafe { output(sprite, out) }
    }

    pub(super) unsafe extern "C" fn arg_object(
        call: *mut abi::Call,
        index: usize,
        class: *const abi::Class,
        exclusive: bool,
        out: *mut *mut c_void,
    ) -> abi::Status {
        // SAFETY: the library's promise.
        let call = unsafe { call_mut(call) };
        // SAFETY: the library's promise.
        let class = unsafe { &*class.cast::<Class>() };
        let object = borrow_object(call, index, class, exclusive)
            .map_err(|err| call.bad_argument(index, err))
            .ok();
        // SAFETY: the library's promise.
        unsafe { output(object, out) }
    }

    /// Borrows the object of `class` at `index` until the call returns.
    fn borrow_object(
        call: &mut Call,
        index: usize,
        class: &Class,
        exclusive: bool,
    ) -> mlua::Result<*mut c_void> {
        let userdata = match call.arg(index) {
            mlua::Value::UserData(userdata) if userdata.is::<NativeObject>() => userdata,
            value => return Err(conversion(&value, &class.name, None)),
        };
        let (object, found, borrowed) = if exclusive {
            let object = userdata.borrow_mut::<NativeObject>()?;
            (object.object, object.class, Borrowed::ObjectMut(object))
        } else {
            let object = userdata.borrow::<NativeObject>()?;
            (object.object, object.class, Borrowed::Object(object))
        };
        if !ptr::eq(found, class) {
            let value = mlua::Value::UserData(userdata);
            return Err(conversion(
                &value,
                &class.name,
                Some(format!("got {}", found.name)),
            ));
        }
        call.borrowed.push(borrowed);
        Ok(object)
    }

    /// The error converting `value` to `to`.
    fn conversion(value: &mlua::Value, to: &str, message: Option<String>) -> mlua::Error {
        let from = match value {
            // As `type` names them.
            mlua::Value::Integer(_) | mlua::Value::Number(_) => "number",
            value => value.type_name(),
        };
        mlua::Error::FromLuaConversionError {
            from,
            to: to.into(),
            message,
        }
    }

    pub(super) unsafe extern "C" fn push_nil(call: *mut abi::Call) {
        // SAFETY: the library's promise.
        unsafe { call_mut(call) }.results.push(mlua::Value::Nil);
    }

    pub(super) unsafe extern "C" fn push_boolean(call: *mut abi::Call, value: bool) {
        // SAFETY: the library's promise.
        unsafe { call_mut(call) }
            .results
            .push(mlua::Value::Boolean(value));
    }

    pub(super) unsafe extern "C" fn push_number(call: *mut abi::Call, value: f64) {
        // SAFETY: the library's promise.
        unsafe { call_mut(call) }
            .results
            .push(mlua::Value::Number(value));
    }

    pub(super) unsafe extern "C" fn push_integer(call: *mut abi::Call, value: i64) {
        // SAFETY: the library's promise.
        let call = unsafe { call_mut(call) };
        call.push(value.into_lua(call.lua));
    }

    pub(super) unsafe extern "C" fn push_string(call: *mut abi::Call, value: abi::Str) {
        // SAFETY: the library's promise.
        let call = unsafe { call_mut(call) };
        // SAFETY: the library's promise.
        let string = call.lua.create_string(unsafe { value.as_bytes() });
        call.push(string.map(mlua::Value::String));
    }

    pub(super) unsafe extern "C" fn push_sprite(call: *mut abi::Call, sprite: *const abi::Sprite) {
        // SAFETY: the library's promise.
        let call = unsafe { call_mut(call) };
        // SAFETY: the library's promise.
        let sprite = unsafe { sprite_ref(sprite) }.clone();
        call.push(sprite.into_lua(call.lua));
    }

    pub(super) unsafe extern "C" fn push_object(
        call: *mut abi::Call,
        class: *const abi::Class,
        object: *mut c_void,
    ) {
        // SAFETY: the library's promise.
        let call = unsafe { call_mut(call) };
        // SAFETY: the library's promise. Classes live as long as their library.
        let class: &'static Class = unsafe { &*class.cast::<Class>() };
        // If this fails, dropping the object frees it.
        let object = call.lua.create_userdata(NativeObject { class, object });
        call.push(object.map(mlua::Value::UserData));
    }

    pub(super) unsafe extern "C" fn call_context(call: *mut abi::Call) -> *mut abi::Context {
        // SAFETY: the library's promise.
        unsafe { call_mut(call) }.context.cast()
    }

    pub(super) unsafe extern "C" fn call_canvas(call: *mut abi::Call) -> *mut abi::Canvas {
        // SAFETY: the library's promise.
        unsafe { call_mut(call) }.canvas.cast()
    }

    pub(super) unsafe extern "C" fn context_canvas_size(ctx: *mut abi::Context) -> abi::Size {
        // SAFETY: the library's promise.
        size(unsafe { context_mut(ctx) }.canvas_size())
    }

    /// Whether `action` is in a state `query` finds it in.
    unsafe fn action(
        ctx: *mut abi::Context,
        action: abi::Str,
        query: fn(&Input, &str) -> bool,
    ) -> bool {
        // SAFETY: the library's promise.
        let (ctx, action) = unsafe { (context_mut(ctx), text(action)) };
        action.is_ok_and(|action| query(ctx.input(), action))
    }

    pub(super) unsafe extern "C" fn action_held(ctx: *mut abi::Context, name: abi::Str) -> bool {
        // SAFETY: the library's promise.
        unsafe { action(ctx, name, Input::held) }
    }

    pub(super) unsafe extern "C" fn action_pressed(ctx: *mut abi::Context, name: abi::Str) -> bool {
        // SAFETY: the library's promise.
        unsafe { action(ctx, name, Input::pressed) }
    }

    pub(super) unsafe extern "C" fn action_released(
        ctx: *mut abi::Context,
        name: abi::Str,
    ) -> bool {
        // SAFETY: the library's promise.
        unsafe { action(ctx, name, Input::released) }
    }

    pub(super) unsafe extern "C" fn mouse_position(
        ctx: *mut abi::Context,
        out: *mut abi::Point,
    ) -> bool {
        // SAFETY: the library's promise.
        let position = unsafe { context_mut(ctx) }.input().mouse_position();
        let point = position.map(|[x, y]| abi::Point { x, y });
        // SAFETY: the library's promise.
        unsafe { output(point, out) == abi::Status::OK }
    }

    pub(super) unsafe extern "C" fn create_sprite(
        ctx: *mut abi::Context,
        width: u32,
        height: u32,
        rgba: *const u8,
    ) -> *mut abi::Sprite {
        // SAFETY: the library's promise.
        let ctx = unsafe { context_mut(ctx) };
        let limit = ctx.gpu.device.limits().max_texture_dimension_2d;
        if !(1..=limit).contains(&width) || !(1..=limit).contains(&height) {
            super::set_error(format!(
                "a sprite is from 1x1 to {limit}x{limit} pixels, not {width}x{height}"
            ));
            return ptr::null_mut();
        }
        let len = width as usize * height as usize * 4;
        // SAFETY: the library's promise.
        let pixels = unsafe { std::slice::from_raw_parts(rgba, len) };
        let texture =
            ctx.renderer
                .create_texture(&ctx.gpu.device, &ctx.gpu.queue, width, height, pixels);
        Box::into_raw(Box::new(texture.sprite())).cast()
    }

    pub(super) unsafe extern "C" fn canvas_size(canvas: *mut abi::Canvas) -> abi::Size {
        // SAFETY: the library's promise.
        let canvas = unsafe { canvas_mut(canvas) };
        size([canvas.width(), canvas.height()])
    }

    pub(super) unsafe extern "C" fn fill_rect(
        canvas: *mut abi::Canvas,
        area: abi::Rect,
        fill: abi::Color,
    ) {
        // SAFETY: the library's promise.
        unsafe { canvas_mut(canvas) }.fill_rect(rect(area), color(fill));
    }

    pub(super) unsafe extern "C" fn draw_sprite(
        canvas: *mut abi::Canvas,
        sprite: *const abi::Sprite,
        dest: abi::Rect,
        tint: abi::Color,
    ) {
        // SAFETY: the library's promise.
        let (canvas, sprite) = unsafe { (canvas_mut(canvas), sprite_ref(sprite)) };
        canvas.draw_sprite(sprite, rect(dest), color(tint));
    }

    pub(super) unsafe extern "C" fn clone_sprite(sprite: *const abi::Sprite) -> *mut abi::Sprite {
        // SAFETY: the library's promise.
        let sprite = unsafe { sprite_ref(sprite) }.clone();
        Box::into_raw(Box::new(sprite)).cast()
    }

    pub(super) unsafe extern "C" fn free_sprite(sprite: *mut abi::Sprite) {
        // SAFETY: the library's promise: a sprite the engine boxed, freed once.
        drop(unsafe { Box::from_raw(sprite.cast::<Sprite>()) });
    }

    pub(super) unsafe extern "C" fn sprite_size(sprite: *const abi::Sprite) -> abi::Size {
        // SAFETY: the library's promise.
        let sprite = unsafe { sprite_ref(sprite) };
        size([sprite.width(), sprite.height()])
    }

    pub(super) unsafe extern "C" fn draw_sprites(
        canvas: *mut abi::Canvas,
        sprite: *const abi::Sprite,
        draws: *const abi::SpriteDraw,
        count: usize,
    ) {
        // SAFETY: the library's promise.
        let (canvas, sprite) = unsafe { (canvas_mut(canvas), sprite_ref(sprite)) };
        let draws = if count == 0 {
            &[]
        } else {
            // SAFETY: the library's promise.
            unsafe { std::slice::from_raw_parts(draws, count) }
        };
        for draw in draws {
            canvas.draw_sprite_rotated(sprite, rect(draw.dest), draw.angle, color(draw.tint));
        }
    }
}
