//! Calls from scripts, and converting their arguments and results.

use std::ptr;

use crate::{
    Canvas, Class, Classes, Color, Context, Error, Mesh, Rect, Result, Sprite, Vec3, abi, check,
    host,
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

    /// The bytes of the Luau `buffer` at `index`, as they are now, borrowed until the call
    /// returns. They're a copy, which [`Call::write_buffer`] doesn't change.
    pub fn buffer(&self, index: usize) -> Result<&[u8]> {
        let (mut bytes, mut len) = (ptr::null(), 0);
        // SAFETY: valid for as long as `self` is.
        let status = unsafe { (host().arg_buffer)(self.raw, index, &mut bytes, &mut len) };
        check(status)?;
        if len == 0 {
            return Ok(&[]);
        }
        // SAFETY: the engine keeps the bytes until the call returns, and never changes them.
        Ok(unsafe { std::slice::from_raw_parts(bytes, len) })
    }

    /// Writes `bytes` into the Luau `buffer` at `index`, from its byte `offset` on, for the
    /// script to read once the call returns. It fails if they'd reach past the buffer's end: a
    /// buffer never grows.
    pub fn write_buffer(&mut self, index: usize, offset: usize, bytes: &[u8]) -> Result<()> {
        // SAFETY: valid for as long as `self` is, and the bytes for the call.
        let status =
            unsafe { (host().write_buffer)(self.raw, index, offset, bytes.as_ptr(), bytes.len()) };
        check(status)
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

    /// The Luau type scripts pass for it, as the library's definitions say it: `any`, unless
    /// the type says otherwise.
    fn luau_type() -> String {
        "any".into()
    }
}

/// The Luau type of what may be an `inner` or `nil`.
fn optional(inner: String) -> String {
    if inner == "any" || inner.ends_with('?') {
        inner
    } else if inner.contains("->") || inner.contains('|') {
        format!("({inner})?")
    } else {
        format!("{inner}?")
    }
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

    fn luau_type() -> String {
        "boolean".into()
    }
}

impl FromArg for f64 {
    fn from_arg(call: &Call, index: usize) -> Result<Self> {
        read(call, index, host().arg_number)
    }

    fn luau_type() -> String {
        "number".into()
    }
}

impl FromArg for f32 {
    fn from_arg(call: &Call, index: usize) -> Result<Self> {
        f64::from_arg(call, index).map(|value| value as f32)
    }

    fn luau_type() -> String {
        "number".into()
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

                fn luau_type() -> String {
                    "number".into()
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

    fn luau_type() -> String {
        "string".into()
    }
}

impl FromArg for Color {
    /// The argument, a hex string such as `"#f2c14e"` or a table `{ r, g, b, a }`.
    fn from_arg(call: &Call, index: usize) -> Result<Self> {
        read(call, index, host().arg_color)
    }

    fn luau_type() -> String {
        "Color".into()
    }
}

impl FromArg for Rect {
    /// The argument, a table `{ x, y, width, height }`.
    fn from_arg(call: &Call, index: usize) -> Result<Self> {
        read(call, index, host().arg_rect)
    }

    fn luau_type() -> String {
        "Rect".into()
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

    fn luau_type() -> String {
        "Sprite".into()
    }
}

impl FromArg for Mesh {
    fn from_arg(call: &Call, index: usize) -> Result<Self> {
        let mut mesh = ptr::null();
        // SAFETY: valid for as long as `call` is.
        let status = unsafe { (host().arg_mesh)(call.raw, index, &mut mesh) };
        check(status)?;
        Ok(Mesh::copy(mesh))
    }

    fn luau_type() -> String {
        "Mesh".into()
    }
}

impl FromArg for Vec3 {
    /// The argument, a Luau vector such as `vector.create(1, 2, 3)`.
    fn from_arg(call: &Call, index: usize) -> Result<Self> {
        read(call, index, host().arg_vector)
    }

    fn luau_type() -> String {
        "vector".into()
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

    fn luau_type() -> String {
        optional(T::luau_type())
    }
}

/// The bytes of a Luau `buffer`: a list of anything, packed as the library and its scripts
/// agree, which crosses in one call however long it is. As an argument, it's a copy of the
/// script's buffer; as a result, a new buffer. Scripts read and write one with Luau's `buffer`
/// library, whose numbers are little-endian, as `f32::to_le_bytes` and the like write them.
///
/// A list of values that convert on their own, such as numbers or strings, can cross as a
/// `Vec` of them instead, which scripts see as an array.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Buffer(pub Vec<u8>);

impl From<Vec<u8>> for Buffer {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

impl FromArg for Buffer {
    /// A copy of the buffer's bytes. [`Call::buffer`] borrows them instead.
    fn from_arg(call: &Call, index: usize) -> Result<Self> {
        call.buffer(index).map(|bytes| Self(bytes.to_vec()))
    }

    fn luau_type() -> String {
        "buffer".into()
    }
}

impl<T: FromArg> FromArg for Vec<T> {
    /// The argument, an array: a table's values from 1 to its length, each converted to `T`.
    fn from_arg(call: &Call, index: usize) -> Result<Self> {
        let (mut first, mut count) = (0, 0);
        // SAFETY: valid for as long as `call` is.
        let status = unsafe { (host().arg_array)(call.raw, index, &mut first, &mut count) };
        check(status)?;
        // The engine made the array's values arguments of the call, from `first` on.
        (first..first + count)
            .map(|item| T::from_arg(call, item))
            .collect()
    }

    fn luau_type() -> String {
        format!("{{ {} }}", T::luau_type())
    }
}

/// What a function's arguments convert to: one value, a tuple of them, or `()` for none.
/// Arguments beyond those are ignored.
pub trait FromArgs: Sized {
    fn from_args(call: &Call) -> Result<Self>;

    /// The Luau type of each argument, in order, for the library's definitions, or `None` if
    /// the type doesn't say, as one that reads its arguments by hand doesn't unless it's
    /// written to.
    fn luau_types() -> Option<Vec<String>> {
        None
    }
}

impl FromArgs for () {
    fn from_args(_: &Call) -> Result<Self> {
        Ok(())
    }

    fn luau_types() -> Option<Vec<String>> {
        Some(Vec::new())
    }
}

impl<T: FromArg> FromArgs for T {
    fn from_args(call: &Call) -> Result<Self> {
        T::from_arg(call, 0)
    }

    fn luau_types() -> Option<Vec<String>> {
        Some(vec![T::luau_type()])
    }
}

macro_rules! tuple_args {
    ($($name:ident $index:tt),+) => {
        impl<$($name: FromArg),+> FromArgs for ($($name,)+) {
            fn from_args(call: &Call) -> Result<Self> {
                Ok(($($name::from_arg(call, $index)?,)+))
            }

            fn luau_types() -> Option<Vec<String>> {
                Some(vec![$($name::luau_type()),+])
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

    /// The Luau type scripts get for it, as the library's definitions say it: `any`, unless
    /// the type says otherwise.
    fn luau_type() -> String {
        "any".into()
    }
}

impl IntoResult for bool {
    fn push(self, call: &Call) -> Result<()> {
        // SAFETY: valid for as long as `call` is.
        unsafe { (host().push_boolean)(call.raw, self) };
        Ok(())
    }

    fn luau_type() -> String {
        "boolean".into()
    }
}

impl IntoResult for f64 {
    fn push(self, call: &Call) -> Result<()> {
        // SAFETY: valid for as long as `call` is.
        unsafe { (host().push_number)(call.raw, self) };
        Ok(())
    }

    fn luau_type() -> String {
        "number".into()
    }
}

impl IntoResult for f32 {
    fn push(self, call: &Call) -> Result<()> {
        f64::from(self).push(call)
    }

    fn luau_type() -> String {
        "number".into()
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

                fn luau_type() -> String {
                    "number".into()
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

    fn luau_type() -> String {
        "string".into()
    }
}

impl IntoResult for String {
    fn push(self, call: &Call) -> Result<()> {
        self.as_str().push(call)
    }

    fn luau_type() -> String {
        "string".into()
    }
}

impl IntoResult for Sprite {
    fn push(self, call: &Call) -> Result<()> {
        // SAFETY: valid for as long as `call` is, and the engine copies the sprite.
        unsafe { (host().push_sprite)(call.raw, self.as_ptr()) };
        Ok(())
    }

    fn luau_type() -> String {
        "Sprite".into()
    }
}

impl IntoResult for Mesh {
    fn push(self, call: &Call) -> Result<()> {
        // SAFETY: valid for as long as `call` is, and the engine copies the handle.
        unsafe { (host().push_mesh)(call.raw, self.as_ptr()) };
        Ok(())
    }

    fn luau_type() -> String {
        "Mesh".into()
    }
}

impl IntoResult for Vec3 {
    /// A Luau vector.
    fn push(self, call: &Call) -> Result<()> {
        // SAFETY: valid for as long as `call` is.
        unsafe { (host().push_vector)(call.raw, self) };
        Ok(())
    }

    fn luau_type() -> String {
        "vector".into()
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

    fn luau_type() -> String {
        optional(T::luau_type())
    }
}

impl IntoResult for Buffer {
    /// A new Luau `buffer` of the bytes.
    fn push(self, call: &Call) -> Result<()> {
        // SAFETY: valid for as long as `call` is, and the engine copies the bytes.
        unsafe { (host().push_buffer)(call.raw, self.0.as_ptr(), self.0.len()) };
        Ok(())
    }

    fn luau_type() -> String {
        "buffer".into()
    }
}

impl<T: IntoResult> IntoResult for Vec<T> {
    /// An array of the values: a table of them, from 1.
    fn push(self, call: &Call) -> Result<()> {
        let count = self.len();
        for value in self {
            value.push(call)?;
        }
        // SAFETY: valid for as long as `call` is.
        check(unsafe { (host().push_array)(call.raw, count) })
    }

    fn luau_type() -> String {
        format!("{{ {} }}", T::luau_type())
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

    /// The class's name.
    fn luau_type() -> String {
        T::NAME.into()
    }
}

/// What a function returns to scripts: one value, a tuple of them, or `()` for none.
pub trait IntoResults {
    fn push_all(self, call: &Call) -> Result<()>;

    /// The Luau type of each result, in order, for the library's definitions, or `None` if the
    /// type doesn't say.
    fn luau_types() -> Option<Vec<String>> {
        None
    }
}

impl IntoResults for () {
    fn push_all(self, _: &Call) -> Result<()> {
        Ok(())
    }

    fn luau_types() -> Option<Vec<String>> {
        Some(Vec::new())
    }
}

impl<T: IntoResult> IntoResults for T {
    fn push_all(self, call: &Call) -> Result<()> {
        self.push(call)
    }

    fn luau_types() -> Option<Vec<String>> {
        Some(vec![T::luau_type()])
    }
}

macro_rules! tuple_results {
    ($($name:ident $index:tt),+) => {
        impl<$($name: IntoResult),+> IntoResults for ($($name,)+) {
            fn push_all(self, call: &Call) -> Result<()> {
                $(self.$index.push(call)?;)+
                Ok(())
            }

            fn luau_types() -> Option<Vec<String>> {
                Some(vec![$($name::luau_type()),+])
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
