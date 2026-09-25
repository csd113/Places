use sdl3::event::Event;
use sdl3::keyboard::Keycode;

use crate::settings::KeyBindings;

/// One gameplay control the player can hold.
///
/// The held controls live as bits in [`InputState`]; this enum is the named
/// handle used by the event handler, the player simulation and the tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// Walk towards the way the camera faces.
    MoveForward,
    /// Walk away from the way the camera faces.
    MoveBackward,
    /// Step left of the way the camera faces.
    StrafeLeft,
    /// Step right of the way the camera faces.
    StrafeRight,
    /// Pitch the camera up.
    LookUp,
    /// Pitch the camera down.
    LookDown,
    /// Yaw the camera left.
    LookLeft,
    /// Yaw the camera right.
    LookRight,
}

impl Control {
    /// Bit this control occupies in [`InputState`].
    const fn bit(self) -> u16 {
        match self {
            Self::MoveForward => 1 << 0,
            Self::MoveBackward => 1 << 1,
            Self::StrafeLeft => 1 << 2,
            Self::StrafeRight => 1 << 3,
            Self::LookUp => 1 << 4,
            Self::LookDown => 1 << 5,
            Self::LookLeft => 1 << 6,
            Self::LookRight => 1 << 7,
        }
    }
}

/// Raw input state representing gameplay movement and camera looking.
///
/// The eight movement and look controls are independent bits rather than eight
/// separate `bool` fields: they are all set and cleared by the same binding
/// lookup, and the whole state is copied every frame. `quit_requested` stays a
/// named field because the game flips it itself instead of holding a key.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct InputState {
    held: u16,
    pub quit_requested: bool,
}

impl InputState {
    /// The control the binding called `name` holds, or `None` when `name` is
    /// not one of the movement or look bindings.
    fn binding_control(bindings: &KeyBindings, name: &str) -> Option<Control> {
        if name == bindings.forward {
            Some(Control::MoveForward)
        } else if name == bindings.backward {
            Some(Control::MoveBackward)
        } else if name == bindings.strafe_left {
            Some(Control::StrafeLeft)
        } else if name == bindings.strafe_right {
            Some(Control::StrafeRight)
        } else if name == bindings.look_up {
            Some(Control::LookUp)
        } else if name == bindings.look_down {
            Some(Control::LookDown)
        } else if name == bindings.look_left {
            Some(Control::LookLeft)
        } else if name == bindings.look_right {
            Some(Control::LookRight)
        } else {
            None
        }
    }

    /// Presses or releases one held control.
    const fn set_held(&mut self, control: Control, pressed: bool) {
        if pressed {
            self.held |= control.bit();
        } else {
            self.held &= !control.bit();
        }
    }

    /// Releases every held control, leaving the overlay and quit flags alone.
    const fn release_all(&mut self) {
        self.held = 0;
    }

    /// True while `control` is held.
    #[must_use]
    pub const fn is_held(self, control: Control) -> bool {
        self.held & control.bit() != 0
    }

    /// State with every listed control held, for tests that drive the player
    /// without an SDL event queue.
    #[cfg(test)]
    pub(crate) fn holding(controls: &[Control]) -> Self {
        let mut state = Self::default();
        for control in controls {
            state.set_held(*control, true);
        }
        state
    }
}

/// Menu navigation events independent of user-rebindable gameplay controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuNavEvent {
    Up,
    Down,
    Left,
    Right,
    Activate,
    Back,
}

/// Keys whose binding name is not SDL's own key name.
///
/// Every other key falls back to `Keycode::name()` uppercased; that fallback is
/// the identity written to `settings.json` and the one `is_reserved_key`
/// compares against. The table is explicit (rather than a wildcard match over
/// SDL3's extensible `Keycode` enum) so a future SDL3 key constant can never be
/// mistaken for one of these spellings.
const KEY_NAME_OVERRIDES: [(Keycode, &str); 16] = [
    (Keycode::Period, "."),
    (Keycode::Minus, "-"),
    (Keycode::Equals, "="),
    (Keycode::Comma, ","),
    (Keycode::Slash, "/"),
    (Keycode::Backslash, "\\"),
    (Keycode::Semicolon, ";"),
    (Keycode::Return, "ENTER"),
    (Keycode::Escape, "ESC"),
    (Keycode::Space, "SPACE"),
    (Keycode::Tab, "TAB"),
    (Keycode::Backspace, "BACKSPACE"),
    (Keycode::Up, "UP"),
    (Keycode::Down, "DOWN"),
    (Keycode::Left, "LEFT"),
    (Keycode::Right, "RIGHT"),
];

/// Converts an SDL Keycode into a normalized string representation.
#[must_use]
pub fn keycode_to_str(key: Keycode) -> String {
    KEY_NAME_OVERRIDES
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map_or_else(
            || key.name().to_uppercase(),
            |(_, name)| (*name).to_string(),
        )
}

/// Manages active input states, menu navigation, and rebinding event capture.
pub struct InputHandler {
    state: InputState,
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new()
    }
}

/// The keys that navigate menus, independent of gameplay bindings.
///
/// W / S or Up / Down select; A / D or Left / Right adjust; Enter activates;
/// Escape goes back. The table is explicit rather than a wildcard match over
/// SDL3's extensible `Keycode` enum, so a new key constant never silently
/// gains (or loses) a menu meaning.
const MENU_NAV_KEYS: [(Keycode, MenuNavEvent); 10] = [
    (Keycode::W, MenuNavEvent::Up),
    (Keycode::Up, MenuNavEvent::Up),
    (Keycode::S, MenuNavEvent::Down),
    (Keycode::Down, MenuNavEvent::Down),
    (Keycode::A, MenuNavEvent::Left),
    (Keycode::Left, MenuNavEvent::Left),
    (Keycode::D, MenuNavEvent::Right),
    (Keycode::Right, MenuNavEvent::Right),
    (Keycode::Return, MenuNavEvent::Activate),
    (Keycode::Escape, MenuNavEvent::Back),
];

impl InputHandler {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: InputState::default(),
        }
    }

    #[must_use]
    pub const fn state(&self) -> &InputState {
        &self.state
    }

    #[must_use]
    pub const fn quit_requested(&self) -> bool {
        self.state.quit_requested
    }

    pub const fn clear_gameplay_inputs(&mut self) {
        self.state.release_all();
    }

    /// Handles gameplay events using active `KeyBindings`.
    pub fn handle_gameplay_event(&mut self, event: &Event, bindings: &KeyBindings) {
        if let Event::Quit { .. } = event {
            self.state.quit_requested = true;
            return;
        }
        if let Event::KeyDown {
            keycode: Some(key),
            repeat: false,
            ..
        } = event
        {
            let name = keycode_to_str(*key);
            if let Some(button) = InputState::binding_control(bindings, &name) {
                self.state.set_held(button, true);
            }
            return;
        }
        if let Event::KeyUp {
            keycode: Some(key), ..
        } = event
        {
            let name = keycode_to_str(*key);
            if let Some(button) = InputState::binding_control(bindings, &name) {
                self.state.set_held(button, false);
            }
        }
    }

    /// Extracts menu navigation events independent of gameplay bindings.
    ///
    /// Only a first `KeyDown` (`repeat: false`) navigates; releases and OS
    /// auto-repeat never do.
    #[must_use]
    pub fn poll_menu_nav_event(event: &Event) -> Option<MenuNavEvent> {
        if let Event::KeyDown {
            keycode: Some(key),
            repeat: false,
            ..
        } = event
        {
            return MENU_NAV_KEYS
                .iter()
                .find(|(candidate, _)| candidate == key)
                .map(|(_, nav)| *nav);
        }
        None
    }
}

#[cfg(test)]
mod tests;
