//! Reuse ordinary wall/mount edit guards. Groups merge original record identities
//! before deleting indices, so mixed repairs cannot silently target shifted walls.
use super::{SafetyFinding, SafetyFix};
use crate::editor::document::{ObjectId, wall_edit};
use crate::engine::level::{LevelDef, WallAxis};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(super) enum Correction {
    RemoveWall(usize),
    Ceiling(usize, f32),
    Endpoint(crate::map_compiler::geometry_check::construction::EndpointIssue),
    Join(usize, usize),
    Collision(usize),
    Mount(usize),
    Fields(Vec<(String, Value)>),
}
impl Correction {
    const fn removed(&self) -> Option<usize> {
        match self {
            Self::RemoveWall(i) | Self::Join(_, i) => Some(*i),
            Self::Ceiling(_, _)
            | Self::Endpoint(_)
            | Self::Collision(_)
            | Self::Mount(_)
            | Self::Fields(_) => None,
        }
    }
    pub(super) fn candidate(&self, raw: &Value) -> Result<Value, String> {
        let level: LevelDef = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
        let mut candidate = raw.clone();
        match self {
            Self::RemoveWall(index) => {
                remove(&mut candidate, *index)?;
            }
            Self::Ceiling(room, height) => {
                set(&mut candidate, "rooms", *room, "height", json!(height))?;
                let _adjusted =
                    super::reanchor_ceiling_fixtures(&level, &mut candidate, *room, *height);
            }
            Self::Endpoint(issue) => {
                let wall = level.walls.get(issue.wall).ok_or("Wall changed")?;
                let mut pose = crate::editor::document::objects::transform(
                    &level,
                    ObjectId::Wall(issue.wall),
                )?;
                let (origin, length) = match wall.axis() {
                    WallAxis::X => (wall.x, wall.width),
                    WallAxis::Z => (wall.z, wall.depth),
                };
                let next_origin = if issue.start { issue.target } else { origin };
                let next_length = if issue.start {
                    origin + length - issue.target
                } else {
                    issue.target - origin
                };
                match wall.axis() {
                    WallAxis::X => {
                        pose.position[0] = next_origin;
                        pose.size[0] = next_length;
                    }
                    WallAxis::Z => {
                        pose.position[2] = next_origin;
                        pose.size[2] = next_length;
                    }
                }
                candidate = wall_edit::edit_candidate(raw, &level, issue.wall, pose)?;
            }
            Self::Join(first, second) => {
                let a = level.walls.get(*first).ok_or("First wall changed")?;
                let b = level.walls.get(*second).ok_or("Second wall changed")?;
                let (field, size, low, high) = match a.axis() {
                    WallAxis::X => (
                        "x",
                        "width",
                        a.x.min(b.x),
                        (a.x + a.width).max(b.x + b.width),
                    ),
                    WallAxis::Z => (
                        "z",
                        "depth",
                        a.z.min(b.z),
                        (a.z + a.depth).max(b.z + b.depth),
                    ),
                };
                set(&mut candidate, "walls", *first, field, json!(low))?;
                set(&mut candidate, "walls", *first, size, json!(high - low))?;
                remove(&mut candidate, *second)?;
            }
            Self::Collision(index) => {
                let catalog = crate::engine::assets::AssetCatalog::load_default();
                candidate = crate::editor::document::components::catalog_collision_candidate(
                    raw, &level, *index, &catalog,
                )?;
            }
            Self::Fields(fields) => {
                for (pointer, value) in fields {
                    *candidate
                        .pointer_mut(pointer)
                        .ok_or("Alignment field changed")? = value.clone();
                }
            }
            Self::Mount(index) => {
                candidate = crate::editor::document::mounting::candidate(
                    &level,
                    raw,
                    ObjectId::Prop(*index),
                )?;
            }
        }
        Ok(candidate)
    }
}
fn set(
    raw: &mut Value,
    collection: &str,
    index: usize,
    key: &str,
    value: Value,
) -> Result<(), String> {
    let record = raw
        .get_mut(collection)
        .and_then(|a| a.get_mut(index))
        .and_then(Value::as_object_mut)
        .ok_or("Correction object changed")?;
    drop(record.insert(key.into(), value));
    Ok(())
}
fn remove(raw: &mut Value, index: usize) -> Result<(), String> {
    let walls = raw
        .get_mut("walls")
        .and_then(Value::as_array_mut)
        .ok_or("Walls changed")?;
    if index >= walls.len() {
        return Err("Wall changed".into());
    }
    drop(walls.remove(index));
    Ok(())
}

/// Explicit choices only. Distinct edits of the same original object conflict,
/// including attachments touched by another fix. No partial acceptance.
/// # Errors
/// Refuses stale, ignored, invalid, empty or conflicting correction groups.
pub fn preview_group(raw: &Value, findings: &[&SafetyFinding]) -> Result<Value, String> {
    let mut edits: BTreeMap<(String, usize), Option<Value>> = BTreeMap::new();
    if findings.is_empty() {
        return Err("Choose at least one correction first".into());
    }
    for finding in findings {
        if finding.placement_ignore.is_some() {
            return Err("Clear the saved ignore before choosing its correction".into());
        }
        let fix = finding
            .fix
            .as_ref()
            .ok_or("A chosen finding has no safe correction")?;
        let candidate = fix.preview(raw)?;
        collect_changes(raw, &candidate, fix, &mut edits)?;
    }
    // Unmodified carrier/support records are read dependencies. Another
    // correction changing one invalidates the first proposal's evidence.
    for finding in findings {
        let fix = finding.fix.as_ref().ok_or("Missing chosen correction")?;
        let own = fix.preview(raw)?;
        let mut changes = BTreeMap::new();
        collect_changes(raw, &own, fix, &mut changes)?;
        for object in &finding.objects {
            let (collection, index) = object.collection();
            let key = (collection.to_owned(), index);
            if !changes.contains_key(&key) && edits.contains_key(&key) {
                return Err(format!(
                    "Chosen corrections conflict with a shared carrier at {collection}[{index}]. Review separately; nothing was changed."
                ));
            }
        }
    }
    let mut result = raw.clone();
    for ((collection, index), value) in edits.iter().rev() {
        let records = result
            .get_mut(collection)
            .and_then(Value::as_array_mut)
            .ok_or("Correction collection changed")?;
        if let Some(after) = value {
            *records.get_mut(*index).ok_or("Correction record changed")? = after.clone();
        } else if *index < records.len() {
            drop(records.remove(*index));
        } else {
            return Err("Correction record changed".into());
        }
    }
    super::ensure_locked_correction(raw, &result)?;
    crate::editor::document::intent::ensure_automatic_edit_allowed(raw, &result)?;
    let level: LevelDef = serde_json::from_value(result.clone()).map_err(|e| e.to_string())?;
    crate::engine::loader::validate_level(&level)?;
    Ok(result)
}
fn collect_changes(
    raw: &Value,
    candidate: &Value,
    fix: &SafetyFix,
    edits: &mut BTreeMap<(String, usize), Option<Value>>,
) -> Result<(), String> {
    let object = raw.as_object().ok_or("Expected scene object")?;
    for (collection, before) in object {
        let after = candidate
            .get(collection)
            .ok_or("Correction removed a source collection")?;
        if before == after {
            continue;
        }
        let old = before
            .as_array()
            .ok_or("Only object corrections can be grouped")?;
        let new = after
            .as_array()
            .ok_or("Only object corrections can be grouped")?;
        let removed = if collection == "walls" {
            fix.correction.removed()
        } else {
            None
        };
        if new.len().saturating_add(usize::from(removed.is_some())) != old.len() {
            return Err("Unsupported collection change in correction group".into());
        }
        for (index, record) in old.iter().enumerate() {
            let next = if removed == Some(index) {
                None
            } else {
                new.get(if removed.is_some_and(|deleted| index > deleted) {
                    index.saturating_sub(1)
                } else {
                    index
                })
                .cloned()
            };
            if next.as_ref() == Some(record) {
                continue;
            }
            let key = (collection.clone(), index);
            if let Some(previous) = edits.get(&key)
                && previous != &next
            {
                return Err(format!(
                    "Chosen corrections conflict at {collection}[{index}]. Review separately or remove a choice; nothing was changed."
                ));
            }
            drop(edits.insert(key, next));
        }
    }
    Ok(())
}
