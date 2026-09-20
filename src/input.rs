use sdl2::event::Event;
use sdl2::keyboard::Keycode;

/// Raw input state representing keys used by PocketCHIP navigation and controls.
#[derive(Debug, Default, Clone, Copy)]
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

/// Handles incoming SDL events and maintains active input state.
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

    /// Process a single SDL event according to the PocketCHIP control layout.
    pub fn handle_event(&mut self, event: &Event) {
        match event {
            Event::Quit { .. } => {
                self.state.quit_requested = true;
            }
            Event::KeyDown {
                keycode: Some(key),
                repeat: false,
                ..
            } => match *key {
                Keycode::Escape => self.state.quit_requested = true,
                Keycode::W => self.state.move_forward = true,
                Keycode::A => self.state.strafe_left = true,
                Keycode::S => self.state.strafe_right = true,
                Keycode::Z => self.state.move_backward = true,
                Keycode::Q => self.state.look_up = true,
                Keycode::Period => self.state.look_down = true,
                Keycode::K => self.state.look_left = true,
                Keycode::L => self.state.look_right = true,
                Keycode::Minus => self.state.toggle_overlay = !self.state.toggle_overlay,
                _ => {}
            },
            Event::KeyUp {
                keycode: Some(key), ..
            } => match *key {
                Keycode::W => self.state.move_forward = false,
                Keycode::A => self.state.strafe_left = false,
                Keycode::S => self.state.strafe_right = false,
                Keycode::Z => self.state.move_backward = false,
                Keycode::Q => self.state.look_up = false,
                Keycode::Period => self.state.look_down = false,
                Keycode::K => self.state.look_left = false,
                Keycode::L => self.state.look_right = false,
                _ => {}
            },
            _ => {}
        }
    }
}
