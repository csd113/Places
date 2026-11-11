//! Versioned, copy-on-apply map rules. Presets never become live dependencies.
use serde::{Deserialize, Serialize};

use super::{health::HealthSettings, level::LevelDef, project::ProjectPath, tools::ToolSettings};

pub const MAX_MODE_BYTES: u64 = 16 * 1024;

/// Small inline mode identity and movement override; health/tools keep their
/// existing level fields so older source maps retain their exact behavior.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ModeInfo {
    pub name: String,
    pub walk_speed: f32,
}

impl Default for ModeInfo {
    fn default() -> Self {
        Self {
            name: "Custom mode".into(),
            walk_speed: 3.75,
        }
    }
}

impl ModeInfo {
    /// # Errors
    /// Rejects blank/oversized names and unsupported movement speeds.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty()
            || self.name.len() > 64
            || self.name.chars().any(char::is_control)
        {
            return Err("Mode name must contain 1–64 bytes and no control characters".into());
        }
        if !self.walk_speed.is_finite() || !(1.0..=10.0).contains(&self.walk_speed) {
            return Err("Walking speed must be between 1 and 10 metres per second".into());
        }
        Ok(())
    }
}

/// The same portable value is used for a library preset and the map's own copy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameMode {
    pub schema: String,
    pub format_version: u32,
    pub mode: ModeInfo,
    pub health: HealthSettings,
    pub tools: ToolSettings,
}

impl GameMode {
    #[must_use]
    pub fn from_level(level: &LevelDef) -> Self {
        Self {
            schema: "places-game-mode".into(),
            format_version: 1,
            mode: level.game_mode.clone().unwrap_or_default(),
            health: level.health,
            tools: level.tools,
        }
    }

    /// # Errors
    /// Rejects incompatible formats and invalid gameplay values.
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != "places-game-mode" || self.format_version != 1 {
            return Err("Expected places-game-mode format version 1".into());
        }
        self.mode.validate()?;
        self.health.validate().map_err(|error| error.to_string())?;
        self.tools.validate()
    }

    /// # Errors
    /// Rejects oversized, malformed or unsupported preset data.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_MODE_BYTES {
            return Err("Game mode exceeds 16 KiB".into());
        }
        crate::engine::project::check_editable_source(bytes)?;
        let mode: Self = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        mode.validate()?;
        Ok(mode)
    }

    /// # Errors
    /// Rejects invalid values before serializing a stable map-owned copy.
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let mut bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        Ok(bytes)
    }

    /// Immutable content-addressed copies make save publication crash-safe:
    /// publish the complete copy first, then atomically replace the level source.
    /// # Errors
    /// Rejects invalid mode values.
    pub fn project_path(&self) -> Result<ProjectPath, String> {
        ProjectPath::new(format!(
            "game_modes/{}.json",
            super::package::hash::sha256_hex(&self.bytes()?)
        ))
    }

    /// # Errors
    /// Refuses a missing or changed map-owned source rather than silently using
    /// a global preset or disagreeing inline runtime rules.
    pub fn verify_copy(&self, bytes: &[u8]) -> Result<(), String> {
        if Self::from_bytes(bytes)? != *self {
            return Err("Map game-mode copy disagrees with its saved level rules".into());
        }
        Ok(())
    }
}
