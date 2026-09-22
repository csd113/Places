use sdl2::event::Event;
use sdl2::keyboard::Keycode;

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
/// lookup, and the whole state is copied every frame. `toggle_overlay` and
/// `quit_requested` stay named fields because the game flips them itself
/// instead of holding a key.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct InputState {
    held: u16,
    pub toggle_overlay: bool,
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

/// Converts an SDL Keycode into a normalized string representation.
#[must_use]
pub fn keycode_to_str(key: Keycode) -> String {
    match key {
        Keycode::Period => ".".to_string(),
        Keycode::Minus => "-".to_string(),
        Keycode::Equals => "=".to_string(),
        Keycode::Comma => ",".to_string(),
        Keycode::Slash => "/".to_string(),
        Keycode::Backslash => "\\".to_string(),
        Keycode::Semicolon => ";".to_string(),
        Keycode::Return => "ENTER".to_string(),
        Keycode::Escape => "ESC".to_string(),
        Keycode::Space => "SPACE".to_string(),
        Keycode::Tab => "TAB".to_string(),
        Keycode::Backspace => "BACKSPACE".to_string(),
        Keycode::Up => "UP".to_string(),
        Keycode::Down => "DOWN".to_string(),
        Keycode::Left => "LEFT".to_string(),
        Keycode::Right => "RIGHT".to_string(),
        other => other.name().to_uppercase(),
    }
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

    pub const fn set_overlay_visible(&mut self, visible: bool) {
        self.state.toggle_overlay = visible;
    }

    /// Handles gameplay events using active `KeyBindings`.
    pub fn handle_gameplay_event(&mut self, event: &Event, bindings: &KeyBindings) {
        match event {
            Event::Quit { .. } => {
                self.state.quit_requested = true;
            }
            Event::KeyDown {
                keycode: Some(key),
                repeat: false,
                ..
            } => {
                let name = keycode_to_str(*key);
                if let Some(button) = InputState::binding_control(bindings, &name) {
                    self.state.set_held(button, true);
                }
                if *key == Keycode::Minus || *key == Keycode::KpMinus {
                    self.state.toggle_overlay = !self.state.toggle_overlay;
                }
            }
            Event::KeyUp {
                keycode: Some(key), ..
            } => {
                let name = keycode_to_str(*key);
                if let Some(button) = InputState::binding_control(bindings, &name) {
                    self.state.set_held(button, false);
                }
            }
            _ => {}
        }
    }

    /// Extracts menu navigation events independent of gameplay bindings.
    /// W / S or Up / Down for item selection; A / D or Left / Right for
    /// adjusting; Enter for activation; Escape for back.
    #[must_use]
    pub const fn poll_menu_nav_event(event: &Event) -> Option<MenuNavEvent> {
        match event {
            Event::KeyDown {
                keycode: Some(key),
                repeat: false,
                ..
            } => match *key {
                Keycode::W | Keycode::Up => Some(MenuNavEvent::Up),
                Keycode::S | Keycode::Down => Some(MenuNavEvent::Down),
                Keycode::A | Keycode::Left => Some(MenuNavEvent::Left),
                Keycode::D | Keycode::Right => Some(MenuNavEvent::Right),
                Keycode::Return => Some(MenuNavEvent::Activate),
                Keycode::Escape => Some(MenuNavEvent::Back),
                _ => None,
            },
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
