//! Input routing keeps modal edits, panels and viewport navigation separate.

use crate::engine::render::DrawableSize;
use sdl3::event::{Event, WindowEvent};
use sdl3::keyboard::{Keycode, Mod};
use sdl3::mouse::MouseButton;
use sdl3::video::Window;

use super::App;
use super::actions::{Pending, Popup};
use super::ui::{Action, Hit};

impl App {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "SDL delivers unrelated window, device and key events; this modal consumes only numeric-entry events."
    )]
    fn numeric_event(&mut self, event: &Event) -> bool {
        if let Some(Popup::Number { field, value }) = &mut self.popup {
            let mut apply = None;
            match event {
                Event::TextInput {
                    text,
                    timestamp: _,
                    window_id: _,
                } => {
                    if value.len() < 64 {
                        value.push_str(text);
                    }
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Backspace),
                    timestamp: _,
                    window_id: _,
                    scancode: _,
                    keymod: _,
                    repeat: _,
                    which: _,
                    raw: _,
                } => {
                    let _removed = value.pop();
                }
                Event::KeyDown {
                    keycode: Some(Keycode::Return),
                    repeat: false,
                    timestamp: _,
                    window_id: _,
                    scancode: _,
                    keymod: _,
                    which: _,
                    raw: _,
                } => apply = Some((*field, value.parse::<f32>())),
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    repeat: false,
                    timestamp: _,
                    window_id: _,
                    scancode: _,
                    keymod: _,
                    which: _,
                    raw: _,
                } => {
                    self.popup = if *field >= 100 {
                        Some(Popup::Gameplay { page: 0 })
                    } else if (7..=10).contains(field) {
                        Some(Popup::Shape)
                    } else {
                        None
                    }
                }
                Event::KeyDown {
                    keycode: Some(Keycode::A),
                    keymod,
                    timestamp: _,
                    window_id: _,
                    scancode: _,
                    repeat: _,
                    which: _,
                    raw: _,
                } if command(*keymod) => value.clear(),
                _ => {}
            }
            if let Some((edited_field, parsed)) = apply {
                match parsed {
                    Ok(number) => {
                        let applied = self.apply_field(edited_field, number);
                        if applied.is_ok() {
                            self.popup = if edited_field >= 100 {
                                Some(Popup::Gameplay { page: 0 })
                            } else if (7..=10).contains(&edited_field) {
                                Some(Popup::Shape)
                            } else {
                                None
                            };
                        }
                        self.report(applied);
                    }
                    Err(_) => {
                        self.status =
                            "Enter a number; Cmd/Ctrl A clears the field. Esc cancels.".into();
                    }
                }
            }
            return true;
        }

        false
    }

    fn focus_event(&mut self, event: &Event, window: &Window, hits: &[Hit]) -> bool {
        if let Event::KeyDown {
            keycode: Some(key),
            keymod,
            repeat: false,
            timestamp: _,
            window_id: _,
            scancode: _,
            which: _,
            raw: _,
        } = event
        {
            if *key == Keycode::Tab {
                let current = hits
                    .iter()
                    .position(|hit| Some(hit.action) == self.ui_focus);
                let index = if keymod.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD) {
                    current
                        .unwrap_or(0)
                        .checked_sub(1)
                        .unwrap_or_else(|| hits.len().saturating_sub(1))
                } else {
                    current
                        .map_or(0, |i| i.saturating_add(1))
                        .checked_rem(hits.len().max(1))
                        .unwrap_or(0)
                };
                self.ui_focus = hits.get(index).map(|hit| hit.action);
                return true;
            }
            if *key == Keycode::Return
                && let Some(hit) = hits.iter().find(|hit| Some(hit.action) == self.ui_focus)
            {
                let result = self.activate(hit.action, window);
                self.report(result);
                return true;
            }
        }
        false
    }

    fn click(&mut self, window: &Window, hits: &[Hit], window_point: [f32; 2]) {
        let physical = window.size_in_pixels();
        if let Some(point) =
            DrawableSize::new(physical.0, physical.1).window_to_ui(window.size(), window_point)
        {
            if let Some(hit) = hits.iter().find(|hit| hit.contains(point)) {
                self.ui_focus = Some(hit.action);
                let result = self.activate(hit.action, window);
                self.report(result);
            } else if self.popup.is_none()
                && point[0] >= 112.0
                && point[1] >= 38.0
                && point[1] < 242.0
            {
                if self.navigation_click(point) {
                    return;
                }
                self.selected = self.viewport.pick(point);
                self.viewport.select(self.selected);
            }
        }
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Unrelated SDL events are routed to the viewport or ignored; the editor does not own every native event."
    )]
    pub(super) fn event(&mut self, event: &Event, window: &Window, hits: &[Hit]) {
        if matches!(
            event,
            Event::Quit { timestamp: _ }
                | Event::Window {
                    win_event: WindowEvent::CloseRequested,
                    timestamp: _,
                    window_id: _
                }
        ) {
            self.request(Pending::Close, window);
            return;
        }
        if self.project_text_event(event)
            || self.script_event(event, window)
            || self.component_text_event(event)
            || self.transform_event(event)
            || self.search_event(event)
            || self.numeric_event(event)
            || self.focus_event(event, window, hits)
        {
            return;
        }
        match event {
            Event::MouseButtonUp {
                mouse_btn: MouseButton::Left,
                x,
                y,
                timestamp: _,
                window_id: _,
                which: _,
                clicks: _,
            } => {
                self.click(window, hits, [*x, *y]);
            }
            Event::MouseWheel {
                y,
                mouse_x,
                mouse_y,
                timestamp: _,
                window_id: _,
                which: _,
                x: _,
                direction: _,
                integer_x: _,
                integer_y: _,
            } if *mouse_x
                < 112.0 * f32::from(u16::try_from(window.size().0).unwrap_or(1440)) / 480.0
                && *mouse_y < 400.0 =>
            {
                let count = self.scene_objects().len();
                self.object_scroll = if *y > 0.0 {
                    self.object_scroll.saturating_sub(1)
                } else {
                    self.object_scroll
                        .saturating_add(1)
                        .min(count.saturating_sub(4))
                };
            }
            Event::KeyDown {
                keycode: Some(key),
                keymod,
                repeat: false,
                timestamp: _,
                window_id: _,
                scancode: _,
                which: _,
                raw: _,
            } if self.popup.is_none() => {
                if let Some(action) = shortcut(*key, *keymod) {
                    let result = self.activate(action, window);
                    self.report(result);
                } else {
                    let _handled = self.viewport.handle_event(event);
                }
            }
            Event::KeyDown {
                keycode: Some(Keycode::Escape),
                repeat: false,
                timestamp: _,
                window_id: _,
                scancode: _,
                keymod: _,
                which: _,
                raw: _,
            } => self.close_popup(),
            _ if self.popup.is_none()
                || matches!(
                    self.popup,
                    Some(Popup::Safety {
                        index: _,
                        preview: true
                    })
                ) =>
            {
                let _handled = self.viewport.handle_event(event);
            }
            _ => {}
        }
    }
}

pub(super) fn command(modifiers: Mod) -> bool {
    modifiers.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD | Mod::LGUIMOD | Mod::RGUIMOD)
}

#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "Only the listed native keys are editor shortcuts; all other keys are viewport input."
)]
fn shortcut(key: Keycode, modifiers: Mod) -> Option<Action> {
    if command(modifiers) {
        match key {
            Keycode::S => Some(Action::Save),
            Keycode::O => Some(Action::Open),
            Keycode::N => Some(Action::New),
            Keycode::D => Some(Action::Duplicate),
            Keycode::Z => Some(if modifiers.intersects(Mod::LSHIFTMOD | Mod::RSHIFTMOD) {
                Action::Redo
            } else {
                Action::Undo
            }),
            Keycode::Y => Some(Action::Redo),
            Keycode::B => Some(Action::Build),
            Keycode::P => Some(Action::Play),
            _ => None,
        }
    } else {
        match key {
            Keycode::F12 => Some(Action::Capture),
            Keycode::Delete | Keycode::Backspace => Some(Action::Delete),
            Keycode::Left => Some(Action::Nudge(0, false)),
            Keycode::Right => Some(Action::Nudge(0, true)),
            Keycode::Up => Some(Action::Nudge(2, false)),
            Keycode::Down => Some(Action::Nudge(2, true)),
            Keycode::PageUp => Some(Action::Nudge(1, true)),
            Keycode::PageDown => Some(Action::Nudge(1, false)),
            Keycode::R => Some(Action::Nudge(6, true)),
            Keycode::Plus | Keycode::Equals | Keycode::KpPlus => Some(Action::Nudge(3, true)),
            Keycode::Minus | Keycode::KpMinus => Some(Action::Nudge(3, false)),
            _ => None,
        }
    }
}
