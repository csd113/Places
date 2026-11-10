//! A map-granted tool is inventory, never the carried physics-item owner.
use glam::Vec3;

use super::Game;
use crate::engine::{
    health::{HealthAction, HealthTarget},
    physics::ItemWorld,
    render::Renderer,
    spatial::Aabb,
    tools::ToolSettings,
};

#[derive(Default)]
pub(super) struct PlayerTools {
    serial: Option<(u64, u64)>,
    settings: ToolSettings,
    equipped: bool,
    progress: Option<f32>,
    contacted: bool,
    swings: u64,
    contacts: u64,
}

impl Game {
    /// The tool is a small camera-owned view. Carried props and cinematic views
    /// hide it without changing the map inventory or the world mannequin.
    pub fn tool_view_pose(
        &mut self,
        camera: crate::engine::render::RenderCamera,
        colour: super::player_colour::PlayerColour,
        allowed: bool,
        drawable: crate::engine::render::DrawableSize,
    ) -> Option<crate::engine::tools::ToolViewPose> {
        if !allowed || !self.tool_equipped() {
            return None;
        }
        let progress = self.tool_progress();
        let swing = if progress < 0.32 {
            (progress / 0.32 * std::f32::consts::FRAC_PI_2).sin()
        } else {
            ((1.0 - progress) / 0.68).clamp(0.0, 1.0).powi(2)
        };
        let forward = Vec3::new(
            camera.yaw.sin() * camera.pitch.cos(),
            camera.pitch.sin(),
            -camera.yaw.cos() * camera.pitch.cos(),
        );
        let right = forward.cross(camera.up).normalize_or(Vec3::X);
        let up = right.cross(forward).normalize_or(Vec3::Y);
        let backward = Vec3::new(-forward.x, -forward.y, -forward.z);
        let basis = glam::Quat::from_mat3(&glam::Mat3::from_cols(right, up, backward));
        let fov = crate::engine::render::vertical_fov_for_aspect(
            camera.vertical_fov_degrees,
            drawable.aspect_ratio(),
        );
        // Keep the cropped elbow below the drawable edge across FOV/aspect
        // changes; the grip remains just above that edge during the sweep.
        let lower_edge = -1.05 * (fov.to_radians() * 0.5).tan();
        let local = Vec3::new(
            (-0.25_f32).mul_add(swing, 0.5),
            0.08_f32.mul_add(swing, lower_edge - 0.015),
            0.1_f32.mul_add(swing, -1.05),
        );
        let offset = basis.mul_vec3(local);
        Some(crate::engine::tools::ToolViewPose {
            grip: Vec3::new(
                camera.position.x + offset.x,
                camera.position.y + offset.y,
                camera.position.z + offset.z,
            ),
            rotation: basis.mul_quat(glam::Quat::from_euler(
                glam::EulerRot::XYZ,
                -0.65 * swing,
                // Turn the forearm toward the eye so its cropped end stays below frame.
                0.65_f32.mul_add(swing, -0.85),
                (-0.35_f32).mul_add(swing, -0.12),
            )),
            eye: camera.position,
            arm_tint: colour.linear_rgb(),
        })
    }
    pub const fn cancel_tool_swing(&mut self) {
        // Cancel a pending hit, retaining the full configured recovery clock.
        self.tools.contacted = true;
    }
    fn reconcile_tools(&mut self) {
        let serial = (self.entities().generation(), self.reset_count());
        let settings = self.entities().tool_settings();
        if self.tools.serial != Some(serial) || self.tools.settings != settings {
            self.tools = PlayerTools {
                serial: Some(serial),
                settings,
                equipped: settings.crowbar,
                ..PlayerTools::default()
            };
        }
        if self.player_is_dead() {
            self.tools.progress = None;
        }
    }

    pub fn toggle_tool(&mut self) -> bool {
        self.reconcile_tools();
        if !self.tools.settings.crowbar || self.player_is_dead() {
            return false;
        }
        self.tools.equipped = !self.tools.equipped;
        if !self.tools.equipped {
            self.cancel_tool_swing();
        }
        true
    }

    pub fn tool_equipped(&mut self) -> bool {
        self.reconcile_tools();
        self.tools.equipped && !self.player_is_dead() && self.form_model().is_none()
    }

    pub fn tool_ready(&mut self) -> bool {
        self.tool_equipped() && self.tools.progress.is_none()
    }

    /// No buffered or automatic repeats: an input during recovery is consumed.
    pub fn start_tool_swing(&mut self) -> bool {
        if !self.tool_ready() {
            return false;
        }
        self.tools.progress = Some(0.0);
        self.tools.contacted = false;
        self.tools.swings = self.tools.swings.saturating_add(1);
        true
    }

    /// Returns one contact opportunity per accepted swing, at its strike phase.
    pub fn advance_tool_frame(&mut self) -> bool {
        self.advance_tool(self.world_delta_seconds())
    }

    /// Explicit authority-side advancement; native adapters use the shared
    /// clamped frame clock through `advance_tool_frame`.
    pub fn advance_tool(&mut self, seconds: f32) -> bool {
        self.reconcile_tools();
        let Some(progress) = self.tools.progress else {
            return false;
        };
        if !seconds.is_finite() || seconds <= 0.0 {
            return false;
        }
        let next = seconds.mul_add(self.tools.settings.swings_per_second, progress);
        let strike = !self.tools.contacted && next >= 0.32;
        self.tools.contacted |= strike;
        self.tools.progress = (next < 1.0).then_some(next);
        strike
    }

    #[must_use]
    pub fn tool_progress(&self) -> f32 {
        self.tools.progress.unwrap_or(0.0)
    }

    #[must_use]
    pub fn tool_snapshot(&self) -> serde_json::Value {
        serde_json::json!({"inventory":self.tools.settings.crowbar,"equipped":self.tools.equipped,
            "progress":self.tools.progress,"swings":self.tools.swings,"contacts":self.tools.contacts,
            "settings":self.tools.settings})
    }
}

/// Releasing a carried prop and starting the tool swing share one input edge.
/// A geometrically blocked release leaves the prop owned and refuses the swing.
/// # Errors
/// Reports a blocked safe release, without throwing or teleporting the prop.
pub fn begin_swing(game: &mut Game, world: &mut ItemWorld) -> Result<bool, String> {
    if !game.tool_ready() {
        return Ok(false);
    }
    if world.held().is_some() && !world.drop() {
        return Err("Crowbar blocked: move the carried prop clear so it can drop".into());
    }
    Ok(game.start_tool_swing())
}

/// Conservative current-pose hitboxes, with exact slab ray entry, never a cone
/// that could damage a second actor behind the first contact.
fn ray_entry(bounds: Aabb, origin: Vec3, direction: Vec3, limit: f32) -> Option<f32> {
    let mut near = 0.0_f32;
    let mut far = limit;
    for ((low, high), (eye, aim)) in bounds
        .min
        .into_iter()
        .zip(bounds.max)
        .zip(origin.to_array().into_iter().zip(direction.to_array()))
    {
        if !low.is_finite() || !high.is_finite() || low > high {
            return None;
        }
        if aim.abs() < 1.0e-6 {
            if eye < low || eye > high {
                return None;
            }
        } else {
            let first = (low - eye) / aim;
            let last = (high - eye) / aim;
            near = near.max(first.min(last));
            far = far.min(first.max(last));
            if near > far {
                return None;
            }
        }
    }
    (near <= limit && far >= 0.0).then_some(near)
}

/// Resolves only the nearest surface at the physical strike instant. Static
/// scenery, fixed items and living character bounds all stop the attack.
pub fn strike(
    game: &mut Game,
    world: &mut ItemWorld,
    renderer: &Renderer,
    item_identity: impl Fn(u64) -> Option<String>,
) -> String {
    let rules = game.tools.settings;
    let eye = game.view_position();
    let aim = game.view_direction().normalize_or(Vec3::NEG_Z);
    let end = point_along(eye, aim, rules.reach_m);
    let mut limit = game
        .camera_sweep(eye, end, 0.01)
        .distance(eye)
        .min(rules.reach_m);
    let mut item = world.first_body_hit(eye, aim, limit);
    if let Some((_, distance)) = item {
        limit = distance;
    }
    let mut character = None;
    for id in game.entities().tool_target_ids() {
        if !world.has_ragdoll(id)
            && let Some(bounds) = renderer.character_strike_bounds(id)
            && let Some(distance) = ray_entry(bounds, eye, aim, limit)
        {
            limit = distance;
            character = Some(id.to_string());
            item = None;
        }
    }
    if let Some(id) = world.strike_ragdoll(eye, aim, limit) {
        game.tools.contacts = game.tools.contacts.saturating_add(1);
        return format!("Crowbar hit ragdoll {id}");
    }
    let target = if let Some((key, _)) = item {
        let _impulse = world.strike_body(key, aim, point_along(eye, aim, limit));
        item_identity(key)
    } else {
        character
    };
    if item.is_some() || target.is_some() {
        game.tools.contacts = game.tools.contacts.saturating_add(1);
    }
    if let Some(id) = target {
        let message = format!("Crowbar hit {id}");
        if game.health_settings().enabled {
            let _damage = game.apply_health_action(
                &HealthTarget::Entity { id },
                HealthAction::Damage {
                    hp: rules.damage_hp,
                },
            );
        }
        return message;
    }
    item.map_or_else(
        || {
            if limit < rules.reach_m {
                "Crowbar hit scenery".into()
            } else {
                "Crowbar swing".into()
            }
        },
        |(key, _)| format!("Crowbar hit physics item {key}"),
    )
}

const fn point_along(origin: Vec3, aim: Vec3, distance: f32) -> Vec3 {
    Vec3::new(
        aim.x.mul_add(distance, origin.x),
        aim.y.mul_add(distance, origin.y),
        aim.z.mul_add(distance, origin.z),
    )
}

#[cfg(test)]
#[path = "tests/tools.rs"]
mod tests;
