//! One small project protects atomic attachment edits, transform history and
//! save/reopen. CPU/source I/O only, no map bake or external assets.
use crate::editor::document::{Document, ObjectId, room_resize::Edge};
use serde_json::json;

#[test]
fn pillar_and_wall_attachments_follow_history_save_and_room_edge_changes() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!("places-trim-authoring-{}", std::process::id()));
    let mut document = Document::create(&root, "trim_authoring", "Trim authoring")?;
    let result = workflow(&mut document, &root);
    let _cleanup = std::fs::remove_dir_all(&root);
    result
}
fn workflow(document: &mut Document, root: &std::path::Path) -> Result<(), String> {
    let original = document.raw().clone();
    let mut candidate = original.clone();
    drop(
        candidate
            .as_object_mut()
            .ok_or("Expected source object")?
            .insert(
                "rooms".into(),
                json!([{"width":8.0_f32,"depth":8.0_f32,"height":3.0_f32}]),
            ),
    );
    drop(candidate.as_object_mut().ok_or("Expected source object")?.insert("walls".into(), json!([{"x":0.0_f32,"z":0.0_f32,"width":8.0_f32,"depth":0.2_f32,"height":3.0_f32,"trim":{"placement":"both","height":0.09_f32,"material":"home:baseboard_white_01"}}])));
    drop(candidate.as_object_mut().ok_or("Expected source object")?.insert("columns".into(), json!([{"x":2.0_f32,"z":3.0_f32,"width":0.6_f32,"depth":1.0_f32,"material":"home:wall_paint_offwhite_01","trim":{"placement":"bottom"}}])));
    drop(candidate.as_object_mut().ok_or("Expected source object")?.insert("pillars".into(), json!([{"x":5.0_f32,"z":3.0_f32,"radius":0.4_f32,"material":"core:pool_tile_wall_01","trim":{"placement":"both"}}])));
    let _preview = document.validate_candidate(&candidate)?;
    assert_eq!(
        document.raw(),
        &original,
        "preview validation must leave source/history unchanged"
    );
    document.apply_authoring_edit(&original, candidate)?;
    let applied = document.raw().clone();
    document.apply_authoring_edit(&applied, applied.clone())?;
    assert!(
        document.undo(),
        "one undo restores a repeated Apply as one real edit"
    );
    assert_eq!(
        document.raw(),
        &original,
        "all attachments share the existing atomic history"
    );
    assert!(document.redo(), "redo restores the full source");
    assert_eq!(
        document.raw(),
        &applied,
        "redo restores both shapes and trim settings"
    );
    let mut pose = document.transform(ObjectId::Pillar(0))?;
    pose.position[0] += 0.5_f32;
    pose.size[0] = 1.2_f32;
    pose.size[2] = 1.2_f32;
    document.set_transform(ObjectId::Pillar(0), pose)?;
    assert_eq!(
        document
            .level()
            .pillars
            .first()
            .and_then(|p| p.trim.as_ref())
            .map(|t| t.placement),
        Some(crate::engine::level::TrimPlacement::Both),
        "moving and resizing retains the attachment"
    );
    document.set_material(ObjectId::Column(0), "home:wallpaper_offwhite_01")?;
    document.resize_room_edge(ObjectId::Room(0), Edge::East, 1.0_f32)?;
    assert!(
        (document.level().walls.first().ok_or("Missing wall")?.width - 9.0_f32).abs() < 0.0001_f32,
        "attached wall follows the existing room-edge operation"
    );
    document.save()?;
    let reopened = Document::open(root)?;
    assert_eq!(
        reopened.raw(),
        document.raw(),
        "save/reopen preserves complete attachments and material choice"
    );
    let saved = document.raw().clone();
    document.delete(ObjectId::Pillar(0))?;
    assert!(
        document.level().pillars.is_empty(),
        "deleting the owner removes its attachment"
    );
    assert!(document.undo(), "delete is reversible");
    assert_eq!(
        document.raw(),
        &saved,
        "undo delete restores complete pillar and attachment"
    );
    let mut invalid = saved.clone();
    *invalid
        .pointer_mut("/walls/0/trim/height")
        .ok_or("Missing wall attachment")? = json!(2.0_f32);
    assert!(
        document.apply_authoring_edit(&saved, invalid).is_err(),
        "bad fit/settings cannot partially mutate history"
    );
    assert_eq!(
        document.raw(),
        &saved,
        "failed fit admission leaves the document exact"
    );
    Ok(())
}
