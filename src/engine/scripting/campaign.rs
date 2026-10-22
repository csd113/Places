//! Bounded package-session completion, separate from disposable map VMs.
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

const MAX_OBJECTIVES: usize = 64;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CampaignProgress {
    completed: BTreeSet<String>,
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
        let _inserted = self.completed.insert(id.to_string());
        Ok(())
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

pub(super) fn commit(world: &crate::engine::entities::EntityWorld) -> Result<(), String> {
    world
        .campaign
        .lock()
        .map_err(|_problem| "Campaign state unavailable".to_string())
        .and_then(|mut progress| progress.complete(&world.campaign_level))
}
