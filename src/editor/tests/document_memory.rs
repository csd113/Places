//! One tiny source fixture proves budget refusal preserves unsaved author data
//! and its undo history. This adds no asset discovery, rendering or baking cost.
use super::Document;

#[test]
fn memory_refusal_preserves_unsaved_source_and_undo() -> Result<(), String> {
    let path = std::env::temp_dir().join(format!("places-memory-document-{}", std::process::id()));
    let mut document = Document::create(&path, "memory", "Memory fixture")?;
    let result = (|| {
        let mut edit = document.raw().clone();
        let name = edit.get_mut("name").ok_or("starter name missing")?;
        *name = serde_json::json!("Unsaved author edit");
        document.accept_edit(edit)?;
        let before = document.raw().clone();
        assert!(document.admit_document_bytes(usize::MAX).is_err());
        assert_eq!(document.raw(), &before);
        assert!(document.is_dirty() && document.can_undo());
        assert!(document.undo(), "refusal cannot consume the undo history");
        Ok(())
    })();
    drop(document);
    std::fs::remove_dir_all(&path).map_err(|error| error.to_string())?;
    result
}

// A constrained starter tests the native fixture workflow without allocating a
// large scene. It guards budget ordering as well as the cost of accepted edits.
#[test]
fn fixture_commit_checks_budget_before_a_single_full_validation() -> Result<(), String> {
    use super::{ObjectId, ObjectKind, document_memory_error, persistence::RAW_VALIDATION_CALLS};
    use crate::engine::assets::AssetCatalog;

    let path = std::env::temp_dir().join(format!("places-fixture-budget-{}", std::process::id()));
    let mut document = Document::create(&path, "fixture_budget", "Fixture budget")?;
    let result = (|| {
        let catalog = AssetCatalog::from_json_str(include_str!("../../../assets/catalog.json"))?;
        let original = document.raw().clone();
        let _existing_edit = document.add(ObjectKind::Light, [2.0, 1.0, 2.0], None)?;
        let before = document.raw().clone();
        let budget = document.heap_budget;
        document.heap_budget = document.heap_charge();
        let history = document.resource_snapshot();

        // This wall fixture would fail scene validation for missing height.
        // Budget refusal must happen first, without entering the validator.
        RAW_VALIDATION_CALLS.set(0);
        assert_eq!(
            document.set_gameplay_asset(ObjectId::Light(0), "core:pool_light_wall", &catalog),
            Err(document_memory_error())
        );
        assert_eq!(RAW_VALIDATION_CALLS.get(), 0);
        assert_eq!(document.raw(), &before);
        assert_eq!(document.resource_snapshot(), history);
        assert!(document.is_dirty() && document.can_undo() && !document.can_redo());

        document.heap_budget = budget;
        document.set_gameplay_asset(ObjectId::Light(0), "core:pool_light_round", &catalog)?;
        assert_eq!(
            RAW_VALIDATION_CALLS.get(),
            1,
            "native commit must validate exactly once"
        );
        assert!(document.undo());
        assert_eq!(document.raw(), &before);
        assert!(document.undo());
        assert_eq!(document.raw(), &original);
        assert!(!document.can_undo());
        Ok(())
    })();
    drop(document);
    std::fs::remove_dir_all(&path).map_err(|error| error.to_string())?;
    result
}

// Fragmented unknown JSON previously bypassed admission on first Open. A small
// injectable budget also proves map-switch refusal preserves dirty source and
// history; no stress allocations, renderer or asset/bake work are needed.
#[test]
fn initial_and_selected_map_admission_preserve_unknown_fields_and_current_work()
-> Result<(), String> {
    let path = std::env::temp_dir().join(format!("places-memory-open-{}", std::process::id()));
    let starter = Document::create(&path, "entry", "Memory open")?;
    drop(starter);
    let result = (|| {
        let source = path.join("levels/entry.json");
        let mut raw: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&source).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        drop(raw.as_object_mut().ok_or("Missing object")?.insert(
            "unknown_fragmented".into(),
            serde_json::Value::Array((0_u8..32).map(|_| serde_json::json!({"a":[]})).collect()),
        ));
        let bytes = serde_json::to_vec(&raw).map_err(|e| e.to_string())?;
        std::fs::write(&source, &bytes).map_err(|e| e.to_string())?;
        let charge = super::source::opening_charge(&bytes)?;
        assert!(
            charge > bytes.len().saturating_mul(8),
            "serialized length misses fragmentation"
        );
        assert!(Document::open_with_budget(&path, charge.saturating_sub(1)).is_err());
        let mut doc = Document::open_with_budget(&path, charge)?;
        assert_eq!(doc.raw(), &raw, "below-budget unknown data survives");
        doc.heap_budget = super::MAX_DOCUMENT_HEAP_CHARGE;
        doc.add_level("second")?;
        doc.select_level("entry")?;
        let mut edited = doc.raw().clone();
        *edited.get_mut("name").ok_or("Missing name")? = serde_json::json!("Unsaved");
        doc.heap_budget = super::MAX_DOCUMENT_HEAP_CHARGE;
        doc.accept_edit(edited)?;
        // Evict only the clean cached second map, then force its reload to fail.
        doc.other_levels.clear();
        doc.heap_budget = doc.heap_charge();
        let before = doc.raw().clone();
        assert!(doc.select_level("second").is_err());
        assert_eq!(doc.level_id(), "entry");
        assert_eq!(doc.raw(), &before);
        assert!(doc.is_dirty() && doc.can_undo());
        assert!(doc.undo());
        Ok(())
    })();
    std::fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
    result
}

// Exercise real editor selection refusal after committed membership changes.
// Add and Import share one disposable two-level project and the writer lock.
#[test]
fn add_and_import_keep_committed_sources_when_selection_cannot_refresh() -> Result<(), String> {
    use crate::engine::project::{MANIFEST_NAME, ProjectFolder};
    use std::fs;

    let path = std::env::temp_dir().join(format!("places-member-refresh-{}", std::process::id()));
    let mut starter = Document::create(&path, "entry", "Membership fixture")?;
    starter.add_level("second")?;
    drop(starter);
    let result = (|| {
        let mut editor = Document::open(&path)?;
        editor.heap_budget = editor.heap_charge();
        let manifest_path = path.join(MANIFEST_NAME);
        let before = fs::read(&manifest_path).map_err(|error| error.to_string())?;
        let writer = super::member_publication::lock(editor.root())?;
        let busy = editor
            .add_level("third")
            .err()
            .ok_or("Concurrent writer was admitted")?;
        assert!(busy.contains("Another editor"), "{busy}");
        assert_eq!(
            fs::read(&manifest_path).map_err(|error| error.to_string())?,
            before
        );
        assert!(!path.join("levels/third.json").exists());
        drop(writer);

        let problem = editor
            .add_level("third")
            .err()
            .ok_or("Selection unexpectedly fit")?;
        assert!(
            problem.contains("was published, but could not be selected"),
            "{problem}"
        );
        assert_eq!(editor.level_id(), "entry");
        let _row = editor.manifest().level(Some("third"))?;
        let project = ProjectFolder::open(&path)?;
        assert_eq!(project.read_level(Some("third"))?.2.id, "third");

        let incoming = path.join("incoming.json");
        let bytes =
            serde_json::to_vec_pretty(&super::persistence::starter_scene("imported", "Imported"))
                .map_err(|error| error.to_string())?;
        fs::write(&incoming, &bytes).map_err(|error| error.to_string())?;
        let import_problem = editor
            .import_level(&incoming)
            .err()
            .ok_or("Import selection unexpectedly fit")?;
        assert!(
            import_problem.contains("was published, but could not be selected"),
            "{import_problem}"
        );
        assert_eq!(editor.level_id(), "entry");
        let _imported_row = editor.manifest().level(Some("imported"))?;
        let imported = ProjectFolder::open(&path)?.read_level(Some("imported"))?;
        assert_eq!(imported.1, bytes);
        Ok(())
    })();
    fs::remove_dir_all(&path).map_err(|error| error.to_string())?;
    result
}
