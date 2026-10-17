//! One tiny in-memory triangle-pair test protects the new measured highlight
//! payload and a specific regression risk: a suppressed warning must not
//! attach its geometry to the previous finding. No map, assets or bake run.

use super::{Checker, Severity, SurfaceKey, SurfaceKind, Tri};
use crate::engine::level::LevelDef;

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
