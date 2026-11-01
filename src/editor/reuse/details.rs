//! Contextual details are ordinary decals with conservative source ownership.
mod placement;

use super::{Document, ObjectId, Value, objects};
use crate::engine::level::DecalDef;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest as _, Sha256};
use std::collections::{BTreeMap, BTreeSet};

const OWNER: &str = "editor_surface_details";
const MEMBER: &str = "editor_surface_detail";
const LIMIT: usize = 1_024;

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Parameters {
    pub preset: usize,
    pub density: f32,
    pub intensity: f32,
    pub scale: f32,
    pub variation: f32,
    pub seed: u64,
}
impl Parameters {
    fn validate(self) -> Result<(), String> {
        if self.preset > 3
            || !self.density.is_finite()
            || !(0.0..=1.0).contains(&self.density)
            || !self.intensity.is_finite()
            || !(0.0..=1.0).contains(&self.intensity)
            || !self.scale.is_finite()
            || !(0.25..=3.0).contains(&self.scale)
            || !self.variation.is_finite()
            || !(0.0..=1.0).contains(&self.variation)
        {
            return Err(
                "Details need preset 0..3, density/intensity/variation 0..1 and scale 0.25..3"
                    .into(),
            );
        }
        Ok(())
    }
    const fn includes(self, kind: usize) -> bool {
        self.preset == 0 || self.preset == kind
    }
    fn random(self, key: &str) -> f32 {
        let digest = Sha256::digest(format!("{}:{key}", self.seed).as_bytes());
        let first = digest.first().copied().unwrap_or(0);
        let second = digest.get(1).copied().unwrap_or(0);
        f32::from(u16::from_be_bytes([first, second])) / 65_535.0
    }
}

#[derive(Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    version: u8,
    baseline: BTreeMap<String, Value>,
    deleted: BTreeSet<String>,
    parameters: Option<Parameters>,
}
impl Registry {
    fn read(record: &Value) -> Result<Self, String> {
        let Some(value) = record.get(OWNER) else {
            return Ok(Self {
                version: 1,
                ..Self::default()
            });
        };
        let registry: Self = serde_json::from_value(value.clone())
            .map_err(|error| format!("Invalid saved detail baseline: {error}"))?;
        if registry.version != 1
            || registry
                .baseline
                .len()
                .saturating_add(registry.deleted.len())
                > LIMIT
        {
            return Err("Unsupported or oversized saved detail baseline".into());
        }
        Ok(registry)
    }
}

fn record(raw: &Value, id: ObjectId) -> Result<&Value, String> {
    let (key, index) = id.collection();
    raw.get(key)
        .and_then(|entries| entries.get(index))
        .ok_or_else(|| "Detail scope no longer exists".into())
}
fn scope(doc: &Document, selected: ObjectId) -> Result<ObjectId, String> {
    let source = record(doc.raw(), selected)?;
    if let Some(group) = source
        .pointer("/editor_surface_detail/group")
        .and_then(Value::as_str)
    {
        return doc
            .objects()
            .iter()
            .find_map(|object| {
                record(doc.raw(), object.id)
                    .ok()
                    .filter(|entry| {
                        entry.get("editor_instance_id").and_then(Value::as_str) == Some(group)
                    })
                    .map(|_| object.id)
            })
            .ok_or_else(|| {
                "This detail's original scope was removed; edit the decal manually".into()
            });
    }
    Ok(selected)
}
pub(super) fn saved(doc: &Document, selected: ObjectId) -> Option<Parameters> {
    Registry::read(record(doc.raw(), scope(doc, selected).ok()?).ok()?)
        .ok()?
        .parameters
}

fn retain_custom(raw: &mut Value, group: &str, registry: &mut Registry) -> Result<usize, String> {
    let entries = objects::array_mut(raw, "decals")?;
    let mut found = BTreeSet::new();
    let mut replaced = BTreeSet::new();
    for entry in entries.iter() {
        if entry
            .pointer("/editor_surface_detail/group")
            .and_then(Value::as_str)
            != Some(group)
        {
            continue;
        }
        let key = entry
            .pointer("/editor_surface_detail/key")
            .and_then(Value::as_str)
            .ok_or("Invalid detail member identity")?;
        if !found.insert(key.to_owned()) || !registry.baseline.contains_key(key) {
            return Err("Detail identities are duplicated or missing their baseline; preserve these records and repair their source metadata".into());
        }
        if registry.baseline.get(key) == Some(entry)
            && entry.get("editor_locked").and_then(Value::as_bool) != Some(true)
        {
            let _inserted = replaced.insert(key.to_owned());
        }
    }
    for key in registry.baseline.keys() {
        if !found.contains(key) {
            let _inserted = registry.deleted.insert(key.clone());
        }
    }
    entries.retain(|entry| {
        entry
            .pointer("/editor_surface_detail/group")
            .and_then(Value::as_str)
            != Some(group)
            || !entry
                .pointer("/editor_surface_detail/key")
                .and_then(Value::as_str)
                .is_some_and(|key| replaced.contains(key))
    });
    registry
        .baseline
        .retain(|key, _| found.contains(key) && !replaced.contains(key));
    Ok(found.len().saturating_sub(replaced.len()))
}

pub(super) fn candidate(
    doc: &Document,
    selected: ObjectId,
    parameters: Parameters,
    catalog: &crate::engine::assets::AssetCatalog,
) -> Result<(Value, String, Option<ObjectId>), String> {
    parameters.validate()?;
    let owner = scope(doc, selected)?;
    let mut context = doc.level().clone();
    let materials = crate::engine::materials::MaterialTable::logical(&context, catalog, None);
    let _snapped = context.snap_ceiling_decals(&materials);
    let proposals = placement::generate(&context, catalog, owner, parameters)?;
    let mut registry = Registry::read(record(doc.raw(), owner)?)?;
    if proposals.is_empty() && registry.baseline.is_empty() && parameters.density > 0.0 {
        return Err("No supported receivers fit this scope/preset. Select a room, wall, door opening or vent; details never bridge unsupported areas.".into());
    }
    let mut raw = doc.raw().clone();
    crate::editor::document::intent::pin_identity(&mut raw, owner)?;
    let group = record(&raw, owner)?
        .get("editor_instance_id")
        .and_then(Value::as_str)
        .ok_or("Missing detail scope identity")?
        .to_owned();
    let kept = retain_custom(&mut raw, &group, &mut registry)?;
    let owner_after = if matches!(owner, ObjectId::Decal(_)) {
        ObjectId::Decal(
            raw.get("decals")
                .and_then(Value::as_array)
                .and_then(|entries| {
                    entries.iter().position(|entry| {
                        entry.get("editor_instance_id").and_then(Value::as_str)
                            == Some(group.as_str())
                    })
                })
                .ok_or("Detail carrier was removed")?,
        )
    } else {
        owner
    };
    let mut receiving = doc.level().clone();
    receiving.decals =
        serde_json::from_value(raw.get("decals").cloned().unwrap_or_else(|| json!([])))
            .map_err(|error| error.to_string())?;
    let mut added = 0_usize;
    let mut skipped = 0_usize;
    for (key, decal) in proposals {
        if registry.deleted.contains(&key) || registry.baseline.contains_key(&key) {
            continue;
        }
        if !crate::editor::document::signs::clear_of_signs(&receiving, &decal) {
            skipped = skipped.saturating_add(1);
            continue;
        }
        let mut value = serde_json::to_value(&decal).map_err(|error| error.to_string())?;
        let member = value.as_object_mut().ok_or("Invalid decal")?;
        drop(member.insert(MEMBER.into(), json!({"group":group,"key":key})));
        drop(registry.baseline.insert(key, value.clone()));
        objects::array_mut(&mut raw, "decals")?.push(value);
        receiving.decals.push(decal);
        added = added.saturating_add(1);
    }
    if registry
        .baseline
        .len()
        .saturating_add(registry.deleted.len())
        > LIMIT
    {
        return Err("Detail ownership exceeds 1024 slots; choose a smaller scope".into());
    }
    registry.parameters = Some(parameters);
    drop(objects::object_mut(&mut raw, owner_after)?.insert(
        OWNER.into(),
        serde_json::to_value(registry).map_err(|error| error.to_string())?,
    ));
    doc.check_locked_edit(&raw)?;
    crate::editor::document::intent::ensure_automatic_edit_allowed(doc.raw(), &raw)?;
    Ok((
        raw,
        format!("{added} details; {kept} custom kept; {skipped} overlaps skipped"),
        Some(owner_after),
    ))
}

#[cfg(test)]
#[path = "../tests/surface_details.rs"]
mod tests;
