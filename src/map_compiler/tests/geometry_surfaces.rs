//! One tiny in-memory triangle-pair test protects the new measured highlight
//! payload and a specific regression risk: a suppressed warning must not
//! attach its geometry to the previous finding. No map, assets or bake run.

use super::{Checker, Severity, SurfaceKey, SurfaceKind, Tri};
use crate::engine::level::LevelDef;

// Essential room-flood regressions: closed/open door poses and an upper floor
// above a sealed lower storey previously had the wrong leak result. These tiny
// in-memory rooms invoke only collision/flood logic, with no mesh or asset load.
#[test]
fn room_leaks_use_authored_doors_and_the_room_own_storey() -> Result<(), String> {
    use super::authored_colliders;
    use crate::engine::level::LevelSurfaces;

    let mut raw = serde_json::json!({
        "schema":"places-level","format_version":1_u32,"id":"door_leak","name":"Door leak",
        "spawn":{"x":1.0_f32,"z":1.0_f32},
        "rooms":[{"width":2.0_f32,"depth":2.0_f32,"height":4.0_f32}],
        "walls":[
            {"x":-0.2_f32,"z":0.0_f32,"width":0.2_f32,"depth":2.0_f32,"height":2.5_f32},
            {"x":2.0_f32,"z":0.0_f32,"width":0.2_f32,"depth":2.0_f32,"height":2.5_f32},
            {"x":0.0_f32,"z":2.0_f32,"width":2.0_f32,"depth":0.2_f32,"height":2.5_f32}
        ],
        "doors":[{"id":"edge_leaf","x":0.0_f32,"z":0.0_f32,"width":2.0_f32,"height":2.5_f32}]
    });
    let leaks = |source: &serde_json::Value| -> Result<Vec<super::Finding>, String> {
        let level: LevelDef =
            serde_json::from_value(source.clone()).map_err(|error| error.to_string())?;
        let surfaces = LevelSurfaces::new(&level);
        let mut checker = Checker::new(&level);
        checker.check_rooms(&surfaces, &authored_colliders(&level, &surfaces));
        Ok(checker
            .findings
            .into_iter()
            .filter(|entry| entry.check == "room-leak")
            .collect())
    };
    assert!(
        leaks(&raw)?.is_empty(),
        "a closed leaf must seal the lower room's flood"
    );
    let open_door = raw
        .get_mut("doors")
        .and_then(serde_json::Value::as_array_mut)
        .and_then(|doors| doors.first_mut())
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("door")?;
    let _previous_closed_state = open_door.insert(
        "initial_state".into(),
        serde_json::Value::String("open".into()),
    );
    assert_eq!(
        leaks(&raw)?.len(),
        1,
        "the open starting pose exposes a real void route"
    );
    let closed_door = raw
        .get_mut("doors")
        .and_then(serde_json::Value::as_array_mut)
        .and_then(|doors| doors.first_mut())
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("door")?;
    let _previous_open_state = closed_door.insert(
        "initial_state".into(),
        serde_json::Value::String("closed".into()),
    );
    raw.get_mut("rooms")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or("rooms")?
        .push(
            serde_json::json!({"width":2.0_f32,"depth":2.0_f32,"height":4.0_f32,"floor_y":3.0_f32}),
        );
    let upper = leaks(&raw)?;
    assert_eq!(
        upper.len(),
        1,
        "the sealed lower room cannot hide an unguarded upper-floor leak"
    );
    assert!(
        upper.iter().all(|entry| entry.id == "room 1"
            && entry
                .position
                .get(1)
                .is_some_and(|height| *height > 2.9_f32)),
        "upper-floor evidence must use that room's own height"
    );
    Ok(())
}

#[test]
fn measured_surfaces_survive_reports_without_leaking_from_suppressed_warnings() -> Result<(), String>
{
    let level: LevelDef = serde_json::from_value(serde_json::json!({
        "schema":"places-level","format_version":1_u32,"id":"surface_highlight","name":"Tiny pair",
        "spawn":{"x":0.0_f32,"z":0.0_f32},"rooms":[{"width":2.0_f32,"depth":2.0_f32}],
        "geometry_intent":[{"x":0.0_f32,"z":0.0_f32,"width":2.0_f32,"depth":2.0_f32,"check":"overlap-emission"}]
    })).map_err(|error| error.to_string())?;
    let triangle = Tri {
        key: SurfaceKey::new(SurfaceKind::Floor, 0),
        points: [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]],
        normal: [0.0, 1.0, 0.0],
        centroid: [0.333, 0.0, 0.333],
        area: 0.5,
    };
    let mut checker = Checker::new(&level);
    checker.push(
        "sentinel",
        Severity::Warning,
        "mesh",
        "unrelated prior finding",
        [3.0; 3],
    );
    checker.push_surfaces(
        "overlap-emission",
        Severity::Warning,
        "Floor m0",
        "intentional paired floor",
        [&triangle, &triangle],
    );
    assert_eq!(
        checker.findings.len(),
        1,
        "annotated overlap must remain suppressed"
    );
    assert!(
        checker
            .findings
            .first()
            .is_some_and(|finding| finding.surfaces.is_empty()),
        "suppressed surface findings must never alter the previous record"
    );
    checker.push_surfaces(
        "duplicate-surface",
        Severity::Error,
        "Wall m0",
        "measured pair",
        [&triangle, &triangle],
    );
    let measured = checker.findings.last().ok_or("missing measured finding")?;
    assert_eq!(
        measured.surfaces,
        vec![triangle.points, triangle.points],
        "reported surface highlights must preserve both exact measured triangles"
    );
    let encoded = serde_json::to_value(measured).map_err(|error| error.to_string())?;
    assert!(
        encoded.get("surfaces").is_some(),
        "additive nonempty surface payload must survive protocol serialization"
    );
    Ok(())
}

// The menu's basin exposed duplicated structural bands over ordinary terrain
// skirts. Two 4 m² recesses protect deep and shallow steps without assets,
// transport solving, GPU work or a full shipped-map bake (milliseconds).
#[test]
fn recessed_floor_skirt_owns_the_upper_structural_band() -> Result<(), String> {
    for offset in [-0.65_f32, -0.08_f32] {
        let level:LevelDef=serde_json::from_value(serde_json::json!({
            "schema":"places-level","format_version":1_u32,"id":"recess_band","name":"Recess band",
            "spawn":{"x":0.25_f32,"z":0.25_f32},"rooms":[{"width":2.0_f32,"depth":2.0_f32}],
            "floor_regions":[{"x":0.5_f32,"z":0.5_f32,"width":1.0_f32,"depth":1.0_f32,"offset_y":offset}]
        })).map_err(|error|error.to_string())?;
        let report = super::check_level(&level, "in-memory recess", true);
        assert!(
            !report
                .findings
                .iter()
                .any(|entry| entry.check == "duplicate-surface"),
            "terrain and backing must have one face owner at offset {offset}"
        );
    }
    Ok(())
}
