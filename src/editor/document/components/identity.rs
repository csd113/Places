//! Identity changes update typed source references but never rewrite opaque Luau.

use super::super::{Document, ObjectId, objects};
use serde_json::Value;

impl Document {
    pub(crate) fn rename_instance(&mut self, id: ObjectId, identity: &str) -> Result<(), String> {
        if !matches!(
            id,
            ObjectId::Prop(_)
                | ObjectId::Door(_)
                | ObjectId::Light(_)
                | ObjectId::Audio(_)
                | ObjectId::Trigger(_)
                | ObjectId::SpawnPoint(_)
        ) {
            return Err("Only gameplay instances have runtime identities".into());
        }
        if identity.len() > 96
            || identity.trim() != identity
            || !crate::engine::assets::is_valid_asset_id(identity)
        {
            return Err("Identity requires 1..96 letters, digits, colon, dot, underscore or hyphen, with no leading colon".into());
        }
        let mut raw = self.raw().clone();
        objects::pin_prop_ids(&mut raw, self.level())?;
        super::super::gameplay::pin_gameplay_ids(&mut raw, self.level())?;
        let object = objects::object_mut(&mut raw, id)?;
        let previous = object
            .get("id")
            .and_then(Value::as_str)
            .ok_or("Instance has no runtime identity")?
            .to_string();
        if previous == identity {
            return Ok(());
        }
        // Scripts can construct ids dynamically. Guessing replacements would silently
        // change gameplay, so establish stable ids before linking a module.
        if !self.script_paths().is_empty() {
            return Err("This map has Luau modules. Keep its stable identity, or update scripted references explicitly before renaming.".into());
        }
        drop(object.insert("id".into(), identity.into()));
        for collection in [
            "props",
            "doors",
            "ceiling_lights",
            "sound_emitters",
            "volumes",
            "timers",
            "sequences",
            "spawn_templates",
        ] {
            for record in raw
                .get_mut(collection)
                .and_then(Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                for component in record
                    .get_mut("components")
                    .and_then(Value::as_array_mut)
                    .into_iter()
                    .flatten()
                {
                    if component.get("component").and_then(Value::as_str) == Some("carry")
                        && component.get("item").and_then(Value::as_str) == Some(&previous)
                        && let Some(contract) = component.as_object_mut()
                    {
                        drop(contract.insert("item".into(), identity.into()));
                    }
                }
                for key in ["bindings", "steps", "filter"] {
                    if let Some(value) = record.get_mut(key) {
                        replace_references(value, &previous, identity);
                    }
                }
            }
        }
        self.accept_edit(raw)
    }
    pub(super) fn rename_state(
        &mut self,
        id: ObjectId,
        index: usize,
        name: &str,
    ) -> Result<(), String> {
        let mut raw = self.raw().clone();
        let owner = super::super::objects::object_mut(&mut raw, id)?
            .get("id")
            .and_then(Value::as_str)
            .ok_or("State owner needs a stable identity")?
            .to_string();
        let state = super::values::record_mut(
            super::super::objects::object_mut(&mut raw, id)?,
            super::Target::State(index),
        )?;
        let previous = state
            .get("name")
            .and_then(Value::as_str)
            .ok_or("State has no name")?
            .to_string();
        if previous == name {
            return Ok(());
        }
        if !self.script_paths().is_empty() {
            return Err("This map has Luau modules. Keep declared state names stable, or update scripted references explicitly before renaming.".into());
        }
        drop(state.insert("name".into(), name.into()));
        rename_state_references(&mut raw, &owner, &previous, name, false);
        self.accept_edit(raw)
    }
}

fn replace_references(root: &mut Value, previous: &str, identity: &str) {
    match root {
        Value::Object(object) => {
            for (key, value) in object {
                if matches!(key.as_str(), "target" | "instance" | "point")
                    && value.as_str() == Some(previous)
                {
                    *value = identity.into();
                } else {
                    replace_references(value, previous, identity);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                replace_references(value, previous, identity);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn rename_state_references(value: &mut Value, owner: &str, previous: &str, name: &str, owns: bool) {
    match value {
        Value::Object(record) => {
            let current_owner = record
                .get("id")
                .and_then(Value::as_str)
                .map_or(owns, |id| id == owner);
            let targeted = record
                .get("target")
                .and_then(Value::as_str)
                .map_or(current_owner, |target| target == owner);
            if targeted
                && (record.get("action").and_then(Value::as_str) == Some("set_state")
                    || record.get("check").and_then(Value::as_str) == Some("state"))
                && record.get("name").and_then(Value::as_str) == Some(previous)
            {
                drop(record.insert("name".into(), name.into()));
            }
            for (key, child) in record {
                if matches!(
                    key.as_str(),
                    "props"
                        | "doors"
                        | "ceiling_lights"
                        | "sound_emitters"
                        | "volumes"
                        | "timers"
                        | "sequences"
                        | "bindings"
                        | "actions"
                        | "steps"
                        | "when"
                ) {
                    rename_state_references(child, owner, previous, name, current_owner);
                }
            }
        }
        Value::Array(entries) => {
            for entry in entries {
                rename_state_references(entry, owner, previous, name, owns);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}
