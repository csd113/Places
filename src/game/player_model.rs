//! The local player's world mannequin. Controller, appearance and physical death
//! are separate owners; no tool meshes or multiplayer camera policy live here.
use glam::{Mat4, Quat, Vec3};

use super::{AppState, Game, player_colour::PlayerColour};
use crate::engine::{
    physics::ItemWorld,
    render::{RenderCamera, Renderer},
};

/// Reserved local visual identity; future held-tool rendering can use its sockets.
pub const PLAYER_MODEL_ID: &str = "player:avatar:local";

#[derive(Clone, Copy)]
struct DeathView {
    eye: Vec3,
    forward: Vec3,
    up: Vec3,
}
impl DeathView {
    fn capture(head: Mat4, eye: Vec3, forward: Vec3) -> Result<Self, String> {
        if !head.is_finite() || head.determinant().abs() < 1.0e-8 {
            return Err("The player head has no valid physical camera pose".into());
        }
        let inverse = head.inverse();
        Ok(Self {
            eye: inverse.transform_point3(eye),
            forward: inverse.transform_vector3(forward),
            up: inverse.transform_vector3(Vec3::Y),
        })
    }
    fn camera(self, head: Mat4, fov: f32) -> RenderCamera {
        RenderCamera::from_basis(
            head.transform_point3(self.eye),
            head.transform_vector3(self.forward),
            head.transform_vector3(self.up),
            fov,
        )
    }
}

#[derive(Default)]
enum Life {
    #[default]
    Alive,
    Ragdoll(DeathView),
    /// A refused handoff is diagnosed once; it never silently revives health.
    Unavailable,
}

#[derive(Default)]
pub struct PlayerModel {
    serial: Option<(u64, u64)>,
    spawned: bool,
    scale: f32,
    anchor: Vec3,
    colour: Option<PlayerColour>,
    life: Life,
    head: Option<Mat4>,
    camera: Option<RenderCamera>,
}

impl PlayerModel {
    pub fn reset(&mut self, renderer: &mut Renderer, world: &mut ItemWorld) {
        if self.spawned {
            renderer.cancel_ragdoll(PLAYER_MODEL_ID, world);
            let _removed = renderer.despawn_runtime_character(PLAYER_MODEL_ID);
        }
        *self = Self::default();
    }

    fn spawn(&mut self, game: &Game, renderer: &mut Renderer) -> Result<(), String> {
        let info = renderer.runtime_model_info("mannequin")?;
        let [x0, y0, z0] = info.bounds.0;
        let [x1, y1, z1] = info.bounds.1;
        let height = y1 - y0;
        if !height.is_finite() || height <= 0.0 {
            return Err("The player mannequin has invalid measured bounds".into());
        }
        self.scale = crate::engine::collision::PLAYER_HEIGHT / height;
        if !(0.25..=3.0).contains(&self.scale) {
            return Err("The player mannequin is outside the supported humanoid scale".into());
        }
        self.anchor = Vec3::new(
            f32::midpoint(x0, x1) * self.scale,
            y0 * self.scale,
            f32::midpoint(z0, z1) * self.scale,
        );
        let (position, yaw) = self.pose(game);
        renderer.spawn_runtime_character(
            PLAYER_MODEL_ID,
            "mannequin",
            position.to_array(),
            yaw.to_degrees(),
            self.scale,
        )?;
        self.spawned = true;
        self.colour = None;
        Ok(())
    }

    fn pose(&self, game: &Game) -> (Vec3, f32) {
        let yaw = std::f32::consts::PI - game.player_yaw;
        let anchor = Quat::from_rotation_y(yaw).mul_vec3(self.anchor);
        (
            Vec3::new(
                game.player_position.x - anchor.x,
                game.feet_y() - anchor.y,
                game.player_position.z - anchor.z,
            ),
            yaw,
        )
    }

    /// Runs before the shared solver. Normal movement retains its own body;
    /// death removes that proxy and transfers velocity exactly once.
    /// # Errors
    /// Reports missing assets, invalid rigs or exhausted shared scene/physics budgets.
    pub fn sync(
        &mut self,
        game: &Game,
        colour: PlayerColour,
        delta: f32,
        renderer: &mut Renderer,
        world: &mut ItemWorld,
    ) -> Result<(), String> {
        let serial = (game.entities().generation(), game.reset_count());
        if self.serial != Some(serial) {
            self.reset(renderer, world);
            self.serial = Some(serial);
        }
        if !matches!(
            game.app_state(),
            AppState::Playing | AppState::Paused | AppState::PauseSettings
        ) || game.form_model().is_some()
        {
            self.reset(renderer, world);
            return Ok(());
        }
        if !game.player_is_dead() && !matches!(self.life, Life::Alive) {
            self.reset(renderer, world);
            self.serial = Some(serial);
        }
        if !self.spawned {
            self.spawn(game, renderer)?;
        }
        if self.colour != Some(colour) {
            if !renderer.set_runtime_character_tint(PLAYER_MODEL_ID, colour.linear_rgb()) {
                return Err("The player mannequin colour could not be applied".into());
            }
            self.colour = Some(colour);
        }
        if matches!(self.life, Life::Alive) {
            let (position, yaw) = self.pose(game);
            let _moved = renderer.set_runtime_character_pose(
                PLAYER_MODEL_ID,
                position.to_array(),
                Quat::from_rotation_y(yaw).to_array(),
            );
            let _posed = renderer
                .set_runtime_character_locomotion(PLAYER_MODEL_ID, game.locomotion_snapshot());
            let _clock = renderer.set_runtime_character_frame_delta(PLAYER_MODEL_ID, delta);
            if game.player_is_dead() {
                self.life = Life::Unavailable;
                let head = renderer
                    .character_socket_transform(PLAYER_MODEL_ID, "head")
                    .ok_or("The player mannequin has no head socket")?;
                let view = DeathView::capture(head, game.view_position(), game.view_direction())?;
                renderer.start_ragdoll_from_body(PLAYER_MODEL_ID, world, game.player_velocity())?;
                let _released = world.release_held_for_reset();
                self.life = Life::Ragdoll(view);
            }
        }
        Ok(())
    }

    /// Resolves the actual current head after bone physics/animation publication.
    /// First-person body hiding also applies during death; other camera owners
    /// may explicitly present this world model without moving the controller.
    pub fn refresh(&mut self, game: &Game, renderer: &mut Renderer, fov: f32, external_view: bool) {
        self.head = renderer.character_socket_transform(PLAYER_MODEL_ID, "head");
        self.camera = match (&self.life, self.head) {
            (Life::Ragdoll(view), Some(head)) => {
                let mut camera = view.camera(head, fov);
                camera.position = game.camera_sweep(head.w_axis.truncate(), camera.position, 0.06);
                Some(camera)
            }
            (Life::Alive | Life::Unavailable, _) | (Life::Ragdoll(_), None) => None,
        };
        let _visibility = renderer.set_runtime_character_visible(PLAYER_MODEL_ID, external_view);
    }

    #[must_use]
    pub const fn camera(&self) -> Option<RenderCamera> {
        self.camera
    }

    #[must_use]
    pub fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({"id":PLAYER_MODEL_ID,"model":"mannequin","spawned":self.spawned,"colour":self.colour,"ragdoll":matches!(self.life,Life::Ragdoll(_)),"head":self.head,"camera":self.camera.map(|camera|serde_json::json!({"position":camera.position,"yaw":camera.yaw,"pitch":camera.pitch,"up":camera.up}))})
    }
}

#[cfg(test)]
#[path = "tests/player_model.rs"]
mod tests;
