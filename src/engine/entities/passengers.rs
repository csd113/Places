//! Portable component state is scoped to a named passenger, never a whole map.
use super::{EntityWorld, checkpoint::EntityState};
use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    state: EntityState,
    ai: Option<crate::engine::ai::travel::Record>,
}

impl Record {
    pub fn set_pose(&mut self, id: &str, position: Vec3, yaw: f32) {
        self.state.id = id.into();
        self.state.transform = Some((position.to_array(), yaw));
    }
    pub fn cabin_home(&mut self, frame: &crate::engine::elevator::CabinFrame) {
        if let Some(ai) = self.ai.as_mut()
            && let Some((position, yaw)) = self.state.transform
        {
            ai.cabin_home(frame, Vec3::from_array(position), yaw);
        }
    }
    pub fn relocate(&mut self, id: &str, from: Vec3, to: Vec3, rotation: Quat) {
        self.state.id = id.into();
        if let Some((position, yaw)) = self.state.transform.as_mut() {
            *position = crate::engine::elevator::map_cabin_point(
                from,
                to,
                rotation,
                Vec3::from_array(*position),
            )
            .to_array();
            *yaw = rotation
                .mul_quat(Quat::from_rotation_y(yaw.to_radians()))
                .to_euler(glam::EulerRot::YXZ)
                .0
                .to_degrees();
        }
        if let Some(ai) = self.ai.as_mut() {
            ai.transform(from, to, rotation);
        }
    }
    pub fn remap_targets(&mut self, names: &[(String, String)]) {
        if let Some(ai) = self.ai.as_mut() {
            ai.remap_target(names);
        }
    }
}

impl Record {
    pub fn retain_targets(&mut self, ids: &[String]) {
        if let Some(ai) = self.ai.as_mut() {
            ai.retain_targets(ids);
        }
    }
}

impl EntityWorld {
    /// # Errors
    /// Refuses incomplete actors and owners tied to an articulated assembly.
    pub fn passenger_record(&self, id: &str) -> Result<Record, String> {
        let handle = self.handle_of(id).ok_or("Passenger entity is missing")?;
        if self.physics_actors.contains_key(id)
            || self
                .components
                .carries
                .get(handle)
                .is_some_and(|c| c.item.is_some())
            || self.components.lifetimes.get(handle).is_some()
            || self.components.balloons.get(handle).is_some()
        {
            return Err(format!(
                "Passenger '{id}' has an unsupported articulated, carried-assembly or lifetime owner"
            ));
        }
        Ok(Record {
            state: self.checkpoint_entity(handle),
            ai: self
                .ai
                .agent(id)
                .map(|agent| {
                    crate::engine::ai::travel::Record::capture(agent, |owner| {
                        self.id_of(owner).map(str::to_string)
                    })
                })
                .transpose()?,
        })
    }

    /// # Errors
    /// Rejects mismatched components, identities and unsafe state values.
    pub fn restore_passenger(&mut self, record: &Record) -> Result<(), String> {
        self.restore_entity(&record.state)?;
        if let Some(ai) = &record.ai {
            let (position, yaw) = record
                .state
                .transform
                .ok_or("Passenger transform is missing")?;
            let names = &self.names;
            self.ai.restore_passenger(
                &record.state.id,
                ai,
                Vec3::from_array(position),
                yaw,
                |id| names.get(id),
            )?;
        }
        self.rebuild_entity_frames();
        self.rebuild_interactables_from_tables();
        Ok(())
    }
}
