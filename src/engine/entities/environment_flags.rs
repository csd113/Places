//! Runtime water and steam switches and their presentation handoff.

use crate::engine::level::LevelDef;

use super::{ComponentTables, EntityHandle, EntityWorld, WorldCommand};

#[derive(Default)]
pub(super) struct EnvironmentFlags {
    steam: Vec<(EntityHandle, bool)>,
    water: Vec<(EntityHandle, bool)>,
}

impl EnvironmentFlags {
    pub(super) fn capture(components: &ComponentTables) -> Self {
        Self {
            steam: components
                .steam
                .iter()
                .map(|(handle, steam)| (handle, steam.enabled))
                .collect(),
            water: components
                .water
                .iter()
                .map(|(handle, water)| (handle, water.enabled))
                .collect(),
        }
    }
}

impl EntityWorld {
    pub(super) fn resolve_environment(&mut self, level: &LevelDef) {
        self.resolve_effects(level);
        self.resolve_water(level);
        self.authored_environment = EnvironmentFlags::capture(&self.components);
    }

    pub(super) fn reset_environment_flags(&mut self) {
        for &(handle, enabled) in &self.authored_environment.steam {
            if let Some(steam) = self.components.steam.get_mut(handle) {
                steam.enabled = enabled;
                if self.components.interactables.get(handle).is_none() {
                    Self::restore_enabled_mirror(
                        &self.authored_states,
                        &mut self.components.states,
                        handle,
                        self.components
                            .lights
                            .get(handle)
                            .map_or(enabled, |light| light.enabled),
                    );
                }
            }
        }
        for &(handle, enabled) in &self.authored_environment.water {
            if let Some(water) = self.components.water.get_mut(handle) {
                water.enabled = enabled;
                if self.components.interactables.get(handle).is_none()
                    && self.components.steam.get(handle).is_none()
                {
                    Self::restore_enabled_mirror(
                        &self.authored_states,
                        &mut self.components.states,
                        handle,
                        self.components
                            .lights
                            .get(handle)
                            .map_or(enabled, |light| light.enabled),
                    );
                }
            }
        }
        self.publish_environment_flags();
    }

    pub(super) fn publish_environment_flags(&mut self) {
        self.commands
            .extend(self.components.steam.iter().filter_map(|(_, steam)| {
                steam.effect.map(|index| WorldCommand::SetEffectEnabled {
                    index,
                    enabled: steam.enabled,
                })
            }));
        let water: Vec<_> = self
            .components
            .water
            .iter()
            .filter_map(|(handle, water)| {
                self.water_volume_index(handle)
                    .map(|index| WorldCommand::SetWaterEnabled {
                        index,
                        enabled: water.enabled,
                    })
            })
            .collect();
        self.commands.extend(water);
    }
}
