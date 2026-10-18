//! Shared validation of preserved authoring-only metadata. Runtime ignores never bypass validation.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
const FIELD: &str = "editor_placement_intent";
const INSTANCE: &str = "editor_instance_id";
pub const MAX_IGNORES: usize = 256;
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlacementIntent {
    pub version: u32,
    pub ignores: Vec<Ignore>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Ignore {
    pub key: String,
    pub check: String,
    pub issue: String,
    pub context: String,
    pub instances: Vec<String>,
    pub min: [f32; 3],
    pub max: [f32; 3],
    pub label: String,
}
pub fn validate_raw(raw: &Value) -> Result<(), String> {
    let _instances = instance_records(raw)?;
    let intent = read_intent(raw)?;
    if intent.version != 1 || intent.ignores.len() > MAX_IGNORES {
        return Err("Unsupported or oversized editor placement intent metadata".into());
    }
    let mut keys = BTreeSet::new();
    for entry in intent.ignores {
        let unique: BTreeSet<_> = entry.instances.iter().collect();
        if !hash_valid(&entry.key)
            || !hash_valid(&entry.issue)
            || !hash_valid(&entry.context)
            || !keys.insert(entry.key)
            || entry.instances.is_empty()
            || entry.instances.len() > 32
            || unique.len() != entry.instances.len()
            || entry.instances.iter().any(|id| !identity_valid(id))
            || entry.check.is_empty()
            || entry.check.len() > 96
            || !entry
                .check
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || entry.label.len() > 1024
            || entry.label.chars().any(char::is_control)
            || !entry
                .min
                .iter()
                .chain(&entry.max)
                .all(|value| value.is_finite())
            || entry.min.iter().zip(entry.max).any(|(min, max)| *min > max)
        {
            return Err(
                "Malformed editor placement ignore; scene validation remains enforced".into(),
            );
        }
    }
    Ok(())
}
pub fn read_intent(raw: &Value) -> Result<PlacementIntent, String> {
    raw.get(FIELD).map_or_else(
        || {
            Ok(PlacementIntent {
                version: 1,
                ignores: Vec::new(),
            })
        },
        |value| {
            serde_json::from_value(value.clone())
                .map_err(|error| format!("Invalid placement intent metadata: {error}"))
        },
    )
}
fn hash_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn identity_valid(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

pub fn instance_records(raw: &Value) -> Result<BTreeMap<String, &Value>, String> {
    let mut instances = BTreeMap::new();
    for values in raw
        .as_object()
        .ok_or("Scene must be an object")?
        .values()
        .filter_map(Value::as_array)
    {
        for record in values {
            if let Some(value) = record.get(INSTANCE) {
                let id = value
                    .as_str()
                    .filter(|id| identity_valid(id))
                    .ok_or("Invalid editor placement identity")?;
                if instances.insert(id.into(), record).is_some() {
                    return Err(format!(
                        "Duplicate editor placement identity '{id}'; no ignore was applied"
                    ));
                }
            }
        }
    }
    Ok(instances)
}
pub fn validate_drafts(raw: &Value) -> Result<(), String> {
    if let Some(volumes) = raw.get("volumes").and_then(Value::as_array) {
        for volume in volumes {
            if let Some(policy) = volume.get("editor_trigger_policy") {
                let binding: crate::engine::level::EventBindingDef =
                    serde_json::from_value(policy.clone())
                        .map_err(|error| format!("Invalid pending trigger policy: {error}"))?;
                if !binding.cooldown_seconds.is_finite()
                    || binding.cooldown_seconds < 0.0_f32
                    || !matches!(
                        binding.on,
                        crate::engine::level::EventKindName::EnterVolume
                            | crate::engine::level::EventKindName::ExitVolume
                    )
                    || !binding.actions.is_empty()
                {
                    return Err("Pending trigger policy requires enter/exit, a nonnegative finite cooldown and no actions".into());
                }
            }
        }
    }
    Ok(())
}
