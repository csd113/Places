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
