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
