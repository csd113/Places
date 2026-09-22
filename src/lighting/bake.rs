//! The static bake and the queries it answers.
//!
//! [`LevelLighting::bake`] turns a level definition into room baselines, one
//! local pool per fixture, the bounded doorway blends between rooms, and the
//! static wall-visibility set that keeps a fixture from lighting what it cannot
//! see. Everything in this module runs once per level load; the render loop only
//! reads the vertex colours that were baked from it.

use super::color::LightColor;
use super::math::{
    ceiling_height_factor, effective_power, fixture_half_extents_for, room_baseline,
    sanitize_intensity, smooth_falloff,
};
use super::tuning::WALL_LIGHT_DEFAULT_HEIGHT_M;
use super::tuning::*;
use super::visibility::{QuerySite, Visibility};
use crate::level::{LevelDef, LightMount, WallAxis};

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
    pub profile: crate::level::CeilingProfileDef,
    /// Floor area in square metres.
    pub area_m2: f32,
    /// Number of ceiling fixtures owned by this room.
    pub fixture_count: usize,
    /// Summed emitted colour of the owned fixtures, each scaled by
    /// `intensity x ceiling-height factor`.
    pub effective_power: LightColor,
    /// Baked baseline illumination, every channel inside
    /// `[AMBIENT_LEVEL, MAX_BRIGHTNESS]`.
    pub baseline: LightColor,
}

/// One ceiling fixture resolved for baking.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BakedLight {
    pub x: f32,
    pub z: f32,
    /// World Y of the fixture panel: the lowest point of its room's ceiling
    /// over the panel footprint, minus [`FIXTURE_DROP_M`]. Under a gable the
    /// panel therefore hangs below the slope instead of intersecting it.
    pub y: f32,
    /// Sanitised authored intensity.
    pub intensity: f32,
    /// Sanitised emitted colour.
    pub color: LightColor,
    /// Ceiling-height correction of the owned room.
    pub height_factor: f32,
    /// Half-extents of the luminous panel in world X/Z, after rotation.
    pub half_w: f32,
    pub half_d: f32,
    /// Owning room, or `None` when no room contains the fixture.
    pub room: Option<usize>,
}

/// Aggregate bake statistics, used for developer logging and tests.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct LightingSummary {
    pub rooms: usize,
    pub lights: usize,
    /// Opaque wall boxes the bake tests light against.
    pub blockers: usize,
    pub min_baseline: f32,
    pub max_baseline: f32,
    pub average_baseline: f32,
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
    let Some(room) = room else {
        return fallback;
    };
    let mut lowest = f32::INFINITY;
    for corner_x in [x - half_w, x + half_w] {
        for corner_z in [z - half_d, z + half_d] {
            lowest = lowest.min(room.ceiling_y_at(corner_x, corner_z));
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
    let floor_y =
        LevelLighting::room_index_of(rooms, x, z).map_or(0.0, |index| rooms[index].floor_y);
    floor_y + WALL_LIGHT_DEFAULT_HEIGHT_M
}

/// A doorway/passage link between two rooms.
#[derive(Clone, Copy, Debug, PartialEq)]
struct OpeningBlend {
    x: f32,
    z: f32,
    /// Floor Y of the lower of the two rooms the opening joins, used as the
    /// bottom of the aperture when testing whether the sample can see through.
    base_y: f32,
    /// Top edge of the opening in world Y.
    top_y: f32,
    /// Query site of this opening in [`LevelLighting::visibility`].
    site: u32,
    /// Baseline of the room on the other side of the opening.
    neighbor_baseline: LightColor,
}

impl OpeningBlend {
    /// World point the opening's light is treated as coming from: the middle of
    /// the aperture. Only the visibility test uses it.
    fn source(&self) -> [f32; 3] {
        [self.x, f32::midpoint(self.base_y, self.top_y), self.z]
    }
}

/// Fully baked static lighting for one level.
///
/// Cheap to keep resident (a few dozen bytes per room and fixture) and sampled
/// only while the level geometry is being built.
#[derive(Clone, Debug, Default)]
pub struct LevelLighting {
    rooms: Vec<RoomLighting>,
    lights: Vec<BakedLight>,
    /// Per room, the opening links that blend neighbouring light into it.
    blends: Vec<Vec<OpeningBlend>>,
    /// Static opaque wall geometry, used to keep a fixture's local pool from
    /// lighting surfaces it cannot see. Built once per level load.
    visibility: Visibility,
    /// Per room, indices of the fixtures whose pool can reach that room, in
    /// fixture order. A fixture outside the list is farther than
    /// [`LOCAL_LIGHT_RADIUS_M`] from every point in the room, so pruning is
    /// exact and the per-vertex sum is unchanged.
    room_lights: Vec<Vec<u32>>,
    /// Every fixture index, for samples outside all rooms.
    all_lights: Vec<u32>,
    /// Ceiling plane used for fixtures that no room contains.
    default_ceiling_y: f32,
    /// Clear height used for the height factor of fixtures outside every room.
    default_height_m: f32,
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
}

/// A closed interval on the floor plane: `(min, max)`.
type Span = (f32, f32);

/// How far apart two spans are on one axis, or zero when they touch or overlap.
fn interval_gap(room_span: Span, panel_span: Span) -> f32 {
    (room_span.0 - panel_span.1)
        .max(panel_span.0 - room_span.1)
        .max(0.0)
}

impl LevelLighting {
    /// Bakes room baselines, fixture pools and opening blends from a level.
    ///
    /// Malformed data never panics and never yields NaN: non-finite fixtures are
    /// skipped, non-finite dimensions fall back to safe values and every result
    /// is clamped.
    #[must_use]
    pub fn bake(level: &LevelDef) -> Self {
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
            // like `RoomDef::ceiling_y_at` would resolve it.
            let profile = match room.ceiling {
                crate::level::CeilingProfileDef::Gable { ridge_rise, .. }
                    if !(ridge_rise.is_finite() && ridge_rise > 0.0) =>
                {
                    crate::level::CeilingProfileDef::Flat
                }
                profile => profile,
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

        // Resolve every fixture once: ownership, fixture plane, rotated panel
        // footprint and its contribution to its room's effective power.
        let mut lights: Vec<BakedLight> = Vec::with_capacity(level.ceiling_lights.len());
        for light in &level.ceiling_lights {
            if !light.x.is_finite() || !light.z.is_finite() {
                continue;
            }
            let room = Self::room_index_of(&rooms, light.x, light.z);
            // The height factor is calibrated against the room's eave, so a
            // gable ridge changes the ceiling's shape but not the room's
            // illumination response.
            let height_m = room.map_or(default_height_m, |index| rooms[index].height_m);
            let height_factor = ceiling_height_factor(height_m);
            let intensity = sanitize_intensity(light.intensity());
            let color = light.emitted_color();
            // Rotation swaps the panel's long axis, exactly like the fixture
            // geometry emitted by `crate::render` (shared helper, so a
            // fractional rotation cannot drift between the two). The fixture
            // family owns the footprint, so a round downlight pools light in a
            // small disc while the office panel pools it over its rectangle.
            let profile = fixture_profile(&light.fixture);
            let (half_w, half_d) = fixture_half_extents_for(profile.kind, light.rotation_degrees);
            let panel_y = match light.mount {
                LightMount::Ceiling => {
                    panel_min_ceiling_y(
                        room.map(|index| &rooms[index]),
                        default_ceiling_y,
                        light.x,
                        light.z,
                        half_w,
                        half_d,
                    ) - FIXTURE_DROP_M
                }
                // A wall fixture is authored at its own world height; the
                // fallback only keeps a hand-edited level finite.
                LightMount::Wall => resolve_wall_fixture_y(&rooms, light.x, light.z, light.y),
            };
            if let Some(index) = room {
                rooms[index].fixture_count += 1;
                let power = effective_power(intensity, height_m);
                rooms[index].effective_power.r += power * color.r;
                rooms[index].effective_power.g += power * color.g;
                rooms[index].effective_power.b += power * color.b;
            }
            lights.push(BakedLight {
                x: light.x,
                z: light.z,
                y: panel_y,
                intensity,
                color,
                height_factor,
                half_w,
                half_d,
                room,
            });
        }

        for room in &mut rooms {
            room.baseline = room_baseline(room.area_m2, room.effective_power);
        }

        // Per-room fixture candidates: only fixtures whose panel can come
        // within `LOCAL_LIGHT_RADIUS_M` of the room footprint, always including
        // the owning room. Built in fixture order so the per-vertex sum (and
        // its early saturation) is bit-identical to checking every fixture.
        let mut room_lights: Vec<Vec<u32>> = vec![Vec::new(); rooms.len()];
        for (index, light) in lights.iter().enumerate() {
            for (room_index, room) in rooms.iter().enumerate() {
                if light.room == Some(room_index) || Self::light_reaches_room(light, room) {
                    room_lights[room_index].push(u32::try_from(index).unwrap_or(u32::MAX));
                }
            }
        }
        let all_lights: Vec<u32> = (0..u32::try_from(lights.len()).unwrap_or(u32::MAX)).collect();

        // Link the rooms on either side of every walk-through opening.
        let mut blends: Vec<Vec<OpeningBlend>> = vec![Vec::new(); rooms.len()];
        let mut blend_sites: Vec<QuerySite> = Vec::new();
        let light_site_count = u32::try_from(lights.len()).unwrap_or(u32::MAX);
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
                let room_a = Self::room_index_of(&rooms, side_a.0, side_a.1);
                let room_b = Self::room_index_of(&rooms, side_b.0, side_b.1);
                let (Some(room_a), Some(room_b)) = (room_a, room_b) else {
                    continue;
                };
                if room_a == room_b {
                    continue;
                }
                // A walk-through opening has to reach the floor it connects: a
                // wall raised off the floor (`wall.y`) is a header or lintel,
                // not a passage, and an opening that only reaches an upper
                // room's floor does not join the two rooms for light either.
                let floor = rooms[room_a].floor_y.min(rooms[room_b].floor_y);
                if wall.y + opening.sill.max(0.0) > floor + 1e-3 {
                    continue;
                }
                let top_y = opening.top(wall.y);
                let site = light_site_count
                    .saturating_add(u32::try_from(blend_sites.len()).unwrap_or(u32::MAX));
                blend_sites.push(QuerySite::new(center_x, center_z, OPENING_BLEND_RADIUS_M));
                blends[room_a].push(OpeningBlend {
                    x: center_x,
                    z: center_z,
                    base_y: floor,
                    top_y,
                    site,
                    neighbor_baseline: rooms[room_b].baseline,
                });
                blends[room_b].push(OpeningBlend {
                    x: center_x,
                    z: center_z,
                    base_y: floor,
                    top_y,
                    site,
                    neighbor_baseline: rooms[room_a].baseline,
                });
            }
        }

        // Opaque wall geometry for the whole bake. Query sites are the fixtures
        // first (one per light, in fixture order) and then the doorway blends,
        // so a fixture's site index is exactly its own index.
        let mut sites: Vec<QuerySite> = Vec::with_capacity(lights.len() + blend_sites.len());
        for light in &lights {
            sites.push(QuerySite::new(
                light.x,
                light.z,
                LOCAL_LIGHT_RADIUS_M.max(light.half_w).max(light.half_d),
            ));
        }
        sites.extend_from_slice(&blend_sites);
        let visibility = Visibility::build(level, &sites);

        Self {
            rooms,
            lights,
            blends,
            visibility,
            room_lights,
            all_lights,
            default_ceiling_y,
            default_height_m,
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

    /// The room a world position belongs to, or `None` outside every room.
    ///
    /// Deterministic ownership rule for the overlapping/intersecting rooms this
    /// engine allows: the *smallest-area* containing room wins, and equal areas
    /// keep the level's own room order. This is the single shared containment
    /// helper used for lighting, so fixtures are never double counted.
    #[must_use]
    pub fn room_index_at(&self, x: f32, z: f32) -> Option<usize> {
        Self::room_index_of(&self.rooms, x, z)
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
                Some(current) if rooms[current].area_m2 <= room.area_m2 => {}
                _ => best = Some(index),
            }
        }
        best
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
        Self::room_index_strict_of(&self.rooms, x, z)
    }

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
                Some(current) if rooms[current].area_m2 <= room.area_m2 => {}
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
        if let Some(room) = self.room_index_strict_at(x, z) {
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
            self.room_index_at(x, z).map(|index| &self.rooms[index]),
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

    /// World Y of a wall-mounted fixture at `(x, z)`.
    ///
    /// The authored height wins; a hand-edited level without one falls back to
    /// [`WALL_LIGHT_DEFAULT_HEIGHT_M`] above the owning room's floor. Shared by
    /// the bake and the fixture geometry.
    #[must_use]
    pub fn wall_fixture_y(&self, x: f32, z: f32, authored: Option<f32>) -> f32 {
        resolve_wall_fixture_y(&self.rooms, x, z, authored)
    }

    /// Clear eave height of the room owning `(x, z)`, for tests and diagnostics.
    #[must_use]
    pub fn ceiling_height_at(&self, x: f32, z: f32) -> f32 {
        self.room_index_at(x, z)
            .map_or(self.default_height_m, |index| self.rooms[index].height_m)
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
            let walked = step as f32 * CLEAR_SAMPLE_STEP_M;
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
    /// containment.
    ///
    /// Used for props and for geometry that does not know its room. Points
    /// outside every room still receive the ambient fill and any local fixture
    /// pools they are inside.
    #[must_use]
    pub fn sample(&self, x: f32, y: f32, z: f32) -> LightColor {
        self.room_index_at(x, z).map_or_else(
            || {
                self.local_light(&self.all_lights, x, y, z)
                    .plus(ambient_color())
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
    #[must_use]
    pub fn sample_in_room(&self, room: usize, x: f32, y: f32, z: f32) -> LightColor {
        let Some(info) = self.rooms.get(room) else {
            return self.sample(x, y, z);
        };
        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return ambient_color();
        }
        let (x, z) = self.clear_sample(room, x, z);

        let candidates = &self.room_lights[room];
        let mut value = info.baseline.plus(self.local_light(candidates, x, y, z));
        value = value.plus(self.blend_delta(room, x, y, z));

        if value.is_finite() {
            value.clamped(AMBIENT_LEVEL, MAX_BRIGHTNESS)
        } else {
            ambient_color()
        }
    }

    /// The doorway-blend part of [`Self::sample_in_room`] at a world position.
    ///
    /// This is the exchange between the rooms on either side of a walk-through
    /// opening, isolated from the room's own baseline and fixture pools: a delta
    /// that is negative on the brighter side and positive on the dimmer one. It
    /// is exposed for the editor parity mirror, the developer log and the
    /// doorway regression tests, which need to prove the blending is bounded,
    /// symmetric and blind to opaque walls without the local pools moving
    /// underneath the measurement.
    #[must_use]
    pub fn opening_blend(&self, room: usize, x: f32, y: f32, z: f32) -> LightColor {
        if !self.rooms.get(room).is_some() || !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return LightColor::BLACK;
        }
        let (x, z) = self.clear_sample(room, x, z);
        let delta = self.blend_delta(room, x, y, z);
        if delta.is_finite() {
            delta
        } else {
            LightColor::BLACK
        }
    }

    fn blend_delta(&self, room: usize, x: f32, y: f32, z: f32) -> LightColor {
        let Some(info) = self.rooms.get(room) else {
            return LightColor::BLACK;
        };
        let mut delta = LightColor::BLACK;
        for blend in &self.blends[room] {
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
                r: (blend.neighbor_baseline.r - info.baseline.r).mul_add(influence, delta.r),
                g: (blend.neighbor_baseline.g - info.baseline.g).mul_add(influence, delta.g),
                b: (blend.neighbor_baseline.b - info.baseline.b).mul_add(influence, delta.b),
            };
        }
        delta
    }

    /// Local fixture pools at a world position: broad, smooth and bounded.
    ///
    /// Each fixture's contribution falls from [`LOCAL_LIGHT_STRENGTH`] at its
    /// panel to zero at [`LOCAL_LIGHT_RADIUS_M`], scaled per channel by the
    /// fixture's emitted colour, intensity and room ceiling-height factor. The
    /// summed colour is capped per channel at [`LOCAL_LIGHT_MAX`] so clusters
    /// stay in range; a fixture emits nothing at all in a channel whose colour
    /// is zero.
    ///
    /// `candidates` are indices into [`Self::lights`]; squared distances are
    /// compared against the radius before the square root, so fixtures that
    /// cannot reach the sample are rejected with a couple of multiplies.
    fn local_light(&self, candidates: &[u32], x: f32, y: f32, z: f32) -> LightColor {
        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return LightColor::BLACK;
        }
        let radius_squared = LOCAL_LIGHT_RADIUS_M * LOCAL_LIGHT_RADIUS_M;
        let inv_radius = 1.0 / LOCAL_LIGHT_RADIUS_M;
        let mut sum = LightColor::BLACK;
        for index in candidates {
            let light = &self.lights[*index as usize];
            // Horizontal distance to the rotated panel footprint.
            let dx = ((x - light.x).abs() - light.half_w).max(0.0);
            let dz = ((z - light.z).abs() - light.half_d).max(0.0);
            let horizontal_squared = dx * dx + dz * dz;
            if !horizontal_squared.is_finite() || horizontal_squared >= radius_squared {
                continue;
            }
            // Full 3D distance to the panel, so a wall at fixture height reads
            // brighter than the floor below it.
            let vertical = y - light.y;
            let distance_squared = vertical.mul_add(vertical, horizontal_squared);
            if !distance_squared.is_finite() || distance_squared >= radius_squared {
                continue;
            }
            // Static opaque-wall visibility: a fixture contributes only where
            // its panel can actually see the sample. The segment starts at the
            // closest point of the panel, so a wide fixture is not blocked by a
            // wall its brighter edge can see past.
            let source = [
                x.clamp(light.x - light.half_w, light.x + light.half_w),
                light.y,
                z.clamp(light.z - light.half_d, light.z + light.half_d),
            ];
            if self.visibility.occludes(*index, source, [x, y, z]) {
                continue;
            }
            let falloff = smooth_falloff(distance_squared.sqrt() * inv_radius);
            let strength = LOCAL_LIGHT_STRENGTH * light.intensity * light.height_factor * falloff;
            sum = LightColor {
                r: strength.mul_add(light.color.r, sum.r),
                g: strength.mul_add(light.color.g, sum.g),
                b: strength.mul_add(light.color.b, sum.b),
            };
            if sum.min_channel() >= LOCAL_LIGHT_MAX {
                return LightColor::grey(LOCAL_LIGHT_MAX);
            }
        }
        sum.clamped(0.0, LOCAL_LIGHT_MAX)
    }

    /// True when a fixture's panel can come within [`LOCAL_LIGHT_RADIUS_M`] of
    /// some point above a room's footprint.
    ///
    /// Used to build the per-room candidate lists: a fixture this test rejects
    /// contributes exactly zero everywhere in the room, so pruning is lossless.
    /// The test ignores vertical distance, which only makes it more permissive.
    fn light_reaches_room(light: &BakedLight, room: &RoomLighting) -> bool {
        let panel_span_x = (light.x - light.half_w, light.x + light.half_w);
        let panel_span_z = (light.z - light.half_d, light.z + light.half_d);
        let room_span_x = (room.x0 - ROOM_EDGE_EPS_M, room.x1 + ROOM_EDGE_EPS_M);
        let room_span_z = (room.z0 - ROOM_EDGE_EPS_M, room.z1 + ROOM_EDGE_EPS_M);
        let gap_x = interval_gap(room_span_x, panel_span_x);
        let gap_z = interval_gap(room_span_z, panel_span_z);
        gap_x.mul_add(gap_x, gap_z * gap_z) < LOCAL_LIGHT_RADIUS_M * LOCAL_LIGHT_RADIUS_M
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
                min_baseline: 0.0,
                max_baseline: 0.0,
                average_baseline: 0.0,
            };
        }
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        let mut total = 0.0;
        for room in &self.rooms {
            let luminance = room.baseline.luminance();
            min = min.min(luminance);
            max = max.max(luminance);
            total += luminance;
        }
        LightingSummary {
            rooms: self.rooms.len(),
            lights: self.lights.len(),
            blockers: self.visibility.blocker_count(),
            min_baseline: min,
            max_baseline: max,
            average_baseline: total / self.rooms.len() as f32,
        }
    }
}
