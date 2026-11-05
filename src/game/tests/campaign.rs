//! One tiny save guards the actual disk round trip, bounded admission and
//! idempotence across app restarts. No environment mutation, asset load or bake.
use super::{
    CampaignProgress, CampaignSave, CampaignSession, legacy_path, migration_path, read,
    read_installed, save_path, write,
};

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
    let path = folder.join("progress.json");
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
    let shared = std::sync::Arc::new(std::sync::Mutex::new(read(&path)?));
    let mut save = CampaignSave::default();
    save.active = CampaignSession {
        save: Some(path.clone()),
        progress: shared.clone(),
        saved_count: 8,
        receipt_valid: true,
        ..CampaignSession::default()
    };
    // A directory at the publication target produces a deterministic disk error.
    let blocked = folder.join("blocked.json");
    std::fs::create_dir(&blocked).map_err(|problem| problem.to_string())?;
    save.active.save = Some(blocked.clone());
    shared
        .lock()
        .map_err(|problem| problem.to_string())?
        .request_reset();
    assert!(
        save.current_generation().is_err(),
        "pending clears cannot be captured"
    );
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
    save.active.save = Some(path.clone());
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
        session.attach_resolved(
            original,
            canonical.clone(),
            scope.to_string(),
            Ok(save_path(&folder, &canonical, scope)),
        )
    };
    let first_progress = bind(&mut save, first);
    assert_eq!(
        first_progress
            .lock()
            .map_err(|problem| problem.to_string())?
            .count(),
        1,
        "first project migrates exact raw-path progress"
    );
    let second_progress = bind(&mut save, second);
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
    let restored_first = bind(&mut save, first);
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
        save.current_generation().is_err(),
        "new generation cannot stamp an old unlocked world before re-entry"
    );
    let cleared_generation = save.retained_generation()?;
    let _fresh_visit = bind(&mut save, first);
    assert_eq!(
        save.current_generation()?,
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
    for file in [first_path, second_path, legacy, marker] {
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
    let session = restarted.attach_resolved(
        original,
        canonical.to_path_buf(),
        project.to_string(),
        Ok(path.clone()),
    );
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
        save.attach_resolved(
            &canonical,
            canonical.clone(),
            project.into(),
            Ok(save_path(&folder, &canonical, project)),
        )
    };
    let mut save = CampaignSave::default();
    let progress = bind(&mut save, "project:first");
    progress
        .lock()
        .map_err(|e| e.to_string())?
        .complete("home")?;
    std::fs::create_dir(&path).map_err(|e| e.to_string())?;
    let now = std::time::Instant::now();
    assert!(save.flush_at(now).is_some());
    let other = bind(&mut save, "project:other");
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
    let returned = bind(&mut save, "project:first");
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
    let _other = bind(&mut save, "project:other");
    let retained = bind(&mut save, "project:first");
    assert!(
        std::sync::Arc::ptr_eq(&returned, &retained),
        "return before recovery reuses the dirty owner"
    );
    assert!(
        retained
            .lock()
            .map_err(|e| e.to_string())?
            .completed("pool")
    );
    std::fs::remove_dir(&path).map_err(|e| e.to_string())?;
    assert!(
        save.flush_at(std::time::Instant::now() + std::time::Duration::from_secs(2))
            .is_none()
    );
    let mut restart = CampaignSave::default();
    assert_eq!(
        bind(&mut restart, "project:first")
            .lock()
            .map_err(|e| e.to_string())?
            .count(),
        2
    );
    std::fs::remove_dir_all(&folder).map_err(|e| e.to_string())?;
    Ok(())
}
