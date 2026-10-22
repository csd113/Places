//! Small atomic completion saves, keyed to the exact installed package path.
//! A package rebuild keeps progress; another project/path cannot inherit it.
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::engine::scripting::{CampaignProgress, SharedCampaign};

#[cfg(test)]
#[path = "tests/campaign.rs"]
mod tests;

#[derive(Default)]
pub(super) struct CampaignSave {
    package: Option<PathBuf>,
    save: Option<PathBuf>,
    progress: SharedCampaign,
    saved_count: usize,
    failed_count: Option<usize>,
}

impl CampaignSave {
    pub fn attach(&mut self, package: &Path) -> SharedCampaign {
        // Retry a transient save failure once at a real level boundary.
        self.failed_count = None;
        self.flush();
        if self.package.as_deref() == Some(package) {
            return self.progress.clone();
        }
        let identity =
            crate::engine::package::hash::sha256_hex(package.to_string_lossy().as_bytes());
        let save = crate::engine::paths::user()
            .map(|paths| paths.saves().join(format!("campaign-{identity}.json")));
        let progress = match save
            .as_ref()
            .map_err(ToString::to_string)
            .and_then(|path| read(path))
        {
            Ok(progress) => progress,
            Err(problem) => {
                crate::engine::logging::warn(format!("Campaign save unavailable: {problem}"));
                CampaignProgress::default()
            }
        };
        self.saved_count = progress.count();
        self.failed_count = None;
        self.package = Some(package.to_path_buf());
        self.save = save.ok();
        self.progress = std::sync::Arc::new(std::sync::Mutex::new(progress));
        self.progress.clone()
    }

    pub fn flush(&mut self) {
        let Ok(progress) = self.progress.lock() else {
            return;
        };
        let count = progress.count();
        if count == self.saved_count || self.failed_count == Some(count) {
            return;
        }
        let result = self
            .save
            .as_ref()
            .ok_or_else(|| "No writable campaign save path".to_string())
            .and_then(|path| write(path, &progress));
        match result {
            Ok(()) => {
                self.saved_count = count;
                self.failed_count = None;
            }
            Err(problem) => {
                self.failed_count = Some(count);
                crate::engine::logging::warn(format!(
                    "Campaign progress retained for this session; save failed: {problem}"
                ));
            }
        }
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
    let parent = path.parent().ok_or("Campaign save has no parent")?;
    std::fs::create_dir_all(parent).map_err(|problem| problem.to_string())?;
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec(progress).map_err(|problem| problem.to_string())?;
    std::fs::write(&temporary, bytes).map_err(|problem| problem.to_string())?;
    std::fs::rename(temporary, path).map_err(|problem| problem.to_string())
}
