//! Native lift arrival uses the same private candidate-world admission as Resume.
use super::{FrameLoop, loading};
use crate::game::checkpoint::Snapshot;

impl FrameLoop<'_> {
    pub(super) fn elevator_notice(&mut self, problem: &str) {
        self.ui_state.clear_status();
        self.subtitles.push("ELEVATOR".into(), &[problem.into()]);
        crate::engine::logging::warn(problem);
    }
    pub(super) fn sync_mechanisms(&mut self) {
        self.renderer.sync_doors(self.game.doors());
        self.renderer
            .sync_elevators(self.game.entities().elevators());
    }
    pub(super) fn prepare_elevator_arrival(
        &self,
        prepared: &mut loading::PreparedWorld,
    ) -> Result<(Option<Box<crate::game::Game>>, Option<Snapshot>), String> {
        let collision = std::mem::take(&mut prepared.collision);
        let mut candidate = crate::game::Game::new(
            crate::game::spawn_position(&prepared.loaded.level),
            prepared.loaded.level.spawn.yaw_degrees.to_radians(),
            collision,
        );
        prepared
            .cargo
            .restore_world(&mut candidate, &prepared.loaded.level.id)?;
        if let Some(arrival) = &self.elevator_arrival {
            candidate.arrive_elevator_from(arrival)?;
        }
        if let Some(items) = prepared.physical.as_mut() {
            items.world.sync_context(
                super::item_runtime::player_context(&candidate),
                candidate.door_colliders(),
            );
            prepared
                .cargo
                .restore_items(items, &prepared.loaded.level.id)?;
            if !items
                .world
                .player_pose_clear(super::item_runtime::player_context(&candidate))
            {
                return Err(
                    "Arrival is obstructed at the rider's exact position; departure retained"
                        .into(),
                );
            }
        }
        prepared.cargo.validate_arrivals(
            &candidate,
            prepared.physical.as_ref(),
            &prepared.loaded.level.id,
        )?;
        Ok((Some(Box::new(candidate)), None))
    }
}
