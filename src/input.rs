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
    /// Jump, and swim upwards while in deep water.
    Jump,
    /// Toggle the crouched stance (press to change, not hold).
    Crouch,
    /// Act on the object the player is looking at (press once, not hold).
    Interact,
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
            Self::Jump => 1 << 8,
            Self::Crouch => 1 << 9,
            Self::Interact => 1 << 10,
        }
    }
}

/// Every gameplay control, in bit order.
///
/// The developer move script iterates this table so a scripted release always
/// covers every control the script names; tests use it to drive each binding.
pub const ALL_CONTROLS: [Control; 11] = [
    Control::MoveForward,
    Control::MoveBackward,
    Control::StrafeLeft,
    Control::StrafeRight,
    Control::LookUp,
    Control::LookDown,
    Control::LookLeft,
    Control::LookRight,
    Control::Jump,
    Control::Crouch,
    Control::Interact,
];

/// One control hold from the `PLACES_MOVE_SCRIPT` developer override.
///
/// The hold covers the ready-world simulation seconds in
/// `first_seconds..=last_seconds`, so a capture sequence happens at the same
/// in-world time at any frame rate. It exists so a capture run can drive the
/// player through real motion - a jump, a pool crossing - without touching the
/// world directly; ordinary play never sets it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScriptedHold {
    /// The control to hold.
    pub control: Control,
    /// First ready-world simulation second of the hold, inclusive.
    pub first_seconds: f32,
    /// Last ready-world simulation second of the hold, inclusive.
    pub last_seconds: f32,
}

/// The control a script name selects, or `None` for an unknown name.
#[must_use]
pub fn scripted_control(name: &str) -> Option<Control> {
    match name {
        "forward" => Some(Control::MoveForward),
        "backward" => Some(Control::MoveBackward),
        "strafe_left" => Some(Control::StrafeLeft),
        "strafe_right" => Some(Control::StrafeRight),
        "look_up" => Some(Control::LookUp),
        "look_down" => Some(Control::LookDown),
        "look_left" => Some(Control::LookLeft),
        "look_right" => Some(Control::LookRight),
        "jump" => Some(Control::Jump),
        "crouch" => Some(Control::Crouch),
        "interact" => Some(Control::Interact),
        _ => None,
    }
}

/// Parses a `PLACES_MOVE_SCRIPT` value: `control@first-last` entries separated
/// by commas, for example `forward@0-6.5,jump@0.2-0.6,jump@3-3.4`.
///
/// `first` and `last` are ready-world simulation seconds, so the same script
/// drives the same in-world sequence at any frame rate. Returns the accepted
/// holds in authored order and the rejected entries, so the caller can report a
/// typo once instead of silently doing nothing.
#[must_use]
pub fn parse_move_script(value: &str) -> (Vec<ScriptedHold>, Vec<String>) {
    let mut holds = Vec::new();
    let mut rejected = Vec::new();
    for entry in value.split(',') {
        let trimmed_entry = entry.trim();
        if trimmed_entry.is_empty() {
            continue;
        }
        let Some((name, range)) = trimmed_entry.split_once('@') else {
            rejected.push(trimmed_entry.to_string());
            continue;
        };
        let Some((first, last)) = range.split_once('-') else {
            rejected.push(trimmed_entry.to_string());
            continue;
        };
        let (Some(control), Ok(first_seconds), Ok(last_seconds)) = (
            scripted_control(name.trim()),
            first.trim().parse::<f32>(),
            last.trim().parse::<f32>(),
        ) else {
            rejected.push(trimmed_entry.to_string());
            continue;
        };
        if !first_seconds.is_finite()
            || !last_seconds.is_finite()
            || first_seconds < 0.0
            || last_seconds < first_seconds
        {
            rejected.push(trimmed_entry.to_string());
            continue;
        }
        holds.push(ScriptedHold {
            control,
            first_seconds,
            last_seconds,
        });
    }
    (holds, rejected)
}

/// Raw input state representing gameplay movement, camera looking and the
/// accumulated relative mouse motion.
///
/// The movement, look, jump, crouch and interact controls are independent bits
/// rather than separate `bool` fields: they are all set and cleared by the same
/// binding lookup. Rising edges survive a release until the next gameplay
/// frame, so short taps cannot disappear between event polling and simulation.
/// `quit_requested` stays a named field because the game flips it itself
/// instead of holding a key. Relative mouse motion accumulates in pixels and is
/// consumed once per simulation update.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct InputState {
    held: u16,
    /// Rising edges retained until one gameplay frame consumes them.
    pressed: u16,
    pub quit_requested: bool,
    /// Accumulated relative mouse motion, in pixels, since the last consume.
    mouse_dx: f32,
    mouse_dy: f32,
}

impl InputState {
    /// The control the binding called `name` holds, or `None` when `name` is
    /// not one of the movement, look or jump bindings.
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
        } else if name == bindings.jump {
            Some(Control::Jump)
        } else if name == bindings.crouch {
            Some(Control::Crouch)
        } else if name == bindings.interact {
            Some(Control::Interact)
        } else {
            None
        }
    }

    /// Presses or releases one held control.
    const fn set_held(&mut self, control: Control, pressed: bool) {
        if pressed {
            if self.held & control.bit() == 0 {
                self.pressed |= control.bit();
            }
            self.held |= control.bit();
        } else {
            self.held &= !control.bit();
        }
    }

    /// Applies one frame of a `PLACES_MOVE_SCRIPT` developer script.
    ///
    /// Every control the script names is held while *any* of its ranges covers
    /// the ready-world simulation second `seconds`, and released otherwise,
    /// exactly as a player pressing and lifting the key would be: the rising
    /// edge is set once per range, so two `jump@` ranges are two presses.
    /// Controls the script never names are left untouched.
    pub(crate) fn apply_move_script(&mut self, script: &[ScriptedHold], seconds: f32) {
        for control in ALL_CONTROLS {
            let mut scripted = false;
            let mut held = false;
            for entry in script {
                if entry.control != control {
                    continue;
                }
                scripted = true;
                held = held || (seconds >= entry.first_seconds && seconds <= entry.last_seconds);
            }
            if scripted {
                self.set_held(control, held);
            }
        }
    }

    /// Releases every held control and discards pending mouse motion, leaving
    /// the quit flag alone.
    const fn release_all(&mut self) {
        self.held = 0;
        self.pressed = 0;
        self.mouse_dx = 0.0;
        self.mouse_dy = 0.0;
    }

    /// True while `control` is held.
    #[must_use]
    pub const fn is_held(self, control: Control) -> bool {
        self.held & control.bit() != 0
    }

    /// True when a physical press arrived since the previous gameplay frame,
    /// including a press released before that frame began.
    pub(crate) const fn was_pressed(self, control: Control) -> bool {
        self.pressed & control.bit() != 0
    }

    pub(crate) const fn clear_presses(&mut self) {
        self.pressed = 0;
    }

    /// Accumulates one relative mouse-motion event's pixel deltas.
    ///
    /// A non-finite delta is ignored and an accumulation that would overflow to
    /// infinity is refused, so the camera can never acquire a NaN or an
    /// infinite turn from a broken platform event.
    pub const fn accumulate_mouse_motion(&mut self, dx: f32, dy: f32) {
        self.mouse_dx = accumulate_axis(self.mouse_dx, dx);
        self.mouse_dy = accumulate_axis(self.mouse_dy, dy);
    }

    /// Returns the accumulated relative mouse motion in pixels and clears it,
    /// so one event's motion can be applied exactly once.
    #[must_use]
    pub const fn take_mouse_motion(&mut self) -> (f32, f32) {
        let motion = (self.mouse_dx, self.mouse_dy);
        self.mouse_dx = 0.0;
        self.mouse_dy = 0.0;
        motion
    }

    /// State with every listed control held, for tests that drive the player
    /// without an SDL event queue.
    #[cfg(test)]
    pub(crate) fn holding(controls: &[Control]) -> Self {
        let mut state = Self::default();
        for control in controls {
            state.held |= control.bit();
        }
        state
    }
}

/// Adds one finite mouse delta to an accumulated axis, refusing any sum that
/// leaves the finite range.
const fn accumulate_axis(current: f32, delta: f32) -> f32 {
    if !delta.is_finite() {
        return current;
    }
    let sum = current + delta;
    if sum.is_finite() { sum } else { current }
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

    /// Mutable access to the held state, for the simulation that consumes the
    /// accumulated mouse motion each frame.
    #[must_use]
    pub const fn state_mut(&mut self) -> &mut InputState {
        &mut self.state
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
        if let Event::MouseMotion {
            xrel,
            yrel,
            timestamp: _,
            window_id: _,
            which: _,
            mousestate: _,
            x: _,
            y: _,
        } = event
        {
            self.state.accumulate_mouse_motion(*xrel, *yrel);
            return;
        }
        if let Event::Quit { timestamp: _ } = event {
            self.state.quit_requested = true;
            return;
        }
        if let Event::KeyDown {
            keycode: Some(key),
            repeat: false,
            timestamp: _,
            window_id: _,
            scancode: _,
            keymod: _,
            which: _,
            raw: _,
        } = event
        {
            let name = keycode_to_str(*key);
            if let Some(button) = InputState::binding_control(bindings, &name) {
                self.state.set_held(button, true);
            }
            return;
        }
        if let Event::KeyUp {
            keycode: Some(key),
            timestamp: _,
            window_id: _,
            scancode: _,
            keymod: _,
            repeat: _,
            which: _,
            raw: _,
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
            timestamp: _,
            window_id: _,
            scancode: _,
            keymod: _,
            which: _,
            raw: _,
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
