//! Keyboard and mouse input, queried by key, by mouse button, or by named action.

use std::{
    collections::{HashMap, HashSet},
    fmt,
    hash::Hash,
    str::FromStr,
};

use winit::keyboard::KeyCode;

/// Keyboard and mouse state for the current frame, available from
/// [`Context::input`](crate::Context::input).
///
/// Games usually bind named actions (such as `"confirm"`) to keys with [`Input::bind`] and query
/// those, so controls can be rebound without touching game code. Keys and mouse buttons can also
/// be queried directly.
///
/// A key or button is *pressed* or *released* on the frame its state changed, and *held* while it
/// is down. One tapped between two frames counts as both pressed and released on the next frame,
/// so quick taps are never lost.
#[derive(Debug, Default)]
pub struct Input {
    keys: Buttons<Key>,
    mouse_buttons: Buttons<MouseButton>,
    mouse_position: Option<[f32; 2]>,
    actions: HashMap<String, Vec<Key>>,
}

impl Input {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Binds `action` to `keys`, replacing any keys it was bound to before.
    pub fn bind(&mut self, action: impl Into<String>, keys: impl IntoIterator<Item = Key>) {
        self.actions
            .insert(action.into(), keys.into_iter().collect());
    }

    /// Whether `key` is down.
    pub fn key_held(&self, key: Key) -> bool {
        self.keys.held.contains(&key)
    }

    /// Whether `key` went down this frame.
    pub fn key_pressed(&self, key: Key) -> bool {
        self.keys.pressed.contains(&key)
    }

    /// Whether `key` went up this frame.
    pub fn key_released(&self, key: Key) -> bool {
        self.keys.released.contains(&key)
    }

    /// Whether any key bound to `action` is down. Unbound actions are never held.
    pub fn held(&self, action: &str) -> bool {
        self.any_key(action, |key| self.key_held(key))
    }

    /// Whether any key bound to `action` went down this frame.
    pub fn pressed(&self, action: &str) -> bool {
        self.any_key(action, |key| self.key_pressed(key))
    }

    /// Whether any key bound to `action` went up this frame.
    pub fn released(&self, action: &str) -> bool {
        self.any_key(action, |key| self.key_released(key))
    }

    /// The cursor's position in canvas pixels, or `None` while it is outside the window. With a
    /// fixed [`Resolution`](crate::graphics::Resolution), positions over the black bars fall
    /// outside the canvas. Use [`Camera::canvas_to_world`](crate::graphics::Camera::canvas_to_world)
    /// to find the world position under the cursor.
    pub fn mouse_position(&self) -> Option<[f32; 2]> {
        self.mouse_position
    }

    /// Whether `button` is down.
    pub fn mouse_held(&self, button: MouseButton) -> bool {
        self.mouse_buttons.held.contains(&button)
    }

    /// Whether `button` went down this frame.
    pub fn mouse_pressed(&self, button: MouseButton) -> bool {
        self.mouse_buttons.pressed.contains(&button)
    }

    /// Whether `button` went up this frame.
    pub fn mouse_released(&self, button: MouseButton) -> bool {
        self.mouse_buttons.released.contains(&button)
    }

    fn any_key(&self, action: &str, test: impl Fn(Key) -> bool) -> bool {
        self.actions
            .get(action)
            .is_some_and(|keys| keys.iter().copied().any(test))
    }

    pub(crate) fn key_down(&mut self, key: Key) {
        self.keys.down(key);
    }

    pub(crate) fn key_up(&mut self, key: Key) {
        self.keys.up(key);
    }

    pub(crate) fn mouse_down(&mut self, button: MouseButton) {
        self.mouse_buttons.down(button);
    }

    pub(crate) fn mouse_up(&mut self, button: MouseButton) {
        self.mouse_buttons.up(button);
    }

    pub(crate) fn set_mouse_position(&mut self, position: Option<[f32; 2]>) {
        self.mouse_position = position;
    }

    /// Releases every held key and button, for when the window loses focus and stops receiving
    /// input events.
    pub(crate) fn release_all(&mut self) {
        self.keys.release_all();
        self.mouse_buttons.release_all();
    }

    /// Clears this frame's presses and releases. Call after the game has updated.
    pub(crate) fn end_frame(&mut self) {
        self.keys.end_frame();
        self.mouse_buttons.end_frame();
    }
}

/// Held, pressed and released state for a set of keys or buttons.
#[derive(Debug)]
struct Buttons<T> {
    held: HashSet<T>,
    pressed: HashSet<T>,
    released: HashSet<T>,
}

impl<T> Default for Buttons<T> {
    fn default() -> Self {
        Self {
            held: HashSet::new(),
            pressed: HashSet::new(),
            released: HashSet::new(),
        }
    }
}

impl<T: Copy + Eq + Hash> Buttons<T> {
    fn down(&mut self, button: T) {
        // Key repeat sends more presses while a key is held; only the first counts.
        if self.held.insert(button) {
            self.pressed.insert(button);
        }
    }

    fn up(&mut self, button: T) {
        if self.held.remove(&button) {
            self.released.insert(button);
        }
    }

    fn release_all(&mut self) {
        self.released.extend(self.held.drain());
    }

    fn end_frame(&mut self) {
        self.pressed.clear();
        self.released.clear();
    }
}

/// A mouse button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Back,
    Forward,
}

impl MouseButton {
    pub(crate) fn from_winit(button: winit::event::MouseButton) -> Option<Self> {
        use winit::event::MouseButton as Winit;
        match button {
            Winit::Left => Some(Self::Left),
            Winit::Right => Some(Self::Right),
            Winit::Middle => Some(Self::Middle),
            Winit::Back => Some(Self::Back),
            Winit::Forward => Some(Self::Forward),
            Winit::Other(_) => None,
        }
    }
}

/// Parses a button name such as `"left"`, ignoring case.
impl FromStr for MouseButton {
    type Err = ParseInputError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        [
            ("Left", Self::Left),
            ("Right", Self::Right),
            ("Middle", Self::Middle),
            ("Back", Self::Back),
            ("Forward", Self::Forward),
        ]
        .into_iter()
        .find(|(candidate, _)| name.eq_ignore_ascii_case(candidate))
        .map(|(_, button)| button)
        .ok_or_else(|| ParseInputError(format!("unknown mouse button `{name}`")))
    }
}

/// An unrecognized key or mouse button name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseInputError(String);

impl fmt::Display for ParseInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseInputError {}

macro_rules! keys {
    ($($key:ident => $code:ident,)*) => {
        /// A physical key, named after the key in the same position on a US QWERTY keyboard.
        ///
        /// Keys are identified by position rather than by the character they type, so `W`, `A`,
        /// `S` and `D` stay in the same place on every keyboard layout (on AZERTY, `Key::W` is the
        /// key labelled Z).
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum Key {
            $($key,)*
        }

        impl Key {
            /// The engine's key for a winit key code, if it has one.
            pub(crate) fn from_winit(code: KeyCode) -> Option<Self> {
                match code {
                    $(KeyCode::$code => Some(Self::$key),)*
                    _ => None,
                }
            }
        }

        /// Parses a key's name, as written in [`Key`] (such as `"Up"` or `"Digit1"`), ignoring
        /// case.
        impl FromStr for Key {
            type Err = ParseInputError;

            fn from_str(name: &str) -> Result<Self, Self::Err> {
                $(if name.eq_ignore_ascii_case(stringify!($key)) {
                    return Ok(Self::$key);
                })*
                Err(ParseInputError(format!("unknown key `{name}`")))
            }
        }
    };
}

keys! {
    A => KeyA, B => KeyB, C => KeyC, D => KeyD, E => KeyE, F => KeyF, G => KeyG, H => KeyH,
    I => KeyI, J => KeyJ, K => KeyK, L => KeyL, M => KeyM, N => KeyN, O => KeyO, P => KeyP,
    Q => KeyQ, R => KeyR, S => KeyS, T => KeyT, U => KeyU, V => KeyV, W => KeyW, X => KeyX,
    Y => KeyY, Z => KeyZ,
    Digit0 => Digit0, Digit1 => Digit1, Digit2 => Digit2, Digit3 => Digit3, Digit4 => Digit4,
    Digit5 => Digit5, Digit6 => Digit6, Digit7 => Digit7, Digit8 => Digit8, Digit9 => Digit9,
    Up => ArrowUp, Down => ArrowDown, Left => ArrowLeft, Right => ArrowRight,
    Space => Space, Enter => Enter, Escape => Escape, Tab => Tab, Backspace => Backspace,
    LeftShift => ShiftLeft, RightShift => ShiftRight,
    LeftControl => ControlLeft, RightControl => ControlRight,
    LeftAlt => AltLeft, RightAlt => AltRight,
    Insert => Insert, Delete => Delete, Home => Home, End => End,
    PageUp => PageUp, PageDown => PageDown,
    Minus => Minus, Equal => Equal, Comma => Comma, Period => Period, Slash => Slash,
    Semicolon => Semicolon, Quote => Quote, Backquote => Backquote, Backslash => Backslash,
    LeftBracket => BracketLeft, RightBracket => BracketRight,
    F1 => F1, F2 => F2, F3 => F3, F4 => F4, F5 => F5, F6 => F6,
    F7 => F7, F8 => F8, F9 => F9, F10 => F10, F11 => F11, F12 => F12,
    Numpad0 => Numpad0, Numpad1 => Numpad1, Numpad2 => Numpad2, Numpad3 => Numpad3,
    Numpad4 => Numpad4, Numpad5 => Numpad5, Numpad6 => Numpad6, Numpad7 => Numpad7,
    Numpad8 => Numpad8, Numpad9 => Numpad9, NumpadEnter => NumpadEnter,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn press_is_held_until_released_but_pressed_for_one_frame() {
        let mut input = Input::new();
        input.key_down(Key::W);
        assert!(input.key_pressed(Key::W) && input.key_held(Key::W));

        input.end_frame();
        assert!(!input.key_pressed(Key::W) && input.key_held(Key::W));

        input.key_up(Key::W);
        assert!(input.key_released(Key::W) && !input.key_held(Key::W));

        input.end_frame();
        assert!(!input.key_released(Key::W));
    }

    #[test]
    fn key_repeat_does_not_press_again() {
        let mut input = Input::new();
        input.key_down(Key::Space);
        input.end_frame();
        input.key_down(Key::Space);
        assert!(!input.key_pressed(Key::Space));
        assert!(input.key_held(Key::Space));
    }

    #[test]
    fn tap_between_frames_is_pressed_and_released() {
        let mut input = Input::new();
        input.key_down(Key::Enter);
        input.key_up(Key::Enter);
        assert!(input.key_pressed(Key::Enter));
        assert!(input.key_released(Key::Enter));
        assert!(!input.key_held(Key::Enter));
    }

    #[test]
    fn release_without_press_is_ignored() {
        let mut input = Input::new();
        input.key_up(Key::A);
        assert!(!input.key_released(Key::A));
    }

    #[test]
    fn release_all_releases_held_keys_and_buttons() {
        let mut input = Input::new();
        input.key_down(Key::LeftAlt);
        input.key_down(Key::Tab);
        input.mouse_down(MouseButton::Left);
        input.end_frame();
        input.release_all();
        assert!(input.key_released(Key::LeftAlt) && input.key_released(Key::Tab));
        assert!(!input.key_held(Key::LeftAlt) && !input.key_held(Key::Tab));
        assert!(input.mouse_released(MouseButton::Left));
        assert!(!input.mouse_held(MouseButton::Left));
    }

    #[test]
    fn mouse_buttons_track_like_keys() {
        let mut input = Input::new();
        input.mouse_down(MouseButton::Right);
        assert!(input.mouse_pressed(MouseButton::Right) && input.mouse_held(MouseButton::Right));
        input.end_frame();
        input.mouse_up(MouseButton::Right);
        assert!(input.mouse_released(MouseButton::Right));
        assert!(!input.mouse_held(MouseButton::Right));
    }

    #[test]
    fn actions_follow_any_bound_key() {
        let mut input = Input::new();
        input.bind("up", [Key::Up, Key::W]);
        input.key_down(Key::W);
        assert!(input.pressed("up") && input.held("up"));
        assert!(!input.held("down"), "unbound actions are never held");

        input.end_frame();
        input.key_up(Key::W);
        assert!(input.released("up") && !input.held("up"));
    }

    #[test]
    fn rebinding_replaces_keys() {
        let mut input = Input::new();
        input.bind("confirm", [Key::Enter]);
        input.bind("confirm", [Key::Space]);
        input.key_down(Key::Enter);
        assert!(!input.held("confirm"));
        input.key_down(Key::Space);
        assert!(input.held("confirm"));
    }

    #[test]
    fn winit_codes_map_by_position() {
        assert_eq!(Key::from_winit(KeyCode::KeyW), Some(Key::W));
        assert_eq!(Key::from_winit(KeyCode::ArrowLeft), Some(Key::Left));
        assert_eq!(Key::from_winit(KeyCode::MediaPlayPause), None);
    }

    #[test]
    fn names_parse_ignoring_case() {
        assert_eq!("right".parse(), Ok(Key::Right));
        assert_eq!("DIGIT1".parse(), Ok(Key::Digit1));
        assert_eq!("LeftShift".parse(), Ok(Key::LeftShift));
        assert!("Shift".parse::<Key>().is_err());
        assert_eq!("middle".parse(), Ok(MouseButton::Middle));
        assert!("wheel".parse::<MouseButton>().is_err());
    }
}
