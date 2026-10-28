//! Essential circuit regressions: atomic lifecycle and flicker power gating.
//! Tiny source records and one generated Luau module; no bake/GPU/assets.
//! These protect deletion retargeting and off-state script races absent from
//! existing flicker/group tests, with milliseconds and under 64 KiB of fixtures.
use super::super::document::{Document, ObjectId, flicker::Flicker};
use crate::engine::{entities::EntityWorld, level::ActionDef};
use serde_json::json;

#[test]
fn circuit_lifecycle_retargets_flicker_atomically_and_reopens() -> Result<(), String> {
    let path = std::env::temp_dir().join(format!("places-circuit-{}", std::process::id()));
    std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    let project = path.join("project");
    let mut doc = Document::create(&project, "circuit_test", "Circuit test")?;
    let original = doc.raw().clone();
    let mut raw = original.clone();
    drop(raw.as_object_mut().ok_or("source")?.insert(
        "ceiling_lights".into(),
        json!([
        {"id":"a","fixture":"core:fluorescent_panel_01","x":1.0_f32,"z":1.0_f32},
        {"id":"b","fixture":"core:fluorescent_panel_01","x":3.0_f32,"z":1.0_f32},
        {"id":"c","fixture":"core:fluorescent_panel_01","x":5.0_f32,"z":1.0_f32}]),
    ));
    drop(raw.as_object_mut().ok_or("source")?.insert(
        "props".into(),
        json!([
        {"id":"switch_a","model":"home:wall_switch","x":1.0_f32,"z":0.1_f32},
        {"id":"switch_b","model":"home:wall_switch","x":3.0_f32,"z":0.1_f32},
        {"model":"core:plant","x":5.0_f32,"z":5.0_f32,"editor_locked":true}]),
    ));
    doc.apply_authoring_edit(&original, raw)?;
    let before = doc.raw().clone();
    doc.assign_circuit(&[0, 1], 0)?;
    assert_eq!(
        doc.raw().pointer("/props/2"),
        before.pointer("/props/2"),
        "assigning lights preserves unrelated locked objects and derived IDs"
    );
    assert!(doc.undo(), "assignment is undoable");
    assert_eq!(
        doc.raw(),
        &before,
        "one undo restores membership and switch"
    );
    assert!(doc.redo(), "assignment is redoable");
    let locked = doc.raw().clone();
    let mut unlocked = locked.clone();
    *unlocked
        .pointer_mut("/props/2/editor_locked")
        .ok_or("lock")? = json!(false);
    doc.apply_authoring_edit(&locked, unlocked)?;
    doc.apply_flicker(
        ObjectId::Light(0),
        &Flicker {
            enabled: true,
            circuit: true,
            ..Flicker::default()
        },
        false,
    )?;
    doc.assign_circuit(&[2], 0)?;
    let assigned = doc.raw().clone();
    doc.assign_circuit(&[0, 1, 2], 0)?;
    assert_eq!(
        doc.raw(),
        &assigned,
        "repeated assignment does not create another module or undo entry"
    );
    assert_eq!(
        doc.level().scripts.len(),
        1,
        "membership refresh keeps one controller"
    );
    assert_eq!(
        doc.flicker_script(ObjectId::Light(0))?,
        doc.flicker_script(ObjectId::Light(2))?,
        "joined fixture uses existing controller"
    );
    let before_delete = doc.raw().clone();
    doc.delete(ObjectId::Light(0))?;
    assert_eq!(
        doc.level().scripts.len(),
        1,
        "deleting owner retargets one surviving controller"
    );
    assert!(doc.undo(), "delete is undoable");
    assert_eq!(
        doc.raw(),
        &before_delete,
        "delete and controller link restore atomically"
    );
    assert!(doc.redo(), "delete is redoable");
    doc.assign_circuit(&[1], 1)?;
    doc.unlink_circuit(&[1])?;
    doc.delete(ObjectId::Prop(0))?;
    doc.save_all()?;
    let reopened = Document::open(&project)?;
    assert_eq!(
        reopened.raw(),
        doc.raw(),
        "save/reopen retains all circuit changes"
    );
    assert_eq!(
        reopened.level().scripts.len(),
        1,
        "no duplicate flicker after reopen"
    );
    std::fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
    Ok(())
}

#[test]
fn circuit_power_stays_off_through_flicker_requests_and_resets() -> Result<(), String> {
    let level=crate::engine::level::LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"power_test","name":"Power test","spawn":{"x":0,"z":0},"requires_full_lighting":true,
        "props":[{"id":"switch","model":"home:wall_switch","light_switch":"pair","components":[{"component":"interactable"}]}],
        "ceiling_lights":[{"id":"a","fixture":"core:fluorescent_panel_01","x":1,"z":1,"switchable":true,"switch_group":"pair"},{"id":"b","fixture":"core:fluorescent_panel_01","x":3,"z":1,"switchable":true,"switch_group":"pair"}]}"#).map_err(|e|e.to_string())?;
    let mut world = EntityWorld::from_level(&level);
    let target = world.interactables().index_of("switch").ok_or("switch")?;
    for _ in 0..3_u8 {
        let _switched_off = world.dispatch_interaction(Some(target));
        for on in [false, true, false, true] {
            let _pattern_request =
                world.dispatch_on("a", &[ActionDef::SetLight { target: None, on }]);
            assert_eq!(
                world.light_states(),
                [(0, false), (1, false)],
                "flicker cannot repower an off circuit"
            );
        }
        let _switched_on = world.dispatch_interaction(Some(target));
        assert_eq!(
            world.light_states(),
            [(0, true), (1, true)],
            "one switch resumes both requested contributions"
        );
    }
    let _report = world.dispatch_interaction(Some(target));
    world.reset_runtime();
    assert_eq!(
        world.light_states(),
        [(0, true), (1, true)],
        "reset restores circuit power and authored state"
    );
    Ok(())
}
