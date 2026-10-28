//! Essential regressions: sub-grid endpoint welding must preserve aperture world
//! coordinates, attachments and exact source history. Original-index correction
//! groups must refuse conflicts rather than editing a shifted/deleted record.
//! Four tiny CPU fixtures; no render, map bake or installed model dependencies.
use super::{Correction, SafetyCategory, SafetyFinding, SafetyFix, SafetySeverity};
use crate::editor::document::{Document, ObjectId};
use crate::engine::level::LevelDef;
use serde_json::{Value, json};
use std::sync::{Arc, atomic::AtomicBool};

fn scene() -> Value {
    json!({"schema":"places-level","format_version":1_u32,"id":"integrity","name":"Integrity",
    "spawn":{"x":4.0_f32,"z":4.0_f32},"rooms":[{"width":8.0_f32,"depth":8.0_f32,"height":3.0_f32}],
    "walls":[{"x":1.01_f32,"z":1.0_f32,"width":2.0_f32,"depth":0.2_f32,"height":3.0_f32,
        "trim":{"placement":"both"},"openings":[{"kind":"door","offset":0.5_f32,"width":1.0_f32,"height":2.2_f32}],"future_wall":{"preserve":true}},
        {"x":0.8_f32,"z":0.5_f32,"width":0.2_f32,"depth":4.0_f32,"height":3.0_f32},
        {"x":5.0_f32,"z":5.0_f32,"width":2.0_f32,"depth":0.2_f32,"height":3.0_f32},
        {"x":5.0_f32,"z":5.0_f32,"width":2.0_f32,"depth":0.2_f32,"height":3.0_f32}],
    "doors":[{"id":"leaf","x":1.55_f32,"z":1.1_f32,"width":0.92_f32,"height":2.15_f32}],
    "future_scene":{"untouched":[true,7_u32]}})
}
fn level(raw: &Value) -> Result<LevelDef, String> {
    serde_json::from_value(raw.clone()).map_err(|e| e.to_string())
}
fn finding(raw: &Value, correction: Correction, objects: Vec<ObjectId>) -> SafetyFinding {
    SafetyFinding {
        check: "regression",
        category: SafetyCategory::GeometryOverlap,
        severity: SafetySeverity::Warning,
        title: "regression".into(),
        risk: String::new(),
        position: [0.0; 3],
        bounds: super::marker_bounds([0.0; 3]),
        objects,
        surfaces: Vec::new(),
        placement_ignore: None,
        fix: Some(SafetyFix {
            title: "regression".into(),
            explanation: String::new(),
            expected: Arc::new(raw.clone()),
            correction,
        }),
    }
}
#[test]
fn endpoint_weld_keeps_opening_world_position_and_atomic_saved_history() -> Result<(), String> {
    let raw = scene();
    let parsed = level(&raw)?;
    let issue = crate::map_compiler::geometry_check::construction::endpoints(&parsed)
        .into_iter()
        .find(|i| i.wall == 0 && i.start)
        .ok_or("missing start crack")?;
    let correction = finding(
        &raw,
        Correction::Endpoint(issue),
        vec![ObjectId::Wall(0), ObjectId::Wall(1)],
    );
    let candidate = correction.fix.as_ref().ok_or("fix")?.preview(&raw)?;
    let changed = level(&candidate)?;
    let wall = changed.walls.first().ok_or("wall")?;
    let aperture = wall.openings.first().ok_or("aperture")?;
    assert!(
        (wall.x + aperture.offset - 1.51).abs() < 0.0001,
        "sub-15mm edits must rebase the aperture instead of moving its leaf"
    );
    assert_eq!(
        raw.get("doors"),
        candidate.get("doors"),
        "endpoint resize keeps the functional door fixed"
    );
    assert_eq!(
        raw.get("future_scene"),
        candidate.get("future_scene"),
        "unknown root metadata survives"
    );
    assert_eq!(
        raw.pointer("/walls/0/future_wall"),
        candidate.pointer("/walls/0/future_wall"),
        "unknown wall metadata survives"
    );
    assert_eq!(
        raw.pointer("/walls/0/trim"),
        candidate.pointer("/walls/0/trim"),
        "trim remains attached"
    );
    let root =
        std::env::temp_dir().join(format!("places-integrity-history-{}", std::process::id()));
    let mut document = Document::create(&root, "integrity", "Integrity")?;
    let result = (|| {
        document.apply_authoring_edit(&document.raw().clone(), raw.clone())?;
        document.apply_checked_edit(&raw, candidate.clone())?;
        assert!(
            document
                .apply_checked_edit(&raw, candidate.clone())
                .is_err(),
            "repeated stale Apply refuses"
        );
        assert!(document.undo(), "one undo restores the complete correction");
        assert_eq!(document.raw(), &raw, "undo is exact");
        assert!(document.redo(), "one redo restores the complete correction");
        assert_eq!(document.raw(), &candidate, "redo is exact");
        document.save_all()?;
        let reopened = Document::open(&root)?;
        assert_eq!(reopened.raw(), &candidate, "save/reopen is exact");
        assert!(
            !crate::map_compiler::geometry_check::construction::endpoints(reopened.level())
                .iter()
                .any(|i| i.wall == 0 && i.start),
            "reanalysis cannot offer the same weld again"
        );
        Ok(())
    })();
    let _cleanup = std::fs::remove_dir_all(&root);
    result
}
#[test]
fn original_index_groups_combine_deletions_and_refuse_mixed_conflicts() -> Result<(), String> {
    let raw = scene();
    let issue = crate::map_compiler::geometry_check::construction::endpoints(&level(&raw)?)
        .into_iter()
        .find(|i| i.wall == 0 && i.start)
        .ok_or("crack")?;
    let weld = finding(
        &raw,
        Correction::Endpoint(issue),
        vec![ObjectId::Wall(0), ObjectId::Wall(1)],
    );
    let remove = finding(
        &raw,
        Correction::RemoveWall(3),
        vec![ObjectId::Wall(2), ObjectId::Wall(3)],
    );
    let result = super::preview_group(&raw, &[&weld, &remove])?;
    assert_eq!(
        result.get("walls").and_then(Value::as_array).map(Vec::len),
        Some(3),
        "group deletes the original redundant wall"
    );
    assert_eq!(
        result.pointer("/walls/2"),
        raw.pointer("/walls/2"),
        "untouched survivor keeps its exact data"
    );
    let conflict = finding(
        &raw,
        Correction::Fields(vec![("/walls/0/width".into(), json!(2.5_f32))]),
        vec![ObjectId::Wall(0)],
    );
    assert!(
        super::preview_group(&raw, &[&weld, &remove, &conflict]).is_err(),
        "a mixed conflicting group is wholly refused"
    );
    let mut locked = raw;
    *locked.pointer_mut("/walls/0").ok_or("wall")? = json!({"x":1.01_f32,"z":1.0_f32,"width":2.0_f32,"depth":0.2_f32,"height":3.0_f32,"editor_locked":true});
    let blocked = finding(
        &locked,
        Correction::Fields(vec![("/walls/0/width".into(), json!(2.5_f32))]),
        vec![ObjectId::Wall(0)],
    );
    assert!(
        super::preview_group(&locked, &[&blocked]).is_err(),
        "locked source records cannot enter a correction preview"
    );
    Ok(())
}
#[test]
fn correct_contacts_and_authored_apertures_are_not_corner_defects() -> Result<(), String> {
    let mut raw = scene();
    *raw.pointer_mut("/walls/0/x").ok_or("x")? = json!(0.9_f32);
    assert!(
        crate::map_compiler::geometry_check::construction::endpoints(&level(&raw)?).is_empty(),
        "an endpoint inside carrier thickness is a valid L/T contact"
    );
    *raw.pointer_mut("/walls/0/x").ok_or("x")? = json!(1.01_f32);
    *raw.pointer_mut("/walls/1").ok_or("carrier")? = json!({"x":0.8_f32,"z":0.5_f32,"width":0.2_f32,"depth":4.0_f32,"height":3.0_f32,"openings":[{"kind":"passage","offset":0.3_f32,"width":1.0_f32,"height":2.2_f32}]});
    assert!(
        crate::map_compiler::geometry_check::construction::endpoints(&level(&raw)?).is_empty(),
        "a carrier aperture is intentional empty space, not a solid weld target"
    );
    let mut near_corner = scene();
    *near_corner
        .pointer_mut("/walls/1/x")
        .ok_or("carrier position")? = json!(0.65_f32);
    assert!(
        crate::map_compiler::geometry_check::construction::endpoints(&level(&near_corner)?)
            .iter()
            .any(|issue| issue.wall == 0
                && issue.start
                && issue.check == "wall-corner-review"
                && !issue.auto_repairable),
        "gaps beyond the weld tolerance must remain visible without an automatic target"
    );
    let cancelled = super::analyze_cancellable(&level(&raw)?, &raw, &AtomicBool::new(true));
    assert!(
        cancelled.findings.is_empty(),
        "cancelled work cannot manufacture partial findings or corrections"
    );
    Ok(())
}

// Two tiny room footprints protect a specific false positive: their gap is
// ordinary wall thickness until an actual doorway slice exposes its floor.
// This reuses the production checker/emitter clip; CPU only, no model or bake.
#[test]
fn floor_gap_review_clips_solid_carriers_and_localizes_an_opening() -> Result<(), String> {
    let mut raw = json!({"schema":"places-level","format_version":1_u32,"id":"floor_gap","name":"Floor gap",
        "spawn":{"x":1.0_f32,"z":1.0_f32},
        "rooms":[{"width":2.0_f32,"depth":2.0_f32,"height":3.0_f32},{"x":2.2_f32,"width":2.0_f32,"depth":2.0_f32,"height":3.0_f32}],
        "walls":[{"x":2.0_f32,"z":0.0_f32,"width":0.2_f32,"depth":2.0_f32,"height":3.0_f32,"trim":{"placement":"none"}}]});
    let solid =
        crate::map_compiler::geometry_check::check_level(&level(&raw)?, "solid carrier", true);
    assert!(
        !solid
            .findings
            .iter()
            .any(|finding| finding.check == "floor-footprint-crack"),
        "wall thickness between rooms is supported geometry, not a void crack"
    );
    let record = raw
        .pointer_mut("/walls/0")
        .and_then(Value::as_object_mut)
        .ok_or("wall")?;
    drop(record.insert(
        "openings".into(),
        json!([{"kind":"door","offset":0.5_f32,"width":1.0_f32,"height":2.2_f32}]),
    ));
    let open =
        crate::map_compiler::geometry_check::check_level(&level(&raw)?, "exposed aperture", true);
    let crack = open
        .findings
        .iter()
        .find(|finding| finding.check == "floor-footprint-crack")
        .ok_or("missing floor crack through opening")?;
    assert!(
        (crack.position[0] - 2.1).abs() < 0.001 && (crack.position[2] - 1.0).abs() < 0.001,
        "the marker belongs in the uncovered aperture, not inside the supporting wall"
    );
    Ok(())
}
