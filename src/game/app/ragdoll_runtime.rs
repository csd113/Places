//! Native transaction boundary for skeletal, carry and controller ownership.
use super::FrameLoop;
use glam::Vec3;

impl FrameLoop<'_> {
    pub(super) fn apply_ragdoll(&mut self, id: &str, enabled: bool, velocity: Vec3) {
        if enabled {
            if let Err(problem) = self
                .renderer
                .start_ragdoll(id, &mut self.items.world, velocity)
            {
                self.ui_state.developer_diagnostic(
                    format!("Ragdoll refused for '{id}': {problem}"),
                    self.settings.developer_console,
                );
                self.trace_world("ragdoll_refused");
                return;
            }
            // A carried resource is returned to the ordinary solver at its current
            // socket before the skeleton moves. A blocked release rolls back physics.
            self.release_carried_item_now(id);
            if self.items.carry.has_attachment(id) {
                self.renderer.cancel_ragdoll(id, &mut self.items.world);
                self.ui_state.developer_diagnostic(
                    "Ragdoll refused: carried item could not be released safely".to_string(),
                    self.settings.developer_console,
                );
                self.trace_world("ragdoll_carry_refused");
                return;
            }
            let _visible = self.renderer.set_character_visible(id, true);
            self.game.entities_mut().set_ragdoll_owned(id, true);
            crate::engine::logging::info(format!("[ragdoll] '{id}' active: 15 bodies / 14 joints"));
            self.trace_world("ragdoll_started");
        } else if self.renderer.reset_ragdoll(id, &mut self.items.world) {
            self.game.entities_mut().set_ragdoll_owned(id, false);
            crate::engine::logging::info(format!("[ragdoll] '{id}' reset to authored behavior"));
            self.trace_world("ragdoll_reset");
        } else {
            self.ui_state.developer_diagnostic(
                format!("'{id}' has no active ragdoll"),
                self.settings.developer_console,
            );
        }
    }
}
