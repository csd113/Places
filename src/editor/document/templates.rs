//! Atomic event-template module creation: validate the scene link before writing.
use super::{Document, ObjectId, gameplay, objects};
use serde_json::{Value, json};
impl Document {
    pub(crate) fn create_event_template(
        &mut self,
        source: &str,
        owner: Option<ObjectId>,
        event: &str,
        once: bool,
    ) -> Result<String, String> {
        let mut raw = self.raw().clone();
        objects::pin_prop_ids(&mut raw, self.level())?;
        gameplay::pin_gameplay_ids(&mut raw, self.level())?;
        if event != "start" {
            let id = owner.ok_or("Select the event owner first, then reopen the template")?;
            if !(event == "interact" && matches!(id, ObjectId::Prop(_) | ObjectId::Door(_))
                || matches!(event, "enter_volume" | "exit_volume")
                    && matches!(id, ObjectId::Trigger(_)))
            {
                return Err(
                    "Interact requires a prop/door; enter/exit requires a trigger volume".into(),
                );
            }
            if event == "interact" {
                let components = objects::object_mut(&mut raw, id)?
                    .entry("components")
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                    .ok_or("Invalid components")?;
                if !components
                    .iter()
                    .any(|v| v.get("component").and_then(Value::as_str) == Some("interactable"))
                {
                    components.push(json!({"component":"interactable","prompt":"Interact"}));
                }
            }
        }
        self.create_script_edit(&raw, source, |candidate, path| {
            if event != "start" {
                let id = owner.ok_or("Select the event owner")?;
                let bindings = objects::object_mut(candidate, id)?
                    .entry("bindings")
                    .or_insert_with(|| json!([]))
                    .as_array_mut()
                    .ok_or("Invalid bindings")?;
                if bindings.len() >= 16 {
                    return Err("Event owner already has 16 bindings".into());
                }
                bindings.push(json!({"on":event,"once":once,"cooldown_seconds":0.5_f32,
                    "actions":[{"action":"run_script","script":path,"handler":"handle_event"}]}));
            }
            Ok(())
        })
    }
}
