//! Outdoor optical depth, shared by world and decal passes. Shelter volumes
//! are convex, including gable roof planes; overlapping intervals are unioned.
use crate::engine::level::{CeilingProfileDef, LevelDef, WallAxis};
use crate::engine::weather::{MAX_STORM_SHELTERS, SnowfallDef};
use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct StormShelter {
    pub min: [f32; 4],
    pub max: [f32; 4],
    /// x/z gradients, roof peak, slope-axis midpoint.
    pub roof: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct StormUniform {
    /// RGB atmosphere and optical density.
    pub color_density: [f32; 4],
    pub count: [u32; 4],
    pub shelters: [StormShelter; MAX_STORM_SHELTERS],
}

impl Default for StormUniform {
    fn default() -> Self {
        Self::zeroed()
    }
}

impl StormUniform {
    #[must_use]
    pub fn build(level: &LevelDef, config: &SnowfallDef) -> Self {
        let mut result = Self::default();
        if config.storm_severity == 0.0 {
            return result;
        }
        result.color_density = [
            config.fog_color[0],
            config.fog_color[1],
            config.fog_color[2],
            config.storm_severity * 2.0 / config.visibility_m,
        ];
        let interiors = level
            .rooms
            .iter()
            .filter(|room| !room.ceiling.is_open())
            .map(|room| {
                let (x0, x1, z0, z1) = room.bounds();
                let eave = room.floor_y + room.height;
                let (gx, gz, peak, mid) = match room.ceiling {
                    CeilingProfileDef::Gable { ridge, ridge_rise } => match ridge {
                        WallAxis::X => (
                            0.0,
                            ridge_rise * 2.0 / (z1 - z0),
                            eave + ridge_rise,
                            z0.midpoint(z1),
                        ),
                        WallAxis::Z => (
                            ridge_rise * 2.0 / (x1 - x0),
                            0.0,
                            eave + ridge_rise,
                            x0.midpoint(x1),
                        ),
                    },
                    CeilingProfileDef::Flat | CeilingProfileDef::Open => (0.0, 0.0, eave, 0.0),
                };
                StormShelter {
                    min: [x0, room.floor_y, z0, 0.0],
                    max: [x1, peak, z1, 0.0],
                    roof: [gx, gz, peak, mid],
                }
            });
        let roofs = level
            .void_walls
            .iter()
            .filter(|wall| wall.occludes)
            .map(|wall| {
                let (min, max) = wall.bounds();
                StormShelter {
                    min: [min[0], -1000.0, min[2], 0.0],
                    max: [max[0], max[1], max[2], 0.0],
                    roof: [0.0, 0.0, max[1], 0.0],
                }
            });
        for (slot, shelter) in result.shelters.iter_mut().zip(interiors.chain(roofs)) {
            *slot = shelter;
            result.count[0] = result.count[0].saturating_add(1);
        }
        result
    }
}

impl StormUniform {
    /// Analytic exterior distance; fixed scratch and merged shelter intervals.
    #[must_use]
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "Finite world positions and validated bounded convex roof planes"
    )]
    pub fn transmission(&self, eye: glam::Vec3, point: glam::Vec3) -> f32 {
        if self.color_density[3] == 0.0 {
            return 1.0;
        }
        let ray = point - eye;
        let mut spans = [[0.0_f32; 2]; MAX_STORM_SHELTERS];
        let mut count = 0_usize;
        for shelter in self
            .shelters
            .iter()
            .take(usize::try_from(self.count[0]).unwrap_or(0))
        {
            let mut span = [0.0, 1.0];
            for (axis, normal) in [glam::Vec3::X, glam::Vec3::Y, glam::Vec3::Z]
                .into_iter()
                .enumerate()
            {
                clip(
                    &mut span,
                    eye,
                    ray,
                    normal,
                    shelter.max.get(axis).copied().unwrap_or(0.0),
                );
                clip(
                    &mut span,
                    eye,
                    ray,
                    -normal,
                    -shelter.min.get(axis).copied().unwrap_or(0.0),
                );
            }
            let [gx, gz, peak, mid] = shelter.roof;
            let slope = glam::Vec3::new(gx, 0.0, gz);
            clip(
                &mut span,
                eye,
                ray,
                slope + glam::Vec3::Y,
                (gx + gz).mul_add(mid, peak),
            );
            clip(
                &mut span,
                eye,
                ray,
                -slope + glam::Vec3::Y,
                (-(gx + gz)).mul_add(mid, peak),
            );
            if span[1] > span[0]
                && let Some(slot) = spans.get_mut(count)
            {
                *slot = span;
                count = count.saturating_add(1);
            }
        }
        let active = spans.get_mut(..count).unwrap_or(&mut []);
        active.sort_unstable_by(|left, right| left[0].total_cmp(&right[0]));
        let mut covered = 0.0_f32;
        let mut end = 0.0_f32;
        for [start, stop] in active {
            covered += (*stop - end.max(*start)).max(0.0);
            end = end.max(*stop);
        }
        optical_transmission(self.color_density[3] * ray.length() * (1.0 - covered).clamp(0.0, 1.0))
    }
}

/// Squared-exponential extinction with a smooth tail to exact zero. The
/// finite endpoint prevents HDR emitters retaining contrast at great distance.
#[must_use]
pub fn optical_transmission(optical: f32) -> f32 {
    let tail = (optical - 2.0).clamp(0.0, 1.0);
    (-optical.powi(2)).exp() * (tail * tail).mul_add(-2.0_f32.mul_add(-tail, 3.0), 1.0)
}

fn clip(span: &mut [f32; 2], eye: glam::Vec3, ray: glam::Vec3, normal: glam::Vec3, offset: f32) {
    let start = offset - normal.dot(eye);
    let delta = normal.dot(ray);
    if delta.abs() < 1.0e-6 {
        if start < -1.0e-5 {
            *span = [1.0, 0.0];
        }
    } else if delta > 0.0 {
        span[1] = span[1].min(start / delta);
    } else {
        span[0] = span[0].max(start / delta);
    }
}
