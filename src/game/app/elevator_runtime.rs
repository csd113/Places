//! Native lift arrival uses the same private candidate-world admission as Resume.
use super::{FrameLoop, loading};
use crate::game::checkpoint::Snapshot;

impl FrameLoop<'_> {
    pub(super) fn sync_mechanisms(&mut self) {
        self.renderer.sync_doors(self.game.doors());
        self.renderer
            .sync_elevators(self.game.entities().elevators());
    }
    pub(super) fn prepare_elevator_arrival(
        &self,
        prepared: &mut loading::PreparedWorld,
    ) -> Result<(Option<Box<crate::game::Game>>, Option<Snapshot>), String> {
        let id = self
            .elevator_arrival
            .as_deref()
            .ok_or("Missing elevator arrival")?;
        let collision = std::mem::take(&mut prepared.collision);
        let mut candidate = crate::game::Game::new(
            crate::game::spawn_position(&prepared.loaded.level),
            prepared.loaded.level.spawn.yaw_degrees.to_radians(),
            collision,
        );
        candidate.arrive_elevator(id)?;
        Ok((Some(Box::new(candidate)), None))
    }
}
