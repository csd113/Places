//! Bounded scene-name filtering keeps large authored scenes easy to select.

use sdl3::event::Event;
use sdl3::keyboard::{Keycode, Mod};

use super::{App, actions::Popup, document::ObjectInfo};

pub(super) fn draw_search(
    v: &mut Vec<crate::engine::render::Vertex>,
    hits: &mut Vec<super::ui::Hit>,
    value: &str,
) {
    use super::ui::{self, Action};
    ui::text(v, "FIND SCENE OBJECTS", 140.0, 78.0, ui::INK, 60);
    ui::text(v, value, 140.0, 103.0, ui::INK, 60);
    ui::text(
        v,
        "Enter applies. Empty text shows all.",
        140.0,
        129.0,
        ui::INK,
        60,
    );
    ui::text(
        v,
        "Cmd/Ctrl A clears. Escape cancels.",
        140.0,
        145.0,
        ui::INK,
        60,
    );
    ui::popup_button(
        v,
        hits,
        [140.0, 178.0, 235.0, 13.0],
        "Cancel",
        Action::ClosePopup,
    );
}

impl App {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only selection and field-sheet actions belong to this contextual handler."
    )]
    pub(super) fn selection_ui_action(
        &mut self,
        action: super::ui::Action,
    ) -> Result<bool, String> {
        use super::ui::Action;
        match action {
            Action::SceneSearch => self.popup = Some(Popup::SceneSearch(self.object_query.clone())),
            Action::PreviewSound(index) => self.preview_sound(index)?,
            Action::StopPreviewSound => {
                self.preview_audio.clear_world();
                self.sound_preview_until = None;
            }
            Action::Select(id) => {
                self.selected = Some(id);
                self.viewport.select(Some(id));
                self.viewport.highlight(None, &[]);
            }
            Action::Field(field) => self.open_number(field)?,
            Action::Assets | Action::Materials => {
                self.browser.material_ceiling = false;
                self.browser.query.clear();
                self.browser.theme = 0;
                self.popup = Some(Popup::Library {
                    materials: action == Action::Materials,
                    page: 0,
                });
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub(super) fn scene_objects(&self) -> Vec<&ObjectInfo> {
        let query = self.object_query.to_ascii_lowercase();
        self.document.as_ref().map_or_else(Vec::new, |document| {
            document
                .objects()
                .iter()
                .filter(|object| {
                    self.focus.included(document.level(), object.id)
                        && (query.is_empty() || object.label.to_ascii_lowercase().contains(&query))
                })
                .collect()
        })
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "The scene filter only consumes its own text and keyboard events."
    )]
    pub(super) fn search_event(&mut self, event: &Event) -> bool {
        let Some(Popup::SceneSearch(value)) = &mut self.popup else {
            return false;
        };
        match event {
            Event::TextInput {
                text,
                timestamp: _,
                window_id: _,
            } => {
                if value.len().saturating_add(text.len()) <= 64 {
                    value.push_str(text);
                }
            }
            Event::KeyDown {
                keycode: Some(key),
                keymod,
                timestamp: _,
                window_id: _,
                scancode: _,
                repeat: _,
                which: _,
                raw: _,
            } => match *key {
                Keycode::A
                    if keymod.intersects(
                        Mod::LCTRLMOD | Mod::RCTRLMOD | Mod::LGUIMOD | Mod::RGUIMOD,
                    ) =>
                {
                    value.clear();
                }
                Keycode::Backspace => {
                    let _removed = value.pop();
                }
                Keycode::Return => {
                    self.object_query.clone_from(value);
                    self.object_scroll = 0;
                    self.popup = None;
                }
                Keycode::Escape => self.popup = None,
                _ => {}
            },
            Event::MouseButtonUp {
                timestamp: _,
                window_id: _,
                which: _,
                mouse_btn: _,
                clicks: _,
                x: _,
                y: _,
            } => return false,
            _ => {}
        }
        true
    }
}
