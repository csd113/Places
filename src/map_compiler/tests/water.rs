//! One tiny canonical-input regression protects source-only water controls
//! without masking real geometry/transport edits. No baking, assets or GPU;
//! hashing three metadata snapshots costs only a few kilobytes.
use crate::engine::level::LevelDef;
use crate::engine::quality::LightmapQuality;

#[test]
fn water_motion_changes_semantics_but_only_mean_plane_changes_preparation() -> Result<(), String> {
    let original = LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"water_key","name":"Water key","spawn":{"x":0,"z":0},"water":[{"x":1,"z":1,"width":2,"depth":2,"surface_y":0,"bottom_y":-2}]}"#).map_err(|error|error.to_string())?;
    let key = |level: &LevelDef| {
        super::super::lighting_fingerprint_with_revision(level, &[LightmapQuality::Off], &[], 3)
    };
    let mut changed = original.clone();
    let motion = &mut changed.water.first_mut().ok_or("missing water")?.motion;
    motion.flow_speed = 1.6;
    motion.direction_degrees = 270.0;
    motion.strength = 0.8;
    assert_ne!(
        crate::engine::canonical_json::canonical_json_bytes(&original)?,
        crate::engine::canonical_json::canonical_json_bytes(&changed)?,
        "changed motion must survive in package source identity"
    );
    assert_eq!(
        key(&original)?,
        key(&changed)?,
        "runtime-only motion must not invalidate prepared world records"
    );
    changed.water.first_mut().ok_or("missing water")?.surface_y = 0.1;
    assert_ne!(
        key(&original)?,
        key(&changed)?,
        "the water plane still changes prepared geometry and transport"
    );
    Ok(())
}
