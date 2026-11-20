//! One tiny save guards the actual disk round trip, bounded admission and
//! idempotence across app restarts. No environment mutation, asset load or bake.
use super::{
    Binding, CampaignProgress, CampaignSave, Installation, Location, SharedCampaign, legacy_path,
    migration_path, read, read_installed, save_path, write,
};
use std::path::Path;

fn location(folder: &Path, canonical: &Path, original: &Path, scope: &str) -> Location {
    Installation::new(
        original.to_path_buf(),
        canonical.to_path_buf(),
        scope.strip_prefix("project:"),
        "fixture",
    )
    .location(folder)
}

fn prepare(owner: &CampaignSave, location: &Location) -> Result<Option<Binding>, String> {
    owner.prepare_binding(location.installation.clone(), location.try_lock(), None)
}

fn attach(owner: &mut CampaignSave, location: &Location) -> Result<SharedCampaign, String> {
    let binding = prepare(owner, location)?.ok_or("Unexpected fixture lock contention")?;
    Ok(owner.attach(binding))
}

// Use one tiny synthetic receipt to exercise the same-project travel and
// corrupt-receipt CLEAR path, including replacement by valid same-epoch data.
#[test]
fn ordinary_home_to_hub_travel_preserves_favors_until_a_fresh_clear() -> Result<(), String> {
    let folder = std::env::temp_dir().join(format!("places-campaign-hub-{}", std::process::id()));
    let package = folder.join("fixture.placesmap");
    let home = Installation::new(package.clone(), package.clone(), Some("fixture"), "home")
        .location(&folder);
    let hub = Installation::new(package.clone(), package, Some("fixture"), "hub").location(&folder);
    assert_eq!(home.path, hub.path);
    write(&home.path, &CampaignProgress::empty_at(4))?;
    let mut owner = CampaignSave::default();
    let home_progress = attach(&mut owner, &home)?;
    let original_epoch = owner.lock_checkpoint()?.1;
    {
        let mut progress = home_progress.lock().map_err(|error| error.to_string())?;
        progress.complete("home")?;
        progress.request_reset();
    }
    let damaged = b"corrupt Home receipt";
    std::fs::write(&home.path, damaged).map_err(|error| error.to_string())?;
    let hub_progress = attach(&mut owner, &hub)?;
    assert!(std::sync::Arc::ptr_eq(&home_progress, &hub_progress));
    {
        let progress = hub_progress.lock().map_err(|error| error.to_string())?;
        assert!(progress.completed("home"));
        assert_eq!(progress.status(), "failed");
        assert!(
            !progress.reset_pending(),
            "a prior visit's CLEAR must be discarded"
        );
        drop(progress);
    }
    assert!(owner.lock_checkpoint().is_err());
    assert!(
        owner
            .prepare_binding(
                hub.installation.clone(),
                hub.try_lock(),
                Some(original_epoch),
            )
            .is_err(),
        "checkpoint resume still requires a readable receipt"
    );
    let _notice = owner.flush();
    assert_eq!(
        std::fs::read(&hub.path).map_err(|error| error.to_string())?,
        damaged,
        "an ordinary flush must preserve malformed bytes"
    );

    let mut repaired = CampaignProgress::empty_at(original_epoch);
    repaired.complete("pool")?;
    write(&hub.path, &repaired)?;
    hub_progress
        .lock()
        .map_err(|error| error.to_string())?
        .request_reset();
    assert!(owner.flush().is_some());
    let preserved = read(&hub.path)?;
    assert_eq!(preserved.generation(), original_epoch);
    assert!(
        preserved.completed("pool"),
        "corruption recovery cannot clear a valid replacement"
    );
    assert!(
        hub_progress
            .lock()
            .map_err(|error| error.to_string())?
            .completed("home")
    );

    std::fs::write(&hub.path, b"new corrupt receipt").map_err(|error| error.to_string())?;
    let fresh_confirmation = attach(&mut owner, &hub)?;
    assert!(std::sync::Arc::ptr_eq(&hub_progress, &fresh_confirmation));
    fresh_confirmation
        .lock()
        .map_err(|error| error.to_string())?
        .request_reset();
    assert!(owner.flush().is_none());
    let cleared = read(&hub.path)?;
    assert_eq!(cleared.count(), 0);
    assert!(cleared.generation() > original_epoch);
    std::fs::remove_file(&hub.path)
        .and_then(|()| std::fs::remove_file(hub.path.with_extension("lock")))
        .and_then(|()| std::fs::remove_dir(&folder))
        .map_err(|error| error.to_string())
}

// This uses the generation from a separate owner's post-CLEAR visit as the
// selected checkpoint's generation. One synthetic receipt and a held sidecar
// exercise final admission for active, retained, and previously unseen owners.
#[test]
fn checkpoint_visit_uses_the_locked_epoch_and_waits_after_selection() -> Result<(), String> {
    let folder =
        std::env::temp_dir().join(format!("places-campaign-resume-{}", std::process::id()));
    let package = folder.join("fixture.placesmap");
    let receipt = location(&folder, &package, &package, "project:fixture");
    let other = location(&folder, &package, &package, "project:other");
    let checkpoint_visit = |owner: &CampaignSave, epoch| {
        owner.prepare_binding(
            receipt.installation.clone(),
            receipt.try_lock(),
            Some(epoch),
        )
    };
    let mut active = CampaignSave::default();
    let mut departed = CampaignSave::default();
    let mut clearing = CampaignSave::default();
    let old_active = attach(&mut active, &receipt)?;
    let old_departed = attach(&mut departed, &receipt)?;
    let resetter = attach(&mut clearing, &receipt)?;
    let old_epoch = active.lock_checkpoint()?.1;
    old_active
        .lock()
        .map_err(|error| error.to_string())?
        .request_reset();
    assert!(
        checkpoint_visit(&active, old_epoch).is_err(),
        "a local CLEAR pending in the same epoch blocks checkpoint admission"
    );
    old_departed
        .lock()
        .map_err(|error| error.to_string())?
        .complete("winter")?;
    let _different_visit = attach(&mut departed, &other)?;
    assert_eq!(departed.retained.len(), 1);

    resetter
        .lock()
        .map_err(|error| error.to_string())?
        .request_reset();
    assert!(clearing.flush().is_none());
    let _cleared_visit = attach(&mut clearing, &receipt)?;
    let selected_epoch = clearing.lock_checkpoint()?.1;
    assert!(selected_epoch > old_epoch);
    for owner in [&mut active, &mut departed] {
        assert!(checkpoint_visit(owner, old_epoch).is_err());
        let binding = checkpoint_visit(owner, selected_epoch)?
            .ok_or("A checkpoint from the installed epoch must be admissible")?;
        let _attached = owner.attach(binding);
        assert_eq!(owner.lock_checkpoint()?.1, selected_epoch);
    }

    let mut first_visit = CampaignSave::default();
    let held = receipt.try_lock()?.ok_or("Expected a fresh fixture lock")?;
    assert!(
        checkpoint_visit(&first_visit, selected_epoch)?.is_none(),
        "a lock gained after selection leaves a first visit pending"
    );
    drop(held);
    let binding = checkpoint_visit(&first_visit, selected_epoch)?
        .ok_or("Selected checkpoint did not become admissible after contention")?;
    let first_progress = first_visit.attach(binding);
    assert_eq!(first_visit.lock_checkpoint()?.1, selected_epoch);
    let recheck_binding = checkpoint_visit(&first_visit, selected_epoch)?
        .ok_or("Expected a locked checkpoint visit")?;
    first_progress
        .lock()
        .map_err(|error| error.to_string())?
        .request_reset();
    assert!(
        first_visit
            .validate_resume(&recheck_binding, selected_epoch)
            .is_err(),
        "a CLEAR queued after admission must still stop the final attachment",
    );
    drop(recheck_binding);

    std::fs::remove_file(&receipt.path)
        .and_then(|()| std::fs::remove_file(receipt.path.with_extension("lock")))
        .and_then(|()| std::fs::remove_file(other.path.with_extension("lock")))
        .and_then(|()| std::fs::remove_dir(&folder))
        .map_err(|error| error.to_string())
}

// Two owners share one tiny synthetic receipt. This exercises real lock
// contention, same-epoch union and fresh re-entry before a stale writer runs.
#[test]
fn reentry_waits_for_contended_clear_and_rejects_stale_writers() -> Result<(), String> {
    let folder = std::env::temp_dir().join(format!("places-campaign-epoch-{}", std::process::id()));
    let package = folder.join("fixture.placesmap");
    let receipt = location(&folder, &package, &package, "project:fixture");
    let mut returning = CampaignSave::default();
    let mut clearing = CampaignSave::default();
    let mut stale_writer = CampaignSave::default();
    let mut stale_clear = CampaignSave::default();
    let old = attach(&mut returning, &receipt)?;
    let resetter = attach(&mut clearing, &receipt)?;
    let stale_progress = attach(&mut stale_writer, &receipt)?;
    let stale_reset = attach(&mut stale_clear, &receipt)?;
    old.lock()
        .map_err(|error| error.to_string())?
        .complete("beach")?;
    resetter
        .lock()
        .map_err(|error| error.to_string())?
        .complete("home")?;
    assert!(returning.flush().is_none() && clearing.flush().is_none());
    let merged = read(&receipt.path)?;
    assert!(merged.completed("beach") && merged.completed("home"));
    assert_eq!(merged.generation(), 0);

    old.lock()
        .map_err(|error| error.to_string())?
        .complete("winter")?;
    stale_progress
        .lock()
        .map_err(|error| error.to_string())?
        .complete("pool")?;
    stale_reset
        .lock()
        .map_err(|error| error.to_string())?
        .request_reset();
    let held = receipt.try_lock()?.ok_or("Expected a fresh fixture lock")?;
    resetter
        .lock()
        .map_err(|error| error.to_string())?
        .request_reset();
    assert!(clearing.flush().is_none());
    assert!(prepare(&returning, &receipt)?.is_none());
    assert!(
        old.lock()
            .map_err(|error| error.to_string())?
            .completed("winter")
    );
    assert_eq!(read(&receipt.path)?.generation(), 0);
    drop(held);

    assert!(clearing.flush().is_none());
    let cleared_generation = read(&receipt.path)?.generation();
    assert!(cleared_generation > 0);
    let binding = prepare(&returning, &receipt)?.ok_or("Expected re-entry to resume")?;
    assert_eq!(binding.generation()?, cleared_generation);
    assert!(receipt.try_lock()?.is_none());
    let new = returning.attach(binding);
    assert!(!std::sync::Arc::ptr_eq(&old, &new));
    {
        let fresh = new.lock().map_err(|error| error.to_string())?;
        assert_eq!(fresh.generation(), cleared_generation);
        assert_eq!(fresh.count(), 0);
    }
    assert!(stale_writer.flush().is_some());
    assert!(stale_clear.flush().is_some());
    let unchanged = read(&receipt.path)?;
    assert_eq!(unchanged.generation(), cleared_generation);
    assert_eq!(unchanged.count(), 0);
    std::fs::remove_file(&receipt.path)
        .and_then(|()| std::fs::remove_file(receipt.path.with_extension("lock")))
        .and_then(|()| std::fs::remove_dir(&folder))
        .map_err(|error| error.to_string())
}

#[test]
fn completion_save_roundtrips_atomically_and_refuses_bad_or_oversized_data() -> Result<(), String> {
    let folder = std::env::temp_dir().join(format!("places-campaign-save-{}", std::process::id()));
    let path = folder.join("progress.json");
    let mut progress = CampaignProgress::default();
    for id in ["beach", "home", "arcade", "beach"] {
        progress.complete(id)?;
    }
    write(&path, &progress)?;
    let restored = read(&path)?;
    assert_eq!(
        restored.count(),
        3,
        "repeat completions cannot duplicate a reward"
    );
    assert!(
        restored.completed("home"),
        "completion survives a new reader"
    );
    assert!(
        !path.with_extension("json.tmp").exists(),
        "publication consumes the temporary file"
    );
    progress.complete("winter")?;
    write(&path, &progress)?;
    assert_eq!(
        read(&path)?.count(),
        4,
        "an existing save is atomically replaced"
    );
    std::fs::write(&path, b"{\"completed\":[\"bad\\nidentity\"]}")
        .map_err(|problem| problem.to_string())?;
    assert!(
        read(&path).is_err(),
        "malformed saved identities are refused"
    );
    std::fs::write(&path, vec![b' '; 16_385]).map_err(|problem| problem.to_string())?;
    assert!(
        read(&path).is_err(),
        "save reads stop after the small admission bound"
    );
    std::fs::remove_file(&path).map_err(|problem| problem.to_string())?;
    std::fs::remove_dir(&folder).map_err(|problem| problem.to_string())?;
    Ok(())
}

// Unique value: a failed explicit reset must preserve both live and durable
// unlocks, while a successful empty receipt invalidates old checkpoints. Uses
// four tiny writes and one refused replacement, with no global paths or bake.
#[test]
fn collection_clear_commits_only_after_publication_and_survives_restart() -> Result<(), String> {
    let folder = std::env::temp_dir().join(format!("places-campaign-clear-{}", std::process::id()));
    std::fs::create_dir(&folder).map_err(|problem| problem.to_string())?;
    let package = folder.join("fixture.placesmap");
    let receipt = location(&folder, &package, &package, "project:first");
    let path = receipt.path.clone();
    let mut progress = CampaignProgress::default();
    for id in [
        "home",
        "pool",
        "office",
        "arcade",
        "lantern_hollow",
        "winter",
        "beach",
        "aero",
    ] {
        progress.complete(id)?;
    }
    write(&path, &progress)?;
    let mut save = CampaignSave::default();
    let shared = attach(&mut save, &receipt)?;
    // A directory at the publication target produces a deterministic disk error.
    let blocked = folder.join("blocked.json");
    std::fs::create_dir(&blocked).map_err(|problem| problem.to_string())?;
    shared
        .lock()
        .map_err(|problem| problem.to_string())?
        .request_reset();
    assert!(
        save.lock_checkpoint().is_err(),
        "pending clears cannot be captured"
    );
    let mut blocked_receipt = receipt.clone();
    blocked_receipt.path.clone_from(&blocked);
    save.active.location = Some(blocked_receipt);
    assert!(
        save.flush().is_some(),
        "publication failure must reach the player"
    );
    {
        let unchanged = shared.lock().map_err(|problem| problem.to_string())?;
        assert_eq!(unchanged.count(), 8, "failed clear retains live favors");
        assert_eq!(
            unchanged.generation(),
            0,
            "failed clear cannot invalidate checkpoints"
        );
        assert_eq!(
            unchanged.status(),
            "failed",
            "script must not acknowledge failed clear"
        );
    }
    assert_eq!(
        read(&path)?.count(),
        8,
        "failed clear preserves durable favors"
    );
    save.active.location = Some(receipt);
    shared
        .lock()
        .map_err(|problem| problem.to_string())?
        .request_reset();
    assert!(save.flush().is_none(), "successful clear reports no error");
    {
        let cleared = shared.lock().map_err(|problem| problem.to_string())?;
        assert_eq!(cleared.count(), 0, "successful clear removes live favors");
        assert!(
            cleared.generation() > 0,
            "clear advances checkpoint admission generation"
        );
        assert_eq!(
            cleared.status(),
            "saved",
            "hub can acknowledge durable clear"
        );
    }
    let restarted = read(&path)?;
    assert_eq!(restarted.count(), 0, "empty progress survives restart");
    assert!(
        restarted.generation() > 0,
        "restart cannot admit pre-clear checkpoints"
    );
    shared
        .lock()
        .map_err(|problem| problem.to_string())?
        .complete("home")?;
    assert!(save.flush().is_none(), "new play can save after clearing");
    assert_eq!(
        read(&path)?.count(),
        1,
        "post-clear progress starts a new collection"
    );
    std::fs::remove_file(&path)
        .and_then(|()| std::fs::remove_file(path.with_extension("lock")))
        .and_then(|()| std::fs::remove_file(blocked.with_extension("lock")))
        .and_then(|()| std::fs::remove_dir(&blocked))
        .and_then(|()| std::fs::remove_dir(&folder))
        .map_err(|problem| problem.to_string())?;
    Ok(())
}

// Essential boundary: different projects reusing one installed filename must not
// inherit/clear each other, and legacy data can migrate to only one owner. This
// also closes the same-frame stale-world checkpoint gap using the real attach
// path. A handful of tiny receipt/metadata writes, no global storage, assets or
// bake. The corrupt case adds one refused ordinary write and one explicit reset;
// damaged bytes must survive until the player authorizes their replacement.
#[test]
fn project_scopes_own_legacy_once_and_clear_requires_a_fresh_visit() -> Result<(), String> {
    let folder = std::env::temp_dir().join(format!("places-campaign-scope-{}", std::process::id()));
    std::fs::create_dir(&folder).map_err(|problem| problem.to_string())?;
    let original = std::path::Path::new("same-installed-file.placesmap");
    let canonical = folder.join("canonical-installed-file.placesmap");
    let first = "project:first";
    let second = "project:second";
    let first_path = save_path(&folder, &canonical, first);
    let second_path = save_path(&folder, &canonical, second);
    let legacy = legacy_path(&folder, original);
    let marker = migration_path(&folder, &canonical);
    let mut old = CampaignProgress::default();
    old.complete("home")?;
    write(&legacy, &old)?;
    let mut save = CampaignSave::default();
    let bind = |session: &mut CampaignSave, scope: &str| {
        attach(session, &location(&folder, &canonical, original, scope))
    };
    let first_progress = bind(&mut save, first)?;
    assert_eq!(
        first_progress
            .lock()
            .map_err(|problem| problem.to_string())?
            .count(),
        1,
        "first project migrates exact raw-path progress"
    );
    let second_progress = bind(&mut save, second)?;
    assert_eq!(
        second_progress
            .lock()
            .map_err(|problem| problem.to_string())?
            .count(),
        0,
        "same filename cannot attach another project's live or legacy favors"
    );
    second_progress
        .lock()
        .map_err(|problem| problem.to_string())?
        .complete("pool")?;
    assert!(save.flush().is_none(), "second project's own receipt saves");
    let restored_first = bind(&mut save, first)?;
    assert!(
        restored_first
            .lock()
            .map_err(|problem| problem.to_string())?
            .completed("home"),
        "switching back retains the first project"
    );
    restored_first
        .lock()
        .map_err(|problem| problem.to_string())?
        .request_reset();
    assert!(save.flush().is_none(), "scoped clear publishes");
    assert!(
        save.lock_checkpoint().is_err(),
        "new generation cannot stamp an old unlocked world before re-entry"
    );
    let cleared_generation = read(&first_path)?.generation();
    let _fresh_visit = bind(&mut save, first)?;
    assert_eq!(
        save.lock_checkpoint()?.1,
        cleared_generation,
        "fresh same-package attach releases capture barrier"
    );
    assert_eq!(
        read(&first_path)?.count(),
        0,
        "clear persists only the first project"
    );
    assert!(
        read(&second_path)?.completed("pool"),
        "clear preserves the second project's receipt"
    );
    assert!(
        read(&legacy)?.completed("home"),
        "migration/reset preserve original legacy evidence"
    );
    preserve_corrupt_receipt_until_clear(&folder, &canonical, original, first)?;
    assert!(
        read_installed(&second_path, &canonical, original, second)?
            .0
            .completed("pool"),
        "another project's corruption does not block an existing scoped save"
    );
    for file in [
        first_path.with_extension("lock"),
        second_path.with_extension("lock"),
        first_path,
        second_path,
        legacy,
        marker,
    ] {
        std::fs::remove_file(file).map_err(|problem| problem.to_string())?;
    }
    std::fs::remove_dir(folder).map_err(|problem| problem.to_string())?;
    Ok(())
}

fn preserve_corrupt_receipt_until_clear(
    folder: &std::path::Path,
    canonical: &std::path::Path,
    original: &std::path::Path,
    project: &str,
) -> Result<(), String> {
    let path = save_path(folder, canonical, project);
    let damaged = b"recoverable but malformed collection receipt";
    std::fs::write(&path, damaged).map_err(|problem| problem.to_string())?;
    assert!(
        read_installed(&path, canonical, original, project).is_err(),
        "corrupt scoped receipt cannot resurrect legacy favors"
    );
    let mut restarted = CampaignSave::default();
    let session = attach(
        &mut restarted,
        &location(folder, canonical, original, project),
    )?;
    session
        .lock()
        .map_err(|problem| problem.to_string())?
        .complete("winter")?;
    assert!(
        restarted.flush().is_some(),
        "ordinary completion cannot replace unreadable data"
    );
    assert_eq!(
        std::fs::read(&path)
            .map_err(|problem| problem.to_string())?
            .as_slice(),
        damaged.as_slice(),
        "recoverable original bytes survive ordinary completion and failed flush"
    );
    assert_eq!(
        session
            .lock()
            .map_err(|problem| problem.to_string())?
            .status(),
        "failed",
        "unsaved session progress is not presented as durable"
    );
    session
        .lock()
        .map_err(|problem| problem.to_string())?
        .request_reset();
    assert!(
        restarted.flush().is_none(),
        "explicit clear can replace unreadable data transactionally"
    );
    assert_eq!(
        read(&path)?.count(),
        0,
        "authorized clear repairs the receipt as empty progress"
    );
    Ok(())
}

// LSR-02: failed writes must outlive a project switch and retry after recovery.
// Three tiny independent receipts and an injected Instant avoid sleeps/assets.
#[test]
fn failed_progress_survives_switch_and_retries_without_frame_writes() -> Result<(), String> {
    let folder = std::env::temp_dir().join(format!("places-campaign-retry-{}", std::process::id()));
    std::fs::create_dir(&folder).map_err(|e| e.to_string())?;
    let canonical = folder.join("installed.placesmap");
    let path = save_path(&folder, &canonical, "project:first");
    let bind = |save: &mut CampaignSave, project: &str| {
        attach(save, &location(&folder, &canonical, &canonical, project))
    };
    let mut save = CampaignSave::default();
    let progress = bind(&mut save, "project:first")?;
    progress
        .lock()
        .map_err(|e| e.to_string())?
        .complete("home")?;
    std::fs::create_dir(&path).map_err(|e| e.to_string())?;
    let now = std::time::Instant::now();
    assert!(save.flush_at(now).is_some());
    let other = bind(&mut save, "project:other")?;
    assert_eq!(other.lock().map_err(|e| e.to_string())?.count(), 0);
    assert_eq!(save.retained.len(), 1, "dirty owner survives departure");
    std::fs::remove_dir(&path).map_err(|e| e.to_string())?;
    assert!(save.flush_at(now).is_none());
    assert!(!path.exists(), "retry is bounded, not attempted each frame");
    assert!(
        save.flush_at(now + std::time::Duration::from_secs(2))
            .is_none()
    );
    assert!(
        read(&path)?.completed("home"),
        "inactive project retries too"
    );
    assert!(
        save.retained.is_empty(),
        "durable inactive owners can be released"
    );
    let returned = bind(&mut save, "project:first")?;
    assert!(
        returned
            .lock()
            .map_err(|e| e.to_string())?
            .completed("home")
    );
    returned
        .lock()
        .map_err(|e| e.to_string())?
        .complete("pool")?;
    std::fs::remove_file(&path).map_err(|e| e.to_string())?;
    std::fs::create_dir(&path).map_err(|e| e.to_string())?;
    assert!(save.flush().is_some());
    let _other = bind(&mut save, "project:other")?;
    let unreadable = bind(&mut save, "project:first")?;
    assert!(std::sync::Arc::ptr_eq(&returned, &unreadable));
    assert!(save.lock_checkpoint().is_err());
    unreadable
        .lock()
        .map_err(|error| error.to_string())?
        .request_reset();
    assert!(save.flush().is_some());
    assert!(path.is_dir(), "CLEAR cannot repair an unreadable object");
    std::fs::remove_dir(&path).map_err(|e| e.to_string())?;
    let retained = bind(&mut save, "project:first")?;
    assert!(
        std::sync::Arc::ptr_eq(&returned, &retained),
        "verified same-epoch re-entry reuses the dirty owner"
    );
    assert!(
        retained
            .lock()
            .map_err(|e| e.to_string())?
            .completed("pool")
    );
    assert!(
        save.flush_at(std::time::Instant::now() + std::time::Duration::from_secs(2))
            .is_none()
    );
    let mut restart = CampaignSave::default();
    assert_eq!(
        bind(&mut restart, "project:first")?
            .lock()
            .map_err(|e| e.to_string())?
            .count(),
        2
    );
    std::fs::remove_dir_all(&folder).map_err(|e| e.to_string())?;
    Ok(())
}

// A bounded decode refusal must not strand ordinary travel or make a large
// receipt eligible for repair. One synthetic 16 KiB buffer and a retry.
#[test]
fn oversized_receipt_keeps_dirty_travel_read_only_until_same_epoch_is_readable()
-> Result<(), String> {
    let folder =
        std::env::temp_dir().join(format!("places-campaign-oversize-{}", std::process::id()));
    let package = folder.join("fixture.placesmap");
    let receipt = location(&folder, &package, &package, "project:fixture");
    let mut save = CampaignSave::default();
    let previous = attach(&mut save, &receipt)?;
    previous
        .lock()
        .map_err(|error| error.to_string())?
        .complete("home")?;
    let oversized = vec![b' '; 16_385];
    std::fs::write(&receipt.path, &oversized).map_err(|error| error.to_string())?;
    let travelled = attach(&mut save, &receipt)?;
    assert!(std::sync::Arc::ptr_eq(&travelled, &previous));
    assert!(save.lock_checkpoint().is_err());
    assert!(
        save.prepare_binding(receipt.installation.clone(), receipt.try_lock(), Some(0))
            .is_err()
    );
    travelled
        .lock()
        .map_err(|error| error.to_string())?
        .request_reset();
    assert!(save.flush().is_some());
    assert_eq!(
        std::fs::read(&receipt.path).map_err(|error| error.to_string())?,
        oversized
    );
    assert!(
        travelled
            .lock()
            .map_err(|error| error.to_string())?
            .completed("home")
    );
    write(&receipt.path, &CampaignProgress::default())?;
    let rebound = attach(&mut save, &receipt)?;
    assert!(std::sync::Arc::ptr_eq(&travelled, &rebound));
    assert!(save.flush().is_none());
    assert!(read(&receipt.path)?.completed("home"));
    std::fs::remove_file(&receipt.path)
        .and_then(|()| std::fs::remove_file(receipt.path.with_extension("lock")))
        .and_then(|()| std::fs::remove_dir(&folder))
        .map_err(|error| error.to_string())
}
