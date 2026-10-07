//! Bounded camera-local snowfall. Seeds are retained once; positions remain
//! world-anchored as the camera moves. Wrapping occurs only at faded volume
//! edges. Weather reuses the effect billboard renderer and never enters bakes.
use glam::{Mat4, Vec3};

use super::effects::{EffectPose, EffectVertex, append_billboard};
use crate::level::{CeilingProfileDef, LevelDef, ceiling_y_for_volume};
use crate::quality::QualityLevel;
use crate::spatial::{Aabb, Frustum};
use crate::weather::{MAX_SNOW_PARTICLES, SnowfallDef};

#[derive(Clone, Copy, Debug)]
struct Seed {
    origin: Vec3,
    speed: f32,
    size: f32,
    phase: f32,
    rate: f32,
}

#[derive(Debug)]
struct Shelter {
    bounds: (f32, f32, f32, f32),
    floor: f32,
    height: f32,
    profile: CeilingProfileDef,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SnowStats {
    pub evaluated: usize,
    pub submitted: usize,
    pub sheltered: usize,
    pub culled: usize,
    /// Sum of projected quad areas / screen area, before depth or sheet alpha.
    pub coverage: f32,
}

#[derive(Debug)]
pub struct SnowScene {
    config: SnowfallDef,
    seeds: Vec<Seed>,
    shelters: Vec<Shelter>,
    pub texture: usize,
    opacity: f32,
}

impl SnowScene {
    #[must_use]
    pub fn build(level: &LevelDef, config: &SnowfallDef, texture: usize, opacity: f32) -> Self {
        let seeds = (0..usize::try_from(config.count)
            .unwrap_or(0)
            .min(MAX_SNOW_PARTICLES))
            .map(|index| {
                let key = u32::try_from(index).unwrap_or(0);
                let sample = |lane| random(key.wrapping_mul(8).wrapping_add(lane));
                Seed {
                    origin: Vec3::new(
                        sample(0) * config.radius * 2.0,
                        sample(1) * config.height,
                        sample(2) * config.radius * 2.0,
                    ),
                    speed: lerp(config.speed, sample(3)),
                    size: lerp(config.size, sample(4).powi(2)),
                    phase: sample(5) * std::f32::consts::TAU,
                    rate: sample(6).mul_add(0.35, 0.25),
                }
            })
            .collect();
        let mut shelters: Vec<_> = level
            .rooms
            .iter()
            .filter(|room| !room.ceiling.is_open())
            .map(|room| Shelter {
                bounds: room.bounds(),
                floor: room.floor_y,
                height: room.height,
                profile: room.ceiling,
            })
            .collect();
        // Authored opaque slabs cover porch roofs and overhangs too. Narrow
        // vertical walls only suppress snow within their own solid footprint.
        for wall in level.void_walls.iter().filter(|wall| wall.occludes) {
            let (min, max) = wall.bounds();
            shelters.push(Shelter {
                bounds: (min[0], max[0], min[2], max[2]),
                floor: min[1],
                height: max[1] - min[1],
                profile: CeilingProfileDef::Flat,
            });
        }
        Self {
            config: config.clone(),
            seeds,
            shelters,
            texture,
            opacity,
        }
    }

    #[must_use]
    pub const fn budget(&self) -> usize {
        self.seeds.len()
    }

    /// World-anchored analytic motion; the camera selects an equivalent tile.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite clock, validated bounded weather and finite camera"
    )]
    fn pose(&self, seed: Seed, seconds: f32, camera: Vec3) -> EffectPose {
        let width = self.config.radius * 2.0;
        let phase = seconds.mul_add(seed.rate, seed.phase);
        let moving = seed.origin
            + Vec3::new(
                phase.sin().mul_add(0.22, self.config.wind[0] * seconds),
                -seed.speed * seconds,
                phase
                    .mul_add(0.73, seed.phase)
                    .sin()
                    .mul_add(0.18, self.config.wind[1] * seconds),
            );
        let half_height = self.config.height * 0.5;
        let relative = Vec3::new(
            (moving.x - camera.x + self.config.radius).rem_euclid(width) - self.config.radius,
            (moving.y - camera.y + half_height).rem_euclid(self.config.height) - half_height,
            (moving.z - camera.z + self.config.radius).rem_euclid(width) - self.config.radius,
        );
        let distance = relative.length();
        let radial = smooth((self.config.radius - distance - 0.35) / (self.config.radius * 0.25));
        let vertical = smooth((half_height - relative.y.abs() - 0.35) / 0.8);
        let near = smooth((distance - 0.65) / 0.6);
        EffectPose {
            position: (camera + relative).to_array(),
            size: seed.size,
            alpha: self.config.opacity * self.opacity * radial * vertical * near,
        }
    }

    /// Zero below a real ceiling. A short fade outside its footprint avoids
    /// drift popping at doorways while keeping outside snow visible indoors.
    fn shelter_alpha(&self, position: Vec3, size: f32) -> f32 {
        let mut alpha = 1.0_f32;
        for shelter in &self.shelters {
            let (x0, x1, z0, z1) = shelter.bounds;
            let dx = (x0 - position.x).max(position.x - x1).max(0.0);
            let dz = (z0 - position.z).max(position.z - z1).max(0.0);
            let outside = dx.max(dz);
            if outside >= 0.35 {
                continue;
            }
            let ceiling = ceiling_y_for_volume(
                shelter.bounds,
                shelter.floor,
                shelter.height,
                shelter.profile,
                position.x,
                position.z,
            );
            let above = smooth((position.y - ceiling - size) / 0.35);
            alpha = alpha.min(above.max(smooth(outside / 0.35)));
        }
        alpha
    }

    /// Appends only useful flakes. No spawn, sort, raycast or allocation occurs
    /// here when the caller retains its budget-sized scratch buffer.
    #[must_use]
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Bounded finite billboard projection and weather fades"
    )]
    pub fn append(
        &self,
        seconds: f32,
        camera: Vec3,
        projection: Option<Mat4>,
        frustum: &Frustum,
        quality: QualityLevel,
        out: &mut Vec<EffectVertex>,
    ) -> SnowStats {
        let fraction = match quality {
            QualityLevel::Low => 2,
            QualityLevel::Medium => 3,
            QualityLevel::High => 4,
        };
        let count = self.budget().saturating_mul(fraction) / 4;
        let mut stats = SnowStats::default();
        for seed in self.seeds.iter().take(count) {
            stats.evaluated = stats.evaluated.saturating_add(1);
            let mut pose = self.pose(*seed, seconds, camera);
            let position = Vec3::from_array(pose.position);
            let extent = Vec3::splat(pose.size);
            if pose.alpha <= 0.002
                || !frustum.intersects_aabb(&Aabb {
                    min: (position - extent).to_array(),
                    max: (position + extent).to_array(),
                })
            {
                stats.culled = stats.culled.saturating_add(1);
                continue;
            }
            pose.alpha *= self.shelter_alpha(position, pose.size);
            if pose.alpha <= 0.002 {
                stats.sheltered = stats.sheltered.saturating_add(1);
                continue;
            }
            let first = out.len();
            append_billboard(camera, pose, 1.0, out);
            // Project actual quad corners; this conservative fill estimate
            // includes transparent texels and fragments later rejected by depth.
            if let Some(matrix) = projection {
                let mut min = glam::Vec2::splat(f32::INFINITY);
                let mut max = glam::Vec2::splat(f32::NEG_INFINITY);
                for vertex in out.iter().skip(first) {
                    let clip = matrix * Vec3::from_array(vertex.position).extend(1.0);
                    let xy = clip.truncate().truncate() / clip.w.max(0.001);
                    min = min.min(xy.clamp(glam::Vec2::splat(-1.0), glam::Vec2::ONE));
                    max = max.max(xy.clamp(glam::Vec2::splat(-1.0), glam::Vec2::ONE));
                }
                stats.coverage = ((max.x - min.x) * (max.y - min.y)).mul_add(0.25, stats.coverage);
            }
            stats.submitted = stats.submitted.saturating_add(1);
        }
        stats
    }
}

fn lerp([min, max]: [f32; 2], value: f32) -> f32 {
    (max - min).mul_add(value, min)
}
fn smooth(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * t.mul_add(-2.0, 3.0)
}
fn random(key: u32) -> f32 {
    let mut mixed = key.wrapping_add(0x9e37_79b9);
    mixed = (mixed ^ (mixed >> 16)).wrapping_mul(0x85eb_ca6b);
    mixed = (mixed ^ (mixed >> 13)).wrapping_mul(0xc2b2_ae35);
    mixed ^= mixed >> 16_u32;
    f32::from(u16::try_from(mixed >> 16).unwrap_or(0)) / 65535.0
}

#[cfg(test)]
mod tests {
    use super::*;
    fn scene() -> Result<SnowScene, serde_json::Error> {
        let level: LevelDef =
            serde_json::from_str(include_str!("../../../assets/levels/winter.json"))?;
        Ok(SnowScene::build(&level, &SnowfallDef::default(), 0, 1.0))
    }
    #[test]
    fn fall_is_downward_varied_and_frame_rate_independent() -> Result<(), serde_json::Error> {
        let scene = scene()?;
        let camera = Vec3::new(0.0, 1.6, 12.0);
        let mut checked = 0_usize;
        for seed in &scene.seeds {
            let first = scene.pose(*seed, 3.0, camera);
            let next = scene.pose(*seed, 3.1, camera);
            if first.alpha > 0.1 && next.alpha > 0.1 {
                assert!(
                    next.position[1] < first.position[1],
                    "fall must be downward"
                );
                assert!(
                    (seed
                        .speed
                        .mul_add(0.1, next.position[1] - first.position[1]))
                    .abs()
                        < 0.0001,
                    "analytic fall speed cannot accumulate frame drift"
                );
                checked = checked.saturating_add(1);
            }
        }
        assert!(checked > 200, "test a meaningful visible sample");
        let slow = scene
            .seeds
            .iter()
            .map(|seed| seed.speed)
            .fold(f32::INFINITY, f32::min);
        let fast = scene
            .seeds
            .iter()
            .map(|seed| seed.speed)
            .fold(0.0_f32, f32::max);
        assert!(fast - slow > 0.5, "flakes must have varied speeds");
        Ok(())
    }
    #[test]
    fn camera_motion_preserves_world_positions_and_fades_wraps() -> Result<(), serde_json::Error> {
        let scene = scene()?;
        let camera = Vec3::new(0.0, 1.6, 12.0);
        for seed in &scene.seeds {
            let before = scene.pose(*seed, 2.0, camera);
            let after = scene.pose(*seed, 2.0, camera + Vec3::new(0.15, 0.1, -0.12));
            let moved =
                Vec3::from_array(before.position).distance(Vec3::from_array(after.position));
            if moved > 0.001 {
                assert!(
                    before.alpha < 0.002 && after.alpha < 0.002,
                    "only invisible edge tiles may wrap"
                );
            }
        }
        Ok(())
    }
    #[test]
    fn ceilings_suppress_indoor_flakes_but_keep_outside_and_above_roof()
    -> Result<(), serde_json::Error> {
        let scene = scene()?;
        assert!(
            scene.shelter_alpha(Vec3::new(-11.5, 2.2, -11.5), 0.05) < 0.001,
            "gable lodge protects its interior"
        );
        assert!(
            scene.shelter_alpha(Vec3::new(-11.5, 8.0, -11.5), 0.05) > 0.99,
            "flakes may fall above roof"
        );
        assert!(
            scene.shelter_alpha(Vec3::new(0.0, 1.6, 12.0), 0.05) > 0.99,
            "open ceiling is outdoor"
        );
        assert!(
            scene.shelter_alpha(Vec3::new(-11.5, 2.2, -10.0), 0.05) < 0.001,
            "roof footprint includes doorway edge"
        );
        let outer = scene.shelter_alpha(Vec3::new(-11.5, 2.2, -7.3), 0.05);
        assert!(
            outer > 0.0 && outer < 1.0,
            "short boundary fade prevents drift popping"
        );
        Ok(())
    }
    #[test]
    fn quality_budgets_culling_and_scratch_capacity_stay_bounded() -> Result<(), serde_json::Error>
    {
        let scene = scene()?;
        let camera = Vec3::new(0.0, 1.6, 12.0);
        let projection = Mat4::perspective_rh(60_f32.to_radians(), 16.0 / 9.0, 0.1, 100.0)
            * Mat4::look_at_rh(camera, camera - Vec3::Z, Vec3::Y);
        let frustum =
            Frustum::from_view_projection(&projection, crate::spatial::DepthRange::ZeroToOne);
        let mut vertices = Vec::with_capacity(scene.budget().saturating_mul(4));
        let capacity = vertices.capacity();
        for (quality, expected) in [
            (QualityLevel::Low, 700),
            (QualityLevel::Medium, 1050),
            (QualityLevel::High, 1400),
        ] {
            for step in 0_u16..100 {
                vertices.clear();
                let stats = scene.append(
                    f32::from(step) * 0.1,
                    camera,
                    Some(projection),
                    &frustum,
                    quality,
                    &mut vertices,
                );
                assert_eq!(
                    stats.evaluated, expected,
                    "quality scales a stable seed prefix"
                );
                assert_eq!(
                    vertices.len(),
                    stats.submitted.saturating_mul(4),
                    "only submitted quads are uploaded"
                );
                assert!(
                    stats.submitted > 0 && stats.submitted < stats.evaluated / 2,
                    "frustum and faded volume cull unused flakes"
                );
                assert_eq!(vertices.capacity(), capacity, "no per-frame scratch growth");
                assert!(stats.coverage < 0.02, "light snow cannot fill the screen");
                assert!(
                    vertices.iter().all(|vertex| vertex
                        .position
                        .iter()
                        .chain(vertex.color.iter())
                        .all(|value| value.is_finite())),
                    "no non-finite GPU data"
                );
            }
        }
        Ok(())
    }
}
