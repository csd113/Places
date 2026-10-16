//! Player attachment for the renderer's shared local-light path.

use glam::Vec3;

use crate::engine::render::{Flashlight, Renderer};

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

/// A restrained hand/chest attachment, aimed at view centre six metres away.
/// The visibility callback keeps the source on the camera side of nearby
/// geometry, including live doors and movable furniture.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite camera inputs use bounded metre offsets and glam floating-point vector arithmetic."
)]
fn attached_light(
    eye: Vec3,
    yaw: f32,
    pitch: f32,
    mut visibility: impl FnMut(Vec3, Vec3) -> f32,
) -> Flashlight {
    let forward = Vec3::new(
        yaw.sin() * pitch.cos(),
        pitch.sin(),
        -yaw.cos() * pitch.cos(),
    );
    let right = Vec3::new(yaw.cos(), 0.0, yaw.sin());
    let up = right.cross(forward);
    let offset = right * 0.14 - up * 0.18 + forward * 0.06;
    let candidate = eye + offset;
    let position = if visibility(eye, candidate) >= 0.99 {
        candidate
    } else {
        let mut clear = 0.0_f32;
        let mut blocked = 1.0_f32;
        // Eight bounded queries resolve this 24 cm offset below a millimetre.
        for _ in 0_u8..8 {
            let fraction = clear.midpoint(blocked);
            if visibility(eye, eye + offset * fraction) >= 0.99 {
                clear = fraction;
            } else {
                blocked = fraction;
            }
        }
        eye + offset * (clear - 0.02 / offset.length()).max(0.0)
    };
    Flashlight {
        position,
        direction: (eye + forward * 6.0 - position).normalize_or(forward),
        color: [1.0, 0.965, 0.89],
        intensity: 12.0,
        range: 24.0,
        // Preserve the soft/core relationship while widening the projected
        // diameter by the requested 50%, followed by another 15%, rather
        // than scaling angles approximately.
        inner_cos: (6.0_f32.to_radians().tan() * 1.725).atan().cos(),
        outer_cos: (12.0_f32.to_radians().tan() * 1.725).atan().cos(),
    }
}

fn install(renderer: &mut Renderer, enabled: bool, eye: Vec3, yaw: f32, pitch: f32) {
    let light = enabled.then(|| {
        attached_light(eye, yaw, pitch, |from, to| {
            renderer.light_transmittance(from.to_array(), to.to_array(), None)
        })
    });
    let _accepted = renderer.set_flashlight(light);
}

impl crate::game::app::FrameLoop<'_> {
    pub fn prepare_flashlight(
        &mut self,
        eye: Vec3,
        yaw: f32,
        pitch: f32,
    ) -> crate::engine::render::RenderCamera {
        let in_world = matches!(
            self.game.app_state(),
            crate::game::AppState::Playing
                | crate::game::AppState::Paused
                | crate::game::AppState::PauseSettings
        );
        install(
            self.renderer,
            self.flashlight.is_on() && in_world,
            eye,
            yaw,
            pitch,
        );
        // Observe the exact prepared meshes/source used by this rendered frame.
        // AI consumes that observation on the following simulation tick.
        if self.game.app_state() == crate::game::AppState::Playing
            && !self.console.is_open()
            && self.load_intent.is_none()
        {
            let delta = self.game.sim_delta_seconds();
            self.game
                .entities_mut()
                .update_light_observations(delta, eye, |id, point| {
                    self.renderer
                        .flashlight_illumination(point.to_array(), Some(id))
                });
        }
        crate::engine::render::RenderCamera::new(eye, yaw, pitch, self.settings.fov_degrees)
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

#[cfg(test)]
mod tests {
    use super::*;

    // One tiny numeric fixture protects attachment orientation, convergence and
    // wall retraction without preparing a level, renderer or texture resource.
    #[test]
    fn attachment_converges_and_retracts_before_geometry() {
        let eye = Vec3::new(0.0, 1.6, 0.0);
        let light = attached_light(eye, 0.0, 0.0, |_, _| 1.0);
        assert!(
            (light.outer_cos.acos().tan() / 12.0_f32.to_radians().tan() - 1.725).abs() < 0.0001,
            "the requested beam diameter grows by 50%, then 15%, at fixed distance"
        );
        assert!(light.position.x > 0.0 && light.position.y < eye.y);
        assert!(
            (light.position + light.direction * 6.0 - Vec3::new(0.0, 1.6, -6.0)).length() < 0.07
        );
        let retracted = attached_light(eye, 0.0, 0.0, |_, to| if to.x < 0.07 { 1.0 } else { 0.0 });
        assert!(retracted.position.x < 0.07);
        assert!(retracted.position.distance(eye) < light.position.distance(eye));
        assert!(retracted.direction.is_normalized());
    }
}
