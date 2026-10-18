//! Complete-pose drafts avoid invalid intermediate geometry and commit once.

use sdl3::event::Event;
use sdl3::keyboard::{Keycode, Mod};

use super::App;
use super::actions::Popup;
use super::document::{ObjectId, Transform};
use super::ui::Action;

pub(super) struct Draft {
    pub id: ObjectId,
    pub fields: Vec<String>,
    pub active: Option<usize>,
    pub preview: bool,
    expected: serde_json::Value,
}

impl Draft {
    fn new(app: &App) -> Result<Self, String> {
        let document = app.document.as_ref().ok_or("Open a project first")?;
        let id = app.selected.ok_or("Select an object first")?;
        let transform = document.transform(id)?;
        Ok(Self {
            id,
            fields: transform
                .position
                .into_iter()
                .chain(transform.size)
                .chain([
                    transform.yaw_degrees,
                    transform.pitch_degrees,
                    transform.roll_degrees,
                ])
                .map(|number| number.to_string())
                .collect(),
            active: None,
            preview: false,
            expected: document.raw().clone(),
        })
    }

    fn transform(&self) -> Result<Transform, String> {
        let mut values = [0.0_f32; 9];
        for (value, text) in values.iter_mut().zip(&self.fields) {
            *value = text
                .parse::<f32>()
                .map_err(|_error| "Every transform field needs a number")?;
            if !value.is_finite() {
                return Err("Every transform field must be finite".into());
            }
        }
        let [
            x,
            y,
            z,
            width,
            height,
            depth,
            yaw_degrees,
            pitch_degrees,
            roll_degrees,
        ] = values;
        Ok(Transform {
            position: [x, y, z],
            size: if matches!(self.id, ObjectId::Prop(_)) {
                [width; 3]
            } else {
                [width, height, depth]
            },
            yaw_degrees,
            pitch_degrees,
            roll_degrees,
        })
    }
}

impl App {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only complete-transform commands belong to this contextual handler."
    )]
    pub(super) fn transform_action(&mut self, action: Action) -> Result<bool, String> {
        match action {
            Action::Transform => self.popup = Some(Popup::Transform(Box::new(Draft::new(self)?))),
            Action::TransformField(index) => {
                if let Some(Popup::Transform(draft)) = &mut self.popup {
                    draft.active = Some(index);
                    draft.preview = false;
                }
            }
            Action::TransformPreview | Action::TransformApply => {
                let Some(Popup::Transform(draft)) = &self.popup else {
                    return Err("Open the transform sheet first".into());
                };
                let document = self.document.as_ref().ok_or("Open a project first")?;
                if document.raw() != &draft.expected {
                    return Err("The scene changed; reopen the transform sheet".into());
                }
                let pose = draft.transform()?;
                let preview = document.preview_transform(draft.id, pose)?;
                if action == Action::TransformApply {
                    let id = draft.id;
                    self.document
                        .as_mut()
                        .ok_or("Open a project first")?
                        .set_transform(id, pose)?;
                    self.popup = None;
                    self.changed();
                    self.status = "Complete transform applied as one undo step.".into();
                } else {
                    self.viewport.rebuild(&preview);
                    if let Some(Popup::Transform(preview_draft)) = &mut self.popup {
                        preview_draft.preview = true;
                        preview_draft.active = None;
                    }
                    self.status =
                        "Preview only. Apply commits the entire pose; Escape cancels.".into();
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "This modal consumes only its bounded text, keyboard and click events."
    )]
    pub(super) fn transform_event(&mut self, event: &Event) -> bool {
        let Some(Popup::Transform(draft)) = &mut self.popup else {
            return false;
        };
        let Some(index) = draft.active else {
            return false;
        };
        let Some(value) = draft.fields.get_mut(index) else {
            return false;
        };
        match event {
            Event::TextInput {
                text,
                timestamp: _,
                window_id: _,
            } => {
                if value.len().saturating_add(text.len()) <= 32 {
                    value.push_str(text);
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
            } if keymod.intersects(Mod::LCTRLMOD | Mod::RCTRLMOD | Mod::LGUIMOD | Mod::RGUIMOD) => {
                value.clear();
            }
            Event::KeyDown {
                keycode: Some(Keycode::Backspace),
                keymod: _,
                timestamp: _,
                window_id: _,
                scancode: _,
                repeat: _,
                which: _,
                raw: _,
            } => {
                let _removed = value.pop();
            }
            Event::KeyDown {
                keycode: Some(Keycode::Return | Keycode::Tab),
                keymod: _,
                timestamp: _,
                window_id: _,
                scancode: _,
                repeat: _,
                which: _,
                raw: _,
            } => draft.active = None,
            Event::KeyDown {
                keycode: Some(Keycode::Escape),
                keymod: _,
                timestamp: _,
                window_id: _,
                scancode: _,
                repeat: _,
                which: _,
                raw: _,
            } => {
                self.popup = None;
                self.changed();
            }
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
