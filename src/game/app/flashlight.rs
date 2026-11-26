//! Player attachment for the renderer's shared local-light path.

use glam::Vec3;

use crate::engine::lighting::flashlight::camera_attached_light;
use crate::engine::render::Renderer;

#[derive(Default)]
pub enum Power {
    #[default]
    Off,
    On,
}

impl Power {
    pub const fn is_on(&self) -> bool {
        matches!(self, Self::On)
    }

    pub const fn toggle(&mut self) {
        *self = match self {
            Self::Off => Self::On,
            Self::On => Self::Off,
        };
    }
}

pub fn install(renderer: &mut Renderer, enabled: bool, eye: Vec3, yaw: f32, pitch: f32) {
    let light = enabled.then(|| {
        let forward = Vec3::new(
            yaw.sin() * pitch.cos(),
            pitch.sin(),
            -yaw.cos() * pitch.cos(),
        );
        let right = Vec3::new(yaw.cos(), 0.0, yaw.sin());
        camera_attached_light(eye, forward, right, |from, to| {
            renderer.light_transmittance(from.to_array(), to.to_array(), None)
        })
    });
    let _accepted = renderer.set_flashlight(light);
}

impl crate::game::app::FrameLoop<'_> {
    pub(super) const fn flashlight_is_lit(&self) -> bool {
        self.flashlight.is_on()
            && matches!(
                self.game.app_state(),
                crate::game::AppState::Playing
                    | crate::game::AppState::Paused
                    | crate::game::AppState::PauseSettings
            )
    }

    pub fn prepare_flashlight(
        &mut self,
        eye: Vec3,
        yaw: f32,
        pitch: f32,
    ) -> crate::engine::render::RenderCamera {
        let (source_eye, source_yaw, source_pitch) = if self.cinematic_controls_blocked() {
            (
                self.game.view_position(),
                self.game.player_yaw,
                self.game.player_pitch,
            )
        } else {
            (eye, yaw, pitch)
        };
        let lit = self.flashlight_is_lit();
        install(self.renderer, lit, source_eye, source_yaw, source_pitch);
        // Observe the exact prepared meshes/source used by this rendered frame.
        // AI consumes that observation on the following simulation tick.
        if self.game.app_state() == crate::game::AppState::Playing
            && !self.console.is_open()
            && self.load_intent.is_none()
        {
            let delta = self.game.world_delta_seconds();
            self.game
                .entities_mut()
                .update_light_observations(delta, source_eye, |id, point| {
                    self.renderer
                        .flashlight_illumination(point.to_array(), Some(id))
                });
        }
        crate::engine::render::RenderCamera::new(eye, yaw, pitch, self.settings.fov_degrees)
    }

    pub const fn scripted_view(&mut self, yaw: f32, pitch: f32) {
        self.game.player_yaw = yaw.to_radians();
        self.game.player_pitch = pitch.to_radians();
    }

    pub fn scripted_view_at(&mut self, position: [f32; 3]) {
        let eye = self.game.player_position;
        let direction = Vec3::new(
            position[0] - eye.x,
            position[1] - eye.y,
            position[2] - eye.z,
        )
        .normalize_or_zero();
        self.game.player_yaw = direction.x.atan2(-direction.z);
        self.game.player_pitch = direction
            .y
            .asin()
            .clamp(-crate::game::MAX_PITCH, crate::game::MAX_PITCH);
    }

    pub fn scripted_aim_entity(&mut self, id: &str, yaw_offset_degrees: f32) {
        let position = self
            .game
            .entity_frames()
            .iter()
            .find(|frame| frame.instance_id == id)
            .and_then(|frame| frame.transform)
            .map(|(base, _)| Vec3::new(base.x, base.y + 1.0, base.z));
        if let Some(target) = position {
            let eye = self.game.player_position;
            let direction =
                Vec3::new(target.x - eye.x, target.y - eye.y, target.z - eye.z).normalize_or_zero();
            // A camera-only action must preserve stance, recovery and AI
            // observations; the player-position action resets those states.
            self.game.player_yaw =
                direction.x.atan2(-direction.z) + yaw_offset_degrees.to_radians();
            self.game.player_pitch = direction
                .y
                .asin()
                .clamp(-crate::game::MAX_PITCH, crate::game::MAX_PITCH);
        } else {
            self.fatal_error = Some(format!("Native aim requested unknown actor {id}"));
            self.game.stop();
        }
    }
}
