//! Two small CPU regressions: reversed rendered/support tread parity survives
//! compiled collision transport; numeric shaft/light drafts preserve atomic history,
//! unknown data and stacked-floor openings. No GPU, bake or catalog sweep.
use super::{Draft, Tool};
use crate::editor::document::Document;
use crate::engine::level::{LevelDef, LevelSurfaces, StairDef, WalkableFloor};
use crate::engine::package::binary::{Reader, Writer};
use serde_json::{Value, json};

fn draft(doc: &Document, tool: Tool, record: Value) -> Draft {
    let index = doc
        .raw()
        .get(tool.collection())
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    Draft {
        owner: None,
        tool,
        native_cursor: 0,
        native_group: "patrol",
        index,
        fields: Vec::new(),
        active: None,
        page: 0,
        preview: false,
        slice_z: false,
        environment_before: false,
        expected: doc.raw().clone(),
        original: Some(record),
    }
}
#[test]
fn reverse_stair_support_treads_and_compiled_collision_agree() -> Result<(), String> {
    for (width, depth) in [(2_f32, 4_f32), (4_f32, 2_f32)] {
        for reverse in [false, true] {
            let mut raw = json!({"schema":"places-level","format_version":1_i32,"id":"reverse","name":"Reverse","spawn":{"x":1_i32,"z":1_i32},"rooms":[{"width":10_i32,"depth":10_i32,"height":8_i32}],"stairs":[{"x":2_i32,"z":2_i32,"width":width,"depth":depth,"offset_y":0_i32,"rise":2_i32,"steps":10_i32,"reverse":reverse}]});
            if !reverse {
                drop(
                    raw.pointer_mut("/stairs/0")
                        .and_then(Value::as_object_mut)
                        .ok_or("stair")?
                        .remove("reverse"),
                );
            }
            let level = LevelDef::from_json(&raw.to_string()).map_err(|e| e.to_string())?;
            crate::engine::loader::validate_level(&level)?;
            let stair = level.stairs.first().ok_or("stair")?;
            assert_eq!(
                stair.reverse, reverse,
                "omitted direction preserves old maps"
            );
            let floor = WalkableFloor::from_level(&level);
            let mut writer = Writer::new();
            floor.write_compiled(&mut writer)?;
            let bytes = writer.into_bytes();
            let mut reader = Reader::new(&bytes);
            let restored = WalkableFloor::read_compiled(&mut reader)?;
            for step in 0_u16..10 {
                let (a, b) = stair.tread_span(u32::from(step));
                assert!(a < b, "sorted geometry/physics patches");
                let (x, z) = if width > depth {
                    (f32::midpoint(a, b), 3_f32)
                } else {
                    (3_f32, f32::midpoint(a, b))
                };
                let expected = f32::from(step + 1) * 0.2;
                assert!((stair.offset_at(x, z) - expected).abs() < 0.00001);
                assert!((floor.height_at(x, z).ok_or("support")? - expected).abs() < 0.00001);
                assert_eq!(floor.height_at(x, z), restored.height_at(x, z));
                assert_eq!(floor.walk_height_at(x, z), restored.walk_height_at(x, z));
            }
        }
    }
    let invalid = serde_json::from_value::<StairDef>(
        json!({"x":0_i32,"z":0_i32,"width":2_i32,"depth":4_i32,"rise":1_i32,"steps":5_i32,"reverse":"north"}),
    );
    assert!(invalid.is_err());
    Ok(())
}
#[test]
fn numeric_shaft_light_and_northbound_flight_are_atomic_and_recoverable() -> Result<(), String> {
    let path = std::env::temp_dir().join(format!("places-stairs-lights-{}", std::process::id()));
    let mut doc = Document::create(&path, "stairs_lights", "Stairs lights")?;
    let mut raw = doc.raw().clone();
    *raw.get_mut("rooms").ok_or("rooms")? = json!([{"width":12_i32,"depth":16_i32,"height":3.8_f32,"future_room":"kept"},{"width":12_i32,"depth":16_i32,"height":3.8_f32,"floor_y":4.2_f32}]);
    *raw.get_mut("ceiling_lights").ok_or("ceiling_lights")? = json!([]);
    drop(
        raw.as_object_mut()
            .ok_or("raw")?
            .insert("future".into(), json!({"retained":true})),
    );
    doc.apply_authoring_edit(&doc.raw().clone(), raw.clone())?;
    let shaft =
        json!({"lower":1_i32,"upper":2_i32,"x":3_i32,"z":3_i32,"width":4_i32,"depth":10_i32});
    crate::editor::stairs::check_numeric_shaft(
        &doc,
        &shaft,
        &crate::engine::assets::AssetCatalog::default(),
    )?;
    let d = draft(&doc, Tool::Shaft, shaft);
    let shaft_candidate = d.candidate(&doc)?;
    assert_eq!(doc.raw(), &raw);
    doc.apply_authoring_edit(&d.expected, shaft_candidate)?;
    let centre: Vec<_> = doc
        .level()
        .rooms
        .iter()
        .filter(|r| r.contains(5_f32, 8_f32))
        .collect();
    assert_eq!(centre.len(), 1);
    assert_eq!(centre.first().ok_or("carrier")?.floor_y, 0_f32);
    assert!(
        (centre.first().ok_or("carrier")?.eave_y() - 8_f32).abs() < 0.0001,
        "both intervening slabs removed"
    );
    assert!(doc.undo());
    assert_eq!(doc.raw(), &raw);
    assert!(doc.redo());
    let stair = draft(
        &doc,
        Tool::Stairs,
        json!({"x":3.5_f32,"z":3_i32,"width":2.4_f32,"depth":10_i32,"offset_y":0_i32,"rise":4.2_f32,"steps":21_i32,"reverse":true}),
    );
    let stair_candidate = stair.candidate(&doc)?;
    doc.apply_authoring_edit(&stair.expected, stair_candidate)?;
    let surfaces = LevelSurfaces::new(doc.level());
    assert!(surfaces.floor_y_at(4.5, 3.1).ok_or("top")? > 4_f32);
    assert!(surfaces.floor_y_at(4.5, 12.9).ok_or("foot")? < 0.3);
    let light = draft(
        &doc,
        Tool::Light,
        json!({"fixture":"core:fluorescent_panel_01","x":2_i32,"z":2_i32,"y":8_i32,"align":"none"}),
    );
    let light_candidate = light.candidate(&doc)?;
    doc.apply_authoring_edit(&light.expected, light_candidate.clone())?;
    assert_eq!(
        doc.level().ceiling_lights.first().ok_or("light")?.y,
        Some(8_f32)
    );
    assert!(
        doc.level()
            .ceiling_lights
            .first()
            .ok_or("light")?
            .brightness
            .is_none()
    );
    assert!(doc.undo());
    assert!(doc.level().ceiling_lights.is_empty());
    assert!(doc.redo());
    doc.save_all()?;
    let reopened = Document::open(&path)?;
    assert_eq!(reopened.raw(), &light_candidate);
    assert_eq!(reopened.raw().get("future"), raw.get("future"));
    let mut bad =
        json!({"lower":1_i32,"upper":2_i32,"x":-1_i32,"z":3_i32,"width":4_i32,"depth":10_i32});
    assert!(crate::editor::stairs::numeric_shaft_candidate(&doc, &bad).is_err());
    *bad.get_mut("x").ok_or("x")? = json!(3_i32);
    *bad.get_mut("upper").ok_or("upper")? = json!(1_i32);
    assert!(crate::editor::stairs::numeric_shaft_candidate(&doc, &bad).is_err());
    drop(reopened);
    drop(doc);
    std::fs::remove_dir_all(path).map_err(|e| e.to_string())?;
    Ok(())
}
