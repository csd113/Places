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

// The writer must accept an exactly full provenance record and omit the next
// byte. This boundary check uses no allocation or navigation bake.
#[test]
fn oversized_navigation_provenance_is_omitted() -> Result<(), String> {
    use super::super::navigation_cache::fits_provenance_record;
    let maximum = usize::try_from(crate::engine::package::MAX_MANIFEST_BYTES)
        .map_err(|error| error.to_string())?;
    assert!(fits_provenance_record(maximum));
    assert!(!fits_provenance_record(maximum.saturating_add(1)));
    Ok(())
}

// A one-cell encoded grid ties cache diagnostics to the decoded facts. It
// protects counts the input fingerprint alone cannot validate without a bake.
#[test]
fn navigation_report_matches_only_the_decoded_grid() -> Result<(), String> {
    use crate::engine::package::navigation::{
        CELL_SURFACE, NO_NAV_LINK, NavGrid, NavPortal, read_navigation, write_navigation,
    };
    use crate::map_compiler::navigation::NavBakeReport;
    use crate::map_compiler::navigation_cache::validate_report;

    let encoded = write_navigation(&NavGrid {
        cell_m: 0.25,
        cells_x: 1,
        cells_z: 1,
        column_offsets: vec![0, 1],
        links: vec![vec![[NO_NAV_LINK; 8]]],
        classes: vec![crate::engine::nav::reference_class()],
        cell_y: vec![0.0],
        cell_flags: vec![CELL_SURFACE],
        cell_headroom_cm: vec![270],
        cell_portal: vec![0],
        portals: vec![NavPortal {
            door: "fixture-door".into(),
        }],
        walkable: vec![vec![1]],
        region: vec![vec![0]],
        ..NavGrid::default()
    })?;
    let grid = read_navigation(&encoded)?;
    let correct = NavBakeReport {
        cell_m: 0.25,
        cells_x: 1,
        cells_z: 1,
        surface_cells: 1,
        walkable_cells: vec![1],
        regions: vec![1],
        portals: 1,
        workers: 0,
    };
    validate_report(&correct, &grid)?;
    for inconsistent in [
        NavBakeReport {
            cell_m: 0.5,
            ..correct.clone()
        },
        NavBakeReport {
            cells_x: 2,
            ..correct.clone()
        },
        NavBakeReport {
            cells_z: 2,
            ..correct.clone()
        },
        NavBakeReport {
            surface_cells: 0,
            ..correct.clone()
        },
        NavBakeReport {
            walkable_cells: vec![0],
            ..correct.clone()
        },
        NavBakeReport {
            regions: vec![2],
            ..correct.clone()
        },
        NavBakeReport {
            walkable_cells: vec![],
            ..correct.clone()
        },
        NavBakeReport {
            portals: 0,
            ..correct
        },
    ] {
        assert!(validate_report(&inconsistent, &grid).is_err());
    }
    Ok(())
}
