//! Checkpoints are ordinary selectable trigger volumes and source actions.
use super::super::{Document, ObjectId, ObjectKind};
use serde_json::{Value, json};

impl Document {
    /// # Errors
    /// Shared geometry/action validation rejects invalid placement. One history edit.
    pub fn add_checkpoint(&mut self, position: [f32; 3]) -> Result<ObjectId, String> {
        let mut raw = self.raw().clone();
        let id = super::add(&mut raw, self.level(), ObjectKind::Trigger, position, None)?;
        let object = super::object_mut(&mut raw, id)?;
        let name = object
            .get("id")
            .and_then(Value::as_str)
            .ok_or("Checkpoint ID is missing")?
            .to_string();
        drop(object.insert("bindings".into(),json!([{"on":"enter_volume","once":false,"cooldown_seconds":2.0_f32,"actions":[{"action":"save_checkpoint","name":name}]}])));
        self.accept_edit(raw)?;
        Ok(id)
    }

    pub(crate) fn checkpoint_name(&self, id: ObjectId) -> Option<&str> {
        self.component_object(id)?
            .get("bindings")?
            .as_array()?
            .iter()
            .flat_map(|binding| {
                binding
                    .get("actions")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
            })
            .find(|action| action.get("action").and_then(Value::as_str) == Some("save_checkpoint"))?
            .get("name")?
            .as_str()
    }

    /// # Errors
    /// Requires an existing checkpoint action; preserves sibling actions and policy.
    pub(crate) fn set_checkpoint_name(&mut self, id: ObjectId, name: &str) -> Result<(), String> {
        if name.is_empty() || name.len() > 80 || name.chars().any(char::is_control) {
            return Err("Checkpoint name requires 1..=80 printable bytes".into());
        }
        let mut raw = self.raw().clone();
        let action = super::object_mut(&mut raw, id)?
            .get_mut("bindings")
            .and_then(Value::as_array_mut)
            .ok_or("No checkpoint binding")?
            .iter_mut()
            .flat_map(|binding| {
                binding
                    .get_mut("actions")
                    .and_then(Value::as_array_mut)
                    .into_iter()
                    .flatten()
            })
            .find(|action| action.get("action").and_then(Value::as_str) == Some("save_checkpoint"))
            .and_then(Value::as_object_mut)
            .ok_or("No checkpoint action")?;
        drop(action.insert("name".into(), json!(name)));
        self.accept_edit(raw)
    }
}
