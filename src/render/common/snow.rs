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

    #[cfg(test)]
    pub(super) const fn configuration(&self) -> &SnowfallDef {
        &self.config
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
            self.config.storm_severity.sqrt(),
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

#[cfg(test)]
mod tests {
    use super::*;
    fn scene() -> Result<SnowScene, serde_json::Error> {
        let level: LevelDef =
            serde_json::from_str(include_str!("../../../assets/levels/winter.json"))?;
        Ok(SnowScene::build(&level, &SnowfallDef::default(), 0, 1.0))
    }
    #[test]
    fn severe_streaks_and_quality_prefixes_use_bounded_retained_storage()
    -> Result<(), Box<dyn std::error::Error>> {
        let level: LevelDef =
            serde_json::from_str(include_str!("../../../assets/levels/winter.json"))?;
        let mut config = SnowfallDef::blizzard();
        config.intensity = 0.5;
        let scene = SnowScene::build(&level, &config, 0, 1.0);
        let camera = Vec3::new(0.0, 1.6, 10.0);
        let projection = Mat4::perspective_rh(60_f32.to_radians(), 16.0 / 9.0, 0.1, 100.0)
            * Mat4::look_at_rh(camera, Vec3::new(0.0, 1.6, 9.0), Vec3::Y);
        let frustum =
            Frustum::from_view_projection(&projection, crate::spatial::DepthRange::ZeroToOne);
        let mut vertices = Vec::with_capacity(scene.budget().saturating_mul(4));
        let capacity = vertices.capacity();
        for (quality, expected) in [
            (QualityLevel::Low, 350),
            (QualityLevel::Medium, 525),
            (QualityLevel::High, 700),
        ] {
            for step in 0_u16..60 {
                vertices.clear();
                let stats = scene.append(
                    f32::from(step) * 0.1,
                    camera,
                    Some(projection),
                    &frustum,
                    quality,
                    &mut vertices,
                );
                assert_eq!(stats.evaluated, expected);
                assert!(stats.submitted > 0 && stats.submitted < expected / 2);
                assert_eq!(vertices.capacity(), capacity);
                assert!(stats.coverage < 0.2);
                assert!(
                    vertices
                        .iter()
                        .flat_map(|vertex| vertex.position)
                        .all(f32::is_finite)
                );
                let first = vertices.first().ok_or("missing streak corner")?;
                let second = vertices.get(1).ok_or("missing streak corner")?;
                let third = vertices.get(2).ok_or("missing streak corner")?;
                let width =
                    Vec3::from_array(first.position).distance(Vec3::from_array(second.position));
                let length =
                    Vec3::from_array(second.position).distance(Vec3::from_array(third.position));
                assert!(
                    length > width * 2.0,
                    "wind-aligned streak must be elongated"
                );
            }
        }
        Ok(())
    }

    #[test]
    fn extinction_culls_are_not_misreported_as_roof_suppression() -> Result<(), serde_json::Error> {
        let level: LevelDef =
            serde_json::from_str(include_str!("../../../assets/levels/winter.json"))?;
        let mut scene = SnowScene::build(&level, &SnowfallDef::blizzard(), 0, 1.0);
        scene.shelters.clear();
        scene.storm.count = [0; 4];
        let camera = Vec3::new(0.0, 1.6, 10.0);
        let matrix = Mat4::perspective_rh(60_f32.to_radians(), 16.0 / 9.0, 0.1, 100.0)
            * Mat4::look_at_rh(camera, Vec3::new(0.0, 1.6, 9.0), Vec3::Y);
        let frustum = Frustum::from_view_projection(&matrix, crate::spatial::DepthRange::ZeroToOne);
        let mut vertices = Vec::with_capacity(scene.budget().saturating_mul(4));
        let stats = scene.append(
            2.0,
            camera,
            None,
            &frustum,
            QualityLevel::High,
            &mut vertices,
        );
        assert_eq!(stats.sheltered, 0, "open air has no roof suppression");
        assert!(stats.culled > 0);
        assert_eq!(
            stats.evaluated,
            stats.submitted.saturating_add(stats.culled)
        );
        Ok(())
    }

    #[test]
    fn weather_preset_timeline_preserves_phase_distribution_at_every_quality()
    -> Result<(), Box<dyn std::error::Error>> {
        let level: LevelDef =
            serde_json::from_str(include_str!("../../../assets/levels/winter.json"))?;
        let base = level.weather.as_ref().ok_or("Winter needs base weather")?;
        let endpoint = level
            .weather_alternate
            .as_ref()
            .ok_or("Winter needs alternate weather")?;
        let mut snow = SnowScene::build(&level, base.snowfall(), 0, 1.0);
        let alternate = SnowScene::build(&level, endpoint.snowfall(), 0, 1.0);
        let seed_address = snow.seeds.as_ptr();
        let seed_capacity = snow.seeds.capacity();
        let mut playback = crate::weather::WeatherPlayback::new(None);
        let mut seconds = 0.0_f32;
        for frame in 1_u16..=1500 {
            // Native E edges for moderate -> calm -> mild -> moderate -> severe
            // -> calm, including the two-second transitions between presets.
            let requested_strength = match frame {
                61 | 571 => Some(0.35),
                241 | 1201 => Some(0.0),
                385 => Some(0.15),
                763 => Some(1.0),
                _ => None,
            };
            if let Some(target_strength) = requested_strength {
                assert!(playback.set_strength(target_strength, 2.0));
            }
            let next_seconds = f32::from(frame) / 60.0;
            playback.advance(next_seconds - seconds);
            snow.apply_strength(&alternate, playback.strength(), next_seconds);
            if frame.is_multiple_of(60)
                || matches!(
                    frame,
                    185 | 365 | 510 | 700 | 900 | 970 | 1040 | 1100 | 1350
                )
            {
                assert_phase_distribution(&snow, next_seconds);
            }
            if matches!(frame, 970 | 1040 | 1100) {
                assert_eq!(playback.strength().to_bits(), 1.0_f32.to_bits());
                assert_same_phase_quality_coverage(&snow, next_seconds);
            }
            seconds = next_seconds;
        }
        assert_eq!(playback.strength().to_bits(), 0.0_f32.to_bits());
        assert_calm_motion_configuration(&snow);
        assert_eq!(
            snow.seeds.as_ptr(),
            seed_address,
            "presets retain the original seed allocation"
        );
        assert_eq!(
            snow.seeds.capacity(),
            seed_capacity,
            "presets never grow seed storage"
        );
        Ok(())
    }

    fn assert_phase_distribution(snow: &SnowScene, seconds: f32) {
        let width = snow.config.radius * 2.0;
        for fraction in [2, 3, 4] {
            let prefix = snow.budget().saturating_mul(fraction) / 4;
            let mut bins = [[0_usize; 6]; 3];
            for seed in snow.seeds.iter().take(prefix) {
                let position = snow.pose(*seed, seconds, Vec3::ZERO).position;
                for ((coordinate, period), buckets) in position
                    .into_iter()
                    .zip([width, snow.config.height, width])
                    .zip(&mut bins)
                {
                    let normalized = period.mul_add(0.5, coordinate) / period;
                    assert!(
                        (0.0..=1.0).contains(&normalized),
                        "seed remains within its wrapped volume"
                    );
                    for (bucket, edge) in buckets.iter_mut().zip(1_u8..=6) {
                        if normalized <= f32::from(edge) / 6.0 {
                            *bucket = bucket.saturating_add(1);
                            break;
                        }
                    }
                }
            }
            for (axis, buckets) in bins.iter().enumerate() {
                assert!(
                    buckets
                        .iter()
                        .all(|count| *count >= prefix / 12 && *count <= prefix / 3),
                    "phase coverage collapsed at clock {seconds}, prefix {prefix}, axis {axis}: {buckets:?}"
                );
            }
        }
    }

    fn assert_same_phase_quality_coverage(snow: &SnowScene, seconds: f32) {
        let camera = Vec3::new(-11.5, 1.6, -3.5);
        let render_camera = super::super::camera::RenderCamera::new(
            camera,
            180_f32.to_radians(),
            12_f32.to_radians(),
            60.0,
        );
        let (projection, frustum) =
            render_camera.view_projection(super::super::view::DrawableSize::new(640, 360));
        let mut vertices = Vec::with_capacity(snow.budget().saturating_mul(4));
        let capacity = vertices.capacity();
        let mut previous_submitted = 0_usize;
        for (quality, fraction) in [
            (QualityLevel::Low, 2),
            (QualityLevel::Medium, 3),
            (QualityLevel::High, 4),
        ] {
            vertices.clear();
            let stats = snow.append(
                seconds,
                camera,
                Some(projection),
                &frustum,
                quality,
                &mut vertices,
            );
            let prefix = snow.budget().saturating_mul(fraction) / 4;
            assert_eq!(
                stats.evaluated, prefix,
                "quality retains its authored seed prefix"
            );
            assert!(
                stats.submitted >= prefix / 40,
                "severe outside snow must cover the same phase at {quality:?}, clock {seconds}: {stats:?}"
            );
            assert!(
                stats.submitted >= previous_submitted,
                "higher quality includes the same visible seeds"
            );
            assert_eq!(
                stats
                    .submitted
                    .saturating_add(stats.sheltered)
                    .saturating_add(stats.culled),
                prefix,
                "all retained particles remain accounted for"
            );
            assert_eq!(
                vertices.capacity(),
                capacity,
                "quality appends never grow scratch storage"
            );
            previous_submitted = stats.submitted;
        }
    }

    #[test]
    fn weather_strength_ramps_keep_aged_motion_bounded_and_retained()
    -> Result<(), serde_json::Error> {
        for age in [0.0, 300.0, 900.0] {
            for fixed_dimensions in [false, true] {
                for automatic in [false, true] {
                    assert_aged_ramp_motion(age, fixed_dimensions, automatic)?;
                }
            }
        }
        Ok(())
    }

    fn assert_aged_ramp_motion(
        age: f32,
        fixed_dimensions: bool,
        automatic: bool,
    ) -> Result<(), serde_json::Error> {
        let level: LevelDef =
            serde_json::from_str(include_str!("../../../assets/levels/winter.json"))?;
        let mut snow = scene()?;
        let mut endpoint = SnowfallDef::blizzard();
        if fixed_dimensions {
            endpoint.radius = snow.config.radius;
            endpoint.height = snow.config.height;
        }
        let alternate = SnowScene::build(&level, &endpoint, 0, 1.0);
        let mut playback = ramp_playback(automatic);
        let seed_address = snow.seeds.as_ptr();
        let seed_capacity = snow.seeds.capacity();
        let camera = Vec3::new(0.0, 1.6, 0.0);
        let mut previous = Vec::with_capacity(snow.budget());
        let mut seconds = age;
        for frame in 1_u16..=240 {
            if frame == 121 && !automatic {
                assert!(playback.set_strength(0.0, 2.0));
            }
            let next_seconds = age + f32::from(frame) / 60.0;
            let delta = next_seconds - seconds;
            let previous_config = MotionBounds {
                wind: snow.config.wind,
                speed: snow.config.speed,
                width: snow.config.radius * 2.0,
                height: snow.config.height,
            };
            previous.clear();
            previous.extend(
                snow.seeds
                    .iter()
                    .map(|seed| snow.pose(*seed, seconds, camera)),
            );
            playback.advance(delta);
            snow.apply_strength(&alternate, playback.strength(), next_seconds);
            assert_ramp_displacement(
                &snow,
                &previous,
                &previous_config,
                next_seconds,
                delta,
                camera,
            );
            seconds = next_seconds;
        }
        assert!(
            playback.strength().abs() < 1.0e-6,
            "a full aged ramp returns to calm"
        );
        assert_calm_motion_configuration(&snow);
        let paused: Vec<_> = snow
            .seeds
            .iter()
            .map(|seed| snow.pose(*seed, seconds, camera))
            .collect();
        playback.advance(0.0);
        snow.apply_strength(&alternate, playback.strength(), seconds);
        assert!(
            snow.seeds
                .iter()
                .zip(paused)
                .all(|(seed, pose)| snow.pose(*seed, seconds, camera) == pose),
            "paused strength updates preserve the motion anchor"
        );
        assert_eq!(
            snow.seeds.as_ptr(),
            seed_address,
            "strength ramps retain the original seed allocation"
        );
        assert_eq!(
            snow.seeds.capacity(),
            seed_capacity,
            "strength ramps never grow seed storage"
        );
        Ok(())
    }

    fn ramp_playback(automatic: bool) -> crate::weather::WeatherPlayback {
        let mut playback =
            crate::weather::WeatherPlayback::new(Some(crate::weather::WeatherCycleDef {
                max_strength: 0.35,
                period_seconds: 4.0,
                transition_seconds: 2.0,
                ..crate::weather::WeatherCycleDef::default()
            }));
        if automatic {
            assert!(playback.set_cycle(true));
        } else {
            assert!(playback.set_strength(0.35, 2.0));
        }
        playback
    }

    fn assert_calm_motion_configuration(snow: &SnowScene) {
        assert_eq!(
            snow.config.wind, snow.authored_config.wind,
            "calm wind returns exactly"
        );
        assert_eq!(
            snow.config.speed, snow.authored_config.speed,
            "calm fall speed returns exactly"
        );
        assert_eq!(
            snow.config.radius.to_bits(),
            snow.authored_config.radius.to_bits(),
            "calm radius returns exactly"
        );
        assert_eq!(
            snow.config.height.to_bits(),
            snow.authored_config.height.to_bits(),
            "calm height returns exactly"
        );
    }

    struct MotionBounds {
        wind: [f32; 2],
        speed: [f32; 2],
        width: f32,
        height: f32,
    }

    fn assert_ramp_displacement(
        snow: &SnowScene,
        previous: &[EffectPose],
        prior: &MotionBounds,
        seconds: f32,
        delta: f32,
        camera: Vec3,
    ) {
        let width = snow.config.radius * 2.0;
        let height = snow.config.height;
        let horizontal_bound =
            3.0_f32.mul_add((width - prior.width).abs(), 0.15_f32.mul_add(delta, 0.003));
        let vertical_bound = 3.0_f32.mul_add((height - prior.height).abs(), 0.003);
        let wrapped = |value: f32, period: f32| {
            period.mul_add(-0.5, period.mul_add(0.5, value).rem_euclid(period))
        };
        for (seed, before) in snow.seeds.iter().zip(previous) {
            let after = snow.pose(*seed, seconds, camera);
            let displacement = Vec3::new(
                after.position[0] - before.position[0],
                after.position[1] - before.position[1],
                after.position[2] - before.position[2],
            );
            assert!(
                prior.wind[0]
                    .mul_add(-delta, wrapped(displacement.x, width))
                    .abs()
                    <= horizontal_bound,
                "X motion exceeded authored wind, flutter and bounded resizing at clock {seconds}"
            );
            assert!(
                prior.wind[1]
                    .mul_add(-delta, wrapped(displacement.z, width))
                    .abs()
                    <= horizontal_bound,
                "Z motion exceeded authored wind, flutter and bounded resizing at clock {seconds}"
            );
            assert!(
                lerp(prior.speed, seed.speed)
                    .mul_add(delta, wrapped(displacement.y, height))
                    .abs()
                    <= vertical_bound,
                "fall motion exceeded authored speed and bounded resizing at clock {seconds}"
            );
            assert!(
                seed.motion_origin.is_finite() && seed.motion_origin.abs().max_element() < 65.0,
                "retained seed phases remain bounded independently of session age"
            );
        }
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
                    (lerp(scene.config.speed, seed.speed)
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
            .map(|seed| lerp(scene.config.speed, seed.speed))
            .fold(f32::INFINITY, f32::min);
        let fast = scene
            .seeds
            .iter()
            .map(|seed| lerp(scene.config.speed, seed.speed))
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
