//! A tiny two-level document protects launch identity and raw/save/history
//! preservation. No assets, render or bake; two starter JSON files and one undo
//! step cover behavior outside the game placement tests at negligible cost.
use crate::editor::{Document, document::ObjectKind, play_here::Intent};
use crate::game::play_spawn::Pose;
use glam::Vec3;
use std::{fs, path::PathBuf};
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleaned = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn launch_intent_keeps_saved_spawn_redo_and_selected_level_identity() -> Result<(), String> {
    let root = std::env::temp_dir().join(format!("places-play-here-{}", std::process::id()));
    let fixture = Fixture(root);
    let mut doc = Document::create(&fixture.0, "play_here", "Play here")?;
    let first = doc.level_id().to_string();
    doc.add_level("second")?;
    doc.select_level(&first)?;
    let source = doc.root().join("levels").join(format!("{first}.json"));
    let saved = fs::read(&source).map_err(|error| error.to_string())?;
    let _wall = doc.add(ObjectKind::Wall, [2.0, 0.0, 2.0], None)?;
    assert!(doc.undo(), "fixture provides an independent redo step");
    let before = (
        doc.raw().clone(),
        doc.is_dirty(),
        doc.can_undo(),
        doc.can_redo(),
    );
    let pose = Pose::new(Vec3::new(1.0, 1.6, 1.0), 0.5, -0.2)?;
    let intent = Intent::capture(&doc, Some(pose));
    assert!(intent.matches(&doc), "request belongs to this document");
    assert_eq!(
        before,
        (
            doc.raw().clone(),
            doc.is_dirty(),
            doc.can_undo(),
            doc.can_redo()
        ),
        "capturing a test pose must preserve raw spawn, dirty state and undo/redo"
    );
    assert_eq!(
        saved,
        fs::read(&source).map_err(|error| error.to_string())?,
        "capturing a test pose never writes the saved source"
    );
    doc.select_level("second")?;
    assert!(
        !intent.matches(&doc),
        "an asynchronous export cannot launch the newly selected level with an old pose"
    );
    Ok(())
}
