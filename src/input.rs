use sdl2::event::Event;
use sdl2::keyboard::Keycode;

use crate::settings::KeyBindings;

/// Raw input state representing gameplay movement and camera looking.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct InputState {
    pub move_forward: bool,
    pub move_backward: bool,
    pub strafe_left: bool,
    pub strafe_right: bool,
    pub look_up: bool,
    pub look_down: bool,
    pub look_left: bool,
    pub look_right: bool,
    pub toggle_overlay: bool,
    pub quit_requested: bool,
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
    pub fn new() -> Self {
        Self {
            state: InputState::default(),
        }
    }

    pub fn state(&self) -> &InputState {
        &self.state
    }

    pub fn quit_requested(&self) -> bool {
        self.state.quit_requested
    }

    pub fn clear_gameplay_inputs(&mut self) {
        self.state = InputState {
            quit_requested: self.state.quit_requested,
            toggle_overlay: self.state.toggle_overlay,
            ..Default::default()
        };
    }

    pub fn set_overlay_visible(&mut self, visible: bool) {
        self.state.toggle_overlay = visible;
    }

    /// Handles gameplay events using active KeyBindings.
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
                if name == bindings.forward {
                    self.state.move_forward = true;
                }
                if name == bindings.backward {
                    self.state.move_backward = true;
                }
                if name == bindings.strafe_left {
                    self.state.strafe_left = true;
                }
                if name == bindings.strafe_right {
                    self.state.strafe_right = true;
                }
                if name == bindings.look_up {
                    self.state.look_up = true;
                }
                if name == bindings.look_down {
                    self.state.look_down = true;
                }
                if name == bindings.look_left {
                    self.state.look_left = true;
                }
                if name == bindings.look_right {
                    self.state.look_right = true;
                }
                if *key == Keycode::Minus || *key == Keycode::KpMinus {
                    self.state.toggle_overlay = !self.state.toggle_overlay;
                }
            }
            Event::KeyUp {
                keycode: Some(key), ..
            } => {
                let name = keycode_to_str(*key);
                if name == bindings.forward {
                    self.state.move_forward = false;
                }
                if name == bindings.backward {
                    self.state.move_backward = false;
                }
                if name == bindings.strafe_left {
                    self.state.strafe_left = false;
                }
                if name == bindings.strafe_right {
                    self.state.strafe_right = false;
                }
                if name == bindings.look_up {
                    self.state.look_up = false;
                }
                if name == bindings.look_down {
                    self.state.look_down = false;
                }
                if name == bindings.look_left {
                    self.state.look_left = false;
                }
                if name == bindings.look_right {
                    self.state.look_right = false;
                }
            }
            _ => {}
        }
    }

    /// Extracts menu navigation events independent of gameplay bindings.
    /// W / Z or Up / Down for item selection; Enter for activation; Escape for back.
    pub fn poll_menu_nav_event(event: &Event) -> Option<MenuNavEvent> {
        match event {
            Event::KeyDown {
                keycode: Some(key),
                repeat: false,
                ..
            } => match *key {
                Keycode::W | Keycode::Up => Some(MenuNavEvent::Up),
                Keycode::Z | Keycode::Down => Some(MenuNavEvent::Down),
                Keycode::A | Keycode::Left => Some(MenuNavEvent::Left),
                Keycode::S | Keycode::Right => Some(MenuNavEvent::Right),
                Keycode::Return => Some(MenuNavEvent::Activate),
                Keycode::Escape => Some(MenuNavEvent::Back),
                _ => None,
            },
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simultaneous_movement_and_looking() {
        let mut handler = InputHandler::new();
        let bindings = KeyBindings::default();

        // Press W (forward) and K (look left) and O (look up) simultaneously
        handler.handle_gameplay_event(
            &Event::KeyDown {
                timestamp: 0,
                window_id: 0,
                keycode: Some(Keycode::W),
                scancode: None,
                keymod: sdl2::keyboard::Mod::NOMOD,
                repeat: false,
            },
            &bindings,
        );
        handler.handle_gameplay_event(
            &Event::KeyDown {
                timestamp: 0,
                window_id: 0,
                keycode: Some(Keycode::K),
                scancode: None,
                keymod: sdl2::keyboard::Mod::NOMOD,
                repeat: false,
            },
            &bindings,
        );
        handler.handle_gameplay_event(
            &Event::KeyDown {
                timestamp: 0,
                window_id: 0,
                keycode: Some(Keycode::O),
                scancode: None,
                keymod: sdl2::keyboard::Mod::NOMOD,
                repeat: false,
            },
            &bindings,
        );

        assert!(handler.state().move_forward);
        assert!(handler.state().look_left);
        assert!(handler.state().look_up);
        assert!(!handler.state().move_backward);

        // Releasing W should not stop looking
        handler.handle_gameplay_event(
            &Event::KeyUp {
                timestamp: 0,
                window_id: 0,
                keycode: Some(Keycode::W),
                scancode: None,
                keymod: sdl2::keyboard::Mod::NOMOD,
                repeat: false,
            },
            &bindings,
        );
        assert!(!handler.state().move_forward);
        assert!(handler.state().look_left);
        assert!(handler.state().look_up);
    }

    #[test]
    fn test_menu_nav_independent_of_bindings() {
        // Up event
        let up_event = Event::KeyDown {
            timestamp: 0,
            window_id: 0,
            keycode: Some(Keycode::W),
            scancode: None,
            keymod: sdl2::keyboard::Mod::NOMOD,
            repeat: false,
        };
        assert_eq!(
            InputHandler::poll_menu_nav_event(&up_event),
            Some(MenuNavEvent::Up)
        );

        // Down event
        let down_event = Event::KeyDown {
            timestamp: 0,
            window_id: 0,
            keycode: Some(Keycode::Z),
            scancode: None,
            keymod: sdl2::keyboard::Mod::NOMOD,
            repeat: false,
        };
        assert_eq!(
            InputHandler::poll_menu_nav_event(&down_event),
            Some(MenuNavEvent::Down)
        );

        // Activate event
        let enter_event = Event::KeyDown {
            timestamp: 0,
            window_id: 0,
            keycode: Some(Keycode::Return),
            scancode: None,
            keymod: sdl2::keyboard::Mod::NOMOD,
            repeat: false,
        };
        assert_eq!(
            InputHandler::poll_menu_nav_event(&enter_event),
            Some(MenuNavEvent::Activate)
        );

        // Escape event
        let esc_event = Event::KeyDown {
            timestamp: 0,
            window_id: 0,
            keycode: Some(Keycode::Escape),
            scancode: None,
            keymod: sdl2::keyboard::Mod::NOMOD,
            repeat: false,
        };
        assert_eq!(
            InputHandler::poll_menu_nav_event(&esc_event),
            Some(MenuNavEvent::Back)
        );
    }

    #[test]
    fn test_toggle_overlay_event() {
        let mut handler = InputHandler::new();
        let bindings = KeyBindings::default();

        assert!(!handler.state().toggle_overlay);

        handler.handle_gameplay_event(
            &Event::KeyDown {
                timestamp: 0,
                window_id: 0,
                keycode: Some(Keycode::Minus),
                scancode: None,
                keymod: sdl2::keyboard::Mod::NOMOD,
                repeat: false,
            },
            &bindings,
        );
        assert!(handler.state().toggle_overlay);

        handler.handle_gameplay_event(
            &Event::KeyDown {
                timestamp: 0,
                window_id: 0,
                keycode: Some(Keycode::Minus),
                scancode: None,
                keymod: sdl2::keyboard::Mod::NOMOD,
                repeat: false,
            },
            &bindings,
        );
        assert!(!handler.state().toggle_overlay);

        handler.handle_gameplay_event(
            &Event::KeyDown {
                timestamp: 0,
                window_id: 0,
                keycode: Some(Keycode::KpMinus),
                scancode: None,
                keymod: sdl2::keyboard::Mod::NOMOD,
                repeat: false,
            },
            &bindings,
        );
        assert!(handler.state().toggle_overlay);
    }
}
