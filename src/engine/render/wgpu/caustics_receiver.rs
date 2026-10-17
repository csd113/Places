//! Self-excluded caustics for a moving receiver. Whole-object source-region
//! certificates prevent leaks between its eight local interpolation anchors.

use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use std::sync::Arc;

use super::{Grid, RECEIVER_EPSILON, ReceiverField, SourceKind, source_factor};
use crate::engine::level::WaterVolume;
use crate::engine::lighting::LevelLighting;
use crate::engine::render::common::dynamic::DynamicId;
use crate::engine::render::common::dynamic_visibility::DynamicVisibility;
use crate::engine::render::wgpu::caustics::Caustics;
use crate::engine::spatial::Aabb;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct CausticsReceiver {
    /// Local minimum XYZ; W marks a selected receiver, even with zero radiance.
    pub bounds_min: [f32; 4],
    /// Local extent XYZ; W is the selected water's one-based authored priority.
    pub bounds_extent: [f32; 4],
    /// Source-only RGB at XYZ-bit corners; W is the static receiver floor.
    pub radiance: [[f32; 4]; 8],
}

const _: [(); 160] = [(); std::mem::size_of::<CausticsReceiver>()];

impl CausticsReceiver {
    pub const ZERO: Self = Self {
        bounds_min: [0.0; 4],
        bounds_extent: [0.0; 4],
        radiance: [[0.0; 4]; 8],
    };
}

#[derive(PartialEq, Eq)]
struct ReceiverKey {
    lighting: usize,
    dynamic: usize,
    dynamic_revision: u64,
    source_revision: u64,
    source_uploads: u64,
    bounds: [[u32; 3]; 2],
    model: [[u32; 4]; 4],
    excluded: Option<DynamicId>,
}

pub struct CausticsReceiverCache {
    key: Option<ReceiverKey>,
    value: CausticsReceiver,
}

impl Default for CausticsReceiverCache {
    fn default() -> Self {
        Self {
            key: None,
            value: CausticsReceiver::ZERO,
        }
    }
}

impl CausticsReceiverCache {
    pub fn sample(
        &mut self,
        caustics: &Caustics,
        bounds: Aabb,
        model: Mat4,
        excluded: Option<DynamicId>,
        visibility: &DynamicVisibility,
    ) -> CausticsReceiver {
        // Retain the last dependency key while focus is disabled. Re-enabling
        // reuses it only if the sources, receiver and blockers still match.
        if !caustics.has_active_sources() {
            return CausticsReceiver::ZERO;
        }
        let key = ReceiverKey {
            lighting: caustics
                .receiver_lighting
                .as_ref()
                .map_or(0, |lighting| Arc::as_ptr(lighting).addr()),
            dynamic: std::ptr::from_ref(visibility).addr(),
            dynamic_revision: visibility.revision(),
            source_revision: caustics.field.source_revision,
            source_uploads: caustics.source_uploads,
            bounds: [bounds.min.map(f32::to_bits), bounds.max.map(f32::to_bits)],
            model: model
                .to_cols_array_2d()
                .map(|column| column.map(f32::to_bits)),
            excluded,
        };
        if self.key.as_ref() != Some(&key) {
            self.value = caustics.receiver(bounds, model, excluded, visibility);
            self.key = Some(key);
        }
        self.value
    }
}

fn axis_cell(position: f32, minimum: f32, maximum: f32, count: u16) -> (u16, f32) {
    let intervals = count.saturating_sub(1).max(1);
    let step = (maximum - minimum) / f32::from(intervals);
    let last = intervals.saturating_sub(1);
    let lower = (0..intervals)
        .find(|cell| position <= f32::from(cell.saturating_add(1)).mul_add(step, minimum))
        .unwrap_or(last);
    let start = f32::from(lower).mul_add(step, minimum);
    (
        lower,
        ((position - start) / step.max(f32::MIN_POSITIVE)).clamp(0.0, 1.0),
    )
}

fn floor_bound(field: &ReceiverField, volume: &WaterVolume, grid: Grid, point: [f32; 3]) -> f32 {
    let (x, fx) = axis_cell(point[0], volume.x0, volume.x1, grid.dimensions[0]);
    let (z, fz) = axis_cell(point[2], volume.z0, volume.z1, grid.dimensions[2]);
    let floor = |dx: u16, dz: u16| {
        field
            .samples
            .get(grid.offset.saturating_add(grid.record_ordinal(
                x,
                0,
                z,
                dx.saturating_add(dz.saturating_mul(4)),
            )))
            .map_or(volume.bottom_y, |sample| sample[3])
    };
    let first = (floor(1, 0) - floor(0, 0)).mul_add(fx, floor(0, 0));
    let last = (floor(1, 1) - floor(0, 1)).mul_add(fx, floor(0, 1));
    (last - first).mul_add(fz, first)
}

fn anchor_positions(bounds: Aabb, model: Mat4) -> [[f32; 3]; 8] {
    [0_u8, 1, 2, 3, 4, 5, 6, 7].map(|corner| {
        let coordinate = |axis: usize, mask: u8| {
            if corner & mask == 0 {
                bounds.min.get(axis).copied().unwrap_or(0.0)
            } else {
                bounds.max.get(axis).copied().unwrap_or(0.0)
            }
        };
        model
            .transform_point3(Vec3::new(
                coordinate(0, 1),
                coordinate(1, 2),
                coordinate(2, 4),
            ))
            .to_array()
    })
}

fn containing_volume(volumes: &[WaterVolume], world: Aabb) -> Option<(usize, &WaterVolume)> {
    volumes.iter().enumerate().rev().find(|(_, volume)| {
        world.min[1] < volume.surface_y - 0.005
            && world.max[1] >= volume.bottom_y - 0.005
            && [world.min[0], world.max[0]].iter().all(|x| {
                [world.min[2], world.max[2]]
                    .iter()
                    .all(|z| volume.contains(*x, *z))
            })
    })
}

impl Caustics {
    /// Bounds are in model space. The returned anchors exclude this receiver's
    /// own dynamic caster while retaining every other source-specific blocker.
    #[must_use]
    pub fn receiver(
        &self,
        bounds: Aabb,
        model: Mat4,
        excluded: Option<DynamicId>,
        visibility: &DynamicVisibility,
    ) -> CausticsReceiver {
        if bounds.is_empty() || !model.is_finite() {
            return CausticsReceiver::ZERO;
        }
        let Some(lighting) = self.receiver_lighting.as_deref() else {
            return CausticsReceiver::ZERO;
        };
        if !self.field.has_active_sources() {
            return CausticsReceiver::ZERO;
        }
        let world = crate::engine::render::common::props::transform_bounds(&bounds, &model);
        let Some((index, volume)) = containing_volume(&self.receiver_volumes, world) else {
            return CausticsReceiver::ZERO;
        };
        sample_receiver(
            &self.field,
            lighting,
            (index, volume),
            bounds,
            model,
            excluded,
            visibility,
        )
    }
}

pub(super) fn sample_receiver(
    field: &ReceiverField,
    lighting: &LevelLighting,
    water: (usize, &WaterVolume),
    bounds: Aabb,
    model: Mat4,
    excluded: Option<DynamicId>,
    visibility: &DynamicVisibility,
) -> CausticsReceiver {
    let (index, volume) = water;
    let Some(grid) = field.grids.get(index).copied() else {
        return CausticsReceiver::ZERO;
    };
    let world = crate::engine::render::common::props::transform_bounds(&bounds, &model);
    let mut positions = anchor_positions(bounds, model);
    let mut receiver_bounds = Aabb::EMPTY;
    let mut floors = [0.0; 8];
    for (position, floor) in positions.iter_mut().zip(&mut floors) {
        *floor = lighting.caustics_receiver_floor(
            position[0],
            position[2],
            floor_bound(field, volume, grid, *position),
            volume.surface_y,
        );
        position[1] = position[1]
            .max(*floor + RECEIVER_EPSILON)
            .min(volume.surface_y - 0.006);
        receiver_bounds.expand(*position);
    }
    // World bounds cover the complete receiving surface, even when a
    // rotated local box has no interpolation anchor at an X/Z extreme.
    receiver_bounds.min[0] = world.min[0];
    receiver_bounds.min[2] = world.min[2];
    receiver_bounds.max[0] = world.max[0];
    receiver_bounds.max[2] = world.max[2];
    let priority = f32::from(u16::try_from(index.saturating_add(1)).unwrap_or(u16::MAX));
    let mut result = CausticsReceiver {
        bounds_min: [bounds.min[0], bounds.min[1], bounds.min[2], 1.0],
        bounds_extent: [
            bounds.max[0] - bounds.min[0],
            bounds.max[1] - bounds.min[1],
            bounds.max[2] - bounds.min[2],
            priority,
        ],
        radiance: floors.map(|floor| [0.0, 0.0, 0.0, floor]),
    };
    for source in &field.sources {
        if !source.eligible || !source.receiver_supported || !source.state.active() {
            continue;
        }
        let visible = match source.kind {
            SourceKind::Local(light_index) => {
                lighting.runtime_caustic_local_region_visible(light_index, receiver_bounds)
                    && visibility.local_region_visible(source.bounds, receiver_bounds, excluded)
            }
            SourceKind::Directional(incoming) => {
                lighting.runtime_direction_region_visible(receiver_bounds, incoming)
                    && visibility.directional_region_visible(receiver_bounds, incoming, excluded)
            }
        };
        if !visible {
            continue;
        }
        let factors = positions.map(|position| source_factor(source, lighting, volume, position));
        // Full-anchor eligibility also contracts range and water-entry
        // boundaries; interpolation must not borrow another corner's gate.
        if factors
            .iter()
            .any(|factor| !factor.is_finite() || *factor <= 0.0)
        {
            continue;
        }
        let energy = source.state.energy();
        for (factor, radiance) in factors.iter().zip(&mut result.radiance) {
            for (channel, light) in radiance.iter_mut().take(3).zip(energy) {
                *channel = light.mul_add(*factor, *channel);
            }
        }
    }
    result
}
