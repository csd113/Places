//! Game policy around stable frame boundaries and the existing cancellable loader.
use super::{FrameLoop, LoadIntent, loading};
use crate::engine::loader::LevelEntry;
use crate::game::checkpoint::{self, Identity, Snapshot};

#[derive(Default)]
pub(super) struct Session {
    pub request: Option<String>,
    pub pending: Option<(String, f32)>,
    pub last_requested: Option<String>,
    pub resume: Option<(Identity, Snapshot)>,
    pub offered: Option<LevelEntry>,
}

impl FrameLoop<'_> {
    pub(super) fn offer_checkpoint(&mut self, entry: LevelEntry) {
        self.checkpoints.resume = None;
        match Identity::for_entry(&entry)
            .and_then(|optional| optional.map_or(Ok(None), |identity| checkpoint::read(&identity)))
        {
            Ok(Some(saved)) => {
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
                self.checkpoints.offered = Some(entry);
            }
            Ok(None) => self.request_load(loading::Source::entry(entry), LoadIntent::Play),
            Err(problem) => {
                self.ui_state.resume_offer = Some((problem, false));
                self.ui_state.resume_index = 1;
                self.checkpoints.offered = Some(entry);
            }
        }
        self.ui_state.pointer.invalidate();
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
        let Some(entry) = self.checkpoints.offered.take() else {
            return;
        };
        self.ui_state.resume_offer = None;
        self.checkpoints.resume = None;
        if index == 0
            && let Err(problem) = self.select_resume(&entry)
        {
            self.ui_state.set_status(problem, true);
            return;
        }
        self.request_load(loading::Source::entry(entry), LoadIntent::Play);
    }

    fn select_resume(&mut self, entry: &LevelEntry) -> Result<(), String> {
        let identity = Identity::for_entry(entry)?.ok_or("Saving is disabled for this level")?;
        let saved = checkpoint::read(&identity)?.ok_or("No checkpoint exists for this level")?;
        self.checkpoints.resume = Some((identity, saved.snapshot));
        Ok(())
    }

    pub(super) fn resume_current_checkpoint(&mut self) {
        let Some(entry) = self
            .current_level
            .as_ref()
            .map(|loaded| loaded.entry.clone())
        else {
            return;
        };
        match self.select_resume(&entry) {
            Ok(()) => self.request_load(loading::Source::entry(entry), LoadIntent::Play),
            Err(problem) => self.checkpoint_notice(&format!("Resume unavailable: {problem}")),
        }
    }

    pub(super) fn prepare_checkpoint_resume(
        &self,
        prepared: &mut loading::PreparedWorld,
    ) -> Result<(Option<Box<crate::game::Game>>, Option<Snapshot>), String> {
        let Some((identity, snapshot)) = self.checkpoints.resume.as_ref() else {
            return Ok((None, None));
        };
        if &Identity::for_level(&prepared.loaded)? != identity {
            return Err(
                "Checkpoint content changed during loading. Start new to play from the beginning."
                    .into(),
            );
        }
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
        if let Some(items) = prepared.physical.as_mut() {
            items
                .world
                .restore_checkpoint(&items.authored, &snapshot.items)?;
        } else if !snapshot.items.is_empty() {
            return Err("Checkpoint needs a fresh physical world".into());
        }
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
            Ok(()) => {
                self.checkpoint_notice(&format!("Checkpoint saved: {name}"));
                self.trace_world("checkpoint_saved");
            }
            Err(problem) => {
                self.checkpoint_notice(&format!("Checkpoint not saved: {problem}"));
                self.trace_world("checkpoint_refused");
            }
        }
    }

    fn capture_checkpoint(&self, name: &str) -> Result<(), String> {
        let loaded = self.current_level.as_ref().ok_or("No active level")?;
        let identity = Identity::for_level(loaded)?;
        let snapshot = Snapshot {
            player: self.game.checkpoint_player()?,
            world: self.game.entities().checkpoint()?,
            items: self.items.world.checkpoint(&self.items.authored)?,
            flashlight: self.flashlight.is_on(),
        };
        // Prove closure restoration before replacing the previous valid save.
        let mut witness = crate::engine::entities::EntityWorld::from_level(&loaded.level);
        witness.restore_checkpoint(&snapshot.world)?;
        checkpoint::write(&identity, name, &snapshot)
    }

    fn checkpoint_notice(&mut self, message: &str) {
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
