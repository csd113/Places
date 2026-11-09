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
        let raw = self.cinematic_edit(expected, original, sequence, owner, destination, once)?;
        self.accept_edit(raw)
    }

    pub(crate) fn preview_cinematic(
        &self,
        expected: &Value,
        original: Option<&str>,
        sequence: &Value,
        owner: ObjectId,
        destination: Option<&str>,
        once: bool,
    ) -> Result<(), String> {
        let raw = self.cinematic_edit(expected, original, sequence, owner, destination, once)?;
        self.validated_preview(raw).map(|_| ())
    }

    /// The ordinary controller owns either player entry or explicit interaction.
    pub(crate) fn cinematic_controller(&self, owner: ObjectId) -> Option<(String, &'static str)> {
        if let ObjectId::Trigger(index) = owner {
            return self
                .level()
                .volume_instance_ids()
                .get(index)
                .cloned()
                .map(|id| (id, "enter_volume"));
        }
        if let ObjectId::Prop(index) = owner
            && self
                .component_object(owner)?
                .get("components")?
                .as_array()?
                .iter()
                .any(|c| {
                    value_at(c, "/component") == "interactable"
                        && value_at(c, "/enabled").as_bool().unwrap_or(true)
                })
        {
            return self
                .level()
                .prop_instance_ids()
                .get(index)
                .cloned()
                .map(|id| (id, "interact"));
        }
        None
    }

    fn cinematic_edit(
        &self,
        expected: &Value,
        original: Option<&str>,
        sequence: &Value,
        owner: ObjectId,
        destination: Option<&str>,
        once: bool,
    ) -> Result<Value, String> {
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
        let (owner_id, event) = self
            .cinematic_controller(owner)
            .ok_or("Choose a player trigger or an enabled interactable prop as controller")?;
        let previous = value_at(expected, "/sequences")
            .as_array()
            .and_then(|rows| {
                rows.iter()
                    .find(|row| value_at(row, "/id").as_str() == original)
            })
            .unwrap_or(&Value::Null);
        if original.is_some() && previous.is_null() {
            return Err("Sequence no longer exists".into());
        }
        let managed = value_at(previous, "/editor_cinematic/owner").as_str();
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
        let previous_destination = value_at(previous, "/editor_cinematic/destination").as_str();
        let prop = event == "interact";
        wire_bindings(
            bindings,
            id,
            managed == Some(owner_id.as_str()),
            previous_destination,
            destination,
            once,
            prop.then_some((
                owner_id.as_str(),
                value_at(previous, "/editor_cinematic/once")
                    .as_bool()
                    .unwrap_or(true),
            )),
        )?;
        if prop {
            wire_prop_state(&mut raw, owner, id, managed == Some(owner_id.as_str()))?;
        }
        Ok(raw)
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

fn wire_prop_state(
    raw: &mut Value,
    owner: ObjectId,
    id: &str,
    managed: bool,
) -> Result<(), String> {
    let object = objects::object_mut(raw, owner)?;
    let components = object
        .entry("components")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or("Invalid components")?;
    let name = format!("cinematic_{id}_done");
    if let Some(state) = components.iter().find(|v| {
        value_at(v, "/component") == "state" && value_at(v, "/name").as_str() == Some(name.as_str())
    }) {
        if !managed || value_at(state, "/value") != false {
            return Err("Cinematic completion state is already authored; nothing changed".into());
        }
    } else if managed {
        return Err(
            "Cinematic completion state was removed; preserve intent through Events".into(),
        );
    } else {
        components.push(json!({"component":"state","name":name,"value":false}));
    }
    Ok(())
}

fn start_binding(id: &str, prop: Option<&str>, once: bool) -> Result<Value, String> {
    let start_id = format!("cinematic_{id}_start");
    let done = format!("cinematic_{id}_done");
    let mut start = json!({"id":start_id,"on":if prop.is_some() {"interact"} else {"enter_volume"},"once":once && prop.is_none(),"actions":[{"action":"start_sequence","sequence":id}]});
    if let Some(owner) = prop {
        let when = if once {
            json!([{"check":"state","target":owner,"name":done,"equals":false}])
        } else {
            json!([])
        };
        drop(
            start
                .as_object_mut()
                .ok_or("Invalid helper")?
                .insert("when".into(), when),
        );
    }
    Ok(start)
}

fn wire_bindings(
    bindings: &mut Vec<Value>,
    id: &str,
    managed: bool,
    previous_destination: Option<&str>,
    destination: Option<&str>,
    once: bool,
    prop: Option<(&str, bool)>,
) -> Result<(), String> {
    let start_id = format!("cinematic_{id}_start");
    let end_id = format!("cinematic_{id}_complete");
    let done = format!("cinematic_{id}_done");
    let complete_actions = |target: Option<&str>| {
        let mut actions = Vec::new();
        if prop.is_some() {
            actions.push(json!({"action":"set_state","name":done,"value":true}));
        }
        if let Some(level) = target {
            actions.push(json!({"action":"change_level","level":level}));
        }
        actions
    };
    let start = start_binding(id, prop.map(|(owner, _)| owner), once)?;
    for (binding_id, binding) in [
        (&start_id, start),
        (
            &end_id,
            json!({"id":end_id,"on":"sequence_complete","key":id,"actions":complete_actions(destination)}),
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
                json!(complete_actions(previous_destination))
            };
            if bindings.get(index).and_then(|v| v.get("actions")) != Some(&expected_actions) {
                return Err("Helper events were manually edited; preserve them through Events instead of overwriting them here".into());
            }
            let previous_record = bindings
                .get(index)
                .and_then(Value::as_object)
                .ok_or("Invalid helper event")?;
            if binding_id == &start_id
                && let Some((owner, previous_once)) = prop
            {
                let expected_when = if previous_once {
                    json!([{"check":"state","target":owner,"name":done,"equals":false}])
                } else {
                    json!([])
                };
                if previous_record.get("when") != Some(&expected_when)
                    || previous_record.get("once") != Some(&json!(false))
                {
                    return Err(
                        "Helper interaction policy was manually edited; preserve it in Events"
                            .into(),
                    );
                }
            }
            if previous_record.get("on") != binding.get("on")
                || (binding_id == &end_id && previous_record.get("key") != binding.get("key"))
            {
                return Err("Helper event identity was manually changed; edit it in Events".into());
            }
            if binding_id == &end_id && destination.is_none() && prop.is_none() {
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
        } else if binding_id != &end_id || destination.is_some() || prop.is_some() {
            bindings.push(binding);
        }
    }
    Ok(())
}
