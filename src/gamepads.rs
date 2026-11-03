//! Gamepads, read through gilrs for a window's [`Input`].

use gilrs::{
    Event, EventType, Filter, Gilrs, GilrsBuilder,
    ev::filter::{Jitter, axis_dpad_to_button},
};

use crate::input::{Axis, GamepadButton, Input, Stick};

/// The gamepads a window reads, which tell its [`Input`] what happens to them.
pub(crate) struct Gamepads {
    gilrs: Gilrs,
    /// Drops a stick's movements too small to matter, which some report all the time.
    jitter: Jitter,
}

impl Gamepads {
    /// Starts reading gamepads, connecting those already connected in `input`. Without gamepads
    /// on this system, it logs why and returns `None`, and the game plays without them.
    pub(crate) fn new(input: &mut Input) -> Option<Self> {
        // The engine applies its own dead zone, the same on every platform, rather than gilrs's,
        // which only some platforms' drivers give. Its other filters are applied as events come.
        let gilrs = GilrsBuilder::new()
            .with_default_filters(false)
            .set_update_state(false)
            .with_force_feedback(false)
            .build();
        let gilrs = match gilrs {
            Ok(gilrs) => gilrs,
            Err(gilrs::Error::NotImplemented(_)) => {
                log::info!("gamepads aren't supported on this platform");
                return None;
            }
            Err(err) => {
                log::warn!("can't read gamepads: {err}");
                return None;
            }
        };
        for (id, gamepad) in gilrs.gamepads() {
            input.connect_gamepad(number(id), gamepad.name().to_owned());
        }
        Some(Self {
            gilrs,
            jitter: Jitter::new(),
        })
    }

    /// Tells `input` what's happened to the gamepads since the last call. Unless the window is
    /// `focused`, it hears only of gamepads connecting and disconnecting, as it hears nothing of
    /// the keyboard then.
    pub(crate) fn poll(&mut self, input: &mut Input, focused: bool) {
        while let Some(event) = self.gilrs.next_event() {
            let Some(event) = Some(event)
                .filter_ev(&axis_dpad_to_button, &mut self.gilrs)
                .filter_ev(&self.jitter, &mut self.gilrs)
            else {
                continue;
            };
            if event.is_dropped() {
                continue;
            }
            self.gilrs.update(&event);
            self.handle(&event, input, focused);
        }
    }

    fn handle(&self, event: &Event, input: &mut Input, focused: bool) {
        let number = number(event.id);
        match event.event {
            EventType::Connected => {
                let name = self.gilrs.gamepad(event.id).name().to_owned();
                input.connect_gamepad(number, name);
            }
            EventType::Disconnected => input.disconnect_gamepad(number),
            _ if !focused => {}
            // Every press and release comes with a change in how far the button is pushed,
            // which is all the engine needs.
            EventType::ButtonChanged(button, value, _) => {
                if let Some(button) = button_from_gilrs(button) {
                    input.set_gamepad_button(number, button, value);
                }
            }
            EventType::AxisChanged(axis, value, _) => {
                // gilrs's `y` grows upward, and the engine's downward.
                let (stick, axis, value) = match axis {
                    gilrs::Axis::LeftStickX => (Stick::Left, Axis::X, value),
                    gilrs::Axis::LeftStickY => (Stick::Left, Axis::Y, -value),
                    gilrs::Axis::RightStickX => (Stick::Right, Axis::X, value),
                    gilrs::Axis::RightStickY => (Stick::Right, Axis::Y, -value),
                    _ => return,
                };
                input.set_gamepad_axis(number, stick, axis, value);
            }
            _ => {}
        }
    }
}

/// The engine's number for a gamepad gilrs knows by `id`: gilrs numbers them from 0, in the order
/// they connect, and gives a gamepad that connects again its old number.
fn number(id: gilrs::GamepadId) -> u32 {
    u32::try_from(usize::from(id)).map_or(u32::MAX, |id| id.saturating_add(1))
}

fn button_from_gilrs(button: gilrs::Button) -> Option<GamepadButton> {
    use gilrs::Button as B;
    Some(match button {
        B::South => GamepadButton::South,
        B::East => GamepadButton::East,
        B::West => GamepadButton::West,
        B::North => GamepadButton::North,
        // gilrs calls the bumpers triggers, and the triggers its second ones.
        B::LeftTrigger => GamepadButton::LeftShoulder,
        B::RightTrigger => GamepadButton::RightShoulder,
        B::LeftTrigger2 => GamepadButton::LeftTrigger,
        B::RightTrigger2 => GamepadButton::RightTrigger,
        B::Select => GamepadButton::Select,
        B::Start => GamepadButton::Start,
        B::Mode => GamepadButton::Guide,
        B::LeftThumb => GamepadButton::LeftThumb,
        B::RightThumb => GamepadButton::RightThumb,
        B::DPadUp => GamepadButton::DPadUp,
        B::DPadDown => GamepadButton::DPadDown,
        B::DPadLeft => GamepadButton::DPadLeft,
        B::DPadRight => GamepadButton::DPadRight,
        B::C | B::Z | B::Unknown => return None,
    })
}
