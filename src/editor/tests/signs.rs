//! Essential sign regressions: four-facing flush placement across a solid
//! lintel, and document history/locking/save integrity. Two tiny fixtures, no
//! bake or renderer. These protect new integration boundaries absent from the
//! existing generic document tests; cost is a few temporary JSON writes.
use super::*;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "places-sign-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).map_err(|e| e.to_string())?;
        fs::create_dir(path.join("levels")).map_err(|e| e.to_string())?;
        let manifest = serde_json::from_str::<Value>(r#"{"schema":"places-project","project_format":1,"id":"signs","name":"Sign fixture","entry_level":"entry","levels":[{"id":"entry","source":"levels/entry.json"}]}"#).map_err(|error|error.to_string())?;
        fs::write(path.join("places.project.json"), manifest.to_string())
            .map_err(|e| e.to_string())?;
        let scene = serde_json::from_str::<Value>(r#"{"schema":"places-level","format_version":1,"id":"entry","name":"Signs","spawn":{"x":3,"z":3},"rooms":[{"width":6,"depth":6,"height":4}],"walls":[{"x":0,"z":0,"width":6,"depth":0.2,"height":4,"openings":[{"offset":2.5,"width":1,"height":2.2,"kind":"door"}]},{"x":0,"z":5.8,"width":6,"depth":0.2,"height":4},{"x":0,"z":0,"width":0.2,"depth":6,"height":4},{"x":5.8,"z":0,"width":0.2,"depth":6,"height":4}]}"#).map_err(|error|error.to_string())?;
        fs::write(path.join("levels/entry.json"), scene.to_string()).map_err(|e| e.to_string())?;
        Ok(Self(path))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn sign_rays_fit_all_wall_faces_without_double_offset_or_opening_overlap() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut doc = Document::open(&fixture.0)?;
    for (direction, face, coordinate, axis) in [
        (Vec3::NEG_Z, DecalSurface::WallSouth, 0.2, 2),
        (Vec3::Z, DecalSurface::WallNorth, 5.8, 2),
        (Vec3::NEG_X, DecalSurface::WallEast, 0.2, 0),
        (Vec3::X, DecalSurface::WallWest, 5.8, 0),
    ] {
        let (raw, id) = placement(&doc, Vec3::new(3.0, 3.0, 3.0), direction, Some(0.5), 4)?;
        let expected = doc.raw().clone();
        doc.apply_authoring_edit(&expected, raw)?;
        let ObjectId::Decal(index) = id else {
            return Err("Expected a decal".into());
        };
        let d = doc.level().decals.get(index).ok_or("Missing sign")?;
        assert_eq!(d.surface, face, "Clicked face owns the normal");
        let position = [d.x, d.y, d.z];
        assert!(
            (position.get(axis).copied().ok_or("Axis")? - coordinate).abs() < 0.00001,
            "Source lies on carrier; renderer alone adds bias"
        );
        assert!(
            (d.width / d.height - 4.0).abs() < 0.00001,
            "Fitted artwork keeps its ratio"
        );
        let points = corners(d).ok_or("Missing quad")?;
        for point in points {
            let normal = Vec3::from_array(face.normal());
            let source = Vec3::new(d.x, d.y, d.z);
            assert!(
                ((point - source).dot(normal) - crate::engine::render::DECAL_SURFACE_OFFSET_M)
                    .abs()
                    < 0.00001,
                "Exactly one shared depth lift"
            );
        }
    }
    assert!(
        placement(&doc, Vec3::new(3.0, 3.0, 3.0), Vec3::NEG_Z, None, 0).is_err(),
        "Repeated placement cannot stack coplanar signs"
    );
    let opening_doc = Document::open(&fixture.0)?;
    let (raw, id) = placement(&opening_doc, Vec3::new(3.0, 1.0, 3.0), Vec3::NEG_Z, None, 0)?;
    let source = raw
        .get("decals")
        .and_then(|v| v.get(id.collection().1))
        .ok_or("Missing preview")?;
    let y = source
        .get("y")
        .and_then(Value::as_f64)
        .ok_or("Missing height")?;
    assert!(
        y - 0.3_f64 >= 2.2_f64 - 0.000_01_f64,
        "A doorway hit moves the complete sign above its hole"
    );
    Ok(())
}
#[test]
fn sign_edits_keep_lock_history_duplicate_metadata_and_saved_artwork() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut doc = Document::open(&fixture.0)?;
    let (mut raw, id) = placement(&doc, Vec3::new(3.0, 3.0, 3.0), Vec3::NEG_Z, None, 0)?;
    drop(objects::object_mut(&mut raw, id)?.insert(
        "future".into(),
        json!({"text":"Long ÃŽÂ© & < > \\\" ".repeat(40),"empty":""}),
    ));
    let expected = doc.raw().clone();
    doc.apply_authoring_edit(&expected, raw)?;
    let original = doc.raw().clone();
    doc.sign_artwork(id, 4)?;
    assert!(doc.undo(), "Artwork edit has undo");
    assert_eq!(doc.raw(), &original, "Undo restores exact source");
    assert!(doc.redo(), "Artwork edit has redo");
    let copy = doc.duplicate(id)?;
    let position = doc.transform(copy)?.position;
    let source_pose = doc.transform(id)?;
    assert!(
        (position[0] - source_pose.position[0]).abs() >= source_pose.size[0] + 0.01
            || (position[1] - source_pose.position[1]).abs() >= source_pose.size[1] + 0.01,
        "Duplicate finds disjoint wall coverage instead of coplanar overlap"
    );
    assert!(
        (position[2] - 0.2).abs() < 0.00001,
        "Duplicate moves along wall rather than away from it"
    );
    let mut locked = doc.raw().clone();
    drop(objects::object_mut(&mut locked, id)?.insert("editor_locked".into(), true.into()));
    let lock_expected = doc.raw().clone();
    doc.apply_authoring_edit(&lock_expected, locked)?;
    assert!(
        doc.sign_artwork(id, 1).is_err(),
        "Locked sign rejects artwork edits"
    );
    assert!(doc.delete(id).is_err(), "Locked sign rejects deletion");
    let wall_pose = doc.transform(ObjectId::Wall(0))?;
    let mut moved = wall_pose;
    moved.position[2] += 0.5;
    assert!(
        doc.set_transform(ObjectId::Wall(0), moved).is_err(),
        "Moving a carrier cannot bypass its sign's lock"
    );
    doc.save()?;
    let reopened = Document::open(&fixture.0)?;
    assert_eq!(
        doc.raw(),
        reopened.raw(),
        "Save/reopen preserves lettering choice and opaque future data"
    );
    let (key, index) = copy.collection();
    let mut before = reopened.raw().clone();
    objects::delete(&mut before, reopened.level(), copy)?;
    assert_eq!(
        before.get(key).and_then(Value::as_array).map(Vec::len),
        Some(index),
        "Delete addresses the decal collection"
    );
    Ok(())
}
