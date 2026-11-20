//! Atomic completion saves, scoped to canonical installation and declared project.
//! Rebuilds retain progress; another project at the same filename cannot inherit it.
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::engine::scripting::{CampaignProgress, SharedCampaign};

#[cfg(test)]
#[path = "tests/campaign.rs"]
mod tests;

mod receipt;

pub(super) use receipt::{Installation, LockedReceipt};
use receipt::{Location, Stored};

/// A visit keeps the exact receipt locked from admission through attachment.
pub(super) struct Binding {
    installation: Installation,
    receipt: Option<LockedReceipt>,
    stored: Stored,
}

#[derive(Default)]
pub(super) struct CampaignSave {
    active: CampaignSession,
    retained: Vec<CampaignSession>,
    visit_generation: u64,
}

#[derive(Default)]
struct CampaignSession {
    installation: Option<Installation>,
    location: Option<Location>,
    progress: SharedCampaign,
    saved_count: usize,
    saved_generation: u64,
    epoch_known: bool,
    receipt_health: ReceiptHealth,
    stale: bool,
    failed_count: Option<(u64, usize)>,
    retry_after: Option<std::time::Instant>,
}

#[derive(Default)]
enum ReceiptHealth {
    #[default]
    Readable,
    Unavailable {
        damaged: Option<String>,
    },
}

impl ReceiptHealth {
    const fn is_readable(&self) -> bool {
        matches!(self, Self::Readable)
    }

    fn damaged(&self) -> Option<&str> {
        match self {
            Self::Readable => None,
            Self::Unavailable { damaged } => damaged.as_deref(),
        }
    }
}

impl CampaignSave {
    pub fn prepare_visit(
        &self,
        installation: Installation,
        checkpoint_generation: Option<u64>,
    ) -> Result<Option<Binding>, String> {
        let locked = crate::engine::paths::user()
            .and_then(|paths| installation.location(&paths.saves()).try_lock());
        self.prepare_binding(installation, locked, checkpoint_generation)
    }

    fn prepare_binding(
        &self,
        installation: Installation,
        locked: Result<Option<LockedReceipt>, String>,
        checkpoint_generation: Option<u64>,
    ) -> Result<Option<Binding>, String> {
        let (receipt, stored) = match locked {
            Ok(Some(receipt)) => {
                let stored = receipt.read();
                (Some(receipt), stored)
            }
            // Contention cannot establish an epoch. The caller leaves its
            // prepared world pending and tries again without attaching.
            Ok(None) => return Ok(None),
            Err(problem) => (
                None,
                Stored::Unreadable {
                    problem,
                    damaged: None,
                },
            ),
        };
        let binding = Binding {
            installation,
            receipt,
            stored,
        };
        if let Some(expected) = checkpoint_generation {
            self.validate_resume(&binding, expected)?;
        }
        if let Some(owner) = self.owner(&binding.installation) {
            owner.check_reentry(&binding.stored)?;
        }
        Ok(Some(binding))
    }

    pub fn validate_resume(&self, binding: &Binding, expected: u64) -> Result<(), String> {
        if expected != binding.generation()? {
            return Err("This checkpoint belongs to a different collection generation. Start new to keep the current progress.".into());
        }
        if let Some(owner) = self.owner(&binding.installation) {
            let progress = owner
                .progress
                .lock()
                .map_err(|_problem| "Campaign state unavailable")?;
            if expected == progress.generation() && progress.reset_pending() {
                return Err(
                    "Wait for the collection reset to finish before saving or resuming".into(),
                );
            }
        }
        Ok(())
    }

    pub fn attach(&mut self, binding: Binding) -> SharedCampaign {
        let previous = std::mem::take(&mut self.active);
        let existing = if previous.owns(&binding.installation) {
            Some(previous)
        } else {
            if previous.needs_retention() {
                self.retained.push(previous);
            }
            self.retained
                .iter()
                .position(|session| session.owns(&binding.installation))
                .map(|index| self.retained.swap_remove(index))
        };
        self.active = CampaignSession::restore(&binding, existing);
        self.visit_generation = self
            .active
            .progress
            .lock()
            .map_or(u64::MAX, |progress| progress.generation());
        drop(binding);
        self.active.progress.clone()
    }

    fn owner(&self, installation: &Installation) -> Option<&CampaignSession> {
        std::iter::once(&self.active)
            .chain(&self.retained)
            .find(|session| session.owns(installation))
    }

    pub fn owns_current(&self, installation: &Installation) -> bool {
        self.active.owns(installation)
    }

    pub fn lock_checkpoint(&self) -> Result<(LockedReceipt, u64), String> {
        let location = self
            .active
            .location
            .as_ref()
            .ok_or("No collection receipt is attached")?;
        let locked = location
            .try_lock()?
            .ok_or("Collection save is busy. Retry the checkpoint.")?;
        let Stored::Valid(progress, _) = locked.read() else {
            return Err("Collection save is unavailable; repair it or clear progress before saving a checkpoint".into());
        };
        let generation = progress.generation();
        if self.active.retained_generation()? != generation || generation != self.visit_generation {
            return Err(
                "Collection progress changed. Re-enter this level before saving a checkpoint."
                    .into(),
            );
        }
        Ok((locked, generation))
    }

    /// Retry failed publications at most once per second, including departed
    /// projects. Repeated failures stay quiet; the first failure remains visible.
    pub fn flush(&mut self) -> Option<String> {
        self.flush_at(std::time::Instant::now())
    }

    fn flush_at(&mut self, now: std::time::Instant) -> Option<String> {
        let mut notice = self.active.flush_at(now);
        for session in &mut self.retained {
            let problem = session.flush_at(now);
            if notice.is_none() {
                notice = problem;
            }
        }
        self.retained.retain(CampaignSession::needs_retention);
        notice
    }
}

impl Binding {
    pub const fn installation(&self) -> &Installation {
        &self.installation
    }

    pub fn directory(&self) -> Result<&Path, String> {
        self.receipt
            .as_ref()
            .ok_or("No verified collection receipt")?
            .directory()
    }

    /// The receipt epoch observed when admitting this visit.
    fn generation(&self) -> Result<u64, String> {
        match &self.stored {
            Stored::Valid(progress, _) => Ok(progress.generation()),
            Stored::Unreadable {
                problem,
                damaged: _,
            } => Err(format!("Collection save is unavailable: {problem}")),
        }
    }
}

impl CampaignSession {
    fn owns(&self, installation: &Installation) -> bool {
        self.installation
            .as_ref()
            .is_some_and(|owner| owner.same_owner(installation))
    }

    fn pending_updates(&self, progress: &CampaignProgress) -> bool {
        progress.reset_pending()
            || progress.count() != self.saved_count
            || progress.generation() != self.saved_generation
    }

    fn needs_retention(&self) -> bool {
        !self.stale
            && self.installation.is_some()
            && self
                .progress
                .lock()
                .map_or(true, |progress| self.pending_updates(&progress))
    }

    fn retained_generation(&self) -> Result<u64, String> {
        if !self.epoch_known || !self.receipt_health.is_readable() || self.stale {
            return Err("Collection save is unavailable; repair it or clear progress before resuming a checkpoint".into());
        }
        let progress = self
            .progress
            .lock()
            .map_err(|_problem| "Campaign state unavailable")?;
        if progress.reset_pending() {
            return Err("Wait for the collection reset to finish before saving or resuming".into());
        }
        Ok(progress.generation())
    }

    fn check_reentry(&self, stored: &Stored) -> Result<(), String> {
        let progress = self
            .progress
            .lock()
            .map_err(|_problem| "Campaign state unavailable")?;
        if let Stored::Valid(installed, _) = stored
            && self.epoch_known
            && progress.generation() == installed.generation()
        {
            // Verify that two bounded receipts can be combined before the
            // renderer commits. The live owner is untouched if this fails.
            let mut candidate = progress.clone();
            drop(progress);
            return candidate.merge_installed(installed);
        }
        drop(progress);
        Ok(())
    }

    fn restore(binding: &Binding, existing: Option<Self>) -> Self {
        let installation = Some(binding.installation.clone());
        let location = binding
            .receipt
            .as_ref()
            .map(|receipt| receipt.location.clone());
        let (progress, epoch_known, receipt_health, migrated) = match &binding.stored {
            Stored::Valid(installed, migrated) => {
                if let Some(mut owner) = existing {
                    let reused = owner.epoch_known
                        && owner.progress.lock().is_ok_and(|mut progress| {
                            if progress.generation() != installed.generation() {
                                return false;
                            }
                            if let Err(problem) = progress.merge_installed(installed) {
                                progress.mark_failed();
                                crate::engine::logging::warn(problem);
                            }
                            true
                        });
                    if reused {
                        owner.installation = installation;
                        owner.location = location;
                        owner.saved_count = if *migrated {
                            usize::MAX
                        } else {
                            installed.count()
                        };
                        owner.saved_generation = installed.generation();
                        owner.retry_after = None;
                        owner.receipt_health = ReceiptHealth::Readable;
                        owner.stale = false;
                        return owner;
                    }
                }
                (installed.clone(), true, ReceiptHealth::Readable, *migrated)
            }
            Stored::Unreadable { problem, damaged } => {
                crate::engine::logging::warn(format!("Campaign save unavailable: {problem}"));
                let receipt_health = ReceiptHealth::Unavailable {
                    damaged: damaged.clone(),
                };
                if let Some(mut owner) = existing
                    && !owner.stale
                {
                    // Keep the live collection for ordinary travel. A CLEAR
                    // queued in the previous visit cannot authorize a repair.
                    if let Ok(mut progress) = owner.progress.lock() {
                        progress.mark_failed();
                    }
                    owner.installation = installation;
                    owner.location = location.or(owner.location);
                    owner.receipt_health = receipt_health;
                    return owner;
                }
                let mut empty = CampaignProgress::empty_at(next_generation(0).unwrap_or(u64::MAX));
                empty.mark_failed();
                (empty, false, receipt_health, false)
            }
        };
        Self {
            installation,
            location,
            saved_count: if migrated {
                usize::MAX
            } else {
                progress.count()
            },
            saved_generation: progress.generation(),
            progress: std::sync::Arc::new(std::sync::Mutex::new(progress)),
            epoch_known,
            receipt_health,
            ..Self::default()
        }
    }

    /// Returns a newly encountered failure for the player's visible notice.
    fn flush_at(&mut self, now: std::time::Instant) -> Option<String> {
        let Ok(mut progress) = self.progress.lock() else {
            return None;
        };
        if self.stale || self.installation.is_none() {
            return None;
        }
        let resetting = progress.reset_pending();
        let identity = (progress.generation(), progress.count());
        if !resetting && !self.epoch_known {
            if self.failed_count == Some(identity) {
                return None;
            }
            progress.mark_failed();
            self.failed_count = Some(identity);
            let notice = "Collection receipt is unreadable; original data is preserved. Repair it and restart, or confirm CLEAR to replace an unchanged corrupt receipt.".to_string();
            crate::engine::logging::warn(&notice);
            return Some(notice);
        }
        if !resetting
            && (!self.pending_updates(&progress)
                || self.retry_after.is_some_and(|deadline| now < deadline))
        {
            return None;
        }
        let acquisition = self
            .location
            .as_ref()
            .ok_or_else(|| "No writable campaign save path".to_string())
            .and_then(Location::try_lock);
        let result = match &acquisition {
            Ok(Some(receipt)) => self.publish(receipt, &progress, resetting),
            Ok(None) => return None,
            Err(problem) => Err(problem.clone()),
        };
        match result {
            Ok(Some(published)) => {
                *progress = published;
                progress.mark_saved();
                self.epoch_known = true;
                self.receipt_health = ReceiptHealth::Readable;
                self.saved_count = progress.count();
                self.saved_generation = progress.generation();
                self.failed_count = None;
                self.retry_after = None;
                None
            }
            Ok(None) => {
                self.stale = true;
                progress.mark_failed();
                let notice = "Collection progress changed in another session. Re-enter this level before saving or clearing.".to_string();
                crate::engine::logging::warn(&notice);
                Some(notice)
            }
            Err(problem) => {
                let newly_failed = resetting || self.failed_count != Some(identity);
                self.retry_after = now.checked_add(std::time::Duration::from_secs(1));
                self.failed_count = Some(identity);
                progress.mark_failed();
                let notice = if resetting {
                    format!("Collection reset failed; progress was not cleared: {problem}")
                } else {
                    format!("Progress kept for this session; saving failed: {problem}")
                };
                if newly_failed {
                    crate::engine::logging::warn(&notice);
                    Some(notice)
                } else {
                    None
                }
            }
        }
    }

    fn publish(
        &self,
        receipt: &LockedReceipt,
        progress: &CampaignProgress,
        resetting: bool,
    ) -> Result<Option<CampaignProgress>, String> {
        let candidate = match receipt.read() {
            Stored::Valid(installed, _)
                if self.epoch_known && installed.generation() == progress.generation() =>
            {
                if resetting {
                    if !self.receipt_health.is_readable() {
                        return Err("The unavailable receipt was replaced. Re-enter this level before clearing it.".into());
                    }
                    CampaignProgress::empty_at(next_generation(progress.generation())?)
                } else {
                    let mut merged = progress.clone();
                    merged.merge_installed(&installed)?;
                    merged
                }
            }
            Stored::Valid(_, _) => return Ok(None),
            Stored::Unreadable { problem, damaged }
                if resetting
                    && damaged.is_some()
                    && damaged.as_deref() == self.receipt_health.damaged() =>
            {
                drop(problem);
                CampaignProgress::empty_at(next_generation(progress.generation())?)
            }
            Stored::Unreadable {
                problem,
                damaged: _,
            } => return Err(problem),
        };
        receipt.write(&candidate)?;
        Ok(Some(candidate))
    }
}

fn next_generation(previous: u64) -> Result<u64, String> {
    let minimum = previous
        .checked_add(1)
        .ok_or("Campaign reset generation exhausted")?;
    let clock = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|problem| problem.to_string())?;
    let epoch = u64::try_from(clock.as_nanos()).map_err(|problem| problem.to_string())?;
    Ok(minimum.max(epoch))
}

fn project_scope(project_id: Option<&str>, level_id: &str) -> String {
    project_id.map_or_else(
        || format!("standalone:{level_id}"),
        |id| format!("project:{id}"),
    )
}

fn save_path(folder: &Path, package: &Path, project: &str) -> PathBuf {
    let identity = crate::engine::package::hash::sha256_hex(
        format!("{project}\n{}", package.to_string_lossy()).as_bytes(),
    );
    folder.join(format!("campaign-{identity}.json"))
}

fn legacy_path(folder: &Path, package: &Path) -> PathBuf {
    let identity = crate::engine::package::hash::sha256_hex(package.to_string_lossy().as_bytes());
    folder.join(format!("campaign-{identity}.json"))
}

fn migration_path(folder: &Path, canonical: &Path) -> PathBuf {
    let identity = crate::engine::package::hash::sha256_hex(canonical.to_string_lossy().as_bytes());
    folder.join(format!("campaign-migration-{identity}.json"))
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct MigrationOwner {
    project: String,
}

fn owns_migration(path: &Path, project: &str) -> Result<bool, String> {
    let owner = MigrationOwner {
        project: project.to_string(),
    };
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
    {
        Ok(mut file) => {
            // Exclusive creation arbitrates first use. A truncated marker is
            // refused on restart; it can never authorize another project's copy.
            let bytes = serde_json::to_vec(&owner).map_err(|problem| problem.to_string())?;
            file.write_all(&bytes)
                .and_then(|()| file.sync_all())
                .map_err(|problem| problem.to_string())?;
            Ok(true)
        }
        Err(creation_problem) if creation_problem.kind() == std::io::ErrorKind::AlreadyExists => {
            let file = std::fs::File::open(path).map_err(|problem| problem.to_string())?;
            let mut bytes = Vec::new();
            let _read = file
                .take(513)
                .read_to_end(&mut bytes)
                .map_err(|problem| problem.to_string())?;
            if bytes.len() > 512 {
                return Err("Campaign migration owner exceeds 512 bytes".into());
            }
            let existing: MigrationOwner =
                serde_json::from_slice(&bytes).map_err(|problem| problem.to_string())?;
            if existing.project.is_empty()
                || existing.project.len() > 128
                || existing.project.chars().any(char::is_control)
            {
                return Err("Campaign migration owner is invalid".into());
            }
            Ok(existing.project == project)
        }
        Err(problem) => Err(problem.to_string()),
    }
}

fn read_installed(
    path: &Path,
    canonical: &Path,
    original_package: &Path,
    project: &str,
) -> Result<(CampaignProgress, bool), String> {
    // A present receipt, including a corrupt/inaccessible one, is authoritative.
    // Never resurrect pre-reset legacy data as an error-recovery fallback.
    if path.try_exists().map_err(|problem| problem.to_string())? {
        return read(path).map(|progress| (progress, false));
    }
    let parent = path.parent().ok_or("Campaign save has no parent")?;
    let canonical_legacy = legacy_path(parent, canonical);
    let raw_legacy = legacy_path(parent, original_package);
    let legacy_candidate = if canonical_legacy
        .try_exists()
        .map_err(|problem| problem.to_string())?
    {
        Some(canonical_legacy)
    } else if raw_legacy
        .try_exists()
        .map_err(|problem| problem.to_string())?
    {
        Some(raw_legacy)
    } else {
        None
    };
    let Some(legacy) = legacy_candidate else {
        return Ok((CampaignProgress::default(), false));
    };
    if owns_migration(&migration_path(parent, canonical), project)? {
        read(&legacy).map(|progress| (progress, true))
    } else {
        // An older path-only receipt can be adopted by one declared project.
        // Switching the installed filename to another project starts separately.
        Ok((CampaignProgress::default(), false))
    }
}

fn read(path: &Path) -> Result<CampaignProgress, String> {
    let file = std::fs::File::open(path).map_err(|problem| problem.to_string())?;
    let mut bytes = Vec::new();
    let _read = file
        .take(16_385)
        .read_to_end(&mut bytes)
        .map_err(|problem| problem.to_string())?;
    if bytes.len() > 16_384 {
        return Err("Campaign save exceeds 16 KiB".into());
    }
    decode_progress(&bytes)
}

fn decode_progress(bytes: &[u8]) -> Result<CampaignProgress, String> {
    let progress: CampaignProgress =
        serde_json::from_slice(bytes).map_err(|problem| problem.to_string())?;
    progress.validate()?;
    Ok(progress)
}

fn write(path: &Path, progress: &CampaignProgress) -> Result<(), String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let parent = path.parent().ok_or("Campaign save has no parent")?;
    std::fs::create_dir_all(parent).map_err(|problem| problem.to_string())?;
    let temporary = parent.join(format!(
        ".campaign-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let bytes = serde_json::to_vec(progress).map_err(|problem| problem.to_string())?;
    let result = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|problem| problem.to_string())?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(|problem| problem.to_string())?;
        drop(file);
        std::fs::rename(&temporary, path).map_err(|problem| problem.to_string())
    })();
    if result.is_err()
        && let Err(problem) = std::fs::remove_file(&temporary)
        && problem.kind() != std::io::ErrorKind::NotFound
    {
        crate::engine::logging::warn(format!("Campaign temporary cleanup: {problem}"));
    }
    result
}
