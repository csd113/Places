//! Nested spawn-zone edits retain source extensions and normal document history.
use super::super::objects::object_mut;
use super::super::{Document, ObjectId};
use super::GameplaySetting;
use serde_json::{Value, json};
impl Document {
    pub(super) fn spawn_zone_value(&self, id: ObjectId, setting: GameplaySetting) -> Option<Value> {
        if let ObjectId::SpawnPoint(i) = id
            && setting.is_spawn_zone()
        {
            let zone = self.level().spawn_points.get(i).and_then(|p| p.zone);
            return Some(if setting == GameplaySetting::SpawnZone {
                json!(zone.is_some())
            } else {
                serde_json::to_value(zone.unwrap_or_default())
                    .unwrap_or(Value::Null)
                    .get(setting.key())
                    .cloned()
                    .unwrap_or(Value::Null)
            });
        }
        None
    }
    pub(super) fn set_spawn_zone(
        &mut self,
        id: ObjectId,
        setting: GameplaySetting,
        value: Value,
    ) -> Result<(), String> {
        let mut raw = self.raw().clone();
        if !matches!(id, ObjectId::SpawnPoint(_)) {
            return Err("Spawn zone settings belong to a spawn point".into());
        }
        let object = object_mut(&mut raw, id)?;
        if setting == GameplaySetting::SpawnZone {
            if value.as_bool() == Some(true) {
                drop(object.insert(
                    "zone".into(),
                    json!(crate::engine::entities::encounter::SpawnZoneDef::default()),
                ));
                // Ordinary points have an exclusive group. A zone owns its bounded cohort.
                drop(object.remove("group"));
            } else {
                drop(object.remove("zone"));
            }
        } else {
            let zone = object
                .get_mut("zone")
                .and_then(Value::as_object_mut)
                .ok_or("Enable Spawn zone first")?;
            let zone_value = if setting == GameplaySetting::ZoneCount {
                let n = value
                    .as_f64()
                    .filter(|n| {
                        n.is_finite() && n.fract() == 0.0_f64 && (1.0_f64..=20.0_f64).contains(n)
                    })
                    .ok_or("Zone count must be a whole number from 1 to 20")?;
                json!(
                    n.to_string()
                        .parse::<u16>()
                        .map_err(|error| format!("Invalid zone count: {error}"))?
                )
            } else {
                value
            };
            drop(zone.insert(setting.key().into(), zone_value));
        }
        self.accept_edit(raw)
    }
}
