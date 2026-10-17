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
        let path = std::env::temp_dir().join(format!(
            "places-gameplay-editor-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).map_err(|e| e.to_string())?;
        fs::create_dir(path.join("levels")).map_err(|e| e.to_string())?;
        fs::write(path.join("places.project.json"),serde_json::to_vec(&json!({"schema":"places-project","project_format":1_u32,"id":"gameplay","name":"Tiny gameplay","entry_level":"entry","levels":[{"id":"entry","source":"levels/entry.json"}]})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        fs::write(path.join("levels/entry.json"),serde_json::to_vec(&json!({"schema":"places-level","format_version":1_u32,"id":"entry","name":"Tiny gameplay","spawn":{"x":1_u32,"z":1_u32},"rooms":[{"width":8_u32,"depth":8_u32,"height":3_u32}],"props":[{"model":"core:chair","x":2_u32,"z":2_u32,"size":[0.5_f32,1_u32,0.5_f32],"future_capability":{"preserve":true}}]})).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        Ok(Self(path))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _removed = fs::remove_dir_all(&self.0);
    }
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
