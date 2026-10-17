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
