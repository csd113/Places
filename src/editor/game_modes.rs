//! Beginner-facing copied presets; advanced scripting/import UI is intentionally separate future work.
use super::{
    App,
    ui::{self, Action, Hit},
};
use crate::engine::game_mode::{GameMode, MAX_MODE_BYTES};
use crate::engine::render::Vertex;
use std::path::{Path, PathBuf};

pub(super) struct Preset {
    path: PathBuf,
    bytes: Vec<u8>,
    mode: GameMode,
}

pub(super) fn safe_directory(path: &Path) -> Result<(), String> {
    if path
        .symlink_metadata()
        .is_ok_and(|meta| meta.file_type().is_symlink())
    {
        return Err("Game mode directory must not be a symlink".into());
    }
    std::fs::create_dir_all(path).map_err(|error| error.to_string())
}

fn preset_path(directory: &Path, name: &str) -> PathBuf {
    directory.join(format!(
        "{}.json",
        crate::engine::package::hash::sha256_hex(name.trim().to_lowercase().as_bytes())
    ))
}

pub(super) fn read_presets(directory: &Path) -> Result<Vec<Preset>, String> {
    if !directory.try_exists().map_err(|error| error.to_string())? {
        return Ok(Vec::new());
    }
    safe_directory(directory)?;
    let mut presets = Vec::new();
    for result in std::fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = result.map_err(|error| error.to_string())?;
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        if !entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            return Err("Game mode presets must be regular files, not symlinks".into());
        }
        if presets.len() >= 128 {
            return Err("Game mode library supports up to 128 presets".into());
        }
        let bytes = crate::engine::project::read_bounded_file(&path, MAX_MODE_BYTES)?;
        let mode = GameMode::from_bytes(&bytes)?;
        if path != preset_path(directory, &mode.mode.name) {
            return Err("Preset filename does not match its name".into());
        }
        presets.push(Preset { path, bytes, mode });
    }
    presets.sort_by(|a, b| {
        a.mode
            .mode
            .name
            .to_lowercase()
            .cmp(&b.mode.mode.name.to_lowercase())
    });
    Ok(presets)
}

pub(super) fn save_preset(
    directory: &Path,
    mode: &GameMode,
    presets: &[Preset],
) -> Result<(), String> {
    mode.validate()?;
    safe_directory(directory)?;
    let path = preset_path(directory, &mode.mode.name);
    let bytes = mode.bytes()?;
    if let Some(previous) = presets.iter().find(|preset| preset.path == path) {
        if path
            .symlink_metadata()
            .map_err(|error| error.to_string())?
            .file_type()
            .is_symlink()
            || crate::engine::project::read_bounded_file(&path, MAX_MODE_BYTES)? != previous.bytes
        {
            return Err(
                "Preset changed outside the editor; reopen Game Modes before saving".into(),
            );
        }
        super::document::persistence::atomic_replace(&path, &bytes)
    } else {
        if presets.len() >= 128 {
            return Err("Game mode library supports up to 128 presets".into());
        }
        super::document::persistence::atomic_create(&path, &bytes)
    }
}

impl App {
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "Only Game Modes commands belong to this dispatcher."
    )]
    pub(super) fn game_mode_action(&mut self, action: Action) -> Result<bool, String> {
        match action {
            Action::Components(page) if matches!(page % 100, 17 | 18) => {
                self.mode_presets =
                    read_presets(&crate::engine::paths::user()?.data.join("game_modes"))?;
                return Ok(false);
            }
            Action::SaveGameModePreset => {
                let mode =
                    GameMode::from_level(self.document.as_ref().ok_or("Open a project")?.level());
                let directory = crate::engine::paths::user()?.data.join("game_modes");
                save_preset(&directory, &mode, &self.mode_presets)?;
                self.mode_presets = read_presets(&directory)?;
                self.status = format!(
                    "Saved preset '{}'. Existing map copies are unchanged.",
                    mode.mode.name
                );
            }
            Action::ApplyGameModePreset(index) => {
                let preset = self
                    .mode_presets
                    .get(index)
                    .ok_or("Choose a listed preset")?;
                self.document
                    .as_mut()
                    .ok_or("Open a project")?
                    .apply_game_mode(&preset.mode)?;
                self.changed();
                self.component_selection.page = 17;
                self.popup = Some(super::actions::Popup::Components { page: 17 });
                self.status =
                    "Preset copied to this map. Save the map to keep its independent copy.".into();
            }
            Action::NewGameMode => {
                let doc = self.document.as_mut().ok_or("Open a project")?;
                let mut mode = GameMode::from_level(doc.level());
                mode.mode = crate::engine::game_mode::ModeInfo::default();
                mode.health = crate::engine::health::HealthSettings::default();
                mode.tools = crate::engine::tools::ToolSettings::default();
                doc.apply_game_mode(&mode)?;
                self.changed();
                self.status = "New walking mode: health off, no crowbar, 3.75 m/s. Undo restores the previous rules.".into();
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

pub(super) fn draw(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>, page: usize) {
    let Some(doc) = app.document.as_ref() else {
        return;
    };
    if page % 100 == 18 {
        draw_presets(app, vertices, hits, page / 100);
        return;
    }
    let mode = GameMode::from_level(doc.level());
    ui::text(
        vertices,
        &format!("This map: {}", mode.mode.name),
        140.0,
        96.0,
        ui::INK,
        68,
    );
    for (row, (label, action)) in [
        ("Name and movement speed >", Action::Components(19)),
        ("Health, recovery and display >", Action::Components(14)),
        ("Crowbar >", Action::Components(16)),
        ("Choose saved preset >", Action::Components(18)),
        (
            if app.mode_presets.iter().any(|preset| {
                preset
                    .mode
                    .mode
                    .name
                    .trim()
                    .eq_ignore_ascii_case(mode.mode.name.trim())
            }) {
                "Update named preset"
            } else {
                "Save as named preset"
            },
            Action::SaveGameModePreset,
        ),
        ("New mode (walking defaults)", Action::NewGameMode),
    ]
    .into_iter()
    .enumerate()
    {
        ui::button(
            vertices,
            hits,
            [
                140.0,
                f32::from(u16::try_from(row).unwrap_or(0)).mul_add(12.0, 107.0),
                240.0,
                10.0,
            ],
            label,
            action,
            true,
        );
    }
    ui::text(
        vertices,
        "Preset changes never alter existing map copies.",
        140.0,
        185.0,
        ui::MUTED,
        68,
    );
}

fn draw_presets(app: &App, vertices: &mut Vec<Vertex>, hits: &mut Vec<Hit>, offset: usize) {
    for (row, (index, preset)) in app
        .mode_presets
        .iter()
        .enumerate()
        .skip(offset.saturating_mul(5))
        .take(5)
        .enumerate()
    {
        ui::button(
            vertices,
            hits,
            [
                140.0,
                f32::from(u16::try_from(row).unwrap_or(0)).mul_add(14.0, 98.0),
                240.0,
                12.0,
            ],
            &format!("Apply copy: {}", preset.mode.mode.name),
            Action::ApplyGameModePreset(index),
            true,
        );
    }
    if app.mode_presets.is_empty() {
        ui::text(
            vertices,
            "No presets yet. Name your mode, then save a preset.",
            140.0,
            105.0,
            ui::MUTED,
            68,
        );
    }
    ui::components::pagination(
        vertices,
        hits,
        18,
        offset,
        app.mode_presets.len().div_ceil(5),
    );
}
