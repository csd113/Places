//! Player visuals and a following camera, retaining player identity.
//!
//! Shared gameplay actions validate and commit a measured prop body alongside
//! these visuals. All transforms use measured model bounds;
//! models face +Z while the player's zero yaw faces -Z.

use std::f32::consts::PI;

use glam::{Quat, Vec3};

use crate::engine::render::{RenderCamera, Renderer, RuntimeModelInfo};
use crate::engine::scene::LocomotionSnapshot;

/// Portable characters explicitly admitted alongside the shared pickup list.
/// Structural props and trees never enter this opt-in list.
pub const CHARACTER_MODELS: &[&str] = &[
    "spooner-man",
    "mannequin",
    "rat",
    "skeleton",
    "carved-pumpkin",
    "sheet-ghost",
    "pumpkin-skeleton",
    "sheet-ghost-cat",
    "beach:seagull",
    "beach:crab",
    "beach:fish",
    "entity:cockroach",
];

/// Portable furniture admitted as forms without granting developer pickup access.
pub const PROP_FORM_MODELS: &[&str] = &["core:chair", "core:cabinet"];

/// Separate from the console's bounded `u64::MAX..u64::MAX-255` prop namespace.
const REPRESENTATION_KEY_A: u64 = u64::MAX - 1024;
const REPRESENTATION_KEY_B: u64 = u64::MAX - 1025;
const REPRESENTATION_CHARACTER_A: &str = "player:representation:a";
const REPRESENTATION_CHARACTER_B: &str = "player:representation:b";

/// Large eligible models are uniformly reduced; small objects retain metres.
const MAX_REPRESENTATION_EXTENT_M: f32 = 2.0;
const CAMERA_RADIUS_M: f32 = 0.12;
const CAMERA_MIN_DISTANCE_M: f32 = 0.65;
const CAMERA_MAX_PITCH_RAD: f32 = 1.309;

/// A complete visual placement, without any player or controller state.
#[derive(Clone, Debug)]
pub struct RepresentationPlacement {
    pub model: String,
    /// Authored, unscaled bind bounds; physics scales these exactly once.
    pub bounds: ([f32; 3], [f32; 3]),
    pub animated: bool,
    pub scale: f32,
    /// World-space model origin after centring and bottom anchoring.
    pub position: [f32; 3],
    pub orientation: [f32; 4],
    pub yaw_degrees: f32,
}

struct SelectedModel {
    id: String,
    info: RuntimeModelInfo,
    scale: f32,
    slot_b: bool,
}

impl SelectedModel {
    const fn key(&self) -> u64 {
        if self.slot_b {
            REPRESENTATION_KEY_B
        } else {
            REPRESENTATION_KEY_A
        }
    }

    const fn instance_id(&self) -> &'static str {
        if self.slot_b {
            REPRESENTATION_CHARACTER_B
        } else {
            REPRESENTATION_CHARACTER_A
        }
    }

    fn despawn(&self, renderer: &mut Renderer) {
        if self.info.animated {
            let _removed = renderer.despawn_runtime_character(self.instance_id());
        } else {
            let _removed = renderer.despawn_runtime_model(self.key());
        }
    }

    fn pose(&self, feet: Vec3, player_yaw: f32) -> (Vec3, Quat, f32) {
        let model_yaw = (PI - player_yaw).rem_euclid(2.0 * PI);
        let orientation = Quat::from_rotation_y(model_yaw);
        let [x0, y0, z0] = self.info.bounds.0;
        let [x1, _y1, z1] = self.info.bounds.1;
        let local_anchor = Vec3::new(
            f32::midpoint(x0, x1) * self.scale,
            y0 * self.scale,
            f32::midpoint(z0, z1) * self.scale,
        );
        let rotated_anchor = orientation.mul_vec3(local_anchor);
        let position = Vec3::new(
            feet.x - rotated_anchor.x,
            feet.y - rotated_anchor.y,
            feet.z - rotated_anchor.z,
        );
        (position, orientation, model_yaw)
    }

    fn placement(&self, feet: Vec3, player_yaw: f32) -> RepresentationPlacement {
        let (position, orientation, model_yaw) = self.pose(feet, player_yaw);
        RepresentationPlacement {
            model: self.id.clone(),
            bounds: self.info.bounds,
            animated: self.info.animated,
            scale: self.scale,
            position: position.to_array(),
            orientation: orientation.to_array(),
            yaw_degrees: model_yaw.to_degrees(),
        }
    }

    fn dimensions(&self) -> Vec3 {
        let [x0, y0, z0] = self.info.bounds.0;
        let [x1, y1, z1] = self.info.bounds.1;
        Vec3::new(
            (x1 - x0) * self.scale,
            (y1 - y0) * self.scale,
            (z1 - z0) * self.scale,
        )
    }
}

/// A selected visual only. The default presentation remains first person.
#[derive(Default)]
pub struct PlayerRepresentation {
    selected: Option<SelectedModel>,
    first_person: bool,
}

impl PlayerRepresentation {
    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.selected.is_some()
    }

    #[must_use]
    pub const fn is_third_person(&self) -> bool {
        self.selected.is_some() && !self.first_person
    }

    #[must_use]
    pub const fn toggle_view(&mut self) -> bool {
        self.first_person = !self.first_person;
        self.first_person
    }

    #[must_use]
    pub fn selected_id(&self) -> Option<&str> {
        self.selected.as_ref().map(|model| model.id.as_str())
    }

    /// Selects an eligible catalog model, preserving the old visual on failure.
    /// The successful result is its canonical catalog id.
    ///
    /// # Errors
    /// Reports unavailable/unsupported models, invalid poses and scene budgets.
    pub fn select(
        &mut self,
        renderer: &mut Renderer,
        id: &str,
        feet: Vec3,
        player_yaw: f32,
    ) -> Result<String, String> {
        self.select_with_commit(renderer, id, feet, player_yaw, 1.0, || Ok(()))
    }

    /// Allocates the complete new visual before committing gameplay effects.
    ///
    /// # Errors
    /// Returns selection/allocation errors or the commit's error. Failed
    /// commits remove the new visual while preserving the previous selection.
    pub fn select_with_commit(
        &mut self,
        renderer: &mut Renderer,
        id: &str,
        feet: Vec3,
        player_yaw: f32,
        requested_scale: f32,
        commit: impl FnOnce() -> Result<(), String>,
    ) -> Result<String, String> {
        if !requested_scale.is_finite() || !(0.1..=4.0).contains(&requested_scale) {
            return Err("Form scale must be finite within 0.1..=4.".into());
        }
        let canonical = canonical_model_id(id);
        if !is_eligible(canonical) {
            return Err(format!(
                "`{id}` is not an eligible portable model or character. Use /become list."
            ));
        }
        if !feet.is_finite() || !player_yaw.is_finite() {
            return Err("The player has no finite visual placement.".to_string());
        }
        let info = renderer.runtime_model_info(canonical)?;
        let [x0, y0, z0] = info.bounds.0;
        let [x1, y1, z1] = info.bounds.1;
        let largest_extent = (x1 - x0).max(y1 - y0).max(z1 - z0);
        let model = SelectedModel {
            id: canonical.to_string(),
            info,
            scale: (MAX_REPRESENTATION_EXTENT_M / largest_extent).min(1.0) * requested_scale,
            slot_b: self
                .selected
                .as_ref()
                .is_some_and(|current| !current.slot_b),
        };
        let placement = model.placement(feet, player_yaw);
        if info.animated {
            renderer.spawn_runtime_character(
                model.instance_id(),
                canonical,
                placement.position,
                placement.yaw_degrees,
                placement.scale,
            )?;
            let _locomotion_set = renderer.set_runtime_character_locomotion(
                model.instance_id(),
                LocomotionSnapshot::default(),
            );
            if !renderer.set_runtime_character_glow(
                model.instance_id(),
                super::pickup_catalog::runtime_glow(canonical),
            ) {
                model.despawn(renderer);
                return Err("The attached-light budget is full; the previous representation remains selected.".to_string());
            }
        } else if !renderer.spawn_runtime_model(
            model.key(),
            canonical,
            placement.position,
            placement.yaw_degrees,
            placement.scale,
        ) {
            return Err(format!("The renderer could not create `{canonical}`."));
        }
        if let Err(error) = commit() {
            model.despawn(renderer);
            return Err(error);
        }
        if let Some(previous) = self.selected.replace(model) {
            previous.despawn(renderer);
        }
        Ok(canonical.to_string())
    }

    /// Removes the selected visual and immediately restores first person.
    pub fn reset(&mut self, renderer: &mut Renderer) {
        self.first_person = false;
        if let Some(previous) = self.selected.take() {
            previous.despawn(renderer);
        }
    }

    /// Publishes the player's feet/yaw and simulation pose before renderer sync.
    /// Returns false when there is no selected model or its retained handle died.
    pub fn sync(
        &self,
        renderer: &mut Renderer,
        feet: Vec3,
        player_yaw: f32,
        locomotion: LocomotionSnapshot,
        delta_seconds: f32,
    ) -> bool {
        let Some(model) = self.selected.as_ref() else {
            return false;
        };
        if !feet.is_finite() || !player_yaw.is_finite() {
            return false;
        }
        let (position, orientation, _model_yaw) = model.pose(feet, player_yaw);
        if model.info.animated {
            let _visibility =
                renderer.set_runtime_character_visible(model.instance_id(), !self.first_person);
            let moved = renderer.set_runtime_character_pose(
                model.instance_id(),
                position.to_array(),
                orientation.to_array(),
            );
            let posed = renderer.set_runtime_character_locomotion(model.instance_id(), locomotion);
            renderer.set_runtime_character_frame_delta(model.instance_id(), delta_seconds)
                && posed
                && moved
        } else {
            let _visibility = renderer.set_runtime_model_visible(model.key(), !self.first_person);
            renderer.set_runtime_model_pose(
                model.key(),
                position.to_array(),
                orientation.to_array(),
            )
        }
    }

    /// Copies only the currently selected model's visual transform and bounds.
    #[must_use]
    pub fn placement(&self, feet: Vec3, player_yaw: f32) -> Option<RepresentationPlacement> {
        if !feet.is_finite() || !player_yaw.is_finite() {
            return None;
        }
        self.selected
            .as_ref()
            .map(|model| model.placement(feet, player_yaw))
    }

    /// Orbits behind the model and resolves a camera sphere against the world.
    ///
    /// `resolve` sweeps from the target to the desired eye with the supplied
    /// radius, returning the nearest safe eye. It should include live doors and
    /// physical items as well as the static world. Gameplay aiming stays at the
    /// unchanged player eye. A missing selection returns no camera override.
    #[must_use]
    pub fn camera(
        &self,
        feet: Vec3,
        yaw: f32,
        pitch: f32,
        fov_degrees: f32,
        eye_offset: f32,
        mut resolve: impl FnMut(Vec3, Vec3, f32) -> Vec3,
    ) -> Option<RenderCamera> {
        if self.first_person {
            return None;
        }
        let model = self.selected.as_ref()?;
        if !feet.is_finite() || !yaw.is_finite() || !pitch.is_finite() {
            return None;
        }
        let dimensions = model.dimensions();
        let height = dimensions.y;
        let target = Vec3::new(feet.x, feet.y + eye_offset, feet.z);
        let distance = (dimensions.length() * 1.5).max(CAMERA_MIN_DISTANCE_M);
        let bounded_pitch = pitch.clamp(-CAMERA_MAX_PITCH_RAD, CAMERA_MAX_PITCH_RAD);
        let forward = crate::engine::interact::view_direction(yaw, bounded_pitch);
        let desired = Vec3::new(
            forward.x.mul_add(-distance, target.x),
            forward.y.mul_add(-distance, target.y),
            forward.z.mul_add(-distance, target.z),
        );
        let resolved = resolve(
            target,
            desired,
            (height * 0.25).clamp(0.01, CAMERA_RADIUS_M),
        );
        let position = if resolved.is_finite() {
            resolved
        } else {
            target
        };
        let direction = Vec3::new(
            target.x - position.x,
            target.y - position.y,
            target.z - position.z,
        );
        let horizontal = direction.x.hypot(direction.z);
        let (camera_yaw, camera_pitch) = if direction.length_squared() > 1.0e-6 {
            (
                direction.x.atan2(-direction.z),
                direction.y.atan2(horizontal),
            )
        } else {
            (yaw, bounded_pitch)
        };
        Some(RenderCamera::new(
            position,
            camera_yaw,
            camera_pitch,
            fov_degrees,
        ))
    }
}

/// Friendly command names resolve to the existing catalog assets.
#[must_use]
pub fn canonical_model_id(id: &str) -> &str {
    match id {
        "party_cup" => "arcade:party_cup_confetti",
        "spooner_man" => "spooner-man",
        other => other,
    }
}

/// Shared, explicit eligibility for list/completion and actual selection.
#[must_use]
pub fn is_eligible(id: &str) -> bool {
    let canonical = canonical_model_id(id);
    super::pickup_catalog::find(canonical).is_some()
        || CHARACTER_MODELS.contains(&canonical)
        || PROP_FORM_MODELS.contains(&canonical)
}
