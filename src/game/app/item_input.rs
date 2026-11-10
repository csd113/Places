//! A held-item mouse edge survives only eligible captured gameplay. Observing
//! mouse releases before menu/console consumption prevents clicks from leaking.

#[derive(Default)]
pub struct CarryInput {
    left_down: bool,
    focus_guard: bool,
    pending: Option<u64>,
    world_click: bool,
}

#[derive(Clone, Copy)]
pub struct MouseContext {
    pub gameplay: bool,
    pub captured: bool,
    pub held: Option<u64>,
}

impl CarryInput {
    /// A tool claims the complete eligible mouse edge before carry/elevator
    /// dispatch. The same click cannot also throw or press a world button.
    pub const fn take_tool_click(&mut self, eligible: bool) -> bool {
        if !eligible {
            return false;
        }
        let clicked = self.world_click || self.pending.is_some();
        self.world_click = false;
        self.pending = None;
        clicked
    }
    pub const fn invalidate(&mut self) {
        self.pending = None;
        self.world_click = false;
        self.focus_guard = true;
    }
    pub const fn mouse_button(&mut self, pressed: bool, context: MouseContext) {
        if !pressed {
            self.left_down = false;
            return;
        }
        if self.left_down {
            return;
        }
        self.left_down = true;
        if context.gameplay && context.captured && !self.focus_guard {
            self.pending = context.held;
            self.world_click = context.held.is_none();
        }
    }

    pub const fn focus_changed(&mut self, left_down: bool) {
        self.left_down = left_down;
        self.focus_guard = true;
        self.pending = None;
        self.world_click = false;
    }

    pub const fn take_world_click(&mut self, gameplay: bool) -> bool {
        let pressed = self.world_click;
        self.world_click = false;
        gameplay && pressed
    }
    pub fn take_throw(&mut self, gameplay: bool) -> Option<u64> {
        self.focus_guard = false;
        let pending = self.pending.take();
        gameplay.then_some(pending).flatten()
    }
    pub const fn take_elevator_click(&mut self, eligible: bool) -> bool {
        eligible && self.pending.take().is_some()
    }
}

#[cfg(test)]
#[path = "../tests/elevator_input.rs"]
mod tests;
