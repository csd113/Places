//! Circuit authoring reuses stable fixture groups and native power switches.
//! Each operation validates one raw candidate and enters one scene undo step.
use super::{Document, ObjectId, flicker, objects, persistence};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

impl Document {
    pub(crate) fn assign_circuit(
        &mut self,
        fixtures: &[usize],
        switch: usize,
    ) -> Result<(), String> {
        if fixtures.is_empty() {
            return Err("Select at least one fixture".into());
        }
        let prop = self
            .level()
            .props
            .get(switch)
            .ok_or("Choose a switch prop")?;
        if prop.physics.is_some() || prop.float.is_some() || prop.animated_door.is_some() {
            return Err("Choose a fixed prop as the switch".into());
        }
        let mut raw = self.raw().clone();
        let switch_id = self
            .level()
            .prop_instance_ids()
            .get(switch)
            .cloned()
            .ok_or("Missing switch identity")?;
        drop(
            objects::object_mut(&mut raw, ObjectId::Prop(switch))?
                .insert("id".into(), json!(switch_id)),
        );

        let group = prop.light_switch.clone().unwrap_or_else(|| {
            let identity = self
                .level()
                .prop_instance_ids()
                .get(switch)
                .cloned()
                .unwrap_or_default();
            crate::engine::package::hash::digest_hex(&Sha256::digest(identity.as_bytes()))
        });
        let affected = self.circuit_flicker(fixtures, Some(&group))?;
        let ids = self.level().light_instance_ids();
        for (index, record) in objects::array_mut(&mut raw, "ceiling_lights")?
            .iter_mut()
            .enumerate()
        {
            if (fixtures.contains(&index)
                || record.get("switch_group").and_then(Value::as_str) == Some(group.as_str()))
                && let Some(identity) = ids.get(index)
            {
                drop(
                    record
                        .as_object_mut()
                        .ok_or("Invalid fixture")?
                        .insert("id".into(), json!(identity)),
                );
            }
        }
        for index in fixtures {
            let light = objects::object_mut(&mut raw, ObjectId::Light(*index))?;
            drop(light.insert("switch_group".into(), json!(group)));
        }
        for record in objects::array_mut(&mut raw, "ceiling_lights")? {
            if record.get("switch_group").and_then(Value::as_str) == Some(group.as_str()) {
                let light = record.as_object_mut().ok_or("Invalid fixture")?;
                drop(light.insert("enabled".into(), json!(true)));
                drop(light.insert("switchable".into(), json!(true)));
                drop(light.insert("hold_in_static".into(), json!(true)));
                drop(light.insert("static_switch".into(), json!("disabled")));
            }
        }
        let target_ids: std::collections::BTreeSet<_> =
            objects::array_mut(&mut raw, "ceiling_lights")?
                .iter()
                .filter(|light| {
                    light.get("switch_group").and_then(Value::as_str) == Some(group.as_str())
                })
                .filter_map(|light| light.get("id").and_then(Value::as_str).map(str::to_owned))
                .collect();
        let object = objects::object_mut(&mut raw, ObjectId::Prop(switch))?;
        adopt_switch_actions(object, &target_ids);
        drop(object.insert("light_switch".into(), json!(group)));
        enable_switch_interaction(object)?;
        drop(
            raw.as_object_mut()
                .ok_or("Invalid level")?
                .insert("requires_full_lighting".into(), json!(true)),
        );
        cleanup_switches(&mut raw)?;
        if &raw == self.raw() {
            return Ok(());
        }
        if let Some(old) = affected.clone() {
            self.release_flicker(&mut raw, Some(old), false)?;
        }
        self.finish_circuit_edit(raw, affected)
    }

    pub(crate) fn unlink_circuit(&mut self, fixtures: &[usize]) -> Result<(), String> {
        let mut raw = self.raw().clone();
        let affected = self.circuit_flicker(fixtures, None)?;
        for index in fixtures {
            drop(objects::object_mut(&mut raw, ObjectId::Light(*index))?.remove("switch_group"));
        }
        cleanup_switches(&mut raw)?;
        if &raw == self.raw() {
            return Ok(());
        }
        if let Some(old) = affected.clone() {
            self.release_flicker(&mut raw, Some(old), false)?;
        }
        self.finish_circuit_edit(raw, affected)
    }

    pub(super) fn delete_with_circuits(&mut self, id: ObjectId) -> Result<(), String> {
        let affected = if let ObjectId::Light(_) = id {
            self.flicker_owner(id)?.filter(|(_, meta)| {
                meta.pointer("/settings/enabled").and_then(Value::as_bool) == Some(true)
            })
        } else {
            None
        };
        let mut raw = self.raw().clone();
        if let Some(old) = affected.clone() {
            self.release_flicker(&mut raw, Some(old), false)?;
        }
        objects::delete(&mut raw, self.level(), id)?;
        cleanup_switches(&mut raw)?;
        self.finish_circuit_edit(raw, affected)
    }

    fn circuit_flicker(
        &self,
        fixtures: &[usize],
        group: Option<&str>,
    ) -> Result<Option<(usize, Value)>, String> {
        let mut owners = std::collections::BTreeMap::new();
        for (index, light) in self.level().ceiling_lights.iter().enumerate() {
            if (fixtures.contains(&index)
                || group.is_some_and(|g| light.switch_group.as_deref() == Some(g)))
                && let Some((owner, meta)) = self.flicker_owner(ObjectId::Light(index))?
                && meta.pointer("/settings/enabled").and_then(Value::as_bool) == Some(true)
            {
                drop(owners.insert(owner, meta));
            }
        }
        if owners.len() > 1 {
            return Err("These fixtures have different flicker controllers. Disable one before joining them.".into());
        }
        Ok(owners.into_iter().next())
    }

    fn finish_circuit_edit(
        &mut self,
        raw: Value,
        affected: Option<(usize, Value)>,
    ) -> Result<(), String> {
        let Some((old_owner, meta)) = affected else {
            return self.accept_edit(raw);
        };
        let mut settings: flicker::Flicker = serde_json::from_value(
            meta.get("settings")
                .cloned()
                .ok_or("Missing flicker settings")?,
        )
        .map_err(|e| e.to_string())?;
        let (level, _) = persistence::validate_raw(&raw, self.level_id())?;
        let ids = level.light_instance_ids();
        let old_group = self
            .level()
            .ceiling_lights
            .get(old_owner)
            .and_then(|l| l.switch_group.as_deref());
        let retained = level
            .ceiling_lights
            .iter()
            .enumerate()
            .find(|(_, l)| old_group.is_some() && l.switch_group.as_deref() == old_group)
            .map(|(i, _)| i)
            .or_else(|| {
                ids.iter().position(|id| {
                    meta.get("members")
                        .and_then(Value::as_array)
                        .is_some_and(|members| {
                            members.iter().any(|m| m.as_str() == Some(id.as_str()))
                        })
                })
            });
        let Some(owner) = retained else {
            return self.accept_edit(raw);
        };
        let light = level
            .ceiling_lights
            .get(owner)
            .ok_or("Missing flicker fixture")?;
        settings.circuit = light.switch_group.is_some();
        let members: Vec<_> = level
            .ceiling_lights
            .iter()
            .enumerate()
            .filter(|(i, l)| {
                if settings.circuit {
                    l.switch_group == light.switch_group
                } else {
                    *i == owner
                }
            })
            .filter_map(|(i, _)| ids.get(i))
            .collect();
        let identity = ids.get(owner).ok_or("Missing fixture identity")?;
        let source = flicker::flicker_source(identity, &settings, light.enabled);
        let hash = crate::engine::package::hash::digest_hex(&Sha256::digest(source.as_bytes()));
        let _created = self.create_script_edit(&raw, &source, |candidate, path| {
            drop(objects::object_mut(candidate, ObjectId::Light(owner))?.insert("editor_flicker".into(),
                json!({"settings":settings,"members":members,"authored_on":light.enabled,"script":path,"generated_hash":hash})));
            Ok(())
        })?;
        Ok(())
    }
}

fn cleanup_switches(raw: &mut Value) -> Result<(), String> {
    let groups: std::collections::BTreeSet<_> = raw
        .get("ceiling_lights")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|light| {
            light
                .get("switch_group")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect();
    for prop in objects::array_mut(raw, "props")? {
        if prop
            .get("light_switch")
            .and_then(Value::as_str)
            .is_some_and(|group| !groups.contains(group))
        {
            drop(
                prop.as_object_mut()
                    .ok_or("Invalid prop")?
                    .remove("light_switch"),
            );
        }
    }
    Ok(())
}

fn adopt_switch_actions(
    object: &mut serde_json::Map<String, Value>,
    target_ids: &std::collections::BTreeSet<String>,
) {
    // Replace duplicate direct control of these same members when an
    // existing event-authored switch is adopted. Sibling actions survive.
    if let Some(bindings) = object.get_mut("bindings").and_then(Value::as_array_mut) {
        for binding in bindings
            .iter_mut()
            .filter(|binding| binding.get("on").and_then(Value::as_str) == Some("interact"))
        {
            if let Some(actions) = binding.get_mut("actions").and_then(Value::as_array_mut) {
                actions.retain(|action| {
                    !(matches!(
                        action.get("action").and_then(Value::as_str),
                        Some("toggle" | "set_light")
                    ) && action
                        .get("target")
                        .and_then(Value::as_str)
                        .is_some_and(|target| target_ids.contains(target)))
                });
            }
        }
        bindings.retain(|binding| {
            binding
                .get("actions")
                .and_then(Value::as_array)
                .is_none_or(|actions| !actions.is_empty())
        });
    }
}

fn enable_switch_interaction(object: &mut serde_json::Map<String, Value>) -> Result<(), String> {
    let components = object
        .entry("components")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or("Invalid switch components")?;
    if !components
        .iter()
        .any(|c| c.get("component").and_then(Value::as_str) == Some("interactable"))
    {
        components
            .push(json!({"component":"interactable","enabled":true,"prompt":"Lights on / off"}));
    }
    for component in components {
        if component.get("component").and_then(Value::as_str) == Some("interactable") {
            drop(
                component
                    .as_object_mut()
                    .ok_or("Invalid interaction")?
                    .insert("enabled".into(), json!(true)),
            );
        }
    }
    Ok(())
}
