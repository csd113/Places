//! Native input and physics adapters over the shared map tool state.
use super::{AppState, FrameLoop};

impl FrameLoop<'_> {
    pub(super) fn publish_tool_view(&mut self, camera: crate::engine::render::RenderCamera) {
        let allowed = self.game.app_state() == AppState::Playing
            && !self.cinematic_controls_blocked()
            && self.bench.camera_position_override().is_none()
            && self.items.world.held().is_none()
            && !self.console.is_open();
        let pose = self.game.tool_view_pose(
            camera,
            self.settings.player_colour,
            allowed,
            self.renderer.drawable_size(),
        );
        if let Err(problem) = self.renderer.set_tool_view(pose) {
            crate::engine::logging::warn_once("tool-view", format!("Tool view: {problem}"));
        }
    }
    pub(super) fn handle_tool_key(&mut self, event: &sdl3::event::Event) -> bool {
        if crate::game::input::pressed_key(event).is_some_and(|key| key.name() == "1")
            && self.window_focused
            && !self.console.is_open()
        {
            return self.game.toggle_tool();
        }
        false
    }

    pub(super) fn dispatch_tool_click(&mut self, allowed: bool) {
        if !self.game.tool_equipped() || !self.items.input.take_tool_click(allowed) {
            return;
        }
        self.synchronize_physical_player();
        match crate::game::tools::begin_swing(self.game, &mut self.items.world) {
            Ok(true) => self.trace_world("tool_swing"),
            Ok(false) => {}
            Err(problem) => {
                self.console.write(problem);
                self.trace_world("tool_drop_blocked");
            }
        }
    }

    pub(super) fn advance_tool_strike(&mut self) {
        if self.game.app_state() != AppState::Playing
            || !self.window_focused
            || self.gameplay_controls_blocked()
            || self.console.is_open()
            || self.load_intent.is_some()
        {
            self.game.cancel_tool_swing();
            return;
        }
        if !self.game.advance_tool_frame() {
            return;
        }
        let authored = &self.items.authored;
        let runtime = &self.console_runtime;
        let result =
            crate::game::tools::strike(self.game, &mut self.items.world, self.renderer, |key| {
                authored
                    .iter()
                    .find(|(item, _, _)| *item == key)
                    .map(|(_, id, _)| id.clone())
                    .or_else(|| runtime.given_identity(key).map(|(id, _)| id.to_string()))
            });
        self.trace
            .record("tool_contact", self.load_generation, &result);
    }
}
