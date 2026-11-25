//! Shared health adapters; component entries retain generation-safe ownership.
use super::{EntityHandle, EntityWorld, WorldCommand};
use crate::engine::health::{
    HealthAction, HealthChange, HealthError, HealthEvent, HealthSettings, HealthState, HealthTarget,
};
use crate::engine::level::{ComponentDef, LevelDef};

impl LevelDef {
    /// Older players would ignore immortality or reject unconfigured zombies.
    #[must_use]
    pub fn requires_entity_health_rules(&self) -> bool {
        self.props
            .iter()
            .flat_map(|prop| &prop.components)
            .chain(self.doors.iter().flat_map(|door| &door.components))
            .chain(
                self.spawn_templates
                    .iter()
                    .flat_map(|template| &template.components),
            )
            .any(|component| {
                matches!(
                    component,
                    ComponentDef::Health {
                        max_hp: _,
                        starting_hp: _,
                        immortal: true,
                    }
                )
            })
            || (self.encounter.is_some()
                && self.spawn_points.iter().any(|point| {
                    point.zone.is_some()
                        && self.spawn_templates.iter().any(|template| {
                            template.id == point.template
                                && !template
                                    .components
                                    .iter()
                                    .any(|component| component.kind() == "health")
                        })
                }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum InitialHealth {
    Finite(HealthState),
    Immortal,
}

impl EntityWorld {
    pub(super) fn install_authored_health(
        &mut self,
        handle: EntityHandle,
        max_hp: u16,
        starting_hp: u16,
        immortal: bool,
    ) {
        if immortal {
            let _previous = self
                .components
                .health_initial
                .insert(handle, InitialHealth::Immortal);
            let _previous_state = self.components.health.remove(handle);
        } else if let Ok(state) = HealthState::new(starting_hp, max_hp) {
            self.install_finite_health(handle, state);
        }
    }

    fn install_finite_health(&mut self, handle: EntityHandle, state: HealthState) {
        let _previous = self
            .components
            .health_initial
            .insert(handle, InitialHealth::Finite(state));
        let _previous_state = self.components.health.insert(handle, state);
    }

    pub(super) fn install_default_zombie_health(&mut self, handle: EntityHandle) {
        const ZOMBIE_HP: u16 = 3 * crate::engine::health::NORMAL_MAX_HP;
        if !self.components.health_initial.contains(handle)
            && let Ok(state) = HealthState::new(ZOMBIE_HP, ZOMBIE_HP)
        {
            self.install_finite_health(handle, state);
        }
    }

    pub(super) fn reset_health(&mut self) {
        self.components.health.clear();
        for (handle, initial) in self.components.health_initial.iter() {
            if let InitialHealth::Finite(state) = initial {
                let _previous = self.components.health.insert(handle, *state);
            }
        }
        self.health_events.clear();
    }
    #[must_use]
    pub const fn health_settings(&self) -> &HealthSettings {
        &self.health_settings
    }
    /// Replaces reusable rules without reloading or reviving anyone.
    /// # Errors
    /// Rejects invalid numerical settings without changing current rules.
    pub fn set_health_settings(&mut self, settings: HealthSettings) -> Result<(), HealthError> {
        settings.validate()?;
        self.health_settings = settings;
        Ok(())
    }
    /// Finite health, if the entity exists and is not immortal. Entities
    /// without a specific health policy allocate a state only on mutation.
    #[must_use]
    pub fn entity_health(&self, id: &str) -> Option<HealthState> {
        self.handle_of(id.trim())
            .and_then(|handle| self.health_for_handle(handle))
    }
    #[must_use]
    pub fn health_for_handle(&self, handle: EntityHandle) -> Option<HealthState> {
        if !self.store.contains(handle) {
            return None;
        }
        match self.components.health_initial.get(handle) {
            Some(InitialHealth::Immortal) => None,
            Some(InitialHealth::Finite(initial)) => Some(
                self.components
                    .health
                    .get(handle)
                    .copied()
                    .unwrap_or(*initial),
            ),
            None => Some(
                self.components
                    .health
                    .get(handle)
                    .copied()
                    .unwrap_or_default(),
            ),
        }
    }

    pub(super) fn health_handle_is_immortal(&self, handle: EntityHandle) -> bool {
        matches!(
            self.components.health_initial.get(handle),
            Some(InitialHealth::Immortal)
        )
    }

    #[must_use]
    pub fn entity_is_immortal(&self, id: &str) -> bool {
        self.handle_of(id.trim())
            .is_some_and(|handle| self.health_handle_is_immortal(handle))
    }

    /// # Errors
    /// Rejects stale handles, invalid actions and disabled kits.
    pub fn apply_entity_health(
        &mut self,
        handle: EntityHandle,
        action: HealthAction,
    ) -> Result<Option<HealthChange>, HealthError> {
        if self.health_handle_is_immortal(handle) {
            self.health_settings.validate_action(action)?;
            return Ok(None);
        }
        let mut state = self
            .health_for_handle(handle)
            .ok_or(HealthError::MissingTarget)?;
        let change = state.apply(action, &self.health_settings)?;
        let _previous = self.components.health.insert(handle, state);
        if let Some(accepted) = change
            && let Some(id) = self.instance_id_of(handle).map(str::to_string)
        {
            self.record_health_change(HealthTarget::Entity { id }, accepted);
        }
        Ok(change)
    }
    /// Routes authored commands; a future authoritative host can validate them
    /// before invoking this boundary instead of trusting remote client state.
    /// # Errors
    /// Rejects missing targets, invalid actions and disabled kits.
    pub fn dispatch_health(
        &mut self,
        target: &HealthTarget,
        action: HealthAction,
    ) -> Result<(), HealthError> {
        self.health_settings.validate_action(action)?;
        match target {
            HealthTarget::Player => self.commands.push(WorldCommand::PlayerHealth { action }),
            HealthTarget::Entity { id } => {
                let handle = self
                    .handle_of(id.trim())
                    .ok_or(HealthError::MissingTarget)?;
                let _change = self.apply_entity_health(handle, action)?;
            }
        }
        Ok(())
    }
    /// Records accepted changes for presentation, diagnostics or replication.
    /// A bounded queue prevents an unconsumed optional stream growing forever.
    pub fn record_health_change(&mut self, target: HealthTarget, change: HealthChange) {
        if self.health_events.len() >= super::MAX_EVENTS_PER_TICK {
            let _discarded = self.health_events.remove(0);
        }
        let entity_handle = match &target {
            HealthTarget::Player => None,
            HealthTarget::Entity { id } => self
                .handle_of(id)
                .map(|handle| (handle.index(), handle.generation())),
        };
        self.health_events.push(HealthEvent {
            world_generation: self.generation,
            entity_handle,
            target,
            change,
        });
    }
    pub fn take_health_events(&mut self) -> Vec<HealthEvent> {
        std::mem::take(&mut self.health_events)
    }
    pub(super) fn tick_health(&mut self, delta: f32) {
        if !self.health_settings.enabled {
            return;
        }
        let changes: Vec<_> = self
            .components
            .health
            .iter_mut()
            .filter_map(|(handle, state)| {
                state
                    .tick_regeneration(delta, &self.health_settings)
                    .ok()
                    .flatten()
                    .map(|change| (handle, change))
            })
            .collect();
        for (handle, change) in changes {
            if let Some(id) = self.instance_id_of(handle).map(str::to_string) {
                self.record_health_change(HealthTarget::Entity { id }, change);
            }
        }
    }
}
