//! Bounded solid-volume quadrature and Archimedes forces beside Rapier.
//!
//! Rapier solves rigid contacts, not a fluid field. Two vertical rays per
//! column measure the posed convex solid, then clip that interval to the real
//! water footprint, bottom and surface. Integrating its first moment gives a
//! centre of buoyancy that changes with immersion and orientation.

use glam::Vec3;
use rapier3d::parry::bounding_volume::Aabb;
use rapier3d::prelude::{Pose, Ray, SharedShape, Vector};

use crate::level::{WaterShape, WaterVolume, WaterVolumes};

use super::collision::{add, point, scaled, vector};
use super::config::{BodyMotion, WaterPhysicsDef, validate_volume_mass};
use super::world::Runtime;
use super::{FIXED_STEP, GRAVITY};

/// Fresh water in kilograms per cubic metre; independent of its render material.
const WATER_DENSITY_KG_M3: f32 = 1000.0;
/// Bound expensive clipped-domain shape casts even in densely tiled water.
const MAX_CLIPPED_REGIONS: usize = 16;
/// Midpoints of six equal intervals. Each domain needs at most 72 shape rays.
const COLUMN_OFFSETS: [f32; 6] = [
    1.0 / 12.0,
    3.0 / 12.0,
    5.0 / 12.0,
    7.0 / 12.0,
    9.0 / 12.0,
    11.0 / 12.0,
];

struct DisplacedPart {
    shape: SharedShape,
    local_pose: Pose,
    volume_m3: f32,
    volume_centre: Vector,
}

pub(super) struct WaterBody {
    parts: Box<[DisplacedPart]>,
    pub volume_m3: f32,
    pub submerged_fraction: f32,
    linear_drag: f32,
    angular_drag: f32,
}

#[derive(Default)]
struct Immersion {
    volume_m3: f32,
    moment: Vec3,
    // Preserve an analytic centre without multiplying/dividing world positions.
    // Their round-trip error otherwise creates torque on symmetric floats.
    exact_centre: Option<Vec3>,
}

impl WaterBody {
    pub(super) fn new(
        shape: &SharedShape,
        profile: WaterPhysicsDef,
        scale: f32,
        mass_kg: f32,
    ) -> Result<Self, String> {
        let mut parts = Vec::with_capacity(
            shape
                .as_compound()
                .map_or(1, |compound| compound.shapes().len()),
        );
        if let Some(compound) = shape.as_compound() {
            for (local_pose, child) in compound.shapes() {
                parts.push(DisplacedPart::new(child, *local_pose)?);
            }
        } else {
            parts.push(DisplacedPart::new(shape, Pose::IDENTITY)?);
        }
        let solid_volume: f32 = parts.iter().map(|part| part.volume_m3).sum();
        if !solid_volume.is_finite() || solid_volume <= 0.0 {
            return Err("physics solid displaced volume must be finite and positive".into());
        }
        let volume_m3 = profile.scaled_volume(scale)?.unwrap_or(solid_volume);
        validate_volume_mass(volume_m3, mass_kg)?;
        let minimum_thickness = shape.compute_local_aabb().extents().min_element();
        let stiffness_step = WATER_DENSITY_KG_M3 * GRAVITY * volume_m3 * FIXED_STEP * FIXED_STEP
            / (mass_kg * minimum_thickness);
        if !stiffness_step.is_finite() || stiffness_step > 1.0 {
            return Err(
                "physics water lift is too stiff for the collider thickness at 120 Hz; reduce displaced volume or increase mass".into(),
            );
        }
        let volume_ratio = volume_m3 / solid_volume;
        for part in &mut parts {
            part.volume_m3 *= volume_ratio;
        }
        Ok(Self {
            parts: parts.into_boxed_slice(),
            volume_m3,
            submerged_fraction: 0.0,
            linear_drag: profile.linear_drag,
            angular_drag: profile.angular_drag,
        })
    }

    fn immersion(&self, body_pose: Pose, water: &WaterVolumes) -> Immersion {
        if let [part] = self.parts.as_ref() {
            return part.immersion(body_pose, water);
        }
        let mut result = Immersion::default();
        for part in &self.parts {
            let sample = part.immersion(body_pose, water);
            result.volume_m3 += sample.volume_m3;
            result.moment = add(result.moment, sample.moment);
        }
        result
    }
}

impl DisplacedPart {
    fn new(shape: &SharedShape, local_pose: Pose) -> Result<Self, String> {
        let properties = shape.mass_properties(1.0);
        let volume_m3 = properties.mass();
        if !volume_m3.is_finite() || volume_m3 <= 0.0 {
            return Err("physics convex displaced volume must be finite and positive".into());
        }
        Ok(Self {
            shape: shape.clone(),
            local_pose,
            volume_m3,
            volume_centre: properties.local_com,
        })
    }

    fn immersion(&self, body_pose: Pose, water: &WaterVolumes) -> Immersion {
        let world_pose = Pose::from_parts(
            body_pose.transform_point(self.local_pose.translation),
            body_pose.rotation.mul_quat(self.local_pose.rotation),
        );
        let bounds = self.shape.compute_aabb(&world_pose);
        if !water
            .volumes()
            .iter()
            .any(|volume| overlaps_water(volume, &bounds))
        {
            return Immersion::default();
        }
        // The highest potentially covering volume must contain the complete
        // bounds. Partial higher-priority volumes keep the quadrature path.
        if let Some(volume) = water.volumes().iter().rev().find(|volume| {
            volume.enabled
                && bounds.mins.y < volume.surface_y
                && bounds.mins.x < volume.x1
                && bounds.maxs.x > volume.x0
                && bounds.mins.z < volume.z1
                && bounds.maxs.z > volume.z0
        }) && volume.swimming
            && covers_footprint(volume, &bounds)
        {
            let centre = point(world_pose.transform_point(self.volume_centre));
            if bounds.mins.y >= volume.bottom_y && bounds.maxs.y <= volume.surface_y {
                return Immersion {
                    volume_m3: self.volume_m3,
                    moment: scaled(centre, self.volume_m3),
                    exact_centre: Some(centre),
                };
            }
            if self.has_vertical_section(world_pose) && bounds.maxs.y > bounds.mins.y {
                let Some((bottom, top)) = clip_interval(volume, bounds.mins.y, bounds.maxs.y)
                else {
                    return Immersion::default();
                };
                let fraction = (top - bottom) / (bounds.maxs.y - bounds.mins.y);
                let volume_m3 = self.volume_m3 * fraction.clamp(0.0, 1.0);
                let wet_centre = Vec3::new(centre.x, f32::midpoint(bottom, top), centre.z);
                return Immersion {
                    volume_m3,
                    moment: scaled(wet_centre, volume_m3),
                    exact_centre: Some(wet_centre),
                };
            }
        }
        let measured_solid_volume = self.measure_solid(world_pose, &bounds);
        if measured_solid_volume <= 0.0 {
            return Immersion::default();
        }
        let measured_wet = self.measure_water(world_pose, &bounds, water);
        if measured_wet.volume_m3 <= 0.0 {
            return Immersion::default();
        }
        // Calibrating against the solid's actual volume keeps full-submersion
        // force independent of quadrature error. The sampling domain itself
        // clips to each rectangle or exact circular chord, so crossing a rim
        // changes displaced volume continuously rather than switching columns.
        let volume_m3 =
            (measured_wet.volume_m3 / measured_solid_volume).clamp(0.0, 1.0) * self.volume_m3;
        Immersion {
            volume_m3,
            moment: scaled(measured_wet.moment, volume_m3 / measured_wet.volume_m3),
            exact_centre: None,
        }
    }

    fn has_vertical_section(&self, world_pose: Pose) -> bool {
        let up = world_pose.rotation.mul_vec3(Vector::Y);
        (self.shape.as_cuboid().is_some() || self.shape.as_cylinder().is_some())
            && up.x.abs() <= f32::EPSILON
            && up.z.abs() <= f32::EPSILON
    }

    fn measure_solid(&self, world_pose: Pose, bounds: &Aabb) -> f32 {
        let width = bounds.maxs.x - bounds.mins.x;
        let depth = bounds.maxs.z - bounds.mins.z;
        // Both rays start outside the solid, even for the minimum 1 mm shape.
        let above = bounds.maxs.y + 0.001;
        let below = bounds.mins.y - 0.001;
        let ray_length = above - below;
        let mut solid_height = 0.0;
        for x_offset in COLUMN_OFFSETS {
            let x = width.mul_add(x_offset, bounds.mins.x);
            for z_offset in COLUMN_OFFSETS {
                let z = depth.mul_add(z_offset, bounds.mins.z);
                let Some((bottom, top)) =
                    self.column_interval(world_pose, x, z, (below, above, ray_length))
                else {
                    continue;
                };
                solid_height += (top - bottom).max(0.0);
            }
        }
        solid_height * width * depth / 36.0
    }

    fn measure_water(&self, world_pose: Pose, bounds: &Aabb, water: &WaterVolumes) -> Immersion {
        if many_unshadowed_regions(bounds, water) {
            return self.measure_dense_water(world_pose, bounds, water);
        }
        let above = bounds.maxs.y + 0.001;
        let below = bounds.mins.y - 0.001;
        let ray_length = above - below;
        let mut measured_wet_volume = 0.0;
        let mut wet_moment = Vec3::ZERO;
        let mut covering_surface = f32::NEG_INFINITY;
        for volume in water.volumes().iter().rev() {
            if !volume.enabled {
                continue;
            }
            let shadowed = volume.surface_y <= covering_surface;
            if covers_footprint(volume, bounds) {
                covering_surface = covering_surface.max(volume.surface_y);
            }
            if shadowed {
                continue;
            }
            if !overlaps_water(volume, bounds) {
                continue;
            }
            let x_min = bounds.mins.x.max(volume.x0);
            let x_span = bounds.maxs.x.min(volume.x1) - x_min;
            for x_offset in COLUMN_OFFSETS {
                let x = x_span.mul_add(x_offset, x_min);
                let (water_z_min, water_z_max) = match volume.shape {
                    WaterShape::Rect => (volume.z0, volume.z1),
                    WaterShape::Circle => {
                        let (centre_x, centre_z) = volume.center();
                        let x_distance = x - centre_x;
                        let half_chord = (volume
                            .radius
                            .mul_add(volume.radius, -x_distance * x_distance))
                        .max(0.0)
                        .sqrt();
                        (centre_z - half_chord, centre_z + half_chord)
                    }
                };
                let z_min = bounds.mins.z.max(water_z_min);
                let z_span = bounds.maxs.z.min(water_z_max) - z_min;
                if z_span <= 0.0 {
                    continue;
                }
                let column_area = x_span * z_span / 36.0;
                for z_offset in COLUMN_OFFSETS {
                    let z = z_span.mul_add(z_offset, z_min);
                    let Some((bottom, top)) =
                        self.column_interval(world_pose, x, z, (below, above, ray_length))
                    else {
                        continue;
                    };
                    let Some((selected, wet_bottom, wet_top)) =
                        submerged_interval(water, x, z, bottom, top)
                    else {
                        continue;
                    };
                    if !std::ptr::eq(selected, volume) {
                        continue;
                    }
                    let submerged_volume = (wet_top - wet_bottom) * column_area;
                    measured_wet_volume += submerged_volume;
                    wet_moment = add(
                        wet_moment,
                        scaled(
                            Vec3::new(x, f32::midpoint(wet_bottom, wet_top), z),
                            submerged_volume,
                        ),
                    );
                }
            }
        }
        Immersion {
            volume_m3: measured_wet_volume,
            moment: wet_moment,
            exact_centre: None,
        }
    }

    fn measure_dense_water(
        &self,
        world_pose: Pose,
        bounds: &Aabb,
        water: &WaterVolumes,
    ) -> Immersion {
        let above = bounds.maxs.y + 0.001;
        let below = bounds.mins.y - 0.001;
        let limits = (below, above, above - below);
        let width = bounds.maxs.x - bounds.mins.x;
        let depth = bounds.maxs.z - bounds.mins.z;
        let area = width * depth / 36.0;
        let mut result = Immersion::default();
        // Pathological overlapping/finely tiled volumes use the same exact
        // membership and depth contract with coarser edge quadrature. Shape
        // ray work stays constant instead of growing with authored volume count.
        for x_offset in COLUMN_OFFSETS {
            let x = width.mul_add(x_offset, bounds.mins.x);
            for z_offset in COLUMN_OFFSETS {
                let z = depth.mul_add(z_offset, bounds.mins.z);
                let Some((bottom, top)) = self.column_interval(world_pose, x, z, limits) else {
                    continue;
                };
                let Some((_, wet_bottom, wet_top)) = submerged_interval(water, x, z, bottom, top)
                else {
                    continue;
                };
                let volume = (wet_top - wet_bottom) * area;
                result.volume_m3 += volume;
                result.moment = add(
                    result.moment,
                    scaled(Vec3::new(x, f32::midpoint(wet_bottom, wet_top), z), volume),
                );
            }
        }
        result
    }

    fn column_interval(
        &self,
        world_pose: Pose,
        x: f32,
        z: f32,
        (below, above, ray_length): (f32, f32, f32),
    ) -> Option<(f32, f32)> {
        let downward = Ray::new(Vector::new(x, above, z), -Vector::Y);
        let upward = Ray::new(Vector::new(x, below, z), Vector::Y);
        let entry_distance = self
            .shape
            .cast_ray(&world_pose, &downward, ray_length, true)?;
        let exit_distance = self
            .shape
            .cast_ray(&world_pose, &upward, ray_length, true)?;
        Some((below + exit_distance, above - entry_distance))
    }
}

fn many_unshadowed_regions(bounds: &Aabb, water: &WaterVolumes) -> bool {
    let mut count = 0_usize;
    let mut covering_surface = f32::NEG_INFINITY;
    for volume in water.volumes().iter().rev() {
        if !volume.enabled {
            continue;
        }
        let shadowed = volume.surface_y <= covering_surface;
        if covers_footprint(volume, bounds) {
            covering_surface = covering_surface.max(volume.surface_y);
        }
        if !shadowed && overlaps_water(volume, bounds) {
            count = count.saturating_add(1);
            if count > MAX_CLIPPED_REGIONS {
                return true;
            }
        }
    }
    false
}

fn covers_footprint(volume: &WaterVolume, bounds: &Aabb) -> bool {
    volume.contains(bounds.mins.x, bounds.mins.z)
        && volume.contains(bounds.mins.x, bounds.maxs.z)
        && volume.contains(bounds.maxs.x, bounds.mins.z)
        && volume.contains(bounds.maxs.x, bounds.maxs.z)
}

fn overlaps_water(volume: &WaterVolume, bounds: &Aabb) -> bool {
    volume.enabled
        && volume.swimming
        && bounds.mins.x < volume.x1
        && bounds.maxs.x > volume.x0
        && bounds.mins.z < volume.z1
        && bounds.maxs.z > volume.z0
        && bounds.mins.y < volume.surface_y
        && bounds.maxs.y > volume.bottom_y
}

fn submerged_interval(
    water: &WaterVolumes,
    x: f32,
    z: f32,
    bottom: f32,
    top: f32,
) -> Option<(&WaterVolume, f32, f32)> {
    // Authored priority agrees with controller sampling; no double buoyancy
    // when volumes overlap. Decorative and disabled volumes exert no force.
    let selected = water
        .volumes()
        .iter()
        .rev()
        .find(|volume| volume.enabled && volume.contains(x, z) && bottom < volume.surface_y)?;
    if !selected.swimming {
        return None;
    }
    clip_interval(selected, bottom, top)
        .map(|(wet_bottom, wet_top)| (selected, wet_bottom, wet_top))
}

fn clip_interval(volume: &WaterVolume, bottom: f32, top: f32) -> Option<(f32, f32)> {
    let wet_bottom = bottom.max(volume.bottom_y);
    let wet_top = top.min(volume.surface_y);
    (wet_top > wet_bottom).then_some((wet_bottom, wet_top))
}

impl Runtime {
    pub(super) fn apply_water(&mut self, water: &WaterVolumes) {
        for (&key, entry) in &mut self.entries {
            let Some(response) = entry.water.as_mut() else {
                continue;
            };
            if entry.dormant
                || entry.recovery.suspended
                || self.held.as_ref().is_some_and(|held| held.key == key)
            {
                response.submerged_fraction = 0.0;
                continue;
            }
            let Some(body) = self.world.bodies.get_mut(entry.body) else {
                continue;
            };
            if body.is_sleeping() || !body.is_enabled() || !body.is_dynamic() {
                continue;
            }
            let immersion = response.immersion(*body.position(), water);
            let fraction = (immersion.volume_m3 / response.volume_m3).clamp(0.0, 1.0);
            response.submerged_fraction = fraction;
            if immersion.volume_m3 <= 0.0 {
                continue;
            }
            // Exponential mass-weighted damping is stable even at the validated
            // 100/s maximum, unlike an explicit velocity-proportional force.
            // Existing dry damping and every collision/CCD constraint remain
            // owned by Rapier. Drag adds no horizontal current or scripted bob.
            let fluid_mass_ratio = WATER_DENSITY_KG_M3 * immersion.volume_m3 / body.mass();
            let linear_decay = (-response.linear_drag * fluid_mass_ratio * FIXED_STEP).exp();
            let angular_decay = (-response.angular_drag * fluid_mass_ratio * FIXED_STEP).exp();
            body.set_linvel(vector(scaled(point(body.linvel()), linear_decay)), false);
            body.set_angvel(vector(scaled(point(body.angvel()), angular_decay)), false);
            let centre = immersion
                .exact_centre
                .unwrap_or_else(|| scaled(immersion.moment, 1.0 / immersion.volume_m3));
            let impulse = WATER_DENSITY_KG_M3 * GRAVITY * immersion.volume_m3 * FIXED_STEP;
            // This is the resultant of the distributed columns, including its
            // moment about the true mass centre. A fixed-step impulse avoids
            // Rapier's persistent-force accumulation across frames and releases.
            body.apply_impulse_at_point(Vector::new(0.0, impulse, 0.0), vector(centre), false);
        }
    }

    pub(super) fn wake_changed_water(&mut self) {
        for entry in self.entries.values() {
            if entry.motion == BodyMotion::Dynamic && !entry.dormant && !entry.recovery.suspended {
                self.world.wake_up(entry.body, true);
            }
        }
    }
}
