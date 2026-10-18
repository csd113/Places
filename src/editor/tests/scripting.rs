//! Two temporary-file regressions cover script buffers' distinct publication
//! boundary: native edits/linking/reopen and script conflicts blocking a mixed
//! save before any write. No baking/assets/native window; only tiny file writes.

use crate::editor::document::{Document, ObjectKind};
use serde_json::json;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Fixture(std::path::PathBuf);
impl Fixture {
    fn new() -> Result<(Self, Document), String> {
        let parent = std::env::temp_dir().join(format!(
            "places-script-editor-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&parent).map_err(|e| e.to_string())?;
        let doc = Document::create(&parent.join("project"), "script_editor", "Script fixture")?;
        Ok((Self(parent), doc))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn script_buffer_link_save_reopen_and_invalid_check_preserve_sources() -> Result<(), String> {
    let (fixture, mut doc) = Fixture::new()?;
    let manifest = fs::read(doc.root().join("places.project.json")).map_err(|e| e.to_string())?;
    let name = doc.create_script()?;
    let source = "--!strict\nreturn {entered=function(api,event) api.notice('arrived') end}\n";
    doc.set_script_text(&name, source.into())?;
    doc.set_script_text(&name, "-- temporary edit".into())?;
    assert!(
        doc.script_history(&name, false),
        "source undo restores prior text"
    );
    assert_eq!(
        doc.script_text(&name),
        Some(source),
        "source undo independent of map history"
    );
    assert!(doc.script_history(&name, true), "source redo available");
    assert!(
        doc.script_history(&name, false),
        "source undo retains the intended saved source"
    );
    let trigger = doc.add(ObjectKind::Trigger, [1.0, 0.0, 1.0], None)?;
    doc.add_trigger_action(
        trigger,
        json!({"action":"run_script","script":name,"handler":"entered"}),
    )?;
    doc.check_scripts()?;
    assert!(doc.is_dirty(), "scripts join unsaved guard");
    doc.save_all()?;
    assert!(!doc.is_dirty(), "all script/map savepoints clean");
    assert_eq!(
        manifest,
        fs::read(doc.root().join("places.project.json")).map_err(|e| e.to_string())?,
        "manifest untouched"
    );
    let saved = doc.raw().clone();
    let mut reopened = Document::open(&fixture.0.join("project"))?;
    reopened.open_script(&name)?;
    assert_eq!(
        reopened.script_text(&name),
        Some(source),
        "source bytes reopened"
    );
    assert_eq!(reopened.raw(), &saved, "trigger link roundtrip");
    reopened.set_script_text(&name, "return {".into())?;
    assert!(
        reopened.check_scripts().is_err(),
        "invalid script check useful"
    );
    assert_eq!(reopened.raw(), &saved, "invalid check does not change map");
    assert_eq!(
        fs::read_to_string(reopened.root().join(&name)).map_err(|e| e.to_string())?,
        source,
        "checking never publishes buffer"
    );
    Ok(())
}

#[test]
fn external_script_conflict_blocks_map_save_before_publication() -> Result<(), String> {
    let (_fixture, mut doc) = Fixture::new()?;
    let name = doc.create_script()?;
    doc.save_all()?;
    let map_path = doc
        .project()
        .resolve(&doc.manifest().levels.first().ok_or("Missing level")?.source)?;
    let map_bytes = fs::read(&map_path).map_err(|e| e.to_string())?;
    doc.set_script_text(&name, "return {changed=function() end}".into())?;
    let _trigger = doc.add(ObjectKind::Trigger, [1.0, 0.0, 1.0], None)?;
    fs::write(doc.root().join(&name), "return {external=function() end}")
        .map_err(|e| e.to_string())?;
    assert!(
        doc.save_all().is_err(),
        "external script conflict refuses entire preflight"
    );
    assert_eq!(
        fs::read(map_path).map_err(|e| e.to_string())?,
        map_bytes,
        "map not partly saved on conflict"
    );
    assert!(doc.is_dirty(), "buffers retained after refusal");
    assert!(
        doc.open_script("scripts/../outside.luau").is_err(),
        "undeclared traversal refused"
    );
    Ok(())
}
