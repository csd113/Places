//! Camera-local submission of bounded snow inside the two fixed Christmas globes.
use glam::{Mat4, Vec3};

use super::effects::{EffectPose, EffectVertex, append_billboard};
use crate::engine::level::{LevelDef, LevelSurfaces};

const COUNT: usize = 20;
const MAX_NEAR: usize = 8;
pub(super) const BUDGET: usize = MAX_NEAR.saturating_mul(COUNT);
const CENTER: Vec3 = Vec3::new(0.0, 0.132, -0.000_851);
const RADIUS: f32 = 0.087;
const CULL_DISTANCE: f32 = 1.45;

#[derive(Debug)]
pub(super) struct Globe {
    transform: Mat4,
    center: Vec3,
    seed: f32,
    scale: f32,
    runtime_key: Option<u64>,
    enabled: bool,
}

pub(super) fn placed(level: &LevelDef) -> Vec<Globe> {
    let surfaces = LevelSurfaces::new(level);
    level
        .props
        .iter()
        .enumerate()
        .filter(|(_, prop)| {
            crate::engine::physics::prop_fracture::globe(&prop.model)
                && prop.float.is_none()
                && !prop.shadow_only
        })
        .map(|(index, prop)| {
            let y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
            let transform = super::props::prop_instance_matrix(prop, y);
            Globe {
                transform,
                center: transform.transform_point3(CENTER),
                seed: f32::from(u16::try_from(index).unwrap_or(u16::MAX)),
                scale: prop.scale,
                runtime_key: prop
                    .is_physics_movable()
                    .then(|| crate::engine::physics::prop_fracture::authored_item_key(index)),
                enabled: true,
            }
        })
        .collect()
}

#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite clock and seed drive bounded model-local motion inside the authored globe."
)]
fn flake_pose(globe: &Globe, seconds: f32, index: usize, fade: f32) -> EffectPose {
    let seed = globe
        .seed
        .mul_add(0.327_139, f32::from(u16::try_from(index).unwrap_or(0)));
    let rate = (seed * 0.17).fract().mul_add(0.13, 0.25);
    let life = seconds.mul_add(rate, (seed * 0.618_034).fract()).fract();
    let angle = seed.mul_add(2.399_963, life * 0.87);
    let flight_radius = 0.059 * ((seed * 0.713_09).fract().mul_add(0.72, 0.28));
    let y_top = (RADIUS.mul_add(RADIUS, -flight_radius * flight_radius)).sqrt() - 0.005;
    let y = (-0.061 - y_top).mul_add(life, y_top);
    let local = CENTER + Vec3::new(angle.cos() * flight_radius, y, angle.sin() * flight_radius);
    EffectPose {
        position: globe.transform.transform_point3(local).to_array(),
        size: (seed * 0.132_7).fract().mul_add(0.0023, 0.0021) * globe.scale,
        alpha: 0.8 * ((1.0 - life).min(life) * 11.0).clamp(0.0, 1.0) * fade,
    }
}

pub(super) fn append(
    globes: &[Globe],
    seconds: f32,
    eye: Vec3,
    out: &mut Vec<EffectVertex>,
) -> usize {
    // Fixed scratch, closest first, stable first-placed tie break. No work or
    // upload for particles outside the camera's 1.45 m neighborhood.
    let mut nearest = [(f32::INFINITY, 0_usize); MAX_NEAR];
    for (index, globe) in globes.iter().enumerate() {
        let distance = globe.center.distance_squared(eye);
        if !globe.enabled || distance >= CULL_DISTANCE * CULL_DISTANCE || !distance.is_finite() {
            continue;
        }
        if let Some(slot) = nearest.iter().position(|(saved, _)| distance < *saved) {
            // Re-insert with a right rotation of the suffix, retaining closer candidates.
            if let Some(suffix) = nearest.get_mut(slot..) {
                suffix.rotate_right(1);
                if let Some(first) = suffix.first_mut() {
                    *first = (distance, index);
                }
            }
        }
    }
    let mut count = 0_usize;
    for (distance, index) in nearest {
        if let Some(globe) = globes.get(index).filter(|_| distance.is_finite()) {
            let fade = ((CULL_DISTANCE - distance.sqrt()) / 0.25).clamp(0.0, 1.0);
            for flake in 0..COUNT {
                append_billboard(eye, flake_pose(globe, seconds, flake, fade), 1.0, out);
                count = count.saturating_add(1);
            }
        }
    }
    count
}

impl Globe {
    pub(super) fn set_pose(globes: &mut [Self], key: u64, position: [f32; 3], rotation: [f32; 4]) {
        if let Some(globe) = globes
            .iter_mut()
            .find(|entry| entry.runtime_key == Some(key))
        {
            globe.transform = Mat4::from_scale_rotation_translation(
                Vec3::splat(globe.scale),
                glam::Quat::from_array(rotation).normalize(),
                Vec3::from_array(position),
            );
            globe.center = globe.transform.transform_point3(CENTER);
        }
    }

    pub(super) fn set_visible(globes: &mut [Self], key: u64, visible: bool) {
        if let Some(globe) = globes
            .iter_mut()
            .find(|entry| entry.runtime_key == Some(key))
        {
            globe.enabled = visible;
        }
    }
}
