//! Protect cache boundaries with one tiny room; no asset load, bake or GPU.
//! Physical edits must never inherit a stale graph, while metadata/lighting
//! edits must not trigger the expensive graph build.
use super::super::navigation_cache::fingerprint;
use crate::engine::level::LevelDef;

#[test]
fn navigation_identity_tracks_physical_components_and_excludes_display_and_lighting()
-> Result<(), String> {
    let original = LevelDef::from_json(r#"{"schema":"places-level", "format_version":1, "id":"tiny", "name":"Tiny", "spawn":{"x":1,"z":1}, "rooms":[{"width":2,"depth":2,"height":3}]}"#).map_err(|e| e.to_string())?;
    let identity = fingerprint(&original)?;
    let mut display = original.clone();
    display.name = "Renamed".into();
    display.author = "New author".into();
    display.crosshair = false;
    display.ceiling_lights =
        serde_json::from_str(r#"[{"fixture":"core:fluorescent_panel_01","x":1,"z":1}]"#)
            .map_err(|e| e.to_string())?;
    assert_eq!(identity, fingerprint(&display)?);
    let mut geometry = original.clone();
    geometry.rooms.first_mut().ok_or("Missing room")?.height = 3.25;
    assert_ne!(identity, fingerprint(&geometry)?);
    let mut collision = original.clone();
    collision.props = serde_json::from_str(
        r#"[{"model":"fixture", "x":1,"z":1,"solid":true,"size":[0.2,1,0.2]}]"#,
    )
    .map_err(|e| e.to_string())?;
    assert_ne!(identity, fingerprint(&collision)?);
    let mut agent = collision.clone();
    agent.props.first_mut().ok_or("Missing actor")?.components = serde_json::from_str(r#"[{"component":"nav_agent", "radius":0.2,"speed_mps":1,"height":1.5,"step_height":0.3,"max_slope":0.5}]"#).map_err(|e| e.to_string())?;
    assert_ne!(fingerprint(&collision)?, fingerprint(&agent)?);
    let mut obstacle = collision.clone();
    obstacle
        .props
        .first_mut()
        .ok_or("Missing obstacle")?
        .components = serde_json::from_str(
        r#"[{"component":"nav_obstacle", "size":[0.4,1,0.4],"affects_nav":true}]"#,
    )
    .map_err(|e| e.to_string())?;
    assert_ne!(fingerprint(&collision)?, fingerprint(&obstacle)?);
    let mut scripts = original;
    scripts.script_sources =
        serde_json::from_str(r#"[{"path":"scripts/test.luau", "source":"return {}"}]"#)
            .map_err(|e| e.to_string())?;
    assert_ne!(identity, fingerprint(&scripts)?);
    Ok(())
}
