//! Essential transaction regression: a cinematic and its exact event/transition
//! wiring must commit together, preserve sibling bindings, survive save/reopen,
//! and reject stale/invalid drafts without partial mutation. Two tiny starter
//! levels, no renderer, VM, asset load or baking; milliseconds of local JSON IO.
use crate::editor::document::cinematics::{value_at, value_at_mut};
use crate::editor::document::{Document, ObjectKind};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _removed = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn cinematic_wiring_is_atomic_stale_safe_and_preserves_sibling_events() -> Result<(), String> {
    let fixture = Fixture(std::env::temp_dir().join(format!(
        "places-cinematic-editor-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )));
    let mut doc = Document::create(&fixture.0, "cinematic_editor", "Tiny cinematic")?;
    let trigger = doc.add(ObjectKind::Trigger, [1.0, 0.0, 1.0], None)?;
    let level = doc.level_id().to_owned();
    doc.add_level("next")?;
    doc.select_level(&level)?;
    let original = doc.raw().clone();
    let sequence = json!({"id":"tour","cinematic":{"start":{"position":[1_i32,2_i32,1_i32],"yaw_degrees":0_i32,"pitch_degrees":0_i32}},"steps":[{"step":"camera","seconds":1_i32,"motion":{"kind":"move","position":[2_i32,2_i32,1_i32],"easing":"smooth"},"aim":{"kind":"keep"}}],"future_extension":{"retain":true}});
    doc.apply_cinematic(&original, None, &sequence, trigger, Some("next"), true)?;
    let applied = doc.raw().clone();
    assert_eq!(
        value_at(&applied, "/sequences/0/future_extension"),
        value_at(&sequence, "/future_extension"),
        "sequence extension survives"
    );
    assert_eq!(
        value_at(&applied, "/volumes/0/bindings/1/key"),
        "tour",
        "completion is keyed to the exact sequence"
    );
    assert_eq!(
        value_at(&applied, "/transitions"),
        &json!(["next"]),
        "transition declared atomically"
    );
    assert!(doc.undo(), "one undo restores the whole transaction");
    assert_eq!(doc.raw(), &original, "no wiring or transitions escape undo");
    assert!(doc.redo(), "redo reinstates transaction");
    assert!(
        doc.apply_cinematic(&original, Some("tour"), &sequence, trigger, None, false)
            .is_err(),
        "stale draft refused"
    );
    let mut invalid = sequence.clone();
    *value_at_mut(&mut invalid, "/steps/0/seconds")? = json!(-1_i32);
    assert!(
        doc.apply_cinematic(&applied, Some("tour"), &invalid, trigger, None, false)
            .is_err(),
        "invalid camera cannot partially remove completion"
    );
    assert_eq!(doc.raw(), &applied, "all failed transactions are untouched");
    doc.add_binding_action(
        trigger,
        2,
        crate::engine::level::EventKindName::ExitVolume,
        json!({"action":"start_sequence","sequence":"tour"}),
    )?;
    let before = doc.raw().clone();
    let sibling = value_at(&before, "/volumes/0/bindings/2").clone();
    doc.apply_cinematic(&before, Some("tour"), &sequence, trigger, None, false)?;
    assert_eq!(
        value_at(doc.raw(), "/volumes/0/bindings/1"),
        &sibling,
        "return-control removes only its own completion, not sibling event"
    );
    assert!(
        doc.remove_cinematic(&doc.raw().clone(), "tour").is_err(),
        "referenced resource removal refuses"
    );
    doc.save()?;
    let reopened = Document::open(&fixture.0)?;
    assert_eq!(
        reopened.raw(),
        doc.raw(),
        "source roundtrip keeps extensions and wiring"
    );
    Ok(())
}

// Protect the new prop admission, native ownership, atomic history, and the
// cancellation retry/success-only spend policy using the actual emitted bindings.
// No assets/GPU/bake; a tiny native Feltling and a few fixed world ticks.
#[test]
fn interactable_cinematic_is_atomic_and_spends_once_only_on_success() -> Result<(), String> {
    use crate::editor::document::ObjectId;
    use crate::engine::collision_index::CollisionIndex;
    use crate::engine::entities::{EntityWorld, WorldContext};
    use crate::engine::level::WalkableFloor;
    use glam::Vec3;
    let fixture = Fixture(
        std::env::temp_dir().join(format!("places-interact-cinematic-{}", std::process::id())),
    );
    let mut doc = Document::create(&fixture.0, "interaction", "Interaction")?;
    let mut raw = doc.raw().clone();
    let props = json!([{"id":"friend","model":"feltling","x":2_i32,"z":2_i32,"solid":false,"feltling":{"mode":"wait","routes":[]},"components":[{"component":"interactable","prompt":"Listen"},{"component":"state","name":"sibling","value":false}],"bindings":[{"on":"spawn","actions":[{"action":"set_state","name":"sibling","value":true}]}],"future":{"kept":true}}]);
    drop(
        raw.as_object_mut()
            .ok_or("Invalid fixture")?
            .insert("props".into(), props),
    );
    doc.apply_authoring_edit(&doc.raw().clone(), raw)?;
    let before = doc.raw().clone();
    let owner = ObjectId::Prop(0);
    let sequence = json!({"id":"talk","cinematic":{"start":{"position":[2_i32,2_i32,2_i32],"yaw_degrees":0_i32,"pitch_degrees":0_i32}},"steps":[{"step":"camera","seconds":0.1_f32,"motion":{"kind":"move","position":[3_i32,2_i32,2_i32]},"aim":{"kind":"keep"}}]});
    doc.preview_cinematic(&before, None, &sequence, owner, None, true)?;
    assert_eq!(doc.raw(), &before, "preview leaves source/history intact");
    assert!(
        doc.apply_cinematic(&before, None, &sequence, ObjectId::Prop(99), None, true)
            .is_err()
    );
    assert!(
        doc.apply_cinematic(&before, Some("missing"), &sequence, owner, None, true)
            .is_err()
    );
    doc.apply_cinematic(&before, None, &sequence, owner, None, true)?;
    let applied = doc.raw().clone();
    assert_eq!(
        value_at(&applied, "/props/0/feltling"),
        value_at(&before, "/props/0/feltling")
    );
    assert_eq!(
        value_at(&applied, "/props/0/bindings/0"),
        value_at(&before, "/props/0/bindings/0")
    );
    assert!(doc.undo() && doc.raw() == &before);
    assert!(doc.redo() && doc.raw() == &applied);
    doc.save()?;
    let reopened = Document::open(&fixture.0)?;
    assert_eq!(reopened.raw(), &applied);
    let mut modified = applied.clone();
    *value_at_mut(&mut modified, "/props/0/bindings/1/when")? = json!([]);
    doc.apply_authoring_edit(&applied, modified.clone())?;
    assert!(
        doc.apply_cinematic(&modified, Some("talk"), &sequence, owner, None, true)
            .is_err()
    );
    assert_eq!(doc.raw(), &modified);
    let level = reopened.level();
    let floor = WalkableFloor::from_level(level);
    let index = CollisionIndex::build(&[]);
    let mut world = EntityWorld::from_level(level);
    let ctx = WorldContext {
        player_walk_speed: 3.75,
        delta_seconds: 0.1,
        feet_from: Vec3::new(2.0, 0.0, 3.0),
        feet: Vec3::new(2.0, 0.0, 3.0),
        eye: Vec3::new(2.0, 1.6, 3.0),
        body_height: 1.8,
        body_half: crate::engine::collision::PLAYER_RADIUS,
        form_model: None,
        walls: &[],
        index: &index,
        floor: &floor,
        nav: None,
    };
    let _start = world
        .dispatch_interaction(Some(0))
        .ok_or("interaction absent")?;
    let token = world.cinematic_token().ok_or("interaction did not start")?;
    let _again = world.dispatch_interaction(Some(0));
    assert_eq!(
        world.cinematic_token(),
        Some(token),
        "active repeat cannot restart"
    );
    world.cancel_cinematic();
    let _retry = world.dispatch_interaction(Some(0));
    assert!(
        world.cinematic_token().is_some_and(|t| t != token),
        "cancel must remain retryable"
    );
    let _finish_tick = world.tick(&ctx);
    let _dispatch_tick = world.tick(&ctx);
    assert!(world.cinematic_token().is_none());
    let _spent = world.dispatch_interaction(Some(0));
    assert!(
        world.cinematic_token().is_none(),
        "successful once interaction is spent"
    );
    world.reset_runtime();
    let _reset = world.dispatch_interaction(Some(0));
    assert!(world.cinematic_token().is_some(), "normal reset rearms");
    Ok(())
}
