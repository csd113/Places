//! Bounded package-session completion, separate from disposable map VMs.
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

const MAX_OBJECTIVES: usize = 64;

#[derive(Clone, Copy, Debug, Default)]
enum SaveStatus {
    #[default]
    Saved,
    Pending,
    Failed,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum CampaignChange {
    #[default]
    None,
    Complete,
    Reset,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignProgress {
    completed: BTreeSet<String>,
    #[serde(default)]
    generation: u64,
    #[serde(skip)]
    reset_requested: bool,
    #[serde(skip)]
    status: SaveStatus,
}

impl CampaignProgress {
    /// # Errors
    /// Refuses unbounded or malformed data before attaching a save to a world.
    pub fn validate(&self) -> Result<(), String> {
        if self.completed.len() > MAX_OBJECTIVES {
            return Err("Campaign supports at most 64 completed destinations".into());
        }
        for id in &self.completed {
            validate_id(id)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn completed(&self, id: &str) -> bool {
        self.completed.contains(id)
    }

    /// # Errors
    /// Rejects invalid identities and growth beyond the bounded destination set.
    pub fn complete(&mut self, id: &str) -> Result<(), String> {
        validate_id(id)?;
        if !self.completed.contains(id) && self.completed.len() >= MAX_OBJECTIVES {
            return Err("Campaign completion limit reached".into());
        }
        if self.completed.insert(id.to_string()) {
            self.status = SaveStatus::Pending;
        }
        Ok(())
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) const fn reset_pending(&self) -> bool {
        self.reset_requested
    }

    pub(crate) const fn status(&self) -> &'static str {
        match self.status {
            SaveStatus::Saved => "saved",
            SaveStatus::Pending => "pending",
            SaveStatus::Failed => "failed",
        }
    }

    pub(crate) const fn request_reset(&mut self) {
        self.reset_requested = true;
        self.status = SaveStatus::Pending;
    }

    pub(crate) fn empty_at(generation: u64) -> Self {
        Self {
            generation,
            ..Self::default()
        }
    }

    pub(crate) const fn mark_saved(&mut self) {
        self.status = SaveStatus::Saved;
    }

    pub(crate) const fn mark_failed(&mut self) {
        self.reset_requested = false;
        self.status = SaveStatus::Failed;
    }

    #[must_use]
    pub fn count(&self) -> usize {
        self.completed.len()
    }
}

pub(super) fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty() || id.len() > 96 || id.chars().any(char::is_control) {
        return Err("Campaign identity requires 1..=96 printable bytes".into());
    }
    Ok(())
}

pub type SharedCampaign = Arc<Mutex<CampaignProgress>>;

pub(super) fn commit(
    world: &crate::engine::entities::EntityWorld,
    change: CampaignChange,
) -> Result<(), String> {
    let mut progress = world
        .campaign
        .lock()
        .map_err(|_problem| "Campaign state unavailable".to_string())?;
    let committed = match change {
        CampaignChange::None => Ok(()),
        CampaignChange::Complete => progress.complete(&world.campaign_level),
        CampaignChange::Reset => {
            // The application writes the empty receipt before clearing live state.
            // A failed publication must leave the remembered favors intact.
            progress.request_reset();
            Ok(())
        }
    };
    drop(progress);
    committed
}
