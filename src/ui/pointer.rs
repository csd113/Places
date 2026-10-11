//! Pointer ownership and click edges, independent of SDL and the renderer.
use super::MenuHit;

#[derive(Debug, Default, Clone)]
pub struct MenuPointer {
    pub hover: Option<MenuHit>,
    position: Option<[f32; 2]>,
    left_down: bool,
    context: Option<u64>,
}

impl MenuPointer {
    /// A screen, viewport, scroll or rebind transition clears hover. A button
    /// held across that boundary must first be released before another action.
    pub fn sync_context(&mut self, context: u64) {
        if self.context != Some(context) {
            self.invalidate();
            self.context = Some(context);
        }
    }

    pub const fn invalidate(&mut self) {
        self.hover = None;
        self.position = None;
    }

    /// Keyboard focus remains authoritative until real pointer movement.
    pub const fn keyboard(&mut self) {
        self.hover = None;
    }

    pub const fn focus(&mut self, left_down: bool) {
        self.invalidate();
        self.left_down = left_down;
    }

    pub fn motion(&mut self, point: [f32; 2], hit: Option<MenuHit>) -> bool {
        if !point.iter().all(|coordinate| coordinate.is_finite()) || self.position == Some(point) {
            return false;
        }
        self.position = Some(point);
        self.hover = hit;
        true
    }

    /// Game menus act on a fresh press, once per physical click. A release is
    /// required before any later action, including across screen transitions.
    /// The caller supplies no target during gameplay or focus loss.
    pub const fn button(&mut self, down: bool, hit: Option<MenuHit>) -> Option<MenuHit> {
        if down {
            let fresh = !self.left_down;
            self.left_down = true;
            if fresh {
                self.hover = hit;
                hit
            } else {
                None
            }
        } else {
            self.left_down = false;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::MenuIntent;

    const HIT: MenuHit = MenuHit {
        index: 2,
        intent: MenuIntent::Activate,
    };

    #[test]
    fn clicks_require_a_fresh_press_and_never_repeat_on_release() {
        let mut pointer = MenuPointer::default();
        assert_eq!(pointer.button(false, Some(HIT)), None, "orphan release");
        assert_eq!(
            pointer.button(true, Some(HIT)),
            Some(HIT),
            "one fresh press"
        );
        assert_eq!(pointer.button(true, Some(HIT)), None, "held repeat");
        assert_eq!(
            pointer.button(false, Some(HIT)),
            None,
            "release never activates"
        );
        assert_eq!(pointer.button(false, Some(HIT)), None, "duplicate release");
        assert_eq!(pointer.button(true, Some(HIT)), Some(HIT), "fresh press");
        assert_eq!(pointer.button(false, None), None, "release outside");
        assert_eq!(pointer.button(true, None), None, "outside press");
        assert_eq!(
            pointer.button(false, Some(HIT)),
            None,
            "outside press then release inside never activates"
        );
    }

    #[test]
    fn opening_resizing_scrolling_and_focus_loss_cancel_held_clicks() {
        for context in 0..4 {
            let mut pointer = MenuPointer::default();
            pointer.sync_context(context);
            assert_eq!(pointer.button(true, Some(HIT)), Some(HIT), "fresh press");
            pointer.sync_context(context.saturating_add(1));
            assert_eq!(pointer.hover, None, "stale hover cleared");
            assert_eq!(
                pointer.button(true, Some(HIT)),
                None,
                "held after transition"
            );
            assert_eq!(
                pointer.button(false, Some(HIT)),
                None,
                "release after transition"
            );
        }
    }

    #[test]
    fn stationary_pointer_does_not_steal_keyboard_focus() {
        let mut pointer = MenuPointer::default();
        assert!(pointer.motion([10.0, 20.0], Some(HIT)), "initial move");
        pointer.keyboard();
        assert!(
            !pointer.motion([10.0, 20.0], Some(HIT)),
            "stationary motion ignored"
        );
        assert_eq!(pointer.hover, None, "keyboard owns focus");
        assert!(
            pointer.motion([11.0, 20.0], Some(HIT)),
            "new movement takes focus"
        );
        assert_eq!(pointer.hover, Some(HIT), "new hover");
    }
}
