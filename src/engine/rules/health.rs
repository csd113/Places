//! Shared, authority-neutral health. One HP is one half-heart; death is terminal
//! until the owner explicitly replaces the state. This module never respawns.
use serde::{Deserialize, Serialize};

pub const NORMAL_MAX_HP: u16 = 6;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "Map enablement, regeneration, kits and HUD are independent author choices, not mutually exclusive states."
)]
pub struct HealthSettings {
    /// Explicit map opt-in; ordinary walking maps do not simulate health.
    pub enabled: bool,
    pub regeneration: bool,
    pub health_kits: bool,
    pub show_player_health: bool,
    pub player_max_hp: u16,
    pub player_starting_hp: u16,
    pub regeneration_hp_per_second: f32,
    pub regeneration_delay_seconds: f32,
    /// Optional hit protection; zero accepts every hit.
    pub damage_cooldown_seconds: f32,
}
impl Default for HealthSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            regeneration: false,
            health_kits: true,
            show_player_health: true,
            player_max_hp: NORMAL_MAX_HP,
            player_starting_hp: NORMAL_MAX_HP,
            regeneration_hp_per_second: 1.0,
            regeneration_delay_seconds: 5.0,
            damage_cooldown_seconds: 0.0,
        }
    }
}
impl HealthSettings {
    /// Builds a fresh player state for explicit level/reset/respawn operations.
    /// # Errors
    /// Rejects invalid rules or starting HP above a positive maximum.
    pub fn initial_player_health(&self) -> Result<HealthState, HealthError> {
        self.validate()?;
        HealthState::new(self.player_starting_hp, self.player_max_hp)
    }

    /// Rejects invalid rates before they reach simulation.
    /// # Errors
    /// Returns an error for negative or nonfinite numbers.
    pub fn validate(&self) -> Result<(), HealthError> {
        if self.player_max_hp == 0 || self.player_starting_hp > self.player_max_hp {
            return Err(HealthError::InvalidState);
        }
        if [
            self.regeneration_hp_per_second,
            self.regeneration_delay_seconds,
            self.damage_cooldown_seconds,
        ]
        .into_iter()
        .all(|n| n.is_finite() && n >= 0.0)
        {
            Ok(())
        } else {
            Err(HealthError::InvalidNumber)
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HealthError {
    Disabled,
    InvalidNumber,
    InvalidState,
    DisabledHealthKit,
    MissingTarget,
}
impl std::fmt::Display for HealthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Disabled => "Health is disabled for this map",
            Self::InvalidNumber => "Health numbers must be finite and nonnegative",
            Self::InvalidState => "Invalid health state or amount",
            Self::DisabledHealthKit => "Health kits are disabled",
            Self::MissingTarget => "Health target does not exist",
        })
    }
}
impl std::error::Error for HealthError {}

/// Stable authored identity. Resolve against the authoritative world's store;
/// never accept an external runtime slot index as proof of identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HealthTarget {
    Player,
    Entity { id: String },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum HealthAction {
    Damage { hp: u16 },
    Heal { hp: u16 },
    HealthKit { hp: u16 },
}
impl HealthAction {
    /// # Errors
    /// Rejects zero-amount actions.
    pub const fn validate(self) -> Result<(), HealthError> {
        let (Self::Damage { hp } | Self::Heal { hp } | Self::HealthKit { hp }) = self;
        if hp == 0 {
            Err(HealthError::InvalidState)
        } else {
            Ok(())
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthChange {
    pub previous_hp: u16,
    pub current_hp: u16,
    pub max_hp: u16,
}
impl HealthChange {
    #[must_use]
    pub const fn became_dead(self) -> bool {
        self.previous_hp > 0 && self.current_hp == 0
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthEvent {
    /// Local world generation; reject events after replacing the world.
    pub world_generation: u64,
    /// Runtime slot/generation for rejecting same-world despawn/reuse.
    pub entity_handle: Option<(u32, u32)>,
    pub target: HealthTarget,
    pub change: HealthChange,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "HealthRecord")]
pub struct HealthState {
    current_hp: u16,
    max_hp: u16,
    since_damage: f64,
    regeneration_fraction: f64,
    damage_cooldown_remaining: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HealthRecord {
    current_hp: u16,
    max_hp: u16,
    since_damage: f64,
    regeneration_fraction: f64,
    #[serde(default)]
    damage_cooldown_remaining: f64,
}
impl TryFrom<HealthRecord> for HealthState {
    type Error = HealthError;
    fn try_from(value: HealthRecord) -> Result<Self, Self::Error> {
        if !value.damage_cooldown_remaining.is_finite()
            || value.damage_cooldown_remaining < 0.0_f64
            || value.max_hp == 0
            || value.current_hp > value.max_hp
            || !value.since_damage.is_finite()
            || value.since_damage < 0.0_f64
            || !value.regeneration_fraction.is_finite()
            || !(0.0_f64..1.0_f64).contains(&value.regeneration_fraction)
        {
            return Err(HealthError::InvalidState);
        }
        Ok(Self {
            current_hp: value.current_hp,
            max_hp: value.max_hp,
            since_damage: value.since_damage,
            regeneration_fraction: value.regeneration_fraction,
            damage_cooldown_remaining: value.damage_cooldown_remaining,
        })
    }
}
impl Default for HealthState {
    fn default() -> Self {
        Self {
            current_hp: NORMAL_MAX_HP,
            max_hp: NORMAL_MAX_HP,
            since_damage: 0.0,
            regeneration_fraction: 0.0,
            damage_cooldown_remaining: 0.0,
        }
    }
}
impl HealthState {
    /// # Errors
    /// Rejects a zero maximum or current HP greater than the maximum.
    pub fn new(current_hp: u16, max_hp: u16) -> Result<Self, HealthError> {
        HealthRecord {
            current_hp,
            max_hp,
            since_damage: 0.0,
            regeneration_fraction: 0.0,
            damage_cooldown_remaining: 0.0,
        }
        .try_into()
    }
    #[must_use]
    pub const fn current_hp(self) -> u16 {
        self.current_hp
    }
    #[must_use]
    pub const fn max_hp(self) -> u16 {
        self.max_hp
    }
    #[must_use]
    pub const fn is_dead(self) -> bool {
        self.current_hp == 0
    }
    fn change(self, previous_hp: u16) -> Option<HealthChange> {
        (previous_hp != self.current_hp).then_some(HealthChange {
            previous_hp,
            current_hp: self.current_hp,
            max_hp: self.max_hp,
        })
    }
    /// Applies an authoritative action. Healing and kits never revive.
    /// # Errors
    /// Rejects invalid settings, zero amounts and disabled kits without mutation.
    pub fn apply(
        &mut self,
        action: HealthAction,
        settings: &HealthSettings,
    ) -> Result<Option<HealthChange>, HealthError> {
        settings.validate()?;
        action.validate()?;
        if !settings.enabled {
            return Err(HealthError::Disabled);
        }
        if matches!(action, HealthAction::HealthKit { hp: _ }) && !settings.health_kits {
            return Err(HealthError::DisabledHealthKit);
        }
        if self.is_dead() {
            return Ok(None);
        }
        if matches!(action, HealthAction::Damage { hp: _ })
            && settings.damage_cooldown_seconds > 0.0
            && self.damage_cooldown_remaining > 0.0_f64
        {
            return Ok(None);
        }
        let previous = self.current_hp;
        match action {
            HealthAction::Damage { hp } => {
                self.damage_cooldown_remaining = f64::from(settings.damage_cooldown_seconds);
                self.current_hp = self.current_hp.saturating_sub(hp);
                self.since_damage = 0.0_f64;
                self.regeneration_fraction = 0.0_f64;
            }
            HealthAction::Heal { hp } | HealthAction::HealthKit { hp } => {
                self.current_hp = self.current_hp.saturating_add(hp).min(self.max_hp);
                if self.current_hp == self.max_hp {
                    self.regeneration_fraction = 0.0_f64;
                }
            }
        }
        Ok(self.change(previous))
    }
    /// Accumulates sub-HP healing and counts only time beyond the damage delay.
    /// # Errors
    /// Rejects invalid settings or a negative/nonfinite delta without mutation.
    pub fn tick_regeneration(
        &mut self,
        delta: f32,
        settings: &HealthSettings,
    ) -> Result<Option<HealthChange>, HealthError> {
        settings.validate()?;
        if !delta.is_finite() || delta < 0.0 {
            return Err(HealthError::InvalidNumber);
        }
        if !settings.enabled {
            return Ok(None);
        }
        self.damage_cooldown_remaining =
            (self.damage_cooldown_remaining - f64::from(delta)).max(0.0);
        if self.is_dead() {
            return Ok(None);
        }
        let delay = f64::from(settings.regeneration_delay_seconds);
        let eligible = (f64::from(delta) - (delay - self.since_damage).max(0.0)).max(0.0);
        self.since_damage = (self.since_damage + f64::from(delta)).min(f64::from(f32::MAX));
        if !settings.regeneration || self.current_hp == self.max_hp {
            return Ok(None);
        }
        let previous = self.current_hp;
        self.regeneration_fraction = eligible.mul_add(
            f64::from(settings.regeneration_hp_per_second),
            self.regeneration_fraction,
        );
        // At most u16::MAX iterations even for extreme authored rates/deltas.
        while self.regeneration_fraction >= 1.0_f64 && self.current_hp < self.max_hp {
            self.current_hp = self.current_hp.saturating_add(1);
            self.regeneration_fraction -= 1.0_f64;
        }
        if self.current_hp == self.max_hp {
            self.regeneration_fraction = 0.0_f64;
        }
        Ok(self.change(previous))
    }
}
