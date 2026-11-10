//! Shared health adapters; component entries retain generation-safe ownership.
use super::{EntityHandle, EntityWorld, WorldCommand};
use crate::engine::health::{
    HealthAction, HealthChange, HealthError, HealthEvent, HealthSettings, HealthState, HealthTarget,
};

impl EntityWorld {
    pub(super) fn reset_health(&mut self) {
        self.components.health.clear();
        for (handle, initial) in self.components.health_initial.iter() {
            let _previous = self.components.health.insert(handle, *initial);
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
    /// Authored actors start at their specified health. Unconfigured entities
    /// use normal health and allocate a component only on first mutation.
    #[must_use]
    pub fn entity_health(&self, id: &str) -> Option<HealthState> {
        self.handle_of(id.trim())
            .and_then(|handle| self.health_for_handle(handle))
    }
    #[must_use]
    pub fn health_for_handle(&self, handle: EntityHandle) -> Option<HealthState> {
        self.store.contains(handle).then(|| {
            self.components
                .health
                .get(handle)
                .or_else(|| self.components.health_initial.get(handle))
                .copied()
                .unwrap_or_default()
        })
    }
    /// # Errors
    /// Rejects stale handles, invalid actions and disabled kits.
    pub fn apply_entity_health(
        &mut self,
        handle: EntityHandle,
        action: HealthAction,
    ) -> Result<Option<HealthChange>, HealthError> {
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
        action.validate()?;
        self.health_settings.validate()?;
        if !self.health_settings.enabled {
            return Err(HealthError::Disabled);
        }
        if matches!(action, HealthAction::HealthKit { hp: _ }) && !self.health_settings.health_kits
        {
            return Err(HealthError::DisabledHealthKit);
        }
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
