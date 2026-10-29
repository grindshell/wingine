//! Keyboard input, queried by key or by named action.

use std::collections::{HashMap, HashSet};

use winit::keyboard::KeyCode;

/// Keyboard state for the current frame, available from [`Context::input`](crate::Context::input).
///
/// Games usually bind named actions (such as `"confirm"`) to keys with [`Input::bind`] and query
/// those, so controls can be rebound without touching game code. Keys can also be queried
/// directly.
///
/// A key is *pressed* or *released* on the frame its state changed, and *held* while it is down.
/// A key tapped between two frames counts as both pressed and released on the next frame, so
/// quick taps are never lost.
#[derive(Debug, Default)]
pub struct Input {
    held: HashSet<Key>,
    pressed: HashSet<Key>,
    released: HashSet<Key>,
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
        self.held.contains(&key)
    }

    /// Whether `key` went down this frame.
    pub fn key_pressed(&self, key: Key) -> bool {
        self.pressed.contains(&key)
    }

    /// Whether `key` went up this frame.
    pub fn key_released(&self, key: Key) -> bool {
        self.released.contains(&key)
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

    fn any_key(&self, action: &str, test: impl Fn(Key) -> bool) -> bool {
        self.actions
            .get(action)
            .is_some_and(|keys| keys.iter().copied().any(test))
    }

    pub(crate) fn key_down(&mut self, key: Key) {
        // Key repeat sends more presses while a key is held; only the first counts.
        if self.held.insert(key) {
            self.pressed.insert(key);
        }
    }

    pub(crate) fn key_up(&mut self, key: Key) {
        if self.held.remove(&key) {
            self.released.insert(key);
        }
    }

    /// Releases every held key, for when the window loses focus and stops receiving key events.
    pub(crate) fn release_all(&mut self) {
        self.released.extend(self.held.drain());
    }

    /// Clears this frame's presses and releases. Call after the game has updated.
    pub(crate) fn end_frame(&mut self) {
        self.pressed.clear();
        self.released.clear();
    }
}

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
    fn release_all_releases_held_keys() {
        let mut input = Input::new();
        input.key_down(Key::LeftAlt);
        input.key_down(Key::Tab);
        input.end_frame();
        input.release_all();
        assert!(input.key_released(Key::LeftAlt) && input.key_released(Key::Tab));
        assert!(!input.key_held(Key::LeftAlt) && !input.key_held(Key::Tab));
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
}
