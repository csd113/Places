//! Atomic cinematic resource and exact helper-event wiring edits.
use super::{Document, ObjectId, objects};
use serde_json::{Value, json};

impl Document {
    pub(crate) fn apply_cinematic(
        &mut self,
        expected: &Value,
        original: Option<&str>,
        sequence: &Value,
        owner: ObjectId,
        destination: Option<&str>,
        once: bool,
    ) -> Result<(), String> {
        if self.raw() != expected {
            return Err(
                "Scene changed. Select the saved sequence again (or New); nothing was applied."
                    .into(),
            );
        }
        let id = value_at(sequence, "/id")
            .as_str()
            .ok_or("Missing cinematic id")?;
        if original.is_some_and(|old| old != id) {
            return Err("Existing cinematic IDs are stable and cannot be renamed".into());
        }
        if !matches!(owner, ObjectId::Trigger(_)) {
            return Err("Choose an ordinary player trigger as controller".into());
        }
        let owner_id = self
            .level()
            .volume_instance_ids()
            .get(owner.collection().1)
            .cloned()
            .ok_or("Controller no longer exists")?;
        let managed = value_at(expected, "/sequences")
            .as_array()
            .and_then(|rows| {
                rows.iter()
                    .find(|row| value_at(row, "/id").as_str() == original)
            })
            .and_then(|row| value_at(row, "/editor_cinematic/owner").as_str());
        if managed.is_some_and(|old| old != owner_id) {
            return Err("Existing cinematic controller cannot be changed implicitly".into());
        }
        let mut raw = expected.clone();
        self.cinematic_transition(&mut raw, destination)?;
        let sequences = raw
            .as_object_mut()
            .ok_or("Invalid level")?
            .entry("sequences")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("Invalid sequences")?;
        let existing = sequences
            .iter()
            .position(|v| value_at(v, "/id").as_str() == Some(id));
        if original.is_none() && existing.is_some() {
            return Err("Sequence ID already exists".into());
        }
        let mut record = sequence.clone();
        let metadata = record
            .as_object_mut()
            .ok_or("Invalid sequence")?
            .entry("editor_cinematic")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or("Invalid cinematic metadata")?;
        drop(metadata.insert("owner".into(), json!(owner_id)));
        drop(metadata.insert("destination".into(), json!(destination)));
        drop(metadata.insert("once".into(), json!(once)));
        if let Some(index) = existing {
            *sequences.get_mut(index).ok_or("Sequence expired")? = record;
        } else {
            sequences.push(record);
        }
        let object = objects::object_mut(&mut raw, owner)?;
        if object.get("editor_hidden").and_then(Value::as_bool) == Some(true) {
            return Err("Show the controller in Layers before editing".into());
        }
        let bindings = object
            .entry("bindings")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("Invalid bindings")?;
        let previous_destination = value_at(expected, "/sequences")
            .as_array()
            .and_then(|rows| {
                rows.iter()
                    .find(|row| value_at(row, "/id").as_str() == original)
            })
            .and_then(|row| value_at(row, "/editor_cinematic/destination").as_str());
        wire_bindings(
            bindings,
            id,
            managed == Some(owner_id.as_str()),
            previous_destination,
            destination,
            once,
        )?;
        self.accept_edit(raw)
    }

    fn cinematic_transition(
        &self,
        raw: &mut Value,
        destination: Option<&str>,
    ) -> Result<(), String> {
        if let Some(target) = destination {
            let _level = self.manifest().level(Some(target))?;
            let transitions = raw
                .as_object_mut()
                .ok_or("Invalid level")?
                .entry("transitions")
                .or_insert_with(|| json!([]))
                .as_array_mut()
                .ok_or("Invalid transitions")?;
            if !transitions.iter().any(|v| v.as_str() == Some(target)) {
                transitions.push(json!(target));
            }
        }
        Ok(())
    }

    pub(crate) fn remove_cinematic(&mut self, expected: &Value, id: &str) -> Result<(), String> {
        if self.raw() != expected {
            return Err("Scene changed; select the saved sequence again".into());
        }
        // Removal is deliberately conservative: all references must be removed explicitly first.
        if references(expected, id) {
            return Err("Remove this sequence's start/completion event references in Events before removing it".into());
        }
        let mut raw = expected.clone();
        value_at_mut(&mut raw, "/sequences")?
            .as_array_mut()
            .ok_or("No sequences")?
            .retain(|v| value_at(v, "/id").as_str() != Some(id));
        self.accept_edit(raw)
    }
}

/// Optional source reads never index or mutate missing values.
pub fn value_at<'a>(value: &'a Value, path: &str) -> &'a Value {
    value.pointer(path).unwrap_or(&Value::Null)
}
pub fn value_at_mut<'a>(value: &'a mut Value, path: &str) -> Result<&'a mut Value, String> {
    value
        .pointer_mut(path)
        .ok_or_else(|| format!("Cinematic field no longer exists: {path}"))
}

fn references(value: &Value, id: &str) -> bool {
    match value {
        Value::Object(map) => {
            (map.get("sequence").and_then(Value::as_str) == Some(id))
                || (map.get("on").and_then(Value::as_str) == Some("sequence_complete")
                    && map.get("key").and_then(Value::as_str) == Some(id))
                || map.values().any(|v| references(v, id))
        }
        Value::Array(values) => values.iter().any(|v| references(v, id)),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
    }
}

fn wire_bindings(
    bindings: &mut Vec<Value>,
    id: &str,
    managed: bool,
    previous_destination: Option<&str>,
    destination: Option<&str>,
    once: bool,
) -> Result<(), String> {
    let start_id = format!("cinematic_{id}_start");
    let end_id = format!("cinematic_{id}_complete");
    for (binding_id, binding) in [
        (
            &start_id,
            json!({"id":start_id,"on":"enter_volume","once":once,"actions":[{"action":"start_sequence","sequence":id}]}),
        ),
        (
            &end_id,
            json!({"id":end_id,"on":"sequence_complete","key":id,"actions":destination.map_or_else(Vec::new, |level| vec![json!({"action":"change_level","level":level})])}),
        ),
    ] {
        if let Some(index) = bindings
            .iter()
            .position(|v| value_at(v, "/id").as_str() == Some(binding_id))
        {
            if !managed {
                return Err(
                    "Helper binding ID already exists and is not owned by this cinematic sheet"
                        .into(),
                );
            }
            let expected_actions = if binding_id == &start_id {
                json!([{"action":"start_sequence","sequence":id}])
            } else {
                json!(previous_destination.map_or_else(Vec::new, |level| vec![
                    json!({"action":"change_level","level":level})
                ]))
            };
            if bindings.get(index).and_then(|v| v.get("actions")) != Some(&expected_actions) {
                return Err("Helper events were manually edited; preserve them through Events instead of overwriting them here".into());
            }
            let previous_record = bindings
                .get(index)
                .and_then(Value::as_object)
                .ok_or("Invalid helper event")?;
            if previous_record.get("on") != binding.get("on")
                || (binding_id == &end_id && previous_record.get("key") != binding.get("key"))
            {
                return Err("Helper event identity was manually changed; edit it in Events".into());
            }
            if binding_id == &end_id && destination.is_none() {
                if previous_record
                    .keys()
                    .any(|key| !matches!(key.as_str(), "id" | "on" | "key" | "actions"))
                {
                    return Err("Completion event has additional authoring; remove it explicitly in Events to preserve your intent".into());
                }
                drop(bindings.remove(index));
                continue;
            }
            // Only the helper's authored fields change; extension fields survive.
            let previous = bindings
                .get_mut(index)
                .ok_or("Binding expired")?
                .as_object_mut()
                .ok_or("Invalid helper binding")?;
            for (key, value) in binding.as_object().ok_or("Invalid helper")? {
                drop(previous.insert(key.clone(), value.clone()));
            }
        } else if binding_id != &end_id || destination.is_some() {
            bindings.push(binding);
        }
    }
    Ok(())
}
