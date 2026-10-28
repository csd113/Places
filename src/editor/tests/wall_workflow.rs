//! Essential CPU regression: opening/leaf/trim edits must share atomic history,
//! preserve attachments on both endpoints, and survive save/reopen. One six-metre
//! temporary project; no assets, renderer or bake. Focus identity coverage guards
//! selection of the wrong source record after hiding a layer or isolating a room.
use super::super::document::{Document, ObjectId};
use serde_json::json;

fn apply(doc: &mut Document, raw: serde_json::Value) -> Result<(), String> {
    let expected = doc.raw().clone();
    doc.apply_authoring_edit(&expected, raw)
}
fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.001
}
#[test]
fn wall_attachments_follow_move_endpoint_history_and_saved_source() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!("places-wall-workflow-{}", std::process::id()));
    let mut doc = Document::create(&root, "wall_workflow", "Wall workflow")?;
    let result = workflow(&mut doc, &root);
    let _cleanup = std::fs::remove_dir_all(&root);
    result
}
fn workflow(doc: &mut Document, root: &std::path::Path) -> Result<(), String> {
    let original = doc.raw().clone();
    let (wall_source, id) = doc.wall_segment_candidate([1.0, 0.0, 2.0], [5.0, 0.0, 2.0])?;
    assert_eq!(
        doc.raw(),
        &original,
        "placement preview cannot enter source or history"
    );
    apply(doc, wall_source)?;
    let (door_source, _) = doc.doorway_candidate(0, 2.0, true)?;
    apply(doc, door_source)?;
    let mut trim_source = doc.raw().clone();
    let trim_wall = trim_source
        .get_mut("walls")
        .and_then(|a| a.get_mut(0))
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("Missing wall")?;
    drop(trim_wall.insert("trim".into(), json!({"placement":"both"})));
    apply(doc, trim_source)?;
    add_decal_fixture(doc)?;
    let before = doc.raw().clone();
    let hinge = doc.level().doors.first().ok_or("Missing door")?.z;
    let mut moved_pose = doc.transform(id)?;
    moved_pose.position[2] += 0.5;
    doc.set_transform(id, moved_pose)?;
    assert!(
        close(
            doc.level().doors.first().ok_or("Missing door")?.z,
            hinge + 0.5
        ),
        "wall move must carry functional leaf"
    );
    assert!(
        close(
            doc.level().decals.first().ok_or("Missing floor decal")?.z,
            1.95
        ),
        "wall moves must leave unrelated floor decals fixed"
    );
    assert!(
        close(
            doc.level().decals.get(1).ok_or("Missing wall decal")?.z,
            2.4
        ),
        "wall-facing decals follow the same atomic movement"
    );
    assert!(doc.undo(), "wall move creates one undo step");
    assert_eq!(doc.raw(), &before, "undo restores leaf and wall together");
    assert!(doc.redo(), "redo restores the entire attachment edit");
    let door_x = doc.level().doors.first().ok_or("Missing door")?.x;
    let mut extended_pose = doc.transform(id)?;
    extended_pose.position[0] -= 0.5;
    extended_pose.size[0] += 0.5;
    doc.set_transform(id, extended_pose)?;
    let wall = doc.level().walls.first().ok_or("Missing wall")?;
    assert!(
        close(wall.openings.first().ok_or("Missing opening")?.offset, 2.0),
        "start extension rebases the opening to keep its world position"
    );
    assert!(
        close(doc.level().doors.first().ok_or("Missing door")?.x, door_x),
        "lengthening an endpoint keeps the leaf in world coordinates"
    );
    assert!(wall.trim.is_some(), "automatic trim remains attached");
    let retained = doc.raw().clone();
    let mut short_pose = doc.transform(id)?;
    short_pose.size[0] = 1.0;
    assert!(
        doc.set_transform(id, short_pose).is_err(),
        "shortening through an opening must refuse atomically"
    );
    assert_eq!(
        doc.raw(),
        &retained,
        "failed endpoint edits preserve source"
    );
    let mut raw = doc.raw().clone();
    let r = raw
        .get_mut("doors")
        .and_then(|a| a.get_mut(0))
        .and_then(serde_json::Value::as_object_mut)
        .ok_or("Missing door")?;
    drop(r.insert("editor_locked".into(), json!(true)));
    apply(doc, raw)?;
    let mut locked_pose = doc.transform(id)?;
    locked_pose.position[2] += 0.5;
    assert!(
        doc.set_transform(id, locked_pose).is_err(),
        "locked attached door prevents indirect wall edits"
    );
    doc.save_all()?;
    let reopened = Document::open(root)?;
    assert_eq!(
        reopened.raw(),
        doc.raw(),
        "save/reopen preserves exact attachment and lock metadata"
    );
    check_focus(&reopened, doc)
}
fn check_focus(reopened: &Document, doc: &Document) -> Result<(), String> {
    let mut focus_level = reopened.level().clone();
    focus_level
        .walls
        .push(focus_level.walls.first().ok_or("Missing wall")?.clone());
    let (filtered, map) = super::super::focus::Focus {
        hidden: [false, false, true, false],
        isolated: Some(ObjectId::Wall(1)),
    }
    .preview(&focus_level);
    assert!(
        filtered.ceiling_lights.is_empty(),
        "hidden lights do not enter preview"
    );
    assert!(
        map.contains(&(ObjectId::Wall(0), ObjectId::Wall(1))),
        "filtered selection maps back to source identity"
    );
    assert_eq!(
        reopened.raw(),
        doc.raw(),
        "visibility filtering cannot change authoritative source"
    );
    Ok(())
}

// Pure coordinate math protects a different failure: visible controls and pointer
// hit regions must agree at small, wide, tall and doubled-density drawable sizes.
#[test]
fn responsive_layout_keeps_hits_in_sync_with_logical_and_retina_coordinates() -> Result<(), String>
{
    use crate::editor::layout::{Layout, window_to_ui};
    use crate::engine::render::DrawableSize;
    for (width, height) in [(960_u16, 544_u16), (1280, 720), (1920, 1080), (1000, 900)] {
        for density in [1_u16, 2] {
            let drawable = DrawableSize::new(
                u32::from(width) * u32::from(density),
                u32::from(height) * u32::from(density),
            );
            let logical = (u32::from(width), u32::from(height));
            for content_scale in [1.0_f32, 1.25, 1.5, 2.0] {
                let layout = Layout::new(drawable, logical, content_scale);
                for modal in [false, true] {
                    let point = layout.point([421.0, 66.0], modal);
                    let [left, top, right, bottom] = layout.bounds;
                    let window = [
                        (point[0] - left) / (right - left) * f32::from(width),
                        (point[1] - top) / (bottom - top) * f32::from(height),
                    ];
                    let actual =
                        window_to_ui(drawable, logical, window).ok_or("Missing UI point")?;
                    assert!(
                        close(actual[0], point[0]) && close(actual[1], point[1]),
                        "DPI conversion must agree with the rendered control"
                    );
                    assert!(
                        point[0] >= left
                            && point[0] <= right
                            && point[1] >= top
                            && point[1] <= bottom,
                        "controls remain inside the usable window"
                    );
                }
                let [left, top, right, bottom] = layout.viewport();
                assert!(
                    right - left > 300.0 && bottom - top > 180.0,
                    "responsive viewport retains usable editing area"
                );
            }
        }
    }
    Ok(())
}

fn add_decal_fixture(doc: &mut Document) -> Result<(), String> {
    let mut source = doc.raw().clone();
    drop(source.as_object_mut().ok_or("Missing scene")?.insert("decals".into(),json!([
        {"x":2.0_f32,"y":0.01_f32,"z":1.95_f32,"width":0.2_f32,"height":0.2_f32,"material":"core:decal_test_01","surface":"floor","align":"none"},
        {"x":2.0_f32,"y":1.0_f32,"z":1.9_f32,"width":0.2_f32,"height":0.2_f32,"material":"core:decal_test_01","surface":"wall_north","align":"none"}
    ])));
    apply(doc, source)
}
