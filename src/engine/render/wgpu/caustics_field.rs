//! Geometry-only direct-light bases for the caustics receiver lattice.
//! No irradiance bake or imagery generation. Scene revisions refresh bounded
//! source-specific cell certificates against the existing movable caster bounds.

use crate::engine::level::{GlobalIlluminatorDef, LevelDef, LevelSurfaces, WaterVolume};
use crate::engine::lighting::{BakedLight, LevelLighting, directional::DirectionalLight};
use crate::engine::render::common::dynamic_visibility::DynamicVisibility;
use crate::engine::spatial::{
    Aabb, directional_light_region_overlaps, finite_light_region_overlaps,
};

#[path = "caustics_receiver.rs"]
pub(super) mod receiver;

#[cfg(test)]
#[path = "../../tests/caustics_field.rs"]
mod tests;

pub(super) const MAX_SAMPLES: usize = 65_536;
pub(super) const MAX_FACTORS: usize = 4_194_304;
const RECEIVER_EPSILON: f32 = 0.012;
const SAMPLE_SPACING: f32 = 0.15;
const VERTICAL_SAMPLE_SPACING: f32 = 0.45;

#[derive(Clone, Copy, Debug, PartialEq)]
struct SourceState {
    enabled: bool,
    intensity: f32,
    color: [f32; 3],
}

impl SourceState {
    const fn local(light: &BakedLight) -> Self {
        Self {
            enabled: light.enabled(),
            intensity: light.intensity(),
            color: light.color().to_array(),
        }
    }

    fn energy(self) -> [f32; 3] {
        if self.enabled {
            self.color.map(|channel| channel * self.intensity)
        } else {
            [0.0; 3]
        }
    }

    fn active(self) -> bool {
        self.enabled && self.intensity > 0.0 && self.color.iter().any(|channel| *channel > 0.0)
    }
}

#[derive(Clone, Copy)]
enum SourceKind {
    Local(usize),
    Directional([f32; 3]),
}

struct Source {
    id: String,
    kind: SourceKind,
    state: SourceState,
    observed: SourceState,
    factors: Vec<Factor>,
    eligible: bool,
    intensity_share: f32,
    bounds: Aabb,
    receiver_supported: bool,
}

#[derive(Clone, Copy)]
struct Factor {
    sample: u32,
    value: f32,
    transmission: f32,
}

#[derive(Clone, Copy)]
struct Grid {
    dimensions: [u16; 3],
    offset: usize,
    count: usize,
    cell_offset: usize,
    cell_count: usize,
    point_offset: usize,
    point_count: usize,
}

struct ReceiverCell {
    bounds: Aabb,
    corners: [usize; 8],
    points: [usize; 8],
}

impl Grid {
    fn new(volume: &WaterVolume, spacing: f32, offset: usize) -> Self {
        let lengths = [volume.x1 - volume.x0, volume.depth(), volume.z1 - volume.z0];
        // Horizontal detail resolves narrow stair treads. Height remains a
        // bounded smooth approximation with at most three interpolation cells.
        let dimensions = [0_usize, 1, 2].map(|axis| {
            let maximum = if axis == 1 { 4 } else { 128 };
            let step = if axis == 1 {
                spacing.max(VERTICAL_SAMPLE_SPACING)
            } else {
                spacing
            };
            (2_u16..=maximum)
                .find(|count| {
                    f32::from(count.saturating_sub(1)) * step
                        >= lengths.get(axis).copied().unwrap_or(0.0)
                })
                .unwrap_or(maximum)
        });
        let point_count = dimensions.iter().fold(1_usize, |product, axis| {
            product.saturating_mul(usize::from(*axis))
        });
        let cell_count = dimensions.iter().fold(1_usize, |product, axis| {
            product.saturating_mul(usize::from(axis.saturating_sub(1)))
        });
        Self {
            dimensions,
            offset,
            count: cell_count.saturating_mul(8),
            cell_offset: 0,
            cell_count,
            point_offset: 0,
            point_count,
        }
    }

    fn gpu(self) -> [u32; 4] {
        [
            u32::from(self.dimensions[0]),
            u32::from(self.dimensions[1]),
            u32::from(self.dimensions[2]),
            u32::try_from(self.offset).unwrap_or(0),
        ]
    }

    fn ordinal(self, x: u16, y: u16, z: u16) -> usize {
        usize::from(z)
            .saturating_mul(usize::from(self.dimensions[1]))
            .saturating_add(usize::from(y))
            .saturating_mul(usize::from(self.dimensions[0]))
            .saturating_add(usize::from(x))
    }

    fn record_ordinal(self, x: u16, y: u16, z: u16, corner: u16) -> usize {
        usize::from(z)
            .saturating_mul(usize::from(self.dimensions[1].saturating_sub(1)))
            .saturating_add(usize::from(y))
            .saturating_mul(usize::from(self.dimensions[0].saturating_sub(1)))
            .saturating_add(usize::from(x))
            .saturating_mul(8)
            .saturating_add(usize::from(corner))
    }

    fn coordinates(self, ordinal: usize) -> [u16; 3] {
        let width = usize::from(self.dimensions[0]);
        let height = usize::from(self.dimensions[1]);
        [
            u16::try_from(ordinal.checked_rem(width).unwrap_or(0)).unwrap_or(0),
            u16::try_from(
                ordinal
                    .checked_div(width)
                    .and_then(|row| row.checked_rem(height))
                    .unwrap_or(0),
            )
            .unwrap_or(0),
            u16::try_from(
                ordinal
                    .checked_div(width)
                    .and_then(|row| row.checked_div(height))
                    .unwrap_or(0),
            )
            .unwrap_or(0),
        ]
    }

    fn position(self, volume: &WaterVolume, ordinal: usize) -> [f32; 3] {
        let coordinates = self.coordinates(ordinal);
        let fraction = |axis: usize| {
            f32::from(coordinates.get(axis).copied().unwrap_or(0))
                / f32::from(
                    self.dimensions
                        .get(axis)
                        .copied()
                        .unwrap_or(2)
                        .saturating_sub(1),
                )
        };
        [
            (volume.x1 - volume.x0).mul_add(fraction(0), volume.x0),
            volume.depth().mul_add(fraction(1), volume.bottom_y - 0.005),
            (volume.z1 - volume.z0).mul_add(fraction(2), volume.z0),
        ]
    }
}

#[derive(Default)]
pub(super) struct ReceiverField {
    /// Eight independent records per interpolation cell, with linear direct
    /// RGB and the column's static receiver floor in W.
    pub(super) samples: Vec<[f32; 4]>,
    grids: Vec<Grid>,
    sources: Vec<Source>,
    pub(super) dropped_sources: usize,
    receivers: Vec<Option<[f32; 3]>>,
    pub(super) dynamic_revision: Option<u64>,
    pub(super) dynamic_queries: u64,
    pub(super) dynamic_region_queries: u64,
    pub(super) source_revision: u64,
    cells: Vec<ReceiverCell>,
    receiver_bounds: Aabb,
    dynamic_transmissions: Vec<f32>,
    dynamic_support: Vec<bool>,
}

fn local_sources(level: &LevelDef, lighting: &LevelLighting) -> Vec<Source> {
    let fixture_ids = level.light_instance_ids();
    let prop_ids = level.prop_instance_ids();
    let mut identities = vec![String::new(); lighting.lights().len()];
    let mut intensity_shares = vec![1.0; lighting.lights().len()];
    for (fixture, id) in fixture_ids.iter().enumerate() {
        let indices = lighting.fixture_light_indices(fixture);
        let share = 1.0 / f32::from(u16::try_from(indices.len()).unwrap_or(u16::MAX).max(1));
        for index in indices {
            if let Some(identity) = identities.get_mut(index) {
                identity.clone_from(id);
            }
            if let Some(intensity_share) = intensity_shares.get_mut(index) {
                *intensity_share = share;
            }
        }
    }
    lighting
        .lights()
        .iter()
        .enumerate()
        .map(|(index, light)| {
            let id = light
                .owner_prop
                .and_then(|owner| prop_ids.get(owner))
                .cloned()
                .or_else(|| identities.get(index).filter(|id| !id.is_empty()).cloned())
                .unwrap_or_else(|| format!("local_source_{index}"));
            let state = SourceState::local(light);
            let bounds = Aabb {
                min: [
                    light.x() - light.half_w(),
                    light.y(),
                    light.z() - light.half_d(),
                ],
                max: [
                    light.x() + light.half_w(),
                    light.y(),
                    light.z() + light.half_d(),
                ],
            };
            Source {
                id,
                kind: SourceKind::Local(index),
                state,
                observed: state,
                factors: Vec::new(),
                eligible: light.source.caustics,
                intensity_share: intensity_shares.get(index).copied().unwrap_or(1.0),
                bounds,
                receiver_supported: false,
            }
        })
        .collect()
}

fn directional_source(def: &GlobalIlluminatorDef, lighting: &LevelLighting) -> Option<Source> {
    // Prepared global sources omit disabled/zero sources. Resolve those same
    // authored directions once so their bases also support later activation.
    let mut active_definition = def.clone();
    active_definition.enabled = true;
    active_definition.bake = true;
    active_definition.intensity = 1.0;
    let normalized = DirectionalLight::from_definition(&active_definition)?;
    let incoming = lighting
        .global_lights()
        .iter()
        .find(|light| light.incoming.map(f32::to_bits) == normalized.incoming.map(f32::to_bits))
        .map_or(normalized.incoming, |light| light.incoming);
    let state = SourceState {
        enabled: def.enabled,
        intensity: def.intensity,
        color: def.color,
    };
    Some(Source {
        id: def.id.clone(),
        kind: SourceKind::Directional(incoming),
        state,
        observed: state,
        factors: Vec::new(),
        eligible: def.bake && def.caustics() && incoming[1] > 0.0,
        intensity_share: 1.0,
        bounds: Aabb::EMPTY,
        receiver_supported: false,
    })
}

fn grids(volumes: &[WaterVolume]) -> Vec<Grid> {
    let mut spacing = SAMPLE_SPACING;
    loop {
        let mut offset = 0_usize;
        let mut cell_offset = 0_usize;
        let mut point_offset = 0_usize;
        let result: Vec<_> = volumes
            .iter()
            .map(|volume| {
                let mut grid = Grid::new(volume, spacing, offset);
                grid.cell_offset = cell_offset;
                grid.point_offset = point_offset;
                point_offset = point_offset.saturating_add(grid.point_count);
                offset = offset.saturating_add(grid.count);
                cell_offset = cell_offset.saturating_add(grid.cell_count);
                grid
            })
            .collect();
        if offset <= MAX_SAMPLES {
            return result;
        }
        // Every valid map has at most 8000 volumes and needs at most 64000
        // samples at minimum resolution. Growth therefore terminates.
        spacing *= 1.1;
    }
}

fn query_position(
    volume: &WaterVolume,
    surfaces: &LevelSurfaces<'_>,
    lighting: &LevelLighting,
    position: [f32; 3],
) -> (Option<[f32; 3]>, f32) {
    let base_floor = surfaces
        .floor_y_at(position[0], position[2])
        .unwrap_or(volume.bottom_y)
        .max(volume.bottom_y);
    let floor =
        lighting.caustics_receiver_floor(position[0], position[2], base_floor, volume.surface_y);
    if floor >= volume.surface_y - 0.005 || !volume.contains(position[0], position[2]) {
        return (None, floor);
    }
    let mut receiver = position;
    receiver[1] = position[1]
        .max(floor + RECEIVER_EPSILON)
        .min(volume.surface_y - 0.006);
    (Some(receiver), floor)
}

fn source_factor(
    source: &Source,
    lighting: &LevelLighting,
    volume: &WaterVolume,
    position: [f32; 3],
) -> f32 {
    match source.kind {
        SourceKind::Local(index) => {
            let Some(light) = lighting.lights().get(index) else {
                return 0.0;
            };
            if light.y() <= volume.surface_y {
                return 0.0;
            }
            let fraction = (volume.surface_y - position[1]) / (light.y() - position[1]);
            let entry_x = (light.x() - position[0]).mul_add(fraction, position[0]);
            let entry_z = (light.z() - position[2]).mul_add(fraction, position[2]);
            if !volume.contains(entry_x, entry_z) {
                return 0.0;
            }
            lighting.runtime_caustic_local_factor(index, position)
        }
        SourceKind::Directional(incoming) => {
            let distance = (volume.surface_y - position[1]) / incoming[1];
            let entry_x = incoming[0].mul_add(distance, position[0]);
            let entry_z = incoming[2].mul_add(distance, position[2]);
            if !volume.contains(entry_x, entry_z)
                || !lighting.runtime_direction_visible(position, incoming)
            {
                return 0.0;
            }
            incoming[1]
        }
    }
}

fn source_reaches_volume(source: &Source, lighting: &LevelLighting, volume: &WaterVolume) -> bool {
    match source.kind {
        SourceKind::Directional(_) => true,
        SourceKind::Local(index) => lighting.lights().get(index).is_some_and(|light| {
            light.y() > volume.surface_y
                && light.x() + light.half_w() + light.range() > volume.x0
                && light.x() - light.half_w() - light.range() < volume.x1
                && light.z() + light.half_d() + light.range() > volume.z0
                && light.z() - light.half_d() - light.range() < volume.z1
        }),
    }
}

fn prepare_samples(
    volumes: &[WaterVolume],
    grids: &[Grid],
    surfaces: &LevelSurfaces<'_>,
    lighting: &LevelLighting,
) -> (Vec<[f32; 4]>, Vec<Option<[f32; 3]>>) {
    let mut samples = Vec::new();
    let mut receivers = Vec::new();
    for (volume, grid) in volumes.iter().zip(grids) {
        for ordinal in 0..grid.point_count {
            let mut position = grid.position(volume, ordinal);
            // A minimum-resolution disc has no bounding-box corner inside its
            // footprint. Use its centre column as constant coarse support;
            // exact shader containment still excludes every dry corner. This
            // also preserves the global sample bound for 8000 tiny circles.
            if volume.radius > 0.0 && grid.dimensions[0] == 2 && grid.dimensions[2] == 2 {
                (position[0], position[2]) = volume.center();
            }
            let (receiver, floor) = query_position(volume, surfaces, lighting, position);
            samples.push([0.0, 0.0, 0.0, floor]);
            receivers.push(receiver);
        }
    }
    (samples, receivers)
}

/// Certificates cover complete interpolation cells, including the unsampled
/// strip between corners. A disc's dry corners may be absent from radiance
/// samples, but its full cell footprint remains in the conservative envelope.
fn prepare_cells(
    volumes: &[WaterVolume],
    grids: &[Grid],
    receivers: &[Option<[f32; 3]>],
) -> Vec<ReceiverCell> {
    let mut cells = Vec::new();
    for (volume, grid) in volumes.iter().zip(grids) {
        for z in 0..grid.dimensions[2].saturating_sub(1) {
            for y in 0..grid.dimensions[1].saturating_sub(1) {
                for x in 0..grid.dimensions[0].saturating_sub(1) {
                    let points = [0_u16, 1, 2, 3, 4, 5, 6, 7].map(|corner| {
                        grid.point_offset.saturating_add(grid.ordinal(
                            x.saturating_add(corner & 1),
                            y.saturating_add((corner >> 1) & 1),
                            z.saturating_add((corner >> 2) & 1),
                        ))
                    });
                    let corners = [0_u16, 1, 2, 3, 4, 5, 6, 7].map(|corner| {
                        grid.offset
                            .saturating_add(grid.record_ordinal(x, y, z, corner))
                    });
                    let mut bounds = Aabb::EMPTY;
                    for corner in points {
                        if let Some(point) = receivers.get(corner).copied().flatten() {
                            bounds.expand(point);
                        }
                    }
                    if !bounds.is_empty() {
                        let first = grid.position(volume, grid.ordinal(x, y, z));
                        let last = grid.position(
                            volume,
                            grid.ordinal(
                                x.saturating_add(1),
                                y.saturating_add(1),
                                z.saturating_add(1),
                            ),
                        );
                        bounds.min[0] = first[0];
                        bounds.min[2] = first[2];
                        bounds.max[0] = last[0];
                        bounds.max[2] = last[2];
                    }
                    cells.push(ReceiverCell {
                        bounds,
                        corners,
                        points,
                    });
                }
            }
        }
    }
    cells
}

fn source_basis(
    source: &Source,
    lighting: &LevelLighting,
    volumes: &[WaterVolume],
    grids: &[Grid],
    record_receivers: &[Option<[f32; 3]>],
    cells: &[ReceiverCell],
    available: usize,
) -> Option<Vec<Factor>> {
    let mut factors = Vec::new();
    for (volume, grid) in volumes.iter().zip(grids) {
        if !source_reaches_volume(source, lighting, volume) {
            continue;
        }
        let mut values = vec![None; grid.point_count];
        let entries =
            cells.get(grid.cell_offset..grid.cell_offset.saturating_add(grid.cell_count))?;
        for cell in entries {
            if cell.bounds.is_empty() {
                continue;
            }
            let mut corner_values = [0.0; 8];
            for ((point, corner), value) in
                cell.points.iter().zip(cell.corners).zip(&mut corner_values)
            {
                let slot = values.get_mut(point.saturating_sub(grid.point_offset))?;
                *value = *slot.get_or_insert_with(|| {
                    record_receivers
                        .get(corner)
                        .copied()
                        .flatten()
                        .map_or(0.0, |position| {
                            source_factor(source, lighting, volume, position)
                        })
                });
            }
            // Dry bounding-box corners of circular water supply no support.
            // Every actual wet corner must share this source's entry/range
            // gate so interpolation cannot borrow eligibility across an edge.
            if cell
                .corners
                .iter()
                .zip(corner_values)
                .any(|(corner, factor)| {
                    record_receivers.get(*corner).copied().flatten().is_some()
                        && (!factor.is_finite() || factor <= 0.0)
                })
                || !corner_values.iter().any(|value| *value > 0.0)
            {
                continue;
            }
            let visible = match source.kind {
                SourceKind::Local(index) => {
                    lighting.runtime_caustic_local_region_visible(index, cell.bounds)
                }
                SourceKind::Directional(incoming) => {
                    lighting.runtime_direction_region_visible(cell.bounds, incoming)
                }
            };
            if !visible {
                continue;
            }
            // Each cell owns its eight records. A blocked neighbour must not
            // erase the shared spatial corner of this separately clear cell.
            for (sample, factor) in cell.corners.iter().zip(corner_values) {
                if !factor.is_finite() || factor <= 0.0 {
                    continue;
                }
                if factors.len() >= available {
                    return None;
                }
                factors.push(Factor {
                    sample: u32::try_from(*sample).unwrap_or(0),
                    value: factor,
                    transmission: 1.0,
                });
            }
        }
    }
    Some(factors)
}

fn refresh_source_visibility(
    source: &mut Source,
    visibility: &DynamicVisibility,
    cells: &[ReceiverCell],
    transmissions: &mut [f32],
    support: &mut [bool],
    has_casters: bool,
) -> (bool, u64) {
    if !source.state.active() || source.factors.is_empty() {
        return (false, 0);
    }
    transmissions.fill(1.0);
    support.fill(false);
    let mut region_queries = 0_u64;
    for factor in &source.factors {
        let Ok(index) = usize::try_from(factor.sample) else {
            continue;
        };
        if let Some(supported) = support.get_mut(index) {
            *supported = true;
        }
    }
    if has_casters {
        for cell in cells {
            if cell.bounds.is_empty()
                || !cell
                    .corners
                    .iter()
                    .any(|corner| support.get(*corner).copied().unwrap_or(false))
            {
                continue;
            }
            region_queries = region_queries.saturating_add(1);
            let visible = match source.kind {
                SourceKind::Local(_) => {
                    visibility.local_region_visible(source.bounds, cell.bounds, None)
                }
                SourceKind::Directional(incoming) => {
                    visibility.directional_region_visible(cell.bounds, incoming, None)
                }
            };
            if !visible {
                for corner in cell.corners {
                    if let Some(value) = transmissions.get_mut(corner) {
                        *value = 0.0;
                    }
                }
            }
        }
    }
    let mut changed = false;
    for factor in &mut source.factors {
        let transmission = usize::try_from(factor.sample)
            .ok()
            .and_then(|index| transmissions.get(index))
            .copied()
            .unwrap_or(0.0);
        changed |= transmission.to_bits() != factor.transmission.to_bits();
        factor.transmission = transmission;
    }
    (changed, region_queries)
}

impl ReceiverField {
    pub(super) fn build(
        level: &LevelDef,
        lighting: &LevelLighting,
        volumes: &[WaterVolume],
    ) -> Self {
        let grids = grids(volumes);
        let surfaces = LevelSurfaces::new(level);
        let (point_samples, point_receivers) =
            prepare_samples(volumes, &grids, &surfaces, lighting);
        let cells = prepare_cells(volumes, &grids, &point_receivers);
        let samples: Vec<_> = cells
            .iter()
            .flat_map(|cell| {
                cell.points.map(|point| {
                    point_samples
                        .get(point)
                        .copied()
                        .unwrap_or([0.0, 0.0, 0.0, f32::MAX])
                })
            })
            .collect();
        let receivers = cells
            .iter()
            .flat_map(|cell| {
                cell.points
                    .map(|point| point_receivers.get(point).copied().flatten())
            })
            .collect();
        let sample_count = samples.len();
        let receiver_bounds = cells
            .iter()
            .fold(Aabb::EMPTY, |bounds, cell| bounds.union(&cell.bounds));
        let mut field = Self {
            samples,
            grids,
            sources: local_sources(level, lighting),
            dropped_sources: 0,
            receivers,
            dynamic_revision: None,
            dynamic_queries: 0,
            dynamic_region_queries: 0,
            source_revision: 0,
            cells,
            receiver_bounds,
            dynamic_transmissions: vec![1.0; sample_count],
            dynamic_support: vec![false; sample_count],
        };
        field.sources.extend(
            level
                .global_illuminators
                .iter()
                .filter_map(|def| directional_source(def, lighting)),
        );
        let mut factor_count = 0_usize;
        for source in &mut field.sources {
            if !source.eligible {
                continue;
            }
            source.receiver_supported = volumes
                .iter()
                .any(|volume| source_reaches_volume(source, lighting, volume));
            if let Some(factors) = source_basis(
                source,
                lighting,
                volumes,
                &field.grids,
                &field.receivers,
                &field.cells,
                MAX_FACTORS.saturating_sub(factor_count),
            ) {
                factor_count = factor_count.saturating_add(factors.len());
                source.factors = factors;
            } else {
                source.receiver_supported = false;
                field.dropped_sources = field.dropped_sources.saturating_add(1);
            }
        }
        if field.dropped_sources > 0 {
            crate::engine::logging::warn_once(
                "caustics-source-budget",
                "[caustics] sparse source budget exceeded; unsupported sources fail closed (see caustics diagnostics)",
            );
        }
        field
    }

    pub(super) fn grid(&self, index: usize) -> [u32; 4] {
        self.grids.get(index).map_or([0; 4], |grid| grid.gpu())
    }

    pub(super) fn factor_count(&self) -> usize {
        self.sources.iter().map(|source| source.factors.len()).sum()
    }

    pub(super) fn has_active_sources(&self) -> bool {
        self.sources
            .iter()
            .any(|source| source.state.active() && source.receiver_supported)
    }

    pub(super) fn volume_active(&self, index: usize) -> bool {
        self.grids.get(index).is_some_and(|grid| {
            self.samples
                .get(grid.offset..grid.offset.saturating_add(grid.count))
                .is_some_and(|samples| {
                    samples
                        .iter()
                        .any(|sample| sample[..3].iter().any(|channel| *channel > 0.0))
                })
        })
    }

    pub(super) fn compose(&mut self) {
        for sample in &mut self.samples {
            *sample = [0.0, 0.0, 0.0, sample[3]];
        }
        for source in &self.sources {
            let energy = source.state.energy();
            if !energy.iter().any(|channel| *channel > 0.0) {
                continue;
            }
            for factor in &source.factors {
                if let Some(sample) = usize::try_from(factor.sample)
                    .ok()
                    .and_then(|index| self.samples.get_mut(index))
                {
                    for (channel, light) in sample.iter_mut().take(3).zip(energy) {
                        *channel = light.mul_add(factor.value * factor.transmission, *channel);
                    }
                }
            }
        }
    }

    pub(super) fn refresh_sources(&mut self, lighting: &LevelLighting) -> bool {
        let mut changed = false;
        let mut activated = false;
        let mut source_changed = false;
        for source in &mut self.sources {
            let SourceKind::Local(index) = source.kind else {
                continue;
            };
            let Some(light) = lighting.lights().get(index) else {
                continue;
            };
            let incoming = SourceState::local(light);
            if incoming == source.observed {
                continue;
            }
            let previous = source.state;
            if incoming.enabled != source.observed.enabled {
                source.state.enabled = incoming.enabled;
            }
            if incoming.intensity.to_bits() != source.observed.intensity.to_bits() {
                source.state.intensity = incoming.intensity;
            }
            if incoming.color.map(f32::to_bits) != source.observed.color.map(f32::to_bits) {
                source.state.color = incoming.color;
            }
            source.observed = incoming;
            source_changed |= source.eligible
                && previous.energy().map(f32::to_bits) != source.state.energy().map(f32::to_bits);
            changed |= !source.factors.is_empty()
                && previous.energy().map(f32::to_bits) != source.state.energy().map(f32::to_bits);
            activated |= !source.factors.is_empty() && !previous.active() && source.state.active();
        }
        if activated {
            self.dynamic_revision = None;
        }
        if source_changed {
            self.source_revision = self.source_revision.saturating_add(1);
        }
        changed
    }

    pub(super) fn inherit_state(&mut self, previous: &Self) {
        for (source, old) in self.sources.iter_mut().zip(&previous.sources) {
            if source.id == old.id {
                source.state = old.state;
            }
        }
        self.dynamic_revision = None;
        self.source_revision = previous.source_revision;
    }

    pub(super) fn set_source(
        &mut self,
        id: &str,
        enabled: Option<bool>,
        intensity: Option<f32>,
        color: Option<[f32; 3]>,
    ) -> Option<bool> {
        if intensity.is_some_and(|value| !value.is_finite() || !(0.0..=8.0).contains(&value))
            || color.is_some_and(|channels| {
                channels
                    .iter()
                    .any(|value| !value.is_finite() || !(0.0..=1.0).contains(value))
            })
        {
            return None;
        }
        let mut found = false;
        let mut changed = false;
        let mut activated = false;
        let mut source_changed = false;
        for source in &mut self.sources {
            if source.id != id {
                continue;
            }
            found = true;
            let state = SourceState {
                enabled: enabled.unwrap_or(source.state.enabled),
                intensity: intensity.map_or(source.state.intensity, |value| {
                    value * source.intensity_share
                }),
                color: color.unwrap_or(source.state.color),
            };
            changed |= !source.factors.is_empty()
                && source.state.energy().map(f32::to_bits) != state.energy().map(f32::to_bits);
            source_changed |= source.eligible
                && source.state.energy().map(f32::to_bits) != state.energy().map(f32::to_bits);
            activated |= !source.factors.is_empty() && !source.state.active() && state.active();
            source.state = state;
        }
        if activated {
            self.dynamic_revision = None;
        }
        if source_changed {
            self.source_revision = self.source_revision.saturating_add(1);
        }
        found.then_some(changed)
    }

    /// Reuses the already prepared movable visibility scene, with no model or
    /// world reconstruction. Stationary scenes and dark source sets perform no
    /// new geometry queries; moving scenes refresh complete region certificates.
    pub(super) fn refresh_dynamic_visibility(&mut self, visibility: &DynamicVisibility) -> bool {
        let revision = visibility.revision();
        if self.dynamic_revision == Some(revision) {
            return false;
        }
        let changed_bounds = self
            .dynamic_revision
            .and_then(|previous| visibility.changed_bounds_since(previous));
        self.dynamic_revision = Some(revision);
        let has_casters = visibility.stats().casters > 0;
        let mut changed = false;
        for source in &mut self.sources {
            if changed_bounds.is_some_and(|dirty| {
                dirty.is_empty()
                    || match source.kind {
                        SourceKind::Local(_) => !finite_light_region_overlaps(
                            source.bounds,
                            self.receiver_bounds,
                            dirty,
                        ),
                        SourceKind::Directional(incoming) => !directional_light_region_overlaps(
                            incoming,
                            self.receiver_bounds,
                            dirty,
                        ),
                    }
            }) {
                continue;
            }
            let (source_changed, regions) = refresh_source_visibility(
                source,
                visibility,
                &self.cells,
                &mut self.dynamic_transmissions,
                &mut self.dynamic_support,
                has_casters,
            );
            changed |= source_changed;
            self.dynamic_queries = self.dynamic_queries.saturating_add(regions);
            self.dynamic_region_queries = self.dynamic_region_queries.saturating_add(regions);
        }
        changed
    }

    pub(super) fn diagnostics(&self) -> Vec<serde_json::Value> {
        self.sources
            .iter()
            .map(|source| {
                let kind = match source.kind {
                    SourceKind::Local(_) => "local",
                    SourceKind::Directional(_) => "directional",
                };
                serde_json::json!({ "id": source.id, "kind": kind, "enabled": source.state.enabled,
                "intensity": source.state.intensity, "color": source.state.color,
                "resolved_intensity": source.state.intensity, "intensity_share": source.intensity_share,
                "factor_entries": source.factors.len(), "eligible": source.eligible,
                "active": source.state.active() && source.receiver_supported,
                "receiver_supported": source.receiver_supported,
                "energy": if source.eligible { source.state.energy() } else { [0.0; 3] } })
            })
            .collect()
    }
}
