//! Three essential regressions protect editor data preservation, independent
//! savepoint/history state and failure safety. They reuse two tiny JSON levels,
//! perform no bake/render/resource discovery, and cost only a few file writes
//! in one temporary folder per test. These behaviors have no engine-level
//! coverage: they belong to the editor's raw document and publication boundary.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::Value;

use crate::editor::document::{Document, ObjectId, ObjectKind};
use crate::engine::assets::AssetCatalog;

static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Result<Self, String> {
        let sequence = FIXTURE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "places-editor-document-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).map_err(|error| error.to_string())?;
        let levels = path.join("levels");
        fs::create_dir(&levels).map_err(|error| error.to_string())?;
        write_json(
            &path.join("places.project.json"),
            &serde_json::json!({
                "schema":"places-project", "project_format":1_u32, "id":"document", "name":"Document fixture",
                "entry_level":"entry", "levels":[
                    {"id":"entry", "source":"levels/entry.json"},
                    {"id":"second", "source":"levels/second.json"}
                ]
            }),
        )?;
        for id in ["entry", "second"] {
            write_json(&levels.join(format!("{id}.json")), &scene(id))?;
        }
        Ok(Self(path))
    }

    fn source(&self, id: &str) -> PathBuf {
        self.0.join("levels").join(format!("{id}.json"))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

fn write_json(path: &std::path::Path, value: &Value) -> Result<(), String> {
    fs::write(
        path,
        serde_json::to_vec(value).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn scene(id: &str) -> Value {
    serde_json::json!({
        "schema":"places-level", "format_version":1_u32, "id":id, "name":"Tiny fixture",
        "spawn":{"x":2.0_f32,"z":3.0_f32,"future_spawn":{"preserve":[1_u32,2_u32,"three"]}},
        "rooms":[{"width":4.0_f32,"depth":4.0_f32,"height":2.7_f32,
            "ceiling":{"kind":"gable","ridge":"x","ridge_rise":1.25_f32},
            "future_room":{"opaque":true}}],
        "props":[
            {"model":"core:chair","x":1.0_f32,"z":1.0_f32,"future_prop":{"nested":[false,{"id":"keep"}]}},
            {"model":"core:chair","x":3.0_f32,"z":1.0_f32}
        ],
        "future_scene":{"arbitrary":[null,true,1.25_f32,"unchanged"]}
    })
}

#[test]
fn edits_history_and_reopen_preserve_unknown_data() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let source_text =
        fs::read_to_string(fixture.source("entry")).map_err(|error| error.to_string())?;
    fs::write(
        fixture.source("entry"),
        source_text.replacen(
            "\"future_scene\":",
            "\"future_small\":1e-06,\"future_scene\":",
            1,
        ),
    )
    .map_err(|error| error.to_string())?;
    let manifest_before =
        fs::read(fixture.0.join("places.project.json")).map_err(|error| error.to_string())?;
    let mut document = Document::open(&fixture.0)?;
    let initial = document.raw().clone();
    let mut moved = document.transform(ObjectId::Prop(0))?;
    moved.position = [1.5_f32, 0.0_f32, 1.25_f32];
    moved.yaw_degrees = 90.0_f32;
    moved.size = [1.25_f32; 3];
    document.set_transform(ObjectId::Prop(0), moved)?;
    assert!(
        document.is_dirty(),
        "a transform must mark the document dirty"
    );
    assert!(document.undo(), "the transform is undoable");
    assert_eq!(document.raw(), &initial, "undo restores unknown fields too");
    assert!(!document.is_dirty(), "undo reaches the original savepoint");
    assert!(document.redo(), "the transform is redoable");
    let changed = document.raw().clone();
    document.save_all()?;
    assert!(
        !document.is_dirty(),
        "save marks the exact current revision clean"
    );
    assert!(document.undo(), "history remains available after saving");
    assert!(
        document.is_dirty(),
        "undo away from a savepoint becomes dirty"
    );
    assert!(document.redo(), "redo can return to the saved revision");
    assert!(
        !document.is_dirty(),
        "redo reaches the saved revision exactly"
    );
    let reopened = Document::open(&fixture.0)?;
    assert_eq!(
        reopened.raw(),
        &changed,
        "reopening preserves exact JSON values"
    );
    for key in ["spawn", "rooms", "future_scene", "future_small"] {
        assert_eq!(
            reopened.raw().get(key),
            initial.get(key),
            "untouched source data must survive saving"
        );
    }
    assert_eq!(
        fs::read(fixture.0.join("places.project.json")).map_err(|error| error.to_string())?,
        manifest_before,
        "editing a level never rewrites project metadata"
    );
    // Deletion must not silently renumber chair_2 to chair_1: gameplay targets
    // refer to the engine's implicit identity, not an editor array index.
    document.delete(ObjectId::Prop(0))?;
    assert_eq!(
        document.level().prop_instance_ids(),
        vec!["chair_2"],
        "surviving implicit identities remain stable"
    );
    assert!(document.undo(), "deletion is one atomic history entry");
    assert_eq!(
        document.raw(),
        &changed,
        "undo also removes ID-pinning changes"
    );
    let wall = document.add(ObjectKind::Wall, [0.0_f32, 0.0_f32, 0.0_f32], None)?;
    let duplicate = document.duplicate(wall)?;
    assert_ne!(
        wall, duplicate,
        "duplicate creates a separate supported object"
    );
    Ok(())
}

#[test]
fn level_switch_retains_independent_history_and_savepoints() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut document = Document::open(&fixture.0)?;
    let mut first = document.transform(ObjectId::Room(0))?;
    first.size = [5.0_f32, 2.7_f32, 4.0_f32];
    document.set_transform(ObjectId::Room(0), first)?;
    let entry_changed = document.raw().clone();
    document.select_level("second")?;
    assert!(
        document.is_dirty(),
        "another level's unsaved edit remains visible"
    );
    assert!(
        !document.selected_is_dirty(),
        "newly selected source has its own savepoint"
    );
    let catalog = AssetCatalog::from_json_str(
        r#"{"format_version":2,"assets":[
        {"id":"core:chair","display_name":"Wide chair","asset_class":"core","asset_type":"prop",
         "source":"file","model":"shared/chair.glb","size":[1.8,0.95,0.75],"solid":false}
    ]}"#,
    )?;
    let prop = document.add_with_catalog(
        ObjectKind::Prop,
        [2.0_f32, 0.0_f32, 2.0_f32],
        Some("core:chair"),
        &catalog,
    )?;
    let added = document.level().props.last().ok_or("New prop is missing")?;
    let size = added.size.ok_or("New prop has no collision dimensions")?;
    assert!(
        size.into_iter()
            .zip([1.8_f32, 0.95_f32, 0.75_f32])
            .all(|(actual, expected)| (actual - expected).abs() <= f32::EPSILON),
        "new instances retain real catalog collision dimensions"
    );
    assert!(
        !added.solid && added.physics.is_none(),
        "catalog solidity is retained without activating physics"
    );
    document.set_model(prop, "core:chair")?;
    let second_changed = document.raw().clone();
    document.save_all()?;
    assert!(
        !document.is_dirty(),
        "save-all saves every visited dirty source"
    );
    assert!(document.undo(), "second level keeps its own undo history");
    assert_eq!(
        document.level().props.len(),
        2,
        "prop insertion and catalog collision data undo together"
    );
    assert!(
        document.selected_is_dirty(),
        "second history leaves its own savepoint"
    );
    document.select_level("entry")?;
    assert_eq!(
        document.raw(),
        &entry_changed,
        "switching restores the first document's edit"
    );
    assert!(
        !document.selected_is_dirty(),
        "first level savepoint is independent"
    );
    assert!(document.undo(), "first level keeps its original history");
    assert!(document.redo(), "first level can reach its saved revision");
    let mut reopened = Document::open(&fixture.0)?;
    assert_eq!(
        reopened.raw(),
        &entry_changed,
        "first saved source is exact"
    );
    reopened.select_level("second")?;
    assert_eq!(
        reopened.raw(),
        &second_changed,
        "save-all published the other level too"
    );
    Ok(())
}

#[test]
fn invalid_edits_and_external_changes_leave_sources_and_history_safe() -> Result<(), String> {
    let fixture = Fixture::new()?;
    let mut document = Document::open(&fixture.0)?;
    let initial = document.raw().clone();
    let mut invalid = document.transform(ObjectId::Room(0))?;
    invalid.size = [-1.0_f32, 2.7_f32, 4.0_f32];
    assert!(
        document.set_transform(ObjectId::Room(0), invalid).is_err(),
        "invalid geometry is rejected before history mutation"
    );
    assert_eq!(
        document.raw(),
        &initial,
        "failed edits retain the exact document"
    );
    assert!(!document.can_undo(), "failed edits create no history entry");
    let mut changed = document.transform(ObjectId::Prop(0))?;
    changed.position = [1.5_f32, 0.0_f32, 1.5_f32];
    document.set_transform(ObjectId::Prop(0), changed)?;
    let entry_before = fs::read(fixture.source("entry")).map_err(|error| error.to_string())?;
    document.select_level("second")?;
    let external = b"{\"external editor owns this now\":true}";
    fs::write(fixture.source("second"), external).map_err(|error| error.to_string())?;
    assert!(
        document.save_all().is_err(),
        "all documents are preflighted before any source overwrite"
    );
    assert_eq!(
        fs::read(fixture.source("second")).map_err(|error| error.to_string())?,
        external,
        "conflicting external content is never overwritten"
    );
    assert_eq!(
        fs::read(fixture.source("entry")).map_err(|error| error.to_string())?,
        entry_before,
        "save-all conflict leaves earlier sources untouched"
    );
    assert!(document.is_dirty(), "failed save retains unsaved work");
    document.select_level("entry")?;
    assert!(document.undo(), "failed save retains usable undo history");
    assert_eq!(
        document.raw(),
        &initial,
        "undo still restores the complete original document"
    );
    assert!(
        Document::create(&fixture.0, "other", "Other").is_err(),
        "new-project never overwrites an existing folder"
    );
    assert_eq!(
        fs::read(fixture.source("second")).map_err(|error| error.to_string())?,
        external,
        "failed project creation preserves user files"
    );
    let original_text = String::from_utf8(entry_before).map_err(|error| error.to_string())?;
    for unsupported in [
        "184467440737095516161234567890",
        "0.123456789012345678901234567890",
    ] {
        let precise = original_text.replacen(
            "\"future_scene\":",
            &format!("\"future_precision\":{unsupported},\"future_scene\":"),
            1,
        );
        fs::write(fixture.source("entry"), &precise).map_err(|error| error.to_string())?;
        let error = Document::open(&fixture.0)
            .err()
            .ok_or("Lossy source was accepted")?;
        assert!(
            error.contains("numeric precision"),
            "unsupported precision needs a useful error"
        );
        assert_eq!(
            fs::read_to_string(fixture.source("entry"))
                .map_err(|read_error| read_error.to_string())?,
            precise,
            "refusing excessive precision never rewrites source"
        );
    }
    let ambiguous = original_text.replacen(
        "\"future_scene\":",
        "\"future_scene\":false,\"future_scene\":",
        1,
    );
    fs::write(fixture.source("entry"), &ambiguous).map_err(|error| error.to_string())?;
    assert!(
        Document::open(&fixture.0).is_err(),
        "duplicate unknown keys are refused before they could be silently lost"
    );
    assert_eq!(
        fs::read_to_string(fixture.source("entry")).map_err(|error| error.to_string())?,
        ambiguous,
        "refusing an ambiguous source never rewrites it"
    );
    Ok(())
}
