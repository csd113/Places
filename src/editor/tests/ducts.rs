//! Protects rectangular end alignment under all six axes and source/history
//! boundaries. One tiny document, 144 bounded matrix joins, no bake or GPU.
use super::*;
use crate::editor::document::{Document, ObjectKind};
use crate::engine::assets::AssetCatalog;
use serde_json::json;

#[test]
fn duct_flange_joins_match_all_axes_and_survive_document_history() -> Result<(), String> {
    let path = std::env::temp_dir().join(format!("places-duct-joins-{}", std::process::id()));
    let mut doc = Document::create(&path, "duct_joins", "Duct joins")?;
    let result = workflow(&mut doc, &path);
    let _cleanup = std::fs::remove_dir_all(&path);
    result
}

fn workflow(doc: &mut Document, path: &std::path::Path) -> Result<(), String> {
    let original = doc.raw().clone();
    let mut raw = original.clone();
    *raw.get_mut("rooms").ok_or("rooms")? = json!([{"x":-20.0_f32,"z":-20.0_f32,"width":40.0_f32,"depth":40.0_f32,"height":8.0_f32,"floor_y":3.0_f32}]);
    doc.apply_authoring_edit(&original, raw)?;
    let catalog = AssetCatalog::load_default();
    let id = doc.add_with_catalog(
        ObjectKind::Prop,
        [2.0, 1.0, 2.0],
        Some("backrooms:hvac_elbow_up"),
        &catalog,
    )?;
    let initial = doc.raw().clone();
    let pose = doc.transform(id)?;
    let mut final_pose = pose;
    let names: Vec<_> = kit()?.keys().cloned().collect();
    assert_eq!(names.len(), 6);
    for name in names {
        let mut source: PropDef =
            serde_json::from_value(json!({"model":name,"x":2.0_f32,"z":2.0_f32,"y":1.0_f32}))
                .map_err(|e| e.to_string())?;
        let orientations: [(f32, f32, f32); 6] = [
            (0.0, 0.0, 0.0),
            (90.0, 0.0, 0.0),
            (180.0, 0.0, 0.0),
            (270.0, 0.0, 0.0),
            (0.0, 0.0, 90.0),
            (0.0, 0.0, -90.0),
        ];
        for (yaw, pitch, roll) in orientations {
            let other: PropDef = serde_json::from_value(json!({"model":"backrooms:hvac_duct_straight","x":4.0_f32,"z":4.0_f32,"y":2.0_f32,"rotation_degrees":yaw,"pitch_degrees":pitch,"roll_degrees":roll})).map_err(|e|e.to_string())?;
            for (other_end, port) in world_ports(doc.level(), &other)?.into_iter().enumerate() {
                let target = Target {
                    index: 1,
                    end: other_end,
                    model: other.model.clone(),
                    port,
                    scale: other.scale,
                };
                for end in 0..2 {
                    let joined = snap_pose(doc.level(), &source, end, &target, pose)?;
                    source.x = joined.position[0];
                    source.y = joined.position[1];
                    source.z = joined.position[2];
                    source.rotation_degrees = joined.yaw_degrees;
                    source.pitch_degrees = joined.pitch_degrees;
                    source.roll_degrees = joined.roll_degrees;
                    let actual = world_ports(doc.level(), &source)?
                        .get(end)
                        .copied()
                        .ok_or("port")?;
                    assert!(
                        actual.position.distance(port.position) < 0.0001,
                        "flange faces coincide"
                    );
                    assert!(
                        actual.normal.dot(port.normal) < -0.9999,
                        "ends face each other"
                    );
                    assert!(
                        actual.height.dot(port.height).abs() > 0.9999,
                        "rectangular apertures cannot twist by 90 degrees"
                    );
                    final_pose = joined;
                }
            }
        }
        let wrong = Target {
            index: 1,
            end: 0,
            model: source.model.clone(),
            port: world_ports(doc.level(), &source)?[0],
            scale: 2.0,
        };
        assert!(
            snap_pose(doc.level(), &source, 0, &wrong, pose).is_err(),
            "mismatched scale cannot silently gap"
        );
    }
    let _preview = doc.preview_transform(id, final_pose)?;
    assert_eq!(doc.raw(), &initial, "preview never changes source");
    doc.set_transform(id, final_pose)?;
    let joined = doc.raw().clone();
    assert!(doc.undo());
    assert_eq!(doc.raw(), &initial, "one undo restores all six pose fields");
    assert!(doc.redo());
    assert_eq!(doc.raw(), &joined);
    doc.save()?;
    assert_eq!(
        Document::open(path)?.raw(),
        doc.raw(),
        "source save/reopen preserves connector pose"
    );
    Ok(())
}
