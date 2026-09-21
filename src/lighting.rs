//! Static baked interior lighting.
//!
//! The game targets a `PocketCHIP` (Mali-400/Lima, OpenGL ES 2.0, 480x272), so
//! there is no dynamic lighting anywhere in the render loop. Everything in this
//! module runs once per level load, producing a single brightness scalar per
//! sampled point that the geometry builder bakes into ordinary vertex colours:
//!
//! ```text
//! load level
//!     -> collect rooms + ceiling fixtures            (this module)
//!     -> room area, fixture density, height factor   (this module)
//!     -> room baseline + local fixture pools         (this module)
//!     -> bounded doorway blending                    (this module)
//!     -> bake into world geometry + prop instances   (crate::render)
//!     -> upload the same static batches as before    (crate::render)
//! ```
//!
//! There is no per-frame light loop, no light texture, no extra draw call and
//! no shader change: the renderer still draws exactly the batches it drew
//! before, with darker or brighter vertex colours.
//!
//! Lighting model
//! --------------
//! 1. **Room baseline.** Every room sums the effective power of the ceiling
//!    fixtures it owns (`intensity x ceiling-height factor`), divides by its
//!    floor area and feeds that through a smoothly saturating curve. A large
//!    room with two panels is dim; a small room with many panels approaches
//!    full brightness; the result never exceeds [`MAX_BRIGHTNESS`].
//! 2. **Local fixture pools.** Every fixture adds a broad pool of light with a
//!    smooth falloff that reaches zero at [`LOCAL_LIGHT_RADIUS_M`]. The pool is
//!    measured to the fixture's rectangular panel rather than to a point, so it
//!    reads as a fluorescent panel instead of a spotlight.
//! 3. **Opening blending.** Rooms joined by walk-through openings (doors and
//!    passages that reach the floor) mix a bounded fraction of each other's
//!    baseline near the opening, so light appears to leak through doorways
//!    instead of stopping at the threshold. Windows and vents are deliberately
//!    excluded: in this engine they usually face the outside, and a raised
//!    opening does not read as a walk-through connection. Only the openings of
//!    single walls are considered; there is no recursive propagation and no
//!    global solver.
//! 4. **Minimum ambient.** A room without fixtures stays visible: this game
//!    uses empty space, not darkness, for atmosphere.
//!
//! Determinism and ownership
//! -------------------------
//! Overlapping and intersecting rooms are legal level design in this game, so
//! light ownership must be defined rather than rejected: a point (and therefore
//! a fixture) belongs to the *smallest-area* room that contains it, with ties
//! resolved by the level's own room order (`rooms`, then the optional `room`).
//! Each fixture therefore contributes to exactly one room and is never counted
//! twice. See [`LevelLighting::room_index_at`].
//!
//! All tuning values below are deliberately centralised and documented; the
//! visual verification captures in the app changelog were produced with them.

use crate::level::{LevelDef, WallAxis, ceiling_height_at};

// ---------------------------------------------------------------------------
// Tuning
// ---------------------------------------------------------------------------

/// Floor area one standard fixture is expected to illuminate, in square metres.
///
/// This is the reference point of the density curve: a room with
/// `1 / REFERENCE_LIGHT_AREA_M2` fixtures per square metre is "reasonably
/// illuminated" and lands halfway between [`MIN_AMBIENT`] and
/// [`MAX_BRIGHTNESS`].
pub const REFERENCE_LIGHT_AREA_M2: f32 = 8.0;

/// Ceiling height at which a fixture delivers its nominal output, in metres.
pub const REFERENCE_CEILING_HEIGHT_M: f32 = 3.5;

/// Exponent of the ceiling-height correction: `(reference / height) ^ falloff`.
///
/// 0.5 is a deliberately gentle inverse square root: a 2.6 m corridor makes its
/// fixtures ~16% more effective, a 4 m room ~7% less. A physically accurate
/// inverse square would make tall spaces unusably dark.
pub const HEIGHT_FALLOFF: f32 = 0.5;

/// Brightness of a room with no effective fixtures at all.
///
/// Never zero: unlit rooms stay navigable instead of turning pitch black.
pub const MIN_AMBIENT: f32 = 0.55;

/// Hard upper bound on baked brightness. Values above 1.0 would clip textured
/// surfaces to flat white and wash the level out.
pub const MAX_BRIGHTNESS: f32 = 1.0;

/// Radius in metres over which one fixture's local pool fades to nothing.
pub const LOCAL_LIGHT_RADIUS_M: f32 = 6.0;

/// Extra brightness one fixture adds directly beneath itself.
pub const LOCAL_LIGHT_STRENGTH: f32 = 0.42;

/// Cap on the summed local fixture contribution, so a dense cluster of
/// fixtures cannot drive a whole room to white.
pub const LOCAL_LIGHT_MAX: f32 = 0.45;

/// Radius in metres over which light leaks through a doorway or passage.
pub const OPENING_BLEND_RADIUS_M: f32 = 6.0;

/// Fraction of the neighbouring room's baseline mixed in at the opening itself.
/// 0.5 makes both sides of a threshold meet at the average of the two rooms,
/// which is what removes the hard brightness step.
pub const OPENING_BLEND_STRENGTH: f32 = 0.5;

/// Vertical distance above an opening over which the blend fades out: light
/// does not pass through the solid wall above a door header.
pub const OPENING_VERTICAL_FADE_M: f32 = 1.0;

/// Half-extents of a fixture's luminous panel, matching the ceiling-light
/// geometry in [`crate::render`] (a 1.2 x 0.6 m panel).
pub const FIXTURE_HALF_WIDTH_M: f32 = 0.6;
/// See [`FIXTURE_HALF_WIDTH_M`].
pub const FIXTURE_HALF_DEPTH_M: f32 = 0.3;

/// Distance a fixture hangs below its room's ceiling, in metres.
pub const FIXTURE_DROP_M: f32 = 0.01;

/// Cell size of the baked lighting grid used to tessellate floors, ceilings and
/// wall faces.
///
/// Smaller cells sample the pools more smoothly; the cell count is capped per
/// surface so the generated geometry stays bounded.
pub const LIGHT_GRID_CELL_M: f32 = 2.5;

/// Maximum subdivisions per axis of one floor or ceiling.
pub const MAX_LIGHT_GRID_CELLS: u32 = 12;

/// Maximum segments one wall face is split into along its length.
pub const MAX_WALL_LIGHT_SEGMENTS: u32 = 8;

/// Highest fixture intensity that still adds light while baking. Authored
/// values above this are clamped rather than rejected (see
/// [`crate::level::CeilingLightDef::intensity`]).
pub const MAX_LIGHT_INTENSITY: f32 = 8.0;

/// Smallest floor area used by the density calculation, guarding degenerate
/// zero-area rooms against division by zero.
pub const MIN_ROOM_AREA_M2: f32 = 0.01;

/// Distance a sample may sit outside a room's footprint and still count as
/// inside it. Wall faces, floors and ceilings sit exactly on room boundaries.
pub const ROOM_EDGE_EPS_M: f32 = 0.01;

/// Distance inside a room probed when deciding which rooms an opening joins.
const OPENING_PROBE_M: f32 = 0.05;

// ---------------------------------------------------------------------------
// Pure helpers (unit tested; no allocation, no state)
// ---------------------------------------------------------------------------

/// Sanitises an authored fixture intensity for baking.
///
/// Non-finite values fall back to the standard fixture, negatives clamp to
/// zero output and absurd values clamp to [`MAX_LIGHT_INTENSITY`]; the result is
/// always finite and non-negative.
#[must_use]
pub fn sanitize_intensity(intensity: f32) -> f32 {
    if intensity.is_nan() {
        return 1.0;
    }
    if intensity.is_infinite() {
        return if intensity > 0.0 {
            MAX_LIGHT_INTENSITY
        } else {
            0.0
        };
    }
    intensity.clamp(0.0, MAX_LIGHT_INTENSITY)
}

/// Gentle ceiling-height correction applied to one fixture's output.
///
/// A lower ceiling makes the same fixture more effective, a taller one less;
/// the correction is a bounded power law, never an inverse square.
#[must_use]
pub fn ceiling_height_factor(height_m: f32) -> f32 {
    if !height_m.is_finite() || height_m <= 0.0 {
        return 1.0;
    }
    let height = height_m.clamp(0.5, 100.0);
    (REFERENCE_CEILING_HEIGHT_M / height).sqrt()
}

/// Effective fixture power: authored intensity times the height correction.
#[must_use]
pub fn effective_power(intensity: f32, ceiling_height_m: f32) -> f32 {
    sanitize_intensity(intensity) * ceiling_height_factor(ceiling_height_m)
}

/// Whether a ceiling fixture's panel is turned 90 degrees from its default.
///
/// The canonical rule shared by baked lighting and fixture geometry: rounding
/// the authored rotation to the nearest whole degree and testing it against
/// 180 keeps a `90` panel turned, a `180` panel back to default, and fractional
/// rotations identical in both places. The level editor mirrors this rule.
#[must_use]
pub fn fixture_is_turned(rotation_degrees: f32) -> bool {
    if !rotation_degrees.is_finite() {
        return false;
    }
    (rotation_degrees.round() as i64).rem_euclid(180) != 0
}

/// Half-extents of a fixture's luminous panel in world X/Z after rotation.
///
/// Mirrors the panel geometry emitted by `crate::render`: the default 1.2 x 0.6
/// panel runs along X, and a turned fixture swaps its axes.
#[must_use]
pub fn fixture_half_extents(rotation_degrees: f32) -> (f32, f32) {
    if fixture_is_turned(rotation_degrees) {
        (FIXTURE_HALF_DEPTH_M, FIXTURE_HALF_WIDTH_M)
    } else {
        (FIXTURE_HALF_WIDTH_M, FIXTURE_HALF_DEPTH_M)
    }
}

/// Smoothly saturating brightness component of a normalised light density.
///
/// `n / (1 + n)`: continuous, monotonic, zero at zero, asymptotically 1 as the
/// density grows, and numerically safe for every input (NaN maps to 0,
/// infinities map to 0 or 1).
#[must_use]
pub fn saturating_brightness(normalized_density: f32) -> f32 {
    if normalized_density.is_nan() {
        return 0.0;
    }
    if normalized_density.is_infinite() {
        return if normalized_density > 0.0 { 1.0 } else { 0.0 };
    }
    let n = normalized_density.max(0.0);
    n / (1.0 + n)
}

/// Smooth falloff curve shared by local pools and opening blends.
///
/// `(1 - t)^2 * (1 + 2t)` is `1 - smoothstep(t)`: it equals 1 at `t = 0`,
/// reaches 0 at `t = 1` and has a zero derivative at both ends, so lit regions
/// fade in and out without visible rings.
#[must_use]
pub fn smooth_falloff(t: f32) -> f32 {
    if t.is_nan() {
        return 0.0;
    }
    let t = t.clamp(0.0, 1.0);
    let u = 1.0 - t;
    u * u * 2.0f32.mul_add(t, 1.0)
}

/// Baseline brightness of a room from its floor area and effective fixture
/// power. The result is always inside `[MIN_AMBIENT, MAX_BRIGHTNESS]`.
#[must_use]
pub fn room_baseline(area_m2: f32, effective_power_sum: f32) -> f32 {
    let area = if area_m2.is_finite() {
        area_m2.max(MIN_ROOM_AREA_M2)
    } else {
        MIN_ROOM_AREA_M2
    };
    let power = if effective_power_sum.is_finite() {
        effective_power_sum.max(0.0)
    } else if effective_power_sum.is_infinite() && effective_power_sum > 0.0 {
        effective_power_sum
    } else {
        0.0
    };
    let density = power / area;
    let normalized = density * REFERENCE_LIGHT_AREA_M2;
    let component = saturating_brightness(normalized);
    (MAX_BRIGHTNESS - MIN_AMBIENT)
        .mul_add(component, MIN_AMBIENT)
        .clamp(MIN_AMBIENT, MAX_BRIGHTNESS)
}

/// Number of baked-lighting grid cells along one surface axis of `extent_m`.
///
/// Always at least 1 and never more than [`MAX_LIGHT_GRID_CELLS`], so floor and
/// ceiling geometry is bounded no matter how large a room is.
#[must_use]
pub fn light_grid_cells(extent_m: f32) -> u32 {
    if !extent_m.is_finite() || extent_m <= 0.0 {
        return 1;
    }
    ((extent_m / LIGHT_GRID_CELL_M).ceil() as u32).clamp(1, MAX_LIGHT_GRID_CELLS)
}

/// Number of segments one wall face is split into along its length, so baked
/// lighting can vary along long walls without unbounded geometry.
#[must_use]
pub fn wall_light_segments(length_m: f32) -> u32 {
    if !length_m.is_finite() || length_m <= 0.0 {
        return 1;
    }
    ((length_m / LIGHT_GRID_CELL_M).ceil() as u32).clamp(1, MAX_WALL_LIGHT_SEGMENTS)
}

// ---------------------------------------------------------------------------
// Baked level lighting
// ---------------------------------------------------------------------------

/// Baked illumination information for one room.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RoomLighting {
    /// Room footprint (normalised so `x0 <= x1`).
    pub x0: f32,
    pub x1: f32,
    pub z0: f32,
    pub z1: f32,
    /// Ceiling height in metres (always positive).
    pub height_m: f32,
    /// Floor area in square metres.
    pub area_m2: f32,
    /// Number of ceiling fixtures owned by this room.
    pub fixture_count: usize,
    /// Sum of `intensity x ceiling-height factor` over the owned fixtures.
    pub effective_power: f32,
    /// Baked baseline brightness, inside `[MIN_AMBIENT, MAX_BRIGHTNESS]`.
    pub baseline: f32,
}

/// One ceiling fixture resolved for baking.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BakedLight {
    pub x: f32,
    pub z: f32,
    /// World Y of the fixture panel (its room's ceiling minus [`FIXTURE_DROP_M`]).
    pub y: f32,
    /// Sanitised authored intensity.
    pub intensity: f32,
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
    pub min_baseline: f32,
    pub max_baseline: f32,
    pub average_baseline: f32,
}

/// A doorway/passage link between two rooms.
#[derive(Clone, Copy, Debug, PartialEq)]
struct OpeningBlend {
    x: f32,
    z: f32,
    /// Top edge of the opening in world Y.
    top_y: f32,
    /// Baseline of the room on the other side of the opening.
    neighbor_baseline: f32,
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
    /// Per room, indices of the fixtures whose pool can reach that room, in
    /// fixture order. A fixture outside the list is farther than
    /// [`LOCAL_LIGHT_RADIUS_M`] from every point in the room, so pruning is
    /// exact and the per-vertex sum is unchanged.
    room_lights: Vec<Vec<u32>>,
    /// Every fixture index, for samples outside all rooms.
    all_lights: Vec<u32>,
    /// Ceiling height used for fixtures that no room contains.
    default_ceiling_height_m: f32,
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
            rooms.push(RoomLighting {
                x0,
                x1,
                z0,
                z1,
                height_m,
                area_m2: width * depth,
                fixture_count: 0,
                effective_power: 0.0,
                baseline: MIN_AMBIENT,
            });
        }

        let default_ceiling_height_m = room_refs
            .first()
            .map(|room| room.height)
            .filter(|height| height.is_finite() && *height > 0.0)
            .unwrap_or(REFERENCE_CEILING_HEIGHT_M);

        // Resolve every fixture once: ownership, fixture plane, rotated panel
        // footprint and its contribution to its room's effective power.
        let mut lights: Vec<BakedLight> = Vec::with_capacity(level.ceiling_lights.len());
        for light in &level.ceiling_lights {
            if !light.x.is_finite() || !light.z.is_finite() {
                continue;
            }
            let room = Self::room_index_of(&rooms, light.x, light.z);
            let height_m = room.map_or_else(
                || ceiling_height_at(&room_refs, light.x, light.z),
                |index| rooms[index].height_m,
            );
            let height_m = if height_m.is_finite() && height_m > 0.0 {
                height_m
            } else {
                default_ceiling_height_m
            };
            let height_factor = ceiling_height_factor(height_m);
            let intensity = sanitize_intensity(light.intensity());
            // Rotation swaps the panel's long axis, exactly like the fixture
            // geometry emitted by `crate::render` (shared helper, so a
            // fractional rotation cannot drift between the two).
            let (half_w, half_d) = fixture_half_extents(light.rotation_degrees);
            if let Some(index) = room {
                rooms[index].fixture_count += 1;
                rooms[index].effective_power += effective_power(intensity, height_m);
            }
            lights.push(BakedLight {
                x: light.x,
                z: light.z,
                y: height_m - FIXTURE_DROP_M,
                intensity,
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
                if !opening.is_door() || !opening.reaches_floor() {
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
                // A wall raised off the floor (`wall.y`) is a header or lintel,
                // not a walk-through; its opening is above head height, so it
                // must not join the rooms for lighting either.
                if wall.y + opening.sill.max(0.0) > 1e-3 {
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
                let top_y = opening.top(wall.y);
                blends[room_a].push(OpeningBlend {
                    x: center_x,
                    z: center_z,
                    top_y,
                    neighbor_baseline: rooms[room_b].baseline,
                });
                blends[room_b].push(OpeningBlend {
                    x: center_x,
                    z: center_z,
                    top_y,
                    neighbor_baseline: rooms[room_a].baseline,
                });
            }
        }

        Self {
            rooms,
            lights,
            blends,
            room_lights,
            all_lights,
            default_ceiling_height_m,
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

    /// World Y of the fixture panel for a ceiling light placed at `(x, z)`.
    ///
    /// Fixtures hang just below their room's ceiling, so the same panel sits at
    /// 2.59 m in a 2.6 m corridor and at 2.99 m in a 3 m room.
    #[must_use]
    pub fn fixture_y(&self, x: f32, z: f32) -> f32 {
        self.room_index_at(x, z)
            .map_or(self.default_ceiling_height_m - FIXTURE_DROP_M, |index| {
                self.rooms[index].height_m - FIXTURE_DROP_M
            })
    }

    /// Baked brightness at a world position, resolving the room by containment.
    ///
    /// Used for props and for geometry that does not know its room. Points
    /// outside every room still receive the minimum ambient and any local
    /// fixture pools they are inside.
    #[must_use]
    pub fn sample(&self, x: f32, y: f32, z: f32) -> f32 {
        self.room_index_at(x, z).map_or_else(
            || {
                (MIN_AMBIENT + self.local_light(&self.all_lights, x, y, z))
                    .clamp(MIN_AMBIENT, MAX_BRIGHTNESS)
            },
            |index| self.sample_in_room(index, x, y, z),
        )
    }

    /// Baked brightness for a point already known to belong to `room`.
    ///
    /// Floors, ceilings and wall faces use this so a vertex sitting exactly on a
    /// room boundary is lit by the surface's own room, not by whichever room the
    /// containment rule happens to prefer.
    #[must_use]
    pub fn sample_in_room(&self, room: usize, x: f32, y: f32, z: f32) -> f32 {
        let Some(info) = self.rooms.get(room) else {
            return self.sample(x, y, z);
        };
        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return MIN_AMBIENT;
        }

        let candidates = &self.room_lights[room];
        let mut value = info.baseline + self.local_light(candidates, x, y, z);

        // Bounded doorway blending: mix a fraction of the neighbouring room's
        // baseline that fades to nothing over `OPENING_BLEND_RADIUS_M` and above
        // the opening's top edge.
        for blend in &self.blends[room] {
            let dx = x - blend.x;
            let dz = z - blend.z;
            let distance = dx.hypot(dz);
            if !distance.is_finite() || distance >= OPENING_BLEND_RADIUS_M {
                continue;
            }
            let mut influence =
                OPENING_BLEND_STRENGTH * smooth_falloff(distance / OPENING_BLEND_RADIUS_M);
            if y > blend.top_y {
                influence *= smooth_falloff((y - blend.top_y) / OPENING_VERTICAL_FADE_M);
            }
            value = (blend.neighbor_baseline - info.baseline).mul_add(influence, value);
        }

        if value.is_finite() {
            value.clamp(MIN_AMBIENT, MAX_BRIGHTNESS)
        } else {
            MIN_AMBIENT
        }
    }

    /// Local fixture pools at a world position: broad, smooth and bounded.
    ///
    /// Each fixture's contribution falls from [`LOCAL_LIGHT_STRENGTH`] at its
    /// panel to zero at [`LOCAL_LIGHT_RADIUS_M`], scaled by the fixture's
    /// intensity and by its room's ceiling-height factor. The sum is capped at
    /// [`LOCAL_LIGHT_MAX`] so clusters stay in range.
    ///
    /// `candidates` are indices into [`Self::lights`]; squared distances are
    /// compared against the radius before the square root, so fixtures that
    /// cannot reach the sample are rejected with a couple of multiplies.
    fn local_light(&self, candidates: &[u32], x: f32, y: f32, z: f32) -> f32 {
        if !x.is_finite() || !y.is_finite() || !z.is_finite() {
            return 0.0;
        }
        let radius_squared = LOCAL_LIGHT_RADIUS_M * LOCAL_LIGHT_RADIUS_M;
        let inv_radius = 1.0 / LOCAL_LIGHT_RADIUS_M;
        let mut sum = 0.0;
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
            let falloff = smooth_falloff(distance_squared.sqrt() * inv_radius);
            sum = (LOCAL_LIGHT_STRENGTH * light.intensity * light.height_factor)
                .mul_add(falloff, sum);
            if sum >= LOCAL_LIGHT_MAX {
                return LOCAL_LIGHT_MAX;
            }
        }
        sum.clamp(0.0, LOCAL_LIGHT_MAX)
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

    /// Aggregate statistics for developer logging.
    #[must_use]
    pub fn summary(&self) -> LightingSummary {
        if self.rooms.is_empty() {
            return LightingSummary {
                rooms: 0,
                lights: self.lights.len(),
                min_baseline: 0.0,
                max_baseline: 0.0,
                average_baseline: 0.0,
            };
        }
        let mut min = f32::MAX;
        let mut max = f32::MIN;
        let mut total = 0.0;
        for room in &self.rooms {
            min = min.min(room.baseline);
            max = max.max(room.baseline);
            total += room.baseline;
        }
        LightingSummary {
            rooms: self.rooms.len(),
            lights: self.lights.len(),
            min_baseline: min,
            max_baseline: max,
            average_baseline: total / self.rooms.len() as f32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::level::LevelDef;
    use crate::test_support::{assert_exact, scan};

    /// One rectangular room with `lights` fixtures evenly spread across it.
    fn level_with_room(width: f32, depth: f32, height: f32, intensities: &[f32]) -> LevelDef {
        let lights: Vec<String> = intensities
            .iter()
            .enumerate()
            .map(|(index, intensity)| {
                let x = width * (index as f32 + 1.0) / (intensities.len() as f32 + 1.0);
                let z = depth * (index as f32 + 1.0) / (intensities.len() as f32 + 1.0);
                format!(
                    r#"{{ "fixture": "core:fluorescent_panel_01", "x": {x}, "z": {z}, "intensity": {intensity} }}"#
                )
            })
            .collect();
        let json = format!(
            r#"{{
                "format_version": 1,
                "id": "lighting_test",
                "name": "Lighting Test",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [{{ "x": 0.0, "z": 0.0, "width": {width}, "depth": {depth}, "height": {height} }}],
                "ceiling_lights": [{}]
            }}"#,
            lights.join(",")
        );
        LevelDef::from_json(&json).expect("test level parses")
    }

    #[test]
    fn more_lights_raise_the_room_baseline() {
        let dim = LevelLighting::bake(&level_with_room(20.0, 20.0, 3.5, &[1.0]));
        let brighter =
            LevelLighting::bake(&level_with_room(20.0, 20.0, 3.5, &[1.0, 1.0, 1.0, 1.0]));
        assert!(
            brighter.rooms()[0].baseline > dim.rooms()[0].baseline,
            "4 lights ({}) must beat 1 light ({})",
            brighter.rooms()[0].baseline,
            dim.rooms()[0].baseline
        );
        assert!(brighter.rooms()[0].baseline <= MAX_BRIGHTNESS);
    }

    #[test]
    fn larger_rooms_are_dimmer_for_the_same_lights() {
        let small = LevelLighting::bake(&level_with_room(10.0, 10.0, 3.5, &[1.0, 1.0]));
        let large = LevelLighting::bake(&level_with_room(30.0, 30.0, 3.5, &[1.0, 1.0]));
        assert!(
            small.rooms()[0].baseline > large.rooms()[0].baseline,
            "12 m2-class room ({}) must beat 900 m2 one ({})",
            small.rooms()[0].baseline,
            large.rooms()[0].baseline
        );
        // Both rooms' fixtures are owned; area is the only difference.
        assert_eq!(small.rooms()[0].fixture_count, 2);
        assert_eq!(large.rooms()[0].fixture_count, 2);
    }

    #[test]
    fn brighter_fixtures_raise_the_room_baseline() {
        let weak = LevelLighting::bake(&level_with_room(16.0, 16.0, 3.5, &[0.5]));
        let standard = LevelLighting::bake(&level_with_room(16.0, 16.0, 3.5, &[1.0]));
        let strong = LevelLighting::bake(&level_with_room(16.0, 16.0, 3.5, &[2.0]));
        assert!(weak.rooms()[0].baseline < standard.rooms()[0].baseline);
        assert!(standard.rooms()[0].baseline < strong.rooms()[0].baseline);
    }

    #[test]
    fn a_missing_intensity_behaves_as_a_standard_fixture() {
        let json = r#"{
            "format_version": 1,
            "id": "defaults",
            "name": "Defaults",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 16.0, "depth": 16.0, "height": 3.5 }],
            "ceiling_lights": [{ "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 8.0 }]
        }"#;
        let level = LevelDef::from_json(json).expect("valid json");
        assert_exact(level.ceiling_lights[0].intensity(), 1.0);
        let omitted = LevelLighting::bake(&level);
        let explicit = LevelLighting::bake(&level_with_room(16.0, 16.0, 3.5, &[1.0]));
        assert_exact(omitted.rooms()[0].baseline, explicit.rooms()[0].baseline);
        assert_exact(omitted.lights()[0].intensity, 1.0);
    }

    #[test]
    fn the_intensity_alias_is_accepted_and_negative_values_are_sanitized() {
        let json = r#"{
            "format_version": 1,
            "id": "alias",
            "name": "Alias",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 }],
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 3.0, "z": 3.0, "intensity": 1.4 },
                { "fixture": "core:fluorescent_panel_01", "x": 7.0, "z": 7.0, "brightness": -4.0 }
            ]
        }"#;
        let level = LevelDef::from_json(json).expect("valid json");
        assert_exact(level.ceiling_lights[0].intensity(), 1.4);
        assert_exact(level.ceiling_lights[1].intensity(), 0.0);

        let lighting = LevelLighting::bake(&level);
        assert_eq!(lighting.lights().len(), 2);
        // The negative fixture adds no baseline power and no local light.
        let point = [7.0, 0.0, 7.0];
        let under_negative = lighting.sample(point[0], point[1], point[2]);
        assert!(under_negative.is_finite());
        assert!((MIN_AMBIENT..=MAX_BRIGHTNESS).contains(&under_negative));
    }

    #[test]
    fn higher_ceilings_lower_the_effective_illumination() {
        let low = LevelLighting::bake(&level_with_room(16.0, 16.0, 2.6, &[1.0, 1.0]));
        let normal = LevelLighting::bake(&level_with_room(16.0, 16.0, 3.5, &[1.0, 1.0]));
        let tall = LevelLighting::bake(&level_with_room(16.0, 16.0, 5.0, &[1.0, 1.0]));
        assert!(
            low.rooms()[0].baseline > normal.rooms()[0].baseline,
            "2.6 m ({}) should beat 3.5 m ({})",
            low.rooms()[0].baseline,
            normal.rooms()[0].baseline
        );
        assert!(
            normal.rooms()[0].baseline > tall.rooms()[0].baseline,
            "3.5 m ({}) should beat 5 m ({})",
            normal.rooms()[0].baseline,
            tall.rooms()[0].baseline
        );
        // The correction is gentle, not an inverse square: a 5 m room keeps most
        // of the reference output.
        assert!(tall.lights()[0].height_factor > 0.7);
        assert!(low.lights()[0].height_factor < 1.3);
    }

    #[test]
    fn brightness_saturates_instead_of_growing_without_bound() {
        // 200 high-output fixtures in a small room: extreme but finite input.
        let intensities = vec![2.0_f32; 200];
        let lighting = LevelLighting::bake(&level_with_room(4.0, 4.0, 3.5, &intensities));
        let baseline = lighting.rooms()[0].baseline;
        assert!(
            baseline <= MAX_BRIGHTNESS && baseline > 0.99,
            "an absurd fixture count must saturate just below the maximum, got {baseline}"
        );
        assert!(baseline.is_finite());
        for sample in [
            lighting.sample(0.1, 0.0, 0.1),
            lighting.sample(2.0, 1.6, 2.0),
            lighting.sample(3.9, 2.9, 3.9),
        ] {
            assert!(sample.is_finite());
            assert!((MIN_AMBIENT..=MAX_BRIGHTNESS).contains(&sample), "{sample}");
        }

        // Even overflow-sized inputs stay inside the allowed range.
        let extreme = vec![f32::MAX; 4];
        let lighting = LevelLighting::bake(&level_with_room(2.0, 2.0, 3.5, &extreme));
        assert!(lighting.rooms()[0].baseline <= MAX_BRIGHTNESS);
        assert!(lighting.rooms()[0].baseline.is_finite());
    }

    #[test]
    fn a_room_without_fixtures_is_dim_but_never_black() {
        let lighting = LevelLighting::bake(&level_with_room(20.0, 20.0, 3.5, &[]));
        assert_exact(lighting.rooms()[0].baseline, MIN_AMBIENT);
        const { assert!(MIN_AMBIENT > 0.0) };
        let sample = lighting.sample(10.0, 0.0, 10.0);
        assert_exact(sample, MIN_AMBIENT);
    }

    #[test]
    fn samples_under_a_fixture_are_brighter_than_distant_samples() {
        let json = r#"{
            "format_version": 1,
            "id": "pool",
            "name": "Pool",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 24.0, "depth": 8.0, "height": 3.0 }],
            "ceiling_lights": [{ "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }]
        }"#;
        let level = LevelDef::from_json(json).expect("valid json");
        let lighting = LevelLighting::bake(&level);

        let beneath = lighting.sample(4.0, 0.0, 4.0);
        let near = lighting.sample(6.0, 0.0, 4.0);
        let far = lighting.sample(20.0, 0.0, 4.0);
        assert!(
            beneath > near,
            "directly beneath ({beneath}) must beat near ({near})"
        );
        assert!(near > far, "near ({near}) must beat far ({far})");
        assert!(
            (far - lighting.rooms()[0].baseline).abs() < 1e-4,
            "far from every fixture must sit at the room baseline"
        );
        // Pools are broad, not spotlights: 2 m away still benefits.
        assert!(
            near - far > 0.03,
            "expected a broad pool, got {}",
            near - far
        );
    }

    #[test]
    fn local_pools_scale_with_fixture_intensity() {
        // One fixture at a known position in a large room, sampled directly
        // beneath it: a 2.0 fixture must out-light a 0.5 fixture clearly.
        let level = |intensity: f32| {
            LevelDef::from_json(&format!(
                r#"{{
                    "format_version": 1,
                    "id": "pool_intensity",
                    "name": "Pool Intensity",
                    "spawn": {{ "x": 0.0, "z": 0.0 }},
                    "rooms": [{{ "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 3.0 }}],
                    "ceiling_lights": [{{
                        "fixture": "core:fluorescent_panel_01",
                        "x": 10.0, "z": 10.0, "intensity": {intensity}
                    }}]
                }}"#
            ))
            .expect("test level parses")
        };
        let weak = LevelLighting::bake(&level(0.5));
        let strong = LevelLighting::bake(&level(2.0));
        let weak_under = weak.sample(10.0, 0.0, 10.0);
        let strong_under = strong.sample(10.0, 0.0, 10.0);
        assert!(
            strong_under > weak_under + 0.1,
            "2.0 fixture ({strong_under}) must clearly beat 0.5 ({weak_under})"
        );
        // Both stay inside the legal range.
        for value in [weak_under, strong_under] {
            assert!((MIN_AMBIENT..=MAX_BRIGHTNESS).contains(&value));
        }
    }

    /// Two differently lit rooms sharing a wall. `opening` adds a walk-through
    /// doorway; without it the wall is solid.
    fn two_room_level(opening: bool) -> LevelDef {
        let openings = if opening {
            r#"[{ "kind": "door", "offset": 4.5, "width": 1.0, "height": 2.1, "sill": 0.0 }]"#
        } else {
            "[]"
        };
        let json = format!(
            r#"{{
                "format_version": 1,
                "id": "blend",
                "name": "Blend",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [
                    {{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }},
                    {{ "x": 10.4, "z": 0.0, "width": 40.0, "depth": 20.0, "height": 3.0 }}
                ],
                "walls": [{{
                    "x": 10.0, "z": 0.0, "width": 0.4, "depth": 10.0, "height": 3.0,
                    "openings": {openings}
                }}],
                "ceiling_lights": [
                    {{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 2.0 }},
                    {{ "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 2.0 }},
                    {{ "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 2.0 }},
                    {{ "fixture": "core:fluorescent_panel_01", "x": 2.0, "z": 8.0 }},
                    {{ "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 8.0 }},
                    {{ "fixture": "core:fluorescent_panel_01", "x": 8.0, "z": 8.0 }},
                    {{ "fixture": "core:fluorescent_panel_01", "x": 30.0, "z": 2.0 }}
                ]
            }}"#
        );
        LevelDef::from_json(&json).expect("test level parses")
    }

    #[test]
    fn openings_blend_between_differently_lit_rooms() {
        let level = two_room_level(true);
        let solid = LevelLighting::bake(&two_room_level(false));
        let lighting = LevelLighting::bake(&level);
        assert_eq!(lighting.rooms().len(), 2);

        let baseline_bright = lighting.rooms()[0].baseline;
        let baseline_dim = lighting.rooms()[1].baseline;
        assert!(
            baseline_bright > baseline_dim + 0.1,
            "test setup needs contrasting rooms: {baseline_bright} vs {baseline_dim}"
        );

        // Sampling below y = 3 m near the doorway, on both sides of the wall.
        let bright_near_door = lighting.sample_in_room(0, 9.9, 0.0, 5.0);
        let dim_near_door = lighting.sample_in_room(1, 10.5, 0.0, 5.0);
        let bright_without_opening = solid.sample_in_room(0, 9.9, 0.0, 5.0);
        let dim_without_opening = solid.sample_in_room(1, 10.5, 0.0, 5.0);

        // The doorway pulls each side towards the other room...
        assert!(
            bright_near_door < bright_without_opening - 1e-3,
            "the bright side must lose light to the dim room: {bright_near_door} vs {bright_without_opening}"
        );
        assert!(
            dim_near_door > dim_without_opening + 1e-3,
            "the dim side must gain light from the bright room: {dim_near_door} vs {dim_without_opening}"
        );
        // ...by a bounded, gradual amount, so the two sides meet at a threshold
        // instead of stepping.
        let step = (bright_near_door - dim_near_door).abs();
        assert!(step < 0.05, "doorway step too large: {step}");
        for value in [bright_near_door, dim_near_door] {
            assert!((MIN_AMBIENT..=MAX_BRIGHTNESS).contains(&value), "{value}");
        }

        // The influence is bounded: far from the opening the rooms keep their
        // own baselines (local pools are identical in both bakes).
        for (room, x, z) in [(0usize, 1.0f32, 5.0f32), (1, 30.0, 15.0)] {
            let open = lighting.sample_in_room(room, x, 0.0, z);
            let closed = solid.sample_in_room(room, x, 0.0, z);
            assert!(
                (open - closed).abs() < 1e-4,
                "room {room} at ({x}, {z}) must not be blended from {OPENING_BLEND_RADIUS_M} m away: {open} vs {closed}"
            );
        }
    }

    #[test]
    fn openings_do_not_blend_through_solid_walls() {
        let json = r#"{
            "format_version": 1,
            "id": "solid",
            "name": "Solid",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 },
                { "x": 10.4, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }
            ],
            "walls": [{ "x": 10.0, "z": 0.0, "width": 0.4, "depth": 10.0, "height": 3.0 }],
            "ceiling_lights": [{ "fixture": "core:fluorescent_panel_01", "x": 1.0, "z": 1.0 }]
        }"#;
        let level = LevelDef::from_json(json).expect("valid json");
        let lighting = LevelLighting::bake(&level);
        let a = lighting.rooms()[0].baseline;
        let b = lighting.rooms()[1].baseline;
        assert!(a > b, "only room A has fixtures");
        // Deep inside room B, including just past the solid wall, nothing leaks.
        for x in [12.0, 18.0] {
            assert!(
                (lighting.sample_in_room(1, x, 0.0, 5.0) - b).abs() < 1e-4,
                "solid wall leaked light at x = {x}"
            );
        }
    }

    #[test]
    fn overlapping_rooms_own_lights_deterministically_and_only_once() {
        // A big room overlapped by a small one. The fixture inside both belongs
        // to the smaller room only, so the small room is bright and the big one
        // receives nothing.
        let json = r#"{
            "format_version": 1,
            "id": "overlap",
            "name": "Overlap",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 40.0, "depth": 40.0, "height": 3.0 },
                { "x": 4.0, "z": 4.0, "width": 6.0, "depth": 6.0, "height": 3.0 }
            ],
            "ceiling_lights": [{ "fixture": "core:fluorescent_panel_01", "x": 7.0, "z": 7.0 }]
        }"#;
        let level = LevelDef::from_json(json).expect("valid json");
        let lighting = LevelLighting::bake(&level);
        assert_eq!(lighting.rooms()[0].fixture_count, 0);
        assert_eq!(lighting.rooms()[1].fixture_count, 1);
        assert_eq!(lighting.room_index_at(7.0, 7.0), Some(1));
        assert_eq!(lighting.room_index_at(30.0, 30.0), Some(0));
        assert_eq!(lighting.room_index_at(-1.0, -1.0), None);
        // The light is counted once, in the small room.
        assert_exact(
            lighting.rooms()[0].effective_power + lighting.rooms()[1].effective_power,
            lighting.lights()[0].intensity * lighting.lights()[0].height_factor,
        );
    }

    #[test]
    fn malformed_inputs_stay_finite_and_never_panic() {
        let mut level = level_with_room(10.0, 10.0, 3.0, &[1.0]);
        level.ceiling_lights[0].x = f32::NAN;
        level.ceiling_lights[0].brightness = Some(f32::NAN);
        level.rooms[0].height = -2.0;
        let lighting = LevelLighting::bake(&level);
        assert!(
            lighting.lights().is_empty(),
            "non-finite fixtures are skipped"
        );
        assert!(lighting.sample(0.0, 0.0, 0.0).is_finite());
        assert!(lighting.sample(f32::NAN, 0.0, 3.0).is_finite());

        // Extreme-but-finite levels must not overflow or produce NaN.
        let mut extreme = level_with_room(1.0e30, 1.0e30, 3.5, &[1.0, 1.0]);
        extreme.ceiling_lights[0].brightness = Some(f32::MAX);
        extreme.ceiling_lights[1].brightness = Some(f32::INFINITY);
        extreme.ceiling_lights[1].x = f32::NAN; // dropped entirely
        let lighting = LevelLighting::bake(&extreme);
        let baseline = lighting.rooms()[0].baseline;
        assert!(baseline.is_finite() && baseline <= MAX_BRIGHTNESS);
        assert!(lighting.sample(1.0, 0.0, 1.0).is_finite());
    }

    #[test]
    fn helper_curves_are_monotonic_and_numerically_safe() {
        assert_exact(saturating_brightness(0.0), 0.0);
        assert_exact(saturating_brightness(-1.0), 0.0);
        assert_exact(saturating_brightness(f32::NAN), 0.0);
        assert_exact(saturating_brightness(f32::INFINITY), 1.0);
        assert_exact(saturating_brightness(f32::NEG_INFINITY), 0.0);
        let mut previous = 0.0;
        for step in 0..40 {
            let value = saturating_brightness(step as f32 * 0.25);
            assert!(value > previous || step == 0 || previous > 0.99);
            assert!((0.0..=1.0).contains(&value));
            previous = value;
        }
        // Adding lights keeps increasing brightness, but ever more slowly.
        let first = saturating_brightness(0.5) - saturating_brightness(0.0);
        let later = saturating_brightness(4.5) - saturating_brightness(4.0);
        assert!(first > later && later > 0.0);

        assert_exact(smooth_falloff(0.0), 1.0);
        assert_exact(smooth_falloff(1.0), 0.0);
        assert_exact(smooth_falloff(f32::NAN), 0.0);
        assert!(smooth_falloff(0.5) > 0.0 && smooth_falloff(0.5) < 1.0);

        assert_exact(sanitize_intensity(f32::NAN), 1.0);
        assert_exact(sanitize_intensity(-3.0), 0.0);
        assert_exact(sanitize_intensity(1.0e30), MAX_LIGHT_INTENSITY);
        assert_exact(sanitize_intensity(1.4), 1.4);

        assert_exact(ceiling_height_factor(f32::NAN), 1.0);
        assert_exact(ceiling_height_factor(0.0), 1.0);
        assert_exact(ceiling_height_factor(REFERENCE_CEILING_HEIGHT_M), 1.0);
        assert!(ceiling_height_factor(2.6) > 1.0 && ceiling_height_factor(2.6) < 1.3);
        assert!(ceiling_height_factor(6.0) > 0.6 && ceiling_height_factor(6.0) < 1.0);

        assert_eq!(light_grid_cells(0.0), 1);
        assert_eq!(light_grid_cells(2.0), 1);
        assert_eq!(light_grid_cells(6.0), 3);
        assert_eq!(light_grid_cells(1.0e30), MAX_LIGHT_GRID_CELLS);
        assert_eq!(wall_light_segments(1.0), 1);
        assert_eq!(wall_light_segments(1000.0), MAX_WALL_LIGHT_SEGMENTS);
    }

    #[test]
    fn brightness_changes_smoothly_instead_of_in_bands() {
        // One fixture in a 20x20 room: scanning the floor in 5 cm steps must
        // never produce a visible jump, so rooms have gradients rather than
        // discrete brightness tiers.
        let level = level_with_room(20.0, 20.0, 3.0, &[1.0]);
        let lighting = LevelLighting::bake(&level);
        let mut previous = lighting.sample(0.0, 0.0, 10.0);
        for x in scan(0.0, 0.05, 20.0) {
            let current = lighting.sample(x, 0.0, 10.0);
            assert!(
                (current - previous).abs() < 0.02,
                "brightness jumped at x = {x}: {previous} -> {current}"
            );
            previous = current;
        }
        // And it genuinely varies across the room.
        assert!(lighting.sample(10.0, 0.0, 10.0) - lighting.sample(0.0, 0.0, 10.0) > 0.05);
    }

    #[test]
    fn summaries_describe_the_bake() {
        let lighting =
            LevelLighting::bake(&level_with_room(20.0, 20.0, 3.5, &[1.0, 1.0, 1.0, 1.0]));
        let summary = lighting.summary();
        assert_eq!(summary.rooms, 1);
        assert_eq!(summary.lights, 4);
        assert_exact(summary.min_baseline, summary.max_baseline);
        assert!(summary.average_baseline >= MIN_AMBIENT);
        assert!(summary.average_baseline <= MAX_BRIGHTNESS);
    }

    #[test]
    fn fixture_plane_follows_the_room_ceiling() {
        let json = r#"{
            "format_version": 1,
            "id": "heights",
            "name": "Heights",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 },
                { "x": 12.0, "z": 0.0, "width": 10.0, "depth": 4.0, "height": 2.6 }
            ],
            "ceiling_lights": [
                { "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 5.0 },
                { "fixture": "core:fluorescent_panel_01", "x": 17.0, "z": 2.0 }
            ]
        }"#;
        let level = LevelDef::from_json(json).expect("valid json");
        let lighting = LevelLighting::bake(&level);
        assert!((lighting.fixture_y(5.0, 5.0) - 2.99).abs() < 1e-4);
        assert!((lighting.fixture_y(17.0, 2.0) - 2.59).abs() < 1e-4);
        assert_eq!(lighting.lights()[0].room, Some(0));
        assert_eq!(lighting.lights()[1].room, Some(1));
    }

    #[test]
    fn vertically_offset_samples_follow_their_true_position() {
        // Same (x, z), different heights: below the fixture plane the pool is
        // weaker than right next to the panel.
        let json = r#"{
            "format_version": 1,
            "id": "height_sample",
            "name": "Height Sample",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.0 }],
            "ceiling_lights": [{ "fixture": "core:fluorescent_panel_01", "x": 5.0, "z": 5.0 }]
        }"#;
        let level = LevelDef::from_json(json).expect("valid json");
        let lighting = LevelLighting::bake(&level);
        let floor = lighting.sample(5.0, 0.0, 5.0);
        let beside_panel = lighting.sample(5.0, 2.8, 5.0);
        assert!(beside_panel > floor);
    }

    #[test]
    fn sample_points_use_the_smaller_overlapping_room() {
        // The documented ownership rule is shared by light ownership and point
        // sampling, so a sample inside the overlap resolves to the small room.
        let json = r#"{
            "format_version": 1,
            "id": "sample_overlap",
            "name": "Sample Overlap",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 40.0, "depth": 40.0, "height": 3.0 },
                { "x": 4.0, "z": 4.0, "width": 6.0, "depth": 6.0, "height": 3.0 }
            ],
            "ceiling_lights": [{ "fixture": "core:fluorescent_panel_01", "x": 7.0, "z": 7.0 }]
        }"#;
        let level = LevelDef::from_json(json).expect("valid json");
        let lighting = LevelLighting::bake(&level);
        assert_eq!(lighting.room_index_at(7.0, 7.0), Some(1));
        let inside = lighting.sample(7.0, 0.0, 7.0);
        let outside = lighting.sample(30.0, 0.0, 30.0);
        assert!(inside > outside, "the small room owns the light");
        assert!(outside >= MIN_AMBIENT);
    }
}
