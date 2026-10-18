//! Multi-event editing preserves sibling bindings and validates capabilities.

use super::super::objects::object_mut;
use super::super::{Document, ObjectId, objects};
use crate::engine::level::{EventKindName, MAX_BINDINGS_PER_ENTITY};
use serde_json::{Value, json};

impl Document {
    pub(crate) fn binding_count(&self, id: ObjectId) -> usize {
        self.component_object(id)
            .and_then(|object| object.get("bindings"))
            .and_then(Value::as_array)
            .map_or(0, Vec::len)
    }

    /// Insert the action in the selected existing event or materialize a new event.
    /// Missing interactability on a prop is made explicit in the same undo step.
    pub(crate) fn add_binding_action(
        &mut self,
        id: ObjectId,
        index: usize,
        event: EventKindName,
        action: Value,
    ) -> Result<(), String> {
        if !matches!(
            id,
            ObjectId::Prop(_)
                | ObjectId::Door(_)
                | ObjectId::Trigger(_)
                | ObjectId::Light(_)
                | ObjectId::Audio(_)
        ) {
            return Err("Select a prop, door, trigger, fixture or sound".into());
        }
        let mut raw = self.raw().clone();
        objects::pin_prop_ids(&mut raw, self.level())?;
        super::super::gameplay::pin_gameplay_ids(&mut raw, self.level())?;
        if action.get("action").and_then(Value::as_str) == Some("change_level") {
            let target = action
                .get("level")
                .and_then(Value::as_str)
                .ok_or("Missing level")?;
            let _target = self.manifest().level(Some(target))?;
            let transitions = raw
                .as_object_mut()
                .ok_or("Invalid source")?
                .entry("transitions")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or("Invalid transitions")?;
            if !transitions
                .iter()
                .any(|entry| entry.as_str() == Some(target))
            {
                transitions.push(target.into());
            }
        }
        let object = object_mut(&mut raw, id)?;
        if event == EventKindName::Interact && matches!(id, ObjectId::Prop(_)) {
            let components = super::component_array(object)?;
            if !components
                .iter()
                .any(|entry| entry.get("component").and_then(Value::as_str) == Some("interactable"))
            {
                components
                    .push(json!({"component":"interactable","enabled":true,"prompt":"Interact"}));
            }
        }
        let draft = object.get("editor_trigger_policy").cloned();
        let bindings = object
            .entry("bindings")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("Invalid bindings")?;
        if index > bindings.len() {
            return Err("Event selection expired".into());
        }
        if index == bindings.len() {
            if bindings.len() >= MAX_BINDINGS_PER_ENTITY {
                return Err("At most 16 events per object".into());
            }
            let mut binding = if index == 0 {
                draft.unwrap_or_else(|| json!({"on":event.name()}))
            } else {
                json!({"on":event.name()})
            };
            let record = binding.as_object_mut().ok_or("Invalid pending event")?;
            drop(record.insert("on".into(), event.name().into()));
            drop(record.insert("actions".into(), json!([action])));
            bindings.push(binding);
        } else {
            bindings
                .get_mut(index)
                .and_then(Value::as_object_mut)
                .ok_or("Invalid binding")?
                .entry("actions")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or("Invalid actions")?
                .push(action);
        }
        drop(object.remove("editor_trigger_policy"));
        self.accept_edit(raw)
    }

    pub(crate) fn remove_binding(&mut self, id: ObjectId, index: usize) -> Result<(), String> {
        let mut raw = self.raw().clone();
        let bindings = object_mut(&mut raw, id)?
            .get_mut("bindings")
            .and_then(Value::as_array_mut)
            .ok_or("No events")?;
        if index >= bindings.len() {
            return Err("Event selection expired".into());
        }
        drop(bindings.remove(index));
        self.accept_edit(raw)
    }

    pub(crate) fn remove_binding_action(
        &mut self,
        id: ObjectId,
        binding: usize,
        index: usize,
    ) -> Result<(), String> {
        let mut raw = self.raw().clone();
        let bindings = object_mut(&mut raw, id)?
            .get_mut("bindings")
            .and_then(Value::as_array_mut)
            .ok_or("No events")?;
        let actions = bindings
            .get_mut(binding)
            .and_then(|entry| entry.get_mut("actions"))
            .and_then(Value::as_array_mut)
            .ok_or("No actions")?;
        if index >= actions.len() {
            return Err("Action selection expired".into());
        }
        drop(actions.remove(index));
        if actions.is_empty() {
            drop(bindings.remove(binding));
        }
        self.accept_edit(raw)
    }

    pub(crate) fn condition_choices(&self) -> Vec<Value> {
        let mut result = Vec::new();
        for collection in ["props", "doors"] {
            for object in self
                .raw()
                .get(collection)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                let Some(target) = object.get("id").and_then(Value::as_str) else {
                    continue;
                };
                for component in object
                    .get("components")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if component.get("component").and_then(Value::as_str) == Some("state") {
                        result.push(json!({"check":"state","target":target,"name":component.get("name"),"equals":component.get("value")}));
                    }
                }
            }
        }
        result
    }

    pub(crate) fn add_binding_condition(
        &mut self,
        id: ObjectId,
        binding: usize,
        condition: Value,
    ) -> Result<(), String> {
        let mut raw = self.raw().clone();
        let record =
            super::values::record_mut(object_mut(&mut raw, id)?, super::Target::Binding(binding))?;
        let conditions = record
            .entry("when")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("Invalid conditions")?;
        if conditions.len() >= 8 {
            return Err("The editor supports eight conditions per event".into());
        }
        conditions.push(condition);
        self.accept_edit(raw)
    }

    pub(crate) fn remove_binding_condition(
        &mut self,
        id: ObjectId,
        binding: usize,
        index: usize,
    ) -> Result<(), String> {
        let mut raw = self.raw().clone();
        let record =
            super::values::record_mut(object_mut(&mut raw, id)?, super::Target::Binding(binding))?;
        let conditions = record
            .get_mut("when")
            .and_then(Value::as_array_mut)
            .ok_or("No conditions")?;
        if index >= conditions.len() {
            return Err("Condition selection expired".into());
        }
        drop(conditions.remove(index));
        self.accept_edit(raw)
    }
}
