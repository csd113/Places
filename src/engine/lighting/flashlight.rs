//! Handheld beam geometry, receiver response and CPU observation visibility.

use glam::Vec3;

use super::attached_lights::{MAX_DYNAMIC_LIGHT_INTENSITY, MAX_DYNAMIC_LIGHT_RADIUS};
use super::transport::TransportScene;
use crate::engine::{loader::RawImage, spatial::Aabb};
use std::sync::Arc;

/// A handheld spotlight sharing the attached-light material term. The beam is
/// renderer neutral; gameplay samples the same angular and distance response.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Flashlight {
    pub position: Vec3,
    pub direction: Vec3,
    pub color: [f32; 3],
    pub intensity: f32,
    pub range: f32,
    pub inner_cos: f32,
    pub outer_cos: f32,
}

impl Flashlight {
    #[must_use]
    pub fn is_well_formed(self) -> bool {
        self.position.is_finite()
            && self.direction.is_finite()
            && (self.direction.length_squared() - 1.0).abs() < 0.001
            && self
                .color
                .iter()
                .all(|value| value.is_finite() && (0.0..=1.0).contains(value))
            && self.intensity.is_finite()
            && (0.0..=MAX_DYNAMIC_LIGHT_INTENSITY).contains(&self.intensity)
            && self.range.is_finite()
            && (0.1..=MAX_DYNAMIC_LIGHT_RADIUS).contains(&self.range)
            && self.inner_cos.is_finite()
            && self.outer_cos.is_finite()
            && (0.1..1.0).contains(&self.outer_cos)
            && self.inner_cos > self.outer_cos
            && self.inner_cos <= 1.0
    }

    /// Incident linear light before material cosine and scene visibility.
    #[must_use]
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Validated spotlight inputs use finite glam floating-point vector arithmetic."
    )]
    pub fn illumination(self, point: [f32; 3]) -> f32 {
        if !self.is_well_formed() || !point.iter().all(|value| value.is_finite()) {
            return 0.0;
        }
        let offset = Vec3::from_array(point) - self.position;
        let distance = offset.length();
        if !distance.is_finite() || distance >= self.range || distance <= 0.000_001 {
            return 0.0;
        }
        let angular = ((self.direction.dot(offset / distance) - self.outer_cos)
            / (self.inner_cos - self.outer_cos))
            .clamp(0.0, 1.0);
        let cone = angular * angular * 2.0_f32.mul_add(-angular, 3.0);
        let window = (1.0 - (distance / self.range).powi(4)).max(0.0);
        self.intensity * cone * window * window / distance.mul_add(distance, 1.0)
    }
}

/// A near-eye attachment, aimed at view centre six metres away.
/// The visibility callback keeps the source on the camera side of nearby
/// geometry, including live doors and movable furniture.
#[expect(
    clippy::arithmetic_side_effects,
    reason = "Finite camera inputs use bounded metre offsets and glam floating-point vector arithmetic."
)]
pub fn camera_attached_light(
    eye: Vec3,
    forward: Vec3,
    right: Vec3,
    mut visibility: impl FnMut(Vec3, Vec3) -> f32,
) -> Flashlight {
    // Keep a lateral baseline for cast shadows, but no camera-relative drop:
    // a low source projects close props above their visible tops onto distant
    // receivers, making the silhouette look like a floating box when looking up.
    let offset = right * 0.14 + forward * 0.06;
    let candidate = eye + offset;
    let position = if visibility(eye, candidate) >= 0.99 {
        candidate
    } else {
        let mut clear = 0.0_f32;
        let mut blocked = 1.0_f32;
        // Eight bounded queries resolve this 16 cm offset below a millimetre.
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

/// Only gameplay ray candidates acquire a local posed visibility resource.
/// Textures are copied once per asset identity; pose refreshes share those PNGs.
struct CharacterVisibility {
    birth: u64,
    asset: usize,
    revision: u64,
    opacity_bits: u32,
    textures: Vec<Arc<RawImage>>,
    scene: Option<TransportScene>,
}

/// Current model geometry inputs; skinning and mesh conversion stay with the caller.
pub struct PosedBlocker<'a> {
    pub asset: usize,
    pub instance: Option<&'a str>,
    pub opacity: f32,
    pub revision: u64,
    pub bounds: Aabb,
    pub model: glam::Mat4,
    pub textures: &'a [Arc<RawImage>],
}

#[derive(Default)]
pub struct PosedVisibilityCache(Vec<CharacterVisibility>);

impl PosedVisibilityCache {
    pub fn clear(&mut self) {
        self.0.clear();
    }

    pub fn transmittance<'a, T: 'a, I: Iterator<Item = (u64, &'a T)>>(
        &mut self,
        current: impl Fn() -> I,
        describe: impl Fn(&'a T) -> PosedBlocker<'a>,
        build_scene: impl Fn(&T, &[Arc<RawImage>]) -> Option<TransportScene>,
        from: [f32; 3],
        to: [f32; 3],
        excluded: Option<&str>,
    ) -> f32 {
        let cache = &mut self.0;
        cache.retain(|cached| current().any(|(birth, _)| birth == cached.birth));
        let mut throughput = 1.0;
        for (birth, character) in current() {
            let blocker = describe(character);
            if blocker.opacity <= 0.0 || excluded.is_some_and(|id| blocker.instance == Some(id)) {
                continue;
            }
            let bounds = blocker.bounds;
            if !from
                .into_iter()
                .zip(to)
                .zip(bounds.min.into_iter().zip(bounds.max))
                .all(|((a, b), (minimum, maximum))| a.min(b) <= maximum && a.max(b) >= minimum)
            {
                continue;
            }
            let asset = blocker.asset;
            let index = cache
                .iter()
                .position(|cached| cached.birth == birth)
                .unwrap_or(cache.len());
            if index == cache.len() {
                let textures = cache
                    .iter()
                    .find(|cached| cached.asset == asset)
                    .map_or_else(
                        || blocker.textures.to_vec(),
                        |cached| cached.textures.clone(),
                    );
                cache.push(CharacterVisibility {
                    birth,
                    asset,
                    revision: u64::MAX,
                    opacity_bits: u32::MAX,
                    textures,
                    scene: None,
                });
            }
            let Some(cached) = cache.get_mut(index) else {
                return 0.0;
            };
            if cached.asset != asset {
                cached.asset = asset;
                cached.revision = u64::MAX;
                blocker.textures.clone_into(&mut cached.textures);
            }
            let opacity_bits = blocker.opacity.to_bits();
            if cached.revision != blocker.revision || cached.opacity_bits != opacity_bits {
                cached.scene = build_scene(character, &cached.textures);
                cached.revision = blocker.revision;
                cached.opacity_bits = opacity_bits;
            }
            let Some(scene) = cached.scene.as_ref() else {
                return 0.0;
            };
            let inverse = blocker.model.inverse();
            if !inverse.is_finite() {
                return 0.0;
            }
            throughput *= scene.transmittance(
                inverse.transform_point3(Vec3::from_array(from)).to_array(),
                inverse.transform_point3(Vec3::from_array(to)).to_array(),
            );
            if throughput <= 0.0 {
                return 0.0;
            }
        }
        throughput
    }
}

/// Direct incident light on five torso/head anchors. The caller owns receiver
/// identity, tick order and response hysteresis; this only samples the beam.
pub fn observe_receiver(
    position: Vec3,
    yaw_degrees: f32,
    size: [f32; 3],
    mut sample: impl FnMut(Vec3) -> f32,
) -> f32 {
    let yaw = yaw_degrees.to_radians();
    let side_x = yaw.cos() * (size[0] * 0.32);
    let side_z = -yaw.sin() * (size[0] * 0.32);
    let torso = Vec3::new(position.x, size[1].mul_add(0.62, position.y), position.z);
    let samples = [
        torso,
        Vec3::new(torso.x + side_x, torso.y, torso.z + side_z),
        Vec3::new(torso.x - side_x, torso.y, torso.z - side_z),
        Vec3::new(position.x, size[1].mul_add(0.88, position.y), position.z),
        Vec3::new(position.x, size[1].mul_add(0.36, position.y), position.z),
    ];
    samples.into_iter().fold(0.0_f32, |strongest, point| {
        let measured = sample(point);
        if measured.is_finite() {
            strongest.max(measured)
        } else {
            strongest
        }
    })
}
