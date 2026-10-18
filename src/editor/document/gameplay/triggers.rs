//! Empty triggers retain validated editor policy until their first action.

use super::super::objects::object_mut;
use super::super::{Document, ObjectId, objects};
use super::{GameplaySetting, binding_mut};
use serde_json::{Value, json};

impl Document {
    /// Replaces only the trigger occupant selector; actions and bounds survive.
    ///
    /// # Errors
    /// Rejects malformed selectors and missing referenced instances.
    pub fn set_trigger_filter(&mut self, id: ObjectId, filter: Value) -> Result<(), String> {
        if !matches!(id, ObjectId::Trigger(_)) {
            return Err("Select a trigger".into());
        }
        let mut raw = self.raw().clone();
        objects::pin_prop_ids(&mut raw, self.level())?;
        drop(object_mut(&mut raw, id)?.insert("filter".into(), filter));
        self.accept_edit(raw)
    }

    /// Adds one typed action to the trigger's first binding.
    ///
    /// # Errors
    /// Shared validation rejects incompatible targets or action budget overflow.
    pub fn add_trigger_action(&mut self, id: ObjectId, action: Value) -> Result<(), String> {
        if !matches!(id, ObjectId::Trigger(_)) {
            return Err("Select a trigger".into());
        }
        let mut raw = self.raw().clone();
        if let Some(target) = action.get("level").and_then(Value::as_str)
            && action.get("action").and_then(Value::as_str) == Some("change_level")
        {
            let _target = self.manifest().level(Some(target))?;
            let destination = target.to_string();
            let transitions = raw
                .as_object_mut()
                .ok_or("Invalid level")?
                .entry("transitions")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or("Invalid transitions")?;
            if !transitions
                .iter()
                .any(|value| value.as_str() == Some(destination.as_str()))
            {
                transitions.push(json!(destination));
            }
        }
        let binding = binding_mut(&mut raw, id)?;
        binding
            .entry("actions")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("Invalid actions")?
            .push(action);
        drop(object_mut(&mut raw, id)?.remove("editor_trigger_policy"));
        self.accept_edit(raw)
    }

    /// Removes one action without changing other bindings or source extensions.
    ///
    /// # Errors
    /// Rejects stale action indices.
    pub fn remove_trigger_action(&mut self, id: ObjectId, index: usize) -> Result<(), String> {
        let mut raw = self.raw().clone();
        let actions = binding_mut(&mut raw, id)?
            .get_mut("actions")
            .and_then(Value::as_array_mut)
            .ok_or("No actions")?;
        if index >= actions.len() {
            return Err("Action no longer exists".into());
        }
        drop(actions.remove(index));
        if actions.is_empty() {
            let object = object_mut(&mut raw, id)?;
            let bindings = object
                .get_mut("bindings")
                .and_then(Value::as_array_mut)
                .ok_or("Invalid bindings")?;
            let mut policy = bindings
                .remove(0)
                .as_object()
                .cloned()
                .ok_or("Invalid binding policy")?;
            drop(policy.remove("actions"));
            drop(object.insert("editor_trigger_policy".into(), Value::Object(policy)));
        }
        self.accept_edit(raw)
    }
}

pub(super) fn set_trigger_policy(
    raw: &mut Value,
    id: ObjectId,
    setting: GameplaySetting,
    value: Value,
) -> Result<(), String> {
    if !matches!(id, ObjectId::Trigger(_)) {
        return Err("Select a trigger".into());
    }
    let object = object_mut(raw, id)?;
    if let Some(binding) = object
        .get_mut("bindings")
        .and_then(Value::as_array_mut)
        .and_then(|bindings| bindings.first_mut())
        .and_then(Value::as_object_mut)
    {
        drop(binding.insert(setting.key().into(), value));
    } else {
        let policy = object
            .entry("editor_trigger_policy")
            .or_insert_with(|| json!({"on":"enter_volume"}))
            .as_object_mut()
            .ok_or("Invalid editor trigger policy")?;
        drop(policy.insert(setting.key().into(), value));
    }
    Ok(())
}

pub(super) use crate::engine::project::metadata::validate_drafts;
