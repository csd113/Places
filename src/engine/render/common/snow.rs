//! Bounded camera-local snowfall. Seeds are retained once; positions remain
//! world-anchored as the camera moves. Wrapping occurs only at faded volume
//! edges. Weather reuses the effect billboard renderer and never enters bakes.
use glam::{Mat4, Vec3};

use super::effects::{EffectPose, EffectVertex, append_billboard};
use crate::engine::level::{CeilingProfileDef, LevelDef, ceiling_y_for_volume};
use crate::engine::quality::QualityLevel;
use crate::engine::spatial::{Aabb, Frustum};
use crate::engine::weather::{MAX_SNOW_PARTICLES, SnowfallDef};

#[derive(Clone, Copy, Debug)]
struct Seed {
    origin: Vec3,
    speed: f32,
    motion_origin: Vec3,
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
    authored_config: SnowfallDef,
    authored_storm: super::storm::StormUniform,
    pub storm: super::storm::StormUniform,
    seeds: Vec<Seed>,
    shelters: Vec<Shelter>,
    pub texture: usize,
    opacity: f32,
    strength: f32,
    motion_seconds: f32,
    motion_active: bool,
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
                    origin: Vec3::new(sample(0), sample(1), sample(2)),
                    speed: sample(3),
                    motion_origin: Vec3::ZERO,
                    size: sample(4).powi(2),
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
        let storm = super::storm::StormUniform::build(level, config);
        Self {
            config: config.clone(),
            authored_config: config.clone(),
            authored_storm: storm,
            storm,
            seeds,
            shelters,
            texture,
            opacity,
            strength: 0.0,
            motion_seconds: 0.0,
            motion_active: false,
        }
    }

    /// Blends prepared numeric endpoints in place. Normalized seeds, shelters,
    /// strings and texture storage are retained for the entire visit.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Validated bounded weather interpolation; exact endpoints use authored values"
    )]
    pub fn apply_strength(&mut self, alternate: &Self, strength: f32, seconds: f32) {
        if strength.to_bits() == self.strength.to_bits() {
            return;
        }
        self.reanchor_motion(seconds);
        let previous_radius = self.config.radius;
        let previous_height = self.config.height;
        self.strength = strength;
        self.config
            .blend_from(&self.authored_config, &alternate.authored_config, strength);
        let horizontal_scale = self.config.radius / previous_radius;
        let vertical_scale = self.config.height / previous_height;
        let phase_scale = Vec3::new(horizontal_scale, vertical_scale, horizontal_scale);
        for seed in &mut self.seeds {
            // Scale the traveled phase uniformly. Adding the original seed's
            // dimension delta after wrapping would collapse phases into sheets.
            seed.motion_origin *= phase_scale;
        }
        if strength <= 0.0 {
            self.storm = self.authored_storm;
            return;
        }
        self.storm = if self.authored_storm.count[0] > 0 {
            self.authored_storm
        } else {
            alternate.authored_storm
        };
        self.storm.color_density = [
            self.config.fog_color[0],
            self.config.fog_color[1],
            self.config.fog_color[2],
            self.config.storm_severity * 2.0 / self.config.visibility_m,
        ];
    }

    /// Preserve each seed's traveled phase before changing velocity or tile
    /// dimensions. Its history stays within one tile, and new velocity uses
    /// elapsed time since this anchor rather than the entire session age.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite clock and validated positive tile dimensions bound each retained motion phase"
    )]
    fn reanchor_motion(&mut self, animation_seconds: f32) {
        let seconds = if animation_seconds.is_finite() {
            animation_seconds.max(0.0)
        } else {
            0.0
        };
        if seconds < self.motion_seconds {
            self.motion_seconds = 0.0;
            self.motion_active = false;
        }
        let elapsed = if self.motion_active {
            seconds - self.motion_seconds
        } else {
            seconds
        };
        let width = self.config.radius * 2.0;
        for seed in &mut self.seeds {
            let fall_rate = lerp(self.config.speed, seed.speed);
            let origin = if self.motion_active {
                seed.motion_origin
            } else {
                seed.origin * Vec3::new(width, self.config.height, width)
            };
            let moving = origin
                + Vec3::new(
                    self.config.wind[0] * elapsed,
                    -fall_rate * elapsed,
                    self.config.wind[1] * elapsed,
                );
            seed.motion_origin = Vec3::new(
                moving.x.rem_euclid(width),
                moving.y.rem_euclid(self.config.height),
                moving.z.rem_euclid(width),
            );
        }
        self.motion_seconds = seconds;
        self.motion_active = true;
    }

    /// The active authored horizontal snow velocity, also used by native diagnostics.
    #[must_use]
    pub const fn wind(&self) -> [f32; 2] {
        self.config.wind
    }

    #[must_use]
    pub const fn budget(&self) -> usize {
        self.seeds.len()
    }

    #[must_use]
    pub fn sky_storm(&self) -> [f32; 4] {
        [
            self.config.fog_color[0],
            self.config.fog_color[1],
            self.config.fog_color[2],
            // The sky is the far background. Use the same extinction as the
            // world at its far plane, rather than a severity-only colour mix
            // that leaves mild/moderate distant objects silhouetted.
            1.0 - super::storm::optical_transmission(
                self.storm.color_density[3] * super::SCENE_FAR_M,
            ),
        ]
    }

    /// World-anchored analytic motion; the camera selects an equivalent tile.
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite clock, validated bounded weather and finite camera"
    )]
    fn pose(&self, seed: Seed, seconds: f32, camera: Vec3) -> EffectPose {
        let width = self.config.radius * 2.0;
        let phase = seconds.mul_add(seed.rate, seed.phase);
        let origin = if self.motion_active {
            seed.motion_origin
        } else {
            Vec3::new(
                seed.origin.x * self.config.radius * 2.0,
                seed.origin.y * self.config.height,
                seed.origin.z * self.config.radius * 2.0,
            )
        };
        let elapsed = if self.motion_active {
            seconds - self.motion_seconds
        } else {
            seconds
        };
        let fall_rate = lerp(self.config.speed, seed.speed);
        let moving = origin
            + Vec3::new(
                phase.sin().mul_add(0.22, self.config.wind[0] * elapsed),
                -fall_rate * elapsed,
                phase
                    .mul_add(0.73, seed.phase)
                    .sin()
                    .mul_add(0.18, self.config.wind[1] * elapsed),
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
            size: lerp(self.config.size, seed.size),
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
        reason = "Bounded finite billboard projection; validated seed fraction in 0..=1"
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
        let prefix = self.budget().saturating_mul(fraction) / 4;
        let limit = (f32::from(u16::try_from(prefix).unwrap_or(0)) * self.config.intensity).floor();
        let mut stats = SnowStats::default();
        for (seed, _) in self
            .seeds
            .iter()
            .take(prefix)
            .zip(0_u16..)
            .take_while(|(_, index)| f32::from(*index) < limit)
        {
            stats.evaluated = stats.evaluated.saturating_add(1);
            let mut pose = self.pose(*seed, seconds, camera);
            let position = Vec3::from_array(pose.position);
            let velocity = Vec3::new(
                self.config.wind[0],
                -lerp(self.config.speed, seed.speed),
                self.config.wind[1],
            );
            let streak = velocity * (self.config.storm_severity * 0.035);
            let extent = Vec3::splat(pose.size) + streak.abs();
            if pose.alpha <= 0.002
                || !frustum.intersects_aabb(&Aabb {
                    min: (position - extent).to_array(),
                    max: (position + extent).to_array(),
                })
            {
                stats.culled = stats.culled.saturating_add(1);
                continue;
            }
            let shelter = self.shelter_alpha(position, pose.size + streak.length());
            if pose.alpha * shelter <= 0.002 {
                stats.sheltered = stats.sheltered.saturating_add(1);
                continue;
            }
            pose.alpha *= shelter * self.storm.transmission(camera, position);
            if pose.alpha <= 0.002 {
                stats.culled = stats.culled.saturating_add(1);
                continue;
            }
            let first = out.len();
            append_billboard(camera, pose, 1.0, out);
            if self.config.storm_severity > 0.0 {
                let facing = (camera - position).normalize_or_zero();
                let along = streak - facing * streak.dot(facing);
                let up = along.normalize_or_zero();
                if up.length_squared() > 0.5 {
                    let right = up.cross(facing).normalize_or_zero() * pose.size * 0.5;
                    let half = along * 0.5 + up * pose.size * 0.5;
                    for (vertex, corner) in out.iter_mut().skip(first).zip([
                        position - right - half,
                        position + right - half,
                        position + right + half,
                        position - right + half,
                    ]) {
                        vertex.position = corner.to_array();
                    }
                }
            }
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
