//! Capability form state, typed entry, and selected-event routing.

use sdl3::event::Event;
use sdl3::keyboard::Keycode;

use super::App;
use super::actions::Popup;
use super::document::ObjectId;
use super::document::components::{Field, Kind};
use super::ui::Action;
use crate::engine::level::EventKindName;

pub(super) struct Selection {
    object: Option<ObjectId>,
    pub binding: usize,
    pub event: EventKindName,
    pub page: usize,
}
impl Default for Selection {
    fn default() -> Self {
        Self {
            object: None,
            binding: 0,
            event: EventKindName::Interact,
            page: 0,
        }
    }
}
impl App {
    pub(super) fn selected_binding(&self) -> usize {
        if self.component_selection.object == self.selected {
            self.component_selection.binding
        } else {
            0
        }
    }

    pub(super) fn selected_event(&self) -> EventKindName {
        if self.component_selection.object == self.selected {
            self.component_selection.event
        } else if matches!(self.selected, Some(ObjectId::Trigger(_))) {
            EventKindName::EnterVolume
        } else {
            EventKindName::Interact
        }
    }

    fn select_component_binding(&mut self) {
        let count = self
            .document
            .as_ref()
            .zip(self.selected)
            .map_or(0, |(doc, id)| doc.binding_count(id));
        if self.component_selection.object != self.selected {
            self.component_selection = Selection {
                object: self.selected,
                event: if matches!(self.selected, Some(ObjectId::Trigger(_))) {
                    EventKindName::EnterVolume
                } else {
                    EventKindName::Interact
                },
                ..Selection::default()
            };
        }
        self.component_selection.binding = self.component_selection.binding.min(count);
        let on = self
            .document
            .as_ref()
            .zip(self.selected)
            .and_then(|(doc, id)| doc.component_object(id))
            .and_then(|object| object.get("bindings"))
            .and_then(|bindings| bindings.get(self.component_selection.binding))
            .and_then(|binding| binding.get("on"))
            .cloned();
        self.component_selection.event = on
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or(self.component_selection.event);
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only capability form actions belong to this contextual dispatcher."
    )]
    pub(super) fn component_action(&mut self, action: Action) -> Result<bool, String> {
        if let Action::Components(page) = action {
            self.select_component_binding();
            self.component_selection.page = page;
            self.popup = Some(Popup::Components { page });
            return Ok(true);
        }
        let id = self.selected.unwrap_or(ObjectId::Prop(0));
        if self.component_state_action(action, id)? {
            return Ok(true);
        }
        match action {
            Action::ComponentField(field) => self.open_component_field(id, field)?,
            Action::ComponentApply => self.apply_component_text()?,
            Action::BindingSelect(forward) => {
                self.select_component_binding();
                let count = self
                    .document
                    .as_ref()
                    .ok_or("Open a project")?
                    .binding_count(id);
                self.component_selection.binding = if forward {
                    self.component_selection
                        .binding
                        .saturating_add(1)
                        .min(count.saturating_sub(1))
                } else {
                    self.component_selection.binding.saturating_sub(1)
                };
                self.select_component_binding();
            }
            Action::BindingNew => {
                self.select_component_binding();
                self.component_selection.binding = self
                    .document
                    .as_ref()
                    .ok_or("Open a project")?
                    .binding_count(id);
                self.component_selection.event = if matches!(id, ObjectId::Trigger(_)) {
                    EventKindName::EnterVolume
                } else {
                    EventKindName::Interact
                };
            }
            Action::BindingRemove => {
                let binding = self.selected_binding();
                self.document
                    .as_mut()
                    .ok_or("Open a project")?
                    .remove_binding(id, binding)?;
                self.component_selection.binding = binding.saturating_sub(1);
                self.changed();
            }
            Action::ComponentAdd(kind) => {
                self.document
                    .as_mut()
                    .ok_or("Open a project")?
                    .add_component(id, kind)?;
                self.changed();
            }
            Action::ComponentRemove(kind) => {
                self.document
                    .as_mut()
                    .ok_or("Open a project")?
                    .remove_component(id, kind)?;
                self.changed();
            }
            Action::CatalogFixedShape => {
                self.document
                    .as_mut()
                    .ok_or("Open a project")?
                    .use_catalog_fixed_shape(id, &self.catalog)?;
                self.changed();
            }
            Action::AttachedLightAdd => {
                self.document
                    .as_mut()
                    .ok_or("Open a project")?
                    .add_attached_light(id)?;
                self.changed();
            }
            Action::AttachedLightRemove(index) => {
                self.document
                    .as_mut()
                    .ok_or("Open a project")?
                    .remove_attached_light(id, index)?;
                self.changed();
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only state and condition form commands belong here."
    )]
    fn component_state_action(&mut self, action: Action, id: ObjectId) -> Result<bool, String> {
        match action {
            Action::StateAdd => {
                self.popup = Some(Popup::ComponentText {
                    field: Field::new(
                        super::document::components::Target::NewState,
                        "name",
                        "New Boolean state name",
                        Kind::Text,
                        "\"\"",
                    ),
                    value: String::new(),
                    page: 1,
                });
            }
            Action::StateRemove(index) => {
                self.document
                    .as_mut()
                    .ok_or("Open a project")?
                    .remove_state(id, index)?;
                self.changed();
            }
            Action::ConditionChoose(index) => {
                let condition = self
                    .document
                    .as_ref()
                    .ok_or("Open a project")?
                    .condition_choices()
                    .get(index)
                    .cloned()
                    .ok_or("Condition selection expired")?;
                let binding = self.selected_binding();
                self.document
                    .as_mut()
                    .ok_or("Open a project")?
                    .add_binding_condition(id, binding, condition)?;
                self.changed();
                self.component_selection.page = 3;
                self.popup = Some(Popup::Components { page: 3 });
            }
            Action::ConditionRemove(index) => {
                let binding = self.selected_binding();
                self.document
                    .as_mut()
                    .ok_or("Open a project")?
                    .remove_binding_condition(id, binding, index)?;
                self.changed();
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn open_component_field(&mut self, id: ObjectId, field: Field) -> Result<(), String> {
        let current = if field.target
            == super::document::components::Target::Binding(self.selected_binding())
            && field.key == "on"
            && self
                .document
                .as_ref()
                .is_some_and(|doc| self.selected_binding() >= doc.binding_count(id))
        {
            self.selected_event().name().into()
        } else {
            self.document
                .as_ref()
                .ok_or("Open a project")?
                .component_value(id, field)
        };
        let next_value = match field.kind {
            Kind::Bool => Some((!current.as_bool().unwrap_or(false)).to_string()),
            Kind::Choice(choices) => {
                let index = choices
                    .iter()
                    .position(|choice| Some(*choice) == current.as_str())
                    .map_or(0, |index| {
                        index
                            .saturating_add(1)
                            .checked_rem(choices.len().max(1))
                            .unwrap_or(0)
                    });
                Some(choices.get(index).ok_or("Empty choices")?.to_string())
            }
            Kind::Text | Kind::Number | Kind::Vector | Kind::State => None,
        };
        if let Some(next) = next_value {
            if field.target == super::document::components::Target::Binding(self.selected_binding())
                && field.key == "on"
                && self
                    .document
                    .as_ref()
                    .is_some_and(|doc| self.selected_binding() >= doc.binding_count(id))
            {
                self.component_selection.event =
                    serde_json::from_value(next.into()).map_err(|error| error.to_string())?;
            } else {
                self.document
                    .as_mut()
                    .ok_or("Open a project")?
                    .set_component_field(id, field, &next)?;
                self.changed();
            }
        } else {
            let value = if field.kind == Kind::Text {
                current.as_str().unwrap_or_default().to_string()
            } else {
                current.to_string()
            };
            self.popup = Some(Popup::ComponentText {
                field,
                value,
                page: self.component_selection.page,
            });
        }
        Ok(())
    }

    fn apply_component_text(&mut self) -> Result<(), String> {
        let Some(Popup::ComponentText { field, value, page }) = &self.popup else {
            return Ok(());
        };
        let (edited_field, edited_value, edited_page) = (*field, value.clone(), *page);
        self.document
            .as_mut()
            .ok_or("Open a project")?
            .set_component_field(
                self.selected.unwrap_or(ObjectId::Prop(0)),
                edited_field,
                &edited_value,
            )?;
        self.changed();
        self.popup = Some(Popup::Components { page: edited_page });
        Ok(())
    }

    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "The modal consumes only bounded text entry and lets ordinary pointer events reach its controls."
    )]
    pub(super) fn component_text_event(&mut self, event: &Event) -> bool {
        let Some(Popup::ComponentText {
            field: _,
            value,
            page,
        }) = &mut self.popup
        else {
            return false;
        };
        match event {
            Event::TextInput {
                text,
                timestamp: _,
                window_id: _,
            } => {
                if value.len().saturating_add(text.len()) <= 256
                    && !text.chars().any(char::is_control)
                {
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
                keycode: Some(Keycode::A),
                keymod,
                timestamp: _,
                window_id: _,
                scancode: _,
                repeat: _,
                which: _,
                raw: _,
            } if super::input::command(*keymod) => value.clear(),
            Event::KeyDown {
                keycode: Some(Keycode::Return),
                repeat: false,
                timestamp: _,
                window_id: _,
                scancode: _,
                keymod: _,
                which: _,
                raw: _,
            } => {
                let result = self.apply_component_text();
                self.report(result);
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
            } => {
                self.popup = Some(Popup::Components { page: *page });
            }
            _ => return false,
        }
        true
    }
}
