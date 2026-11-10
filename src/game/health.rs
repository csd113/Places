//! Local player ownership over the shared health rules. A future authoritative
//! game mode can apply the same rules and actions without depending on the HUD.
use super::Game;
use crate::engine::health::{HealthAction, HealthSettings, HealthState, HealthTarget};

impl Game {
    #[must_use]
    pub const fn player_health(&self) -> &HealthState {
        &self.player_health
    }

    #[must_use]
    pub const fn health_settings(&self) -> &HealthSettings {
        self.world.health_settings()
    }

    /// Installs a validated rules value without resetting anyone's health.
    /// # Errors
    /// Rejects invalid settings before changing the current rules.
    pub fn set_health_settings(&mut self, settings: HealthSettings) -> Result<(), String> {
        self.world
            .set_health_settings(settings)
            .map_err(|problem| problem.to_string())
    }

    /// Applies an authority-side request against this session's current target.
    /// # Errors
    /// Rejects invalid amounts, unknown targets, or disabled health-kit healing.
    pub fn apply_health_action(
        &mut self,
        target: &HealthTarget,
        action: HealthAction,
    ) -> Result<(), String> {
        match target {
            HealthTarget::Player => {
                if let Some(change) = self
                    .player_health
                    .apply(action, self.world.health_settings())
                    .map_err(|problem| problem.to_string())?
                {
                    self.world
                        .record_health_change(HealthTarget::Player, change);
                }
                Ok(())
            }
            HealthTarget::Entity { id: _ } => self
                .world
                .dispatch_health(target, action)
                .map_err(|problem| problem.to_string()),
        }
    }

    /// Human-readable developer readout; never used as the state source.
    /// # Errors
    /// Rejects an unknown entity target.
    pub fn health_report(&self, target: &HealthTarget) -> Result<String, String> {
        if !self.health_settings().enabled {
            return Ok("Health is disabled for this map".into());
        }
        let health = match target {
            HealthTarget::Player => self.player_health,
            HealthTarget::Entity { id } => self
                .world
                .entity_health(id)
                .ok_or("Unknown health target")?,
        };
        Ok(format!(
            "{target:?}: {}/{} HP{}",
            health.current_hp(),
            health.max_hp(),
            if health.is_dead() { " (dead)" } else { "" }
        ))
    }

    pub(super) fn tick_player_health(&mut self) {
        if !self.world.health_settings().enabled {
            return;
        }
        match self
            .player_health
            .tick_regeneration(self.world_delta_seconds(), self.world.health_settings())
        {
            Ok(Some(change)) => self
                .world
                .record_health_change(HealthTarget::Player, change),
            Ok(None) => {}
            Err(problem) => crate::engine::logging::warn_once(
                "player-health-rules",
                format!("Health regeneration refused: {problem}"),
            ),
        }
    }
}

/// Validated maps always take the success path. Direct engine callers which
/// bypass map admission receive a diagnostic and the safe normal default.
pub(super) fn initial_player_health(settings: &HealthSettings) -> HealthState {
    settings.initial_player_health().unwrap_or_else(|problem| {
        crate::engine::logging::warn(format!("Invalid initial player health: {problem}"));
        HealthState::default()
    })
}
