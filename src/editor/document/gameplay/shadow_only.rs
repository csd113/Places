//! Mesh visibility authoring is independent of collision and behaviour.
use super::super::objects::{array_mut, object_mut};
use super::super::{Document, ObjectId};
use serde_json::Value;

impl Document {
    pub(super) fn set_shadow_only(&mut self, id: ObjectId, value: Value) -> Result<(), String> {
        let mut raw = self.raw().clone();
        if value.as_bool().is_none() {
            return Err("Shadow only must be a Boolean".into());
        }
        match id {
            ObjectId::Prop(_) => {}
            ObjectId::SpawnPoint(i) => {
                let point = self
                    .level()
                    .spawn_points
                    .get(i)
                    .ok_or("Missing spawn point")?;
                let index = self
                    .level()
                    .spawn_templates
                    .iter()
                    .position(|template| template.id == point.template)
                    .ok_or("Missing template")?;
                let template = array_mut(&mut raw, "spawn_templates")?
                    .get_mut(index)
                    .and_then(Value::as_object_mut)
                    .ok_or("Invalid template")?;
                drop(template.insert("shadow_only".into(), value));
                return self.accept_edit(raw);
            }
            ObjectId::Room(_)
            | ObjectId::Wall(_)
            | ObjectId::Ramp(_)
            | ObjectId::FloorRegion(_)
            | ObjectId::Light(_)
            | ObjectId::Audio(_)
            | ObjectId::Door(_)
            | ObjectId::Trigger(_)
            | ObjectId::FogRegion(_)
            | ObjectId::GroundFog(_) => {
                return Err("Shadow only belongs to an entity or spawn template".into());
            }
        }
        drop(object_mut(&mut raw, id)?.insert("shadow_only".into(), value));
        self.accept_edit(raw)
    }
}
