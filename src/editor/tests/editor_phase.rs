//! Small source-only regressions protect edge relationship edits and ownership
//! transitions that engine tests cannot cover. Each uses one tiny temporary
//! project; no bake, GPU, external content, or full-map fixture is involved.
use crate::editor::document::{Document, ObjectId, room_resize::Edge};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Result<(Self, Document), String> {
        let root = std::env::temp_dir().join(format!(
            "places-editor-phase-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).map_err(|e| e.to_string())?;
        let mut doc = Document::create(&root.join("project"), "editor_phase", "Editor phase")?;
        let old = doc.raw().clone();
        let mut raw = old.clone();
        drop(raw.as_object_mut().ok_or("Expected JSON object")?.insert("rooms".into(), json!([{"x":0.0_f32,"z":0.0_f32,"width":8.0_f32,"depth":8.0_f32,"height":3.0_f32,"unique_annotation":"retain"}])));
        drop(raw.as_object_mut().ok_or("Expected JSON object")?.insert("walls".into(), json!([
            {"x":0.0_f32,"z":0.0_f32,"width":8.0_f32,"depth":0.2_f32,"height":3.0_f32,"openings":[{"offset":3.0_f32,"width":1.0_f32,"height":2.1_f32}]},
            {"x":0.0_f32,"z":0.0_f32,"width":0.2_f32,"depth":8.0_f32,"height":3.0_f32},
            {"x":7.8_f32,"z":0.0_f32,"width":0.2_f32,"depth":8.0_f32,"height":3.0_f32}])));
        drop(raw.as_object_mut().ok_or("Expected JSON object")?.insert(
            "props".into(),
            json!([{"id":"editor_actor","model":"mannequin","x":2.0_f32,"z":3.0_f32,"components":[
            {"component":"nav_agent","radius":0.3_f32,"height":1.8_f32,"speed_mps":1.2_f32},
            {"component":"ai","behavior":"idler"}]}]),
        ));
        doc.apply_authoring_edit(&old, raw)?;
        Ok((Self(root), doc))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn room_edge_keeps_opening_world_position_and_history_atomic() -> Result<(), String> {
    let (_fixture, mut doc) = Fixture::new()?;
    let initial = doc.raw().clone();
    let mut with_decal = initial.clone();
    drop(with_decal.as_object_mut().ok_or("Expected object")?.insert("decals".into(), json!([{"x":0.2_f32,"y":1.0_f32,"z":4.0_f32,"width":0.5_f32,"height":0.5_f32,"material":"core:decal_test_01","surface":"wall_east","align":"none"}])));
    doc.apply_authoring_edit(&initial, with_decal)?;
    let before = doc.raw().clone();
    let candidate = doc.room_edge_candidate(ObjectId::Room(0), Edge::West, -2.0_f32)?;
    assert_eq!(
        doc.raw(),
        &before,
        "preview cannot mutate source or history"
    );
    assert_eq!(
        candidate.pointer("/walls/0/openings/0/offset"),
        Some(&json!(5.0_f32)),
        "minimum edge extends wall while opening world X remains 3m"
    );
    assert_eq!(
        candidate.pointer("/decals/0/x"),
        Some(&json!(-1.8_f32)),
        "wall decal remains on its moved slab"
    );
    doc.resize_room_edge(ObjectId::Room(0), Edge::West, -2.0_f32)?;
    let room = doc.level().rooms.first().ok_or("Missing room")?;
    assert!(
        (room.x + room.width - 8.0_f32).abs() < 0.0001_f32,
        "opposite edge stays fixed"
    );
    assert_eq!(
        doc.raw().pointer("/rooms/0/unique_annotation"),
        Some(&json!("retain")),
        "unknown fields survive"
    );
    assert!(doc.undo(), "one undo restores the entire compound edit");
    assert_eq!(doc.raw(), &before, "walls and openings restore with room");
    assert!(doc.redo(), "compound resize can redo");
    let accepted = doc.raw().clone();
    assert!(
        doc.resize_room_edge(ObjectId::Room(0), Edge::West, 7.0_f32)
            .is_err(),
        "occupied/support-losing shrink is refused"
    );
    assert_eq!(doc.raw(), &accepted, "failed resize is nonmutating");
    Ok(())
}

#[test]
fn connected_room_outer_edge_resizes_but_shared_edge_is_refused() -> Result<(), String> {
    let (_fixture, mut doc) = Fixture::new()?;
    let before = doc.raw().clone();
    let mut raw = before.clone();
    raw.get_mut("rooms")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or("Missing rooms")?
        .push(json!({"x":8.0_f32,"z":0.0_f32,"width":4.0_f32,"depth":8.0_f32,"height":3.0_f32}));
    doc.apply_authoring_edit(&before, raw)?;
    assert!(
        doc.room_edge_candidate(ObjectId::Room(0), Edge::East, 1.0_f32)
            .is_err(),
        "shared room boundary cannot be inferred safely"
    );
    doc.resize_room_edge(ObjectId::Room(0), Edge::West, -1.0_f32)?;
    assert!(
        (doc.level().rooms.get(1).ok_or("Missing connected room")?.x - 8.0_f32).abs() < 0.0001_f32,
        "neighbor remains fixed"
    );
    Ok(())
}

fn plan() -> serde_json::Value {
    json!({"component":"ai","behavior":"idler","tasks":[{"key":"walk","priority":20_u8,
    "definition":{"task":"routine","looped":false,"steps":[{"task":"move","point":[4.0_f32,0.0_f32,4.0_f32]},{"task":"wait","seconds":0.5_f32}]}}],
    "senses":[{"filter":{"sense":"hear","categories":["door"],"radius":8.0_f32},"response":{"response":"investigate","seconds":4.0_f32,"radius":2.0_f32}}],"receive_alerts":false})
}

#[test]
fn visual_custom_ownership_roundtrip_preserves_edited_sources_and_references() -> Result<(), String>
{
    let (fixture, mut doc) = Fixture::new()?;
    let id = ObjectId::Prop(0);
    let visual = plan();
    doc.apply_behavior(id, &visual)?;
    let source = doc.generate_behavior_script(id, &visual)?;
    doc.check_scripts()?;
    let custom = format!(
        "{}\n-- user-owned annotation\n",
        doc.script_text(&source).ok_or("Missing source")?
    );
    doc.set_script_text(&source, custom.clone())?;
    let replacement = doc.generate_behavior_script(id, &visual)?;
    assert_ne!(source, replacement, "regeneration uses a new unique module");
    assert_eq!(
        doc.script_text(&source),
        Some(custom.as_str()),
        "custom source never overwritten"
    );
    assert!(
        !doc.script_paths().contains(&source),
        "old execution owner unlinked"
    );
    doc.apply_behavior(id, &visual)?;
    assert!(
        doc.behavior_script(id).is_none(),
        "visual execution owns the NPC again"
    );
    assert!(
        !doc.script_paths().contains(&replacement),
        "visual and generated module cannot both execute"
    );
    doc.save_all()?;
    let reopened = Document::open(&fixture.0.join("project"))?;
    assert_eq!(
        reopened.behavior_value(id)?,
        visual,
        "visual plan survives save/reopen"
    );
    assert_eq!(
        fs::read_to_string(doc.root().join(source)).map_err(|e| e.to_string())?,
        custom,
        "unlinked edited source is preserved on disk"
    );
    let mut invalid = visual.clone();
    *invalid
        .pointer_mut("/tasks/0/definition")
        .ok_or("Missing task")? =
        json!({"task":"follow","target":"deleted_npc","distance":1.6_f32,"memory_seconds":3.0_f32});
    assert!(
        doc.apply_behavior(id, &invalid).is_err(),
        "deleted references fail before admission"
    );
    *invalid
        .pointer_mut("/tasks/0/definition")
        .ok_or("Missing task")? = json!({"task":"routine","steps":[],"looped":true});
    assert!(
        doc.apply_behavior(id, &invalid).is_err(),
        "empty immediate loop is refused"
    );
    Ok(())
}

#[test]
fn invalid_event_template_has_no_partial_module_or_scene_link() -> Result<(), String> {
    let (_fixture, mut doc) = Fixture::new()?;
    let before = doc.raw().clone();
    assert!(
        doc.create_event_template("return {}", Some(ObjectId::Room(0)), "interact", true)
            .is_err(),
        "unsupported event owner is refused before file creation"
    );
    assert_eq!(doc.raw(), &before, "failed form leaves document untouched");
    let path = doc.create_event_template(
        "return {handle_event=function(api,event) api.notice('hello') end}",
        Some(ObjectId::Prop(0)),
        "interact",
        true,
    )?;
    doc.check_scripts()?;
    assert!(doc.undo(), "template scene/link is one undo step");
    assert_eq!(
        doc.raw(),
        &before,
        "undo removes binding, capability and link together"
    );
    assert!(
        doc.root().join(path).is_file(),
        "undo never destroys the new source file"
    );
    Ok(())
}
