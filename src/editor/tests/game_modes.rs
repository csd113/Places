//! Protects copy-on-apply, crash-safe map saves, legacy defaults and stale-file
//! refusal with two tiny source maps and no renderer/assets/bake (milliseconds).
use crate::editor::document::components::{Field, Kind, Target};
use crate::editor::document::{Document, ObjectId};
use crate::engine::game_mode::GameMode;

struct Fixture(std::path::PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn game_mode_copies_survive_preset_changes_and_reject_disk_conflicts() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!("places-game-modes-{}", std::process::id()));
    std::fs::create_dir(&root).map_err(|error| error.to_string())?;
    let fixture = Fixture(root);
    let mut first = Document::create(&fixture.0.join("first"), "first", "First")?;
    assert!(
        first.level().game_mode.is_none() && !first.level().health.enabled,
        "old maps keep legacy defaults"
    );
    let original = first.raw().clone();
    let id = ObjectId::Prop(0);
    for (target, key, value, kind) in [
        (Target::GameMode, "name", "Quick explore", Kind::Text),
        (Target::GameMode, "walk_speed", "6", Kind::Number),
        (Target::Tools, "crowbar", "true", Kind::Bool),
    ] {
        first.set_component_field(id, Field::new(target, key, "Mode", kind, "null"), value)?;
    }
    assert!(
        !first.level().health.enabled,
        "speed and inventory never enable health"
    );
    let mode = GameMode::from_level(first.level());
    let library = fixture.0.join("presets");
    super::super::game_modes::save_preset(&library, &mode, &[])?;
    first.save_all()?;
    let mut second = Document::create(&fixture.0.join("second"), "second", "Second")?;
    second.apply_game_mode(&mode)?;
    second.save_all()?;
    let library_before = super::super::game_modes::read_presets(&library)?;
    let mut replacement = mode.clone();
    replacement.mode.walk_speed = 2.0;
    replacement.health.enabled = true;
    super::super::game_modes::save_preset(&library, &replacement, &library_before)?;
    assert_eq!(
        GameMode::from_level(Document::open(first.root())?.level()),
        mode,
        "preset edits must not modify saved maps"
    );
    assert_eq!(
        GameMode::from_level(Document::open(second.root())?.level()),
        mode,
        "copies on other maps are independent"
    );
    second.apply_game_mode(&replacement)?;
    assert!(second.undo(), "preset application is one undoable edit");
    assert_eq!(GameMode::from_level(second.level()), mode);
    assert!(second.redo(), "preset application supports redo");
    assert!(second.level().health.enabled, "explicit opt-in copied");
    let before_invalid = second.raw().clone();
    for invalid in ["0", "10.1", "NaN"] {
        assert!(
            second
                .set_component_field(
                    id,
                    Field::new(
                        Target::GameMode,
                        "walk_speed",
                        "Speed",
                        Kind::Number,
                        "3.75"
                    ),
                    invalid
                )
                .is_err(),
            "invalid speed rejected"
        );
        assert_eq!(
            second.raw(),
            &before_invalid,
            "invalid edit must not change source"
        );
    }
    second.save_all()?;
    let snapshots = crate::engine::package::project::snapshot(second.project(), &|| Ok(()))?;
    assert!(
        snapshots.contains_key(&replacement.project_path()?),
        "project export includes the selected copy"
    );
    check_conflict(&mut first, &mode, &replacement)?;
    assert!(
        original.get("game_mode").is_none(),
        "opening an old map creates no mode"
    );
    Ok(())
}

fn check_conflict(
    first: &mut Document,
    mode: &GameMode,
    replacement: &GameMode,
) -> Result<(), String> {
    let mode_path = first.root().join(mode.project_path()?.as_str());
    let saved_source =
        std::fs::read(first.root().join("levels/first.json")).map_err(|error| error.to_string())?;
    std::fs::write(&mode_path, replacement.bytes()?).map_err(|error| error.to_string())?;
    first.set_crosshair(false)?;
    assert!(
        first.save_all().is_err(),
        "outside mode edits must stop save"
    );
    assert!(
        Document::open(first.root()).is_err(),
        "outside mode edits must stop reopen"
    );
    assert_eq!(
        std::fs::read(first.root().join("levels/first.json")).map_err(|error| error.to_string())?,
        saved_source,
        "failed save leaves level intact"
    );
    Ok(())
}
