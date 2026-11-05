//! Keyboard, mouse and gamepad input, queried by key, by button, or by named action.

use std::{
    collections::{HashMap, HashSet},
    fmt,
    hash::Hash,
    str::FromStr,
};

use winit::keyboard::KeyCode;

/// How far a button or a stick's direction must be pushed, from 0 to 1, to be held.
const PRESS: f32 = 0.5;
/// How far a held button or stick's direction must fall back, from 0 to 1, to be released: below
/// [`PRESS`], so one resting near the line doesn't flicker between the two.
const RELEASE: f32 = 0.4;
/// How far a stick moves from its middle, from 0 to 1, before it counts as pushed at all, since
/// sticks rarely come to rest exactly in the middle.
const STICK_DEAD_ZONE: f32 = 0.15;
/// How far a stick must move from its middle to count as pushed all the way, since many don't
/// reach their edges, least of all on the diagonals.
const STICK_FULL: f32 = 0.95;

/// Keyboard, mouse and gamepad state for the current frame, available from
/// [`Context::input`](crate::Context::input).
///
/// Games usually bind named actions (such as `"confirm"`) to keys and gamepad buttons with
/// [`Input::bind`] and query those, so controls can be rebound without touching game code. Keys,
/// mouse buttons and gamepads can also be queried directly.
///
/// A key or button is *pressed* or *released* on the frame its state changed, and *held* while it
/// is down. One tapped between two frames counts as both pressed and released on the next frame,
/// so quick taps are never lost.
///
/// A key held down isn't pressed again, but it *repeats* as the system repeats keys: after a
/// delay, then at a steady rate, both as the player's settings say. [`Input::key_repeated`] and
/// [`Input::repeated`] tell of repeats, so a game that takes a step for each press and each
/// repeat walks while a key is held.
#[derive(Debug, Default)]
pub struct Input {
    keys: Buttons<Key>,
    mouse_buttons: Buttons<MouseButton>,
    mouse_position: Option<[f32; 2]>,
    /// Every gamepad connected since the game started, by number: gamepad 1 is first. Those
    /// disconnected keep their number, for when they come back.
    gamepads: Vec<Gamepad>,
    actions: HashMap<String, Binding>,
}

/// What an action is bound to.
#[derive(Debug)]
struct Binding {
    keys: Vec<Key>,
    buttons: Vec<GamepadButton>,
    /// The gamepad whose buttons count, or `None` for any gamepad's.
    gamepad: Option<u32>,
}

impl Input {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Binds `action` to `controls`, keys and gamepad buttons, replacing what it was bound to
    /// before. Its buttons are any gamepad's: [`Input::bind_gamepad`] binds one gamepad's.
    pub fn bind(
        &mut self,
        action: impl Into<String>,
        controls: impl IntoIterator<Item = impl Into<Control>>,
    ) {
        self.bind_to(action.into(), controls.into_iter().map(Into::into), None);
    }

    /// Binds `action` to `controls`, as [`Input::bind`] does, but only `gamepad`'s buttons count,
    /// as when each player has a gamepad of their own. Gamepads are numbered from 1.
    pub fn bind_gamepad(
        &mut self,
        action: impl Into<String>,
        gamepad: u32,
        controls: impl IntoIterator<Item = impl Into<Control>>,
    ) {
        self.bind_to(
            action.into(),
            controls.into_iter().map(Into::into),
            Some(gamepad),
        );
    }

    pub(crate) fn bind_to(
        &mut self,
        action: String,
        controls: impl IntoIterator<Item = Control>,
        gamepad: Option<u32>,
    ) {
        let mut binding = Binding {
            keys: Vec::new(),
            buttons: Vec::new(),
            gamepad,
        };
        for control in controls {
            match control {
                Control::Key(key) => binding.keys.push(key),
                Control::Button(button) => binding.buttons.push(button),
            }
        }
        self.actions.insert(action, binding);
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

    /// Whether the system repeated `key` this frame, as it does while a key is held: after a
    /// delay, then at a steady rate. The press that started it isn't a repeat, so a game that
    /// acts when `key_pressed(key) || key_repeated(key)` acts once as the key goes down, then
    /// again for each repeat.
    pub fn key_repeated(&self, key: Key) -> bool {
        self.keys.repeated.contains(&key)
    }

    /// Whether anything bound to `action` is down. Unbound actions are never held.
    pub fn held(&self, action: &str) -> bool {
        self.any_bound(action, Self::key_held, Gamepad::held)
    }

    /// Whether anything bound to `action` went down this frame.
    pub fn pressed(&self, action: &str) -> bool {
        self.any_bound(action, Self::key_pressed, Gamepad::pressed)
    }

    /// Whether anything bound to `action` went up this frame.
    pub fn released(&self, action: &str) -> bool {
        self.any_bound(action, Self::key_released, Gamepad::released)
    }

    /// Whether a key bound to `action` repeated this frame, as [`Input::key_repeated`] tells.
    /// Gamepad buttons don't repeat, since the system repeats only keys.
    pub fn repeated(&self, action: &str) -> bool {
        self.any_bound(action, Self::key_repeated, |_, _| false)
    }

    /// How far `action` is pushed, from 0 to 1: 1 while one of its keys is held, and otherwise
    /// the furthest of its buttons is pushed, which for a trigger or a stick's direction can be
    /// anywhere between. Unbound actions are never pushed.
    pub fn strength(&self, action: &str) -> f32 {
        let Some(binding) = self.actions.get(action) else {
            return 0.0;
        };
        if binding.keys.iter().any(|&key| self.key_held(key)) {
            return 1.0;
        }
        self.matching_gamepads(binding.gamepad)
            .flat_map(|gamepad| {
                binding
                    .buttons
                    .iter()
                    .map(|&button| gamepad.strength(button))
            })
            .fold(0.0, f32::max)
    }

    /// An axis made of two actions, from -1 to 1: how far `positive` is pushed less how far
    /// `negative` is. Bound to keys, it's -1, 0 or 1; bound to a stick's two directions, it's
    /// where the stick is along them.
    pub fn axis(&self, negative: &str, positive: &str) -> f32 {
        self.strength(positive) - self.strength(negative)
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

    /// Gamepad `number`, counting from 1 in the order they were first connected, if it has ever
    /// been connected. A gamepad keeps its number while it's disconnected, and gets it back when
    /// it's connected again.
    pub fn gamepad(&self, number: u32) -> Option<&Gamepad> {
        let index = usize::try_from(number).ok()?.checked_sub(1)?;
        self.gamepads.get(index)
    }

    /// The gamepads connected now, and their numbers, in order.
    pub fn gamepads(&self) -> impl Iterator<Item = (u32, &Gamepad)> {
        (1..)
            .zip(&self.gamepads)
            .filter(|(_, gamepad)| gamepad.connected)
    }

    /// Gamepad `number`, or with `None`, every gamepad.
    pub(crate) fn matching_gamepads(&self, number: Option<u32>) -> impl Iterator<Item = &Gamepad> {
        (1..)
            .zip(&self.gamepads)
            .filter(move |&(n, _)| number.is_none_or(|number| number == n))
            .map(|(_, gamepad)| gamepad)
    }

    fn any_bound(
        &self,
        action: &str,
        key: impl Fn(&Self, Key) -> bool,
        button: impl Fn(&Gamepad, GamepadButton) -> bool,
    ) -> bool {
        self.actions.get(action).is_some_and(|binding| {
            binding.keys.iter().any(|&k| key(self, k))
                || self
                    .matching_gamepads(binding.gamepad)
                    .any(|gamepad| binding.buttons.iter().any(|&b| button(gamepad, b)))
        })
    }

    pub(crate) fn key_down(&mut self, key: Key) {
        self.keys.down(key);
    }

    /// Repeats `key`, as the system does while a key is held. A repeat of a key that isn't held,
    /// as when the window gains focus while it's down, presses it.
    pub(crate) fn key_repeat(&mut self, key: Key) {
        self.keys.repeat(key);
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

    /// Connects gamepad `number`, called `name`: a new one, or one that was disconnected.
    /// Numbers start at 1.
    pub(crate) fn connect_gamepad(&mut self, number: u32, name: String) {
        let Some(index) = (number as usize).checked_sub(1) else {
            return;
        };
        if self.gamepads.len() <= index {
            self.gamepads.resize_with(index + 1, Gamepad::default);
        }
        let gamepad = &mut self.gamepads[index];
        gamepad.name = name;
        gamepad.connected = true;
    }

    /// Disconnects gamepad `number`, releasing everything held on it.
    pub(crate) fn disconnect_gamepad(&mut self, number: u32) {
        if let Some(gamepad) = self.gamepad_mut(number) {
            gamepad.release_all();
            gamepad.connected = false;
        }
    }

    /// Sets how far `button` on gamepad `number` is pushed, from 0 to 1. A stick's direction
    /// moves the stick, that way or back to the middle.
    pub(crate) fn set_gamepad_button(&mut self, number: u32, button: GamepadButton, strength: f32) {
        if let Some(gamepad) = self.gamepad_mut(number) {
            gamepad.set_button(button, strength);
        }
    }

    /// Moves `stick` on gamepad `number` along one axis, `x` or `y` (`y` down), from -1 to 1.
    pub(crate) fn set_gamepad_axis(&mut self, number: u32, stick: Stick, axis: Axis, value: f32) {
        if let Some(gamepad) = self.gamepad_mut(number) {
            gamepad.set_axis(stick, axis, value);
        }
    }

    fn gamepad_mut(&mut self, number: u32) -> Option<&mut Gamepad> {
        let index = usize::try_from(number).ok()?.checked_sub(1)?;
        self.gamepads.get_mut(index)
    }

    /// Releases every held key and button, for when the window loses focus and stops receiving
    /// input events.
    pub(crate) fn release_all(&mut self) {
        self.keys.release_all();
        self.mouse_buttons.release_all();
        for gamepad in &mut self.gamepads {
            gamepad.release_all();
        }
    }

    /// Clears this frame's presses and releases. Call after the game has updated.
    pub(crate) fn end_frame(&mut self) {
        self.keys.end_frame();
        self.mouse_buttons.end_frame();
        for gamepad in &mut self.gamepads {
            gamepad.buttons.end_frame();
        }
    }
}

/// A gamepad's state, from [`Input::gamepad`].
///
/// Its buttons include its sticks' directions, such as [`GamepadButton::LeftStickLeft`], each
/// pushed as far as the stick is that way, so a stick can be bound to actions as the D-pad is.
/// Sticks are measured from a dead zone around their middle, inside which they count as at rest,
/// to a little short of their edge, where they count as pushed all the way. A button or a
/// direction is held once it's pushed halfway, and released when it falls back below 0.4.
#[derive(Debug, Default)]
pub struct Gamepad {
    name: String,
    connected: bool,
    buttons: Buttons<GamepadButton>,
    /// How far each button is pushed, from 0 to 1, by `GamepadButton as usize`.
    strengths: [f32; GamepadButton::ALL.len()],
    /// Where each stick is, `x` and `y` from -1 to 1 (`y` down), before the dead zone, by
    /// `Stick as usize`.
    sticks: [[f32; 2]; 2],
}

impl Gamepad {
    /// The name its driver gives it, such as `"Xbox Controller"`.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether it's connected. A gamepad that isn't has nothing held.
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Whether `button` is down.
    pub fn held(&self, button: GamepadButton) -> bool {
        self.buttons.held.contains(&button)
    }

    /// Whether `button` went down this frame.
    pub fn pressed(&self, button: GamepadButton) -> bool {
        self.buttons.pressed.contains(&button)
    }

    /// Whether `button` went up this frame.
    pub fn released(&self, button: GamepadButton) -> bool {
        self.buttons.released.contains(&button)
    }

    /// How far `button` is pushed, from 0 to 1: a trigger, or a stick's direction, can be
    /// anywhere between.
    pub fn strength(&self, button: GamepadButton) -> f32 {
        self.strengths[button as usize]
    }

    fn set_button(&mut self, button: GamepadButton, strength: f32) {
        let strength = strength.clamp(0.0, 1.0);
        if let Some((stick, axis, sign)) = button.stick_direction() {
            let value = &mut self.sticks[stick as usize][axis as usize];
            // Pushing one way moves the stick that way; letting go of a way the stick isn't
            // pushed leaves it where it is.
            if strength > 0.0 {
                *value = sign * strength;
            } else if *value * sign > 0.0 {
                *value = 0.0;
            }
            self.update_stick(stick);
            return;
        }
        self.strengths[button as usize] = strength;
        self.update_held(button);
    }

    fn set_axis(&mut self, stick: Stick, axis: Axis, value: f32) {
        self.sticks[stick as usize][axis as usize] = value.clamp(-1.0, 1.0);
        self.update_stick(stick);
    }

    /// Works out how far `stick` is pushed each way from where it is.
    fn update_stick(&mut self, stick: Stick) {
        let [x, y] = dead_zoned(self.sticks[stick as usize]);
        let [up, down, left, right] = stick.directions();
        for (button, strength) in [(up, -y), (down, y), (left, -x), (right, x)] {
            self.strengths[button as usize] = strength.max(0.0);
            self.update_held(button);
        }
    }

    /// Holds or releases `button` by how far it's pushed.
    fn update_held(&mut self, button: GamepadButton) {
        let strength = self.strengths[button as usize];
        if strength >= PRESS {
            self.buttons.down(button);
        } else if strength < RELEASE {
            self.buttons.up(button);
        }
    }

    fn release_all(&mut self) {
        self.buttons.release_all();
        self.strengths = Default::default();
        self.sticks = Default::default();
    }
}

/// Where a stick at `position` counts as: at rest inside the dead zone around its middle, pushed
/// all the way near its edge, and in between, as far as it is between the two, in the direction
/// it's pushed.
fn dead_zoned([x, y]: [f32; 2]) -> [f32; 2] {
    let distance = (x * x + y * y).sqrt();
    if distance <= STICK_DEAD_ZONE {
        return [0.0, 0.0];
    }
    let pushed = ((distance - STICK_DEAD_ZONE) / (STICK_FULL - STICK_DEAD_ZONE)).min(1.0);
    let scale = pushed / distance;
    [x * scale, y * scale]
}

/// Held, pressed, released and repeated state for a set of keys or buttons. Only keys repeat.
#[derive(Debug)]
struct Buttons<T> {
    held: HashSet<T>,
    pressed: HashSet<T>,
    released: HashSet<T>,
    repeated: HashSet<T>,
}

impl<T> Default for Buttons<T> {
    fn default() -> Self {
        Self {
            held: HashSet::new(),
            pressed: HashSet::new(),
            released: HashSet::new(),
            repeated: HashSet::new(),
        }
    }
}

impl<T: Copy + Eq + Hash> Buttons<T> {
    fn down(&mut self, button: T) {
        if self.held.insert(button) {
            self.pressed.insert(button);
        }
    }

    fn repeat(&mut self, button: T) {
        if self.held.contains(&button) {
            self.repeated.insert(button);
        } else {
            self.down(button);
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
        self.repeated.clear();
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

/// A gamepad button, or a direction of one of its sticks, named for where it is on the gamepad,
/// in the layout of an Xbox controller, whatever the gamepad.
///
/// The four face buttons are named for the compass: [`South`](Self::South) is the bottom one (A
/// on an Xbox controller, Cross on a PlayStation's, B on a Nintendo's), [`East`](Self::East) the
/// right, [`West`](Self::West) the left and [`North`](Self::North) the top.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GamepadButton {
    South,
    East,
    West,
    North,
    /// The bumper above the left trigger (LB, L1).
    LeftShoulder,
    /// The bumper above the right trigger (RB, R1).
    RightShoulder,
    /// The left trigger (LT, L2), which can be pushed part of the way.
    LeftTrigger,
    /// The right trigger (RT, R2), which can be pushed part of the way.
    RightTrigger,
    /// The button left of the middle (View, Share, Minus).
    Select,
    /// The button right of the middle (Menu, Options, Plus).
    Start,
    /// The button in the middle, such as the Xbox or PS button, which the system often takes for
    /// itself.
    Guide,
    /// Pressing the left stick in.
    LeftThumb,
    /// Pressing the right stick in.
    RightThumb,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    LeftStickUp,
    LeftStickDown,
    LeftStickLeft,
    LeftStickRight,
    RightStickUp,
    RightStickDown,
    RightStickLeft,
    RightStickRight,
}

impl GamepadButton {
    /// Every button, in order, with its name.
    const ALL: [(&'static str, Self); 25] = [
        ("South", Self::South),
        ("East", Self::East),
        ("West", Self::West),
        ("North", Self::North),
        ("LeftShoulder", Self::LeftShoulder),
        ("RightShoulder", Self::RightShoulder),
        ("LeftTrigger", Self::LeftTrigger),
        ("RightTrigger", Self::RightTrigger),
        ("Select", Self::Select),
        ("Start", Self::Start),
        ("Guide", Self::Guide),
        ("LeftThumb", Self::LeftThumb),
        ("RightThumb", Self::RightThumb),
        ("DPadUp", Self::DPadUp),
        ("DPadDown", Self::DPadDown),
        ("DPadLeft", Self::DPadLeft),
        ("DPadRight", Self::DPadRight),
        ("LeftStickUp", Self::LeftStickUp),
        ("LeftStickDown", Self::LeftStickDown),
        ("LeftStickLeft", Self::LeftStickLeft),
        ("LeftStickRight", Self::LeftStickRight),
        ("RightStickUp", Self::RightStickUp),
        ("RightStickDown", Self::RightStickDown),
        ("RightStickLeft", Self::RightStickLeft),
        ("RightStickRight", Self::RightStickRight),
    ];

    /// The stick and axis this is a direction along, and which way along it (`-1` or `1`), if
    /// it's a stick's direction.
    fn stick_direction(self) -> Option<(Stick, Axis, f32)> {
        Some(match self {
            Self::LeftStickUp => (Stick::Left, Axis::Y, -1.0),
            Self::LeftStickDown => (Stick::Left, Axis::Y, 1.0),
            Self::LeftStickLeft => (Stick::Left, Axis::X, -1.0),
            Self::LeftStickRight => (Stick::Left, Axis::X, 1.0),
            Self::RightStickUp => (Stick::Right, Axis::Y, -1.0),
            Self::RightStickDown => (Stick::Right, Axis::Y, 1.0),
            Self::RightStickLeft => (Stick::Right, Axis::X, -1.0),
            Self::RightStickRight => (Stick::Right, Axis::X, 1.0),
            _ => return None,
        })
    }
}

/// Parses a button's name, as written in [`GamepadButton`] (such as `"South"` or
/// `"LeftStickUp"`), ignoring case.
impl FromStr for GamepadButton {
    type Err = ParseInputError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|(candidate, _)| name.eq_ignore_ascii_case(candidate))
            .map(|(_, button)| button)
            .ok_or_else(|| ParseInputError(format!("unknown gamepad button `{name}`")))
    }
}

/// One of a gamepad's two sticks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Stick {
    Left,
    Right,
}

impl Stick {
    /// Its directions: up, down, left and right.
    fn directions(self) -> [GamepadButton; 4] {
        use GamepadButton as B;
        match self {
            Self::Left => [
                B::LeftStickUp,
                B::LeftStickDown,
                B::LeftStickLeft,
                B::LeftStickRight,
            ],
            Self::Right => [
                B::RightStickUp,
                B::RightStickDown,
                B::RightStickLeft,
                B::RightStickRight,
            ],
        }
    }
}

/// Parses `"left"` or `"right"`, ignoring case.
impl FromStr for Stick {
    type Err = ParseInputError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        [("Left", Self::Left), ("Right", Self::Right)]
            .into_iter()
            .find(|(candidate, _)| name.eq_ignore_ascii_case(candidate))
            .map(|(_, stick)| stick)
            .ok_or_else(|| ParseInputError(format!("unknown stick `{name}`")))
    }
}

/// An axis of a stick: `X` grows to the right and `Y` downward, as canvas pixels do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Axis {
    X,
    Y,
}

/// What an action can be bound to: a key, or a gamepad's button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Key(Key),
    Button(GamepadButton),
}

impl From<Key> for Control {
    fn from(key: Key) -> Self {
        Self::Key(key)
    }
}

impl From<GamepadButton> for Control {
    fn from(button: GamepadButton) -> Self {
        Self::Button(button)
    }
}

/// Parses a key's name or a gamepad button's, ignoring case. No name is both.
impl FromStr for Control {
    type Err = ParseInputError;

    fn from_str(name: &str) -> Result<Self, Self::Err> {
        name.parse()
            .map(Self::Key)
            .or_else(|_| name.parse().map(Self::Button))
            .map_err(|_| ParseInputError(format!("unknown key or gamepad button `{name}`")))
    }
}

/// An unrecognized key or button name.
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
            /// Every key's name.
            #[cfg(test)]
            const NAMES: &'static [&'static str] = &[$(stringify!($key),)*];

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
    fn key_repeat_repeats_without_pressing_again() {
        let mut input = Input::new();
        input.bind("step", [Key::L]);
        input.key_down(Key::L);
        assert!(input.key_pressed(Key::L) && !input.key_repeated(Key::L));
        assert!(input.pressed("step") && !input.repeated("step"));

        input.end_frame();
        input.key_repeat(Key::L);
        assert!(!input.key_pressed(Key::L) && input.key_held(Key::L));
        assert!(input.key_repeated(Key::L) && input.repeated("step"));
        assert!(!input.pressed("step"));

        input.end_frame();
        assert!(!input.key_repeated(Key::L), "a repeat lasts one frame");
        // A second press of a held key is no press at all.
        input.key_down(Key::L);
        assert!(!input.key_pressed(Key::L) && !input.key_repeated(Key::L));
    }

    #[test]
    fn repeat_of_a_key_not_held_presses_it() {
        let mut input = Input::new();
        input.key_repeat(Key::W);
        assert!(input.key_pressed(Key::W) && input.key_held(Key::W));
        assert!(!input.key_repeated(Key::W));
    }

    #[test]
    fn gamepad_buttons_never_repeat() {
        let mut input = Input::new();
        input.connect_gamepad(1, "Pad".into());
        input.bind("left", [GamepadButton::DPadLeft]);
        input.set_gamepad_button(1, GamepadButton::DPadLeft, 1.0);
        input.end_frame();
        input.set_gamepad_button(1, GamepadButton::DPadLeft, 1.0);
        assert!(input.held("left") && !input.repeated("left"));
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
        input.connect_gamepad(1, "Pad".into());
        input.key_down(Key::LeftAlt);
        input.key_down(Key::Tab);
        input.mouse_down(MouseButton::Left);
        input.set_gamepad_button(1, GamepadButton::South, 1.0);
        input.set_gamepad_axis(1, Stick::Left, Axis::X, 1.0);
        input.end_frame();
        input.release_all();
        assert!(input.key_released(Key::LeftAlt) && input.key_released(Key::Tab));
        assert!(!input.key_held(Key::LeftAlt) && !input.key_held(Key::Tab));
        assert!(input.mouse_released(MouseButton::Left));
        assert!(!input.mouse_held(MouseButton::Left));
        let gamepad = input.gamepad(1).unwrap();
        assert!(gamepad.released(GamepadButton::South) && !gamepad.held(GamepadButton::South));
        assert!(gamepad.released(GamepadButton::LeftStickRight));
        assert_eq!(gamepad.strength(GamepadButton::LeftStickRight), 0.0);
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
    fn gamepads_number_from_one_and_keep_their_number() {
        let mut input = Input::new();
        assert!(input.gamepad(0).is_none() && input.gamepad(1).is_none());
        input.connect_gamepad(1, "First".into());
        input.connect_gamepad(2, "Second".into());
        input.set_gamepad_button(1, GamepadButton::East, 1.0);
        input.end_frame();

        input.disconnect_gamepad(1);
        let first = input.gamepad(1).unwrap();
        assert!(!first.is_connected() && first.released(GamepadButton::East));
        let connected: Vec<u32> = input.gamepads().map(|(number, _)| number).collect();
        assert_eq!(connected, [2]);

        input.connect_gamepad(1, "First".into());
        assert_eq!(input.gamepad(1).unwrap().name(), "First");
        assert_eq!(input.gamepads().count(), 2);
    }

    #[test]
    fn actions_follow_bound_buttons_on_any_gamepad_or_one() {
        let mut input = Input::new();
        input.connect_gamepad(1, "One".into());
        input.connect_gamepad(2, "Two".into());
        input.bind(
            "jump",
            [Control::Key(Key::Space), GamepadButton::South.into()],
        );
        input.bind_gamepad("p2_jump", 2, [GamepadButton::South]);

        input.set_gamepad_button(1, GamepadButton::South, 1.0);
        assert!(input.pressed("jump") && input.held("jump"));
        assert!(!input.held("p2_jump"), "only gamepad 2's buttons count");

        input.set_gamepad_button(2, GamepadButton::South, 1.0);
        assert!(input.pressed("p2_jump"));
        input.end_frame();
        input.set_gamepad_button(2, GamepadButton::South, 0.0);
        assert!(input.released("p2_jump") && !input.held("p2_jump"));
        assert!(input.held("jump"), "gamepad 1 still holds it");
    }

    #[test]
    fn analog_buttons_hold_past_halfway_and_release_below_less() {
        let mut input = Input::new();
        input.connect_gamepad(1, "Pad".into());
        input.bind("shield", [GamepadButton::RightTrigger]);
        input.set_gamepad_button(1, GamepadButton::RightTrigger, 0.3);
        assert!(!input.held("shield"));
        assert_eq!(input.strength("shield"), 0.3);
        input.set_gamepad_button(1, GamepadButton::RightTrigger, 0.5);
        assert!(input.pressed("shield"));
        input.end_frame();
        input.set_gamepad_button(1, GamepadButton::RightTrigger, 0.45);
        assert!(
            input.held("shield"),
            "it stays held until it falls below 0.4"
        );
        input.set_gamepad_button(1, GamepadButton::RightTrigger, 0.39);
        assert!(input.released("shield") && !input.held("shield"));
    }

    #[test]
    fn sticks_count_from_their_dead_zone_to_near_their_edge() {
        let mut input = Input::new();
        input.connect_gamepad(1, "Pad".into());
        let strength = |input: &Input, button| input.gamepad(1).unwrap().strength(button);

        input.set_gamepad_axis(1, Stick::Left, Axis::X, 0.1);
        assert_eq!(strength(&input, GamepadButton::LeftStickRight), 0.0);
        input.set_gamepad_axis(1, Stick::Left, Axis::X, 0.55);
        assert!((strength(&input, GamepadButton::LeftStickRight) - 0.5).abs() < 1e-6);
        input.set_gamepad_axis(1, Stick::Left, Axis::X, 0.97);
        assert_eq!(strength(&input, GamepadButton::LeftStickRight), 1.0);
        assert_eq!(strength(&input, GamepadButton::LeftStickLeft), 0.0);

        // Up and to the left, as far as it goes: both directions are held, a little over 0.7.
        input.set_gamepad_axis(1, Stick::Left, Axis::X, -0.7);
        input.set_gamepad_axis(1, Stick::Left, Axis::Y, -0.7);
        let gamepad = input.gamepad(1).unwrap();
        assert!(gamepad.held(GamepadButton::LeftStickUp));
        assert!(gamepad.held(GamepadButton::LeftStickLeft));
        assert!(!gamepad.held(GamepadButton::LeftStickRight));
        let up = gamepad.strength(GamepadButton::LeftStickUp);
        assert!((up - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6, "{up}");
    }

    #[test]
    fn axes_combine_two_actions() {
        let mut input = Input::new();
        input.connect_gamepad(1, "Pad".into());
        input.bind(
            "left",
            [Control::Key(Key::A), GamepadButton::LeftStickLeft.into()],
        );
        input.bind(
            "right",
            [Control::Key(Key::D), GamepadButton::LeftStickRight.into()],
        );
        assert_eq!(input.axis("left", "right"), 0.0);
        input.key_down(Key::A);
        assert_eq!(input.axis("left", "right"), -1.0);
        input.key_up(Key::A);
        input.set_gamepad_axis(1, Stick::Left, Axis::X, 0.55);
        assert!((input.axis("left", "right") - 0.5).abs() < 1e-6);
    }

    #[test]
    fn stick_directions_push_the_stick() {
        let mut input = Input::new();
        input.connect_gamepad(1, "Pad".into());
        input.set_gamepad_button(1, GamepadButton::RightStickDown, 1.0);
        let gamepad = input.gamepad(1).unwrap();
        assert!(gamepad.pressed(GamepadButton::RightStickDown));
        assert_eq!(gamepad.strength(GamepadButton::RightStickDown), 1.0);

        // Letting go of the other way leaves it pushed down.
        input.set_gamepad_button(1, GamepadButton::RightStickUp, 0.0);
        assert!(
            input
                .gamepad(1)
                .unwrap()
                .held(GamepadButton::RightStickDown)
        );
        input.set_gamepad_button(1, GamepadButton::RightStickDown, 0.0);
        assert!(
            !input
                .gamepad(1)
                .unwrap()
                .held(GamepadButton::RightStickDown)
        );
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
        assert_eq!("south".parse(), Ok(GamepadButton::South));
        assert_eq!("LEFTSTICKUP".parse(), Ok(GamepadButton::LeftStickUp));
        assert_eq!("Space".parse(), Ok(Control::Key(Key::Space)));
        assert_eq!(
            "dpadleft".parse(),
            Ok(Control::Button(GamepadButton::DPadLeft))
        );
        assert_eq!(
            "A".parse::<Control>(),
            Ok(Control::Key(Key::A)),
            "A is a key, not the Xbox button"
        );
        assert_eq!(
            "Shift".parse::<Control>().unwrap_err().to_string(),
            "unknown key or gamepad button `Shift`"
        );
        assert_eq!("Right".parse(), Ok(Stick::Right));
    }

    #[test]
    fn no_name_is_both_a_key_and_a_gamepad_button() {
        for name in Key::NAMES {
            assert!(name.parse::<GamepadButton>().is_err(), "{name}");
        }
        for (index, (name, button)) in GamepadButton::ALL.into_iter().enumerate() {
            assert!(name.parse::<Key>().is_err(), "{name}");
            assert_eq!(button as usize, index, "`ALL` is in order");
        }
    }
}
