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
