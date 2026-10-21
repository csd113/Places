//! Three tiny CPU-only scenes protect new editor behavior that engine tests
//! cannot cover: signed ramp transforms and raw round trips, atomic opening
//! splits, and failed/stale correction history safety. They write one small
//! project per test, never bake/render, and add no assets or resource lookup.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

use crate::editor::document::{Document, ObjectId, ObjectKind};
use crate::engine::level::LevelSurfaces;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Result<Self, String> {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "places-editor-geometry-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).map_err(|error| error.to_string())?;
        fs::create_dir(path.join("levels")).map_err(|error| error.to_string())?;
        let manifest = serde_json::json!({
            "schema":"places-project", "project_format":1_u32, "id":"geometry",
            "name":"Geometry fixture", "entry_level":"entry",
            "levels":[{"id":"entry", "source":"levels/entry.json"}]
        });
        let scene = serde_json::json!({
            "schema":"places-level", "format_version":1_u32, "id":"entry",
            "name":"Tiny geometry fixture", "spawn":{"x":1.0_f32,"z":1.0_f32},
            "rooms":[{"width":8.0_f32,"depth":8.0_f32,"height":4.0_f32,
                "ceiling_tile_origin":[0.25_f32,0.5_f32],
                "future_room":{"untouched":[true,"source"]}}],
            "future_scene":{"nested":{"preserve":true}}
        });
        for (name, value) in [
            ("places.project.json", manifest),
            ("levels/entry.json", scene),
        ] {
            fs::write(
                path.join(name),
                serde_json::to_vec(&value).map_err(|error| error.to_string())?,
            )
            .map_err(|error| error.to_string())?;
        }
        Ok(Self(path))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

// Protect the Beam authoring failure: an otherwise valid final ramp pose can
// have an invalid first scalar edit. One tiny source fixture, no GPU or bake.
#[test]
fn complete_pose_preview_is_nonmutating_and_commits_one_undo_step() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut document = Document::open(&fixture.0)?;
    let ramp = document.add(ObjectKind::Ramp, [1.0_f32, 0.0_f32, 1.0_f32], None)?;
    let before = document.raw().clone();
    let mut pose = document.transform(ramp)?;
    pose.position = [1.0_f32, 3.9_f32, 1.0_f32];
    assert!(
        document.set_transform(ramp, pose).is_err(),
        "the raised intermediate ramp still intersects the ceiling"
    );
    assert_eq!(
        document.raw(),
        &before,
        "a refused scalar edit leaves source intact"
    );
    pose.size = [1.0_f32, 0.05_f32, 2.0_f32];
    let preview = document.preview_transform(ramp, pose)?;
    assert_eq!(
        preview.ramps.len(),
        1,
        "the complete valid pose can be previewed"
    );
    assert_eq!(document.raw(), &before, "preview creates no authored edit");
    document.set_transform(ramp, pose)?;
    assert!(document.undo(), "the complete transform is one undo step");
    assert_eq!(
        document.raw(),
        &before,
        "one undo restores the entire original pose and extensions"
    );
    Ok(())
}

#[test]
fn basin_exit_ramp_signed_rotation_and_round_trip() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut document = Document::open(&fixture.0)?;
    let original = document.raw().clone();
    let basin = document.add(ObjectKind::FloorRegion, [4.0_f32, -1.2_f32, 4.0_f32], None)?;
    let ramp = document.add(ObjectKind::Ramp, [4.0_f32, 0.0_f32, 1.0_f32], None)?;
    let mut exit = document.transform(ramp)?;
    exit.size = [2.0_f32, -1.2_f32, 3.0_f32];
    document.set_transform(ramp, exit)?;
    let surfaces = LevelSurfaces::new(document.level());
    assert!(
        surfaces
            .floor_y_at(5.0_f32, 5.0_f32)
            .is_some_and(|y| (y + 1.2_f32).abs() < 0.0001_f32),
        "the basin is a real lowered walking surface"
    );
    assert!(
        surfaces
            .floor_y_at(5.0_f32, 2.5_f32)
            .is_some_and(|y| (y + 0.6_f32).abs() < 0.0001_f32),
        "the adjacent descending ramp provides a continuous exit"
    );
    let before_rotation = document.raw().clone();
    let mut square = exit;
    square.size = [2.0_f32, -1.2_f32, 2.0_f32];
    square.yaw_degrees = 90.0_f32;
    assert!(
        document.set_transform(ramp, square).is_err(),
        "a square ramp cannot silently rotate its engine-fixed X slope to Z"
    );
    assert_eq!(
        document.raw(),
        &before_rotation,
        "unrepresentable rotations leave geometry intact"
    );
    exit.yaw_degrees = 270.0_f32;
    exit.position = [0.0_f32, 0.0_f32, 0.0_f32];
    document.set_transform(ramp, exit)?;
    let turned = document.transform(ramp)?;
    assert!(
        (turned.position.get(1).copied().unwrap_or_default() + 1.2_f32).abs() < 0.0001_f32
            && (turned.size.get(1).copied().unwrap_or_default() - 1.2_f32).abs() < 0.0001_f32,
        "rotating a Z ramp to the negative X direction exchanges its ends"
    );
    assert!(document.undo(), "ramp rotation is one undo step");
    assert_eq!(
        document.raw(),
        &before_rotation,
        "undo restores exact ramp and basin records"
    );
    let saved = document.raw().clone();
    document.save()?;
    let reopened = Document::open(&fixture.0)?;
    assert_eq!(
        reopened.raw(),
        &saved,
        "new geometry survives project save/reopen exactly"
    );
    assert_eq!(
        reopened.raw().get("future_scene"),
        original.get("future_scene"),
        "unknown source data survives geometry edits"
    );
    document.delete(basin)?;
    assert!(document.undo(), "basin deletion is undoable");
    assert_eq!(
        document.raw(),
        &saved,
        "basin undo restores its full source record"
    );
    Ok(())
}

#[test]
fn upper_floor_opening_is_atomic_and_preserves_authored_room_data() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut document = Document::open(&fixture.0)?;
    let original = document.raw().clone();
    let upper = document.add_upper_floor(ObjectId::Room(0))?;
    let stacked = document.raw().clone();
    assert!(
        document.add_upper_floor(ObjectId::Room(0)).is_err(),
        "an existing storey cannot be duplicated at the same plane"
    );
    assert_eq!(
        document.raw(),
        &stacked,
        "a refused duplicate storey creates no partial edit"
    );
    let strips = document.cut_center_opening(upper)?;
    assert_eq!(
        strips.len(),
        4,
        "an interior cut produces four disjoint room strips"
    );
    assert!(
        !document.level().rooms.iter().any(|room| {
            (room.floor_y - 4.0_f32).abs() < 0.0001_f32 && room.contains(4.0_f32, 4.0_f32)
        }),
        "the upper floor has a genuine geometric opening"
    );
    assert_eq!(
        document.raw().get("rooms").and_then(|rooms| rooms.get(0)),
        original.get("rooms").and_then(|rooms| rooms.get(0)),
        "cutting the upper floor does not change the lower floor or its ceiling"
    );
    for id in strips {
        let ObjectId::Room(index) = id else {
            return Err("Opening returned a non-room selection".into());
        };
        for key in ["future_room", "ceiling_tile_origin"] {
            assert_eq!(
                document
                    .raw()
                    .get("rooms")
                    .and_then(|rooms| rooms.get(index))
                    .and_then(|room| room.get(key)),
                original
                    .get("rooms")
                    .and_then(|rooms| rooms.get(0))
                    .and_then(|room| room.get(key)),
                "every retained strip preserves raw metadata and the world-space tile frame"
            );
        }
    }
    let cut = document.raw().clone();
    assert!(document.undo(), "one undo restores the unsplit upper floor");
    assert_eq!(
        document.raw(),
        &stacked,
        "the compound cut never leaves a partial split in history"
    );
    assert!(document.redo(), "the compound cut can be reapplied");
    assert_eq!(document.raw(), &cut, "redo restores the exact strips");
    document.save()?;
    assert_eq!(
        Document::open(&fixture.0)?.raw(),
        &cut,
        "split geometry round trips through project sources"
    );
    Ok(())
}

#[test]
fn failed_geometry_and_stale_corrections_do_not_consume_history() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut document = Document::open(&fixture.0)?;
    let initial = document.raw().clone();
    assert!(
        document
            .cut_floor_opening(ObjectId::Room(0), [0.0_f32, 0.0_f32, 8.0_f32, 8.0_f32])
            .is_err(),
        "a cut cannot silently delete the entire room"
    );
    let mut invalid = initial.clone();
    let width = invalid
        .pointer_mut("/rooms/0/width")
        .ok_or("Fixture width missing")?;
    *width = Value::from(-1.0_f32);
    assert!(
        document.apply_checked_edit(&initial, invalid).is_err(),
        "invalid correction is rejected before publication"
    );
    assert_eq!(
        document.raw(),
        &initial,
        "failed geometry and fixes preserve source data"
    );
    assert!(
        !document.can_undo(),
        "failed operations add no undo revision"
    );
    let mut gable = initial.clone();
    let room = gable
        .pointer_mut("/rooms/0")
        .and_then(Value::as_object_mut)
        .ok_or("Fixture room missing")?;
    drop(room.insert(
        "ceiling".into(),
        serde_json::json!({"kind":"gable","ridge":"x","ridge_rise":1.0_f32}),
    ));
    document.apply_checked_edit(&initial, gable.clone())?;
    assert!(
        document.add_upper_floor(ObjectId::Room(0)).is_err(),
        "a new storey at the eave cannot intersect a gable roof"
    );
    assert!(
        document.cut_center_opening(ObjectId::Room(0)).is_err(),
        "a gable split must not silently reshape the roof"
    );
    assert!(
        document
            .apply_checked_edit(&initial, initial.clone())
            .is_err(),
        "a stale preview cannot replace a newer scene"
    );
    assert_eq!(
        document.raw(),
        &gable,
        "rejected corrections leave the newer scene intact"
    );
    assert!(
        document.undo(),
        "the last successful edit is still the next undo entry"
    );
    assert_eq!(
        document.raw(),
        &initial,
        "failed operations never advance the history cursor"
    );
    assert!(document.redo(), "failed operations leave redo available");
    assert_eq!(
        document.raw(),
        &gable,
        "redo retains the complete authored roof"
    );
    Ok(())
}

// This small raw-document regression specifically protects the stage-5 artistic
// placement contract; no engine-only serialization test observes editor history
// or save/reopen preservation. It writes one tiny JSON project and never bakes.
#[test]
fn sideways_desk_round_trip_and_undo_preserve_intentional_intersection() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut document = Document::open(&fixture.0)?;
    let desk = document.add(ObjectKind::Prop, [3.0, -0.4, 3.0], Some("core:desk"))?;
    let original = document.transform(desk)?;
    let mut tilted = original;
    tilted.pitch_degrees = 90.0;
    tilted.roll_degrees = 17.0;
    tilted.yaw_degrees = 31.0;
    document.set_transform(desk, tilted)?;
    document.save()?;
    let exact_raw = document.raw().clone();
    let reopened = Document::open(&fixture.0)?;
    assert_eq!(
        reopened.raw(),
        &exact_raw,
        "save/reopen cannot ground or remove an intentional embedded placement"
    );
    assert_eq!(reopened.transform(desk)?, tilted);
    assert!(document.undo());
    assert_eq!(
        document.transform(desk)?,
        original,
        "pitch, roll and yaw undo together"
    );
    assert!(document.redo());
    assert_eq!(document.transform(desk)?, tilted);
    Ok(())
}

// A global patch formerly changed every overlapping storey. This small two-room
// fixture uniquely protects local ownership, fixed ceiling height and atomic
// history/source recovery. It does no asset loading, physics step or map bake.
#[test]
fn local_floor_override_preserves_other_storeys_and_ceiling() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut doc = Document::open(&fixture.0)?;
    let upper = doc.add_upper_floor(ObjectId::Room(0))?;
    let ObjectId::Room(index) = upper else {
        return Err("Expected upper room".into());
    };
    let before = doc.raw().clone();
    let patch = serde_json::json!({"x":2_f32,"z":2_f32,"width":2_f32,"depth":2_f32,"offset_y":0.2_f32,"material":"core:tile"});
    let candidate = doc.local_floor_candidate(index, &patch)?;
    doc.apply_authoring_edit(&before, candidate)?;
    assert_eq!(doc.raw().pointer("/rooms/0"), before.pointer("/rooms/0"));
    let local = doc.level().rooms.last().ok_or("Missing local section")?;
    assert!((local.floor_y - (4.2 + crate::engine::level::MIN_STRUCTURE_M)).abs() < 1.0e-5);
    assert!((local.eave_y() - (8.0 + crate::engine::level::MIN_STRUCTURE_M)).abs() < 1.0e-5);
    assert!(doc.undo());
    assert_eq!(doc.raw(), &before);
    assert!(doc.redo());
    doc.save_all()?;
    let recovered = Document::open(&fixture.0)?;
    assert_eq!(doc.raw(), recovered.raw());
    assert_eq!(doc.raw().get("future_scene"), before.get("future_scene"));
    Ok(())
}

// Protects the accepted two-floor workflow's source ownership, actual support
// heights and atomic recovery. One tiny source document, no bake/assets/device.
#[test]
fn generated_stair_shaft_preserves_landings_support_and_one_undo_step() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut doc = Document::open(&fixture.0)?;
    let mut pose = doc.transform(ObjectId::Room(0))?;
    pose.size = [8.0_f32, 3.2_f32, 10.0_f32];
    doc.set_transform(ObjectId::Room(0), pose)?;
    let _upper = doc.add_upper_floor(ObjectId::Room(0))?;
    let before = doc.raw().clone();
    let proposed = crate::editor::stairs::candidate(
        &doc,
        0,
        1,
        1.2_f32,
        &crate::engine::assets::AssetCatalog::default(),
    )?;
    assert_eq!(doc.raw(), &before, "preview must leave source untouched");
    doc.apply_checked_edit(&before, proposed)?;
    let stairs = doc.level().stairs.first().ok_or("No generated stairs")?;
    let floor = crate::engine::level::WalkableFloor::from_level(doc.level());
    let (x0, x1, z0, z1) = stairs.bounds();
    let mut previous = 0.0_f32;
    for index in 0_u16..=100 {
        let fraction = f32::from(index) / 100.0_f32;
        let (x, z) = if stairs.axis() == crate::engine::level::WallAxis::X {
            ((x1 - x0).mul_add(fraction, x0), f32::midpoint(z0, z1))
        } else {
            (f32::midpoint(x0, x1), (z1 - z0).mul_add(fraction, z0))
        };
        let support = floor
            .height_below(x, z, previous + 0.4_f32)
            .ok_or("Missing stair support")?;
        assert!(
            support >= previous - 0.0001_f32 && support - previous <= 0.4_f32,
            "every generated tread must be reachable by the player step contract"
        );
        previous = support;
    }
    assert!(
        (previous - 3.35_f32).abs() < 0.0001_f32,
        "top tread must connect to the actual upper floor"
    );
    assert!(
        crate::engine::structure::clearances(doc.level()).is_empty(),
        "generated shaft must not create thin stacked surfaces"
    );
    assert!(doc.undo());
    assert_eq!(doc.raw(), &before);
    assert!(doc.redo());
    doc.save_all()?;
    let reopened = Document::open(&fixture.0)?;
    assert_eq!(doc.raw(), reopened.raw());
    Ok(())
}

// Protects the formerly informational shared-storey plane: the editor must
// offer a nonmutating, exact-source correction and preserve ordinary undo.
#[test]
fn structural_clearance_fix_requires_review_and_is_reversible() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut doc = Document::open(&fixture.0)?;
    let upper = doc.add_upper_floor(ObjectId::Room(0))?;
    let mut pose = doc.transform(upper)?;
    pose.position[1] = 4.0_f32;
    doc.set_transform(upper, pose)?;
    let mut raw = doc.raw().clone();
    drop(raw.as_object_mut().ok_or("Invalid scene")?.insert("ceiling_lights".into(), serde_json::json!([{"id":"flush", "fixture":"core:fluorescent_panel_01", "x":1.0_f32, "z":1.0_f32,"y":3.98_f32}])));
    doc.apply_checked_edit(&doc.raw().clone(), raw)?;
    let before = doc.raw().clone();
    let mut intruding = doc.level().clone();
    intruding
        .rooms
        .get_mut(0)
        .ok_or("Missing lower room")?
        .height += 0.1_f32;
    assert!(
        !crate::engine::structure::clearances(&intruding).is_empty(),
        "a ceiling inside the upper occupied volume is worse than a zero gap"
    );
    let report = crate::editor::safety::analyze(doc.level(), doc.raw());
    let fix = report
        .findings
        .iter()
        .find(|f| f.check == "structural-clearance")
        .and_then(|f| f.fix.as_ref())
        .ok_or("Missing structural correction")?;
    let proposed = fix.preview(&before)?;
    assert_eq!(doc.raw(), &before, "review must not change source");
    doc.apply_checked_edit(&before, proposed)?;
    let y = doc
        .raw()
        .pointer("/ceiling_lights/0/y")
        .and_then(Value::as_f64)
        .ok_or("Missing fixture Y")?;
    assert!(
        (y - 3.83_f64).abs() < 0.0001_f64,
        "reviewed ceiling fix must preserve the fixture mount drop"
    );
    assert!(crate::engine::structure::clearances(doc.level()).is_empty());
    assert!(doc.undo());
    assert_eq!(doc.raw(), &before);
    Ok(())
}

// A tiny source-only fixture protects room moves from leaving fog behind or
// moving a box shared with another space. This catches authored-data corruption
// that a visual mesh check cannot detect; no assets, GPU or bake are involved.
#[test]
fn room_drag_moves_contained_fog_and_refuses_shared_boxes() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut doc = Document::open(&fixture.0)?;
    let mut raw = doc.raw().clone();
    let object = raw.as_object_mut().ok_or("Invalid scene")?;
    drop(object.insert("fog_regions".into(), serde_json::from_str(r#"[{"id":"mist", "min":[1,0,1], "max":[2,2,2], "density":0.02, "ground_y":0, "top_y":2}]"#).map_err(|error| error.to_string())?));
    drop(object.insert("ground_fog".into(), serde_json::from_str(r#"[{"id":"low_mist", "min":[1,1], "max":[2,2], "density":0.02,"ground_y":0,"height":0.5}]"#).map_err(|error| error.to_string())?));
    doc.apply_checked_edit(&doc.raw().clone(), raw)?;
    let before = doc.raw().clone();
    let mut pose = doc.transform(ObjectId::Room(0))?;
    pose.position = [2.0_f32, 1.0_f32, 3.0_f32];
    doc.apply_drag(
        ObjectId::Room(0),
        pose,
        &crate::engine::assets::AssetCatalog::default(),
    )?;
    assert_eq!(
        doc.raw()
            .pointer("/fog_regions/0/min")
            .ok_or("Missing fog corner")?,
        &serde_json::json!([3.0_f32, 1.0_f32, 4.0_f32])
    );
    assert_eq!(
        doc.raw()
            .pointer("/ground_fog/0/min")
            .ok_or("Missing ground fog corner")?,
        &serde_json::json!([3.0_f32, 4.0_f32])
    );
    assert_eq!(
        doc.raw()
            .pointer("/ground_fog/0/ground_y")
            .and_then(Value::as_f64),
        Some(1.0_f64)
    );
    assert!(doc.undo());
    assert_eq!(doc.raw(), &before);
    let mut shared = before.clone();
    *shared
        .pointer_mut("/fog_regions/0/min/0")
        .ok_or("Missing fog coordinate")? = serde_json::json!(-1.0_f32);
    doc.apply_checked_edit(&before, shared)?;
    let unchanged = doc.raw().clone();
    assert!(
        doc.apply_drag(
            ObjectId::Room(0),
            pose,
            &crate::engine::assets::AssetCatalog::default()
        )
        .is_err()
    );
    assert_eq!(doc.raw(), &unchanged);
    Ok(())
}
