//! The editor's unique boundary is preserving exact gameplay wiring through
//! authoring, undo, duplication and save/reopen. One tiny scene protects those
//! operations and stale-reference rejection without a bake, renderer or audio
//! device; its cost is a few JSON writes and shared semantic validation.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;

use crate::editor::document::{Document, GameplaySetting as S, ObjectId, ObjectKind};
use crate::engine::assets::AssetCatalog;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self, String> {
        Self::with_level(
            &json!({"schema":"places-level","format_version":1_u32,"id":"entry","name":"Tiny gameplay","spawn":{"x":1_u32,"z":1_u32},"rooms":[{"width":8_u32,"depth":8_u32,"height":3_u32}],"props":[{"model":"core:chair","x":2_u32,"z":2_u32,"size":[0.5_f32,1_u32,0.5_f32],"future_capability":{"preserve":true}}]}),
        )
    }

    fn with_level(level: &serde_json::Value) -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "places-gameplay-editor-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).map_err(|e| e.to_string())?;
        fs::create_dir(path.join("levels")).map_err(|e| e.to_string())?;
        fs::write(path.join("places.project.json"),serde_json::to_vec(&json!({"schema":"places-project","project_format":1_u32,"id":"gameplay","name":"Tiny gameplay","entry_level":"entry","levels":[{"id":"entry","source":"levels/entry.json"}]})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        fs::write(
            path.join("levels/entry.json"),
            serde_json::to_vec(level).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _removed = fs::remove_dir_all(&self.0);
    }
}

// Compare the native Power setter with a staged draft, preserving authored
// values, effective intensity and the whole undo snapshot. A tiny fixture
// also exercises non-finite rejection without a renderer or full map bake.
#[test]
fn native_light_power_staging_preserves_values_and_one_step_history() -> Result<(), String> {
    let fixture = Fixture::with_level(&json!({
        "schema":"places-level","format_version":1_u32,"id":"entry","name":"Light power",
        "spawn":{"x":1_u32,"z":1_u32},"rooms":[{"width":8_u32,"depth":8_u32,"height":3_u32}],
        "ceiling_lights":[
            {"id":"target","fixture":"core:fluorescent_panel_01","x":2_u32,"z":3_u32,"range":6_u32},
            {"id":"neighbor","fixture":"core:fluorescent_panel_01","x":4_u32,"z":3_u32,"brightness":0.75_f32}
        ]
    }))?;
    let mut doc = Document::open(&fixture.0)?;
    let catalog = AssetCatalog::builtin();
    let selected = ObjectId::Light(0);
    let before = doc.raw().clone();
    assert_eq!(doc.gameplay_value(selected, S::Brightness), json!(1.0_f32));

    let mut staged = before.clone();
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_eq!(
            Document::stage_light_power(&mut staged, 0, invalid)
                .err()
                .as_deref(),
            Some("Enter a finite number")
        );
        assert!(
            doc.set_gameplay_value(selected, S::Brightness, json!(invalid), &catalog)
                .is_err(),
            "invalid numeric input must not reach the commit gateway"
        );
    }
    assert_eq!(staged, before);
    assert!(
        doc.set_gameplay_value(selected, S::Brightness, json!(-0.5_f32), &catalog)
            .is_err(),
        "the native document validator rejects negative Power"
    );
    assert_eq!(doc.raw(), &before, "rejected input preserves the scene");
    assert!(!doc.is_dirty() && !doc.can_undo() && !doc.can_redo());

    // An accepted edit preserves the exact authored value and leaves the
    // rest of the scene unchanged, including the second light.
    let power = 2.75_f32;
    let mut expected = before.clone();
    let target = expected
        .pointer_mut("/ceiling_lights/0")
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("missing target light")?;
    drop(target.insert("brightness".into(), json!(power)));
    Document::stage_light_power(&mut staged, 0, power)?;
    assert_eq!(staged, expected);
    doc.set_gameplay_value(selected, S::Brightness, json!(power), &catalog)?;
    assert_eq!(doc.raw(), &expected);
    assert_eq!(doc.gameplay_value(selected, S::Brightness), json!(power));

    assert!(doc.is_dirty() && doc.undo(), "one Undo restores the source");
    assert_eq!(doc.raw(), &before);
    assert!(!doc.is_dirty() && !doc.can_undo());
    assert!(doc.redo(), "one Redo restores the accepted edit");
    assert_eq!(doc.raw(), &expected);
    assert!(!doc.can_redo());
    Ok(())
}

// A tiny wall and dummy fixture records protect the Basic wall candidate and
// the master-circuit limit, without baking a shipped map or making GPU calls.
#[test]
fn electrical_panel_basic_wall_mount_and_full_circuit_roundtrip() -> Result<(), String> {
    use crate::editor::electrical_panel::fit_candidate;
    use glam::Vec3;

    let lights: Vec<_> = (0..27_u16).map(|i| {
        json!({"id":format!("fixture_{i}"),"fixture":"core:fluorescent_panel_01","x":2_u32,"z":3_u32})
    }).collect();
    let fixture = Fixture::with_level(&json!({
        "schema":"places-level","format_version":1_u32,"id":"entry","name":"Basic cabinet",
        "spawn":{"x":1_u32,"z":3_u32},"rooms":[{"width":8_u32,"depth":8_u32,"height":3_u32}],
        "walls":[{"x":1_u32,"y":0_u32,"z":1_u32,"width":6_u32,"depth":0.2_f32}],
        "ceiling_lights": lights
    }))?;
    let mut doc = Document::open(&fixture.0)?;
    let before = doc.raw().clone();
    let ray = (Vec3::new(2.0, 1.5, 2.0), Vec3::NEG_Z);
    let (raw, id) = fit_candidate(&doc, 0, ray, None)?;
    assert_eq!(id, ObjectId::Prop(0), "one authored cabinet");
    doc.apply_authoring_edit(&before, raw)?;
    assert!(
        fit_candidate(&doc, 0, ray, None).is_err(),
        "a second box cannot overlap"
    );
    let before_wire = doc.raw().clone();
    doc.assign_circuit(&(0..lights.len()).collect::<Vec<_>>(), 0)?;
    assert!(
        doc.level().requires_full_lighting,
        "a master circuit needs Full"
    );
    let cabinet = doc.level().props.first().ok_or("missing cabinet")?;
    let group = cabinet.light_switch.as_deref().ok_or("missing circuit")?;
    assert!(
        cabinet.electrical_panel.is_some() && (cabinet.z - 1.202).abs() < 0.0001,
        "the body sits on the selected wall face"
    );
    assert!(
        doc.level().ceiling_lights.iter().all(|light| {
            light.enabled && light.switchable && light.switch_group.as_deref() == Some(group)
        }),
        "all fixture contributions start on in the same circuit"
    );
    assert!(doc.undo(), "wiring is one undo");
    assert_eq!(
        doc.raw(),
        &before_wire,
        "the cabinet stays in place when undoing wiring"
    );
    assert!(doc.redo(), "redo puts all fixtures back on the circuit");
    let mut pose = doc.transform(id)?;
    pose.position[0] += 0.3;
    pose.position[1] += 0.15;
    doc.set_transform(id, pose)?;
    let support = doc.raw().clone();
    let mut detached = pose;
    detached.position[2] += 0.04;
    assert!(
        doc.set_transform(id, detached).is_err(),
        "numeric moves cannot detach the cabinet"
    );
    let mut reversed = pose;
    reversed.yaw_degrees = 180.0;
    assert!(
        doc.preview_transform(id, reversed).is_err(),
        "cabinet cannot face into its wall"
    );
    assert_eq!(
        doc.raw(),
        &support,
        "invalid edits leave the supported pose"
    );
    doc.save()?;
    let reopened = Document::open(&fixture.0)?;
    assert_eq!(
        reopened.raw(),
        doc.raw(),
        "panel and fixture wiring reopen exactly"
    );
    let semantics = crate::engine::canonical_json::canonical_json_bytes(reopened.level())?;
    let exported = crate::engine::package::semantics::parse(
        std::str::from_utf8(&semantics).map_err(|error| error.to_string())?,
    )?;
    assert!(
        crate::engine::electrical_panel::present(&exported),
        "the compiler receives the same typed panel and references"
    );
    Ok(())
}

// Selecting every fixture must fail atomically when that would take control
// from another switch, an isolated trigger fixture, or an initially off light.
#[test]
fn electrical_panel_assignment_refuses_existing_controls_without_rewriting_them()
-> Result<(), String> {
    use crate::editor::document::flicker::Flicker;
    let fixture = Fixture::with_level(&json!({
        "schema":"places-level","format_version":1_u32,"id":"entry","name":"Separate circuits",
        "spawn":{"x":2_u32,"z":3_u32},"requires_full_lighting":true,
        "rooms":[{"width":8_u32,"depth":8_u32,"height":3_u32}],
        "walls":[{"x":0_u32,"z":1_u32,"width":8_u32,"depth":0.2_f32,"height":3_u32}],
        "props":[
            {"id":"panel","model":"maintenance:electrical_panel","solid":true,"x":2_u32,"y":1.5_f32,"z":1.202_f32,
             "electrical_panel":{},"components":[{"component":"interactable"}],
             "bindings":[{"on":"interact","actions":[{"action":"show_message","title":"Power","lines":["Lever moved"]}]}]},
            {"id":"other","model":"home:wall_switch","x":5_u32,"z":2_u32,"light_switch":"existing",
             "components":[{"component":"interactable"}],"bindings":[{"on":"interact","actions":[{"action":"set_light","target":"local","on":false}]}]}
        ],
        "ceiling_lights":[
            {"id":"fresh","fixture":"core:fluorescent_panel_01","x":1_u32,"z":2_u32},
            {"id":"local","fixture":"core:fluorescent_panel_01","x":2_u32,"z":2_u32,"switch_group":"existing","switchable":true},
            {"id":"isolated","fixture":"core:fluorescent_panel_01","x":3_u32,"z":2_u32,"switchable":true},
            {"id":"initially_off","fixture":"core:fluorescent_panel_01","x":4_u32,"z":2_u32,"enabled":false}
        ]
    }))?;
    let mut doc = Document::open(&fixture.0)?;
    let before = doc.raw().clone();
    for (selected, reason) in [(1, "already belongs"), (2, "individual"), (3, "starts off")] {
        assert!(
            doc.assign_circuit(&[0, selected], 0)
                .is_err_and(|error| error.contains(reason)),
            "one conflicting fixture refuses the entire assignment"
        );
        assert_eq!(
            doc.raw(),
            &before,
            "no switch, binding, or initialization was rewritten"
        );
    }
    doc.assign_circuit(&[0], 0)?;
    for field in [
        "/props/0/bindings",
        "/props/1",
        "/ceiling_lights/1",
        "/ceiling_lights/2",
        "/ceiling_lights/3",
    ] {
        assert_eq!(
            doc.raw().pointer(field),
            before.pointer(field),
            "only the fresh fixture changed"
        );
    }
    let assigned = doc.raw().clone();
    assert!(
        doc.apply_flicker(
            ObjectId::Light(0),
            &Flicker {
                enabled: true,
                ..Flicker::default()
            },
            false
        )
        .is_err(),
        "panel lights cannot acquire a per-fixture flicker action"
    );
    assert!(
        doc.assign_circuit(&[0], 1).is_err(),
        "ordinary switch cannot steal a panel fixture"
    );
    assert_eq!(doc.raw(), &assigned, "attempted transfers are atomic");
    doc.unlink_circuit(&[0])?;
    assert!(
        !doc.level()
            .ceiling_lights
            .first()
            .ok_or("fixture")?
            .switchable,
        "explicit unlink restores unswitched illumination"
    );
    Ok(())
}

// The same small scene checks both real storeys, a gap narrower than the
// old runtime rail sampling, and the editor's single save/history edit.
#[test]
fn railing_picker_keeps_upper_footing_and_rejects_gaps_and_air() -> Result<(), String> {
    use crate::editor::railings::{candidate, snap_footing, validate_changed};
    use crate::engine::level::WalkableFloor;
    use glam::Vec3;

    let fixture = Fixture::with_level(&json!({
        "schema":"places-level","format_version":1_u32,"id":"entry","name":"Railing supports",
        "spawn":{"x":1_u32,"z":1_u32},"rooms":[
            {"width":8_u32,"depth":8_u32,"height":2.7_f32},
            {"x":1_u32,"z":3.05_f32,"width":2.4_f32,"depth":2_u32,"floor_y":3_u32,"height":3_u32},
            {"x":3.46_f32,"z":3.05_f32,"width":3_u32,"depth":2_u32,"floor_y":3_u32,"height":3_u32}
        ]
    }))?;
    let mut doc = Document::open(&fixture.0)?;
    let raw = doc.raw().clone();
    let floor = WalkableFloor::from_level(doc.level());
    let snapped = snap_footing(
        doc.level(),
        &floor,
        Vec3::new(1.22, 3.0004, 3.12),
        Some(0.25),
    );
    assert!(
        snapped[1].to_bits() == 3.0_f32.to_bits() && (snapped[2] - 3.05).abs() < 0.001,
        "a near-edge pick snaps to the off-grid edge and the actual floor plane"
    );
    let same_side = snap_footing(doc.level(), &floor, Vec3::new(3.39, 3.0, 4.0), Some(0.5));
    assert!(
        (same_side[0] - 3.4).abs() < 0.001,
        "snapping sticks to the edge on this side of the gap"
    );
    let middle = snap_footing(doc.level(), &floor, Vec3::new(2.17, 3.0, 4.07), Some(0.25));
    assert!(
        (middle[0] - 2.25).abs() < 0.001 && (middle[2] - 4.0).abs() < 0.001,
        "a supported interior pick still follows the grid"
    );
    let corner = candidate(&doc, [1.0, 3.0, 3.05], &[], [2.4, 3.0, 3.75]);
    assert!(
        corner.is_err_and(|reason| reason.contains("post base")),
        "a supported centre line does not excuse a mostly floating diagonal end post"
    );
    assert!(
        candidate(&doc, [1.3, 3.0, 3.5], &[], [5.0, 3.0, 3.5]).is_err(),
        "a thin hole in the upper walkway is not the lower floor"
    );
    assert!(
        candidate(&doc, [1.3, 2.0, 3.5], &[], [2.5, 2.0, 3.5]).is_err(),
        "midair never counts as a valid footing"
    );
    assert_eq!(doc.raw(), &raw, "failed previews do not write");
    let (checked, _) = candidate(&doc, [1.3, 3.0, 3.05], &[], [3.1, 3.0, 3.05])?;
    doc.apply_authoring_edit(&raw, checked)?;
    let next = doc.raw().clone();
    let (ground, _) = candidate(&doc, [1.0, 0.0, 1.0], &[], [3.0, 0.0, 1.0])?;
    doc.apply_authoring_edit(&next, ground)?;
    assert!(doc.undo(), "one path is one undo step");
    let before_move = doc.raw().clone();
    let mut moved = before_move.clone();
    *moved.pointer_mut("/guardrails/0/y").ok_or("numeric base")? = json!(3.02_f32);
    let raised = doc.validate_candidate(&moved)?;
    assert!(
        validate_changed(&raised, doc.level(), 0).is_err(),
        "an explicit 2 cm height increase cannot float every post"
    );
    assert_eq!(doc.raw(), &before_move, "rejected height did not apply");
    *moved.pointer_mut("/guardrails/0/y").ok_or("numeric base")? = json!(3.0_f32);
    *moved.pointer_mut("/guardrails/0/x").ok_or("numeric x")? = json!(1.35_f32);
    *moved
        .pointer_mut("/guardrails/0/path/0/0")
        .ok_or("numeric endpoint")? = json!(3.15_f32);
    let shifted = doc.validate_candidate(&moved)?;
    validate_changed(&shifted, doc.level(), 0)?;
    doc.apply_authoring_edit(&before_move, moved)?;
    doc.save()?;
    let reopened = Document::open(&fixture.0)?;
    assert_eq!(reopened.level().guardrails.len(), 1);
    let rail = reopened.level().guardrails.first().ok_or("saved rail")?;
    assert!(
        rail.y.is_some_and(|base| (base - 3.0).abs() < 0.001),
        "reopen preserves actual upper anchoring"
    );
    assert!(
        (rail.x - 1.35).abs() < 0.001,
        "an ordinary numeric move along the edge still persists"
    );
    Ok(())
}

// Tight flat contact must not apply to the height variation across a post
// footprint on a ramp or to the established pitch line of a stair handrail.
#[test]
fn railing_still_accepts_ramp_and_stair_grade() -> Result<(), String> {
    use crate::editor::railings::{candidate, validate_changed};

    let fixture = Fixture::with_level(&json!({
        "schema":"places-level","format_version":1_u32,"id":"entry","name":"Graded railings",
        "spawn":{"x":1_u32,"z":1_u32},"rooms":[{"width":8_u32,"depth":8_u32,"height":3_u32}],
        "ramps":[{"x":1_u32,"z":5_u32,"width":4_u32,"depth":2_u32,"rise":1_u32}],
        "stairs":[{"x":1_u32,"z":2_u32,"width":4_u32,"depth":2_u32,"rise":1_u32,"steps":4_u32}]
    }))?;
    let doc = Document::open(&fixture.0)?;
    let (_raw, _owner) = candidate(&doc, [1.5, 0.125, 6.0], &[], [4.5, 0.875, 6.0])?;
    let mut stair = doc.raw().clone();
    let _previous = stair.as_object_mut().ok_or("stair fixture")?.insert(
        "guardrails".into(),
        json!([{"x":2.25_f32,"y":0.5625_f32,"z":3.0_f32,"length":1_u32,"rise":0.25_f32}]),
    );
    validate_changed(&doc.validate_candidate(&stair)?, doc.level(), 0)?;
    Ok(())
}

// The first room's roof is below this placement. The active upper roof,
// not the first matching room in plan view, must decide whether it fits.
#[test]
fn railing_rejects_the_low_ceiling_on_its_actual_storey() -> Result<(), String> {
    use crate::editor::railings::{candidate, validate_changed};

    let fixture = Fixture::with_level(&json!({
        "schema":"places-level","format_version":1_u32,"id":"entry","name":"Stacked railing clearance",
        "spawn":{"x":1_u32,"z":1_u32},"rooms":[
            {"width":8_u32,"depth":8_u32,"height":2.7_f32},
            {"x":1_u32,"z":3_u32,"width":2.5_f32,"depth":2_u32,"floor_y":3_u32,"height":0.8_f32}
        ]
    }))?;
    let mut doc = Document::open(&fixture.0)?;
    let raw = doc.raw().clone();
    let blocked = candidate(&doc, [1.3, 3.0, 3.5], &[], [2.9, 3.0, 3.5]);
    assert!(
        blocked.is_err_and(|reason| reason.contains("ceiling")),
        "the upper roof blocks Basic placement"
    );
    let (lower, _) = candidate(&doc, [1.3, 0.0, 3.5], &[], [2.9, 0.0, 3.5])?;
    doc.apply_authoring_edit(&raw, lower)?;
    let mut shortened = doc.level().clone();
    let rail = shortened.guardrails.first_mut().ok_or("lower rail")?;
    rail.y = Some(3.0);
    rail.height = 0.5;
    validate_changed(&shortened, doc.level(), 0)?;
    let prior = shortened.clone();
    shortened.guardrails.first_mut().ok_or("upper rail")?.height = 1.0;
    assert!(
        validate_changed(&shortened, &prior, 0).is_err(),
        "a height-only change cannot skip the roof check"
    );
    Ok(())
}

// One editable-source write and semantic encode/decode guard the direction
// against losing its instance scope or being dropped from a package.
#[test]
fn escalator_direction_defaults_and_roundtrips_authoring_and_runtime_semantics()
-> Result<(), String> {
    use crate::engine::escalator::Direction;

    let rig = |x| {
        json!({"model":"mall:escalator","x":x,"z":1_u32,"occludes":false,
            "escalator":{"initial_running":true},
            "components":[{"component":"animation","clip":"escalator_run","looped":true}]})
    };
    let fixture = Fixture::with_level(&json!({
        "schema":"places-level","format_version":1_u32,"id":"entry","name":"Two belts",
        "spawn":{"x":1_u32,"z":1_u32},
        "rooms":[{"width":8_u32,"depth":10_u32,"height":6_u32}],
        "floor_regions":[{"x":1_u32,"z":7.6_f32,"width":6_u32,"depth":2.4_f32,"offset_y":3_u32}],
        "props":[rig(2_u32),rig(5_u32)]
    }))?;
    let mut doc = Document::open(&fixture.0)?;
    let catalog = AssetCatalog::builtin();
    let selected = ObjectId::Prop(0);
    let pose = doc.transform(selected)?;
    assert_eq!(
        doc.gameplay_value(selected, S::EscalatorDirection),
        json!(Direction::Up)
    );
    assert!(
        doc.set_gameplay_value(selected, S::EscalatorDirection, json!("sideways"), &catalog)
            .is_err()
    );
    doc.set_gameplay_value(
        selected,
        S::EscalatorDirection,
        json!(Direction::Down),
        &catalog,
    )?;
    assert!(doc.undo(), "direction is an undoable inspector change");
    assert_eq!(
        doc.gameplay_value(selected, S::EscalatorDirection),
        json!(Direction::Up)
    );
    assert!(doc.redo(), "redo restores the chosen direction");
    doc.save()?;
    let reopened = Document::open(&fixture.0)?;
    assert_eq!(
        reopened.transform(selected)?,
        pose,
        "direction never changes the placement"
    );
    assert_eq!(
        reopened.gameplay_value(selected, S::EscalatorDirection),
        json!(Direction::Down)
    );

    let bytes = crate::engine::canonical_json::canonical_json_bytes(reopened.level())?;
    let semantics = crate::engine::package::semantics::parse(
        std::str::from_utf8(&bytes).map_err(|error| error.to_string())?,
    )?;
    let directions: Vec<_> = semantics
        .props
        .iter()
        .filter_map(|prop| prop.escalator.as_ref())
        .map(|def| (def.direction, def.initial_running))
        .collect();
    assert_eq!(directions, [(Direction::Down, true), (Direction::Up, true)]);
    Ok(())
}

// Exercise the same geometric fit and single commit used by native floor picks.
// The small scene includes one non-default connection and a detached free base.
#[test]
fn escalator_native_authoring_fits_floor_picks_and_persists_free_length() -> Result<(), String> {
    use crate::engine::escalator::{Dimensions, Fit, Placement};
    use glam::Vec3;

    let fixture = Fixture::with_level(&json!({
        "schema":"places-level","format_version":1_u32,"id":"entry","name":"Escalator fits",
        "spawn":{"x":1_u32,"z":1_u32},
        "rooms":[{"width":9_u32,"depth":17_u32,"height":9_u32}],
        "floor_regions":[
            {"x":1.0_f32,"z":4.8_f32,"width":2_u32,"depth":3_u32,"offset_y":1.6_f32}
        ]
    }))?;
    let mut doc = Document::open(&fixture.0)?;
    let lower = Vec3::new(1.9, 0.0, 0.6);
    assert!(Fit::connect(doc.level(), lower, Vec3::new(6.0, 1.6, 5.4)).is_err());
    let connected = Fit::connect(doc.level(), lower, Vec3::new(2.0, 1.6, 5.4))?;
    let _up = doc.add_escalator(connected)?;
    let free = Fit::free(
        doc.level(),
        Vec3::new(6.0, 0.7, 1.0),
        0,
        Dimensions::from_length(11.2)?,
    )?;
    let id = doc.add_escalator(free)?;
    assert!(doc.undo(), "placement adds one undo record");
    assert_eq!(doc.level().props.len(), 1);
    assert!(doc.redo());
    let transform = doc.transform(id)?;
    doc.save()?;
    let reopened = Document::open(&fixture.0)?;
    assert_eq!(reopened.transform(id)?, transform);
    let encoded = crate::engine::canonical_json::canonical_json_bytes(reopened.level())?;
    let decoded = crate::engine::package::semantics::parse(
        std::str::from_utf8(&encoded).map_err(|error| error.to_string())?,
    )?;
    crate::engine::escalator::validate(&decoded)?;
    let rigs: Vec<_> = decoded
        .props
        .iter()
        .filter_map(|prop| prop.escalator.as_ref())
        .collect();
    let [small, tall] = rigs.as_slice() else {
        return Err("both escalators must persist".into());
    };
    assert_eq!(small.placement, Placement::Connected);
    assert_eq!(tall.placement, Placement::Free);
    assert!((small.rise - 1.6).abs() < 0.001 && (tall.rise - 5.0).abs() < 0.001);
    Ok(())
}

#[test]
fn gameplay_wiring_roundtrip_duplicate_and_invalid_edit_are_atomic() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut doc = Document::open(&fixture.0)?;
    let catalog = AssetCatalog::from_json_str(
        r#"{"format_version":2,"assets":[{"id":"core:chair","display_name":"Chair","asset_class":"core","asset_type":"prop","source":"file","model":"shared/chair.glb","size":[0.5,1,0.5],"solid":true}]}"#,
    )?;
    doc.set_gameplay_value(ObjectId::Prop(0), S::Physics, json!(true), &catalog)?;
    let profile = doc
        .level()
        .props
        .first()
        .and_then(|p| p.physics.as_ref())
        .ok_or("Missing physics profile")?;
    assert!(profile.motion.is_movable());
    assert!(
        profile.reset_group.is_some(),
        "authoring physics exposes a reusable trigger group"
    );
    let instance = doc
        .level()
        .prop_instance_ids()
        .first()
        .cloned()
        .ok_or("Missing stable instance")?;
    let sound = doc.add(ObjectKind::Audio, [1.0, 1.0, 2.0], Some("water_emitter"))?;
    let door = doc.add(ObjectKind::Door, [4.0, 0.0, 3.0], None)?;
    doc.set_gameplay_value(door, S::Bidirectional, json!(true), &catalog)?;
    let point = doc.add(ObjectKind::SpawnPoint, [5.0, 0.0, 5.0], Some("mannequin"))?;
    let trigger = doc.add(ObjectKind::Trigger, [1.0, 0.25, 2.0], None)?;
    let mut bounds = doc.transform(trigger)?;
    bounds.size = [1.25, 0.75, 2.5];
    doc.set_transform(trigger, bounds)?;
    doc.set_trigger_filter(trigger, json!({"kind":"item","instance":instance}))?;
    verify_pending_policy(&mut doc, trigger, &catalog, &fixture)?;
    let sound_id = doc
        .level()
        .sound_emitters
        .first()
        .ok_or("Missing emitter")?
        .id
        .clone();
    let door_id = doc.level().doors.first().ok_or("Missing door")?.id.clone();
    let point_id = doc
        .level()
        .spawn_points
        .first()
        .ok_or("Missing point")?
        .id
        .clone();
    doc.add_trigger_action(
        trigger,
        json!({"action":"play_sound","target":sound_id,"loop":true}),
    )?;
    doc.add_trigger_action(trigger, json!({"action":"open","target":door_id}))?;
    doc.add_trigger_action(trigger, json!({"action":"spawn_entity","point":point_id}))?;
    verify_action_removal_policy(&mut doc, trigger)?;
    let wired = doc.raw().clone();
    assert!(
        doc.set_gameplay_value(sound, S::Gain, json!(2.0_f32), &catalog)
            .is_err()
    );
    assert_eq!(
        doc.raw(),
        &wired,
        "invalid capability edits never consume source/history"
    );
    assert!(
        doc.delete(sound).is_err(),
        "referenced sound cannot disappear behind an action"
    );
    assert_eq!(doc.raw(), &wired);
    verify_duplicate_undo(
        &mut doc,
        [sound, door, trigger, point],
        trigger,
        bounds,
        &wired,
    )?;
    verify_save_reopen(&mut doc, &fixture, &wired)
}

fn verify_duplicate_undo(
    doc: &mut Document,
    objects: [ObjectId; 4],
    trigger: ObjectId,
    bounds: crate::editor::document::Transform,
    wired: &serde_json::Value,
) -> Result<(), String> {
    for original in objects {
        let copy = doc.duplicate(original)?;
        assert_ne!(copy, original);
        if original == trigger {
            let actual = doc.transform(copy)?;
            assert_eq!(actual.position[1], bounds.position[1]);
            assert_eq!(actual.size, bounds.size);
        }
        assert!(doc.undo());
        assert_eq!(doc.raw(), wired, "each duplication is one undo step");
    }
    Ok(())
}

fn verify_save_reopen(
    doc: &mut Document,
    fixture: &Fixture,
    wired: &serde_json::Value,
) -> Result<(), String> {
    doc.save_all()?;
    let reopened = Document::open(&fixture.0)?;
    assert_eq!(
        reopened.raw(),
        wired,
        "wiring and unknown capability fields survive source publication"
    );
    assert!(
        reopened
            .level()
            .doors
            .first()
            .is_some_and(|d| d.bidirectional)
    );
    assert!(
        reopened
            .level()
            .sound_emitters
            .first()
            .is_some_and(|s| s.looped)
    );
    assert_eq!(
        reopened
            .level()
            .volumes
            .first()
            .map(|v| v.bindings.first().map(|b| b.actions.len())),
        Some(Some(3))
    );
    Ok(())
}

fn verify_pending_policy(
    doc: &mut Document,
    trigger: ObjectId,
    catalog: &AssetCatalog,
    fixture: &Fixture,
) -> Result<(), String> {
    doc.set_gameplay_value(trigger, S::Once, json!(true), catalog)?;
    doc.set_gameplay_value(trigger, S::Cooldown, json!(0.5_f32), catalog)?;
    doc.set_gameplay_value(trigger, S::ExitEvent, json!("exit_volume"), catalog)?;
    let before = doc.raw().clone();
    assert!(
        doc.set_gameplay_value(trigger, S::Cooldown, json!(-1.0_f32), catalog)
            .is_err()
    );
    assert_eq!(
        doc.raw(),
        &before,
        "malformed pending policy cannot bypass shared source safeguards"
    );
    assert!(
        doc.level()
            .volumes
            .first()
            .is_some_and(|volume| volume.bindings.is_empty()),
        "empty triggers stay valid before wiring actions"
    );
    doc.save_all()?;
    let reopened = Document::open(&fixture.0)?;
    assert_eq!(reopened.gameplay_value(trigger, S::Once), json!(true));
    assert_eq!(
        reopened.gameplay_value(trigger, S::Cooldown),
        json!(0.5_f32)
    );
    Ok(())
}

fn verify_action_removal_policy(doc: &mut Document, trigger: ObjectId) -> Result<(), String> {
    let actions = doc
        .level()
        .volumes
        .first()
        .and_then(|volume| volume.bindings.first())
        .map(|binding| binding.actions.clone())
        .ok_or("Missing materialized actions")?;
    for _ in 0..actions.len() {
        doc.remove_trigger_action(trigger, 0)?;
    }
    assert_eq!(doc.gameplay_value(trigger, S::Once), json!(true));
    assert_eq!(doc.gameplay_value(trigger, S::Cooldown), json!(0.5_f32));
    for action in actions {
        doc.add_trigger_action(
            trigger,
            serde_json::to_value(action).map_err(|error| error.to_string())?,
        )?;
    }
    assert!(
        doc.level()
            .volumes
            .first()
            .and_then(|volume| volume.bindings.first())
            .is_some_and(|binding| binding.once
                && binding.on == crate::engine::level::EventKindName::ExitVolume)
    );
    Ok(())
}
