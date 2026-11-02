//! Bounded scene-name filtering keeps large authored scenes easy to select.

use sdl3::event::Event;
use sdl3::keyboard::{Keycode, Mod};

use super::{App, actions::Popup, document::ObjectInfo};

pub(super) fn draw_search(
    app: &App,
    v: &mut Vec<crate::engine::render::Vertex>,
    hits: &mut Vec<super::ui::Hit>,
    value: &str,
) {
    use super::ui::{self, Action};
    ui::rect(v, [52.0, 42.0, 376.0, 200.0], [0.105, 0.14, 0.185, 1.0]);
    ui::text(v, "SCENE / FIND OBJECTS", 62.0, 51.0, ui::INK, 70);
    ui::button(
        v,
        hits,
        [62.0, 64.0, 350.0, 12.0],
        &format!("Search: {value}|"),
        Action::SceneSearch,
        true,
    );
    let query = value.to_ascii_lowercase();
    let objects = app
        .document
        .as_ref()
        .map(|doc| {
            doc.objects()
                .iter()
                .filter(|object| {
                    app.focus.included(doc.level(), object.id)
                        && object.label.to_ascii_lowercase().contains(&query)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for (offset, object) in objects.iter().skip(app.object_scroll).take(14).enumerate() {
        let column = u16::try_from(offset / 7).unwrap_or(0);
        let row = u16::try_from(offset % 7).unwrap_or(0);
        ui::button(
            v,
            hits,
            [
                f32::from(column).mul_add(181.0, 62.0),
                f32::from(row).mul_add(17.0, 83.0),
                176.0,
                14.0,
            ],
            &format!(
                "{}{}",
                if app.editable(object.id) { "" } else { "[L] " },
                object.label
            ),
            Action::Select(object.id),
            true,
        );
    }
    if objects.is_empty() {
        ui::text(v, "No matching objects", 62.0, 95.0, ui::MUTED, 60);
    }
    ui::button(
        v,
        hits,
        [62.0, 211.0, 62.0, 12.0],
        "< Previous",
        Action::ScenePage(false),
        app.object_scroll > 0,
    );
    ui::button(
        v,
        hits,
        [128.0, 211.0, 62.0, 12.0],
        "Next >",
        Action::ScenePage(true),
        app.object_scroll.saturating_add(14) < objects.len(),
    );
    ui::button(
        v,
        hits,
        [357.0, 223.0, 61.0, 12.0],
        "Close",
        Action::ClosePopup,
        true,
    );
    ui::text(
        v,
        "Type to filter / Ctrl-Cmd A clears / Esc closes",
        62.0,
        227.0,
        ui::MUTED,
        72,
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
            Action::SceneSearch => {
                if !matches!(self.popup, Some(Popup::SceneSearch(_))) {
                    self.object_scroll = 0;
                    self.popup = Some(Popup::SceneSearch(self.object_query.clone()));
                }
            }
            Action::ScenePage(forward) => {
                self.object_scroll = if forward {
                    self.object_scroll.saturating_add(14)
                } else {
                    self.object_scroll.saturating_sub(14)
                };
            }
            Action::PreviewSound(index) => self.preview_sound(index)?,
            Action::StopPreviewSound => {
                self.preview_audio.clear_world();
                self.sound_preview_until = None;
            }
            Action::Select(id) => {
                self.selected = Some(id);
                if matches!(self.popup, Some(Popup::SceneSearch(_))) {
                    self.popup = None;
                }
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
        let field_focused = self.focused_action() == Some(super::ui::Action::SceneSearch);
        let Some(Popup::SceneSearch(value)) = &mut self.popup else {
            return false;
        };
        match event {
            Event::TextInput {
                text,
                timestamp: _,
                window_id: _,
            } => {
                if !field_focused {
                    return false;
                }
                if value.len().saturating_add(text.len()) <= 64 {
                    value.push_str(text);
                    self.object_scroll = 0;
                }
            }
            Event::KeyDown {
                keycode: Some(key),
                keymod: _,
                timestamp: _,
                window_id: _,
                scancode: _,
                repeat: _,
                which: _,
                raw: _,
            } if *key == Keycode::Tab || (!field_focused && *key != Keycode::Escape) => {
                return false;
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
                    self.object_scroll = 0;
                }
                Keycode::Backspace => {
                    let _removed = value.pop();
                    self.object_scroll = 0;
                }
                Keycode::Return => {
                    self.object_query.clone_from(value);
                    self.object_scroll = 0;
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
