//! Game policy around stable frame boundaries and the existing cancellable loader.
use super::{FrameLoop, LoadIntent, loading};
use crate::engine::loader::LevelEntry;
use crate::engine::package::world::PackageRevision;
use crate::game::campaign::{Binding, CampaignSave, Installation};
use crate::game::checkpoint::{self, PackageGuard, Snapshot, Target, WriteOutcome};

#[derive(Default)]
pub(super) struct Session {
    pub request: Option<String>,
    pub pending: Option<(String, f32)>,
    pub last_requested: Option<String>,
    pub resume: Option<Resume>,
    pub live_revision: Option<PackageRevision>,
    pub(super) offered: Option<Offer>,
}

pub(super) struct Offer {
    entry: LevelEntry,
    target: Option<Target>,
}

pub(super) struct Resume {
    target: Target,
    pub snapshot: Snapshot,
    checksum: String,
    binding: Option<Binding>,
    pub deferred: Option<(u64, Box<loading::PreparedWorld>)>,
    pub package: PackageGuard,
}

impl Resume {
    pub(super) fn try_bind(&mut self, campaign: &CampaignSave) -> Result<bool, String> {
        self.target.verify_current()?;
        let generation = self.snapshot.campaign_generation;
        if let Some(binding) = &self.binding {
            campaign.validate_resume(binding, generation)?;
            return Ok(true);
        }
        let Some(binding) =
            campaign.prepare_visit(self.target.installation.clone(), Some(generation))?
        else {
            return Ok(false);
        };
        let path = self.target.identity.path_in(binding.directory()?);
        checkpoint::verify_selection(&path, &self.target.identity, &self.checksum)?;
        self.target.verify_current()?;
        self.binding = Some(binding);
        Ok(true)
    }

    pub(super) fn matches_source(&self, source: &loading::Source) -> bool {
        source.matches_checkpoint(
            &self.target.installation.canonical,
            &self.target.identity.level,
            &self.target.revision,
        )
    }
}

impl FrameLoop<'_> {
    pub(super) fn offer_checkpoint(&mut self, entry: LevelEntry) {
        self.checkpoints.resume = None;
        match Self::inspect_checkpoint_offer(&entry) {
            Ok(Some((target, saved))) => {
                self.ui_state.resume_offer = Some((
                    format!(
                        "{}{}",
                        saved.name,
                        if saved.recovered {
                            " (recovered previous save)"
                        } else {
                            ""
                        }
                    ),
                    true,
                ));
                self.ui_state.resume_index = 0;
                self.checkpoints.offered = Some(Offer {
                    entry,
                    target: Some(target),
                });
            }
            Ok(None) => self.request_load(loading::Source::entry(entry), LoadIntent::Play),
            Err(problem) => {
                self.ui_state.resume_offer = Some((problem, false));
                self.ui_state.resume_index = 1;
                self.checkpoints.offered = Some(Offer {
                    entry,
                    target: None,
                });
            }
        }
        self.ui_state.pointer.invalidate();
    }

    fn inspect_checkpoint_offer(
        entry: &LevelEntry,
    ) -> Result<Option<(Target, checkpoint::Saved)>, String> {
        let Some(target) = Target::for_entry(entry)? else {
            return Ok(None);
        };
        let Some(saved) = checkpoint::read(&target.identity)? else {
            return Ok(None);
        };
        Ok(Some((target, saved)))
    }

    pub(super) fn activate_checkpoint_offer(&mut self) {
        let index = self.ui_state.resume_index;
        if index == 0
            && self
                .ui_state
                .resume_offer
                .as_ref()
                .is_none_or(|(_, available)| !*available)
        {
            return;
        }
        if index == 2 {
            self.ui_state.resume_offer = None;
            self.checkpoints.offered = None;
            return;
        }
        let Some(Offer {
            entry,
            target: offered_target,
        }) = self.checkpoints.offered.take()
        else {
            return;
        };
        self.ui_state.resume_offer = None;
        self.checkpoints.resume = None;
        if index == 0 {
            let result = offered_target
                .ok_or_else(|| "No verified checkpoint offer".to_string())
                .and_then(|target| self.select_resume(entry, target));
            match result {
                Ok(source) => self.request_load(source, LoadIntent::Play),
                Err(problem) => self.ui_state.set_status(problem, true),
            }
        } else {
            self.request_load(loading::Source::entry(entry), LoadIntent::Play);
        }
    }

    fn select_resume(
        &mut self,
        mut entry: LevelEntry,
        target: Target,
    ) -> Result<loading::Source, String> {
        let package = target.hold_package()?;
        let saved =
            checkpoint::read(&target.identity)?.ok_or("No checkpoint exists for this level")?;
        entry.path.clone_from(&target.installation.canonical);
        let source = loading::Source::checkpoint(entry, target.revision.clone());
        let mut resume = Resume {
            target,
            snapshot: saved.snapshot,
            checksum: saved.checksum,
            binding: None,
            deferred: None,
            package,
        };
        let _ready = resume.try_bind(&self.campaign)?;
        self.checkpoints.resume = Some(resume);
        Ok(source)
    }

    pub(super) fn resume_current_checkpoint(&mut self) {
        let Some(entry) = self
            .current_level
            .as_ref()
            .map(|loaded| loaded.entry.clone())
        else {
            return;
        };
        let result = Target::for_entry(&entry).and_then(|selection| {
            let target = selection.ok_or("Saving is disabled for this level")?;
            if !self.campaign.owns_current(&target.installation) {
                return Err(
                    "The current level's installation changed; select it again before resuming"
                        .into(),
                );
            }
            self.select_resume(entry, target)
        });
        match result {
            Ok(source) => self.request_load(source, LoadIntent::Play),
            Err(problem) => self.checkpoint_notice(&format!("Resume unavailable: {problem}")),
        }
    }

    pub(super) fn prepare_checkpoint_resume(
        &self,
        prepared: &mut loading::PreparedWorld,
    ) -> Result<(Option<Box<crate::game::Game>>, Option<Snapshot>), String> {
        if self.elevator_arrival.is_some()
            || prepared.cargo.has_host(&prepared.loaded.level.id)
                && self.checkpoints.resume.is_none()
        {
            return self.prepare_elevator_arrival(prepared);
        }
        let Some(resume) = self.checkpoints.resume.as_ref() else {
            return Ok((None, None));
        };
        let canonical = prepared
            .canonical
            .as_deref()
            .ok_or("Checkpoint needs a verified installation")?;
        resume
            .target
            .check_loaded(&prepared.loaded, canonical, &prepared.revision)?;
        prepared.loaded.entry.path = resume.target.installation.original().to_path_buf();
        let snapshot = &resume.snapshot;
        let collision = std::mem::take(&mut prepared.collision);
        let mut candidate = crate::game::Game::new(
            crate::game::spawn_position(&prepared.loaded.level),
            prepared.loaded.level.spawn.yaw_degrees.to_radians(),
            collision,
        );
        candidate
            .entities_mut()
            .restore_checkpoint(&snapshot.world)?;
        candidate.restore_checkpoint_player(&snapshot.player)?;
        let eye = candidate.player_position;
        candidate.entities_mut().restore_checkpoint_light_holds(eye);
        if let Some(items) = prepared.physical.as_mut() {
            items.world.restore_player_checkpoint(
                &items.authored,
                &snapshot.items,
                super::item_runtime::player_context(&candidate),
            )?;
            candidate.entities_mut().sync_item_obstacles(&items.world);
        } else if !snapshot.items.is_empty() {
            return Err("Checkpoint needs a fresh physical world".into());
        }
        candidate.validate_restored_guests()?;
        Ok((Some(Box::new(candidate)), Some(snapshot.clone())))
    }

    pub(super) fn finish_checkpoint(&mut self) {
        let requested = self.checkpoints.request.take();
        if requested != self.checkpoints.last_requested {
            self.checkpoints.last_requested.clone_from(&requested);
            if let Some(name) = requested {
                self.checkpoints.pending = Some((name, 0.0));
            }
        }
        if !self.gameplay_active() {
            self.checkpoints.pending = None;
            self.checkpoints.last_requested = None;
            return;
        }
        let Some((name, age)) = self.checkpoints.pending.take() else {
            return;
        };
        if !self.game.entities().checkpoint_events_settled() && age < 2.0 {
            self.checkpoints.pending = Some((name, age + self.game.sim_delta_seconds()));
            return;
        }
        match self.capture_checkpoint(&name) {
            Ok(WriteOutcome::Written) => {
                self.checkpoint_notice(&format!("Checkpoint saved: {name}"));
                self.trace_world("checkpoint_saved");
            }
            Ok(WriteOutcome::WrittenUnconfirmed(problem)) => {
                self.checkpoint_notice(&format!(
                    "Checkpoint written, durability not confirmed: {problem}"
                ));
                self.trace_world("checkpoint_written_unconfirmed");
            }
            Err(problem) => {
                self.checkpoint_notice(&format!("Checkpoint not saved: {problem}"));
                self.trace_world("checkpoint_refused");
            }
        }
    }

    fn capture_checkpoint(&self, name: &str) -> Result<WriteOutcome, String> {
        let loaded = self.current_level.as_ref().ok_or("No active level")?;
        let revision = self
            .checkpoints
            .live_revision
            .as_ref()
            .ok_or("No verified live package revision")?;
        let (receipt, generation) = self.campaign.lock_checkpoint()?;
        let target = Target::for_level(loaded, receipt.installation(), revision)?;
        let _package = target.hold_package()?;
        let mut cargo = self.elevator_cargo.clone();
        cargo.refresh(&loaded.level, self.game, &self.items)?;
        let snapshot = Snapshot {
            elevator_cargo: cargo,
            campaign_generation: generation,
            player: self.game.checkpoint_player()?,
            world: self.game.entities().checkpoint()?,
            items: self.items.world.checkpoint(&self.items.authored)?,
            flashlight: self.flashlight.is_on(),
        };
        // Prove closure restoration before replacing the previous valid save.
        let mut witness = crate::engine::entities::EntityWorld::from_level(&loaded.level);
        witness.restore_checkpoint(&snapshot.world)?;
        target.verify_current()?;
        checkpoint::write(&receipt, &target.identity, name, &snapshot, || {
            target.verify_current()
        })
    }

    /// Before upload, establish and retain the authoritative receipt. CPU
    /// preparation is provisional while another process owns the sidecar.
    pub(super) fn prepare_campaign_upload(&mut self) -> Result<bool, String> {
        let Some(pending) = self.pending_commit.as_mut() else {
            return Ok(false);
        };
        if matches!(pending.intent, LoadIntent::Graphics) {
            return Ok(true);
        }
        let canonical = pending
            .canonical
            .as_deref()
            .ok_or("Level has no verified installation")?;
        if let Some(snapshot) = &pending.checkpoint {
            let resume = self
                .checkpoints
                .resume
                .as_ref()
                .ok_or("Checkpoint selection was superseded")?;
            let binding = pending
                .campaign
                .as_ref()
                .ok_or("Checkpoint receipt was superseded")?;
            resume
                .target
                .check_loaded(&pending.loaded, canonical, &pending.revision)?;
            self.campaign
                .validate_resume(binding, snapshot.campaign_generation)?;
            return Ok(true);
        }
        if pending.campaign.is_none() {
            let installation = Installation::new(
                pending.loaded.entry.path.clone(),
                canonical.to_path_buf(),
                pending.loaded.project_id.as_deref(),
                &pending.loaded.level.id,
            );
            installation.verify_path()?;
            pending.campaign = self.campaign.prepare_visit(installation, None)?;
        }
        if let Some(binding) = pending.campaign.as_ref() {
            binding.installation().verify_path()?;
            return Ok(true);
        }
        Ok(false)
    }

    pub(super) fn take_resume_binding(&mut self) -> Option<Binding> {
        self.checkpoints.resume.as_mut()?.binding.take()
    }

    pub(super) fn checkpoint_notice(&mut self, message: &str) {
        crate::engine::logging::info(message);
        self.subtitles.push("CHECKPOINT".into(), &[message.into()]);
    }
}

impl FrameLoop<'_> {
    pub(super) fn prepare_resume_or_report(
        &mut self,
        id: u64,
        prepared: &mut loading::PreparedWorld,
    ) -> Option<(Option<Box<crate::game::Game>>, Option<Snapshot>)> {
        match self.prepare_checkpoint_resume(prepared) {
            Ok(resumption) => Some(resumption),
            Err(error) => {
                self.recover_from_preparation_failure(id, &error);
                None
            }
        }
    }
}
