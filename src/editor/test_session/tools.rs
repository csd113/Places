//! Embedded testing shares inventory, swing timing and contact resolution.
use super::Session;
use crate::engine::render::Renderer;
use sdl3::{event::Event, mouse::MouseButton};

impl Session {
    pub(super) fn dispatch_mouse_actions(&mut self) {
        self.dispatch_tool_mouse();
        if std::mem::take(&mut self.throw_requested)
            && !self.gameplay_controls_blocked()
            && self.physics.world.held().is_some()
        {
            self.status = if self.physics.world.throw(self.game.view_direction()) {
                "Threw held item".into()
            } else {
                "Throw blocked: move the item clear".into()
            };
        }
    }
    pub(super) fn publish_tool_view(&mut self, renderer: &mut Renderer) -> Result<(), String> {
        let allowed = !self.gameplay_controls_blocked() && self.physics.world.held().is_none();
        let camera = self.camera();
        let pose = self.game.tool_view_pose(
            camera,
            self.settings.player_colour,
            allowed,
            renderer.drawable_size(),
        );
        renderer.set_tool_view(pose)
    }
    pub(super) fn observe_tool_event(&mut self, event: &Event) -> bool {
        if matches!(
            event,
            Event::MouseButtonUp {
                mouse_btn: MouseButton::Left,
                timestamp: _,
                window_id: _,
                which: _,
                clicks: _,
                x: _,
                y: _
            }
        ) {
            self.attack_down = false;
        }
        if !self.gameplay_controls_blocked()
            && crate::game::input::pressed_key(event).is_some_and(|key| key.name() == "1")
        {
            return self.game.toggle_tool();
        }
        false
    }

    pub(super) fn dispatch_tool_mouse(&mut self) {
        if self.gameplay_controls_blocked() || !self.game.tool_equipped() {
            return;
        }
        if std::mem::take(&mut self.throw_requested) {
            match crate::game::tools::begin_swing(&mut self.game, &mut self.physics.world) {
                Ok(true) => self.status = "Crowbar swing".into(),
                Ok(false) => {}
                Err(problem) => self.status = problem,
            }
        }
    }

    pub(super) fn advance_tool_strike(&mut self, renderer: &Renderer) {
        if self.gameplay_controls_blocked() {
            self.game.cancel_tool_swing();
            return;
        }
        if !self.game.advance_tool_frame() {
            return;
        }
        let authored = &self.physics.authored;
        self.status =
            crate::game::tools::strike(&mut self.game, &mut self.physics.world, renderer, |key| {
                authored
                    .iter()
                    .find(|(body, _, _)| *body == key)
                    .map(|(_, id, _)| id.clone())
            });
    }
}
