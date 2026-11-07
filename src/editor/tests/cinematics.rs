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
