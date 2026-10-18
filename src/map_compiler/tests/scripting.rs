//! One tiny project protects source-byte build invalidation and selected module
//! confinement. It exercises input planning only (no bake, GPU or installed
//! assets), costing a handful of temporary file writes and two small VM loads.
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Fixture(std::path::PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn project_script_snapshot_is_selected_bounded_and_invalidates_identity() -> Result<(), String> {
    let fixture = Fixture(std::env::temp_dir().join(format!(
        "places-script-input-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )));
    fs::create_dir_all(fixture.0.join("levels")).map_err(|e| e.to_string())?;
    fs::create_dir(fixture.0.join("scripts")).map_err(|e| e.to_string())?;
    fs::write(fixture.0.join("places.project.json"),r#"{"schema":"places-project","project_format":1,"id":"scripts","name":"Scripts","entry_level":"entry","levels":[{"id":"entry","source":"levels/entry.json"}]}"#).map_err(|e|e.to_string())?;
    let source = fixture.0.join("levels/entry.json");
    fs::write(&source,r#"{"schema":"places-level","format_version":1,"id":"entry","name":"Entry","spawn":{"x":0,"z":0},"scripts":["scripts/entry.luau"]}"#).map_err(|e|e.to_string())?;
    let script = fixture.0.join("scripts/entry.luau");
    fs::write(&script, "return {entered=function() end}").map_err(|e| e.to_string())?;
    let first = super::super::project::read_source(&fixture.0, None)?;
    fs::write(
        &script,
        "return {entered=function() local changed = true end}",
    )
    .map_err(|e| e.to_string())?;
    let second = super::super::project::read_source(&fixture.0, None)?;
    assert_ne!(
        first.identity, second.identity,
        "editing only Luau invalidates compiler identity"
    );
    let prepared_key = |level: &crate::engine::level::LevelDef| {
        super::super::lighting_fingerprint_with_revision(
            level,
            &[crate::engine::quality::LightmapQuality::Off],
            &[],
            1,
        )
    };
    assert_eq!(
        prepared_key(&first.level)?,
        prepared_key(&second.level)?,
        "script-only edits reuse prepared lighting/geometry while updating semantics"
    );
    let mut geometry_edit = second.level.clone();
    geometry_edit.rooms.push(
        serde_json::from_value(serde_json::json!({"width":2_u32,"depth":2_u32}))
            .map_err(|e| e.to_string())?,
    );
    assert_ne!(
        prepared_key(&second.level)?,
        prepared_key(&geometry_edit)?,
        "geometry edits still invalidate preparation"
    );
    assert_eq!(
        second.level.script_sources.len(),
        1,
        "selected snapshot embedded"
    );
    assert!(
        super::super::project::read_source(&source, None).is_err(),
        "standalone sources cannot resolve scripts outside a project"
    );
    fs::write(&script, "x".repeat(65_537)).map_err(|e| e.to_string())?;
    assert!(
        super::super::project::read_source(&fixture.0, None).is_err(),
        "oversized reference rejected before VM"
    );
    fs::remove_file(script).map_err(|e| e.to_string())?;
    assert!(
        super::super::project::read_source(&fixture.0, None).is_err(),
        "missing declared source refused"
    );
    Ok(())
}
