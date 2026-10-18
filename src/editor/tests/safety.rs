//! Four small regression fixtures protect essential safety boundaries beyond
//! the document tests: stale/unknown-data fix refusal, real coplanarity versus
//! artistic intersections, and crouched jump clearance at ramp sides. They
//! operate on tiny in-memory scenes/triangles, with no assets, render or bake.
//! The budget fixture repeats one room record only to prove large worker jobs
//! are refused before any asset discovery or generated geometry allocation.

use glam::Vec3;
use serde_json::Value;

use super::{SafetyReport, analyze, boundaries, duplicate_wall_fixes, models};
use crate::engine::level::LevelDef;
use crate::engine::materials::AlphaMode;

fn raw_scene() -> Value {
    serde_json::json!({
        "schema":"places-level", "format_version":1_u32, "id":"safety", "name":"Safety fixture",
        "spawn":{"x":0.5_f32,"z":0.5_f32},
        "rooms":[{"x":0.0_f32,"z":0.0_f32,"width":10.0_f32,"depth":10.0_f32,"height":6.0_f32}],
        "walls":[
            {"x":0.0_f32,"z":0.0_f32,"width":10.0_f32,"depth":0.2_f32,"height":5.0_f32},
            {"x":0.0_f32,"z":0.0_f32,"width":10.0_f32,"depth":0.2_f32,"height":5.0_f32}
        ],
        "future_data":{"preserve":[true,7_u32,"opaque"]}
    })
}

fn report() -> SafetyReport {
    SafetyReport {
        findings: Vec::new(),
        limitations: String::new(),
    }
}

#[test]
fn duplicate_fix_preserves_source_and_refuses_stale_or_unknown_records() -> Result<(), String> {
    let raw = raw_scene();
    let level: LevelDef = serde_json::from_value(raw.clone()).map_err(|error| error.to_string())?;
    let mut findings = report();
    duplicate_wall_fixes(&level, &raw, &mut findings);
    let fix = findings
        .findings
        .first()
        .and_then(|finding| finding.fix.as_ref())
        .ok_or("missing duplicate fix")?;
    let preview = fix.preview(&raw)?;
    assert_eq!(
        raw.get("future_data"),
        preview.get("future_data"),
        "a conservative fix must preserve unknown source data"
    );
    assert_eq!(
        raw.get("walls").and_then(Value::as_array).map(Vec::len),
        Some(2),
        "preview must not mutate its source"
    );
    assert_eq!(
        preview.get("walls").and_then(Value::as_array).map(Vec::len),
        Some(1),
        "preview removes exactly one wall"
    );
    let mut stale = raw.clone();
    let _old_name = stale.as_object_mut().ok_or("scene object")?.insert(
        "name".into(),
        Value::String("changed after analysis".into()),
    );
    assert!(
        fix.preview(&stale).is_err(),
        "a fix must reject every intervening source edit"
    );
    let mut unknown = raw.clone();
    for wall in unknown
        .get_mut("walls")
        .and_then(Value::as_array_mut)
        .ok_or("walls")?
    {
        let _old_field = wall
            .as_object_mut()
            .ok_or("wall")?
            .insert("future_wall_id".into(), Value::String("preserve".into()));
    }
    let unknown_level =
        serde_json::from_value(unknown.clone()).map_err(|error| error.to_string())?;
    let mut protected = report();
    duplicate_wall_fixes(&unknown_level, &unknown, &mut protected);
    assert_eq!(
        protected.findings.len(),
        1,
        "unknown duplicate records remain visible for manual review"
    );
    assert!(
        protected
            .findings
            .iter()
            .all(|finding| finding.fix.is_none()),
        "unknown metadata must disable deletion"
    );
    Ok(())
}

#[test]
fn model_depth_check_requires_real_parallel_positive_area_overlap() {
    let flat = [
        Vec3::ZERO,
        Vec3::new(2.0_f32, 0.0_f32, 0.0_f32),
        Vec3::new(0.0_f32, 0.0_f32, 2.0_f32),
    ];
    let same =
        flat.map(|point| Vec3::new(point.x + 0.2_f32, point.y + 0.001_f32, point.z + 0.2_f32));
    let area = models::coplanar_overlap(flat, same).unwrap_or(0.0_f32);
    assert!(
        area > 1.0_f32,
        "near-coplanar positive area sheets must be detected"
    );
    let crossing = [
        Vec3::new(0.5_f32, -1.0_f32, 0.5_f32),
        Vec3::new(0.5_f32, 1.0_f32, 0.5_f32),
        Vec3::new(1.5_f32, 0.0_f32, 1.5_f32),
    ];
    assert!(
        models::coplanar_overlap(flat, crossing).is_none(),
        "an artistic transverse intersection is not a coplanar depth conflict"
    );
    let distant = flat.map(|point| Vec3::new(point.x, point.y + 0.01_f32, point.z));
    assert!(
        models::coplanar_overlap(flat, distant).is_none(),
        "parallel planes separated by a centimetre must not be called coplanar"
    );
    let disjoint = flat.map(|point| Vec3::new(point.x + 3.0_f32, point.y, point.z));
    assert!(
        models::coplanar_overlap(flat, disjoint)
            .is_none_or(|disjoint_area| disjoint_area <= 0.0001_f32),
        "plane coincidence alone is not triangle overlap"
    );
}

// Essential regressions from the finished games: catalog feet must be
// distinguished from visible-sheet conflicts, without hiding duplicate props
// or a thin sheet over the floor. Four tiny triangles need no installed assets.
#[test]
fn model_support_contacts_keep_duplicate_props_and_thin_sheets_visible() -> Result<(), String> {
    let flat = [Vec3::ZERO, Vec3::X, Vec3::Z];
    let volume = [Vec3::ZERO, Vec3::Y, Vec3::X];
    let make = |points, prop, floor| {
        models::Triangle::new(points, prop, AlphaMode::Opaque, true, floor)
            .ok_or_else(|| "nondegenerate fixture triangle".to_owned())
    };
    let mut resting = vec![
        make(flat, None, true)?,
        make(flat, Some(0), false)?,
        make(volume, Some(0), false)?,
    ];
    let mut support = report();
    models::compare_with_budget(&mut resting, &mut support, 1_000);
    assert!(
        support
            .findings
            .iter()
            .any(|entry| entry.check == "model-floor-contacts")
    );
    assert!(
        !support
            .findings
            .iter()
            .any(|entry| entry.check == "model-coplanar-surface")
    );
    resting.extend([make(flat, Some(1), false)?, make(volume, Some(1), false)?]);
    let mut duplicate = report();
    models::compare_with_budget(&mut resting, &mut duplicate, 1_000);
    assert!(
        duplicate
            .findings
            .iter()
            .any(|entry| entry.check == "model-coplanar-surface" && entry.objects.len() == 2),
        "a duplicated prop remains a real triangle finding even when both bases rest on the floor"
    );
    let mut sheets = vec![make(flat, None, true)?, make(flat, Some(0), false)?];
    let mut thin = report();
    models::compare_with_budget(&mut sheets, &mut thin, 1_000);
    assert!(
        thin.findings
            .iter()
            .any(|entry| entry.check == "model-coplanar-surface"),
        "an opaque sheet without model volume above is still a floor duplicate"
    );
    Ok(())
}

// A same-X distant stack previously consumed the pair budget before reaching
// a nearby real conflict. 258 synthetic triangles now prove spatial rejection
// and explicit incomplete coverage with a tiny budget and no assets or bake.
#[test]
fn model_spatial_budget_reaches_nearby_pairs_and_reports_incomplete_work() -> Result<(), String> {
    let mut triangles = (0_u16..256)
        .map(|index| {
            let y = f32::from(index) * 10.0;
            models::Triangle::new(
                [
                    Vec3::new(0.0, y, 0.0),
                    Vec3::new(1.0, y, 0.0),
                    Vec3::new(0.0, y, 1.0),
                ],
                Some(usize::from(index)),
                AlphaMode::Opaque,
                false,
                false,
            )
            .ok_or_else(|| "fixture triangle".to_owned())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let duplicate = [
        Vec3::new(0.0, 3_000.0, 0.0),
        Vec3::new(1.0, 3_000.0, 0.0),
        Vec3::new(0.0, 3_000.0, 1.0),
    ];
    for prop in [256, 257] {
        triangles.push(
            models::Triangle::new(duplicate, Some(prop), AlphaMode::Opaque, false, false)
                .ok_or("duplicate triangle")?,
        );
    }
    let mut covered = report();
    models::compare_with_budget(&mut triangles, &mut covered, 4_096);
    assert!(
        covered
            .findings
            .iter()
            .any(|entry| entry.check == "model-coplanar-surface"),
        "nearby real duplicates are not starved by distant same-X triangles"
    );
    assert!(
        !covered
            .findings
            .iter()
            .any(|entry| entry.check == "model-check-incomplete")
    );
    let mut incomplete = report();
    models::compare_with_budget(&mut triangles, &mut incomplete, 1);
    assert!(
        incomplete
            .findings
            .iter()
            .any(|entry| entry.check == "model-check-incomplete"
                && entry.risk.contains("incomplete"))
    );
    Ok(())
}

// Exact authored leaf poses must participate in the sampled jump/drop check.
// One two-metre room proves closed protection and a real open route; no bake.
#[test]
fn floor_edge_review_uses_the_actual_authored_door_pose() -> Result<(), String> {
    let mut raw = serde_json::json!({
        "schema":"places-level","format_version":1_u32,"id":"door_edge","name":"Door edge",
        "spawn":{"x":1.0_f32,"z":1.0_f32},
        "rooms":[{"width":2.0_f32,"depth":2.0_f32,"height":4.0_f32}],
        "walls":[
            {"x":-0.2_f32,"z":0.0_f32,"width":0.2_f32,"depth":2.0_f32,"height":4.0_f32},
            {"x":2.0_f32,"z":0.0_f32,"width":0.2_f32,"depth":2.0_f32,"height":4.0_f32},
            {"x":0.0_f32,"z":2.0_f32,"width":2.0_f32,"depth":0.2_f32,"height":4.0_f32}
        ],
        "doors":[{"id":"edge_leaf","x":0.0_f32,"z":0.0_f32,"width":2.0_f32,"height":3.0_f32}]
    });
    let closed_level = serde_json::from_value(raw.clone()).map_err(|error| error.to_string())?;
    let mut closed = report();
    boundaries::check(&closed_level, &mut closed);
    assert!(
        !closed
            .findings
            .iter()
            .any(|entry| entry.check == "floor-edge-drop"),
        "a closed full-height leaf blocks the sampled edge"
    );
    let door = raw
        .get_mut("doors")
        .and_then(Value::as_array_mut)
        .and_then(|doors| doors.first_mut())
        .and_then(Value::as_object_mut)
        .ok_or("door")?;
    let _state = door.insert("initial_state".into(), Value::String("open".into()));
    let open_level = serde_json::from_value(raw).map_err(|error| error.to_string())?;
    let mut open = report();
    boundaries::check(&open_level, &mut open);
    assert!(
        open.findings
            .iter()
            .any(|entry| entry.check == "floor-edge-drop"),
        "rotating the same leaf open must expose the real route"
    );
    Ok(())
}

#[test]
fn ramp_side_review_sees_jumpable_rims_and_respects_tall_barriers() -> Result<(), String> {
    let mut raw = raw_scene();
    let object = raw.as_object_mut().ok_or("scene object")?;
    let _walls = object.insert("walls".into(), serde_json::json!([]));
    let _ramps = object.insert("ramps".into(),serde_json::json!([{"x":1.0_f32,"z":2.0_f32,"width":6.0_f32,"depth":1.0_f32,"rise":2.0_f32}]));
    let _recess = object.insert("floor_regions".into(),serde_json::json!([{"x":1.0_f32,"z":6.0_f32,"width":2.0_f32,"depth":2.0_f32,"offset_y":-2.0_f32}]));
    let level = serde_json::from_value(raw.clone()).map_err(|error| error.to_string())?;
    let mut open = report();
    boundaries::check(&level, &mut open);
    assert_eq!(
        open.findings
            .iter()
            .filter(|finding| finding.check == "ramp-side-escape")
            .count(),
        2,
        "both unguarded ramp sides must receive a jump/drop review"
    );
    assert!(
        open.findings
            .iter()
            .any(|finding| finding.check == "deep-recess-exit"),
        "a deep recessed basin needs an explicit exit review"
    );
    let _guard_walls = raw.as_object_mut().ok_or("scene object")?.insert(
        "walls".into(),
        serde_json::json!([
            {"x":1.0_f32,"z":1.85_f32,"width":6.0_f32,"depth":0.15_f32,"height":5.0_f32},
            {"x":1.0_f32,"z":3.0_f32,"width":6.0_f32,"depth":0.15_f32,"height":5.0_f32}
        ]),
    );
    let guarded_level = serde_json::from_value(raw).map_err(|error| error.to_string())?;
    let mut guarded = report();
    boundaries::check(&guarded_level, &mut guarded);
    assert!(
        !guarded
            .findings
            .iter()
            .any(|finding| finding.check == "ramp-side-escape"),
        "real tall colliders should block the sampled crouched jump apex"
    );
    Ok(())
}

#[test]
fn oversized_review_returns_explicit_incomplete_result_without_assets() -> Result<(), String> {
    let raw = raw_scene();
    let mut level: LevelDef =
        serde_json::from_value(raw.clone()).map_err(|error| error.to_string())?;
    let room = level.rooms.first().ok_or("fixture room")?.clone();
    level.rooms = vec![room; 129];
    let checked = analyze(&level, &raw);
    assert_eq!(
        checked.findings.len(),
        1,
        "budget refusal must return one explicit limitation, not partial safety claims"
    );
    assert!(
        checked
            .findings
            .iter()
            .all(|finding| finding.check == "analysis-budget" && finding.fix.is_none()),
        "oversized scene must not produce or apply corrections"
    );
    assert!(
        checked
            .findings
            .iter()
            .all(|finding| finding.risk.contains("incomplete")),
        "the visible result must explain that review is incomplete"
    );
    Ok(())
}

// Essential regression: actual auto-fix candidates must share the same intent
// guard in individual and batch paths. Two independent wall pairs are
// sufficient; no renderer, imported assets, or full-map preparation is involved.
#[test]
fn correction_candidates_skip_ignored_walls_and_restore_eligibility() -> Result<(), String> {
    let mut raw = raw_scene();
    let walls = raw
        .get_mut("walls")
        .and_then(Value::as_array_mut)
        .ok_or("walls")?;
    walls.extend([
        serde_json::json!({"x":14.0_f32,"z":4.0_f32,"width":1.0_f32,"depth":0.2_f32,"height":2.0_f32}),
        serde_json::json!({"x":14.0_f32,"z":4.0_f32,"width":1.0_f32,"depth":0.2_f32,"height":2.0_f32}),
    ]);
    let level = serde_json::from_value(raw.clone()).map_err(|error| error.to_string())?;
    let mut initial = report();
    duplicate_wall_fixes(&level, &raw, &mut initial);
    let first = initial.findings.first().ok_or("near duplicate")?;
    let ignored = crate::editor::document::intent::ignore_in_raw(&raw, first)?;
    let ignored_level =
        serde_json::from_value(ignored.clone()).map_err(|error| error.to_string())?;
    let mut findings = report();
    duplicate_wall_fixes(&ignored_level, &ignored, &mut findings);
    crate::editor::document::intent::annotate_report(&ignored, &mut findings);
    let candidates = super::preview_corrections(&ignored, &findings.findings);
    assert_eq!(
        candidates.len(),
        1,
        "batch candidate gate skips the ignored issue and retains the independent far correction"
    );
    assert!(
        candidates.iter().all(Result::is_ok),
        "an independent unprotected correction remains eligible"
    );
    assert!(
        findings
            .findings
            .first()
            .and_then(|entry| entry.fix.as_ref())
            .is_some_and(|fix| fix.preview(&ignored).is_err()),
        "direct individual preview independently protects an ignored placement"
    );
    let mut cleared = ignored;
    drop(
        cleared
            .as_object_mut()
            .ok_or("source object")?
            .remove("editor_placement_intent"),
    );
    let cleared_level =
        serde_json::from_value(cleared.clone()).map_err(|error| error.to_string())?;
    let mut eligible = report();
    duplicate_wall_fixes(&cleared_level, &cleared, &mut eligible);
    assert_eq!(
        super::preview_corrections(&cleared, &eligible.findings)
            .iter()
            .filter(|entry| entry.is_ok())
            .count(),
        2,
        "clearing intent restores both corrections despite stable editor IDs"
    );
    Ok(())
}
