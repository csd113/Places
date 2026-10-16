//! A held-item mouse edge survives only eligible captured gameplay. Observing
//! mouse releases before menu/console consumption prevents clicks from leaking.

#[derive(Default)]
pub struct CarryInput {
    left_down: bool,
    focus_guard: bool,
    pending: Option<u64>,
}

#[derive(Clone, Copy)]
pub struct MouseContext {
    pub gameplay: bool,
    pub captured: bool,
    pub held: Option<u64>,
}

impl CarryInput {
    pub const fn invalidate(&mut self) {
        self.pending = None;
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
        }
    }

    pub const fn focus_changed(&mut self, left_down: bool) {
        self.left_down = left_down;
        self.focus_guard = true;
        self.pending = None;
    }

    pub fn take_throw(&mut self, gameplay: bool) -> Option<u64> {
        self.focus_guard = false;
        let pending = self.pending.take();
        gameplay.then_some(pending).flatten()
    }
}
