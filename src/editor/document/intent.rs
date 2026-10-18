//! Placement-specific review decisions belong to the raw authoring source.
//! Runtime validation remains independent: these records never exempt malformed
//! scene data. Hashes bind one measured issue to stable instances and geometry.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use super::{Document, ObjectId};
use crate::editor::safety::{SafetyCategory, SafetyFinding, SafetyReport, SafetySeverity};
use crate::engine::level::LevelDef;
pub(super) use crate::engine::project::metadata::validate_raw;
use crate::engine::project::metadata::{
    Ignore, MAX_IGNORES, PlacementIntent, instance_records, read_intent,
};
use crate::engine::spatial::Aabb;

#[cfg(test)]
#[path = "../tests/placement_intent.rs"]
mod tests;

const FIELD: &str = "editor_placement_intent";
const INSTANCE: &str = "editor_instance_id";
static ID_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct PlacementIgnoreState {
    pub key: String,
    pub active: bool,
}

impl Document {
    /// Records this warning and its current placement/geometry context in one
    /// undo step. Other warnings and other instances remain independently visible.
    ///
    /// # Errors
    /// Rejects non-placement findings, stale objects and malformed intent data.
    pub fn ignore_safety_finding(&mut self, finding: &SafetyFinding) -> Result<(), String> {
        self.accept_edit(ignore_in_raw(self.raw(), finding)?)
    }

    /// Clears exactly one reviewed ignore, including an obsolete context, in
    /// one undo step. The next review can offer its correction again.
    ///
    /// # Errors
    /// Rejects missing records or malformed source metadata without editing.
    pub fn clear_safety_ignore(&mut self, key: &str) -> Result<(), String> {
        let mut edited = self.raw().clone();
        let mut intent = read_intent(&edited)?;
        let before = intent.ignores.len();
        intent.ignores.retain(|entry| entry.key != key);
        if intent.ignores.len() == before {
            return Err("This saved ignore no longer exists; run Safety again".into());
        }
        write_intent(&mut edited, &intent)?;
        self.accept_edit(edited)
    }
}

/// Makes an independently validated candidate for one placement warning.
///
/// # Errors
/// Rejects stale instances, unsupported findings and malformed metadata.
pub fn ignore_in_raw(raw: &Value, finding: &SafetyFinding) -> Result<Value, String> {
    if !can_ignore(finding) {
        return Err("Only a specific placement warning can be ignored".into());
    }
    let mut edited = raw.clone();
    validate_raw(&edited)?;
    for object in &finding.objects {
        pin_identity(&mut edited, *object)?;
    }
    let instances = finding_instances(&edited, finding).ok_or("Finding objects changed")?;
    let issue = issue_signature(finding, &instances)?;
    let mut intent = read_intent(&edited)?;
    if intent.ignores.len() >= MAX_IGNORES {
        return Err(
            "Review and clear saved placement ignores before adding more (limit 256)".into(),
        );
    }
    let context = context_signature(&edited, &instances, finding.bounds)?
        .ok_or("Finding placement no longer exists")?;
    let key = digest(&serde_json::json!([finding.check, issue, context]));
    intent.ignores.retain(|entry| entry.key != key);
    intent.ignores.push(Ignore {
        key,
        check: finding.check.into(),
        issue,
        context,
        instances,
        min: finding.bounds.min,
        max: finding.bounds.max,
        label: finding.title.chars().take(240).collect(),
    });
    write_intent(&mut edited, &intent)?;
    Ok(edited)
}

#[must_use]
pub fn can_ignore(finding: &SafetyFinding) -> bool {
    finding.severity == SafetySeverity::Warning
        && finding.category != SafetyCategory::Review
        && !finding.objects.is_empty()
        && finding.placement_ignore.is_none()
}

/// Validates authoring metadata independently of the runtime typed view.
/// Duplicate IDs are refused rather than allowing one placement to inherit
/// another placement's decision. Missing instances make an ignore stale.
///
/// Final gate shared by individual and batch automatic corrections. A protected
/// object's support/context is protected too: unrelated corrections that would
/// change it require the author to review and clear that decision explicitly.
/// Manual authoring remains possible and naturally invalidates the context.
///
/// # Errors
/// Rejects malformed metadata or a change to an actively protected context.
pub fn ensure_automatic_edit_allowed(before: &Value, after: &Value) -> Result<(), String> {
    validate_raw(before)?;
    validate_raw(after)?;
    if before.get(FIELD) != after.get(FIELD) {
        return Err("Automatic corrections may not change saved placement intent. Review and clear the ignore explicitly first".into());
    }
    for entry in read_intent(before)?.ignores {
        let bounds = Aabb {
            min: entry.min,
            max: entry.max,
        };
        if context_signature(before, &entry.instances, bounds)?.as_deref() == Some(&entry.context)
            && context_signature(after, &entry.instances, bounds)?.as_deref()
                != Some(&entry.context)
        {
            return Err(format!(
                "Automatic correction would change protected placement/context: {}. Review and clear its saved ignore first; manual editing remains available.",
                entry.label
            ));
        }
    }
    Ok(())
}

pub fn annotate_report(raw: &Value, report: &mut SafetyReport) {
    if validate_raw(raw).is_err() {
        return;
    }
    let Ok(intent) = read_intent(raw) else { return };
    for entry in intent.ignores {
        let bounds = Aabb {
            min: entry.min,
            max: entry.max,
        };
        let active = context_signature(raw, &entry.instances, bounds)
            .ok()
            .flatten()
            .is_some_and(|context| context == entry.context);
        let matched = active
            .then(|| {
                report.findings.iter_mut().find(|finding| {
                    finding.check == entry.check
                        && finding_instances(raw, finding).is_some_and(|instances| {
                            issue_signature(finding, &instances)
                                .is_ok_and(|issue| issue == entry.issue)
                        })
                })
            })
            .flatten();
        if let Some(finding) = matched {
            finding.placement_ignore = Some(PlacementIgnoreState {
                key: entry.key,
                active: true,
            });
        } else {
            // Unmatched/stale decisions stay discoverable and independently
            // clearable; they never conceal a new or changed finding.
            report.findings.push(SafetyFinding {
                check: "saved-placement-ignore", category: SafetyCategory::Review,
                severity: SafetySeverity::Info,
                title: format!("Saved ignore: {}", entry.label),
                risk: if active {
                    "The saved warning is absent from this review. It suppresses no other warning; its saved placement/context remains protected until explicitly cleared."
                } else {
                    "Placement or affected geometry changed. This saved ignore is inactive; current warnings and corrections are evaluated again. Clear it, or undo the change to restore its original context."
                }.into(),
                position: crate::editor::safety::bounds_center(bounds), bounds,
                objects: resolve_objects(raw, &entry.instances), surfaces: Vec::new(), fix: None,
                placement_ignore: Some(PlacementIgnoreState { key: entry.key, active }),
            });
        }
    }
}

fn write_intent(raw: &mut Value, intent: &PlacementIntent) -> Result<(), String> {
    let object = raw.as_object_mut().ok_or("Scene must be an object")?;
    if intent.ignores.is_empty() {
        drop(object.remove(FIELD));
    } else {
        drop(object.insert(
            FIELD.into(),
            serde_json::to_value(intent).map_err(|error| error.to_string())?,
        ));
    }
    Ok(())
}

fn pin_identity(raw: &mut Value, id: ObjectId) -> Result<(), String> {
    let (collection, index) = id.collection();
    let record = raw
        .get_mut(collection)
        .and_then(Value::as_array_mut)
        .and_then(|records| records.get_mut(index))
        .and_then(Value::as_object_mut)
        .ok_or("Finding placement no longer exists")?;
    if !record.contains_key(INSTANCE) {
        let key = digest(&serde_json::json!([
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
                .to_string(),
            std::process::id(),
            ID_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ]));
        drop(record.insert(INSTANCE.into(), Value::String(key)));
    }
    Ok(())
}

fn finding_instances(raw: &Value, finding: &SafetyFinding) -> Option<Vec<String>> {
    let mut instances = Vec::new();
    for object in &finding.objects {
        let (collection, index) = object.collection();
        let id = raw
            .get(collection)?
            .as_array()?
            .get(index)?
            .get(INSTANCE)?
            .as_str()?;
        instances.push(id.into());
    }
    instances.sort();
    instances.dedup();
    (!instances.is_empty()).then_some(instances)
}

fn issue_signature(finding: &SafetyFinding, instances: &[String]) -> Result<String, String> {
    let value = serde_json::to_value((
        finding.check,
        instances,
        finding.position,
        finding.bounds.min,
        finding.bounds.max,
        &finding.surfaces,
    ))
    .map_err(|error| error.to_string())?;
    Ok(digest(&value))
}

fn context_signature(
    raw: &Value,
    instances: &[String],
    bounds: Aabb,
) -> Result<Option<String>, String> {
    let records = instance_records(raw)?;
    let Some(placements) = instances
        .iter()
        .map(|id| records.get(id).map(|record| stripped(record)))
        .collect::<Option<Vec<_>>>()
    else {
        return Ok(None);
    };
    let level: LevelDef = serde_json::from_value(raw.clone()).map_err(|error| error.to_string())?;
    let mut geometry = Vec::new();
    for object in super::objects::enumerate(&level) {
        let Some(nearby) = crate::editor::safety::reference_bounds(&level, &[object.id]) else {
            continue;
        };
        // X/Z support matters even when a changed floor/ceiling no longer lies
        // at the original warning's height. A quarter-metre halo captures contact.
        // A legacy prop without captured catalog dimensions has an unknown
        // envelope here. Retain it conservatively rather than missing contact.
        let unknown_prop_envelope = matches!(object.id, ObjectId::Prop(index)
            if level.props.get(index).is_some_and(|prop| prop.size.is_none()));
        if unknown_prop_envelope
            || (0..3).filter(|axis| *axis != 1).all(|axis| {
                nearby.min.get(axis).copied().unwrap_or(f32::INFINITY)
                    <= bounds.max.get(axis).copied().unwrap_or(0.0) + 0.25
                    && nearby.max.get(axis).copied().unwrap_or(f32::NEG_INFINITY)
                        >= bounds.min.get(axis).copied().unwrap_or(0.0) - 0.25
            })
        {
            let (collection, index) = object.id.collection();
            if let Some(record) = raw
                .get(collection)
                .and_then(Value::as_array)
                .and_then(|values| values.get(index))
            {
                // Architecture precedence is authored order (overlapping
                // floors/ramps can change support). Prop array order alone is
                // not placement identity, so only architecture binds its index.
                let precedence = if matches!(object.id, ObjectId::Prop(_)) {
                    None
                } else {
                    Some(index)
                };
                geometry.push(serde_json::json!([
                    collection,
                    precedence,
                    stripped(record)
                ]));
            }
        }
    }
    // These unsupported editor geometry collections remain conservative context
    // dependencies. They are reviewed only on demand, never on the frame loop.
    for collection in [
        "stairs",
        "floor_patches",
        "half_walls",
        "columns",
        "arc_walls",
        "void_walls",
        "pillars",
        "archways",
        "guardrails",
        "thresholds",
        "baseboards",
        "ceiling_lights",
        "doors",
    ] {
        if let Some(values) = raw.get(collection).and_then(Value::as_array) {
            let normalized: Vec<_> = values.iter().map(stripped).collect();
            geometry.push(serde_json::json!([collection, normalized]));
        }
    }
    geometry.sort_by_cached_key(Value::to_string);
    Ok(Some(digest(&serde_json::json!([placements, geometry]))))
}

fn stripped(record: &Value) -> Value {
    let mut result = record.clone();
    if let Some(object) = result.as_object_mut() {
        drop(object.remove(INSTANCE));
    }
    result
}

fn resolve_objects(raw: &Value, instances: &[String]) -> Vec<ObjectId> {
    let Ok(level) = serde_json::from_value::<LevelDef>(raw.clone()) else {
        return Vec::new();
    };
    super::objects::enumerate(&level)
        .into_iter()
        .filter_map(|object| {
            let (collection, index) = object.id.collection();
            raw.get(collection)
                .and_then(Value::as_array)
                .and_then(|values| values.get(index))
                .and_then(|record| record.get(INSTANCE))
                .and_then(Value::as_str)
                .filter(|id| instances.iter().any(|instance| instance == id))
                .map(|_| object.id)
        })
        .collect()
}

fn digest(value: &Value) -> String {
    crate::engine::package::hash::sha256_hex(value.to_string().as_bytes())
}
