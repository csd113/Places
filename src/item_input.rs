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

#[cfg(test)]
mod tests {
    use super::{CarryInput, MouseContext};

    fn context() -> MouseContext {
        MouseContext {
            gameplay: true,
            captured: true,
            held: Some(7),
        }
    }

    // Protects click edges across overlay/focus/capture ownership. Pure values,
    // no SDL window or asset load; each blocked click must require a fresh edge.
    #[test]
    fn only_a_fresh_captured_gameplay_click_can_throw_the_same_held_item() {
        let mut input = CarryInput::default();
        for (gameplay, captured) in [(false, true), (true, false)] {
            input.mouse_button(
                true,
                MouseContext {
                    gameplay,
                    captured,
                    held: Some(7),
                },
            );
            assert_eq!(
                input.take_throw(true),
                None,
                "overlay and recapture clicks stay consumed"
            );
            input.mouse_button(true, context());
            assert_eq!(
                input.take_throw(true),
                None,
                "a held button cannot become a fresh press"
            );
            input.mouse_button(false, context());
        }
        input.focus_changed(false);
        input.mouse_button(true, context());
        assert_eq!(
            input.take_throw(true),
            None,
            "the focus-gaining click is consumed"
        );
        input.mouse_button(false, context());
        input.mouse_button(true, context());
        assert_eq!(
            input.take_throw(true),
            Some(7),
            "a fresh gameplay edge records its item identity"
        );
        assert_eq!(input.take_throw(true), None, "one click dispatches once");
        input.mouse_button(false, context());
        input.mouse_button(true, context());
        input.focus_changed(false);
        assert_eq!(
            input.take_throw(true),
            None,
            "focus loss clears an already queued click"
        );
        input.mouse_button(false, context());
        input.mouse_button(true, context());
        input.invalidate();
        assert_eq!(
            input.take_throw(true),
            None,
            "a same-batch overlay round trip cancels the earlier click"
        );
    }
}
