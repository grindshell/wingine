//! Calls from scripts, and converting their arguments and results.

use std::ptr;

use crate::{
    Canvas, Class, Classes, Color, Context, Error, Rect, Result, Sprite, abi, check, host,
};

/// A call from a script to one of the library's functions or methods: its arguments, and the
/// services the engine lent for it.
pub struct Call {
    raw: *mut abi::Call,
    /// The classes of the registration the game loaded.
    classes: &'static Classes,
    context: Option<Context>,
    canvas: Option<Canvas>,
}

impl Call {
    pub(crate) fn new(raw: *mut abi::Call, classes: &'static Classes) -> Self {
        Self {
            raw,
            classes,
            context: None,
            canvas: None,
        }
    }

    /// How many arguments the script passed, not counting the object a method is called on.
    pub fn arg_count(&self) -> usize {
        // SAFETY: valid for as long as `self` is.
        unsafe { (host().arg_count)(self.raw) }
    }

    /// The argument at `index`, counting from 0, converted to `T`. Arguments past the last are
    /// `nil`.
    pub fn arg<T: FromArg>(&self, index: usize) -> Result<T> {
        T::from_arg(self, index)
    }

    /// The object of class `T` at `index`, borrowed until the call returns.
    pub fn object<T: Class>(&self, index: usize) -> Result<&T> {
        // SAFETY: the engine borrows the object for the call, so it's a live `T`.
        Ok(unsafe { &*self.borrow_object::<T>(index, false)? })
    }

    /// The object of class `T` at `index`, borrowed exclusively until the call returns. It fails
    /// if anything else has the object borrowed, such as a method running on it, or an earlier
    /// argument.
    #[allow(
        clippy::mut_from_ref,
        reason = "the engine tracks the borrow, and fails a second one"
    )]
    pub fn object_mut<T: Class>(&self, index: usize) -> Result<&mut T> {
        // SAFETY: the engine borrows the object exclusively for the call, so it's a live `T`
        // nothing else refers to.
        Ok(unsafe { &mut *self.borrow_object::<T>(index, true)? })
    }

    fn borrow_object<T: Class>(&self, index: usize, exclusive: bool) -> Result<*mut T> {
        let class = self.classes.get::<T>()?;
        let mut object = ptr::null_mut();
        // SAFETY: valid for as long as `self` is.
        let status = unsafe { (host().arg_object)(self.raw, index, class, exclusive, &mut object) };
        check(status).map(|()| object.cast())
    }

    /// The engine's services, lent to scripts while the game loads and in `update`.
    pub fn context(&mut self) -> Result<&mut Context> {
        if self.context.is_none() {
            // SAFETY: valid for as long as `self` is.
            let raw = unsafe { (host().call_context)(self.raw) };
            if raw.is_null() {
                return Err(Error::new(
                    "this only works while the game loads or in `update`, not in `draw`",
                ));
            }
            self.context = Some(Context::new(raw));
        }
        Ok(self.context.as_mut().expect("set above"))
    }

    /// The canvas, lent to scripts in `draw`.
    pub fn canvas(&mut self) -> Result<&mut Canvas> {
        if self.canvas.is_none() {
            // SAFETY: valid for as long as `self` is.
            let raw = unsafe { (host().call_canvas)(self.raw) };
            if raw.is_null() {
                return Err(Error::new("drawing only works in `draw`"));
            }
            self.canvas = Some(Canvas::new(raw));
        }
        Ok(self.canvas.as_mut().expect("set above"))
    }
}

/// A type an argument converts to, as the engine's own functions convert it.
pub trait FromArg: Sized {
    fn from_arg(call: &Call, index: usize) -> Result<Self>;
}

/// Reads an argument with `read`, which writes it to the value it's given.
fn read<T: Default>(
    call: &Call,
    index: usize,
    read: unsafe extern "C" fn(*mut abi::Call, usize, *mut T) -> abi::Status,
) -> Result<T> {
    let mut value = T::default();
    // SAFETY: valid for as long as `call` is.
    let status = unsafe { read(call.raw, index, &mut value) };
    check(status).map(|()| value)
}

impl FromArg for bool {
    /// Whether the argument is truthy: anything but `nil` and `false`.
    fn from_arg(call: &Call, index: usize) -> Result<Self> {
        read(call, index, host().arg_boolean)
    }
}

impl FromArg for f64 {
    fn from_arg(call: &Call, index: usize) -> Result<Self> {
        read(call, index, host().arg_number)
    }
}

impl FromArg for f32 {
    fn from_arg(call: &Call, index: usize) -> Result<Self> {
        f64::from_arg(call, index).map(|value| value as f32)
    }
}

macro_rules! integer_args {
    ($($integer:ty),*) => {
        $(
            impl FromArg for $integer {
                /// The argument, a whole number in the type's range.
                fn from_arg(call: &Call, index: usize) -> Result<Self> {
                    let min = i64::try_from(<$integer>::MIN).unwrap_or(i64::MIN);
                    let max = i64::try_from(<$integer>::MAX).unwrap_or(i64::MAX);
                    let mut value = 0;
                    // SAFETY: valid for as long as `call` is.
                    let status = unsafe {
                        (host().arg_integer)(call.raw, index, min, max, &mut value)
                    };
                    check(status)?;
                    Ok(value.try_into().expect("in range"))
                }
            }
        )*
    };
}

integer_args!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

impl FromArg for String {
    fn from_arg(call: &Call, index: usize) -> Result<Self> {
        let text = read(call, index, host().arg_string)?;
        // SAFETY: the engine borrows the string for the call, and it's UTF-8.
        let text = unsafe { std::str::from_utf8_unchecked(text.as_bytes()) };
        Ok(text.to_owned())
    }
}

impl FromArg for Color {
    /// The argument, a hex string such as `"#f2c14e"` or a table `{ r, g, b, a }`.
    fn from_arg(call: &Call, index: usize) -> Result<Self> {
        read(call, index, host().arg_color)
    }
}

impl FromArg for Rect {
    /// The argument, a table `{ x, y, width, height }`.
    fn from_arg(call: &Call, index: usize) -> Result<Self> {
        read(call, index, host().arg_rect)
    }
}

impl FromArg for Sprite {
    fn from_arg(call: &Call, index: usize) -> Result<Self> {
        let mut sprite = ptr::null();
        // SAFETY: valid for as long as `call` is.
        let status = unsafe { (host().arg_sprite)(call.raw, index, &mut sprite) };
        check(status)?;
        Ok(Sprite::copy(sprite))
    }
}

impl<T: FromArg> FromArg for Option<T> {
    /// `None` for `nil`, or a missing argument.
    fn from_arg(call: &Call, index: usize) -> Result<Self> {
        // SAFETY: valid for as long as `call` is.
        if unsafe { (host().arg_is_nil)(call.raw, index) } {
            return Ok(None);
        }
        T::from_arg(call, index).map(Some)
    }
}

/// What a function's arguments convert to: one value, a tuple of them, or `()` for none.
/// Arguments beyond those are ignored.
pub trait FromArgs: Sized {
    fn from_args(call: &Call) -> Result<Self>;
}

impl FromArgs for () {
    fn from_args(_: &Call) -> Result<Self> {
        Ok(())
    }
}

impl<T: FromArg> FromArgs for T {
    fn from_args(call: &Call) -> Result<Self> {
        T::from_arg(call, 0)
    }
}

macro_rules! tuple_args {
    ($($name:ident $index:tt),+) => {
        impl<$($name: FromArg),+> FromArgs for ($($name,)+) {
            fn from_args(call: &Call) -> Result<Self> {
                Ok(($($name::from_arg(call, $index)?,)+))
            }
        }
    };
}

tuple_args!(A 0);
tuple_args!(A 0, B 1);
tuple_args!(A 0, B 1, C 2);
tuple_args!(A 0, B 1, C 2, D 3);
tuple_args!(A 0, B 1, C 2, D 3, E 4);
tuple_args!(A 0, B 1, C 2, D 3, E 4, F 5);
tuple_args!(A 0, B 1, C 2, D 3, E 4, F 5, G 6);
tuple_args!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7);

/// A type a function returns to scripts.
pub trait IntoResult {
    fn push(self, call: &Call) -> Result<()>;
}

impl IntoResult for bool {
    fn push(self, call: &Call) -> Result<()> {
        // SAFETY: valid for as long as `call` is.
        unsafe { (host().push_boolean)(call.raw, self) };
        Ok(())
    }
}

impl IntoResult for f64 {
    fn push(self, call: &Call) -> Result<()> {
        // SAFETY: valid for as long as `call` is.
        unsafe { (host().push_number)(call.raw, self) };
        Ok(())
    }
}

impl IntoResult for f32 {
    fn push(self, call: &Call) -> Result<()> {
        f64::from(self).push(call)
    }
}

macro_rules! integer_results {
    ($($integer:ty),*) => {
        $(
            impl IntoResult for $integer {
                /// The number, which loses precision beyond 2^53, as every Luau number does.
                fn push(self, call: &Call) -> Result<()> {
                    match i64::try_from(self) {
                        // SAFETY: valid for as long as `call` is.
                        Ok(value) => unsafe { (host().push_integer)(call.raw, value) },
                        Err(_) => return (self as f64).push(call),
                    }
                    Ok(())
                }
            }
        )*
    };
}

integer_results!(i8, i16, i32, i64, isize, u8, u16, u32, u64, usize);

impl IntoResult for &str {
    fn push(self, call: &Call) -> Result<()> {
        // SAFETY: valid for as long as `call` is, and the engine copies the string.
        unsafe { (host().push_string)(call.raw, abi::Str::new(self)) };
        Ok(())
    }
}

impl IntoResult for String {
    fn push(self, call: &Call) -> Result<()> {
        self.as_str().push(call)
    }
}

impl IntoResult for Sprite {
    fn push(self, call: &Call) -> Result<()> {
        // SAFETY: valid for as long as `call` is, and the engine copies the sprite.
        unsafe { (host().push_sprite)(call.raw, self.as_ptr()) };
        Ok(())
    }
}

impl<T: IntoResult> IntoResult for Option<T> {
    /// The value, or `nil` for `None`.
    fn push(self, call: &Call) -> Result<()> {
        match self {
            Some(value) => value.push(call),
            None => {
                // SAFETY: valid for as long as `call` is.
                unsafe { (host().push_nil)(call.raw) };
                Ok(())
            }
        }
    }
}

impl<T: Class> IntoResult for T {
    /// A new object, which scripts own from then on.
    fn push(self, call: &Call) -> Result<()> {
        let class = call.classes.get::<T>()?;
        let object = Box::into_raw(Box::new(self));
        // SAFETY: valid for as long as `call` is, and the engine owns the object from then on.
        unsafe { (host().push_object)(call.raw, class, object.cast()) };
        Ok(())
    }
}

/// What a function returns to scripts: one value, a tuple of them, or `()` for none.
pub trait IntoResults {
    fn push_all(self, call: &Call) -> Result<()>;
}

impl IntoResults for () {
    fn push_all(self, _: &Call) -> Result<()> {
        Ok(())
    }
}

impl<T: IntoResult> IntoResults for T {
    fn push_all(self, call: &Call) -> Result<()> {
        self.push(call)
    }
}

macro_rules! tuple_results {
    ($($name:ident $index:tt),+) => {
        impl<$($name: IntoResult),+> IntoResults for ($($name,)+) {
            fn push_all(self, call: &Call) -> Result<()> {
                $(self.$index.push(call)?;)+
                Ok(())
            }
        }
    };
}

tuple_results!(A 0);
tuple_results!(A 0, B 1);
tuple_results!(A 0, B 1, C 2);
tuple_results!(A 0, B 1, C 2, D 3);
tuple_results!(A 0, B 1, C 2, D 3, E 4);
tuple_results!(A 0, B 1, C 2, D 3, E 4, F 5);
tuple_results!(A 0, B 1, C 2, D 3, E 4, F 5, G 6);
tuple_results!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7);
