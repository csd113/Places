//! Installation ownership and the stable advisory lock for a replaceable receipt.
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::Read as _;
use std::path::{Path, PathBuf};

use super::{CampaignProgress, decode_progress, project_scope, read_installed, save_path, write};

#[derive(Clone)]
pub(in crate::game) struct Installation {
    pub canonical: PathBuf,
    original: PathBuf,
    project: String,
}

impl Installation {
    pub fn new(
        original: PathBuf,
        canonical: PathBuf,
        project_id: Option<&str>,
        level_id: &str,
    ) -> Self {
        Self {
            canonical,
            original,
            project: project_scope(project_id, level_id),
        }
    }

    pub fn same_owner(&self, other: &Self) -> bool {
        self.canonical == other.canonical && self.project == other.project
    }

    pub fn original(&self) -> &Path {
        &self.original
    }

    pub fn verify_path(&self) -> Result<(), String> {
        let current = fs::canonicalize(&self.original).map_err(|problem| problem.to_string())?;
        if current != self.canonical {
            return Err("Map installation changed during loading; select the level again".into());
        }
        Ok(())
    }

    pub(super) fn location(&self, folder: &Path) -> Location {
        Location {
            installation: self.clone(),
            path: save_path(folder, &self.canonical, &self.project),
        }
    }
}

#[derive(Clone)]
pub(super) struct Location {
    pub installation: Installation,
    pub path: PathBuf,
}

pub(in crate::game) struct LockedReceipt {
    pub(super) location: Location,
    _lock: File,
}

pub(super) enum Stored {
    Valid(CampaignProgress, bool),
    Unreadable {
        problem: String,
        damaged: Option<String>,
    },
}

impl Location {
    pub fn try_lock(&self) -> Result<Option<LockedReceipt>, String> {
        let parent = self.path.parent().ok_or("Campaign save has no parent")?;
        fs::create_dir_all(parent).map_err(|problem| problem.to_string())?;
        // The lock file must outlive each holder. Unlinking it would let old
        // and new holders lock different objects for the same receipt.
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.path.with_extension("lock"))
            .map_err(|problem| problem.to_string())?;
        match file.try_lock() {
            Ok(()) => Ok(Some(LockedReceipt {
                location: self.clone(),
                _lock: file,
            })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(problem)) => Err(problem.to_string()),
        }
    }
}

impl LockedReceipt {
    pub const fn installation(&self) -> &Installation {
        &self.location.installation
    }

    pub fn directory(&self) -> Result<&Path, String> {
        self.location
            .path
            .parent()
            .ok_or_else(|| "Campaign receipt has no parent directory".into())
    }

    pub(super) fn read(&self) -> Stored {
        let installation = &self.location.installation;
        match read_installed(
            &self.location.path,
            &installation.canonical,
            &installation.original,
            &installation.project,
        ) {
            Ok((progress, migrated)) => Stored::Valid(progress, migrated),
            Err(problem) => Stored::Unreadable {
                problem,
                damaged: bounded_fingerprint(&self.location.path),
            },
        }
    }

    pub(super) fn write(&self, progress: &CampaignProgress) -> Result<(), String> {
        write(&self.location.path, progress)
    }
}

fn bounded_fingerprint(path: &Path) -> Option<String> {
    // Only these exact bounded bytes can authorize a later explicit CLEAR.
    // Inaccessible, oversized and legacy files stay intact.
    let mut bytes = Vec::new();
    let _read = File::open(path)
        .ok()?
        .take(16_385)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= 16_384 && decode_progress(&bytes).is_err())
        .then(|| crate::engine::package::hash::sha256_hex(&bytes))
}
