//! Map-local mode snapshots are published before level sources, never linked to presets.
use super::{Document, persistence};

/// Check only the small rules value, without cloning/deserializing the saved
/// geometry again during every source conflict preflight.
pub(super) fn verify_saved_copy(
    project: &crate::engine::project::ProjectFolder,
    raw: &serde_json::Value,
) -> Result<(), String> {
    let Some(info) = raw.get("game_mode").filter(|value| !value.is_null()) else {
        return Ok(());
    };
    let mode = GameMode {
        schema: "places-game-mode".into(),
        format_version: 1,
        mode: serde_json::from_value(info.clone()).map_err(|error| error.to_string())?,
        health: serde_json::from_value(
            raw.get("health")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
        )
        .map_err(|error| error.to_string())?,
        tools: serde_json::from_value(
            raw.get("tools")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({})),
        )
        .map_err(|error| error.to_string())?,
    };
    mode.verify_copy(&crate::engine::project::read_bounded_file(
        &project.resolve(&mode.project_path()?)?,
        crate::engine::game_mode::MAX_MODE_BYTES,
    )?)
}
use crate::engine::{game_mode::GameMode, level::LevelDef};

impl Document {
    /// Applies a copied preset in one normal undoable edit.
    /// # Errors
    /// Rejects unsupported rules or any resulting invalid level.
    pub fn apply_game_mode(&mut self, mode: &GameMode) -> Result<(), String> {
        mode.validate()?;
        let mut raw = self.raw().clone();
        let object = raw.as_object_mut().ok_or("Invalid level")?;
        for (key, value) in [
            ("game_mode", serde_json::to_value(&mode.mode)),
            ("health", serde_json::to_value(mode.health)),
            ("tools", serde_json::to_value(mode.tools)),
        ] {
            drop(object.insert(key.into(), value.map_err(|error| error.to_string())?));
        }
        self.accept_edit(raw)
    }
}

pub(super) fn publish_copy(root: &std::path::Path, level: &LevelDef) -> Result<(), String> {
    if level.game_mode.is_none() {
        return Ok(());
    }
    let mode = GameMode::from_level(level);
    let directory = root.join("game_modes");
    super::super::game_modes::safe_directory(&directory)?;
    let path = root.join(mode.project_path()?.as_str());
    if path.symlink_metadata().is_ok() {
        if path
            .symlink_metadata()
            .map_err(|error| error.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Game mode copies must not be symlinks".into());
        }
        mode.verify_copy(&crate::engine::project::read_bounded_file(
            &path,
            crate::engine::game_mode::MAX_MODE_BYTES,
        )?)
    } else {
        persistence::atomic_create(&path, &mode.bytes()?)
    }
}
