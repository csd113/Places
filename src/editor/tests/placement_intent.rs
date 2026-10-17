//! Three tiny document fixtures protect user-authored exceptions: persistence
//! with issue/instance isolation, context invalidation plus undo, and the shared
//! correction gateway's individual/batch protection and clear eligibility.
//! These add coverage absent from `geometry_intent`'s coordinate suppression and
//! cost a few temporary source writes; no models, renderer or map bake load.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

use super::{Document, ObjectId, annotate_report, ensure_automatic_edit_allowed, validate_raw};
use crate::editor::safety::{SafetyCategory, SafetyFinding, SafetyReport, SafetySeverity};
use crate::engine::spatial::Aabb;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "places-intent-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).map_err(|error| error.to_string())?;
        let project = path.join("project");
        let mut document = Document::create(&project, "intent", "Placement intent")?;
        let _first_placement = document.add(
            super::super::ObjectKind::Prop,
            [1.0_f32, -0.2_f32, 1.0_f32],
            Some("core:desk"),
        )?;
        let _second_placement = document.add(
            super::super::ObjectKind::Prop,
            [4.0_f32, 0.0_f32, 4.0_f32],
            Some("core:desk"),
        )?;
        document.save()?;
        Ok(Self(path))
    }

    fn open(&self) -> Result<Document, String> {
        Document::open(&self.0.join("project"))
    }

    fn replace_source(&self, raw: &Value) -> Result<(), String> {
        fs::write(
            self.0.join("project/levels/intent.json"),
            serde_json::to_vec(raw).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

fn finding(index: usize, check: &'static str) -> SafetyFinding {
    let x = if index == 0 { 1.0_f32 } else { 4.0_f32 };
    let bounds = Aabb {
        min: [x - 0.5_f32, -0.2_f32, x - 0.5_f32],
        max: [x + 0.5_f32, 1.0_f32, x + 0.5_f32],
    };
    SafetyFinding {
        check,
        category: SafetyCategory::Collision,
        severity: SafetySeverity::Warning,
        title: format!("Desk {} collision review", index.saturating_add(1)),
        risk: "Fixture placement warning".into(),
        position: [x, 0.0_f32, x],
        bounds,
        objects: vec![ObjectId::Prop(index)],
        surfaces: Vec::new(),
        fix: None,
        placement_ignore: None,
    }
}

fn reviewed(raw: &Value) -> SafetyReport {
    let mut report = SafetyReport {
        findings: vec![
            finding(0, "desk-overlap"),
            finding(1, "desk-overlap"),
            finding(0, "desk-support"),
        ],
        limitations: String::new(),
    };
    annotate_report(raw, &mut report);
    report
}

fn active(report: &SafetyReport) -> usize {
    report
        .findings
        .iter()
        .filter(|entry| {
            entry
                .placement_ignore
                .as_ref()
                .is_some_and(|state| state.active)
        })
        .count()
}

#[test]
fn placement_ignore_persists_isolates_instances_and_clears_with_undo() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut document = fixture.open()?;
    document.ignore_safety_finding(&finding(0, "desk-overlap"))?;
    let protected = document.raw().clone();
    assert_eq!(
        active(&reviewed(&protected)),
        1,
        "only the exact warning and desk are ignored"
    );
    document.save()?;
    let mut reopened = fixture.open()?;
    assert_eq!(
        reopened.raw(),
        &protected,
        "stable identity/context survives save and reopen"
    );
    let report = reviewed(reopened.raw());
    assert!(
        report
            .findings
            .iter()
            .filter(
                |entry| entry.objects == vec![ObjectId::Prop(1)] || entry.check == "desk-support"
            )
            .all(|entry| entry.placement_ignore.is_none()),
        "another desk and an unrelated warning on the ignored desk remain visible"
    );
    let key = report
        .findings
        .iter()
        .find_map(|entry| {
            entry
                .placement_ignore
                .as_ref()
                .map(|state| state.key.clone())
        })
        .ok_or("saved key")?;
    reopened.clear_safety_ignore(&key)?;
    assert_eq!(
        active(&reviewed(reopened.raw())),
        0,
        "clear restores warning eligibility"
    );
    assert!(reopened.undo(), "clear-ignore is undoable");
    assert_eq!(
        reopened.raw(),
        &protected,
        "undo restores exactly the protected source"
    );
    let mut reordered = reopened.raw().clone();
    reordered
        .get_mut("props")
        .and_then(Value::as_array_mut)
        .ok_or("prop records")?
        .reverse();
    let mut same_placement = finding(0, "desk-overlap");
    same_placement.objects = vec![ObjectId::Prop(1)];
    let mut moved_index = SafetyReport {
        findings: vec![same_placement],
        limitations: String::new(),
    };
    annotate_report(&reordered, &mut moved_index);
    assert_eq!(
        active(&moved_index),
        1,
        "stable identity survives array reordering without transferring intent to the other instance"
    );
    let copied = reopened.duplicate(ObjectId::Prop(0))?;
    assert_ne!(copied, ObjectId::Prop(0), "copy is a separate placement");
    validate_raw(reopened.raw())?;
    let mut duplicate_ids = protected;
    let identity = duplicate_ids
        .pointer("/props/0/editor_instance_id")
        .cloned()
        .ok_or("instance identity")?;
    let record = duplicate_ids
        .pointer_mut("/props/1")
        .and_then(Value::as_object_mut)
        .ok_or("other prop")?;
    drop(record.insert("editor_instance_id".into(), identity));
    assert!(
        validate_raw(&duplicate_ids).is_err(),
        "duplicate stable IDs cannot inherit another placement's ignore"
    );
    fixture.replace_source(&duplicate_ids)?;
    assert!(
        fixture.open().is_err(),
        "malformed duplicate identities are rejected on source reopen too"
    );
    Ok(())
}

#[test]
fn material_geometry_and_full_rotation_changes_reactivate_review() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut document = fixture.open()?;
    document.ignore_safety_finding(&finding(0, "desk-overlap"))?;
    let initial = document.raw().clone();
    let mut tilted = document.transform(ObjectId::Prop(0))?;
    tilted.pitch_degrees = 90.0_f32;
    tilted.roll_degrees = 12.0_f32;
    *tilted.position.get_mut(1).ok_or("Y component")? = -0.4_f32;
    document.set_transform(ObjectId::Prop(0), tilted)?;
    assert_eq!(
        active(&reviewed(document.raw())),
        0,
        "rotation/embedding materially changes the placement context"
    );
    document.save()?;
    let reopened = fixture.open()?;
    assert_eq!(
        reopened.transform(ObjectId::Prop(0))?,
        tilted,
        "arbitrary tilt and deliberate embedding survive save/reopen"
    );
    assert!(document.undo(), "manual rotation can be undone");
    assert_eq!(
        document.raw(),
        &initial,
        "undo restores the original raw context"
    );
    assert_eq!(
        active(&reviewed(document.raw())),
        1,
        "original ignore becomes active again after undo"
    );
    let mut room = document.transform(ObjectId::Room(0))?;
    *room.position.get_mut(1).ok_or("floor Y component")? = 0.25_f32;
    document.set_transform(ObjectId::Room(0), room)?;
    assert_eq!(
        active(&reviewed(document.raw())),
        0,
        "a supporting floor edit invalidates the affected context"
    );
    assert!(
        reviewed(document.raw())
            .findings
            .iter()
            .any(|entry| entry.check == "saved-placement-ignore"
                && entry
                    .placement_ignore
                    .as_ref()
                    .is_some_and(|state| !state.active)),
        "inactive decisions remain reviewable and clearable"
    );
    Ok(())
}

#[test]
fn automatic_gateway_protects_ignored_placement_and_support_until_clear() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut document = fixture.open()?;
    document.ignore_safety_finding(&finding(0, "desk-overlap"))?;
    let original = document.raw().clone();
    let mut candidates = Vec::new();
    let mut moved = original.clone();
    let x = moved.pointer_mut("/props/0/x").ok_or("desk x")?;
    *x = serde_json::json!(1.5_f32);
    candidates.push(moved);
    let mut changed_floor = original.clone();
    let floor = changed_floor.pointer_mut("/rooms/0/floor_y");
    if let Some(value) = floor {
        *value = serde_json::json!(0.2_f32);
    } else {
        let room = changed_floor
            .pointer_mut("/rooms/0")
            .and_then(Value::as_object_mut)
            .ok_or("room")?;
        drop(room.insert("floor_y".into(), serde_json::json!(0.2_f32)));
    }
    candidates.push(changed_floor);
    for candidate in &candidates {
        assert!(
            ensure_automatic_edit_allowed(&original, candidate).is_err(),
            "single correction cannot alter protected placement or support"
        );
        assert!(
            document
                .apply_checked_edit(&original, candidate.clone())
                .is_err(),
            "final apply checks independently of preview"
        );
    }
    assert!(
        !candidates
            .iter()
            .any(|candidate| ensure_automatic_edit_allowed(&original, candidate).is_ok()),
        "a bulk pass must skip every protected correction"
    );
    assert_eq!(
        document.raw(),
        &original,
        "failed individual and batch attempts leave the intentional desk exact"
    );
    let mut malformed = original.clone();
    drop(
        malformed
            .pointer_mut("/props/0")
            .and_then(Value::as_object_mut)
            .ok_or("desk record")?
            .insert("scale".into(), serde_json::json!(-1.0_f32)),
    );
    assert!(
        document
            .apply_checked_edit(&original, malformed.clone())
            .is_err(),
        "ignores cannot bypass semantic malformed-data validation"
    );
    fixture.replace_source(&malformed)?;
    assert!(
        fixture.open().is_err(),
        "source loading still rejects a malformed physics/geometry value in an ignored placement"
    );
    let key = reviewed(&original)
        .findings
        .iter()
        .find_map(|entry| {
            entry
                .placement_ignore
                .as_ref()
                .map(|state| state.key.clone())
        })
        .ok_or("saved ignore")?;
    document.clear_safety_ignore(&key)?;
    let mut eligible_again = document.raw().clone();
    *eligible_again.pointer_mut("/props/0/x").ok_or("desk x")? = serde_json::json!(1.5_f32);
    ensure_automatic_edit_allowed(document.raw(), &eligible_again)?;
    let expected = document.raw().clone();
    document.apply_checked_edit(&expected, eligible_again)?;
    assert!(
        document
            .transform(ObjectId::Prop(0))?
            .position
            .first()
            .is_some_and(|position_x| (*position_x - 1.5_f32).abs() < f32::EPSILON),
        "cleared ignore restores correction eligibility"
    );
    Ok(())
}

// A neighboring model's true tilted envelope can reach the protected placement
// while its unrotated footprint does not. This tiny regression closes that
// distinct dependency hole using two explicit boxes and no model discovery.
#[test]
fn neighboring_tilted_envelope_invalidates_the_affected_ignore() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut document = fixture.open()?;
    let expected = document.raw().clone();
    let mut initial = expected.clone();
    let neighbor = initial
        .pointer_mut("/props/1")
        .and_then(Value::as_object_mut)
        .ok_or("neighbor")?;
    for (key, value) in [
        ("x", serde_json::json!(1.0_f32)),
        ("z", serde_json::json!(3.0_f32)),
        ("size", serde_json::json!([0.4_f32, 2.0_f32, 0.4_f32])),
        ("pitch_degrees", serde_json::json!(-90.0_f32)),
    ] {
        drop(neighbor.insert(key.into(), value));
    }
    document.apply_checked_edit(&expected, initial)?;
    document.ignore_safety_finding(&finding(0, "desk-overlap"))?;
    assert_eq!(
        active(&reviewed(document.raw())),
        1,
        "initial neighbor contact context is captured"
    );
    let mut transform = document.transform(ObjectId::Prop(1))?;
    transform.roll_degrees = 10.0_f32;
    document.set_transform(ObjectId::Prop(1), transform)?;
    assert_eq!(
        active(&reviewed(document.raw())),
        0,
        "changing a reaching tilted neighbor reactivates the protected issue"
    );
    Ok(())
}
