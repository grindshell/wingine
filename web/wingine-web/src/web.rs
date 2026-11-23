//! Running a game in the page: WASI libc's allocator as Rust's, the C++ code's constructors, the
//! log, and starting the game.

use wasm_bindgen::prelude::*;

use crate::Game;

/// Starts `game` in a canvas added to the page, and returns at once: hands the engine its files,
/// in `/<its folder's name>`, and runs it with [`wingine::runtime::run`], with no arguments. A
/// page calls it once the player has clicked, so the browser lets its audio play.
///
/// `on_error` is called with each error the engine or the game logs, such as one that stops the
/// game, and with a panic's message. Everything logged also goes to the browser's console.
pub fn start(game: &Game, on_error: js_sys::Function) -> Result<(), JsValue> {
    logger::init(on_error);
    std::panic::set_hook(Box::new(|info| log::error!("panic: {info}")));
    let folder = format!("/{}", game.name);
    for (path, contents) in game.files {
        wingine::files::add(format!("{folder}/{path}"), *contents);
    }
    wingine::runtime::run(&folder, Vec::new()).map_err(|err| JsValue::from_str(&err.to_string()))
}

/// Rust's allocations come from WASI libc's allocator, as the C and C++ code's do, so the two
/// don't both manage the same memory.
mod allocator {
    use std::{
        alloc::{GlobalAlloc, Layout},
        ffi::c_void,
    };

    struct WasiAllocator;

    unsafe extern "C" {
        fn posix_memalign(ptr: *mut *mut c_void, align: usize, size: usize) -> i32;
        fn free(ptr: *mut c_void);
    }

    unsafe impl GlobalAlloc for WasiAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let mut ptr = std::ptr::null_mut();
            let align = layout.align().max(size_of::<usize>());
            // SAFETY: `align` is a power of two and a multiple of the pointer size.
            if unsafe { posix_memalign(&mut ptr, align, layout.size()) } != 0 {
                return std::ptr::null_mut();
            }
            ptr.cast()
        }

        unsafe fn dealloc(&self, ptr: *mut u8, _: Layout) {
            // SAFETY: `ptr` came from `posix_memalign`.
            unsafe { free(ptr.cast()) };
        }
    }

    #[global_allocator]
    static ALLOCATOR: WasiAllocator = WasiAllocator;
}

unsafe extern "C" {
    /// Runs the C++ code's static constructors, such as those registering Luau's flags.
    fn __wasm_call_ctors();
}

/// Runs once, as the module is instantiated.
///
/// Calling the constructors from an export makes the linker leave the module's other exports as
/// they are. Otherwise it would treat the module as a command, and run the constructors again at
/// the start of every call into it, which loops forever through Luau's list of flags.
#[wasm_bindgen(start)]
fn initialize() {
    // SAFETY: nothing has run yet that relies on them, and they run once.
    unsafe { __wasm_call_ctors() };
}

/// Logs to the browser's console what the `wingine` executable logs to standard error: what
/// scripts print, the engine's warnings, and errors from anywhere.
mod logger {
    use std::cell::RefCell;

    use wasm_bindgen::JsValue;
    use web_sys::console;

    thread_local! {
        static ON_ERROR: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
    }

    struct Logger;

    pub fn init(on_error: js_sys::Function) {
        static LOGGER: Logger = Logger;
        ON_ERROR.with_borrow_mut(|callback| *callback = Some(on_error));
        // Set once, however many times a game starts.
        if log::set_logger(&LOGGER).is_ok() {
            log::set_max_level(log::LevelFilter::Info);
        }
    }

    impl log::Log for Logger {
        fn enabled(&self, metadata: &log::Metadata) -> bool {
            let target = metadata.target();
            let level = if target == "lua" || target == "native" {
                log::Level::Info
            } else if target.starts_with("wingine") {
                log::Level::Warn
            } else {
                log::Level::Error
            };
            metadata.level() <= level
        }

        fn log(&self, record: &log::Record) {
            if !self.enabled(record.metadata()) {
                return;
            }
            let message = format!("[{} {}] {}", record.level(), record.target(), record.args());
            let text = JsValue::from_str(&message);
            match record.level() {
                log::Level::Error => {
                    console::error_1(&text);
                    ON_ERROR.with_borrow(|callback| {
                        if let Some(callback) = callback {
                            let _ = callback.call1(&JsValue::NULL, &text);
                        }
                    });
                }
                log::Level::Warn => console::warn_1(&text),
                _ => console::log_1(&text),
            }
        }

        fn flush(&self) {}
    }
}
