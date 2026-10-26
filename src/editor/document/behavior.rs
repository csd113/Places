//! Visual behavior edits target the shared AI contracts. A dedicated Luau copy
//! can own execution; switching back is explicit and keeps every source file.
use super::{Document, ObjectId, objects, persistence};
use crate::engine::ai::{
    AiDef,
    authoring::{SenseRule, TaskRequest},
};
use serde_json::{Value, json};
use std::fmt::Write as _;

impl Document {
    pub(crate) fn behavior_value(&self, id: ObjectId) -> Result<Value, String> {
        let object = self.component_object(id).ok_or("Select a placed NPC")?;
        if !matches!(id, ObjectId::Prop(_)) {
            return Err("Select a placed NPC; spawn templates remain in Configure".into());
        }
        let saved = object
            .get("editor_behavior")
            .and_then(|meta| meta.get("visual"));
        let ai = object
            .get("components")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|component| component.get("component").and_then(Value::as_str) == Some("ai"));
        let mut plan = if self.behavior_script(id).is_some() {
            saved.or(ai)
        } else {
            ai.or(saved)
        }
        .cloned()
        .unwrap_or_else(|| json!({"component":"ai","behavior":"idler"}));
        let fields = plan.as_object_mut().ok_or("Invalid AI component")?;
        for (key, default) in [
            ("tasks", json!([])),
            ("senses", json!([])),
            ("receive_alerts", json!(false)),
        ] {
            let _value = fields.entry(key).or_insert(default);
        }
        Ok(plan)
    }
    pub(crate) fn behavior_script(&self, id: ObjectId) -> Option<&str> {
        self.component_object(id)?
            .get("editor_behavior")?
            .get("script")?
            .as_str()
    }
    pub(crate) fn behavior_candidate(&self, id: ObjectId, plan: &Value) -> Result<Value, String> {
        validate_plan(self, plan)?;
        let mut raw = self.raw().clone();
        objects::pin_prop_ids(&mut raw, self.level())?;
        let old_script = self.behavior_script(id).map(str::to_owned);
        if let Some(path) = old_script {
            unlink_owned_script(&mut raw, &path)?;
        }
        let object = objects::object_mut(&mut raw, id)?;
        let components = object
            .entry("components")
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or("Invalid components")?;
        let has_nav = components
            .iter()
            .any(|c| c.get("component").and_then(Value::as_str) == Some("nav_agent"));
        if !has_nav {
            return Err("NPC needs a navigation body. Choose Configure > Behavior first to fit its body to the actual model.".into());
        }
        components.retain(|c| c.get("component").and_then(Value::as_str) != Some("ai"));
        components.push(plan.clone());
        let previous = object
            .get("editor_behavior")
            .cloned()
            .unwrap_or_else(|| json!({}));
        let mut metadata = previous
            .as_object()
            .cloned()
            .ok_or("Invalid behavior metadata")?;
        drop(metadata.insert("visual".into(), plan.clone()));
        drop(metadata.insert("owner".into(), json!("visual")));
        drop(metadata.remove("script"));
        drop(object.insert("editor_behavior".into(), Value::Object(metadata)));
        let _validated = persistence::validate_raw(&raw, self.level_id())?;
        Ok(raw)
    }
    pub(crate) fn apply_behavior(&mut self, id: ObjectId, plan: &Value) -> Result<(), String> {
        let raw = self.behavior_candidate(id, plan)?;
        self.accept_edit(raw)
    }
    pub(crate) fn generate_behavior_script(
        &mut self,
        id: ObjectId,
        plan: &Value,
    ) -> Result<String, String> {
        let mut raw = self.behavior_candidate(id, plan)?;
        let object = objects::object_mut(&mut raw, id)?;
        let identity = object
            .get("id")
            .and_then(Value::as_str)
            .ok_or("NPC needs a stable identity")?
            .to_string();
        let source = behavior_source(&identity, plan)?;
        let components = object
            .get_mut("components")
            .and_then(Value::as_array_mut)
            .ok_or("Invalid components")?;
        let ai = components
            .iter_mut()
            .find(|c| c.get("component").and_then(Value::as_str) == Some("ai"))
            .and_then(Value::as_object_mut)
            .ok_or("Missing AI")?;
        drop(ai.insert("tasks".into(), json!([])));
        drop(ai.insert("senses".into(), json!([])));
        self.create_script_edit(&raw, &source, |candidate, path| {
            let meta = objects::object_mut(candidate, id)?
                .get_mut("editor_behavior")
                .and_then(Value::as_object_mut)
                .ok_or("Missing visual plan")?;
            drop(meta.insert("owner".into(), json!("script")));
            drop(meta.insert("script".into(), json!(path)));
            Ok(())
        })
    }
}
pub(super) fn unlink_owned_script(raw: &mut Value, path: &str) -> Result<(), String> {
    // Do not disable a module somebody has subsequently wired into another event.
    fn linked(value: &Value, path: &str) -> bool {
        match value {
            Value::Object(map) => {
                map.get("action").and_then(Value::as_str) == Some("run_script")
                    && map.get("script").and_then(Value::as_str) == Some(path)
                    || map.values().any(|v| linked(v, path))
            }
            Value::Array(values) => values.iter().any(|v| linked(v, path)),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => false,
        }
    }
    if linked(raw, path) {
        return Err("This generated module now has event links. Remove those links before changing its execution owner; source is preserved.".into());
    }
    if let Some(paths) = raw.get_mut("scripts").and_then(Value::as_array_mut) {
        paths.retain(|v| v.as_str() != Some(path));
    }
    Ok(())
}
fn validate_plan(doc: &Document, plan: &Value) -> Result<(), String> {
    let ai: AiDef =
        serde_json::from_value(plan.clone()).map_err(|e| format!("Behavior fields: {e}"))?;
    if ai.tasks.len() > 16 || ai.senses.len() > 16 {
        return Err("At most 16 startup tasks and 16 perception rules per NPC".into());
    }
    for (index, task) in ai.tasks.iter().enumerate() {
        if !task.is_valid() {
            return Err(format!(
                "Task {}: invalid range, empty routine, or depth/node budget exceeded",
                index.saturating_add(1)
            ));
        }
        if ai
            .tasks
            .iter()
            .take(index)
            .any(|other| other.key == task.key)
        {
            return Err(format!(
                "Task {}: duplicate key {}",
                index.saturating_add(1),
                task.key
            ));
        }
    }
    for (index, rule) in ai.senses.iter().enumerate() {
        if !rule.is_valid() {
            return Err(format!(
                "Perception {}: invalid filter or response parameters",
                index.saturating_add(1)
            ));
        }
    }
    let mut ids = doc.level().prop_instance_ids();
    ids.extend(doc.level().door_instance_ids());
    ids.extend(doc.level().light_instance_ids());
    ids.push("player".into());
    validate_references(plan, "NPC", &ids)
}
fn validate_references(value: &Value, path: &str, ids: &[String]) -> Result<(), String> {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if (key == "target"
                    || key == "value" && map.get("match").and_then(Value::as_str) == Some("id"))
                    && let Some(id) = child.as_str()
                    && !ids.iter().any(|known| known == id)
                {
                    return Err(format!("{path}/{key}: missing target '{id}'"));
                }
                if key == "objects"
                    && let Some(objects) = child.as_array()
                {
                    for object in objects {
                        if object
                            .as_str()
                            .is_none_or(|id| !ids.iter().any(|known| known == id))
                        {
                            return Err(format!("{path}/objects: unknown object"));
                        }
                    }
                }
                validate_references(child, &format!("{path}/{key}"), ids)?;
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                validate_references(child, &format!("{path}/{index}"), ids)?;
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
    Ok(())
}

pub fn behavior_source(id: &str, plan: &Value) -> Result<String, String> {
    let tasks: Vec<TaskRequest> =
        serde_json::from_value(plan.get("tasks").cloned().unwrap_or_else(|| json!([])))
            .map_err(|e| e.to_string())?;
    let senses: Vec<SenseRule> =
        serde_json::from_value(plan.get("senses").cloned().unwrap_or_else(|| json!([])))
            .map_err(|e| e.to_string())?;
    let target = luau(&json!(id), 0);
    let mut source = format!(
        "--!strict\n-- Shared AI: runtime owns navigation, perception, reservations and physical contact.\n-- Edit freely. Regenerate creates a NEW module; it never replaces this source.\n-- Routines run in order and stop on failure. Higher priorities win.\nlocal actor = {target}\nreturn {{\n    start = function(api, event)\n        if not api.exists(actor) then\n            api.notice(\"Behavior actor no longer exists: \" .. actor)\n            return\n        end\n        api.ai_senses(actor, {})\n",
        luau(&serde_json::to_value(senses).map_err(|e| e.to_string())?, 2)
    );
    for task in tasks {
        writeln!(
            source,
            "        api.ai_task(actor, {}, {})",
            luau(&json!(task.key), 0),
            luau(&serde_json::to_value(task).map_err(|e| e.to_string())?, 2)
        )
        .map_err(|e| e.to_string())?;
    }
    source.push_str("    end,\n    cancel = function(api, event)\n        if not api.exists(actor) then return end\n");
    for task in plan
        .get("tasks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(key) = task.get("key") {
            writeln!(source, "        api.ai_task(actor, {}, nil)", luau(key, 0))
                .map_err(|e| e.to_string())?;
        }
    }
    source.push_str("        api.ai_senses(actor, {})\n    end,\n}\n");
    Ok(source)
}
pub fn luau(value: &Value, depth: usize) -> String {
    let indent = "    ".repeat(depth.min(12));
    let child = "    ".repeat(depth.saturating_add(1).min(12));
    match value {
        Value::Null => "nil".into(),
        Value::Bool(_) | Value::Number(_) => value.to_string(),
        Value::String(s) => {
            let mut escaped = String::from("\"");
            for c in s.chars() {
                match c {
                    '\\' => escaped.push_str("\\\\"),
                    '"' => escaped.push_str("\\\""),
                    '\n' => escaped.push_str("\\n"),
                    '\r' => escaped.push_str("\\r"),
                    '\t' => escaped.push_str("\\t"),
                    _ => escaped.push(c),
                }
            }
            escaped.push('"');
            escaped
        }
        Value::Array(values) => format!(
            "{{\n{}\n{indent}}}",
            values
                .iter()
                .map(|v| format!("{child}{},", luau(v, depth.saturating_add(1))))
                .collect::<Vec<_>>()
                .join("\n")
        ),
        Value::Object(values) => format!(
            "{{\n{}\n{indent}}}",
            values
                .iter()
                .map(|(key, v)| format!(
                    "{child}[{}] = {},",
                    luau(&json!(key), 0),
                    luau(v, depth.saturating_add(1))
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    }
}
