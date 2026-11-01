//! Atomic completion saves, scoped to canonical installation and declared project.
//! Rebuilds retain progress; another project at the same filename cannot inherit it.
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::engine::scripting::{CampaignProgress, SharedCampaign};

#[cfg(test)]
#[path = "tests/campaign.rs"]
mod tests;

#[derive(Default)]
pub(super) struct CampaignSave {
    package: Option<PathBuf>,
    project: String,
    visit_generation: u64,
    save: Option<PathBuf>,
    progress: SharedCampaign,
    saved_count: usize,
    saved_generation: u64,
    receipt_valid: bool,
    failed_count: Option<(u64, usize)>,
}

impl CampaignSave {
    pub fn attach(
        &mut self,
        package: &Path,
        project_id: Option<&str>,
        level_id: &str,
    ) -> SharedCampaign {
        let canonical = std::fs::canonicalize(package).unwrap_or_else(|problem| {
            crate::engine::logging::warn(format!(
                "Campaign installation path unavailable: {problem}"
            ));
            package.to_path_buf()
        });
        let project = project_scope(project_id, level_id);
        let save = crate::engine::paths::user()
            .map(|paths| save_path(&paths.saves(), &canonical, &project));
        self.attach_resolved(package, canonical, project, save)
    }

    fn attach_resolved(
        &mut self,
        package: &Path,
        canonical: PathBuf,
        project: String,
        save: Result<PathBuf, String>,
    ) -> SharedCampaign {
        // This method is called only when a fresh world is committed. Its
        // invocation admits the new visit's authored state for checkpoints.
        self.failed_count = None;
        let _prior_flush_notice = self.flush();
        if self.package.as_deref() == Some(canonical.as_path()) && self.project == project {
            return self.enter_visit();
        }
        let restoration = save
            .as_ref()
            .map_err(ToString::to_string)
            .and_then(|path| read_installed(path, &canonical, package, &project));
        self.receipt_valid = restoration.is_ok();
        let (progress, migrated) = match restoration {
            Ok(restored) => restored,
            Err(problem) => {
                crate::engine::logging::warn(format!("Campaign save unavailable: {problem}"));
                // A fresh epoch prevents a repaired corrupt receipt from admitting
                // old checkpoints with an unknowable earlier reset generation.
                let mut empty = CampaignProgress::empty_at(next_generation(0).unwrap_or(u64::MAX));
                empty.mark_failed();
                (empty, false)
            }
        };
        self.saved_count = if migrated {
            usize::MAX
        } else {
            progress.count()
        };
        self.saved_generation = progress.generation();
        self.failed_count = None;
        self.package = Some(canonical);
        self.project = project;
        self.save = save.ok();
        self.progress = std::sync::Arc::new(std::sync::Mutex::new(progress));
        if migrated {
            // Copy exactly one owned legacy receipt; retain it for recovery.
            let _migration_flush_notice = self.flush();
        }
        self.enter_visit()
    }

    fn enter_visit(&mut self) -> SharedCampaign {
        self.visit_generation = self
            .progress
            .lock()
            .map_or(u64::MAX, |progress| progress.generation());
        self.progress.clone()
    }

    /// # Errors
    /// Refuses unavailable or corrupt installation receipts.
    pub fn generation_for(&self, entry: &crate::engine::loader::LevelEntry) -> Result<u64, String> {
        let canonical =
            std::fs::canonicalize(&entry.path).map_err(|problem| problem.to_string())?;
        let project_id = crate::engine::package::project::inspect(&entry.path)?
            .map(|package| package.project.id);
        let project = project_scope(project_id.as_deref(), &entry.id);
        if self.package.as_deref() == Some(canonical.as_path()) && self.project == project {
            return self.retained_generation();
        }
        let path = save_path(&crate::engine::paths::user()?.saves(), &canonical, &project);
        read_installed(&path, &canonical, &entry.path, &project)
            .map(|(progress, _migrated)| progress.generation())
    }

    /// # Errors
    /// Refuses capture until a fresh authored world attaches after a clear.
    pub fn current_generation(&self) -> Result<u64, String> {
        let generation = self.retained_generation()?;
        if generation != self.visit_generation {
            return Err(
                "Collection progress was cleared. Re-enter this level before saving a checkpoint."
                    .into(),
            );
        }
        Ok(generation)
    }

    fn retained_generation(&self) -> Result<u64, String> {
        if !self.receipt_valid {
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

    /// Returns a newly encountered failure for the player's visible notice.
    pub fn flush(&mut self) -> Option<String> {
        let Ok(mut progress) = self.progress.lock() else {
            return None;
        };
        let resetting = progress.reset_pending();
        let identity = (progress.generation(), progress.count());
        if !resetting && !self.receipt_valid && self.package.is_some() {
            if self.failed_count == Some(identity) {
                return None;
            }
            progress.mark_failed();
            self.failed_count = Some(identity);
            drop(progress);
            let notice = "Collection receipt is unreadable; original data is preserved. Repair it and restart, or confirm CLEAR to replace it.".to_string();
            crate::engine::logging::warn(&notice);
            return Some(notice);
        }
        if !resetting
            && ((progress.count() == self.saved_count
                && progress.generation() == self.saved_generation)
                || self.failed_count == Some(identity))
        {
            return None;
        }
        let prepared_reset = if resetting {
            next_generation(progress.generation())
                .map(|generation| Some(CampaignProgress::empty_at(generation)))
        } else {
            Ok(None)
        };
        let result = prepared_reset.and_then(|reset_candidate| {
            self.save
                .as_ref()
                .ok_or_else(|| "No writable campaign save path".to_string())
                .and_then(|path| write(path, reset_candidate.as_ref().unwrap_or(&progress)))?;
            Ok(reset_candidate)
        });
        match result {
            Ok(published_reset) => {
                if let Some(cleared) = published_reset {
                    *progress = cleared;
                }
                progress.mark_saved();
                self.receipt_valid = true;
                self.saved_count = progress.count();
                self.saved_generation = progress.generation();
                self.failed_count = None;
                None
            }
            Err(problem) => {
                self.failed_count = Some(identity);
                progress.mark_failed();
                let notice = if resetting {
                    format!("Collection reset failed; progress was not cleared: {problem}")
                } else {
                    format!("Progress kept for this session; saving failed: {problem}")
                };
                crate::engine::logging::warn(&notice);
                Some(notice)
            }
        }
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
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(problem) if problem.kind() == std::io::ErrorKind::NotFound => {
            return Ok(CampaignProgress::default());
        }
        Err(problem) => return Err(problem.to_string()),
    };
    let mut bytes = Vec::new();
    let _read = file
        .take(16_385)
        .read_to_end(&mut bytes)
        .map_err(|problem| problem.to_string())?;
    if bytes.len() > 16_384 {
        return Err("Campaign save exceeds 16 KiB".into());
    }
    let progress: CampaignProgress =
        serde_json::from_slice(&bytes).map_err(|problem| problem.to_string())?;
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
