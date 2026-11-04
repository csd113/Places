//! Data-loss regressions use only tiny editable files and ZIP entries. They
//! exercise the final guard after encoding without assets, a bake or a GPU.
use super::super::publication::Guard;
use crate::engine::package::archive::{self, PendingEntry};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup = fs::remove_dir_all(&self.0);
    }
}
fn fixture() -> Result<Fixture, String> {
    let root = Fixture(std::env::temp_dir().join(format!(
        "places-publication-{}-{}",
        std::process::id(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )));
    fs::create_dir(&root.0).map_err(|e| e.to_string())?;
    Ok(root)
}
fn entries() -> Vec<PendingEntry> {
    vec![PendingEntry {
        name: "manifest.json".into(),
        bytes: b"candidate".to_vec(),
    }]
}

#[test]
fn build_guard_rejects_aliases_and_preserves_concurrent_work_and_cancellation() -> Result<(), String>
{
    let root = fixture()?;
    let source = root.0.join("editable.placesmap");
    let out = root.0.join("out.placesmap");
    fs::write(&source, b"original source").map_err(|e| e.to_string())?;
    assert!(Guard::new(&source, &source).is_err());
    assert!(Guard::new(&source, &root.0.join("./editable.placesmap")).is_err());
    fs::hard_link(&source, &out).map_err(|e| e.to_string())?;
    assert!(Guard::new(&source, &out).is_err());
    fs::remove_file(&out).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&source, &out).map_err(|e| e.to_string())?;
        assert!(Guard::new(&source, &out).is_err());
        fs::remove_file(&out).map_err(|e| e.to_string())?;
    }
    fs::write(&out, b"prior output").map_err(|e| e.to_string())?;
    let guard = Guard::new(&source, &out)?;
    let source_change = archive::write_archive_guarded(
        &out,
        entries(),
        || Ok(()),
        || {
            fs::write(&source, b"new source").map_err(|e| e.to_string())?;
            guard.check()
        },
    );
    assert!(source_change.is_err());
    assert_eq!(fs::read(&source).map_err(|e| e.to_string())?, b"new source");
    assert_eq!(fs::read(&out).map_err(|e| e.to_string())?, b"prior output");
    let destination_guard = Guard::new(&source, &out)?;
    let output_change = archive::write_archive_guarded(
        &out,
        entries(),
        || Ok(()),
        || {
            fs::write(&out, b"external work").map_err(|e| e.to_string())?;
            destination_guard.check()
        },
    );
    assert!(output_change.is_err());
    let cancellation = archive::write_archive_guarded(
        &out,
        entries(),
        || Ok(()),
        || Err("cancelled after encoding".into()),
    );
    assert!(cancellation.is_err());
    assert_eq!(fs::read(&out).map_err(|e| e.to_string())?, b"external work");
    // A previously absent destination may also appear while encoding.
    fs::remove_file(&out).map_err(|e| e.to_string())?;
    let absent_guard = Guard::new(&source, &out)?;
    fs::write(&out, b"new external file").map_err(|e| e.to_string())?;
    assert!(absent_guard.check().is_err());
    Ok(())
}

#[test]
fn project_inputs_are_protected_while_display_edits_keep_member_identity() -> Result<(), String> {
    let root = fixture()?;
    fs::create_dir(root.0.join("levels")).map_err(|e| e.to_string())?;
    let level = root.0.join("levels/test_room.json");
    fs::write(
        &level,
        include_bytes!("../../../tests/fixtures/levels/test_room.json"),
    )
    .map_err(|e| e.to_string())?;
    let manifest = root.0.join("places.project.json");
    let mut metadata = crate::engine::project::ProjectManifest::from_json(br#"{"schema":"places-project", "project_format":1, "id":"project", "name":"Original", "entry_level":"test_room", "levels":[{"id":"test_room", "source":"levels/test_room.json"}]}"#)?;
    fs::write(
        &manifest,
        serde_json::to_vec(&metadata).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    assert!(Guard::new(&root.0, &manifest).is_err());
    assert!(Guard::new(&manifest, &level).is_err());
    let out = root.0.join("out.placesmap");
    fs::hard_link(&level, &out).map_err(|e| e.to_string())?;
    assert!(Guard::new(&root.0, &out).is_err());
    fs::remove_file(&out).map_err(|e| e.to_string())?;
    let guard = Guard::new(&root.0, &out)?;
    let original = super::super::project::read_source(&root.0, None)?.identity;
    metadata.name = "Renamed".into();
    metadata.author = "New author".into();
    metadata.description = "New description".into();
    metadata
        .levels
        .first_mut()
        .ok_or("Missing level")?
        .display_name = "Menu title".into();
    fs::write(
        &manifest,
        serde_json::to_vec(&metadata).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    assert!(
        guard.check().is_err(),
        "display edits still protect the full source snapshot"
    );
    assert_eq!(
        original,
        super::super::project::read_source(&root.0, None)?.identity
    );
    metadata.package_version =
        crate::engine::package::release::ReleaseVersion::try_from("1.0.1".to_string())?;
    fs::write(
        &manifest,
        serde_json::to_vec(&metadata).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    assert_ne!(
        original,
        super::super::project::read_source(&root.0, None)?.identity
    );
    Ok(())
}
