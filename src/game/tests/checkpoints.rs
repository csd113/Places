//! Essential boundary regression: one tiny semantic world protects persisted
//! typed progression/door state plus real atomic replacement, recovery and bounds.
//! Unique value: existing campaign tests cannot restore gameplay or reject a
//! damaged payload. A handful of small temporary writes; no assets, GPU or bake.
use super::{
    Identity, PackageGuard, Snapshot, Target, WriteOutcome, publish_checkpoint, read_at, write_at,
};
use crate::{
    engine::{
        entities::components::StateValue,
        level::{ActionDef, LevelDef},
        loader::{LevelEntry, LevelSourceType, LoadedLevel},
    },
    game::{CollisionWorld, Game},
};
use std::path::Path;

// Two byte-identical synthetic archives prove that generation and content
// checks alone cannot admit a different installation at the upload boundary.
// Only discovery records are read. No GPU, assets, or prepared map records.
#[test]
fn checkpoint_upload_rejects_same_generation_installation_retarget() -> Result<(), String> {
    let (folder, mut entry) = package_fixture("retarget")?;
    let replacement = folder.join("replacement.placesmap");
    let _copied = std::fs::copy(&entry.path, &replacement).map_err(|error| error.to_string())?;
    let selected = Target::for_entry(&entry)?.ok_or("Fixture does not support checkpoints")?;
    let package = selected.hold_package()?;
    #[cfg(windows)]
    assert!(
        std::fs::OpenOptions::new()
            .write(true)
            .open(&entry.path)
            .is_err()
    );
    #[cfg(windows)]
    assert!(
        std::fs::rename(&replacement, &entry.path).is_err(),
        "the verified open handle must deny late replacement"
    );
    let (level, snapshot) = snapshot_fixture()?;
    let mut loaded = loaded_fixture(level, &entry, &selected);
    selected.check_loaded(
        &loaded,
        &selected.installation.canonical,
        &selected.revision,
    )?;
    entry.path = replacement;
    let retarget =
        Target::for_entry(&entry)?.ok_or("Replacement fixture does not support checkpoints")?;
    assert_eq!(selected.identity.content, retarget.identity.content);
    assert_eq!(snapshot.campaign_generation, 0);
    loaded.entry = entry;
    retarget.check_loaded(
        &loaded,
        &retarget.installation.canonical,
        &retarget.revision,
    )?;
    assert!(
        selected
            .check_loaded(
                &loaded,
                &retarget.installation.canonical,
                &retarget.revision
            )
            .is_err(),
        "same-generation world preparation cannot change the receipt owner"
    );
    let late_resolution = crate::game::campaign::Installation::new(
        loaded.entry.path,
        selected.installation.canonical.clone(),
        None,
        "checkpoint",
    );
    assert!(
        late_resolution.verify_path().is_err(),
        "a late alias resolution must match the owner selected earlier"
    );
    #[cfg(unix)]
    {
        let original = selected.installation.original();
        // Simulate an external replacement that does not acquire our lock.
        std::fs::rename(retarget.installation.original(), original)
            .map_err(|error| error.to_string())?;
        assert!(
            selected.verify_current().is_err(),
            "a byte-identical replacement after CPU preparation is a different file"
        );
    }
    drop(package);
    #[cfg(windows)]
    std::fs::remove_file(retarget.installation.original()).map_err(|error| error.to_string())?;
    std::fs::remove_file(selected.installation.original())
        .and_then(|()| std::fs::remove_dir(&folder))
        .map_err(|error| error.to_string())
}

fn package_fixture(label: &str) -> Result<(std::path::PathBuf, LevelEntry), String> {
    let folder =
        std::env::temp_dir().join(format!("places-checkpoint-{label}-{}", std::process::id()));
    std::fs::create_dir(&folder).map_err(|error| error.to_string())?;
    let entry = LevelEntry {
        id: "checkpoint".into(),
        name: "Checkpoint".into(),
        author: String::new(),
        source_type: LevelSourceType::Installed,
        path: folder.join("selected.placesmap"),
    };
    checkpoint_archive(&entry.path)?;
    Ok((folder, entry))
}

fn loaded_fixture(level: LevelDef, entry: &LevelEntry, selected: &Target) -> LoadedLevel {
    LoadedLevel {
        project_id: None,
        package_identity: Some(selected.identity.content.clone()),
        catalog: std::sync::Arc::default(),
        level,
        materials: crate::engine::materials::MaterialTable::default(),
        light_sheets: Vec::new(),
        entry: entry.clone(),
    }
}

fn replace_with_lookalike(path: &Path) -> Result<(), String> {
    let original = path.with_extension("original");
    let replacement = path.with_extension("replacement");
    let before = std::fs::metadata(path).map_err(|error| error.to_string())?;
    let modified = before.modified().map_err(|error| error.to_string())?;
    let _copied = std::fs::copy(path, &replacement).map_err(|error| error.to_string())?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(&replacement)
        .and_then(|file| file.set_modified(modified))
        .map_err(|error| error.to_string())?;
    // Keep the original allocated so the filesystem cannot reuse its ID.
    std::fs::rename(path, original)
        .and_then(|()| std::fs::rename(replacement, path))
        .map_err(|error| error.to_string())?;
    let after = std::fs::metadata(path).map_err(|error| error.to_string())?;
    assert_eq!(after.len(), before.len());
    assert_eq!(
        after.modified().map_err(|error| error.to_string())?,
        modified
    );
    Ok(())
}

// Swap two byte-identical tiny archives before opening the guard. The old
// length/time/manifest tuple would accept the new file on Windows.
#[test]
fn checkpoint_guard_rejects_same_path_lookalike_before_open() -> Result<(), String> {
    let (folder, entry) = package_fixture("before-hold")?;
    let selected = Target::for_entry(&entry)?.ok_or("Fixture does not support checkpoints")?;
    replace_with_lookalike(&entry.path)?;
    let replacement = Target::for_entry(&entry)?.ok_or("Replacement is not a valid package")?;
    assert_eq!(selected.identity, replacement.identity);
    assert!(
        selected.revision.hold(&entry.path, &entry.id).is_err(),
        "the handle used by the guard must identify the originally selected file",
    );
    std::fs::remove_dir_all(folder).map_err(|error| error.to_string())
}

// The live world already holds its package revision. A later same-manifest
// installation must never become the source of a new checkpoint.
#[test]
fn checkpoint_capture_requires_the_revision_used_by_the_live_world() -> Result<(), String> {
    let (folder, entry) = package_fixture("capture-revision")?;
    let selected = Target::for_entry(&entry)?.ok_or("Fixture does not support checkpoints")?;
    let (level, _) = snapshot_fixture()?;
    let loaded = loaded_fixture(level, &entry, &selected);
    let live_revision = selected.revision.clone();
    let original = Target::for_level(&loaded, &selected.installation, &live_revision)?;
    drop(original.hold_package()?);
    replace_with_lookalike(&entry.path)?;
    let replacement = Target::for_entry(&entry)?.ok_or("Replacement is not a valid package")?;
    assert_eq!(selected.identity, replacement.identity);
    assert!(Target::for_level(&loaded, &selected.installation, &live_revision).is_err());
    std::fs::remove_dir_all(folder).map_err(|error| error.to_string())
}

// Exercise the commit wrapper with the real cooperating publisher. The last
// operation inside the commit must see the old archive protected, and a
// publication immediately after commit must succeed.
#[test]
fn checkpoint_guard_blocks_package_publication_through_world_commit() -> Result<(), String> {
    let (folder, entry) = package_fixture("commit-lifetime")?;
    let selected = Target::for_entry(&entry)?.ok_or("Fixture does not support checkpoints")?;
    let guard = selected.hold_package()?;
    PackageGuard::through_commit(Some(guard), || {
        assert!(
            checkpoint_archive(&entry.path).is_err(),
            "the archive must stay protected through the end of the commit callback"
        );
        selected.verify_current()
    })?;
    checkpoint_archive(&entry.path)?;
    std::fs::remove_dir_all(folder).map_err(|error| error.to_string())
}

fn checkpoint_archive(path: &Path) -> Result<(), String> {
    use crate::engine::package::{
        self,
        archive::{PendingEntry, write_archive},
        hash::sha256_hex,
    };

    let semantics = br#"{"schema":"places-level","format_version":1,"id":"checkpoint","name":"Checkpoint","saving":true,"spawn":{"x":0,"z":0}}"#.to_vec();
    let mut entries = vec![PendingEntry {
        name: "semantics.json".into(),
        bytes: semantics,
    }];
    for role in ["mesh", "props", "lighting", "collision", "navigation"] {
        entries.push(PendingEntry {
            name: format!("{role}.bin"),
            bytes: Vec::new(),
        });
    }
    let manifest = serde_json::json!({
        "schema": package::PACKAGE_SCHEMA, "package_format": package::FORMAT_VERSION,
        "id": "checkpoint", "package_id": "checkpoint", "package_version": "0.1.0",
        "places_version": package::release::PLACES_VERSION, "name": "Checkpoint",
        "required_capabilities": ["gameplay-checkpoints-v1"],
        "entries": entries.iter().map(|entry| serde_json::json!({
            "name": entry.name, "bytes": entry.bytes.len(), "sha256": sha256_hex(&entry.bytes),
            "role": entry.name.strip_suffix(".bin").unwrap_or("semantics")
        })).collect::<Vec<_>>(),
        "variants": [{"lightmap_quality": "off", "quality_profile": "low", "entries": {
            "mesh": "mesh.bin", "props": "props.bin", "lighting": "lighting.bin",
            "collision": "collision.bin", "navigation": "navigation.bin"
        }}]
    });
    entries.push(PendingEntry {
        name: "manifest.json".into(),
        bytes: serde_json::to_vec(&manifest).map_err(|error| error.to_string())?,
    });
    write_archive(path, entries)
}

#[test]
fn gameplay_checkpoint_roundtrip_recovers_previous_and_preserves_failed_publication()
-> Result<(), String> {
    let (level, snapshot) = snapshot_fixture()?;
    let folder =
        std::env::temp_dir().join(format!("places-gameplay-checkpoint-{}", std::process::id()));
    std::fs::create_dir(&folder).map_err(|problem| problem.to_string())?;
    let path = folder.join("checkpoint.json");
    let identity = Identity {
        project: "fixture".into(),
        level: "checkpoint".into(),
        content: "a".repeat(64),
    };
    assert_eq!(
        write_at(&path, &identity, "First", &snapshot)?,
        WriteOutcome::Written
    );
    let original = std::fs::read(&path).map_err(|problem| problem.to_string())?;
    std::fs::create_dir(path.with_extension("previous")).map_err(|problem| problem.to_string())?;
    assert!(
        write_at(&path, &identity, "Refused", &snapshot).is_err(),
        "backup/publication failure must report failure"
    );
    assert_eq!(
        std::fs::read(&path).map_err(|problem| problem.to_string())?,
        original,
        "failed writes preserve the last valid primary"
    );
    std::fs::remove_dir(path.with_extension("previous")).map_err(|problem| problem.to_string())?;
    assert_eq!(
        write_at(&path, &identity, "Second", &snapshot)?,
        WriteOutcome::Written
    );
    std::fs::write(&path, b"{truncated").map_err(|problem| problem.to_string())?;
    let recovered = read_at(&path, &identity)?.ok_or("Missing recovery")?;
    assert!(
        recovered.recovered,
        "a corrupt primary recovers the verified previous save"
    );
    assert_eq!(
        recovered.name, "First",
        "recovery never promotes an incomplete temp file"
    );
    let mut restored = Game::new(
        crate::game::spawn_position(&level),
        0.0,
        CollisionWorld::from_level(&level),
    );
    restored
        .entities_mut()
        .restore_checkpoint(&recovered.snapshot.world)?;
    restored.restore_checkpoint_player(&recovered.snapshot.player)?;
    assert_eq!(
        restored.player_health().current_hp(),
        3,
        "checkpoint restore preserves injury rather than silently healing"
    );
    let handle = restored
        .entities()
        .handle_of("quest")
        .ok_or("Missing quest")?;
    assert_eq!(
        restored
            .entities()
            .components()
            .states
            .get(handle)
            .and_then(|state| state.get("count"))
            .and_then(StateValue::as_int),
        Some(3),
        "real typed progression survives reconstruction"
    );
    assert_eq!(
        restored.doors().is_locked("door"),
        Some(true),
        "door lock survives reconstruction"
    );
    assert_eq!(recovered.snapshot.campaign_generation, 0);
    let mut changed = identity.clone();
    changed.content = "b".repeat(64);
    assert!(
        read_at(&path, &changed).is_err(),
        "changed content cannot inherit checkpoint state"
    );
    std::fs::write(path.with_extension("previous"), b"bad")
        .map_err(|problem| problem.to_string())?;
    assert!(
        read_at(&path, &identity).is_err(),
        "two corrupt copies must offer a fresh start"
    );
    std::fs::write(&path, vec![b' '; 4_194_305]).map_err(|problem| problem.to_string())?;
    assert!(
        read_at(&path, &identity).is_err(),
        "input admission bounds memory before decoding"
    );
    std::fs::remove_file(&path)
        .and_then(|()| std::fs::remove_file(path.with_extension("previous")))
        .and_then(|()| std::fs::remove_dir(&folder))
        .map_err(|problem| problem.to_string())?;
    Ok(())
}

// Inject an error only after the atomic swap. A new reader must see the
// committed checkpoint, and the previous save must remain recoverable.
#[test]
fn checkpoint_post_sync_error_reports_visible_write() -> Result<(), String> {
    let directory = std::env::temp_dir().join(format!(
        "places-checkpoint-post-sync-{}",
        std::process::id()
    ));
    std::fs::create_dir(&directory).map_err(|error| error.to_string())?;
    let path = directory.join("checkpoint.json");
    let identity = Identity {
        project: "fixture".into(),
        level: "checkpoint".into(),
        content: "synthetic".into(),
    };
    let (_, snapshot) = snapshot_fixture()?;
    assert_eq!(
        publish_checkpoint(&path, &identity, "First", &snapshot, || Ok(()), || Ok(()))?,
        WriteOutcome::Written
    );
    let preflight_done = std::cell::Cell::new(false);
    let written = publish_checkpoint(
        &path,
        &identity,
        "Second",
        &snapshot,
        || Ok(()),
        || {
            if preflight_done.replace(true) {
                Err("synthetic sync failure".into())
            } else {
                Ok(())
            }
        },
    )?;
    assert_eq!(
        written,
        WriteOutcome::WrittenUnconfirmed("synthetic sync failure".into())
    );
    assert_eq!(
        read_at(&path, &identity)?
            .ok_or("missing primary checkpoint")?
            .name,
        "Second"
    );
    assert_eq!(
        read_at(&path.with_extension("previous"), &identity)?
            .ok_or("missing previous checkpoint")?
            .name,
        "First"
    );
    std::fs::remove_dir_all(&directory).map_err(|error| error.to_string())
}

fn snapshot_fixture() -> Result<(LevelDef, Snapshot), String> {
    let level=LevelDef::from_json(r#"{"schema":"places-level","format_version":1,"id":"checkpoint","name":"Checkpoint","saving":true,"health":{"enabled":true},"spawn":{"x":2,"z":2},"rooms":[{"width":6,"depth":6}],"ceiling_lights":[{"id":"lamp","fixture":"core:fluorescent_panel_01","x":2,"z":2,"switchable":true}],"props":[{"id":"quest","model":"core:chair","x":1,"z":1,"components":[{"component":"state","name":"count","value":0}]}],"doors":[{"id":"door","x":0,"z":0,"width":1,"height":2}]}"#).map_err(|problem|problem.to_string())?;
    let mut game = Game::new(
        crate::game::spawn_position(&level),
        0.0,
        CollisionWorld::from_level(&level),
    );
    game.apply_health_action(
        &crate::engine::health::HealthTarget::Player,
        crate::engine::health::HealthAction::Damage { hp: 3 },
    )?;
    let _initial = game.entities_mut().take_commands();
    let _change = game.entities_mut().dispatch_on(
        "quest",
        &[
            ActionDef::SetState {
                target: None,
                name: "count".into(),
                value: StateValue::Int(3),
            },
            ActionDef::SetLight {
                target: Some("lamp".into()),
                on: false,
            },
            ActionDef::Lock {
                target: Some("door".into()),
            },
        ],
    );
    // Flush bounded state-change events through an ordinary tick before capture.
    let floor = crate::engine::level::WalkableFloor::from_level(&level);
    let index = crate::engine::collision_index::CollisionIndex::build(&[]);
    let _tick = game
        .entities_mut()
        .tick(&crate::engine::entities::WorldContext {
            player_walk_speed: 3.75,
            delta_seconds: 0.016,
            feet_from: glam::Vec3::new(2.0, 0.0, 2.0),
            feet: glam::Vec3::new(2.0, 0.0, 2.0),
            eye: glam::Vec3::new(2.0, 1.6, 2.0),
            body_height: 1.8,
            body_half: crate::engine::collision::PLAYER_RADIUS,
            form_model: None,
            walls: &[],
            index: &index,
            floor: &floor,
            nav: None,
        });
    let _commands = game.entities_mut().take_commands();
    let snapshot = Snapshot {
        elevator_cargo: crate::game::app::elevator_cargo::Ledger::default(),
        campaign_generation: 0,
        player: game.checkpoint_player()?,
        world: game.entities().checkpoint()?,
        items: Vec::new(),
        flashlight: true,
    };
    Ok((level, snapshot))
}
