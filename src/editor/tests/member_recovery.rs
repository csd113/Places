//! Real document reopen gates. These use only tiny editor starters and one
//! mode copy, with no bake, assets or renderer. The snapshots distinguish
//! resumable commits from ambiguous state and ordinary in-session failure.
use std::fs;
use std::path::{Path, PathBuf};

use super::{PENDING_NAME, stage};
use crate::editor::Document;
use crate::engine::game_mode::GameMode;
use crate::engine::project::{MANIFEST_NAME, ProjectFolder, ProjectLevel};

struct Fixture(PathBuf);

impl Fixture {
    fn new(name: &str) -> Result<Self, String> {
        let path = std::env::temp_dir().join(format!(
            "places-member-recovery-{}-{name}",
            std::process::id()
        ));
        fs::create_dir(&path).map_err(|error| error.to_string())?;
        Ok(Self(path))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|error| error.to_string())
}

fn interrupt(target: &Document, origin: &Document) -> Result<ProjectLevel, String> {
    let mut member = origin.manifest().level(None)?.clone();
    member.display_name.clone_from(&origin.level().name);
    let mut next = target.manifest().clone();
    next.levels.push(member.clone());
    stage(
        target.root(),
        &read(&target.root().join(MANIFEST_NAME))?,
        &serde_json::to_vec_pretty(&next).map_err(|error| error.to_string())?,
        &member,
        &read(&origin.root().join(member.source.as_str()))?,
    )?;
    Ok(member)
}

#[test]
fn pending_import_recovers_without_original_and_compiler_sees_exact_completed_inputs()
-> Result<(), String> {
    let fixture = Fixture::new("import")?;
    let target_path = fixture.0.join("target");
    let target = Document::create(&target_path, "entry", "Entry")?;
    let mut origin = Document::create(&fixture.0.join("origin"), "imported", "Imported title")?;
    let mut mode = GameMode::from_level(origin.level());
    mode.mode.walk_speed = 6.0;
    origin.apply_game_mode(&mode)?;
    origin.save_all()?;
    let member = interrupt(&target, &origin)?;
    let expected_source = read(&origin.root().join(member.source.as_str()))?;
    let expected_mode = read(&origin.root().join(mode.project_path()?.as_str()))?;
    fs::remove_dir_all(origin.root()).map_err(|error| error.to_string())?;
    let before = ProjectFolder::open(&target_path)?;
    let prior = crate::engine::package::project::snapshot(&before, &|| Ok(()))?;
    assert_eq!(before.manifest().levels.len(), 1);
    assert!(!prior.contains_key(&member.source));

    let mut reopened = Document::open(&target_path.join(MANIFEST_NAME))?;
    assert_eq!(
        reopened.recovery_notice(),
        Some("Recovered a pending Add/Import")
    );
    assert_eq!(reopened.level_id(), "entry");
    assert_eq!(reopened.manifest().level(Some("imported"))?, &member);
    reopened.select_level("imported")?;
    assert_eq!(GameMode::from_level(reopened.level()), mode);
    let after = ProjectFolder::open(&target_path)?;
    let compiled = crate::engine::package::project::snapshot(&after, &|| Ok(()))?;
    let old_source = &before.manifest().level(None)?.source;
    assert_eq!(compiled.get(old_source), prior.get(old_source));
    assert_eq!(compiled.get(&member.source), Some(&expected_source));
    assert_eq!(compiled.get(&mode.project_path()?), Some(&expected_mode));
    assert!(!target_path.join(PENDING_NAME).exists());
    assert_eq!(Document::open(&target_path)?.manifest().levels.len(), 2);
    Ok(())
}

#[test]
fn pending_starter_resumes_after_source_publication_and_retires_after_commit() -> Result<(), String>
{
    let fixture = Fixture::new("starter")?;
    let target = Document::create(&fixture.0.join("target"), "entry", "Entry")?;
    let origin = Document::create(&fixture.0.join("origin"), "starter", "starter")?;
    let member = origin.manifest().level(None)?.clone();
    let source = read(&origin.root().join(member.source.as_str()))?;
    let old_manifest = read(&target.root().join(MANIFEST_NAME))?;
    let mut next = target.manifest().clone();
    next.levels.push(member.clone());
    let new_manifest = serde_json::to_vec_pretty(&next).map_err(|error| error.to_string())?;
    stage(
        target.root(),
        &old_manifest,
        &new_manifest,
        &member,
        &source,
    )?;
    let destination = target.root().join(member.source.as_str());
    let _copied = fs::copy(origin.root().join(member.source.as_str()), &destination)
        .map_err(|error| error.to_string())?;
    let writer = super::lock(target.root())?;
    let busy = Document::open(target.root())?;
    assert!(
        busy.recovery_notice()
            .is_some_and(|notice| notice.contains("Another editor"))
    );
    assert_eq!(read(&target.root().join(MANIFEST_NAME))?, old_manifest);
    drop(writer);

    let reopened = Document::open(target.root())?;
    assert_eq!(reopened.manifest().level(Some("starter"))?, &member);
    assert_eq!(read(&destination)?, source);

    // Recreate the crash window after replacing the manifest and before
    // retiring the record. Opening must remain idempotent.
    stage(
        target.root(),
        &old_manifest,
        &new_manifest,
        &member,
        &source,
    )?;
    let committed = Document::open(target.root())?;
    assert_eq!(committed.recovery_notice(), None);
    assert_eq!(read(&target.root().join(MANIFEST_NAME))?, new_manifest);
    assert_eq!(committed.manifest().levels.len(), 2);
    assert!(!target.root().join(PENDING_NAME).exists());
    Ok(())
}

#[test]
fn recovery_preserves_manifest_source_and_record_when_conflicts_are_unverified()
-> Result<(), String> {
    let fixture = Fixture::new("conflict")?;
    let mut target = Document::create(&fixture.0.join("target"), "entry", "Entry")?;
    let origin = Document::create(&fixture.0.join("origin"), "pending", "Pending")?;
    let member = interrupt(&target, &origin)?;
    let pending_path = target.root().join(PENDING_NAME);
    let record = read(&pending_path)?;
    let manifest_path = target.root().join(MANIFEST_NAME);
    let before = read(&manifest_path)?;
    let destination = target.root().join(member.source.as_str());
    let _copied = fs::copy(target.root().join("levels/entry.json"), &destination)
        .map_err(|error| error.to_string())?;
    let existing = read(&destination)?;
    let blocked = Document::open(target.root())?;
    assert!(
        blocked
            .recovery_notice()
            .is_some_and(|notice| notice.contains("Existing level source differs"))
    );
    assert_eq!(read(&destination)?, existing);
    assert_eq!(read(&manifest_path)?, before);
    assert_eq!(read(&pending_path)?, record);

    fs::remove_file(&destination).map_err(|error| error.to_string())?;
    target.update_manifest("name", "External edit")?;
    let edited = read(&manifest_path)?;
    let stale = Document::open(target.root())?;
    assert!(
        stale
            .recovery_notice()
            .is_some_and(|notice| notice.contains("manifest changed"))
    );
    assert_eq!(read(&manifest_path)?, edited);
    assert_eq!(read(&pending_path)?, record);
    assert!(!destination.exists());

    // Test a complete foreign record versus a truncated record independently.
    let truncated = record
        .get(..record.len().saturating_sub(1))
        .ok_or("Missing record")?;
    fs::write(&pending_path, truncated).map_err(|error| error.to_string())?;
    let damaged = Document::open(target.root())?;
    assert!(
        damaged
            .recovery_notice()
            .is_some_and(|notice| notice.contains("source digest"))
    );
    assert_eq!(read(&manifest_path)?, edited);
    assert_eq!(read(&pending_path)?, truncated);
    assert!(!destination.exists());
    Ok(())
}

#[test]
fn ordinary_import_failure_cancels_restart_intent() -> Result<(), String> {
    let fixture = Fixture::new("live-failure")?;
    let mut target = Document::create(&fixture.0.join("target"), "entry", "Entry")?;
    let mut origin = Document::create(&fixture.0.join("origin"), "imported", "Imported")?;
    let mut mode = GameMode::from_level(origin.level());
    mode.mode.walk_speed = 6.0;
    origin.apply_game_mode(&mode)?;
    origin.save_all()?;
    let member = origin.manifest().level(None)?;
    let conflicting = target.root().join(mode.project_path()?.as_str());
    fs::create_dir(conflicting.parent().ok_or("Missing mode parent")?)
        .map_err(|error| error.to_string())?;
    fs::create_dir(&conflicting).map_err(|error| error.to_string())?;
    let manifest = read(&target.root().join(MANIFEST_NAME))?;
    assert!(
        target
            .import_level(&origin.root().join(member.source.as_str()))
            .is_err()
    );
    assert!(conflicting.is_dir());
    fs::remove_dir(&conflicting).map_err(|error| error.to_string())?;
    assert!(!target.root().join(PENDING_NAME).exists());
    assert!(
        Document::open(target.root())?
            .manifest()
            .level(Some("imported"))
            .is_err()
    );
    assert_eq!(read(&target.root().join(MANIFEST_NAME))?, manifest);
    assert!(!target.root().join(member.source.as_str()).exists());
    Ok(())
}
