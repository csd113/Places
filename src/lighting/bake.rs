//! The static bake and the queries it answers.
//!
//! [`LevelLighting::bake`] turns a level definition into room baselines, the
//! spatially varying baseline field that lets an internal partition isolate a
//! room's areas, one local pool per fixture, the bounded doorway blends between
//! areas, and the static geometry-visibility set that keeps a fixture from
//! lighting what it cannot see. Everything in this module runs once per level
//! load; the render loop only reads the vertex colours that were baked from it.
//!
//! Baseline field
//! --------------
//! A room whose footprint has no internal partition keeps the historical
//! single [`RoomLighting::baseline`]. A room whose footprint *is* split by
//! opaque walls into disconnected areas gets one baseline per connected area:
//! each area's fixture power is spread over that area's own floor area, so a
//! fixture cannot lend baseline brightness to a part of the room it cannot
//! reach. Connectivity is decided by the same wall-solid geometry the pools
//! use, probed just below the ceiling, so a full-height partition separates,
//! a door's header separates (the doorway blend still transmits locally), and
//! a wall that stops short of the ceiling does not. The two areas are still one
//! room for every other purpose: openings blend their baselines, the summary
//! reports one room, and surfaces keep sampling through the room they belong to.

use super::color::LightColor;
use super::light::{LightFalloff, LightShape, LightSource};
use super::math::{
    ceiling_height_factor, effective_power, fixture_half_extents_for, room_baseline, smooth_falloff,
};
use super::tuning::{
    AMBIENT_LEVEL, CLEAR_SAMPLE_MAX_STEPS, CLEAR_SAMPLE_STEP_M, FILL_MAX, FILL_RANGE_MULTIPLIER,
    FILL_STRENGTH, FIXTURE_DROP_M, LOCAL_LIGHT_MAX, LOCAL_LIGHT_STRENGTH, MAX_BRIGHTNESS,
    OPENING_BLEND_RADIUS_M, OPENING_BLEND_STRENGTH, OPENING_PROBE_M, OPENING_VERTICAL_FADE_M,
    REFERENCE_CEILING_HEIGHT_M, ROOM_EDGE_EPS_M, WALL_FACE_PROBE_M, WALL_LIGHT_DEFAULT_HEIGHT_M,
    ZONE_PROBE_DROP_M, ZONE_PROBE_MIN_ABOVE_FLOOR_M, ambient_color, fixture_profile,
};
use super::visibility::{Occluders, QuerySite, ShadowSampling, Visibility};
use crate::level::{
    CeilingProfileDef, LevelDef, LevelSurfaces, LightFixtureDef, LightMount, MAX_PROP_LIGHTS,
    WallAxis,
};
use crate::package::binary::{Reader, Writer};

/// Baked illumination information for one room.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoomLighting {
    /// Room footprint (normalised so `x0 <= x1`).
    pub x0: f32,
    pub x1: f32,
    pub z0: f32,
    pub z1: f32,
    /// World Y of the room's own floor plane.
    pub floor_y: f32,
    /// Clear eave height in metres (always positive): floor to `height`.
    ///
    /// This is the height the illumination model is calibrated against. A gable
    /// ridge adds shape, not brightness, so rooms that author the same `height`
    /// stay equally lit whether or not they have a pitched ceiling.
    pub height_m: f32,
    /// Ceiling profile of the room, sanitised for lookup.
    pub profile: CeilingProfileDef,
    /// Floor area in square metres.
    pub area_m2: f32,
    /// Number of ceiling fixtures owned by this room.
    pub fixture_count: usize,
    /// Summed emitted colour of the owned fixtures, each scaled by
    /// `intensity x ceiling-height factor`.
    pub effective_power: LightColor,
    /// Baked room-wide baseline illumination, every channel inside
    /// `[AMBIENT_LEVEL, BASELINE_MAX]`.
    ///
    /// This is the value every sample in an unpartitioned room gets, and the
    /// value the developer summary reports. It is deliberately the room-wide
    /// *fill*: local fixture pools are summed on top of it, up to
    /// `LOCAL_LIGHT_MAX`, and the two together are what the renderer's clamp
    /// sees. A room with internal partitions additionally carries a
    /// [`RoomZones`] field whose areas override it spatially; see
    /// [`LevelLighting::baseline_in_room`].
    pub baseline: LightColor,
}

/// One light resolved for baking: a generic [`LightSource`] plus the context
/// the bake resolved for it.
///
/// The source carries everything about *what the light is* (shape, position,
/// colour, intensity, range, falloff, enabled). The extra fields are what the
/// bake knows and the source does not: which room owns it, the ceiling
/// height correction that room applies, and whether the light is a recessed
/// ceiling fixture, whose direct pool is directional (it lights what is below
/// it, with a cosine incidence term) rather than an isotropic ball. Visible
/// fixture geometry is derived from the level definition, not from this record,
/// so a light with no fixture (a prop-attached source) and a fixture with no
/// light (an emissive-only sign) are both ordinary.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BakedLight {
    /// The engine-level light source, sanitised and positioned.
    pub source: LightSource,
    /// Ceiling-height correction of the owning room.
    pub height_factor: f32,
    /// Owning room, or `None` when no room contains the light.
    pub room: Option<usize>,
    /// True for a ceiling-mounted fixture: its direct pool only lights samples
    /// below the emitter and is weighted by the incidence cosine. Wall sconces
    /// and prop-attached lights keep the historical isotropic ball, because a
    /// horizontal cone would zero the floor beneath a sconce.
    pub directional: bool,
    /// Index into the level's `props` of the prop this light is attached to,
    /// or `None` for a ceiling or wall fixture.
    ///
    /// A prop-attached light is the source, so [`crate::lighting::Visibility`]
    /// exempts it from the occlusion boxes derived from its own prop's model;
    /// the light still sees every other solid, including other props.
    pub owner_prop: Option<usize>,
}

impl BakedLight {
    /// World X of the light's centre.
    #[must_use]
    pub const fn x(&self) -> f32 {
        self.source.position[0]
    }

    /// World Y of the light's emitting plane.
    #[must_use]
    pub const fn y(&self) -> f32 {
        self.source.position[1]
    }

    /// World Z of the light's centre.
    #[must_use]
    pub const fn z(&self) -> f32 {
        self.source.position[2]
    }

    /// Sanitised authored intensity.
    #[must_use]
    pub const fn intensity(&self) -> f32 {
        self.source.intensity
    }

    /// Sanitised emitted colour.
    #[must_use]
    pub const fn color(&self) -> LightColor {
        self.source.color
    }

    /// Half-extent of the emitting surface along world X, after rotation.
    #[must_use]
    pub const fn half_w(&self) -> f32 {
        self.source.half_extents().0
    }

    /// Half-extent of the emitting surface along world Z, after rotation.
    #[must_use]
    pub const fn half_d(&self) -> f32 {
        self.source.half_extents().1
    }

    /// Distance at which this light's contribution reaches zero, in metres.
    #[must_use]
    pub const fn range(&self) -> f32 {
        self.source.range
    }

    /// The light's falloff curve.
    #[must_use]
    pub const fn falloff(&self) -> LightFalloff {
        self.source.falloff
    }

    /// Whether this light casts environmental illumination at all.
    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.source.enabled
    }

    /// True when this light contributes illumination.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.source.is_active()
    }
}

/// Aggregate bake statistics, used for developer logging and tests.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct LightingSummary {
    pub rooms: usize,
    pub lights: usize,
    /// Opaque boxes the bake tests light against: wall solids plus the
    /// horizontal floor/ceiling slabs. See [`Visibility::blocker_count`].
    /// Static prop bodies are reported separately in [`Self::props`].
    pub blockers: usize,
    /// The wall-solid subset of [`Self::blockers`], for the developer log.
    pub walls: usize,
    /// Static prop occlusion boxes derived from the placed models' triangles.
    /// See [`Visibility::prop_blocker_count`].
    pub props: usize,
    /// Connected baseline areas across every room. An unpartitioned room
    /// contributes one, so this equals `rooms` for a fully open level.
    pub zones: usize,
    pub min_baseline: f32,
    pub max_baseline: f32,
    pub average_baseline: f32,
}

/// One connected baseline area of a partitioned room.
#[derive(Clone, Copy, Debug, PartialEq)]
struct ZoneLight {
    /// Baked baseline illumination of this area.
    baseline: LightColor,
    /// Floor area of this area, in square metres.
    area_m2: f32,
    /// Summed emitted colour of the fixtures inside this area.
    power: LightColor,
    /// Fixtures whose emitter lies inside this area.
    fixture_count: usize,
}

/// Developer view of one connected baseline area of a room.
///
/// The same data [`ZoneLight`] carries, exposed read-only so the developer
/// report and the calibration tests can print what each area of a partitioned
/// room was actually baked from. See [`LevelLighting::zones_in_room`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ZoneLighting {
    /// Floor area of this area, in square metres.
    pub area_m2: f32,
    /// Fixtures whose emitter lies inside this area.
    pub fixture_count: usize,
    /// Summed emitted colour of those fixtures, intensity and height factor
    /// included.
    pub power: LightColor,
    /// Baked baseline illumination of this area.
    pub baseline: LightColor,
}

/// The three additive terms of the baked model at one sample point.
///
/// A developer decomposition of [`LevelLighting::sample`] /
/// [`LevelLighting::sample_in_room`]: the value the renderer stores is
/// `clamp(baseline + pool + blend, AMBIENT_LEVEL, MAX_BRIGHTNESS)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BakeTerms {
    /// Room the sample resolved to, or `None` outside every room.
    pub room: Option<usize>,
    /// Room/area baseline at the sample position.
    pub baseline: LightColor,
    /// Summed visibility-tested local direct pool, screened against
    /// [`LOCAL_LIGHT_MAX`].
    pub pool: LightColor,
    /// Broad bounce fill from the same lights, screened against [`FILL_MAX`].
    pub fill: LightColor,
    /// Bounded doorway-blend delta.
    pub blend: LightColor,
    /// The clamped final value the bake returns.
    pub value: LightColor,
}

/// One fixture's contribution to a sample's local lighting, before the screen
/// composition.
///
/// The bake resolves the fixture's geometry once and records both the direct
/// pool's shape factor and the bounce fill's shape factor, so the developer
/// decomposition and the production composition cannot drift. The production
/// direct strength is `LOCAL_LIGHT_STRENGTH * intensity * height_factor *
/// direct_shape * visibility`, applied to the light's own `color` per channel;
/// the fill strength is the same with `FILL_STRENGTH` and `fill_shape`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PoolTerm {
    /// Index into [`LevelLighting::lights`].
    pub light: u32,
    /// Distance from the sample to the light's emitting surface, in metres.
    pub distance_m: f32,
    /// Horizontal distance from the sample to the emitting footprint, in
    /// metres (zero directly beneath it).
    pub horizontal_m: f32,
    /// Emitter height above the sample, in metres. Negative when the sample
    /// is above the emitter.
    pub vertical_m: f32,
    /// The light's sanitised authored intensity.
    pub intensity: f32,
    /// Ceiling-height correction of the light's owning room.
    pub height_factor: f32,
    /// Fraction of the light's emitting rectangle that sees the sample.
    pub visibility: f32,
    /// True when this light's direct pool is directional (a ceiling fixture).
    pub directional: bool,
    /// Direct shape factor in `0..=1`: `lateral * incidence` for a ceiling
    /// fixture, the authored falloff curve for an isotropic light.
    pub direct_shape: f32,
    /// Bounce-fill shape factor in `0..=1`: the smooth cushion at
    /// `distance_m / fill_range`. Zero beyond the fill's reach.
    pub fill_shape: f32,
    /// The light's emitted colour.
    pub color: LightColor,
}

impl PoolTerm {
    /// One screened term, per channel, with the light's colour preserved.
    ///
    /// A term's channels cap at `cap * colour[channel] / max_colour`: the cap
    /// scales with the light's own colour, so a single warm fixture reads warm
    /// all the way to the cap and never whitens. Each channel then claims a
    /// share of the headroom to *its own* cap, so a dim light adds its full
    /// energy, overlaps grow monotonically and sublinearly, and every channel
    /// depends only on its own contributions (blocking one fixture cannot
    /// change another fixture's channel).
    fn accumulate(pool: &mut [f32; 3], factor: f32, color: LightColor, cap: f32) {
        if !factor.is_finite() || factor <= 0.0 || !(cap.is_finite() && cap > 0.0) {
            return;
        }
        let peak = color.max_channel();
        if !peak.is_finite() || peak <= 0.0 {
            return;
        }
        for (channel, value) in [color.r, color.g, color.b].into_iter().enumerate() {
            let channel_cap = cap * value / peak;
            if !channel_cap.is_finite() || channel_cap <= 0.0 {
                continue;
            }
            let Some(slot) = pool.get_mut(channel) else {
                continue;
            };
            let claim = (factor * value).min(channel_cap) / channel_cap;
            *slot = ((channel_cap - *slot).max(0.0)).mul_add(claim, *slot);
        }
    }

    /// Adds this term's direct pool to the per-channel accumulators.
    fn screen_direct(&self, pool: &mut [f32; 3], strength: f32) {
        let factor =
            strength * self.intensity * self.height_factor * self.direct_shape * self.visibility;
        Self::accumulate(pool, factor, self.color, LOCAL_LIGHT_MAX);
    }

    /// Adds this term's bounce fill to the per-channel accumulators.
    fn screen_fill(&self, pool: &mut [f32; 3], strength: f32) {
        let factor =
            strength * self.intensity * self.height_factor * self.fill_shape * self.visibility;
        Self::accumulate(pool, factor, self.color, FILL_MAX);
    }
}

/// The spatial baseline field of one partitioned room.
///
/// The room footprint is tiled by the baked-lighting grid; every cell carries
/// the index of the connected area it belongs to, and every area carries the
/// baseline its own fixtures support. Lookups clamp into the room, so a sample
/// on the boundary resolves the nearest cell deterministically.
#[derive(Clone, Debug, PartialEq)]
struct RoomZones {
    edges_x: Vec<f32>,
    edges_z: Vec<f32>,
    zone_of_cell: Vec<u32>,
    zones: Vec<ZoneLight>,
}

impl RoomZones {
    const fn cells_x(&self) -> usize {
        self.edges_x.len().saturating_sub(1)
    }

    /// Area index containing `(x, z)`, clamped into the room footprint.
    fn zone_at(&self, x: f32, z: f32) -> Option<u32> {
        let ix = cell_index(&self.edges_x, x)?;
        let iz = cell_index(&self.edges_z, z)?;
        let index = iz.saturating_mul(self.cells_x()).saturating_add(ix);
        self.zone_of_cell.get(index).copied()
    }

    /// Baseline of the area containing `(x, z)`.
    fn baseline_at(&self, x: f32, z: f32) -> Option<LightColor> {
        let zone = self.zone_at(x, z)?;
        self.zones
            .get(usize::try_from(zone).ok()?)
            .map(|area| area.baseline)
    }
}

/// Index of the cell whose span contains `value` in a sorted edge list.
///
/// The grid tiles the whole room, so a value outside `[first, last]` clamps to
/// the nearest cell instead of being rejected: baseline lookups happen for
/// surface samples that sit exactly on a room boundary, where float noise can
/// put them a hair outside.
fn cell_index(edges: &[f32], value: f32) -> Option<usize> {
    let cells = edges.len().checked_sub(1)?;
    if cells == 0 || !value.is_finite() {
        return None;
    }
    let first = edges.first().copied().unwrap_or(0.0);
    let last = edges.last().copied().unwrap_or(0.0);
    let clamped_value = if first <= last {
        value.clamp(first, last)
    } else {
        first
    };
    let index = edges.partition_point(|edge| *edge <= clamped_value);
    Some(index.saturating_sub(1).min(cells.saturating_sub(1)))
}

/// World XZ of a cell's centre from its grid position.
fn cell_centre(edges_x: &[f32], edges_z: &[f32], cells_x: usize, cell: usize) -> (f32, f32) {
    let stride = cells_x.max(1);
    let ix = cell.checked_rem(stride).unwrap_or(0);
    let iz = cell.checked_div(stride).unwrap_or(0);
    let x = match (edges_x.get(ix), edges_x.get(ix.saturating_add(1))) {
        (Some(a), Some(b)) => f32::midpoint(*a, *b),
        _ => 0.0,
    };
    let z = match (edges_z.get(iz), edges_z.get(iz.saturating_add(1))) {
        (Some(a), Some(b)) => f32::midpoint(*a, *b),
        _ => 0.0,
    };
    (x, z)
}

/// World Y of the lowest point of a room's ceiling under a horizontal panel of
/// the given half extents, or `fallback` when no room owns the point.
///
/// Taking the minimum over the panel's corners is what keeps a fixture visibly
/// below a sloping ceiling: a panel near the eave hangs at the eave, a panel
/// near the ridge hangs at the ridge, and neither ever intersects the slope.
fn panel_min_ceiling_y(
    room: Option<&RoomLighting>,
    fallback: f32,
    x: f32,
    z: f32,
    half_w: f32,
    half_d: f32,
) -> f32 {
    let Some(room_index) = room else {
        return fallback;
    };
    let mut lowest = f32::INFINITY;
    for corner_x in [x - half_w, x + half_w] {
        for corner_z in [z - half_d, z + half_d] {
            lowest = lowest.min(room_index.ceiling_y_at(corner_x, corner_z));
        }
    }
    if lowest.is_finite() { lowest } else { fallback }
}

/// World Y of a wall-mounted fixture: the authored height when it is finite,
/// otherwise a safe height above the owning room's floor.
///
/// Both the bake and the fixture mesh call this, so the drawn luminaire and the
/// light pool it casts can never sit at different heights.
#[must_use]
fn resolve_wall_fixture_y(rooms: &[RoomLighting], x: f32, z: f32, authored: Option<f32>) -> f32 {
    if let Some(y) = authored
        && y.is_finite()
    {
        return y;
    }
    let floor_y = LevelLighting::room_index_of(rooms, x, z)
        .and_then(|index| rooms.get(index))
        .map_or(0.0, |room| room.floor_y);
    floor_y + WALL_LIGHT_DEFAULT_HEIGHT_M
}

/// A doorway/passage link between two room areas.
#[derive(Clone, Copy, Debug, PartialEq)]
struct OpeningBlend {
    x: f32,
    z: f32,
    /// Floor Y of the lower of the two areas the opening joins, used as the
    /// bottom of the aperture when testing whether the sample can see through.
    base_y: f32,
    /// Top edge of the opening in world Y.
    top_y: f32,
    /// Query site of this opening in [`LevelLighting::visibility`].
    site: u32,
    /// Baseline of the area on the other side of the opening.
    neighbor_baseline: LightColor,
    /// Baseline of the area this entry belongs to.
    own_baseline: LightColor,
    /// Area id within the entry's room, when that room is partitioned. `None`
    /// for a uniform room: the entry then applies to every sample of the room,
    /// exactly as it always did.
    own_zone: Option<u32>,
}

impl OpeningBlend {
    /// World point the opening's light is treated as coming from: the middle of
    /// the aperture. Only the visibility test uses it.
    const fn source(&self) -> [f32; 3] {
        [self.x, f32::midpoint(self.base_y, self.top_y), self.z]
    }
}

/// Conservative XZ candidates in original room order. Quantile boundaries keep
/// the fixed 16-by-16 table useful for sparse layouts without coordinate casts.
#[derive(Clone, Debug, Default)]
struct RoomIndex {
    x_edges: Vec<f32>,
    z_edges: Vec<f32>,
    cells: Vec<Vec<usize>>,
}

impl RoomIndex {
    fn new(rooms: &[RoomLighting]) -> Self {
        if rooms.len() <= 8
            || rooms.iter().any(|room| {
                [room.x0, room.x1, room.z0, room.z1]
                    .iter()
                    .any(|bound| !bound.is_finite())
            })
        {
            return Self {
                cells: vec![(0..rooms.len()).collect()],
                ..Self::default()
            };
        }
        let edges = |mut values: Vec<f32>| {
            values.sort_by(f32::total_cmp);
            (1_usize..16)
                .filter_map(|step| values.get(values.len().saturating_mul(step) / 16).copied())
                .collect::<Vec<_>>()
        };
        let x_edges = edges(rooms.iter().map(|room| room.x0 - ROOM_EDGE_EPS_M).collect());
        let z_edges = edges(rooms.iter().map(|room| room.z0 - ROOM_EDGE_EPS_M).collect());
        let mut cells = vec![Vec::new(); 256];
        for (index, room) in rooms.iter().enumerate() {
            let x0 = x_edges.partition_point(|edge| *edge <= room.x0 - ROOM_EDGE_EPS_M);
            let x1 = x_edges.partition_point(|edge| *edge <= room.x1 + ROOM_EDGE_EPS_M);
            let z0 = z_edges.partition_point(|edge| *edge <= room.z0 - ROOM_EDGE_EPS_M);
            let z1 = z_edges.partition_point(|edge| *edge <= room.z1 + ROOM_EDGE_EPS_M);
            for z in z0..=z1 {
                for x in x0..=x1 {
                    if let Some(cell) = cells.get_mut(z.saturating_mul(16).saturating_add(x)) {
                        cell.push(index);
                    }
                }
            }
        }
        Self {
            x_edges,
            z_edges,
            cells,
        }
    }

    fn candidates(&self, x: f32, z: f32) -> &[usize] {
        if !x.is_finite() || !z.is_finite() {
            return &[];
        }
        let cell = if self.cells.len() <= 1 {
            0
        } else {
            let x_partition = self.x_edges.partition_point(|edge| *edge <= x);
            let z_partition = self.z_edges.partition_point(|edge| *edge <= z);
            z_partition.saturating_mul(16).saturating_add(x_partition)
        };
        self.cells.get(cell).map_or(&[], Vec::as_slice)
    }
}

/// Global light candidates; each bucket preserves source order so screen
/// composition remains bit-identical after exact room membership filtering.
#[derive(Clone, Debug, Default)]
struct LightIndex {
    x_edges: Vec<f32>,
    z_edges: Vec<f32>,
    cells: Vec<Vec<u32>>,
}

impl LightIndex {
    fn new(lights: &[BakedLight]) -> Self {
        if lights.len() <= 8 {
            return Self {
                cells: vec![
                    (0..lights.len())
                        .filter_map(|index| u32::try_from(index).ok())
                        .collect(),
                ],
                ..Self::default()
            };
        }
        let edges = |mut values: Vec<f32>| {
            values.sort_by(f32::total_cmp);
            (1_usize..16)
                .filter_map(|step| values.get(values.len().saturating_mul(step) / 16).copied())
                .collect::<Vec<_>>()
        };
        let x_edges = edges(lights.iter().map(BakedLight::x).collect());
        let z_edges = edges(lights.iter().map(BakedLight::z).collect());
        let mut cells = vec![Vec::new(); 256];
        for (index, light) in lights.iter().enumerate() {
            let Ok(zone_index) = u32::try_from(index) else {
                continue;
            };
            let (half_w, half_d) = light.source.half_extents();
            let reach = fill_range_for(light.source.range);
            // Expand for the rounding of subtraction in pool_term_for. This
            // only admits extra candidates; the original test still decides.
            let axis = |center: f32, half: f32| {
                let extent = half + reach;
                let guard = ((center.abs() + extent) * f32::EPSILON).mul_add(8.0, f32::EPSILON);
                (center - extent - guard, center + extent + guard)
            };
            let (min_x, max_x) = axis(light.x(), half_w);
            let (min_z, max_z) = axis(light.z(), half_d);
            let x0 = x_edges.partition_point(|edge| *edge <= min_x);
            let x1 = x_edges.partition_point(|edge| *edge <= max_x);
            let z0 = z_edges.partition_point(|edge| *edge <= min_z);
            let z1 = z_edges.partition_point(|edge| *edge <= max_z);
            for z in z0..=z1 {
                for x in x0..=x1 {
                    if let Some(cell) = cells.get_mut(z.saturating_mul(16).saturating_add(x)) {
                        cell.push(zone_index);
                    }
                }
            }
        }
        Self {
            x_edges,
            z_edges,
            cells,
        }
    }

    fn candidates(&self, x: f32, z: f32) -> &[u32] {
        let cell = if self.cells.len() <= 1 {
            0
        } else {
            let x_partition = self.x_edges.partition_point(|edge| *edge <= x);
            let z_partition = self.z_edges.partition_point(|edge| *edge <= z);
            z_partition.saturating_mul(16).saturating_add(x_partition)
        };
        self.cells.get(cell).map_or(&[], Vec::as_slice)
    }
}

/// Fully baked static lighting for one level.
///
/// Cheap to keep resident (a few dozen bytes per room and fixture) and sampled
/// only while the level geometry is being built.
#[derive(Clone, Debug, Default)]
pub struct LevelLighting {
    rooms: Vec<RoomLighting>,
    room_index: RoomIndex,
    lights: Vec<BakedLight>,
    global_lights: Vec<super::directional::DirectionalLight>,
    light_index: LightIndex,
    /// Per room, the opening links that blend neighbouring light into it.
    blends: Vec<Vec<OpeningBlend>>,
    /// Per room, the spatial baseline field when opaque walls split the room
    /// into disconnected areas. `None` means the room is one connected volume
    /// and its samples use [`RoomLighting::baseline`] exactly as before.
    room_zones: Vec<Option<RoomZones>>,
    /// Static solid geometry, used to keep a fixture's local pool from lighting
    /// surfaces it cannot see. Built once per level load.
    visibility: Visibility,
    /// Per room, indices of the fixtures whose pool can reach that room, in
    /// fixture order. A fixture outside the list is farther than
    /// [`LOCAL_LIGHT_RADIUS_M`] from every point in the room, so pruning is
    /// exact and the per-vertex sum is unchanged.
    room_lights: Vec<Vec<u32>>,
    /// Every fixture index, for samples outside all rooms.
    all_lights: Vec<u32>,
    /// Per ceiling fixture, the light index it baked into, or `None` for a
    /// fixture with a non-finite position (which casts nothing).
    fixture_lights: Vec<Option<usize>>,
    /// Ceiling plane used for fixtures that no room contains.
    default_ceiling_y: f32,
    /// Emitter taps per axis for local-pool visibility, from the bake's
    /// [`BakeConfig`]. Stored as the plain byte so the empty [`Default`] bake
    /// stays derivable; `0` and `1` are hard shadows.
    sampling_taps: u8,
}

/// Everything the bake needs beyond the level definition.
///
/// [`Self::HARD`] is the historical bake: one visibility tap per fixture (a
/// binary shadow edge) and the historical 0.15 m prop-occlusion grid, so
/// [`LevelLighting::bake`] keeps every value it always had. The renderer
/// passes the active quality level's config through
/// [`LevelLighting::bake_with`], which buys the soft penumbra and the finer
/// prop occluders at Medium and High.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BakeConfig {
    /// How each local pool's visibility to a sample is resolved.
    pub sampling: ShadowSampling,
    /// Grid cell, in metres, a prop model's triangles are ground into for the
    /// bake's occlusion boxes.
    pub prop_occlusion_cell_m: f32,
}

impl BakeConfig {
    /// The historical bake: hard shadows and the historical prop grid.
    pub const HARD: Self = Self {
        sampling: ShadowSampling::HARD,
        prop_occlusion_cell_m: super::tuning::PROP_OCCLUSION_CELL_M,
    };
}

impl RoomLighting {
    /// World Y of this room's ceiling surface at `(x, z)`.
    #[must_use]
    pub fn ceiling_y_at(&self, x: f32, z: f32) -> f32 {
        crate::level::ceiling_y_for_volume(
            (self.x0, self.x1, self.z0, self.z1),
            self.floor_y,
            self.height_m,
            self.profile,
            x,
            z,
        )
    }

    /// Vertical span of the room's air at `(x, z)`: floor to ceiling.
    fn span_at(&self, x: f32, z: f32) -> (f32, f32) {
        let ceiling = self.ceiling_y_at(x, z);
        let ceiling_y = if ceiling.is_finite() {
            ceiling
        } else {
            self.floor_y + self.height_m
        };
        (self.floor_y, ceiling_y)
    }
}

/// A closed interval on the floor plane: `(min, max)`.
type Span = (f32, f32);

/// Horizontal reach of a light's bounce fill, in metres.
///
/// The fill is the room's first reflected light, so it is deliberately much
/// broader than the direct pool: [`FILL_RANGE_MULTIPLIER`] times the light's
/// own range. The multiplier is at least `1.0`, so the fill reach always
/// covers the direct pool's reach and the candidate prefilters can use it as
/// the single admission bound for both terms.
fn fill_range_for(range: f32) -> f32 {
    if !range.is_finite() {
        return 0.0;
    }
    let positive_range = range.max(0.0);
    let multiplier = if FILL_RANGE_MULTIPLIER.is_finite() {
        FILL_RANGE_MULTIPLIER.max(1.0)
    } else {
        2.0
    };
    positive_range * multiplier
}

/// How far apart two spans are on one axis, or zero when they touch or overlap.
fn interval_gap(room_span: Span, panel_span: Span) -> f32 {
    (room_span.0 - panel_span.1)
        .max(panel_span.0 - room_span.1)
        .max(0.0)
}

/// Bakes one [`RoomLighting`] per authored room, in level order.
///
/// Returns the rooms plus the default clear height and ceiling plane used for
/// fixtures that no room contains: the first room's values, or the historical
/// reference height for an empty level.
fn baked_rooms(level: &LevelDef) -> (Vec<RoomLighting>, f32, f32) {
    let room_refs: Vec<_> = level.room_iter().collect();
    let mut rooms: Vec<RoomLighting> = Vec::with_capacity(room_refs.len());
    for room in &room_refs {
        let x0 = room.x.min(room.x + room.width);
        let x1 = room.x.max(room.x + room.width);
        let z0 = room.z.min(room.z + room.depth);
        let z1 = room.z.max(room.z + room.depth);
        let width = if (x1 - x0).is_finite() {
            (x1 - x0).max(0.0)
        } else {
            0.0
        };
        let depth = if (z1 - z0).is_finite() {
            (z1 - z0).max(0.0)
        } else {
            0.0
        };
        let height_m = if room.height.is_finite() && room.height > 0.0 {
            room.height
        } else {
            REFERENCE_CEILING_HEIGHT_M
        };
        // A gable profile with a malformed rise behaves as flat, exactly
        // like `RoomDef::ceiling_y_at` would resolve it. An open ceiling
        // resolves its volume as a flat one: the room still participates in
        // the bake's fill and fixture span, it simply emits no ceiling
        // surface.
        let profile = match room.ceiling {
            crate::level::CeilingProfileDef::Gable {
                ridge_rise,
                ridge: _,
            } if !(ridge_rise.is_finite() && ridge_rise > 0.0) => {
                crate::level::CeilingProfileDef::Flat
            }
            crate::level::CeilingProfileDef::Open => crate::level::CeilingProfileDef::Flat,
            profile @ (crate::level::CeilingProfileDef::Flat
            | crate::level::CeilingProfileDef::Gable {
                ridge: _,
                ridge_rise: _,
            }) => profile,
        };
        rooms.push(RoomLighting {
            x0,
            x1,
            z0,
            z1,
            floor_y: if room.floor_y.is_finite() {
                room.floor_y
            } else {
                0.0
            },
            height_m,
            profile,
            area_m2: width * depth,
            fixture_count: 0,
            effective_power: LightColor::BLACK,
            baseline: ambient_color(),
        });
    }

    // Ceiling plane and clear height used for fixtures that no room
    // contains: the first room's values, or the historical reference height
    // for an empty level.
    let default_height_m = room_refs
        .first()
        .map_or(REFERENCE_CEILING_HEIGHT_M, |room| {
            if room.height.is_finite() && room.height > 0.0 {
                room.height
            } else {
                REFERENCE_CEILING_HEIGHT_M
            }
        });
    let default_ceiling_y = room_refs
        .first()
        .map_or(REFERENCE_CEILING_HEIGHT_M, |room| room.eave_y());

    (rooms, default_height_m, default_ceiling_y)
}

/// Every room whose footprint contains `(x, z)`, in level order.
fn room_candidates(rooms: &[RoomLighting], x: f32, z: f32) -> Vec<usize> {
    if !x.is_finite() || !z.is_finite() {
        return Vec::new();
    }
    rooms
        .iter()
        .enumerate()
        .filter(|(_, room)| {
            x >= room.x0 - ROOM_EDGE_EPS_M
                && x <= room.x1 + ROOM_EDGE_EPS_M
                && z >= room.z0 - ROOM_EDGE_EPS_M
                && z <= room.z1 + ROOM_EDGE_EPS_M
        })
        .map(|(index, _)| index)
        .collect()
}

/// Distance from `y` to a room's vertical span at `(x, z)`, zero when inside.
fn span_distance(room: &RoomLighting, x: f32, z: f32, y: f32) -> f32 {
    let (floor, ceiling) = room.span_at(x, z);
    if y < floor {
        floor - y
    } else if y > ceiling {
        y - ceiling
    } else {
        0.0
    }
}

/// The room that owns a light emitter at `(x, z)`.
///
/// `hint` is a world height the author gave the fixture (a wall fixture's `y`,
/// or the authored `y` of a ceiling fixture that mounts at a chosen height). It
/// picks the candidate whose vertical air volume contains that height, which is
/// what keeps a fixture on an upper floor from lending its power to the room
/// below it and vice versa. The smallest area breaks ties, exactly like every
/// other ownership rule in the bake, and an unauthored ceiling fixture keeps
/// the historical 2D rule.
fn resolve_light_room(
    rooms: &[RoomLighting],
    candidates: &[usize],
    x: f32,
    z: f32,
    hint: Option<f32>,
) -> Option<usize> {
    let smallest = |list: &[usize]| -> Option<usize> {
        list.iter().copied().min_by(|a, b| {
            let area_a = rooms.get(*a).map_or(f32::MAX, |room| room.area_m2);
            let area_b = rooms.get(*b).map_or(f32::MAX, |room| room.area_m2);
            area_a
                .partial_cmp(&area_b)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.cmp(b))
        })
    };
    let Some(hint_y) = hint.filter(|y| y.is_finite()) else {
        return smallest(candidates);
    };
    // Prefer the candidate whose air volume contains the hint. When none does
    // (a hand-edited height outside every room), the closest span wins so the
    // fixture still lands in the plausible room rather than dropping out.
    candidates
        .iter()
        .copied()
        .filter_map(|index| {
            let room = rooms.get(index)?;
            Some((span_distance(room, x, z, hint_y), room.area_m2, index))
        })
        .min_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
                .then(a.2.cmp(&b.2))
        })
        .map(|(_, _, index)| index)
        .or_else(|| smallest(candidates))
}

/// Resolves every light in a level once: ownership, world position, shape and
/// its contribution to its room's effective power.
///
/// Two authored forms converge here into one list of [`LightSource`]s:
///
/// 1. **Placed fixtures** (`ceiling_lights`): the visible fixture's family owns
///    the emitting shape, and the height resolves from the room's ceiling (or
///    the authored world `y` for a wall mount).
/// 2. **Prop-attached lights** (`props[].lights`): the level authors the shape
///    and the offset, the prop's own transform places it.
///
/// The list order is the bake order and the visibility-site order, so a light's
/// index is stable for the whole bake. Fixture lights come first, in level
/// order, then attached lights in prop order.
fn baked_lights(
    level: &LevelDef,
    rooms: &mut [RoomLighting],
    surfaces: Option<&LevelSurfaces<'_>>,
    default_height_m: f32,
    default_ceiling_y: f32,
    fixture_lights: &mut [Option<usize>],
) -> Vec<BakedLight> {
    let attached = level
        .props
        .iter()
        .map(|prop| prop.lights.len())
        .sum::<usize>();
    let mut lights: Vec<BakedLight> =
        Vec::with_capacity(level.ceiling_lights.len().saturating_add(attached));
    for (fixture_index, light) in level.ceiling_lights.iter().enumerate() {
        if !light.x.is_finite() || !light.z.is_finite() {
            continue;
        }
        let candidates = room_candidates(rooms, light.x, light.z);
        // A ceiling fixture only expresses a height when the author writes one;
        // a wall fixture always does, because the loader requires it.
        let hint = match light.mount {
            LightMount::Ceiling => light.y.filter(|y| y.is_finite()),
            LightMount::Wall => Some(resolve_wall_fixture_y(rooms, light.x, light.z, light.y)),
        };
        let room = resolve_light_room(rooms, &candidates, light.x, light.z, hint);
        // The height factor is calibrated against the room's eave, so a
        // gable ridge changes the ceiling's shape but not the room's
        // illumination response.
        let height_m = room
            .and_then(|index| rooms.get(index))
            .map_or(default_height_m, |info| info.height_m);
        let height_factor = ceiling_height_factor(height_m);
        let intensity = light.intensity();
        let color = light.emitted_color();
        // Rotation swaps the panel's long axis, exactly like the fixture
        // geometry emitted by `crate::render` (shared helper, so a
        // fractional rotation cannot drift between the two). The fixture
        // family owns the footprint, so a round downlight pools light in a
        // small disc while the office panel pools it over its rectangle.
        let profile = fixture_profile(&light.fixture);
        let shape = profile.shape();
        let (half_w, half_d) = shape.half_extents_rotated(light.rotation_degrees);
        let panel_y = match light.mount {
            LightMount::Ceiling => light.y.filter(|y| y.is_finite()).unwrap_or_else(|| {
                panel_min_ceiling_y(
                    room.and_then(|index| rooms.get(index)),
                    default_ceiling_y,
                    light.x,
                    light.z,
                    half_w,
                    half_d,
                ) - FIXTURE_DROP_M
            }),
            // A wall fixture is authored at its own world height; the
            // fallback only keeps a hand-edited level finite.
            LightMount::Wall => resolve_wall_fixture_y(rooms, light.x, light.z, light.y),
        };
        let source = LightSource {
            shape,
            position: [light.x, panel_y, light.z],
            rotation_degrees: light.rotation_degrees,
            color,
            intensity,
            range: light
                .range
                .unwrap_or(crate::lighting::DEFAULT_LIGHT_RANGE_M),
            falloff: light.falloff.unwrap_or_default(),
            enabled: light.enabled,
        }
        .sanitized();
        if let Some(info) = room.and_then(|index| rooms.get_mut(index)) {
            // Ownership is recorded for every owned light, active or not: the
            // count describes the room's fixtures, exactly as it always has.
            info.fixture_count = info.fixture_count.saturating_add(1);
            // A switchable fixture contributes no room baseline: the baseline is
            // a bake-time aggregate of always-on illumination, while a
            // switchable fixture is a local pool that comes and goes. Excluding
            // it keeps a runtime toggle exact (the room's aggregate never needs
            // re-deriving) and local (only its own pool's charts change).
            if source.enabled && !light.switchable {
                let power = effective_power(intensity, height_m);
                info.effective_power.r = power.mul_add(color.r, info.effective_power.r);
                info.effective_power.g = power.mul_add(color.g, info.effective_power.g);
                info.effective_power.b = power.mul_add(color.b, info.effective_power.b);
            }
        }
        if let Some(slot) = fixture_lights.get_mut(fixture_index) {
            *slot = Some(lights.len());
        }
        lights.push(BakedLight {
            source,
            height_factor,
            room,
            directional: matches!(light.mount, LightMount::Ceiling),
            owner_prop: None,
        });
    }

    // Lights owned by placed objects. `surfaces` is only absent when the level
    // has none, which is also when this cannot add anything.
    if let Some(level_surfaces) = surfaces {
        lights.extend(baked_attached_lights(
            level,
            rooms,
            level_surfaces,
            default_height_m,
        ));
    }

    lights
}

/// Resolves every light a placed object owns into a world-positioned source.
///
/// The offset is in the object's local frame, so the placement transform is the
/// same one the prop geometry uses: translate to the walkable floor plus the
/// authored offset, rotate about Y by the object's yaw, scale by the object's
/// scale. A source that lands in no room still exists — it lights whatever its
/// pool reaches, exactly like a fixture outside every room.
fn baked_attached_lights(
    level: &LevelDef,
    rooms: &mut [RoomLighting],
    surfaces: &LevelSurfaces<'_>,
    default_height_m: f32,
) -> Vec<BakedLight> {
    let mut lights: Vec<BakedLight> = Vec::new();
    for (prop_index, prop) in level.props.iter().enumerate() {
        if prop.lights.is_empty()
            || !prop.x.is_finite()
            || !prop.y.is_finite()
            || !prop.z.is_finite()
            || !prop.rotation_degrees.is_finite()
            || !prop.scale.is_finite()
            || prop.scale <= 0.0
        {
            continue;
        }
        let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
        let origin = [prop.x, base_y + prop.y, prop.z];
        let yaw = prop.rotation_degrees.to_radians();
        let (sin, cos) = yaw.sin_cos();
        for light in prop.lights.iter().take(MAX_PROP_LIGHTS) {
            let offset = light.offset;
            if !offset.iter().all(|value| value.is_finite()) {
                continue;
            }
            let scaled = [
                offset[0] * prop.scale,
                offset[1] * prop.scale,
                offset[2] * prop.scale,
            ];
            let position = [
                origin[0] + scaled[0].mul_add(cos, scaled[2] * sin),
                origin[1] + scaled[1],
                origin[2] + scaled[2].mul_add(cos, -scaled[0] * sin),
            ];
            let rotation = prop.rotation_degrees + light.rotation_degrees;
            let source = light.to_source(position, rotation, prop.scale).sanitized();
            let Some(room) = resolve_light_room(
                rooms,
                &room_candidates(rooms, position[0], position[2]),
                position[0],
                position[2],
                Some(position[1]),
            ) else {
                lights.push(BakedLight {
                    source,
                    height_factor: 1.0,
                    room: None,
                    directional: false,
                    owner_prop: Some(prop_index),
                });
                continue;
            };
            let height_m = rooms
                .get(room)
                .map_or(default_height_m, |info| info.height_m);
            let height_factor = ceiling_height_factor(height_m);
            if let Some(info) = rooms.get_mut(room) {
                info.fixture_count = info.fixture_count.saturating_add(1);
                if source.enabled {
                    let power = effective_power(source.intensity, height_m);
                    info.effective_power.r = power.mul_add(source.color.r, info.effective_power.r);
                    info.effective_power.g = power.mul_add(source.color.g, info.effective_power.g);
                    info.effective_power.b = power.mul_add(source.color.b, info.effective_power.b);
                }
            }
            lights.push(BakedLight {
                source,
                height_factor,
                room: Some(room),
                directional: false,
                owner_prop: Some(prop_index),
            });
        }
    }
    lights
}

/// Per-room fixture candidates: only fixtures whose panel can come within
/// [`LOCAL_LIGHT_RADIUS_M`] of the room footprint, always including the owning
/// room. Built in fixture order so the per-vertex sum (and its early
/// saturation) is bit-identical to checking every fixture.
fn room_light_candidates(lights: &[BakedLight], rooms: &[RoomLighting]) -> Vec<Vec<u32>> {
    let mut room_lights: Vec<Vec<u32>> = vec![Vec::new(); rooms.len()];
    for (index, light) in lights.iter().enumerate() {
        if !light.is_active() {
            continue;
        }
        let slot = u32::try_from(index).unwrap_or(u32::MAX);
        for (room_index, room) in rooms.iter().enumerate() {
            if (light.room == Some(room_index) || LevelLighting::light_reaches_room(light, room))
                && let Some(list) = room_lights.get_mut(room_index)
            {
                list.push(slot);
            }
        }
    }
    room_lights
}

/// True when a wall separates the two cells of a room strongly enough that
/// their baseline illumination must not be shared.
///
/// The probe runs across the *middle half* of the line between the two cell
/// centres, at three parallel offsets along their shared edge. The inset keeps
/// a segment from merely grazing a wall face at its endpoint (which would
/// over-segment a room whose cell centres sit exactly on a wall), and the three
/// offsets keep a wall that happens to cross a cell centre from severing the
/// cell in two.
fn cells_connected(
    room: &RoomLighting,
    occluders: &Occluders,
    edges_x: &[f32],
    edges_z: &[f32],
    cells_x: usize,
    a: usize,
    b: usize,
) -> bool {
    let (ax, az) = cell_centre(edges_x, edges_z, cells_x, a);
    let (bx, bz) = cell_centre(edges_x, edges_z, cells_x, b);
    let lower_ceiling = room.ceiling_y_at(ax, az).min(room.ceiling_y_at(bx, bz));
    let probe_y =
        (lower_ceiling - ZONE_PROBE_DROP_M).max(room.floor_y + ZONE_PROBE_MIN_ABOVE_FLOOR_M);
    if !probe_y.is_finite() {
        // A malformed room must never be split by a non-finite probe.
        return true;
    }
    let delta_x = bx - ax;
    let delta_z = bz - az;
    let length = delta_x.hypot(delta_z);
    if !length.is_finite() || length <= f32::EPSILON {
        return true;
    }
    // Middle half of the connection, so an endpoint can never land on a wall
    // face and count as a crossing.
    let start_x = delta_x.mul_add(0.25, ax);
    let start_z = delta_z.mul_add(0.25, az);
    let end_x = (ax - bx).mul_add(0.25, bx);
    let end_z = (az - bz).mul_add(0.25, bz);
    // Offset along the shared edge, a quarter of the centre distance either way.
    let normal_x = -delta_z / length;
    let normal_z = delta_x / length;
    let offset = length * 0.25;
    for step in [-1.0_f32, 0.0, 1.0] {
        let shift_x = normal_x * offset * step;
        let shift_z = normal_z * offset * step;
        if !occluders.walls_block(
            [start_x + shift_x, probe_y, start_z + shift_z],
            [end_x + shift_x, probe_y, end_z + shift_z],
        ) {
            return true;
        }
    }
    false
}

/// Builds the spatial baseline field of every room that internal walls split
/// into disconnected areas.
///
/// Returns `None` for a room that stays a single connected volume, which is
/// every unpartitioned room and therefore every level written before internal
/// partitions mattered: such a room keeps its exact historical baseline.
///
/// The grid is the finer *zone* grid ([`crate::lighting::ZONE_GRID_CELL_M`]),
/// not the light grid: a narrow air region (a 1.4 m corridor between two
/// partition walls) must own a cell centre of its own, or the samples in it
/// would be assigned to whichever unrelated zone's coarse cell they happened
/// to fall in and draw a baseline seam across open floor.
///
/// Only cells whose centres stand in air take part in the flood fill; a cell
/// whose centre is inside a wall is not a sampleable place and must never
/// bridge two areas (the historical fill walked through such cells). After the
/// fill, every wall cell inherits the zone of the nearest air cell, in a
/// deterministic scan, so a baseline lookup on a sample that lands on a wall
/// cell still resolves the area it borders.
fn room_baseline_zones(
    room_index: usize,
    room: &RoomLighting,
    lights: &[BakedLight],
    occluders: &Occluders,
) -> Option<RoomZones> {
    let width = room.x1 - room.x0;
    let depth = room.z1 - room.z0;
    let cells_x = super::math::zone_grid_cells(width);
    let cells_z = super::math::zone_grid_cells(depth);
    let columns = usize::try_from(cells_x).ok()?;
    let rows = usize::try_from(cells_z).ok()?;
    let cell_count = columns.checked_mul(rows)?;
    if cell_count <= 1 || !occluders.may_partition(room) {
        return None;
    }
    let edges_x = crate::level::axis_positions(room.x0, width, cells_x);
    let edges_z = crate::level::axis_positions(room.z0, depth, cells_z);

    // A cell is "air" when its own centre is not inside a wall solid. The
    // flood fill only visits air cells, so a wall can never connect the areas
    // on either side of it through a cell centre buried in its thickness.
    let air: Vec<bool> = (0..cell_count)
        .map(|cell| {
            let (x, z) = cell_centre(&edges_x, &edges_z, columns, cell);
            !occluders.contains_point(x, z)
        })
        .collect();
    if !air.iter().any(|is_air| *is_air) {
        return None;
    }

    let mut zone_of_cell =
        flood_fill_room(room, occluders, &edges_x, &edges_z, &air, columns, rows);
    let zone_count = zone_of_cell
        .iter()
        .enumerate()
        .filter(|(index, _)| air.get(*index).copied().unwrap_or(false))
        .map(|(_, zone)| *zone)
        .max()
        .map_or(0, |zone| {
            usize::try_from(zone).unwrap_or(0).saturating_add(1)
        });
    if zone_count <= 1 {
        return None;
    }
    inherit_wall_zones(&mut zone_of_cell, &air, columns);

    let mut zones: Vec<ZoneLight> = vec![
        ZoneLight {
            baseline: ambient_color(),
            area_m2: 0.0,
            power: LightColor::BLACK,
            fixture_count: 0,
        };
        zone_count
    ];
    accumulate_zone_areas(&mut zones, &zone_of_cell, &air, &edges_x, &edges_z, columns);
    accumulate_zone_power(
        &mut zones,
        &zone_of_cell,
        &edges_x,
        &edges_z,
        columns,
        room_index,
        lights,
    );
    for entry in &mut zones {
        entry.baseline = room_baseline(entry.area_m2, entry.power);
    }

    Some(RoomZones {
        edges_x,
        edges_z,
        zone_of_cell,
        zones,
    })
}

/// Flood fills the air cells across every edge no wall blocks.
///
/// Cells whose centres are inside a wall are never visited, and never start a
/// component: they are not places a surface sample can stand. The scan order
/// and neighbour order are fixed, so the component numbering is deterministic
/// for a given level.
fn flood_fill_room(
    room: &RoomLighting,
    occluders: &Occluders,
    edges_x: &[f32],
    edges_z: &[f32],
    air: &[bool],
    columns: usize,
    rows: usize,
) -> Vec<u32> {
    let cell_count = columns.saturating_mul(rows);
    let mut zone_of_cell: Vec<u32> = vec![u32::MAX; cell_count];
    let mut zone_count: u32 = 0;
    let mut queue: Vec<usize> = Vec::new();
    for start in 0..cell_count {
        if zone_of_cell.get(start).copied() != Some(u32::MAX) {
            continue;
        }
        if !air.get(start).copied().unwrap_or(false) {
            continue;
        }
        let zone_id = zone_count;
        zone_count = zone_count.saturating_add(1);
        queue.clear();
        queue.push(start);
        if let Some(slot) = zone_of_cell.get_mut(start) {
            *slot = zone_id;
        }
        while let Some(cell) = queue.pop() {
            let ix = cell.checked_rem(columns).unwrap_or(0);
            let iz = cell.checked_div(columns).unwrap_or(0);
            let mut visit = |neighbor: usize| {
                if !air.get(neighbor).copied().unwrap_or(false) {
                    return;
                }
                if zone_of_cell.get(neighbor).copied() != Some(u32::MAX) {
                    return;
                }
                if !cells_connected(room, occluders, edges_x, edges_z, columns, cell, neighbor) {
                    return;
                }
                if let Some(slot) = zone_of_cell.get_mut(neighbor) {
                    *slot = zone_id;
                }
                queue.push(neighbor);
            };
            if ix > 0 {
                visit(cell.saturating_sub(1));
            }
            if ix.saturating_add(1) < columns {
                visit(cell.saturating_add(1));
            }
            if iz > 0 {
                visit(cell.saturating_sub(columns));
            }
            if iz.saturating_add(1) < rows {
                visit(cell.saturating_add(columns));
            }
        }
    }
    zone_of_cell
}

/// Gives every non-air cell the zone of the nearest air cell.
///
/// The search is a full deterministic scan of the air cells, taking the
/// smallest squared centre distance and keeping the earliest air cell on a
/// tie, so a wall cell always resolves the same area on every load. `columns`
/// is the row stride.
fn inherit_wall_zones(zone_of_cell: &mut [u32], air: &[bool], columns: usize) {
    let air_cells: Vec<(usize, u32)> = zone_of_cell
        .iter()
        .enumerate()
        .filter(|(index, _)| air.get(*index).copied().unwrap_or(false))
        .map(|(index, zone)| (index, *zone))
        .collect();
    if air_cells.is_empty() {
        return;
    }
    for (cell, zone) in zone_of_cell.iter_mut().enumerate() {
        if air.get(cell).copied().unwrap_or(false) {
            continue;
        }
        let ix = cell.checked_rem(columns).unwrap_or(0);
        let iz = cell.checked_div(columns).unwrap_or(0);
        let mut best: Option<(usize, u32)> = None;
        for (air_cell, air_zone) in &air_cells {
            let ax = air_cell.checked_rem(columns).unwrap_or(0);
            let az = air_cell.checked_div(columns).unwrap_or(0);
            let dx = ix.abs_diff(ax);
            let dz = iz.abs_diff(az);
            let distance = dx.saturating_mul(dx).saturating_add(dz.saturating_mul(dz));
            match best {
                Some((best_distance, _)) if best_distance <= distance => {}
                _ => best = Some((distance, *air_zone)),
            }
        }
        if let Some((_, air_zone)) = best {
            *zone = air_zone;
        }
    }
}

/// Adds up each connected area's floor area over the exact air cells it owns.
///
/// A cell whose centre stands in a wall is not floor: it contributes no area,
/// so the density curve sees the area a fixture can actually light.
fn accumulate_zone_areas(
    zones: &mut [ZoneLight],
    zone_of_cell: &[u32],
    air: &[bool],
    edges_x: &[f32],
    edges_z: &[f32],
    columns: usize,
) {
    for (iz, z_span) in edges_z.windows(2).enumerate() {
        let &[z0, z1] = z_span else {
            continue;
        };
        let row_depth = z1 - z0;
        for (ix, x_span) in edges_x.windows(2).enumerate() {
            let &[x0, x1] = x_span else {
                continue;
            };
            let index = iz.saturating_mul(columns).saturating_add(ix);
            if !air.get(index).copied().unwrap_or(false) {
                continue;
            }
            let Some(&zone) = zone_of_cell.get(index) else {
                continue;
            };
            let Some(entry) = zones.get_mut(usize::try_from(zone).unwrap_or(usize::MAX)) else {
                continue;
            };
            entry.area_m2 = (x1 - x0).mul_add(row_depth, entry.area_m2);
        }
    }
}

/// Adds up each area's fixture power, in fixture order so a zone's sum is built
/// with the same order (and `mul_add` accumulation) as the room-wide power.
///
/// A fixture whose own cell centre is inside a wall is assigned to the nearest
/// air cell's area (the same inheritance [`inherit_wall_zones`] resolved), so a
/// fixture mounted on a partition still powers the area it hangs over.
fn accumulate_zone_power(
    zones: &mut [ZoneLight],
    zone_of_cell: &[u32],
    edges_x: &[f32],
    edges_z: &[f32],
    columns: usize,
    room_index: usize,
    lights: &[BakedLight],
) {
    for light in lights {
        if light.room != Some(room_index) || !light.is_active() {
            continue;
        }
        let Some(ix) = cell_index(edges_x, light.x()) else {
            continue;
        };
        let Some(iz) = cell_index(edges_z, light.z()) else {
            continue;
        };
        let index = iz.saturating_mul(columns).saturating_add(ix);
        let Some(zone) = zone_of_cell.get(index).copied() else {
            continue;
        };
        if zone == u32::MAX {
            continue;
        }
        let Some(entry) = zones.get_mut(usize::try_from(zone).unwrap_or(usize::MAX)) else {
            continue;
        };
        let power = light.intensity() * light.height_factor;
        entry.power.r = power.mul_add(light.color().r, entry.power.r);
        entry.power.g = power.mul_add(light.color().g, entry.power.g);
        entry.power.b = power.mul_add(light.color().b, entry.power.b);
        entry.fixture_count = entry.fixture_count.saturating_add(1);
    }
}

/// One side of an opening: the room area the probe point falls in, its area id
/// (when the room is partitioned) and that area's baseline.
struct OpeningSide {
    room: usize,
    zone: Option<u32>,
    baseline: LightColor,
}

/// Resolves the room area at `(x, z)` and the baseline a sample there receives.
fn opening_side(
    rooms: &[RoomLighting],
    room_zones: &[Option<RoomZones>],
    x: f32,
    z: f32,
) -> Option<OpeningSide> {
    let room = LevelLighting::room_index_of(rooms, x, z)?;
    let zones = room_zones.get(room).and_then(Option::as_ref);
    let zone = zones.and_then(|selected_zones| selected_zones.zone_at(x, z));
    let baseline = zones
        .and_then(|selected_zones| selected_zones.baseline_at(x, z))
        .unwrap_or_else(|| {
            rooms
                .get(room)
                .map_or_else(ambient_color, |info| info.baseline)
        });
    Some(OpeningSide {
        room,
        zone,
        baseline,
    })
}

/// Links the room areas on either side of every walk-through opening.
///
/// Returns the per-room blend lists plus the query sites of the openings, in
/// the order they must be appended after the fixture sites in the visibility
/// set.
fn opening_blends(
    level: &LevelDef,
    rooms: &[RoomLighting],
    room_zones: &[Option<RoomZones>],
    light_site_count: u32,
) -> (Vec<Vec<OpeningBlend>>, Vec<QuerySite>) {
    let mut blends: Vec<Vec<OpeningBlend>> = vec![Vec::new(); rooms.len()];
    let mut blend_sites: Vec<QuerySite> = Vec::new();
    for wall in &level.walls {
        let length = wall.length();
        if !length.is_finite() || length <= 0.0 {
            continue;
        }
        let axis = wall.axis();
        let (origin_x, origin_z) = wall.length_origin();
        let (t0, t1) = match axis {
            WallAxis::X => (
                wall.z.min(wall.z + wall.depth),
                wall.z.max(wall.z + wall.depth),
            ),
            WallAxis::Z => (
                wall.x.min(wall.x + wall.width),
                wall.x.max(wall.x + wall.width),
            ),
        };
        let half_thickness = (t1 - t0).abs() * 0.5;
        for opening in &wall.openings {
            if !opening.is_door() {
                continue;
            }
            // Only openings the geometry actually cuts count as passages:
            // the same guards `wall_solid_slices` uses, so a zero-width or
            // non-finite opening cannot blend light through a solid wall.
            if !opening.offset.is_finite()
                || !opening.width.is_finite()
                || !opening.height.is_finite()
                || !opening.sill.is_finite()
                || opening.width <= 0.0
                || opening.height <= 0.0
            {
                continue;
            }
            let center = opening
                .width
                .mul_add(0.5, opening.offset)
                .clamp(0.0, length);
            let across = f32::midpoint(t0, t1);
            let probe = half_thickness + OPENING_PROBE_M;
            let (center_x, center_z) = match axis {
                WallAxis::X => (origin_x + center, across),
                WallAxis::Z => (across, origin_z + center),
            };
            let (side_a, side_b) = match axis {
                WallAxis::X => ((center_x, center_z + probe), (center_x, center_z - probe)),
                WallAxis::Z => ((center_x + probe, center_z), (center_x - probe, center_z)),
            };
            let (Some(a), Some(b)) = (
                opening_side(rooms, room_zones, side_a.0, side_a.1),
                opening_side(rooms, room_zones, side_b.0, side_b.1),
            ) else {
                continue;
            };
            // Two probes inside the same connected area are not separated by
            // anything: an opening in a stub wall does not create a link.
            if a.room == b.room && a.zone == b.zone {
                continue;
            }
            let (Some(info_a), Some(info_b)) = (rooms.get(a.room), rooms.get(b.room)) else {
                continue;
            };
            // A walk-through opening has to reach the floor it connects: a
            // wall raised off the floor (`wall.y`) is a header or lintel,
            // not a passage, and an opening that only reaches an upper
            // room's floor does not join the two rooms for light either.
            let floor = info_a.floor_y.min(info_b.floor_y);
            if wall.y + opening.sill.max(0.0) > floor + 1e-3 {
                continue;
            }
            let top_y = opening.top(wall.y);
            let site = light_site_count
                .saturating_add(u32::try_from(blend_sites.len()).unwrap_or(u32::MAX));
            blend_sites.push(QuerySite::new(center_x, center_z, OPENING_BLEND_RADIUS_M));
            let blend = OpeningBlend {
                x: center_x,
                z: center_z,
                base_y: floor,
                top_y,
                site,
                neighbor_baseline: b.baseline,
                own_baseline: a.baseline,
                own_zone: (a.zone != b.zone).then_some(a.zone).flatten(),
            };
            if let Some(list) = blends.get_mut(a.room) {
                list.push(blend);
            }
            if let Some(list) = blends.get_mut(b.room) {
                list.push(OpeningBlend {
                    neighbor_baseline: a.baseline,
                    own_baseline: b.baseline,
                    own_zone: (a.zone != b.zone).then_some(b.zone).flatten(),
                    ..blend
                });
            }
        }
    }
    (blends, blend_sites)
}

impl LevelLighting {
    /// Resident vector allocations; used to bound immutable prepared-build reuse.
    #[must_use]
    pub(crate) fn retained_heap_bytes(&self) -> usize {
        let flat = [
            allocation_bytes(&self.rooms),
            allocation_bytes(&self.lights),
            allocation_bytes(&self.global_lights),
            allocation_bytes(&self.blends),
            allocation_bytes(&self.room_zones),
            allocation_bytes(&self.room_lights),
            allocation_bytes(&self.all_lights),
            allocation_bytes(&self.room_index.x_edges),
            allocation_bytes(&self.room_index.z_edges),
            allocation_bytes(&self.room_index.cells),
            allocation_bytes(&self.light_index.x_edges),
            allocation_bytes(&self.light_index.z_edges),
            allocation_bytes(&self.light_index.cells),
            self.visibility.retained_heap_bytes(),
        ]
        .into_iter()
        .fold(0_usize, usize::saturating_add);
        let nested = self
            .blends
            .iter()
            .map(allocation_bytes)
            .chain(self.room_lights.iter().map(allocation_bytes))
            .chain(self.room_index.cells.iter().map(allocation_bytes))
            .chain(self.light_index.cells.iter().map(allocation_bytes))
            .fold(flat, usize::saturating_add);
        self.room_zones
            .iter()
            .flatten()
            .fold(nested, |bytes, zones| {
                [
                    allocation_bytes(&zones.edges_x),
                    allocation_bytes(&zones.edges_z),
                    allocation_bytes(&zones.zone_of_cell),
                    allocation_bytes(&zones.zones),
                ]
                .into_iter()
                .fold(bytes, usize::saturating_add)
            })
    }

    /// Bakes room baselines, fixture pools and opening blends from a level.
    ///
    /// Malformed data never panics and never yields NaN: non-finite fixtures are
    /// skipped, non-finite dimensions fall back to safe values and every result
    /// is clamped.
    ///
    /// This is [`Self::bake_with`] at [`BakeConfig::HARD`]: one visibility tap
    /// per fixture and the historical prop-occlusion grid, i.e. the bake every
    /// value this engine ever produced came from.
    #[must_use]
    pub fn bake(level: &LevelDef) -> Self {
        Self::bake_with(level, BakeConfig::HARD)
    }

    /// [`Self::bake`] with an explicit [`BakeConfig`]: the same model, baked
    /// with the active profile's visibility sampling and prop-occlusion grid.
    ///
    /// The bake is a pure function of the level definition *and* `config`:
    /// [`BakeConfig::HARD`] reproduces every value [`Self::bake`] produces.
    #[must_use]
    pub fn bake_with(level: &LevelDef, config: BakeConfig) -> Self {
        // The occluder set is built first: the baseline field's partition
        // connectivity asks it whether a wall separates two cells, and the
        // doorway blends then add their own query sites to it.
        let occluders = Occluders::build_with(level, config.prop_occlusion_cell_m);

        let (mut rooms, default_height_m, default_ceiling_y) = baked_rooms(level);

        // Resolve every light once: fixtures and prop-attached sources converge
        // into one list of generic light sources, each with its owning room and
        // its contribution to that room's effective power. Prop lights need the
        // walkable floor at their prop's position, which is the same lookup the
        // prop geometry uses; a level that attaches no lights to props (which is
        // every level authored before the generic model existed) skips building
        // that lookup entirely and bakes exactly as it always did.
        let mut fixture_lights: Vec<Option<usize>> = vec![None; level.ceiling_lights.len()];
        let lights = if level.props.iter().any(|prop| !prop.lights.is_empty()) {
            let surfaces = LevelSurfaces::new(level);
            baked_lights(
                level,
                &mut rooms,
                Some(&surfaces),
                default_height_m,
                default_ceiling_y,
                &mut fixture_lights,
            )
        } else {
            baked_lights(
                level,
                &mut rooms,
                None,
                default_height_m,
                default_ceiling_y,
                &mut fixture_lights,
            )
        };

        // A room split by internal walls gets one baseline per connected area;
        // an open room stays a single uniform baseline, bit for bit.
        let room_zones: Vec<Option<RoomZones>> = rooms
            .iter()
            .enumerate()
            .map(|(index, room)| room_baseline_zones(index, room, &lights, &occluders))
            .collect();

        for room in &mut rooms {
            room.baseline = room_baseline(room.area_m2, room.effective_power);
        }

        let room_lights = room_light_candidates(&lights, &rooms);
        let all_lights: Vec<u32> = (0..u32::try_from(lights.len()).unwrap_or(u32::MAX)).collect();

        // Link the areas on either side of every walk-through opening.
        let light_site_count = u32::try_from(lights.len()).unwrap_or(u32::MAX);
        let (blends, blend_sites) = opening_blends(level, &rooms, &room_zones, light_site_count);

        // Opaque geometry for the whole bake. Query sites are the fixtures
        // first (one per light, in fixture order) and then the doorway blends,
        // so a fixture's site index is exactly its own index.
        let mut sites: Vec<QuerySite> =
            Vec::with_capacity(lights.len().saturating_add(blend_sites.len()));
        for light in &lights {
            let (half_w, half_d) = light.source.half_extents();
            // The admission test measures from the emitter rectangle, so the
            // site has to cover the rectangle's far corner too; `for_emitter`
            // adds `hypot(half_w, half_d)` and uses the pool's full reach
            // (the bounce fill extends past the direct range).
            sites.push(
                QuerySite::for_emitter(
                    light.x(),
                    light.z(),
                    fill_range_for(light.source.range),
                    half_w,
                    half_d,
                )
                .with_owner_prop(light.owner_prop),
            );
        }
        sites.extend_from_slice(&blend_sites);
        let visibility = Visibility::build_with_occluders(occluders, &sites);

        Self {
            room_index: RoomIndex::new(&rooms),
            rooms,
            light_index: LightIndex::new(&lights),
            lights,
            global_lights: level
                .global_illuminators
                .iter()
                .filter_map(super::directional::DirectionalLight::from_definition)
                .collect(),
            blends,
            room_zones,
            visibility,
            room_lights,
            all_lights,
            fixture_lights,
            default_ceiling_y,
            sampling_taps: config.sampling.taps_per_axis,
        }
    }

    /// Baked rooms, in the level's room order.
    #[must_use]
    pub fn rooms(&self) -> &[RoomLighting] {
        &self.rooms
    }

    /// Baked fixtures, in the level's ceiling-light order (minus non-finite ones).
    #[must_use]
    pub fn lights(&self) -> &[BakedLight] {
        &self.lights
    }

    /// Directional direct light at a surface on the vertex-lit path. Origins
    /// use geometric separation, and infinite rays have no range falloff.
    #[must_use]
    pub fn global_surface_light(&self, position: [f32; 3], normal: [f32; 3]) -> LightColor {
        let origin = super::transport::receiver_position(position, normal);
        let mut color = LightColor::BLACK;
        for light in &self.global_lights {
            let cosine = normal
                .iter()
                .zip(light.incoming)
                .map(|(a, b)| a * b)
                .sum::<f32>()
                .max(0.0);
            if cosine <= 0.0
                || (light.cast_shadows
                    && self.visibility.occludes_direction(origin, light.incoming))
            {
                continue;
            }
            color = color.plus(LightColor::rgb(
                light.color[0] * light.intensity * cosine,
                light.color[1] * light.intensity * cosine,
                light.color[2] * light.intensity * cosine,
            ));
        }
        color
    }

    /// The light index one ceiling fixture baked into, if it casts light.
    ///
    /// `fixture_index` is the fixture's position in the level's
    /// `ceiling_lights` array; `None` means the fixture has a non-finite
    /// position (loader-rejected, so unreachable for a loaded level).
    #[must_use]
    pub fn fixture_light_index(&self, fixture_index: usize) -> Option<usize> {
        self.fixture_lights.get(fixture_index).copied().flatten()
    }

    /// Enables or disables one baked light at runtime.
    ///
    /// Returns whether the state changed. A disabled light contributes nothing
    /// to any later sample, so the caller refreshes the resident lightmap
    /// selection uniform it derives from this state. Only a `switchable`
    /// fixture is a valid target: its prepared illumination contribution ships
    /// as its own layer group, so toggling it changes what the shader sums
    /// without a runtime bake.
    pub fn set_light_enabled(&mut self, light_index: usize, enabled: bool) -> bool {
        let Some(light) = self.lights.get_mut(light_index) else {
            return false;
        };
        if light.source.enabled == enabled {
            return false;
        }
        light.source.enabled = enabled;
        true
    }

    /// Number of connected baseline areas across every room. Equals the room
    /// count for a level with no internal partitions.
    #[must_use]
    pub fn zone_count(&self) -> usize {
        self.room_zones
            .iter()
            .map(|zones| {
                zones
                    .as_ref()
                    .map_or(1, |room_zones| room_zones.zones.len())
            })
            .sum()
    }

    /// True when internal walls split `room` into more than one baseline area.
    #[must_use]
    pub fn is_partitioned(&self, room: usize) -> bool {
        self.room_zones.get(room).is_some_and(Option::is_some)
    }

    /// Existing connected-area ID for capture diagnostics, paired with `room`.
    /// An unpartitioned room has area zero; invalid rooms resolve to nothing.
    #[must_use]
    pub fn probe_region_at(&self, room: usize, position: [f32; 3]) -> Option<u32> {
        // The surface-lighting height helper has a historical XZ fallback.
        // Entity probes must occupy this room's actual air volume instead.
        if self.indexed_room(position[0], Some(position[1]), position[2], false) != Some(room) {
            return None;
        }
        self.room_zones
            .get(room)
            .and_then(Option::as_ref)
            .map_or(Some(0), |zones| zones.zone_at(position[0], position[2]))
    }

    /// Whether two positions share an existing connected lighting area.
    /// Uses the same zone lookup as the existing surface-lighting path.
    #[must_use]
    pub fn same_probe_region(&self, room: usize, a: [f32; 3], b: [f32; 3]) -> bool {
        let region = self.probe_region_at(room, a);
        region.is_some() && region == self.probe_region_at(room, b)
    }

    /// Validates the compiler's owner before testing cross-region visibility.
    /// A corrupt label never becomes a neighbouring-room lighting source.
    #[must_use]
    pub fn labelled_probe_visible_from(
        &self,
        position: [f32; 3],
        probe: [f32; 3],
        label: i32,
    ) -> bool {
        usize::try_from(label).ok() == self.room_index_at_height(probe[0], probe[1], probe[2])
            && self.probe_visible_from(position, probe)
    }

    /// Whether a prepared probe may contribute at an entity position.
    /// Indexed compiled solids require a clear segment even within one area:
    /// a short divider or furniture can block light without splitting a room.
    /// An open doorway permits both visible room populations to contribute.
    #[must_use]
    pub fn probe_visible_from(&self, position: [f32; 3], probe: [f32; 3]) -> bool {
        let area = |point: [f32; 3]| {
            self.indexed_room(point[0], Some(point[1]), point[2], false)
                .and_then(|room| self.probe_region_at(room, point).map(|zone| (room, zone)))
        };
        let (Some(_source), Some(_target)) = (area(position), area(probe)) else {
            return false;
        };
        self.runtime_probe_segment_visible(position, probe)
    }

    /// Tests one finite source tap against its compiled, owner-aware solids.
    #[must_use]
    pub fn runtime_light_visible(&self, light_index: usize, from: [f32; 3], to: [f32; 3]) -> bool {
        self.lights
            .get(light_index)
            .is_some_and(BakedLight::is_active)
            && from.iter().chain(&to).all(|value| value.is_finite())
            && u32::try_from(light_index)
                .is_ok_and(|site| !self.visibility.occludes(site, from, to))
    }

    /// Tests an entity/probe segment against the prepared world without a site.
    #[must_use]
    pub fn runtime_probe_segment_visible(&self, from: [f32; 3], to: [f32; 3]) -> bool {
        from.iter().chain(&to).all(|value| value.is_finite())
            && !self.visibility.occludes_anywhere(from, to)
    }

    /// Baked baseline illumination of the area containing `(x, z)` in `room`.
    ///
    /// A room with no internal partition returns its room-wide
    /// [`RoomLighting::baseline`] for every position, so open rooms behave
    /// exactly as they did before the baseline became spatially aware. A
    /// partitioned room returns the baseline of the connected area the
    /// position falls in: the side of a solid wall with no fixtures of its own
    /// does not inherit the other side's density.
    #[must_use]
    pub fn baseline_in_room(&self, room: usize, x: f32, z: f32) -> LightColor {
        self.baseline_at(room, x, z)
    }

    fn baseline_at(&self, room: usize, x: f32, z: f32) -> LightColor {
        let uniform = self
            .rooms
            .get(room)
            .map_or_else(ambient_color, |info| info.baseline);
        let zones = self.room_zones.get(room).and_then(Option::as_ref);
        zones
            .and_then(|room_zones| room_zones.baseline_at(x, z))
            .unwrap_or(uniform)
    }

    /// The room a world position belongs to, or `None` outside every room.
    ///
    /// Deterministic ownership rule for the overlapping/intersecting rooms this
    /// engine allows: the *smallest-area* containing room wins, and equal areas
    /// keep the level's own room order. This is the single shared containment
    /// helper used for lighting, so fixtures are never double counted.
    #[must_use]
    pub fn room_index_at(&self, x: f32, z: f32) -> Option<usize> {
        self.indexed_room(x, None, z, false)
    }

    fn room_index_of(rooms: &[RoomLighting], x: f32, z: f32) -> Option<usize> {
        if !x.is_finite() || !z.is_finite() {
            return None;
        }
        let mut best: Option<usize> = None;
        for (index, room) in rooms.iter().enumerate() {
            if x < room.x0 - ROOM_EDGE_EPS_M
                || x > room.x1 + ROOM_EDGE_EPS_M
                || z < room.z0 - ROOM_EDGE_EPS_M
                || z > room.z1 + ROOM_EDGE_EPS_M
            {
                continue;
            }
            match best {
                // Strictly smaller areas only, so ties keep the earlier room.
                Some(current)
                    if rooms
                        .get(current)
                        .is_some_and(|info| info.area_m2 <= room.area_m2) => {}
                _ => best = Some(index),
            }
        }
        best
    }

    /// The room whose air volume contains `(x, y, z)`.
    ///
    /// Room footprints may overlap — stacked rooms share XZ — so a lookup that
    /// ignored Y would resolve a point on the upper floor to whichever of the
    /// two rooms the area tie-break preferred. This prefers the smallest room
    /// whose vertical span at `(x, z)` contains `y`, then falls back to the
    /// historical footprint rule when `y` is outside every span (a point above
    /// a roof, or a hand-edited room). It is the lookup the whole-position
    /// [`Self::sample`] uses.
    #[must_use]
    pub fn room_index_at_height(&self, x: f32, y: f32, z: f32) -> Option<usize> {
        self.indexed_room(x, Some(y), z, false)
            .or_else(|| y.is_finite().then(|| self.room_index_at(x, z)).flatten())
    }

    #[cfg(test)]
    fn room_index_at_height_of(rooms: &[RoomLighting], x: f32, y: f32, z: f32) -> Option<usize> {
        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return None;
        }
        let mut best: Option<usize> = None;
        for (index, room) in rooms.iter().enumerate() {
            // The footprint bounds are cheap; the ceiling profile is not.
            // Rejecting on the footprint first keeps the per-sample room search
            // affordable when a level has many rooms.
            if x < room.x0 - ROOM_EDGE_EPS_M
                || x > room.x1 + ROOM_EDGE_EPS_M
                || z < room.z0 - ROOM_EDGE_EPS_M
                || z > room.z1 + ROOM_EDGE_EPS_M
            {
                continue;
            }
            let (floor, ceiling) = room.span_at(x, z);
            if y < floor - ROOM_EDGE_EPS_M || y > ceiling + ROOM_EDGE_EPS_M {
                continue;
            }
            match best {
                Some(current)
                    if rooms
                        .get(current)
                        .is_some_and(|info| info.area_m2 <= room.area_m2) => {}
                _ => best = Some(index),
            }
        }
        best.or_else(|| Self::room_index_of(rooms, x, z))
    }

    /// The room whose *interior* contains a world position.
    ///
    /// [`Self::room_index_at`] treats a point within [`ROOM_EDGE_EPS_M`] of a
    /// footprint edge as inside, because floors, ceilings and wall faces sit on
    /// room boundaries. A point that is genuinely inside one room while merely
    /// touching another — a wall face sample on a shared room boundary, for
    /// example — is resolved by this rule instead, so the surface is lit by the
    /// room it belongs to rather than by whichever neighbour the tie-break
    /// happened to prefer.
    #[must_use]
    pub fn room_index_strict_at(&self, x: f32, z: f32) -> Option<usize> {
        self.indexed_room(x, None, z, true)
    }

    #[cfg(test)]
    fn room_index_strict_of(rooms: &[RoomLighting], x: f32, z: f32) -> Option<usize> {
        if !x.is_finite() || !z.is_finite() {
            return None;
        }
        let mut best: Option<usize> = None;
        for (index, room) in rooms.iter().enumerate() {
            if x < room.x0 + ROOM_EDGE_EPS_M
                || x > room.x1 - ROOM_EDGE_EPS_M
                || z < room.z0 + ROOM_EDGE_EPS_M
                || z > room.z1 - ROOM_EDGE_EPS_M
            {
                continue;
            }
            match best {
                Some(current)
                    if rooms
                        .get(current)
                        .is_some_and(|info| info.area_m2 <= room.area_m2) => {}
                _ => best = Some(index),
            }
        }
        best
    }

    /// [`Self::room_index_strict_at`] restricted to rooms whose air volume
    /// contains `y`, for surfaces in a stacked building.
    pub(crate) fn room_index_strict_at_height(&self, x: f32, y: f32, z: f32) -> Option<usize> {
        self.indexed_room(x, Some(y), z, true)
    }

    fn indexed_room(&self, x: f32, y: Option<f32>, z: f32, strict: bool) -> Option<usize> {
        if !x.is_finite() || !z.is_finite() || y.is_some_and(|value| !value.is_finite()) {
            return None;
        }
        let margin = if strict {
            -ROOM_EDGE_EPS_M
        } else {
            ROOM_EDGE_EPS_M
        };
        let mut best: Option<usize> = None;
        for &index in self.room_index.candidates(x, z) {
            let Some(room) = self.rooms.get(index) else {
                continue;
            };
            if x < room.x0 - margin
                || x > room.x1 + margin
                || z < room.z0 - margin
                || z > room.z1 + margin
            {
                continue;
            }
            if let Some(mount_y) = y {
                let (floor, ceiling) = room.span_at(x, z);
                if mount_y < floor - ROOM_EDGE_EPS_M || mount_y > ceiling + ROOM_EDGE_EPS_M {
                    continue;
                }
            }
            match best {
                Some(current)
                    if self
                        .rooms
                        .get(current)
                        .is_some_and(|info| info.area_m2 <= room.area_m2) => {}
                _ => best = Some(index),
            }
        }
        best
    }

    /// The room a wall face opens into, from a point on the face and the face's
    /// outward normal.
    ///
    /// Wall faces are lit by the room they look into, and the emitter resolves
    /// that room once per face from a point that is unambiguous (the middle of
    /// the face). A face that runs along a room boundary would otherwise be lit
    /// by whichever of the two rooms the containment tie-break preferred, which
    /// is what turned a shared boundary into a dark, wrongly coloured wedge.
    /// A stacked building resolves each face *sample* by its own height in
    /// [`Self::sample_face`]; this pick is the fallback for samples that sit
    /// exactly on a boundary.
    #[must_use]
    pub fn face_room(&self, x: f32, z: f32, normal_x: f32, normal_z: f32) -> Option<usize> {
        let length = normal_x.hypot(normal_z);
        if !length.is_finite() || length <= f32::EPSILON {
            return None;
        }
        let probe_x = (normal_x / length).mul_add(WALL_FACE_PROBE_M, x);
        let probe_z = (normal_z / length).mul_add(WALL_FACE_PROBE_M, z);
        self.room_index_strict_at(probe_x, probe_z)
            .or_else(|| self.room_index_at(probe_x, probe_z))
    }

    /// Baked illumination for a wall face sample, with the face's own room.
    ///
    /// `hint` is the room [`Self::face_room`] resolved for the face. A sample
    /// that is strictly inside another room (a face that genuinely spans two
    /// rooms) uses that room; a sample that is only touching an edge, or that
    /// falls inside the perpendicular wall a face ends against, is evaluated
    /// inside the hinted room instead of dropping to the outside fill. The
    /// position is clamped into the room footprint for that case, so the pools
    /// are measured from the room boundary the face lies on.
    #[must_use]
    pub fn sample_face(&self, hint: Option<usize>, x: f32, y: f32, z: f32) -> LightColor {
        if let Some(room) = self.room_index_strict_at_height(x, y, z) {
            return self.sample_in_room(room, x, y, z);
        }
        match hint.and_then(|room| self.rooms.get(room).map(|info| (room, info))) {
            Some((room, info)) => {
                let clamped_x = x.clamp(info.x0 + ROOM_EDGE_EPS_M, info.x1 - ROOM_EDGE_EPS_M);
                let clamped_z = z.clamp(info.z0 + ROOM_EDGE_EPS_M, info.z1 - ROOM_EDGE_EPS_M);
                self.sample_in_room(room, clamped_x, y, clamped_z)
            }
            None => self.sample(x, y, z),
        }
    }

    /// World Y of a ceiling light's horizontal panel at `(x, z)`.
    ///
    /// Fixtures hang just below their room's ceiling, so the same panel sits at
    /// 2.59 m in a 2.6 m corridor and at 2.99 m in a 3 m room. Under a gable the
    /// panel uses the *lowest* ceiling point it covers, so it never intersects
    /// the slope; this is the single function the mesh and the bake both use, so
    /// the drawn panel and the baked light pool can never drift apart.
    #[must_use]
    pub fn fixture_panel_y(&self, x: f32, z: f32, half_w: f32, half_d: f32) -> f32 {
        panel_min_ceiling_y(
            self.room_index_at(x, z)
                .and_then(|index| self.rooms.get(index)),
            self.default_ceiling_y,
            x,
            z,
            half_w.max(0.0),
            half_d.max(0.0),
        ) - FIXTURE_DROP_M
    }

    /// World Y of a point fixture panel at `(x, z)`, ignoring panel extents.
    #[must_use]
    pub fn fixture_y(&self, x: f32, z: f32) -> f32 {
        self.fixture_panel_y(x, z, 0.0, 0.0)
    }

    /// World Y of a placed fixture's luminous panel.
    ///
    /// The single function the fixture mesh uses, so the drawn panel can never
    /// drift from the baked light plane:
    ///
    /// * a wall fixture mounts at its authored world `y`;
    /// * a ceiling fixture with a finite authored `y` mounts at that world
    ///   height (the form a stacked building uses to pick a floor), which also
    ///   decides the room [`Self::bake`] assigns it to;
    /// * a ceiling fixture without one hangs just below the ceiling of the room
    ///   its position resolves to, exactly as it always did.
    #[must_use]
    pub fn fixture_y_for(&self, light: &LightFixtureDef) -> f32 {
        match light.mount {
            LightMount::Wall => self.wall_fixture_y(light.x, light.z, light.y),
            LightMount::Ceiling => {
                if let Some(y) = light.y.filter(|y| y.is_finite()) {
                    return y;
                }
                let profile = fixture_profile(&light.fixture);
                let (half_w, half_d) =
                    fixture_half_extents_for(profile.kind, light.rotation_degrees);
                self.fixture_panel_y(light.x, light.z, half_w, half_d)
            }
        }
    }

    /// World Y of a wall-mounted fixture at `(x, z)`.
    ///
    /// The authored height wins; a hand-edited level without one falls back to
    /// [`WALL_LIGHT_DEFAULT_HEIGHT_M`] above the owning room's floor. Shared by
    /// the bake and the fixture geometry.
    #[must_use]
    pub fn wall_fixture_y(&self, x: f32, z: f32, authored: Option<f32>) -> f32 {
        resolve_wall_fixture_y(&self.rooms, x, z, authored)
    }

    /// True when `(x, z)` lies inside a static wall solid, ignoring height.
    ///
    /// The baked-lighting query behind [`Self::clear_sample`]. Surface emitters
    /// ask it directly for lightmap texels: a wall-face texel nudged off its own
    /// face can still land inside another wall at a junction (a cross wall, a
    /// doorway jamb), and such a texel has to take the walked
    /// [`Self::sample_in_room`] path instead of the fast texel path.
    #[must_use]
    pub fn wall_contains_point(&self, x: f32, z: f32) -> bool {
        self.visibility.contains_point(x, z)
    }

    /// The world XZ a surface sample in `room` is evaluated at, after the
    /// wall-clearing walk of [`Self::sample_in_room`].
    ///
    /// Developer diagnostics only: a candidate model must be evaluated at the
    /// same point the production model evaluates, which for a sample that
    /// falls inside a wall is not the sample's own XZ.
    #[must_use]
    pub fn cleared_sample_position(&self, room: usize, x: f32, z: f32) -> (f32, f32) {
        if self.rooms.get(room).is_none() {
            return (x, z);
        }
        self.clear_sample(room, x, z)
    }

    /// Moves a room surface sample out of an opaque wall it lies inside.
    ///
    /// The walk runs straight toward the middle of the room in fixed steps and
    /// stops at the first point that is not inside a wall, which for a wall that
    /// straddles the room boundary is a few centimetres. A sample that never
    /// leaves the solid (a room authored entirely inside a wall) falls back to
    /// the room centre. The step count is bounded, so a pathological level
    /// cannot turn this into an unbounded search.
    fn clear_sample(&self, room: usize, x: f32, z: f32) -> (f32, f32) {
        if !self.visibility.contains_point(x, z) {
            return (x, z);
        }
        let Some(info) = self.rooms.get(room) else {
            return (x, z);
        };
        let target_x = f32::midpoint(info.x0, info.x1);
        let target_z = f32::midpoint(info.z0, info.z1);
        let delta_x = target_x - x;
        let delta_z = target_z - z;
        let distance = delta_x.hypot(delta_z);
        if !distance.is_finite() || distance <= ROOM_EDGE_EPS_M {
            return (x, z);
        }
        for step in 1..=CLEAR_SAMPLE_MAX_STEPS {
            let Ok(step_index) = u16::try_from(step) else {
                break;
            };
            let walked = f32::from(step_index) * CLEAR_SAMPLE_STEP_M;
            if walked > distance {
                break;
            }
            let t = walked / distance;
            let probe_x = delta_x.mul_add(t, x);
            let probe_z = delta_z.mul_add(t, z);
            if !self.visibility.contains_point(probe_x, probe_z) {
                return (probe_x, probe_z);
            }
        }
        (target_x, target_z)
    }

    /// Baked illumination at a world position, resolving the room by
    /// containment at the sample's own height.
    ///
    /// Used for props and for geometry that does not know its room. Points
    /// outside every room still receive the ambient fill and any local fixture
    /// pools they are inside.
    #[must_use]
    pub fn sample(&self, x: f32, y: f32, z: f32) -> LightColor {
        self.room_index_at_height(x, y, z).map_or_else(
            || {
                let (direct, fill) = self.local_light(&self.all_lights, None, x, y, z);
                ambient_color().plus(direct).plus(fill)
            },
            |index| self.sample_in_room(index, x, y, z),
        )
    }

    /// Scalar luminance view of [`Self::sample`].
    ///
    /// Diagnostics and brightness-only comparisons (logging, audit tests) use
    /// this; anything that cares about colour must read the [`LightColor`]
    /// channels from [`Self::sample`] instead.
    #[must_use]
    pub fn sample_luminance(&self, x: f32, y: f32, z: f32) -> f32 {
        self.sample(x, y, z).luminance()
    }

    /// Scalar luminance view of [`Self::sample_in_room`].
    #[must_use]
    pub fn sample_in_room_luminance(&self, room: usize, x: f32, y: f32, z: f32) -> f32 {
        self.sample_in_room(room, x, y, z).luminance()
    }

    /// The additive terms behind [`Self::sample`].
    ///
    /// The sample resolves its room by containment at its own height, exactly
    /// like [`Self::sample`]; a sample outside every room reports the ambient
    /// fill as its baseline, the local pools it is inside and a zero blend.
    /// Developer diagnostics only — the render path never calls this.
    #[must_use]
    pub fn bake_terms(&self, x: f32, y: f32, z: f32) -> BakeTerms {
        let room = self.room_index_at_height(x, y, z);
        self.bake_terms_for(room, x, y, z)
    }

    /// The additive terms behind [`Self::sample_in_room`].
    ///
    /// Includes the same wall-clearing walk a real surface sample takes, so
    /// the terms reproduce the returned [`LightColor`] exactly. A room that
    /// does not exist falls back to [`Self::bake_terms`], mirroring
    /// [`Self::sample_in_room`].
    #[must_use]
    pub fn bake_terms_in_room(&self, room: usize, x: f32, y: f32, z: f32) -> BakeTerms {
        if self.rooms.get(room).is_none() {
            return self.bake_terms(x, y, z);
        }
        self.bake_terms_for(Some(room), x, y, z)
    }

    /// Shared decomposition of the two sample paths.
    fn bake_terms_for(&self, room: Option<usize>, x: f32, y: f32, z: f32) -> BakeTerms {
        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            let value = ambient_color();
            return BakeTerms {
                room: None,
                baseline: value,
                pool: LightColor::BLACK,
                fill: LightColor::BLACK,
                blend: LightColor::BLACK,
                value,
            };
        }
        let Some(room_index) = room.filter(|index| self.rooms.get(*index).is_some()) else {
            let baseline = ambient_color();
            let (direct, fill) = self.local_light(&self.all_lights, None, x, y, z);
            let value = baseline
                .plus(direct)
                .plus(fill)
                .clamped(AMBIENT_LEVEL, MAX_BRIGHTNESS);
            return BakeTerms {
                room: None,
                baseline,
                pool: direct,
                fill,
                blend: LightColor::BLACK,
                value,
            };
        };
        let (sample_x, sample_z) = self.clear_sample(room_index, x, z);
        let baseline = self.baseline_at(room_index, sample_x, sample_z);
        let candidates = self
            .room_lights
            .get(room_index)
            .map_or(&[][..], Vec::as_slice);
        let (pool, fill) = self.local_light(candidates, Some(room_index), sample_x, y, sample_z);
        let blend = self.blend_delta(room_index, sample_x, y, sample_z);
        let value = baseline
            .plus(pool)
            .plus(fill)
            .plus(blend)
            .clamped(AMBIENT_LEVEL, MAX_BRIGHTNESS);
        BakeTerms {
            room: Some(room_index),
            baseline,
            pool,
            fill,
            blend,
            value,
        }
    }

    /// Developer view of the connected baseline areas of one room.
    ///
    /// A room with no internal partition returns a single entry describing the
    /// whole room (the exact terms [`RoomLighting::baseline`] was baked from),
    /// so a report can print every room the same way. A partitioned room
    /// returns one entry per area, in the bake's deterministic component
    /// order.
    #[must_use]
    pub fn zones_in_room(&self, room: usize) -> Vec<ZoneLighting> {
        let Some(info) = self.rooms.get(room) else {
            return Vec::new();
        };
        let Some(zones) = self.room_zones.get(room).and_then(Option::as_ref) else {
            return vec![ZoneLighting {
                area_m2: info.area_m2,
                fixture_count: info.fixture_count,
                power: info.effective_power,
                baseline: info.baseline,
            }];
        };
        zones
            .zones
            .iter()
            .map(|zone| ZoneLighting {
                area_m2: zone.area_m2,
                fixture_count: zone.fixture_count,
                power: zone.power,
                baseline: zone.baseline,
            })
            .collect()
    }

    /// Baked illumination for a point already known to belong to `room`.
    ///
    /// Floors, ceilings and wall faces use this so a vertex sitting exactly on a
    /// room boundary is lit by the surface's own room, not by whichever room the
    /// containment rule happens to prefer.
    ///
    /// A room's floor and ceiling are sampled on the room's own footprint, and a
    /// wall authored across that boundary (the common construction in the
    /// showcase levels) puts the outermost sample row *inside* the wall. Such a
    /// sample is walked back into the room first — see [`Self::clear_sample`] —
    /// so the wall does not cast a false shadow along its own base.
    ///
    /// The baseline is the sample's own connected area
    /// ([`Self::baseline_in_room`]), so an internal partition isolates the
    /// fixture-derived baseline while an open room is unchanged. Local fixture
    /// pools and doorway blends are then tested against the same solid geometry,
    /// which now includes every room's floor and ceiling slab, so a fixture on
    /// another storey cannot light through a solid slab.
    #[must_use]
    pub fn sample_in_room(&self, room: usize, x: f32, y: f32, z: f32) -> LightColor {
        if self.rooms.get(room).is_none() {
            return self.sample(x, y, z);
        }
        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return ambient_color();
        }
        let (sample_x, sample_z) = self.clear_sample(room, x, z);
        self.illumination_in_room(room, sample_x, y, sample_z)
    }

    /// Baked illumination for a lightmap texel: [`Self::sample_in_room`]
    /// without the wall-clearing walk.
    ///
    /// A texel is generated on a surface plane, not walked to from a
    /// mesh vertex, so the common case has no wall to leave. Dropping the walk
    /// removes one grid lookup and the bounded step loop from every texel of a
    /// lightmap page; for a point that is not inside a wall the two functions
    /// return **exactly** the same value (the walk is the identity there),
    /// which a test pins over a sample grid.
    ///
    /// The caller must rule out a buried point itself. A texel can sit inside a
    /// crossing wall at a junction, and the outermost floor/ceiling row sits
    /// inside a wall authored across the room boundary; `lightmap::fill` checks
    /// [`Self::wall_contains_point`] for every patch kind and falls back to
    /// [`Self::sample_in_room`] for exactly those texels.
    ///
    /// `room` is the patch's room hint. `None` — a patch outside every room —
    /// resolves the room by containment, exactly like [`Self::sample`].
    #[must_use]
    pub fn lightmap_texel(&self, room: Option<usize>, x: f32, y: f32, z: f32) -> LightColor {
        match room {
            Some(room_index) if self.rooms.get(room_index).is_some() => {
                if !x.is_finite() || !y.is_finite() || !z.is_finite() {
                    return ambient_color();
                }
                self.illumination_in_room(room_index, x, y, z)
            }
            _ => self.sample(x, y, z),
        }
    }

    /// The illumination sum of one room at an already-resolved world point.
    ///
    /// Shared by [`Self::sample_in_room`] (after its walk) and
    /// [`Self::lightmap_texel`] (which skips it), so the two can never drift.
    fn illumination_in_room(&self, room: usize, x: f32, y: f32, z: f32) -> LightColor {
        let Some(candidates) = self.room_lights.get(room) else {
            return ambient_color();
        };
        let baseline = self.baseline_at(room, x, z);
        let (pool, fill) = self.local_light(candidates, Some(room), x, y, z);
        let mut value = baseline.plus(pool).plus(fill);
        value = value.plus(self.blend_delta(room, x, y, z));

        if value.is_finite() {
            value.clamped(AMBIENT_LEVEL, MAX_BRIGHTNESS)
        } else {
            ambient_color()
        }
    }

    /// The doorway-blend part of [`Self::sample_in_room`] at a world position.
    ///
    /// This is the exchange between the areas on either side of a walk-through
    /// opening, isolated from the area's own baseline and fixture pools: a delta
    /// that is negative on the brighter side and positive on the dimmer one. It
    /// is exposed for the developer log and the
    /// doorway regression tests, which need to prove the blending is bounded,
    /// symmetric and blind to opaque walls without the local pools moving
    /// underneath the measurement.
    #[must_use]
    pub fn opening_blend(&self, room: usize, x: f32, y: f32, z: f32) -> LightColor {
        if self.rooms.get(room).is_none() || !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return LightColor::BLACK;
        }
        let (sample_x, sample_z) = self.clear_sample(room, x, z);
        let delta = self.blend_delta(room, sample_x, y, sample_z);
        if delta.is_finite() {
            delta
        } else {
            LightColor::BLACK
        }
    }

    fn blend_delta(&self, room: usize, x: f32, y: f32, z: f32) -> LightColor {
        let mut delta = LightColor::BLACK;
        let Some(blends) = self.blends.get(room) else {
            return LightColor::BLACK;
        };
        let zone = self
            .room_zones
            .get(room)
            .and_then(Option::as_ref)
            .and_then(|zones| zones.zone_at(x, z));
        for blend in blends {
            // A partitioned room carries one entry per side of the opening;
            // only the entry whose own area contains the sample may speak for
            // it, or the two sides would double-count each other.
            if blend.own_zone.is_some() && blend.own_zone != zone {
                continue;
            }
            let dx = x - blend.x;
            let dz = z - blend.z;
            let distance = dx.hypot(dz);
            if !distance.is_finite() || distance >= OPENING_BLEND_RADIUS_M {
                continue;
            }
            if self
                .visibility
                .occludes(blend.site, blend.source(), [x, y, z])
            {
                continue;
            }
            let mut influence =
                OPENING_BLEND_STRENGTH * smooth_falloff(distance / OPENING_BLEND_RADIUS_M);
            if y > blend.top_y {
                influence *= smooth_falloff((y - blend.top_y) / OPENING_VERTICAL_FADE_M);
            }
            if influence <= 0.0 {
                continue;
            }
            delta = LightColor {
                r: (blend.neighbor_baseline.r - blend.own_baseline.r).mul_add(influence, delta.r),
                g: (blend.neighbor_baseline.g - blend.own_baseline.g).mul_add(influence, delta.g),
                b: (blend.neighbor_baseline.b - blend.own_baseline.b).mul_add(influence, delta.b),
            };
        }
        delta
    }

    /// Local fixture lighting at a world position: directional direct pools
    /// plus the broad bounce fill, both bounded.
    ///
    /// A ceiling fixture's direct pool is directional: it only lights samples
    /// below the emitter, falls as `lateral * incidence` where
    /// `lateral = (1 - dh/range)^2` is the horizontal cushion to the emitter
    /// rectangle and `incidence = vertical / distance` is the cosine of a
    /// horizontal surface's view, so the floor beneath the panel is the local
    /// maximum. A wall sconce or prop light keeps the historical isotropic
    /// ball on its authored falloff curve. Both pool kinds are visibility
    /// tested, and both are screened against their own caps
    /// ([`LOCAL_LIGHT_MAX`], [`FILL_MAX`]) instead of summed and clipped, so
    /// colour is never discarded and overlaps stay monotone.
    ///
    /// The bounce fill is the room's first reflected light: the same
    /// per-fixture visibility test, an isotropic cushion out to
    /// [`FILL_RANGE_MULTIPLIER`] times the light's range and a weaker
    /// [`FILL_STRENGTH`], which is what gives ceilings and upper walls a
    /// plausible halo around a recessed fixture without pretending the panel
    /// points at them.
    ///
    /// `candidates` are indices into [`Self::lights`]; squared distances are
    /// compared against the reach before the square root, so fixtures that
    /// cannot reach the sample are rejected with a couple of multiplies.
    ///
    /// `room` is the sample's own room when it has one. A fixture belonging to
    /// another room is skipped outright when the two rooms overlap in plan but
    /// their air columns do not (`spans` touch at most): that is a solid
    /// floor/ceiling between two storeys, and no slab ordering or shared plane
    /// can make it transmit. Rooms that genuinely share air (a mezzanine, or an
    /// overlapping same-level pair) still resolve by geometry.
    fn local_light(
        &self,
        candidates: &[u32],
        room: Option<usize>,
        x: f32,
        y: f32,
        z: f32,
    ) -> (LightColor, LightColor) {
        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return (LightColor::BLACK, LightColor::BLACK);
        }
        let mut direct = [0.0_f32; 3];
        let mut fill = [0.0_f32; 3];
        let spatial = self.light_index.candidates(x, z);
        let use_spatial = spatial.len() < candidates.len();
        let selected = if use_spatial { spatial } else { candidates };
        for index in selected {
            // room_light_candidates and all_lights are sorted source indices.
            // Keep their exact membership even when a neighbouring room's
            // light occupies this spatial bucket.
            if use_spatial && candidates.binary_search(index).is_err() {
                continue;
            }
            let Some(term) = self.pool_term_for(*index, room, x, y, z) else {
                continue;
            };
            term.screen_direct(&mut direct, LOCAL_LIGHT_STRENGTH);
            term.screen_fill(&mut fill, FILL_STRENGTH);
        }
        (
            LightColor::rgb(direct[0], direct[1], direct[2]),
            LightColor::rgb(fill[0], fill[1], fill[2]),
        )
    }

    /// One light's local term at a sample.
    ///
    /// `None` when the light does not contribute: inactive, outside the
    /// pool's reach (a ceiling fixture reaches `range` horizontally, an
    /// isotropic light `range` in 3D, and every light's bounce fill reaches
    /// [`FILL_RANGE_MULTIPLIER`] times its range), separated by a solid slab
    /// from an overlapping room, or fully occluded. The single place the
    /// pool's geometry and visibility are resolved, shared by
    /// [`Self::local_light`] and [`Self::pool_terms_in_room`], so the
    /// production composition and its developer decomposition cannot drift.
    fn pool_term_for(
        &self,
        index: u32,
        room: Option<usize>,
        x: f32,
        y: f32,
        z: f32,
    ) -> Option<PoolTerm> {
        let light = self.lights.get(usize::try_from(index).ok()?)?;
        if !light.is_active() {
            return None;
        }
        let (half_w, half_d) = light.source.half_extents();
        let range = light.source.range;
        let fill_range = fill_range_for(range);
        let reach_squared = fill_range * fill_range;
        if let Some(sample_room) = room
            && let Some(light_room) = light.room
            && light_room != sample_room
            && self.footprint_contains(light_room, x, z)
            && !self.rooms_share_air(light_room, sample_room, x, z)
        {
            return None;
        }
        // Horizontal distance to the rotated emitter footprint. Both terms
        // are inside this reach, so the cheap rejection is lossless.
        let dx = ((x - light.x()).abs() - half_w).max(0.0);
        let dz = ((z - light.z()).abs() - half_d).max(0.0);
        let horizontal_squared = dz.mul_add(dz, dx * dx);
        if !horizontal_squared.is_finite() || horizontal_squared >= reach_squared {
            return None;
        }
        let horizontal_m = horizontal_squared.sqrt();
        // Emitter height above the sample. Positive means the sample is below
        // the emitter; a ceiling fixture only lights what is below it.
        let vertical = light.y() - y;
        let distance_squared = vertical.mul_add(vertical, horizontal_squared);
        if !distance_squared.is_finite() {
            return None;
        }
        let distance_m = distance_squared.sqrt();
        let direct_shape = if light.directional {
            if vertical <= 0.0 || distance_m <= f32::EPSILON || horizontal_m >= range {
                0.0
            } else {
                // The authored falloff names the lateral curve: the default
                // smooth cushion, a straight line, or a flat hold to the
                // range. Incidence always applies (it is geometry, not a
                // curve choice).
                let lateral = match light.falloff() {
                    LightFalloff::Smooth => {
                        let u = 1.0 - horizontal_m / range;
                        u * u
                    }
                    LightFalloff::Linear => 1.0 - horizontal_m / range,
                    LightFalloff::Constant => 1.0,
                };
                lateral * (vertical / distance_m)
            }
        } else if distance_m < range {
            light.falloff().factor(distance_m / range)
        } else {
            0.0
        };
        let fill_shape = if distance_m < fill_range {
            smooth_falloff(distance_m / fill_range)
        } else {
            0.0
        };
        if direct_shape <= 0.0 && fill_shape <= 0.0 {
            return None;
        }
        // Static solid visibility: a light contributes only where its
        // emitter can actually see the sample. With
        // [`ShadowSampling::HARD`] this is the historical test from the
        // emitter's closest point and returns exactly 0 or 1; a soft
        // sampling fades the same contribution over the penumbra.
        let visible = self.visibility.visible_fraction(
            index,
            [light.x(), light.y(), light.z()],
            half_w,
            half_d,
            [x, y, z],
            ShadowSampling {
                taps_per_axis: self.sampling_taps,
            },
        );
        if visible <= 0.0 {
            return None;
        }
        Some(PoolTerm {
            light: index,
            distance_m,
            horizontal_m,
            vertical_m: vertical,
            intensity: light.intensity(),
            height_factor: light.height_factor,
            visibility: visible,
            directional: light.directional,
            direct_shape,
            fill_shape,
            color: light.color(),
        })
    }

    /// Every fixture term at a world position, for developer diagnostics.
    ///
    /// The same terms, in the same order, that [`Self::local_light`] composes
    /// for the same `room`; a caller can re-evaluate the direct pools and the
    /// bounce fill under candidate constants without re-baking the level. A
    /// room that does not exist and a non-finite position both yield an empty
    /// list.
    #[must_use]
    pub fn pool_terms_in_room(&self, room: Option<usize>, x: f32, y: f32, z: f32) -> Vec<PoolTerm> {
        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return Vec::new();
        }
        let candidates: &[u32] = match room {
            Some(room_index) => self.room_lights.get(room_index).map_or(&[], Vec::as_slice),
            None => &self.all_lights,
        };
        candidates
            .iter()
            .filter_map(|index| self.pool_term_for(*index, room, x, y, z))
            .collect()
    }

    /// True when a room's footprint contains `(x, z)`, edge tolerance included.
    fn footprint_contains(&self, room: usize, x: f32, z: f32) -> bool {
        self.rooms.get(room).is_some_and(|info| {
            x >= info.x0 - ROOM_EDGE_EPS_M
                && x <= info.x1 + ROOM_EDGE_EPS_M
                && z >= info.z0 - ROOM_EDGE_EPS_M
                && z <= info.z1 + ROOM_EDGE_EPS_M
        })
    }

    /// True when two rooms' air columns overlap at `(x, z)`.
    ///
    /// Two spans that merely touch (a room stacked directly on another room's
    /// eave) do not overlap: the shared plane is a solid boundary, not shared
    /// air. Overlapping spans (a mezzanine inside a tall space, or two
    /// same-level rooms the level deliberately overlaps) do.
    fn rooms_share_air(&self, a: usize, b: usize, x: f32, z: f32) -> bool {
        let (Some(first), Some(second)) = (self.rooms.get(a), self.rooms.get(b)) else {
            return true;
        };
        let (a_floor, a_ceiling) = first.span_at(x, z);
        let (b_floor, b_ceiling) = second.span_at(x, z);
        a_floor < b_ceiling - ROOM_EDGE_EPS_M && b_floor < a_ceiling - ROOM_EDGE_EPS_M
    }

    /// True when a light's emitter can come within its own range of some point
    /// above a room's footprint.
    ///
    /// Used to build the per-room candidate lists: a light this test rejects
    /// contributes exactly zero everywhere in the room, so pruning is lossless.
    /// The test ignores vertical distance, which only makes it more permissive;
    /// a light on another storey is admitted here and then refused by the
    /// floor/ceiling slabs during the actual visibility test.
    fn light_reaches_room(light: &BakedLight, room: &RoomLighting) -> bool {
        let (half_w, half_d) = light.source.half_extents();
        let panel_span_x = (light.x() - half_w, light.x() + half_w);
        let panel_span_z = (light.z() - half_d, light.z() + half_d);
        let room_span_x = (room.x0 - ROOM_EDGE_EPS_M, room.x1 + ROOM_EDGE_EPS_M);
        let room_span_z = (room.z0 - ROOM_EDGE_EPS_M, room.z1 + ROOM_EDGE_EPS_M);
        let gap_x = interval_gap(room_span_x, panel_span_x);
        let gap_z = interval_gap(room_span_z, panel_span_z);
        // The bounce fill reaches `FILL_RANGE_MULTIPLIER` times the light's
        // range, so pruning has to use that reach, not the direct range: a
        // room reached only by the fill still has to see the light.
        let reach = fill_range_for(light.source.range);
        gap_x.mul_add(gap_x, gap_z * gap_z) < reach * reach
    }

    /// Fingerprint of the whole solid set the bake tests light against.
    ///
    /// Wall solids, floor interfaces, ceiling bodies and the derived static
    /// prop occluders, in their deterministic build order. A lightmap is a pure
    /// function of the level definition, the lightmap config *and* this set, so
    /// the lightmap cache key folds it in: editing a prop model that changes its
    /// occlusion produces a different fingerprint and a different key, while a
    /// texture-only edit correctly keeps the cached atlas valid.
    #[must_use]
    pub fn occlusion_fingerprint(&self) -> u64 {
        self.visibility.occluder_fingerprint()
    }

    /// Aggregate statistics for developer logging. Baselines are reported as
    /// luminance so one number can describe a coloured room.
    #[must_use]
    pub fn summary(&self) -> LightingSummary {
        if self.rooms.is_empty() {
            return LightingSummary {
                rooms: 0,
                lights: self.lights.len(),
                blockers: self.visibility.blocker_count(),
                walls: self.visibility.wall_blocker_count(),
                props: self.visibility.prop_blocker_count(),
                zones: self.zone_count(),
                min_baseline: 0.0,
                max_baseline: 0.0,
                average_baseline: 0.0,
            };
        }
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        let mut total = 0.0;
        let mut count = 0.0;
        for room in &self.rooms {
            let luminance = room.baseline.luminance();
            min = min.min(luminance);
            max = max.max(luminance);
            total += luminance;
            count += 1.0;
        }
        LightingSummary {
            rooms: self.rooms.len(),
            lights: self.lights.len(),
            blockers: self.visibility.blocker_count(),
            walls: self.visibility.wall_blocker_count(),
            props: self.visibility.prop_blocker_count(),
            zones: self.zone_count(),
            min_baseline: min,
            max_baseline: max,
            average_baseline: total / count,
        }
    }

    /// Encodes the complete bake for the compiled map package.
    ///
    /// The runtime keeps baked-lighting sampling from this record: a position
    /// the prepared irradiance field cannot resolve falls back to
    /// [`LevelLighting::sample`], and the field's room labels are assigned
    /// against the same `room_index_at_height` this bake exposes. A switchable
    /// fixture's prepared illumination ships as its own lightmap layer group;
    /// this record still carries the light so the runtime light-state API and
    /// the vertex-lit variant read one source of truth. The package therefore
    /// carries the built result rather than a level from which a player would
    /// have to bake it again.
    pub(crate) fn write_compiled(&self, writer: &mut Writer) -> Result<(), String> {
        writer.bytes(&COMPILED_MAGIC);
        writer.u16(COMPILED_VERSION);
        write_rooms(writer, &self.rooms)?;
        write_lights(writer, &self.lights)?;
        writer.u8(u8::try_from(self.global_lights.len())
            .map_err(|error| format!("too many directional illuminators: {error}"))?);
        for light in &self.global_lights {
            light.write_compiled(writer);
        }
        write_blends(writer, &self.blends)?;
        write_room_zones(writer, &self.room_zones)?;
        self.visibility.write_compiled(writer)?;
        write_u32_lists(writer, &self.room_lights)?;
        write_u32_list(writer, &self.all_lights)?;
        write_fixture_lights(writer, &self.fixture_lights)?;
        writer.f32(self.default_ceiling_y);
        writer.u8(self.sampling_taps);
        Ok(())
    }

    /// Decodes and validates a compiled bake.
    pub(crate) fn read_compiled(reader: &mut Reader<'_>) -> Result<Self, String> {
        if reader.bytes(4)? != COMPILED_MAGIC {
            return Err("lighting record has the wrong magic".to_string());
        }
        let version = reader.u16()?;
        if version != COMPILED_VERSION {
            return Err(format!(
                "lighting record version {version} is not supported (this build reads {COMPILED_VERSION})"
            ));
        }
        let rooms = read_rooms(reader)?;
        let lights = read_lights(reader, rooms.len())?;
        let count = usize::from(reader.u8()?);
        if count > crate::level::MAX_GLOBAL_ILLUMINATORS {
            return Err("too many compiled directional illuminators".to_string());
        }
        let global_lights = (0..count)
            .map(|_| super::directional::DirectionalLight::read_compiled(reader))
            .collect::<Result<Vec<_>, _>>()?;
        let blends = read_blends(reader, Some(&rooms), None)?;
        let room_zones = read_room_zones(reader, &rooms)?;
        let visibility = Visibility::read_compiled(reader)?;
        let room_lights = read_u32_lists(reader, rooms.len(), lights.len())?;
        let all_lights = read_u32_list(reader, lights.len())?;
        let fixture_lights = read_fixture_lights(reader, lights.len())?;
        let default_ceiling_y = read_finite(reader, "default ceiling height")?;
        let sampling_taps = reader.u8()?;
        if !reader.is_empty() {
            return Err(format!(
                "lighting record has {} trailing bytes",
                reader.remaining()
            ));
        }
        let room_index = RoomIndex::new(&rooms);
        let light_index = LightIndex::new(&lights);
        Ok(Self {
            rooms,
            room_index,
            lights,
            global_lights,
            light_index,
            blends,
            room_zones,
            visibility,
            room_lights,
            all_lights,
            fixture_lights,
            default_ceiling_y,
            sampling_taps,
        })
    }
}

/// Magic identifying a compiled lighting record.
const COMPILED_MAGIC: [u8; 4] = *b"PLLT";

/// Version of the compiled lighting record layout.
pub(super) const COMPILED_VERSION: u16 = 2;

/// Largest room count a compiled record may declare.
const MAX_COMPILED_ROOMS: u64 = 100_000;

/// Largest light count a compiled record may declare.
const MAX_COMPILED_LIGHTS: u64 = 1_000_000;

/// Largest opening-blend count per room.
const MAX_COMPILED_BLENDS_PER_ROOM: u64 = 100_000;

/// Largest zone cell count in one room.
const MAX_COMPILED_ZONE_CELLS: u64 = 1 << 22;

fn read_finite(reader: &mut Reader<'_>, what: &str) -> Result<f32, String> {
    let value = reader.f32()?;
    if !value.is_finite() {
        return Err(format!("{what} is non-finite"));
    }
    Ok(value)
}

fn write_color(writer: &mut Writer, color: LightColor) {
    writer.f32(color.r);
    writer.f32(color.g);
    writer.f32(color.b);
}

fn read_color(reader: &mut Reader<'_>, what: &str) -> Result<LightColor, String> {
    Ok(LightColor {
        r: read_finite(reader, what)?,
        g: read_finite(reader, what)?,
        b: read_finite(reader, what)?,
    })
}

fn write_optional_u32(writer: &mut Writer, value: Option<u32>) {
    match value {
        None => writer.u8(0),
        Some(raw_value) => {
            writer.u8(1);
            writer.u32(raw_value);
        }
    }
}

fn read_optional_u32(reader: &mut Reader<'_>) -> Result<Option<u32>, String> {
    match reader.u8()? {
        0 => Ok(None),
        1 => Ok(Some(reader.u32()?)),
        other => Err(format!("invalid optional marker {other}")),
    }
}

fn write_rooms(writer: &mut Writer, rooms: &[RoomLighting]) -> Result<(), String> {
    let count =
        u32::try_from(rooms.len()).map_err(|error| format!("too many rooms to encode: {error}"))?;
    writer.u32(count);
    for room in rooms {
        writer.f32(room.x0);
        writer.f32(room.x1);
        writer.f32(room.z0);
        writer.f32(room.z1);
        writer.f32(room.floor_y);
        writer.f32(room.height_m);
        match room.profile {
            // Normalized to `Flat` when the room volume was built; an open
            // ceiling resolves as a flat volume there.
            CeilingProfileDef::Flat | CeilingProfileDef::Open => writer.u8(0),
            CeilingProfileDef::Gable { ridge, ridge_rise } => {
                writer.u8(1);
                writer.u8(match ridge {
                    WallAxis::X => 0,
                    WallAxis::Z => 1,
                });
                writer.f32(ridge_rise);
            }
        }
        writer.f32(room.area_m2);
        let fixtures = u32::try_from(room.fixture_count)
            .map_err(|error| format!("room fixture count is too large: {error}"))?;
        writer.u32(fixtures);
        write_color(writer, room.effective_power);
        write_color(writer, room.baseline);
    }
    Ok(())
}

fn read_rooms(reader: &mut Reader<'_>) -> Result<Vec<RoomLighting>, String> {
    let count = reader.count(MAX_COMPILED_ROOMS, "room count")?;
    let mut rooms = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let x0 = read_finite(reader, "room x0")?;
        let x1 = read_finite(reader, "room x1")?;
        let z0 = read_finite(reader, "room z0")?;
        let z1 = read_finite(reader, "room z1")?;
        let floor_y = read_finite(reader, "room floor")?;
        let height_m = read_finite(reader, "room height")?;
        let profile = match reader.u8()? {
            0 => CeilingProfileDef::Flat,
            1 => {
                let ridge = match reader.u8()? {
                    0 => WallAxis::X,
                    1 => WallAxis::Z,
                    other => return Err(format!("unknown ridge axis {other}")),
                };
                CeilingProfileDef::Gable {
                    ridge,
                    ridge_rise: read_finite(reader, "ridge rise")?,
                }
            }
            other => return Err(format!("unknown ceiling profile {other}")),
        };
        let area_m2 = read_finite(reader, "room area")?;
        let fixture_count = usize::try_from(reader.u32()?)
            .map_err(|error| format!("room fixture count is too large: {error}"))?;
        let effective_power = read_color(reader, "room power")?;
        let baseline = read_color(reader, "room baseline")?;
        rooms.push(RoomLighting {
            x0,
            x1,
            z0,
            z1,
            floor_y,
            height_m,
            profile,
            area_m2,
            fixture_count,
            effective_power,
            baseline,
        });
    }
    Ok(rooms)
}

fn write_lights(writer: &mut Writer, lights: &[BakedLight]) -> Result<(), String> {
    let count = u32::try_from(lights.len())
        .map_err(|error| format!("too many lights to encode: {error}"))?;
    writer.u32(count);
    for light in lights {
        match light.source.shape {
            LightShape::Point => writer.u8(0),
            LightShape::Rect {
                half_width,
                half_depth,
            } => {
                writer.u8(1);
                writer.f32(half_width);
                writer.f32(half_depth);
            }
            LightShape::Line { length } => {
                writer.u8(2);
                writer.f32(length);
            }
        }
        writer.f32_3(light.source.position);
        writer.f32(light.source.rotation_degrees);
        write_color(writer, light.source.color);
        writer.f32(light.source.intensity);
        writer.f32(light.source.range);
        writer.u8(match light.source.falloff {
            LightFalloff::Smooth => 0,
            LightFalloff::Linear => 1,
            LightFalloff::Constant => 2,
        });
        writer.bool(light.source.enabled);
        writer.f32(light.height_factor);
        write_optional_u32(
            writer,
            light
                .room
                .map(|room| u32::try_from(room).unwrap_or(u32::MAX)),
        );
        writer.bool(light.directional);
        write_optional_u32(
            writer,
            light
                .owner_prop
                .map(|owner| u32::try_from(owner).unwrap_or(u32::MAX)),
        );
    }
    Ok(())
}

fn read_lights(reader: &mut Reader<'_>, room_count: usize) -> Result<Vec<BakedLight>, String> {
    let count = reader.count(MAX_COMPILED_LIGHTS, "light count")?;
    let mut lights = Vec::with_capacity(count.min(16384));
    for _ in 0..count {
        let shape = match reader.u8()? {
            0 => LightShape::Point,
            1 => LightShape::Rect {
                half_width: read_finite(reader, "rect half width")?,
                half_depth: read_finite(reader, "rect half depth")?,
            },
            2 => LightShape::Line {
                length: read_finite(reader, "line length")?,
            },
            other => return Err(format!("unknown light shape {other}")),
        };
        let position = reader.f32_3()?;
        if !position.iter().all(|value| value.is_finite()) {
            return Err("light position is non-finite".to_string());
        }
        let rotation_degrees = read_finite(reader, "light rotation")?;
        let color = read_color(reader, "light colour")?;
        let intensity = read_finite(reader, "light intensity")?;
        let range = read_finite(reader, "light range")?;
        let falloff = match reader.u8()? {
            0 => LightFalloff::Smooth,
            1 => LightFalloff::Linear,
            2 => LightFalloff::Constant,
            other => return Err(format!("unknown light falloff {other}")),
        };
        let enabled = reader.bool()?;
        let height_factor = read_finite(reader, "light height factor")?;
        let room = read_optional_u32(reader)?;
        if let Some(room_id) = room
            && usize::try_from(room_id).map_or(true, |room_index| room_index >= room_count)
        {
            return Err("light room index is out of range".to_string());
        }
        let directional = reader.bool()?;
        let owner_prop = read_optional_u32(reader)?;
        lights.push(BakedLight {
            source: LightSource {
                shape,
                position,
                rotation_degrees,
                color,
                intensity,
                range,
                falloff,
                enabled,
            },
            height_factor,
            room: room.map(|room_id| usize::try_from(room_id).unwrap_or(usize::MAX)),
            directional,
            owner_prop: owner_prop.map(|owner| usize::try_from(owner).unwrap_or(usize::MAX)),
        });
    }
    Ok(lights)
}

fn write_blends(writer: &mut Writer, blends: &[Vec<OpeningBlend>]) -> Result<(), String> {
    let count =
        u32::try_from(blends.len()).map_err(|error| format!("too many blend rooms: {error}"))?;
    writer.u32(count);
    for room in blends {
        let entries = u32::try_from(room.len())
            .map_err(|error| format!("too many blends in one room: {error}"))?;
        writer.u32(entries);
        for blend in room {
            writer.f32(blend.x);
            writer.f32(blend.z);
            writer.f32(blend.base_y);
            writer.f32(blend.top_y);
            writer.u32(blend.site);
            write_color(writer, blend.neighbor_baseline);
            write_color(writer, blend.own_baseline);
            write_optional_u32(writer, blend.own_zone);
        }
    }
    Ok(())
}

fn read_blends(
    reader: &mut Reader<'_>,
    rooms: Option<&[RoomLighting]>,
    zone_counts: Option<&[usize]>,
) -> Result<Vec<Vec<OpeningBlend>>, String> {
    let count = reader.count(MAX_COMPILED_ROOMS, "blend room count")?;
    if let Some(zone_rooms) = rooms
        && count != zone_rooms.len()
    {
        return Err(format!(
            "blend record lists {count} rooms but the bake has {}",
            zone_rooms.len()
        ));
    }
    let mut blends = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        let entries = reader.count(MAX_COMPILED_BLENDS_PER_ROOM, "blend count")?;
        let mut room = Vec::with_capacity(entries.min(4096));
        for _ in 0..entries {
            let x = read_finite(reader, "blend x")?;
            let z = read_finite(reader, "blend z")?;
            let base_y = read_finite(reader, "blend base")?;
            let top_y = read_finite(reader, "blend top")?;
            let site = reader.u32()?;
            let neighbor_baseline = read_color(reader, "blend neighbour")?;
            let own_baseline = read_color(reader, "blend own")?;
            let own_zone = read_optional_u32(reader)?;
            if let (Some(counts), Some(zone)) = (zone_counts, own_zone) {
                let in_range = counts
                    .get(usize::try_from(site).unwrap_or(usize::MAX))
                    .is_some_and(|zones| {
                        usize::try_from(zone).is_ok_and(|zone_index| zone_index < *zones)
                    });
                if !in_range {
                    return Err("blend zone index is out of range".to_string());
                }
            }
            room.push(OpeningBlend {
                x,
                z,
                base_y,
                top_y,
                site,
                neighbor_baseline,
                own_baseline,
                own_zone,
            });
        }
        blends.push(room);
    }
    Ok(blends)
}

fn write_room_zones(writer: &mut Writer, zones: &[Option<RoomZones>]) -> Result<(), String> {
    let count =
        u32::try_from(zones.len()).map_err(|error| format!("too many zone rooms: {error}"))?;
    writer.u32(count);
    for room in zones {
        let Some(room_zones) = room else {
            writer.u8(0);
            continue;
        };
        writer.u8(1);
        writer.blob_f32s(&room_zones.edges_x)?;
        writer.blob_f32s(&room_zones.edges_z)?;
        writer.blob_u32s(&room_zones.zone_of_cell)?;
        let entries = u32::try_from(room_zones.zones.len())
            .map_err(|error| format!("too many zones in one room: {error}"))?;
        writer.u32(entries);
        for zone in &room_zones.zones {
            write_color(writer, zone.baseline);
            writer.f32(zone.area_m2);
            write_color(writer, zone.power);
            let fixtures = u32::try_from(zone.fixture_count)
                .map_err(|error| format!("zone fixture count is too large: {error}"))?;
            writer.u32(fixtures);
        }
    }
    Ok(())
}

fn read_room_zones(
    reader: &mut Reader<'_>,
    rooms: &[RoomLighting],
) -> Result<Vec<Option<RoomZones>>, String> {
    let count = reader.count(MAX_COMPILED_ROOMS, "zone room count")?;
    if count != rooms.len() {
        return Err(format!(
            "zone record lists {count} rooms but the bake has {}",
            rooms.len()
        ));
    }
    let mut all = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        if reader.u8()? == 0 {
            all.push(None);
            continue;
        }
        let edges_x = reader.f32s(MAX_COMPILED_ZONE_CELLS)?;
        let edges_z = reader.f32s(MAX_COMPILED_ZONE_CELLS)?;
        let zone_of_cell = reader.u32s(MAX_COMPILED_ZONE_CELLS)?;
        let entries = reader.count(MAX_COMPILED_ZONE_CELLS, "zone count")?;
        let mut zones = Vec::with_capacity(entries.min(4096));
        for _ in 0..entries {
            zones.push(ZoneLight {
                baseline: read_color(reader, "zone baseline")?,
                area_m2: read_finite(reader, "zone area")?,
                power: read_color(reader, "zone power")?,
                fixture_count: usize::try_from(reader.u32()?)
                    .map_err(|error| format!("zone fixture count is too large: {error}"))?,
            });
        }
        let cells = u64::try_from(edges_x.len().saturating_sub(1))
            .unwrap_or(u64::MAX)
            .saturating_mul(u64::try_from(edges_z.len().saturating_sub(1)).unwrap_or(u64::MAX));
        if u64::try_from(zone_of_cell.len()).unwrap_or(u64::MAX) != cells {
            return Err("zone cell count does not match its edge lists".to_string());
        }
        if zone_of_cell
            .iter()
            .any(|zone| usize::try_from(*zone).map_or(true, |zone_index| zone_index >= zones.len()))
        {
            return Err("zone cell index is out of range".to_string());
        }
        all.push(Some(RoomZones {
            edges_x,
            edges_z,
            zone_of_cell,
            zones,
        }));
    }
    Ok(all)
}

fn write_u32_list(writer: &mut Writer, values: &[u32]) -> Result<(), String> {
    let count =
        u32::try_from(values.len()).map_err(|error| format!("list is too long: {error}"))?;
    writer.u32(count);
    for value in values {
        writer.u32(*value);
    }
    Ok(())
}

fn read_u32_list(reader: &mut Reader<'_>, max_index: usize) -> Result<Vec<u32>, String> {
    let count = reader.count(MAX_COMPILED_LIGHTS, "index list")?;
    let mut values = Vec::with_capacity(count.min(16384));
    for _ in 0..count {
        let value = reader.u32()?;
        if usize::try_from(value).map_or(true, |value_index| value_index >= max_index.max(1)) {
            return Err("index list entry is out of range".to_string());
        }
        values.push(value);
    }
    Ok(values)
}

fn write_u32_lists(writer: &mut Writer, lists: &[Vec<u32>]) -> Result<(), String> {
    let count =
        u32::try_from(lists.len()).map_err(|error| format!("too many index lists: {error}"))?;
    writer.u32(count);
    for list in lists {
        write_u32_list(writer, list)?;
    }
    Ok(())
}

fn read_u32_lists(
    reader: &mut Reader<'_>,
    list_count: usize,
    max_index: usize,
) -> Result<Vec<Vec<u32>>, String> {
    let count = reader.count(MAX_COMPILED_ROOMS, "index list count")?;
    if count != list_count {
        return Err(format!(
            "index record lists {count} rooms but the bake has {list_count}"
        ));
    }
    let mut lists = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        lists.push(read_u32_list(reader, max_index)?);
    }
    Ok(lists)
}

fn write_fixture_lights(writer: &mut Writer, fixtures: &[Option<usize>]) -> Result<(), String> {
    let count =
        u32::try_from(fixtures.len()).map_err(|error| format!("too many fixtures: {error}"))?;
    writer.u32(count);
    for fixture in fixtures {
        write_optional_u32(
            writer,
            fixture.map(|index| u32::try_from(index).unwrap_or(u32::MAX)),
        );
    }
    Ok(())
}

fn read_fixture_lights(
    reader: &mut Reader<'_>,
    max_light: usize,
) -> Result<Vec<Option<usize>>, String> {
    let count = reader.count(MAX_COMPILED_LIGHTS, "fixture light count")?;
    let mut fixtures = Vec::with_capacity(count.min(16384));
    for _ in 0..count {
        let value = read_optional_u32(reader)?;
        if let Some(fixture_id) = value
            && usize::try_from(fixture_id).map_or(true, |fixture_index| fixture_index >= max_light)
        {
            return Err("fixture light index is out of range".to_string());
        }
        fixtures.push(value.map(|fixture_id| usize::try_from(fixture_id).unwrap_or(usize::MAX)));
    }
    Ok(fixtures)
}

/// Dynamic-range audit of the baked light, over real lightmap texels.
///
/// Test-only: it is the measurement the rebalance was calibrated against and
/// the guard that keeps the model from drifting back into saturation.
#[cfg(test)]
mod range_audit;

#[cfg(test)]
mod room_index_tests {
    use super::{
        CeilingProfileDef, LevelLighting, LightColor, ROOM_EDGE_EPS_M, RoomIndex, RoomLighting,
        WallAxis,
    };

    const fn room(x: f32, z: f32, y: f32, size: f32) -> RoomLighting {
        RoomLighting {
            x0: x,
            x1: x + size,
            z0: z,
            z1: z + size,
            floor_y: y,
            height_m: 3.0,
            profile: CeilingProfileDef::Flat,
            area_m2: size * size,
            fixture_count: 0,
            effective_power: LightColor::BLACK,
            baseline: LightColor::BLACK,
        }
    }

    fn check(rooms: Vec<RoomLighting>) {
        let lighting = LevelLighting {
            room_index: RoomIndex::new(&rooms),
            rooms,
            ..LevelLighting::default()
        };
        let mut coordinates = vec![-1000.0, 1000.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY];
        for room in &lighting.rooms {
            for edge in [room.x0, room.x1, room.z0, room.z1] {
                coordinates.extend([edge - ROOM_EDGE_EPS_M, edge, edge + ROOM_EDGE_EPS_M]);
            }
        }
        coordinates.sort_by(f32::total_cmp);
        coordinates.dedup_by(|a, b| a.to_bits() == b.to_bits());
        for &x in &coordinates {
            for &z in &coordinates {
                assert_eq!(
                    lighting.room_index_at(x, z),
                    LevelLighting::room_index_of(&lighting.rooms, x, z)
                );
                assert_eq!(
                    lighting.room_index_strict_at(x, z),
                    LevelLighting::room_index_strict_of(&lighting.rooms, x, z)
                );
                for y in [-1.0, 0.0, 1.5, 3.0, 4.5, 6.0, 9.0, f32::NAN] {
                    assert_eq!(
                        lighting.room_index_at_height(x, y, z),
                        LevelLighting::room_index_at_height_of(&lighting.rooms, x, y, z)
                    );
                    let expected = lighting
                        .rooms
                        .iter()
                        .enumerate()
                        .filter(|(_, room)| {
                            let (floor, ceiling) = room.span_at(x, z);
                            x.is_finite()
                                && y.is_finite()
                                && z.is_finite()
                                && x >= room.x0 + ROOM_EDGE_EPS_M
                                && x <= room.x1 - ROOM_EDGE_EPS_M
                                && z >= room.z0 + ROOM_EDGE_EPS_M
                                && z <= room.z1 - ROOM_EDGE_EPS_M
                                && y >= floor - ROOM_EDGE_EPS_M
                                && y <= ceiling + ROOM_EDGE_EPS_M
                        })
                        .min_by(|(a, left), (b, right)| {
                            left.area_m2.total_cmp(&right.area_m2).then(a.cmp(b))
                        })
                        .map(|(index, _)| index);
                    assert_eq!(lighting.room_index_strict_at_height(x, y, z), expected);
                }
            }
        }
    }

    #[test]
    fn nonfinite_room_bounds_keep_linear_candidates_above_index_threshold() {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for bound in 0_i32..4_i32 {
                let mut rooms = (0_u16..9)
                    .map(|index| room(f32::from(index) * 10.0, 0.0, 0.0, 6.0))
                    .collect::<Vec<_>>();
                if let Some(room) = rooms.first_mut() {
                    match bound {
                        0_i32 => room.x0 = invalid,
                        1_i32 => room.x1 = invalid,
                        2_i32 => room.z0 = invalid,
                        _ => room.z1 = invalid,
                    }
                }
                let lighting = LevelLighting {
                    room_index: RoomIndex::new(&rooms),
                    rooms,
                    ..LevelLighting::default()
                };
                assert_eq!(lighting.room_index.cells.len(), 1);
                let all: Vec<_> = (0..lighting.rooms.len()).collect();
                for (x, z) in [(-1000.0, 3.0), (3.0, 3.0), (83.0, 3.0), (3.0, 1000.0)] {
                    assert_eq!(lighting.room_index.candidates(x, z), all.as_slice());
                    assert_eq!(
                        lighting.room_index_at(x, z),
                        LevelLighting::room_index_of(&lighting.rooms, x, z)
                    );
                    assert_eq!(
                        lighting.room_index_strict_at(x, z),
                        LevelLighting::room_index_strict_of(&lighting.rooms, x, z)
                    );
                    for y in [-1.0, 1.5, 6.0] {
                        assert_eq!(
                            lighting.room_index_at_height(x, y, z),
                            LevelLighting::room_index_at_height_of(&lighting.rooms, x, y, z)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn spatial_room_queries_match_linear_reference() {
        check(Vec::new());
        check(vec![room(-2.0, -2.0, 0.0, 4.0)]);
        let mut rooms = (0_u16..100)
            .map(|index| {
                room(
                    f32::from(index % 10) * 10.0,
                    f32::from(index / 10) * 10.0,
                    0.0,
                    6.0,
                )
            })
            .collect::<Vec<_>>();
        rooms.extend([
            room(0.0, 0.0, 0.0, 6.0),
            room(1.0, 1.0, 0.0, 2.0),
            room(0.0, 0.0, 3.0, 6.0),
        ]);
        if let Some(room) = rooms.first_mut() {
            room.profile = CeilingProfileDef::Gable {
                ridge: WallAxis::X,
                ridge_rise: 2.0,
            };
        }
        check(rooms);
    }
}

#[cfg(test)]
mod light_index_tests {
    use super::{
        CeilingProfileDef, LevelDef, LevelLighting, LightColor, LightFalloff, LightIndex,
        LightMount, WallAxis, fill_range_for,
    };

    const fn bits(color: LightColor) -> [u32; 3] {
        [color.r.to_bits(), color.g.to_bits(), color.b.to_bits()]
    }

    fn fixture() -> Result<LevelDef, String> {
        let mut level =
            LevelDef::from_json(include_str!("../../tests/fixtures/levels/test_room.json"))
                .map_err(|error| error.to_string())?;
        let fixture = level
            .ceiling_lights
            .first()
            .cloned()
            .ok_or("fixture has no light")?;
        let room = level.rooms.first().cloned().ok_or("fixture has no room")?;
        level.rooms = vec![room.clone(), room.clone(), room];
        if let Some(upper_room) = level.rooms.get_mut(1) {
            upper_room.floor_y = 4.0;
            upper_room.ceiling = CeilingProfileDef::Gable {
                ridge: WallAxis::X,
                ridge_rise: 2.0,
            };
        }
        if let Some(lower_room) = level.rooms.get_mut(2) {
            lower_room.x = -1.0;
            lower_room.z = -1.0;
            lower_room.width = 2.0;
            lower_room.depth = 2.0;
        }
        level.ceiling_lights = (0_u16..100)
            .map(|index| {
                let mut light = fixture.clone();
                light.id = None;
                light.x = f32::mul_add(f32::from(index % 10), 5.0, -25.0);
                light.z = f32::mul_add(f32::from(index / 10), 5.0, -25.0);
                light.y = Some(if index % 3 == 0 { 6.5 } else { 2.5 });
                light.range = Some(1.0 + f32::from(index % 7));
                light.rotation_degrees = if index % 2 == 0 { 90.0 } else { 0.0 };
                light.mount = if index % 4 == 0 {
                    LightMount::Wall
                } else {
                    LightMount::Ceiling
                };
                light.falloff = Some(match index % 3 {
                    0 => LightFalloff::Constant,
                    1 => LightFalloff::Linear,
                    _ => LightFalloff::Smooth,
                });
                light.enabled = index % 9 != 0;
                light
            })
            .collect();
        Ok(level)
    }

    #[test]
    fn spatial_light_samples_match_full_candidates_exactly() -> Result<(), String> {
        let level = fixture()?;
        for count in [0, 1, 100] {
            let mut selected = level.clone();
            selected.ceiling_lights.truncate(count);
            let indexed = LevelLighting::bake(&selected);
            let mut reference = indexed.clone();
            reference.light_index = LightIndex {
                cells: vec![indexed.all_lights.clone()],
                ..LightIndex::default()
            };
            let mut points = Vec::new();
            for x in -8_i16..=8 {
                for z in -8_i16..=8 {
                    points.push((f32::from(x) * 3.0, f32::from(z) * 3.0));
                }
            }
            for light in &indexed.lights {
                let (half_w, half_d) = light.source.half_extents();
                let reach = fill_range_for(light.source.range);
                for edge in [light.x() - half_w - reach, light.x() + half_w + reach] {
                    points.extend([
                        (edge.next_down(), light.z()),
                        (edge, light.z()),
                        (edge.next_up(), light.z()),
                    ]);
                }
                points.push((light.x(), light.z() + half_d + reach));
            }
            points.extend([(f32::NAN, 0.0), (0.0, f32::INFINITY), (10000.0, -10000.0)]);
            for (x, z) in points {
                for y in [-1.0, 0.0, 1.5, 3.5, 4.0, 6.0, 8.0, f32::NAN] {
                    assert_eq!(
                        bits(indexed.sample(x, y, z)),
                        bits(reference.sample(x, y, z))
                    );
                    for room in [None, Some(0), Some(1), Some(2)] {
                        assert_eq!(
                            bits(indexed.sample_face(room, x, y, z)),
                            bits(reference.sample_face(room, x, y, z))
                        );
                        assert_eq!(
                            bits(indexed.lightmap_texel(room, x, y, z)),
                            bits(reference.lightmap_texel(room, x, y, z))
                        );
                    }
                }
            }
        }
        Ok(())
    }
}

/// Vec capacity is required here: a slice omits allocated unused elements.
const fn allocation_bytes<T>(values: &Vec<T>) -> usize {
    values.capacity().saturating_mul(std::mem::size_of::<T>())
}
