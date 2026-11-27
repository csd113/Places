use super::{Resolved, Step, stage_action};
use crate::{
    editor::{Document, ObjectId, document::objects, focus::Focus, railings},
    engine::assets::AssetCatalog,
};
use glam::Vec3;
use serde_json::Value;
use std::{fs, path::PathBuf};

struct Project(PathBuf);

impl Drop for Project {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

// Two accepted native semantics are installed as one history entry. A late
// rejected operation and a refused final grant must leave the original state
// and both history cursors unchanged. No native mesh build or GPU is needed.
#[test]
fn staged_edits_match_native_with_one_history_install() -> Result<(), String> {
    let fixture =
        Project(std::env::temp_dir().join(format!("places-api03-batch-{}", std::process::id())));
    let catalog = AssetCatalog::from_json_str(include_str!("../../../assets/catalog.json"))?;
    let light = ObjectId::Light(0);
    let mut setup = Document::create(&fixture.0, "batch", "Pilot batch")?;
    let mut pose = setup.transform(light)?;
    pose.position[1] = 1.6;
    setup.set_transform(light, pose)?;
    setup.set_gameplay_asset(light, "core:pool_light_round", &catalog)?;
    setup.save_all()?;
    drop(setup);

    let mut doc = Document::open(&fixture.0)?;
    let before = doc.raw().clone();
    let stamp = doc.scene_stamp().clone();
    let focus = Focus::default();
    let asset = "core:pool_light_wall";
    let first = stage_action(
        &focus,
        &catalog,
        &doc,
        &before,
        doc.level(),
        Some(light),
        Resolved::Fixture { light: 0, asset },
    )
    .map_err(|failure| failure.message)?;

    check_late_rejection(&doc, &catalog, &focus, &before, &first);

    let start = [1.0, 0.0, 1.75];
    let end = [2.5, 0.0, 1.75];
    let second = stage_action(
        &focus,
        &catalog,
        &doc,
        &first.raw,
        &first.level,
        first.selection,
        Resolved::Railing(vec![start, end]),
    )
    .map_err(|failure| failure.message)?;

    let mut native = Document::open(&fixture.0)?;
    native.set_gameplay_asset(light, asset, &catalog)?;
    let (raw, _owner_room) = railings::candidate(&native, start, &[], end)?;
    native.apply_authoring_edit(&native.raw().clone(), raw)?;
    assert_eq!(
        &second.raw,
        native.raw(),
        "the complete candidate matches two native edits"
    );

    assert!(
        doc.install_agent_batch(
            &stamp,
            second.raw.clone(),
            second.level.clone(),
            second.byte_count,
            || false
        )
        .is_err(),
        "final grant denial must refuse even a prepared valid scene"
    );
    assert_eq!(doc.raw(), &before, "denial preserves the full scene");
    assert!(
        !doc.can_undo() && !doc.can_redo(),
        "denial preserves both cursors"
    );
    doc.install_agent_batch(&stamp, second.raw, second.level, second.byte_count, || true)?;
    assert_eq!(
        doc.raw(),
        native.raw(),
        "applied scene matches native semantics"
    );
    assert!(doc.is_dirty() && doc.undo(), "one native Undo is available");
    assert_eq!(
        doc.raw(),
        &before,
        "a single Undo restores the exact baseline"
    );
    assert!(
        !doc.is_dirty() && !doc.can_undo() && doc.redo(),
        "one redo restores the batch"
    );
    assert_eq!(
        doc.raw(),
        native.raw(),
        "Redo restores the exact accepted candidate"
    );
    assert!(
        !doc.scene_stamp().matches(&stamp),
        "Undo and Redo do not revive the old context"
    );

    check_locked_target(&mut doc, &catalog, &focus, light)
}

fn check_late_rejection(
    doc: &Document,
    catalog: &AssetCatalog,
    focus: &Focus,
    before: &Value,
    first: &Step,
) {
    let equal = stage_action(
        focus,
        catalog,
        doc,
        &first.raw,
        &first.level,
        first.selection,
        Resolved::Escalator {
            lower: Vec3::new(1.5, 0.0, 1.5),
            upper: Vec3::new(2.5, 0.0, 1.5),
        },
    );
    assert!(
        equal.is_err(),
        "an equal-height landing cannot follow a valid staged edit"
    );
    assert_eq!(
        doc.raw(),
        before,
        "a partial batch never installs an earlier action"
    );
    assert!(
        !doc.is_dirty() && !doc.can_undo() && !doc.can_redo(),
        "rejection preserves history and dirty state"
    );
}

fn check_locked_target(
    doc: &mut Document,
    catalog: &AssetCatalog,
    focus: &Focus,
    light: ObjectId,
) -> Result<(), String> {
    let expected = doc.raw().clone();
    let mut locked = expected.clone();
    drop(
        objects::object_mut(&mut locked, light)?.insert("editor_locked".into(), Value::Bool(true)),
    );
    doc.apply_authoring_edit(&expected, locked.clone())?;
    let locked_result = stage_action(
        focus,
        catalog,
        doc,
        &locked,
        doc.level(),
        Some(light),
        Resolved::Fixture {
            light: 0,
            asset: "core:pool_light_round",
        },
    );
    assert!(
        locked_result.is_err_and(|failure| matches!(failure.code, super::Code::LockedTarget)),
        "the selected locked light is rejected before draft or install"
    );
    assert_eq!(
        doc.raw(),
        &locked,
        "a locked-target request cannot change the scene"
    );
    Ok(())
}
