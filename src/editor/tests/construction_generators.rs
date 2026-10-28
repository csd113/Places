//! Two small CPU regressions protect new contracts absent from primitive tests:
//! derived backing/shell collision and atomic source history/save, plus bounded
//! refusals and stair-path regeneration. No mesh build, assets or map bake.
use crate::editor::document::{Document, ObjectId};
use crate::engine::level::{LevelDef, construction};
use serde_json::{Value, json};
use std::fs;

fn source() -> Value {
    json!({"schema":"places-level","format_version":1_u32,"id":"construction","name":"Construction",
        "spawn":{"x":1.0_f32,"z":3.0_f32},"rooms":[{"width":12.0_f32,"depth":10.0_f32,"height":3.0_f32,
        "tunnels":[{"start":[2.0_f32,3.0_f32],"end":[7.0_f32,3.0_f32],"width":1.1_f32,"clearance":1.05_f32,"thickness":0.1_f32}]}],
        "walls":[{"x":2.0_f32,"z":7.0_f32,"width":7.0_f32,"depth":0.4_f32,"height":3.0_f32,
        "openings":[{"kind":"recess","offset":1.0_f32,"width":1.4_f32,"height":0.8_f32,"sill":1.0_f32,"recess":{"depth":0.2_f32}},
        {"kind":"window","offset":4.0_f32,"width":1.0_f32,"height":1.0_f32,"sill":1.0_f32,"surround":{"width":0.07_f32,"projection":0.025_f32,"sill_projection":0.12_f32,"trim_width":0.02_f32}}]}],
        "guardrails":[{"x":9.0_f32,"z":1.0_f32,"length":1_u32,"path":[[10.0_f32,1.0_f32],[10.0_f32,4.0_f32]],"height":1.0_f32,"post_spacing":1.2_f32}]})
}
fn level(raw: &Value) -> Result<LevelDef, String> {
    serde_json::from_value(raw.clone()).map_err(|e| e.to_string())
}

#[test]
fn backing_shell_and_source_history_survive_moves_and_reopen() -> Result<(), String> {
    let path = std::env::temp_dir().join(format!("places-construction-{}", std::process::id()));
    let mut doc = Document::create(&path, "construction", "Construction")?;
    let initial = doc.raw().clone();
    let raw = source();
    doc.apply_authoring_edit(&initial, raw.clone())?;
    let colliders = doc.level().collision_aabbs();
    let solid = |x: f32, y: f32, z: f32| {
        colliders.iter().any(|b| {
            x > b.min_x && x < b.max_x && y > b.min_y && y < b.max_y && z > b.min_z && z < b.max_z
        })
    };
    assert!(
        !solid(3.5, 1.4, 7.1) && solid(3.5, 1.4, 7.3),
        "a recess has an open cavity and solid backing"
    );
    assert!(
        !solid(6.5, 1.4, 7.2),
        "a through-window remains open through the entire wall"
    );
    assert!(
        !solid(4.0, 0.9, 3.0) && solid(4.0, 1.1, 3.0),
        "the 0.9 m hull fits below a roof that blocks standing"
    );
    let mut moved = doc.transform(ObjectId::Wall(0))?;
    moved.position[0] += 0.5;
    doc.set_transform(ObjectId::Wall(0), moved)?;
    let accepted = doc.raw().clone();
    assert!(
        doc.undo() && doc.raw() == &raw,
        "one undo restores all derived attachments"
    );
    assert!(
        doc.redo() && doc.raw() == &accepted,
        "one redo restores the complete move"
    );
    let expected = doc.raw().clone();
    doc.apply_authoring_edit(&expected, expected.clone())?;
    doc.save()?;
    let reopened = Document::open(&path)?;
    assert_eq!(
        reopened.raw(),
        &accepted,
        "save/reopen preserves generator source exactly"
    );
    assert_eq!(
        reopened.level().collision_aabbs(),
        doc.level().collision_aabbs(),
        "regeneration is deterministic after reopening"
    );
    drop(reopened);
    drop(doc);
    fs::remove_dir_all(path).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn generator_refusals_and_stair_corners_follow_actual_sources() -> Result<(), String> {
    let raw = source();
    for (pointer, value) in [
        ("/walls/0/openings/0/recess/depth", json!(0.39_f32)),
        ("/rooms/0/tunnels/0/clearance", json!(1.8_f32)),
        ("/rooms/0/tunnels/0/end/1", json!(4.0_f32)),
        ("/walls/0/openings/1/offset", json!(2.44_f32)),
        (
            "/guardrails/0/path",
            json!([[10.0_f32, 1.0_f32], [9.0_f32, 1.0_f32]]),
        ),
    ] {
        let mut bad = raw.clone();
        *bad.pointer_mut(pointer).ok_or("fixture path")? = value;
        assert!(
            construction::validate(&level(&bad)?).is_err(),
            "unsupported depth, standing tunnels and angled routes must refuse"
        );
    }
    let mut rail_raw = raw;
    *rail_raw.pointer_mut("/rooms/0/tunnels").ok_or("tunnels")? = json!([]);
    drop(rail_raw.as_object_mut().ok_or("fixture")?.insert("stairs".into(),json!([{"x":9.0_f32,"z":1.0_f32,"width":1.0_f32,"depth":3.0_f32,"rise":1.0_f32,"steps":5_u32}])));
    let parsed = level(&rail_raw)?;
    let rail = parsed.guardrails.first().ok_or("rail")?;
    let runs = construction::rail_runs(rail, &parsed);
    assert!(
        runs.len() >= 2 && runs.iter().any(|r| r.rise().abs() > 0.5),
        "a corner path follows the real stair rise"
    );
    let again = construction::rail_runs(rail, &parsed);
    assert_eq!(
        serde_json::to_value(&runs).map_err(|e| e.to_string())?,
        serde_json::to_value(again).map_err(|e| e.to_string())?,
        "rail regeneration is idempotent"
    );
    Ok(())
}
