use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::collision::{PLAYER_STEP_HEIGHT, WallAabb};
use crate::lighting::{DEFAULT_LIGHT_COLOR, LightColor};

/// Clear ceiling height of a room whose level JSON omits `height`, in metres.
///
/// Levels that author `"height": 3.5` keep it verbatim; only rooms that leave
/// the key out (or that are created without one) receive this default.
pub const DEFAULT_CEILING_HEIGHT_M: f32 = 4.0;

const fn default_ceiling_height() -> f32 {
    DEFAULT_CEILING_HEIGHT_M
}

/// Tolerance applied when testing whether a point lies inside a room footprint,
/// in metres.
///
/// Shared by every room ownership lookup so walls, floors, ceilings, fixtures
/// and collision agree on where a room ends.
pub const ROOM_EDGE_EPS_M: f32 = 0.01;

/// The profile of a room's ceiling.
///
/// `Flat` is the historical single horizontal plane. `Gable` is a symmetrical
/// pitched ceiling with one horizontal ridge; the representation is a tagged
/// enum so later profiles (shed, vaulted, stepped, custom) can be added without
/// changing the room model or the serialized shape of existing entries.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CeilingProfileDef {
    /// One horizontal ceiling plane at the room's eave height.
    #[default]
    Flat,
    /// A symmetrical pitched ceiling: eave height at two opposite walls,
    /// rising linearly to a single horizontal ridge between them.
    Gable {
        /// Horizontal axis the ridge runs along: `x` leaves the ridge constant
        /// in X and sloping along Z, `z` is the mirror case.
        ridge: WallAxis,
        /// Ridge height above the eave, in metres. Must be finite and positive.
        ridge_rise: f32,
    },
}

impl CeilingProfileDef {
    /// True for the historical flat ceiling.
    #[must_use]
    pub const fn is_flat(self) -> bool {
        matches!(self, Self::Flat)
    }

    /// Ridge axis of a gable ceiling, `None` for a flat one.
    #[must_use]
    pub const fn ridge_axis(self) -> Option<WallAxis> {
        match self {
            Self::Flat => None,
            Self::Gable { ridge, .. } => Some(ridge),
        }
    }

    /// Ridge rise in metres, sanitised to zero for anything malformed.
    #[must_use]
    pub fn ridge_rise_m(self) -> f32 {
        match self {
            Self::Flat => 0.0,
            Self::Gable { ridge_rise, .. } => {
                if ridge_rise.is_finite() && ridge_rise > 0.0 {
                    ridge_rise
                } else {
                    0.0
                }
            }
        }
    }
}

/// World Y of a room volume's ceiling surface at `(x, z)`.
///
/// This is the single implementation of ceiling-profile maths: floors, walls,
/// fixtures and decals all resolve their ceiling through it, so a profile can
/// never drift between the mesh, the bake and collision. Malformed input
/// degrades to the eave plane instead of producing NaN.
#[must_use]
pub fn ceiling_y_for_volume(
    bounds: (f32, f32, f32, f32),
    floor_y: f32,
    height: f32,
    profile: CeilingProfileDef,
    x: f32,
    z: f32,
) -> f32 {
    let floor_y = if floor_y.is_finite() { floor_y } else { 0.0 };
    let height = if height.is_finite() && height > 0.0 {
        height
    } else {
        DEFAULT_CEILING_HEIGHT_M
    };
    let eave = floor_y + height;
    let CeilingProfileDef::Gable { ridge, ridge_rise } = profile else {
        return eave;
    };
    let rise = if ridge_rise.is_finite() && ridge_rise > 0.0 {
        ridge_rise
    } else {
        return eave;
    };
    let (x0, x1, z0, z1) = bounds;
    if !x0.is_finite() || !x1.is_finite() || !z0.is_finite() || !z1.is_finite() {
        return eave;
    }
    if !x.is_finite() || !z.is_finite() {
        return eave;
    }
    // The ridge runs along `ridge`; the ceiling slopes across the other axis,
    // from the eave at both edges up to `rise` above the eave at the centre.
    let (centre, half_extent, across) = match ridge {
        WallAxis::X => (f32::midpoint(z0, z1), (z1 - z0).abs() * 0.5, z),
        WallAxis::Z => (f32::midpoint(x0, x1), (x1 - x0).abs() * 0.5, x),
    };
    if !centre.is_finite() || half_extent <= 1e-4 {
        return eave;
    }
    let tent = (1.0 - (across - centre).abs() / half_extent).clamp(0.0, 1.0);
    eave + rise * tent
}

/// Rectangular room section defining floor and ceiling boundaries.
///
/// `floor_y` is the world Y of the room's normal floor plane: the room's floor,
/// walls and ceiling are all generated relative to it, so a room can sit at
/// `0.0`, `2.0` or `-1.0` without its geometry being pulled back to world zero.
/// `height` stays the room-local clear height from that floor to the eave; the
/// ceiling profile only ever adds height above the eave.
///
/// `material` and `ceiling_material` are the object-level material overrides
/// (individual surface override -> object-level material -> level default
/// material). Both are optional: an omitted value keeps the
/// level's `defaults.floor` / `defaults.ceiling`. The level editor authors these
/// exact keys, so a single room's floor or ceiling can be damp or stained
/// without changing the whole level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomDef {
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub z: f32,
    pub width: f32,
    pub depth: f32,
    #[serde(default = "default_ceiling_height")]
    pub height: f32,
    /// World Y of this room's floor plane. Omitted means `0.0`, the historical
    /// global floor, so legacy levels load unchanged.
    #[serde(default)]
    pub floor_y: f32,
    /// Ceiling profile. Omitted means `Flat` at `floor_y + height`.
    #[serde(default)]
    pub ceiling: CeilingProfileDef,
    /// Floor material id for this room. Falls back to `defaults.floor`.
    #[serde(default)]
    pub material: Option<String>,
    /// Ceiling material id for this room. Falls back to `defaults.ceiling`.
    #[serde(default)]
    pub ceiling_material: Option<String>,
}

impl RoomDef {
    /// Room footprint as `(x0, x1, z0, z1)`, normalised and finite-safe.
    #[must_use]
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        (
            self.x.min(self.x + self.width),
            self.x.max(self.x + self.width),
            self.z.min(self.z + self.depth),
            self.z.max(self.z + self.depth),
        )
    }

    /// World Y of the room's eave: the `height` plane, before any gable rise.
    #[must_use]
    pub fn eave_y(&self) -> f32 {
        let floor = if self.floor_y.is_finite() {
            self.floor_y
        } else {
            0.0
        };
        let height = if self.height.is_finite() && self.height > 0.0 {
            self.height
        } else {
            DEFAULT_CEILING_HEIGHT_M
        };
        floor + height
    }

    /// World Y of this room's ceiling surface at `(x, z)`.
    #[must_use]
    pub fn ceiling_y_at(&self, x: f32, z: f32) -> f32 {
        ceiling_y_for_volume(self.bounds(), self.floor_y, self.height, self.ceiling, x, z)
    }

    /// World Y of the ridge of a gable ceiling, `None` for a flat one.
    #[must_use]
    pub fn ridge_y(&self) -> Option<f32> {
        (self.ceiling.ridge_axis().is_some()).then(|| self.eave_y() + self.ceiling.ridge_rise_m())
    }

    /// Coordinate of the ridge along the axis the ceiling slopes over.
    #[must_use]
    pub fn ridge_across(&self) -> Option<f32> {
        let (x0, x1, z0, z1) = self.bounds();
        match self.ceiling.ridge_axis()? {
            WallAxis::X => Some(f32::midpoint(z0, z1)),
            WallAxis::Z => Some(f32::midpoint(x0, x1)),
        }
    }

    /// True when `(x, z)` lies inside the room footprint.
    #[must_use]
    pub fn contains(&self, x: f32, z: f32) -> bool {
        if !x.is_finite() || !z.is_finite() {
            return false;
        }
        let (x0, x1, z0, z1) = self.bounds();
        x >= x0 - ROOM_EDGE_EPS_M
            && x <= x1 + ROOM_EDGE_EPS_M
            && z >= z0 - ROOM_EDGE_EPS_M
            && z <= z1 + ROOM_EDGE_EPS_M
    }
}

/// A rectangular local floor area with its own vertical offset.
///
/// The offset is relative to the containing room's `floor_y`: negative values
/// recess the floor (an empty pool basin, a service trench, a sunken seating
/// area), positive values raise a platform. Regions cut the room's floor grid
/// at their edges and generate real vertical transition faces where the height
/// changes, and collision resolves the same heights through
/// [`LevelSurfaces::floor_y_at`], so the walked surface always matches the
/// rendered one.
///
/// `material` overrides the region's floor material; `edge_material` overrides
/// the vertical transition faces (both fall back to the room's floor/wall
/// material). When regions overlap, the later entry wins, exactly like
/// overlapping [`FloorPatchDef`] entries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorRegionDef {
    pub x: f32,
    pub z: f32,
    pub width: f32,
    pub depth: f32,
    /// Vertical offset from the containing room's floor, in metres. Negative
    /// recesses, positive raises.
    #[serde(default)]
    pub offset_y: f32,
    /// Floor material id for the region; falls back to the room's floor.
    #[serde(default)]
    pub material: Option<String>,
    /// Material for the vertical transition faces around the region; falls back
    /// to the room's wall material.
    #[serde(default)]
    pub edge_material: Option<String>,
}

impl FloorRegionDef {
    /// Region footprint as `(x0, x1, z0, z1)`, normalised.
    #[must_use]
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        (
            self.x.min(self.x + self.width),
            self.x.max(self.x + self.width),
            self.z.min(self.z + self.depth),
            self.z.max(self.z + self.depth),
        )
    }

    /// True when `(x, z)` lies inside the region footprint.
    #[must_use]
    pub fn contains(&self, x: f32, z: f32) -> bool {
        if !x.is_finite() || !z.is_finite() {
            return false;
        }
        let (x0, x1, z0, z1) = self.bounds();
        x >= x0 && x <= x1 && z >= z0 && z <= z1
    }

    /// Vertical offset, sanitised to `0.0` for non-finite values.
    #[must_use]
    pub const fn offset(&self) -> f32 {
        if self.offset_y.is_finite() {
            self.offset_y
        } else {
            0.0
        }
    }
}

/// Player initial spawn position and orientation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpawnDef {
    pub x: f32,
    pub z: f32,
    #[serde(default)]
    pub yaw_degrees: f32,
}

/// Default material codes for room surfaces.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelDefaults {
    #[serde(default)]
    pub wall: String,
    #[serde(default)]
    pub floor: String,
    #[serde(default)]
    pub ceiling: String,
}

impl Default for LevelDefaults {
    fn default() -> Self {
        Self {
            wall: "core:wallpaper_yellow_01".into(),
            floor: "core:carpet_beige_01".into(),
            ceiling: "core:ceiling_panel_01".into(),
        }
    }
}

/// The axis a wall's length runs along: the longer of width/depth.
///
/// The serialized spelling (`"x"`/`"z"`) is reused by the gable ceiling profile
/// for the axis its ridge runs along.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WallAxis {
    X,
    Z,
}

impl WallAxis {
    /// Picks the axis a wall of the given dimensions runs along.
    ///
    /// The wall's length is the larger of `width`/`depth`; ties resolve to `X`.
    #[must_use]
    pub fn of(width: f32, depth: f32) -> Self {
        if width >= depth { Self::X } else { Self::Z }
    }
}

/// Rectangular wall footprint definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallDef {
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    pub z: f32,
    pub width: f32,
    pub depth: f32,
    #[serde(default)]
    pub height: Option<f32>,
    #[serde(default)]
    pub faces: HashMap<String, String>,
    /// Object-level material for this wall's length faces.
    /// `faces` overrides it per face; an omitted value keeps `defaults.wall`.
    /// Faces are named `north`/`south` on an X-axis wall and `west`/`east` on a
    /// Z-axis wall.
    #[serde(default)]
    pub material: Option<String>,
    /// Rectangular cutouts (doors, windows, passages, vents) through this wall.
    #[serde(default)]
    pub openings: Vec<WallOpeningDef>,
}

impl WallDef {
    #[must_use]
    pub fn resolved_height(&self, default_ceiling: f32) -> f32 {
        self.height.unwrap_or(default_ceiling)
    }

    /// The axis this wall's length runs along (the larger of width/depth).
    #[must_use]
    pub fn axis(&self) -> WallAxis {
        WallAxis::of(self.width.abs(), self.depth.abs())
    }

    /// Length of the wall's footprint along its length axis, in metres.
    #[must_use]
    pub fn length(&self) -> f32 {
        match self.axis() {
            WallAxis::X => self.width.abs(),
            WallAxis::Z => self.depth.abs(),
        }
    }

    /// Thickness of the wall across its length axis, in metres.
    #[must_use]
    pub fn thickness(&self) -> f32 {
        match self.axis() {
            WallAxis::X => self.depth.abs(),
            WallAxis::Z => self.width.abs(),
        }
    }

    /// Minimum (x, z) corner of the wall footprint.
    #[must_use]
    pub fn min_corner(&self) -> (f32, f32) {
        (
            self.x.min(self.x + self.width),
            self.z.min(self.z + self.depth),
        )
    }

    /// World (x, z) point at local offset 0 along the wall's length axis.
    ///
    /// Offsets increase along +X or +Z from the min corner, and the thickness
    /// axis starts at its min coordinate too, so this is the min corner for
    /// both axes and matches the current face layout.
    #[must_use]
    pub fn length_origin(&self) -> (f32, f32) {
        self.min_corner()
    }

    #[must_use]
    pub fn to_aabb(&self) -> WallAabb {
        let h = self.resolved_height(DEFAULT_CEILING_HEIGHT_M);
        WallAabb::with_y(self.x, self.y, self.z, self.width, h, self.depth)
    }

    #[must_use]
    pub fn to_aabb_with_ceiling(&self, default_ceiling: f32) -> WallAabb {
        let h = self.resolved_height(default_ceiling);
        WallAabb::with_y(self.x, self.y, self.z, self.width, h, self.depth)
    }
}

/// A rectangular cutout through a wall's thickness: doorway, window, passage, vent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallOpeningDef {
    /// Opening type: "door", "window", "passage", "vent" (unknown kinds are allowed for forward compatibility).
    #[serde(default = "default_opening_kind")]
    pub kind: String,
    /// Distance in metres along the wall's length axis from the wall's length origin to the opening's near edge.
    pub offset: f32,
    pub width: f32,
    pub height: f32,
    /// Height of the opening's bottom edge above the wall's base (wall.y). 0.0 = walk-through doorway.
    #[serde(default)]
    pub sill: f32,
    /// Optional surface material that fills the opening with a pane: the level's
    /// way to put **actual glass** in a window instead of leaving a hole.
    ///
    /// The value is an ordinary material id, so the pane's colour, dirt,
    /// roughness, sheen and translucency are the material's, not the opening's
    /// (`"glass": "core:glass_window_dirty_01"`). An opening without `glass` is
    /// exactly the historical hole.
    #[serde(default)]
    pub glass: Option<String>,
}

fn default_opening_kind() -> String {
    "door".into()
}

impl WallOpeningDef {
    /// Absolute Y of the opening's bottom edge for a wall based at `base_y`.
    #[must_use]
    pub fn bottom(&self, base_y: f32) -> f32 {
        base_y + self.sill
    }

    /// Absolute Y of the opening's top edge for a wall based at `base_y`.
    #[must_use]
    pub fn top(&self, base_y: f32) -> f32 {
        base_y + self.sill + self.height
    }

    /// Offset of the opening's far edge along the wall's length axis.
    #[must_use]
    pub fn end(&self) -> f32 {
        self.offset + self.width
    }

    /// True when the opening reaches the wall base (walk-through doorway).
    #[must_use]
    pub fn reaches_floor(&self) -> bool {
        self.sill <= 1e-3
    }

    /// True for walk-through openings ("door" and "passage").
    #[must_use]
    pub fn is_door(&self) -> bool {
        self.kind == "door" || self.kind == "passage"
    }

    /// The material id of the pane filling this opening, if it authors one.
    ///
    /// A blank or whitespace-only id is treated as "no glass" rather than as an
    /// unresolved material, so an empty string cannot paint the diagnostic
    /// pattern across a window.
    #[must_use]
    pub fn glass_material(&self) -> Option<&str> {
        self.glass
            .as_deref()
            .map(str::trim)
            .filter(|glass| !glass.is_empty())
    }
}

/// One solid rectangular slice of a wall in local wall space.
/// `start`/`end` are offsets along the wall's length axis; `bottom`/`top` are absolute Y.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallSlice {
    pub start: f32,
    pub end: f32,
    pub bottom: f32,
    pub top: f32,
}

/// Tolerance used when clamping and comparing wall opening geometry, in metres.
const WALL_SLICE_EPS: f32 = 1e-4;

/// Splits a wall into solid vertical slices, with the openings removed.
///
/// The returned slices are ordered by `start` and are suitable for building
/// geometry and collision. Openings that fall outside the wall or that do not
/// overlap the wall's vertical range are ignored defensively.
///
/// `ceiling_height` is the room's clear floor-to-ceiling height, matching the
/// historical signature; the room's own `floor_y` is not involved because the
/// wall's authored `y` is already absolute world space.
#[must_use]
pub fn wall_solid_slices(wall: &WallDef, ceiling_height: f32) -> Vec<WallSlice> {
    wall_solid_slices_profiled(wall, |_| ceiling_height, &[])
}

/// Splits a wall into solid vertical slices against a *varying* ceiling.
///
/// `clear_ceiling_at` returns the room's clear floor-to-ceiling height at a
/// distance along the wall's length axis, and `breaks` lists extra length
/// positions where that value is not linear (a gable ridge crossing a wall, for
/// example). Every returned slice therefore spans a length range over which the
/// ceiling is linear, which is what lets the emitter draw the wall's top edge as
/// a straight sloped line instead of a staircase. A slice's `top` is the
/// highest ceiling over its span, so collision boxes stay conservative; the
/// emitter clips each face against the exact local ceiling.
#[must_use]
pub fn wall_solid_slices_profiled(
    wall: &WallDef,
    clear_ceiling_at: impl Fn(f32) -> f32,
    breaks: &[f32],
) -> Vec<WallSlice> {
    let length = wall.length();
    if !length.is_finite() || length <= WALL_SLICE_EPS {
        return Vec::new();
    }

    // Top of the wall at a length offset: an explicitly authored height is
    // constant, an omitted one follows the room's ceiling profile.
    let top_at = |offset: f32| -> f32 {
        let clear = if wall.height.is_some() {
            wall.height.unwrap_or(0.0)
        } else {
            clear_ceiling_at(offset)
        };
        wall.y + clear
    };

    let mut probes: Vec<f32> = vec![0.0, length];
    probes.extend(
        breaks
            .iter()
            .copied()
            .filter(|at| at.is_finite() && *at > WALL_SLICE_EPS && *at < length - WALL_SLICE_EPS),
    );
    let base = probes.iter().fold(wall.y, |low, at| low.min(top_at(*at)));
    if !base.is_finite() {
        return Vec::new();
    }

    // Clamp every opening to the wall footprint and the ceiling over its own
    // span. Malformed entries (non-finite, zero-sized, out of range) are
    // ignored.
    let openings = clamped_wall_openings(wall, length, base, &top_at);

    // Split the wall's length at every opening boundary and profile break.
    let mut cuts: Vec<f32> = Vec::with_capacity(
        openings
            .len()
            .saturating_mul(2)
            .saturating_add(breaks.len())
            .saturating_add(2),
    );
    cuts.push(0.0);
    cuts.push(length);
    for opening in &openings {
        cuts.push(opening.start);
        cuts.push(opening.end);
    }
    for at in breaks {
        if at.is_finite() && *at > WALL_SLICE_EPS && *at < length - WALL_SLICE_EPS {
            cuts.push(*at);
        }
    }
    cuts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    cuts.dedup_by(|a, b| (*a - *b).abs() <= WALL_SLICE_EPS);

    // Emit the vertical complement of the openings covering each segment, so
    // neighbouring solid ranges stay merged.
    solid_wall_slices(&cuts, &openings, base, &top_at)
}

/// Clamps a wall's authored openings to its footprint and local ceiling.
///
/// Malformed entries (non-finite, zero-sized, outside the wall or the ceiling)
/// are dropped; every surviving opening is returned with `start`/`end` inside
/// `[0, length]` and `bottom`/`top` inside the wall's own vertical range.
fn clamped_wall_openings(
    wall: &WallDef,
    length: f32,
    base: f32,
    top_at: &impl Fn(f32) -> f32,
) -> Vec<WallSlice> {
    let mut openings: Vec<WallSlice> = Vec::with_capacity(wall.openings.len());
    for opening in &wall.openings {
        if !opening.offset.is_finite()
            || !opening.width.is_finite()
            || !opening.height.is_finite()
            || !opening.sill.is_finite()
        {
            continue;
        }
        if opening.width <= 0.0 || opening.height <= 0.0 {
            continue;
        }
        let start = opening.offset.clamp(0.0, length);
        let end = opening.end().clamp(0.0, length);
        if end <= start + WALL_SLICE_EPS {
            continue;
        }
        // The higher end of the opening's span is the conservative local
        // ceiling: a hole can never be taller than the wall that contains it.
        let local_ceiling = top_at(start).max(top_at(end));
        if !local_ceiling.is_finite() {
            continue;
        }
        let low = base.min(local_ceiling);
        let sill = opening.sill.max(0.0);
        let bottom = (base + sill).clamp(low, local_ceiling);
        let top = (base + sill + opening.height).clamp(low, local_ceiling);
        if top <= bottom + WALL_SLICE_EPS {
            continue;
        }
        openings.push(WallSlice {
            start,
            end,
            bottom,
            top,
        });
    }
    openings
}

/// Emits one solid slice per vertical span left between `openings` over every
/// length segment between consecutive `cuts`.
///
/// `cuts` must be sorted; adjacent segments separated by an opening boundary
/// produce separate slices exactly as the historical implementation did.
fn solid_wall_slices(
    cuts: &[f32],
    openings: &[WallSlice],
    base: f32,
    top_at: &impl Fn(f32) -> f32,
) -> Vec<WallSlice> {
    let mut slices = Vec::new();
    for bounds in cuts.windows(2) {
        let &[start, end] = bounds else {
            continue;
        };
        if end <= start + WALL_SLICE_EPS {
            continue;
        }
        let segment_ceiling = top_at(start).max(top_at(end));
        if !segment_ceiling.is_finite() || segment_ceiling <= base + WALL_SLICE_EPS {
            continue;
        }
        let mut holes: Vec<(f32, f32)> = openings
            .iter()
            .filter(|o| o.start <= start + WALL_SLICE_EPS && o.end + WALL_SLICE_EPS >= end)
            .map(|o| (o.bottom, o.top))
            .collect();
        holes.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        let mut cursor = base;
        for (hole_bottom, hole_top) in holes {
            if hole_bottom > cursor + WALL_SLICE_EPS {
                slices.push(WallSlice {
                    start,
                    end,
                    bottom: cursor,
                    top: hole_bottom,
                });
            }
            cursor = cursor.max(hole_top);
        }
        if segment_ceiling > cursor + WALL_SLICE_EPS {
            slices.push(WallSlice {
                start,
                end,
                bottom: cursor,
                top: segment_ceiling,
            });
        }
    }
    slices
}

/// Rectangular floor material patch (e.g. damp carpet).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorPatchDef {
    pub x: f32,
    pub z: f32,
    pub width: f32,
    pub depth: f32,
    pub material: String,
}

impl FloorPatchDef {
    /// Patch footprint as `(x0, x1, z0, z1)`, normalised.
    #[must_use]
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        (
            self.x.min(self.x + self.width),
            self.x.max(self.x + self.width),
            self.z.min(self.z + self.depth),
            self.z.max(self.z + self.depth),
        )
    }
}

/// Which surface a decal lies on, and therefore which way its outward normal
/// points.
///
/// The wall names match the wall face names of the level format: `north` faces
/// -Z, `south` +Z, `west` -X and `east` +X. Floors
/// face +Y and ceilings -Y. A decal is a small, intentionally decorative
/// surface marking (a sign, a floor line, a warning), so unlike a material
/// overlay it is a separate piece of geometry and never part of the wall it is
/// applied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecalSurface {
    /// Horizontal, normal +Y.
    Floor,
    /// Horizontal, normal -Y.
    Ceiling,
    /// Vertical, normal -Z.
    WallNorth,
    /// Vertical, normal +Z.
    WallSouth,
    /// Vertical, normal -X.
    WallWest,
    /// Vertical, normal +X.
    WallEast,
}

impl DecalSurface {
    /// Outward unit normal of the surface the decal lies on.
    #[must_use]
    pub const fn normal(self) -> [f32; 3] {
        match self {
            Self::Floor => [0.0, 1.0, 0.0],
            Self::Ceiling => [0.0, -1.0, 0.0],
            Self::WallNorth => [0.0, 0.0, -1.0],
            Self::WallSouth => [0.0, 0.0, 1.0],
            Self::WallWest => [-1.0, 0.0, 0.0],
            Self::WallEast => [1.0, 0.0, 0.0],
        }
    }

    /// True for floors and ceilings.
    #[must_use]
    pub const fn is_horizontal(self) -> bool {
        matches!(self, Self::Floor | Self::Ceiling)
    }

    /// True for ceiling decals, whose surface carries the ceiling shade.
    #[must_use]
    pub const fn is_ceiling(self) -> bool {
        matches!(self, Self::Ceiling)
    }
}

/// Largest decal edge the loader accepts, in metres.
///
/// Decals are surface decoration, not architecture; anything larger than a
/// normal sign or floor marking is almost certainly a malformed level rather
/// than an intentional overlay.
pub const MAX_DECAL_SIZE_M: f32 = 10.0;
/// Hard ceiling on the number of decals a level may place.
pub const MAX_LEVEL_DECALS: u64 = 5000;
/// Number of quads one decal generates.
pub const MAX_DECAL_QUADS: u64 = 1;

/// One local surface decal: a rectangular marking placed flat on an existing
/// wall, floor or ceiling.
///
/// `x`, `y` and `z` are the world-space centre of the decal and must lie on
/// the surface it targets (`surface` then fixes the normal and the default
/// in-plane axes). `width`/`height` are the decal's size in metres along its
/// own horizontal and vertical axes before `rotation_degrees` spins it in the
/// surface plane. `material` is a decal sheet id resolved by the renderer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecalDef {
    pub x: f32,
    /// Vertical centre of the decal. Floors and ceilings use their plane's
    /// height, walls the height on the wall.
    #[serde(default)]
    pub y: f32,
    pub z: f32,
    pub width: f32,
    pub height: f32,
    /// In-plane rotation about the surface normal, in degrees.
    #[serde(default)]
    pub rotation_degrees: f32,
    /// Decal sheet id, e.g. `core:decal_test_01`.
    pub material: String,
    pub surface: DecalSurface,
}

impl DecalDef {
    /// Half-size along the decal's own horizontal and vertical axes, in metres.
    #[must_use]
    pub const fn half_extents(&self) -> [f32; 2] {
        [self.width * 0.5, self.height * 0.5]
    }
}

/// Ceiling light fixture placement.
///
/// `brightness` is the optional fixture intensity/power. It is the field the
/// level editor already authors and writes, so it stays the canonical key; the
/// more descriptive `intensity` spelling is accepted as an alias so levels
/// written from the design notes load unchanged. Omitted means `1.0`.
///
/// `color` is the optional emitted light colour as an `[r, g, b]` array of
/// `0.0..=1.0` fractions. It drives both the fixture panel's visible tint and
/// the coloured illumination the bake applies to surrounding geometry. Levels
/// that omit it keep loading: they emit [`DEFAULT_LIGHT_COLOR`], the restrained
/// warm fluorescent the game has always implied.
/// Where a light fixture is mounted inside its room.
///
/// The level key is `ceiling_lights` for compatibility with existing levels;
/// it holds every fixture, including wall-mounted ones, which author
/// `"mount": "wall"` plus a world-space `y`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LightMount {
    /// Ceiling-mounted; the fixture hangs just below the room's ceiling and
    /// `y` is derived, not authored.
    #[default]
    Ceiling,
    /// Wall-mounted at the authored world `y`, facing `rotation_degrees`.
    Wall,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightFixtureDef {
    pub fixture: String,
    pub x: f32,
    pub z: f32,
    #[serde(default)]
    pub rotation_degrees: f32,
    #[serde(default, alias = "intensity")]
    pub brightness: Option<f32>,
    /// Emitted light colour; omitted means [`DEFAULT_LIGHT_COLOR`].
    #[serde(default)]
    pub color: Option<LightColor>,
    /// Ceiling (default) or wall mounting.
    #[serde(default)]
    pub mount: LightMount,
    /// World Y of a wall fixture's centre. Ignored for ceiling fixtures, whose
    /// height is derived from the room's ceiling.
    #[serde(default)]
    pub y: Option<f32>,
    /// Distance at which the light reaches zero, in metres. Omitted means
    /// [`crate::lighting::DEFAULT_LIGHT_RANGE_M`] (the historical pool radius).
    #[serde(default)]
    pub range: Option<f32>,
    /// Falloff curve; omitted means `smooth` (the historical pool curve).
    #[serde(default)]
    pub falloff: Option<crate::lighting::LightFalloff>,
    /// Whether the fixture casts environmental light. Defaults to `true`.
    ///
    /// `false` makes the fixture a *luminous object only*: its visible face
    /// still glows with its authored colour and brightness, while the bake skips
    /// it entirely. This is the authored half of the separation between material
    /// emission and environmental illumination — a sign, a screen or a
    /// decorative tube that reads bright while lighting nothing.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    /// Independent emissive strength for the fixture's visible face.
    ///
    /// Omitted means the face glows with the fixture's own `brightness`, the
    /// historical behaviour. Authoring it decouples the two sides of the
    /// fixture: a dying tube can read fully bright while casting its dim light,
    /// and a screen-like face can glow without its light being raised to match.
    /// The value drives only the material emission; illumination always comes
    /// from `brightness` (and only while `enabled`).
    #[serde(default)]
    pub emission: Option<f32>,
}

impl Default for LightFixtureDef {
    fn default() -> Self {
        Self {
            fixture: String::new(),
            x: 0.0,
            z: 0.0,
            rotation_degrees: 0.0,
            brightness: None,
            color: None,
            mount: LightMount::Ceiling,
            y: None,
            range: None,
            falloff: None,
            enabled: true,
            emission: None,
        }
    }
}

impl LightFixtureDef {
    /// Authored fixture intensity, sanitised for rendering.
    ///
    /// * omitted (or `NaN`) -> `1.0`, the standard fixture;
    /// * negative -> `0.0` (no output) rather than invalid negative lighting;
    /// * non-finite -> the finite [`MAX_LIGHT_INTENSITY`] or `0.0`.
    ///
    /// The value is therefore always finite and never negative; baking clamps it
    /// to [`crate::lighting::MAX_LIGHT_INTENSITY`] as well. It drives both the
    /// fixture's visible emission and, unless [`Self::enabled`] is false, the
    /// light it casts.
    #[must_use]
    pub fn intensity(&self) -> f32 {
        self.brightness
            .map_or(1.0, crate::lighting::sanitize_intensity)
    }

    /// Authored emissive strength of the fixture's visible face.
    ///
    /// Defaults to the fixture's own [`Self::intensity`], so an existing level
    /// keeps its appearance; an authored value lets the face read at a
    /// different brightness from the light the fixture casts.
    #[must_use]
    pub fn emission_intensity(&self) -> f32 {
        self.emission.map_or_else(
            || self.intensity(),
            |value| {
                if value.is_finite() {
                    value.clamp(0.0, crate::materials::MAX_EMISSION_INTENSITY)
                } else if value.is_sign_positive() {
                    crate::materials::MAX_EMISSION_INTENSITY
                } else {
                    0.0
                }
            },
        )
    }

    /// Emitted light colour, sanitised for baking.
    ///
    /// Omitted means [`DEFAULT_LIGHT_COLOR`]; authored channels are clamped
    /// into `[0, 1]` and non-finite channels emit nothing (see
    /// [`LightColor::sanitized`]). This is the single source of truth for both
    /// the fixture panel appearance and the coloured environmental illumination;
    /// the two must never diverge.
    #[must_use]
    pub fn emitted_color(&self) -> LightColor {
        self.color.unwrap_or(DEFAULT_LIGHT_COLOR).sanitized()
    }

    /// Authored range, or the documented default when omitted.
    #[must_use]
    pub fn range(&self) -> f32 {
        self.range
            .filter(|value| value.is_finite() && *value > 0.0)
            .map_or(crate::lighting::DEFAULT_LIGHT_RANGE_M, |value| {
                value.clamp(
                    crate::lighting::MIN_LIGHT_RANGE_M,
                    crate::lighting::MAX_LIGHT_RANGE_M,
                )
            })
    }

    /// Authored falloff curve, or the documented default when omitted.
    #[must_use]
    pub fn falloff(&self) -> crate::lighting::LightFalloff {
        self.falloff.unwrap_or_default()
    }
}

/// Authoring-level shape name of a prop-attached light.
///
/// The serialized form is flat — `{"shape": "rect", "half_width": 0.3, ...}` —
/// so a level stays readable and the validator can report one field at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LightShapeKind {
    /// A single point: an indicator LED, a small lamp.
    #[default]
    Point,
    /// A flat panel: a screen, a sign face, a diffuser.
    Rect,
    /// A tube: a fluorescent batten, a neon strip.
    Line,
}

/// One generic light attached to a placed object.
///
/// This is how an object — a vending machine, a TV, an arcade cabinet, a
/// future glowing prop — owns illumination without a new hardcoded light
/// family: the light's shape and numbers live here, and its position is an
/// offset in the prop's own local frame. Emission stays a separate property of
/// the object's material; a light authored here is the only way a prop
/// illuminates anything.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightDef {
    /// Shape of the emitting surface; defaults to `point`.
    #[serde(default)]
    pub shape: LightShapeKind,
    /// Half-extent along the local X axis, in metres (`rect` only).
    #[serde(default)]
    pub half_width: Option<f32>,
    /// Half-extent along the local Z axis, in metres (`rect` only).
    #[serde(default)]
    pub half_depth: Option<f32>,
    /// Total length along the local X axis, in metres (`line` only).
    #[serde(default)]
    pub length: Option<f32>,
    /// Position of the light's centre in the object's local frame, in metres.
    #[serde(default)]
    pub offset: [f32; 3],
    /// Yaw of the light's shape about Y, relative to the object, in degrees.
    #[serde(default)]
    pub rotation_degrees: f32,
    /// Emitted colour; omitted means [`DEFAULT_LIGHT_COLOR`].
    #[serde(default)]
    pub color: Option<LightColor>,
    /// Authored intensity; `brightness` is accepted as an alias. Omitted means
    /// the standard fixture strength (`1.0`).
    #[serde(default, alias = "brightness")]
    pub intensity: Option<f32>,
    /// Distance at which the light reaches zero, in metres. Omitted means
    /// [`crate::lighting::DEFAULT_LIGHT_RANGE_M`].
    #[serde(default)]
    pub range: Option<f32>,
    /// Falloff curve; omitted means `smooth`.
    #[serde(default)]
    pub falloff: Option<crate::lighting::LightFalloff>,
    /// Whether the light illuminates at all; defaults to `true`.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

impl Default for LightDef {
    fn default() -> Self {
        Self {
            shape: LightShapeKind::Point,
            half_width: None,
            half_depth: None,
            length: None,
            offset: [0.0; 3],
            rotation_degrees: 0.0,
            color: None,
            intensity: None,
            range: None,
            falloff: None,
            enabled: true,
        }
    }
}

impl LightDef {
    /// The engine-level shape this authored light describes.
    #[must_use]
    pub fn shape(&self) -> crate::lighting::LightShape {
        match self.shape {
            LightShapeKind::Point => crate::lighting::LightShape::Point,
            LightShapeKind::Rect => crate::lighting::LightShape::Rect {
                half_width: self.half_width.unwrap_or(0.0),
                half_depth: self.half_depth.unwrap_or(0.0),
            },
            LightShapeKind::Line => crate::lighting::LightShape::Line {
                length: self.length.unwrap_or(0.0),
            },
        }
    }

    /// Authored intensity, sanitised exactly like a fixture's `brightness`.
    #[must_use]
    pub fn intensity(&self) -> f32 {
        self.intensity
            .map_or(1.0, crate::lighting::sanitize_intensity)
    }

    /// Emitted colour, sanitised for baking.
    #[must_use]
    pub fn emitted_color(&self) -> LightColor {
        self.color.unwrap_or(DEFAULT_LIGHT_COLOR).sanitized()
    }

    /// Authored range, or the documented default when omitted.
    #[must_use]
    pub fn range(&self) -> f32 {
        self.range
            .filter(|value| value.is_finite() && *value > 0.0)
            .map_or(crate::lighting::DEFAULT_LIGHT_RANGE_M, |value| {
                value.clamp(
                    crate::lighting::MIN_LIGHT_RANGE_M,
                    crate::lighting::MAX_LIGHT_RANGE_M,
                )
            })
    }

    /// Authored falloff curve, or the documented default when omitted.
    #[must_use]
    pub fn falloff(&self) -> crate::lighting::LightFalloff {
        self.falloff.unwrap_or_default()
    }

    /// This authored light as an engine-level source at a resolved world
    /// position, with every value sanitised.
    ///
    /// `scale` is the owning object's scale: it scales the emitter's shape and
    /// (at the call site) its offset, exactly as object geometry scales.
    #[must_use]
    pub fn to_source(
        &self,
        position: [f32; 3],
        rotation_degrees: f32,
        scale: f32,
    ) -> crate::lighting::LightSource {
        crate::lighting::LightSource {
            shape: self.shape().scaled(scale),
            position,
            rotation_degrees,
            color: self.emitted_color(),
            intensity: self.intensity(),
            range: self.range(),
            falloff: self.falloff(),
            enabled: self.enabled,
        }
    }
}

/// Default for the `enabled` field of lights and fixtures.
const fn default_enabled() -> bool {
    true
}

/// Largest number of attached lights one placed prop may declare.
pub const MAX_PROP_LIGHTS: usize = 8;

/// Fallback prop box extents [width, height, depth] in metres, used whenever
/// neither the placed prop nor the prop catalog provides explicit sizes.
pub const PROP_FALLBACK_SIZE: [f32; 3] = [0.6, 0.9, 0.6];

const fn default_prop_scale() -> f32 {
    1.0
}

/// A placed prop / furniture / appliance instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropDef {
    /// Registry identifier, e.g. "core:couch". Resolved through the prop catalog.
    pub model: String,
    #[serde(default)]
    pub x: f32,
    /// Vertical offset of the prop's base above the local walkable floor (the
    /// containing room's `floor_y` plus any floor region). Negative values sink
    /// the prop into the floor (intentional). Because the floor of a legacy
    /// room is at world Y `0.0`, this was always absolute world Y in practice.
    #[serde(default)]
    pub y: f32,
    #[serde(default)]
    pub z: f32,
    #[serde(default)]
    pub rotation_degrees: f32,
    #[serde(default = "default_prop_scale")]
    pub scale: f32,
    /// Optional explicit box extents [width, height, depth] in metres, overriding the catalog entry.
    #[serde(default)]
    pub size: Option<[f32; 3]>,
    /// When true the prop blocks the player (axis-aligned box from position/size). Defaults to false.
    #[serde(default)]
    pub solid: bool,
    /// Generic light sources this object owns, positioned in its local frame.
    ///
    /// Zero by default: an object glows only through its material unless a
    /// light is authored here. Nothing about the object's model, material or
    /// category decides whether it lights a room.
    #[serde(default)]
    pub lights: Vec<LightDef>,
}

impl PropDef {
    /// Box extents in metres, applying `scale` to the explicit `size` when
    /// present or to `fallback` otherwise.
    #[must_use]
    pub fn resolved_size(&self, fallback: [f32; 3]) -> [f32; 3] {
        let base = self.size.unwrap_or(fallback);
        [
            base[0] * self.scale,
            base[1] * self.scale,
            base[2] * self.scale,
        ]
    }
}

/// Level schema supporting both single rooms and multiple connected room
/// sections.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelDef {
    pub format_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub room: Option<RoomDef>,
    #[serde(default)]
    pub rooms: Vec<RoomDef>,
    pub spawn: SpawnDef,
    #[serde(default)]
    pub defaults: LevelDefaults,
    #[serde(default)]
    pub walls: Vec<WallDef>,
    #[serde(default)]
    pub floor_patches: Vec<FloorPatchDef>,
    /// Rectangular local floor areas with their own vertical offset (recesses,
    /// raised platforms). Empty on every legacy level.
    #[serde(default)]
    pub floor_regions: Vec<FloorRegionDef>,
    /// Local surface decals (signs, floor markings, warnings).
    #[serde(default)]
    pub decals: Vec<DecalDef>,
    /// Every placed light fixture, in bake order.
    ///
    /// The key is `ceiling_lights` for compatibility with existing levels
    /// (and accepts `lights` as an alias); it holds every fixture, including
    /// wall-mounted ones, which author `"mount": "wall"` plus a world-space
    /// `y`. A fixture is visible geometry that owns one generic light; lights
    /// attached to props live on the prop instead (see [`PropDef::lights`]).
    #[serde(default, alias = "lights")]
    pub ceiling_lights: Vec<LightFixtureDef>,
    /// Placed props / furniture / appliances.
    #[serde(default)]
    pub props: Vec<PropDef>,
    /// Surfaces whose *emission* moves over time: a breathing illuminated sign,
    /// a failing tube. Empty on every level that does not ask for one.
    ///
    /// The animation scales the additive emissive term only. The baked
    /// illumination is static by design, so a flickering panel keeps lighting
    /// the room exactly as it was baked.
    #[serde(default)]
    pub animated_emissions: Vec<AnimatedEmissionDef>,
}

/// One animated emission a level declares, by material id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimatedEmissionDef {
    /// Material whose emissive term animates. Must be a material the level uses.
    pub material: String,
    /// `pulse` or `flicker`. Defaults to `pulse`.
    #[serde(default)]
    pub effect: Option<String>,
    /// Cycles per second; the effect's own default when absent.
    #[serde(default)]
    pub hz: Option<f32>,
    /// How far the emission may fall below its authored value.
    #[serde(default)]
    pub depth: Option<f32>,
    /// Phase offset in cycles, so two signs do not breathe in lockstep.
    #[serde(default)]
    pub phase: Option<f32>,
}

/// Number of quads the office fluorescent panel generates (panel plus two
/// bezels). Other fixture families declare their own budget on
/// [`crate::lighting::FixtureProfile::quads`].
pub const MAX_LIGHT_QUADS: u64 = 3;
/// Number of quads a prop generates in its placeholder-box form. Real prop
/// geometry is batched separately and bounded by [`MAX_LEVEL_PROP_VERTICES`].
pub const MAX_PROP_QUADS: u64 = 6;
/// Preferred triangle count for one prop model (see `assets/README.md`).
pub const PROP_TRIANGLE_TARGET: usize = 500;
/// Triangle count above which a prop model needs an explicit justification.
pub const PROP_TRIANGLE_REVIEW: usize = 800;
/// The Places art budget for one shipped prop model.
///
/// This is the count the prop tooling enforces when it builds the shipped
/// library, and the number `assets/README.md` documents. It is deliberately
/// **not** an engine limit: a model from another source that lands above it
/// still loads, with an art-budget warning naming the count, because the
/// renderer handles it correctly. The visual language is protected by the
/// budget being the authored norm, not by refusing the file.
pub const PROP_TRIANGLE_BUDGET: usize = 1_500;
/// Hard engine ceiling on one prop model's triangle count.
///
/// Four times the art budget: far above anything the Places visual language
/// wants, and still small enough that one model's vertices and the level's
/// instance budget stay bounded on the `PocketCHIP` target. A file above this is
/// genuinely unsupported rather than merely over budget.
pub const MAX_PROP_TRIANGLES: usize = 6_000;
/// Engine ceiling on the primitives (draw ranges) one prop model may declare.
///
/// Production GLBs split a model per material, so a handful is normal; the cap
/// exists so a pathological file cannot turn one prop into hundreds of draws.
pub const MAX_PROP_PRIMITIVES: usize = 32;
/// Engine ceiling on the materials one prop model may declare.
pub const MAX_PROP_MATERIALS: usize = 16;
/// Engine ceiling on the distinct images embedded in one prop model.
pub const MAX_PROP_IMAGES: usize = 16;
/// Hard ceiling on one prop model's vertex count (16-bit indices, `PocketCHIP` RAM).
pub const MAX_PROP_VERTICES: usize = 65_535;
/// The Places art budget for a prop texture's edge length.
///
/// Shipped prop artwork is 64x64 or 128x128 (256 for a few detailed sheets).
/// Larger embedded textures load and are downscaled to the runtime budget, but
/// tooling warns: the low-poly visual language wants restrained texture detail,
/// not photographic sheets hidden inside a crude mesh.
pub const PROP_TEXTURE_PREFERRED_SIZE: u32 = 256;
/// Hard engine ceiling on a prop texture's edge length.
///
/// Matches the surface decoder's [`crate::assets::MAX_TEXTURE_DIMENSION`]: a
/// GLB may carry a texture up to the same size any other asset may, and the
/// runtime quality profile decides what actually reaches the GPU.
pub const MAX_PROP_TEXTURE_SIZE: u32 = 1_024;

/// Hard ceiling on the number of distinct prop models a single level may use.
pub const MAX_LEVEL_PROP_MODELS: usize = 256;
/// Upper bound on the summed prop vertex count a level may expand into after
/// instance transforms are baked, keeping one level's prop geometry bounded.
pub const MAX_LEVEL_PROP_VERTICES: usize = 1_500_000;
/// Hard ceiling on the number of local floor regions a level may define.
pub const MAX_LEVEL_FLOOR_REGIONS: u64 = 2000;
/// Hard ceiling on the number of floor patches a level may define.
///
/// A patch is a material override, not geometry, so this only bounds parse and
/// lookup cost; it is deliberately the same order as the region budget.
pub const MAX_LEVEL_FLOOR_PATCHES: u64 = 2000;
/// Hard ceiling on the number of openings a single wall may declare.
pub const MAX_WALL_OPENINGS: usize = 64;
/// Hard byte ceiling on a standalone level JSON file before it is parsed.
///
/// The shipped demo is about 23 KB, so this is three orders of magnitude of
/// headroom for a hand-authored or generated level while still refusing an
/// accidentally huge file before it is read into memory.
pub const MAX_LEVEL_JSON_BYTES: u64 = 8 * 1024 * 1024;
/// PocketCHIP-safe budget for total authored floor area, in square metres.
///
/// Floor rendering no longer scales with area, but absurdly large levels still
/// stress collision, fill rate and level-design tooling, so a generous cap is
/// kept as a sanity guard.
pub const MAX_LEVEL_FLOOR_AREA_M2: u64 = 1_000_000;
/// PocketCHIP-safe budget on the estimated number of generated vertices.
pub const MAX_LEVEL_VERTICES: u64 = 2_000_000;

/// Estimated generated geometry for a level, used to bound memory use before
/// building vertex data and to reserve capacity without overallocating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GeometryEstimate {
    pub floor_area_m2: u64,
    pub floor_quads: u64,
    pub ceiling_quads: u64,
    pub wall_quads: u64,
    pub light_quads: u64,
    pub prop_quads: u64,
    pub decal_quads: u64,
    pub total_vertices: u64,
}

/// `value` clamped into `[0, max]` and rounded up, as `u64`.
///
/// A non-finite `value` counts as zero, matching what a float-to-integer cast
/// of a `NaN` has always produced.
const fn clamped_ceil_u64(value: f32, max: f32) -> u64 {
    let clamped = value.clamp(0.0, max);
    if !clamped.is_finite() {
        return 0;
    }
    // The clamp bounds the value to `[0, max]` and every call site passes a
    // `max` of 1_000_000, so the cast is in range; `ceil` has already removed
    // the fraction.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let result = clamped.ceil() as u64;
    result
}

impl LevelDef {
    /// # Errors
    ///
    /// Returns the `serde_json` error when the document is not valid JSON or
    /// does not match the level schema.
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }

    /// Iterates over all room sections (merging optional `room` and `rooms`)
    /// without cloning or allocating.
    pub fn room_iter(&self) -> impl Iterator<Item = &RoomDef> {
        self.rooms.iter().chain(self.room.iter())
    }

    /// Floor regions overlapping the given room, in authored order.
    ///
    /// A region is not scoped to one room: like a material-only floor patch it
    /// applies to every room it overlaps, resolved against that room's own
    /// `floor_y`. This is the single list the mesh, the collision rims and the
    /// walkable surface all read.
    #[must_use]
    pub fn floor_regions_for_room(&self, room: &RoomDef) -> Vec<&FloorRegionDef> {
        let (x0, x1, z0, z1) = room.bounds();
        self.floor_regions
            .iter()
            .filter(|region| {
                let (rx0, rx1, rz0, rz1) = region.bounds();
                rx1 > x0 && rx0 < x1 && rz1 > z0 && rz0 < z1
            })
            .collect()
    }

    /// Upper bound on the extra floor-grid cut lines the floor patches and
    /// floor regions overlapping `room` add, as an `(x, z)` count pair.
    ///
    /// Every patch/region edge inside the room becomes a cut line, so the
    /// floor's cell count grows by at most two per axis per intersecting
    /// element. Including them keeps [`Self::estimate_geometry`] an upper bound
    /// on what the builder emits.
    fn room_floor_cut_counts(&self, room: &RoomDef) -> (u64, u64) {
        let (x0, x1, z0, z1) = room.bounds();
        let mut x_cuts = 0u64;
        let mut z_cuts = 0u64;
        let edges = self
            .floor_patches
            .iter()
            .map(FloorPatchDef::bounds)
            .chain(self.floor_regions.iter().map(FloorRegionDef::bounds));
        for (ex0, ex1, ez0, ez1) in edges {
            if ex1 <= x0 || ex0 >= x1 || ez1 <= z0 || ez0 >= z1 {
                continue;
            }
            x_cuts = x_cuts.saturating_add(2);
            z_cuts = z_cuts.saturating_add(2);
        }
        (x_cuts, z_cuts)
    }

    /// Estimates the generated geometry for this level using saturating
    /// arithmetic, so malformed input cannot overflow the calculation.
    #[must_use]
    pub fn estimate_geometry(&self) -> GeometryEstimate {
        let mut floor_area_m2: u64 = 0;
        let mut floor_quads: u64 = 0;
        let mut ceiling_quads: u64 = 0;
        for room in self.room_iter() {
            let w = clamped_ceil_u64(room.width, 1_000_000.0);
            let d = clamped_ceil_u64(room.depth, 1_000_000.0);
            floor_area_m2 = floor_area_m2.saturating_add(w.saturating_mul(d));

            // Floors and ceilings are tessellated on the baked-lighting grid so
            // fixture pools can vary across them. The cell count is capped by
            // `lighting::MAX_LIGHT_GRID_CELLS`, so this stays bounded no matter
            // how large a room is. A floor patch or region adds two cut lines
            // per axis to the floor grid (its edges), which is what keeps its
            // boundary exact; a gable ridge adds one cut line to the ceiling
            // grid so the ridge is never approximated by a cell edge.
            let cells_x = u64::from(crate::lighting::light_grid_cells(room.width.abs()));
            let cells_z = u64::from(crate::lighting::light_grid_cells(room.depth.abs()));
            let (edge_x, edge_z) = self.room_floor_cut_counts(room);
            let floor_cells = cells_x
                .saturating_add(edge_x)
                .saturating_mul(cells_z.saturating_add(edge_z));
            floor_quads = floor_quads.saturating_add(floor_cells);

            let (ridge_x, ridge_z) = match room.ceiling.ridge_axis() {
                Some(WallAxis::X) => (0, 1),
                Some(WallAxis::Z) => (1, 0),
                None => (0, 0),
            };
            let ceiling_cells = cells_x
                .saturating_add(ridge_x)
                .saturating_mul(cells_z.saturating_add(ridge_z));
            ceiling_quads = ceiling_quads.saturating_add(ceiling_cells);

            // Recessed/raised regions need real vertical transition faces.
            // Every grid edge can carry at most one skirt, so the perimeter of
            // the room's floor grid bounds them, and a room without regions
            // adds none.
            if !self.floor_regions_for_room(room).is_empty() {
                let cols = cells_x.saturating_add(edge_x).saturating_add(1);
                let rows = cells_z.saturating_add(edge_z).saturating_add(1);
                let skirts = cols
                    .saturating_mul(rows)
                    .saturating_mul(2)
                    .saturating_add(cols.saturating_mul(2))
                    .saturating_add(rows.saturating_mul(2));
                floor_quads = floor_quads.saturating_add(skirts);
            }
        }

        // Walls are bounded by replaying the same solid-slice decomposition the
        // geometry builder uses (`wall_solid_slices_profiled`), so the estimate
        // tracks per-slice segment counts and opening reveals instead of
        // assuming a fixed number of faces per wall. Everything saturates, so
        // malformed dimensions cannot overflow the total.
        let surfaces = LevelSurfaces::new(self);
        let mut wall_quads: u64 = 0;
        for wall in &self.walls {
            let breaks = surfaces.wall_profile_breaks(wall);
            let clear = |offset: f32| surfaces.clear_ceiling_height_along(wall, offset);
            let slices = wall_solid_slices_profiled(wall, clear, &breaks);
            for slice in &slices {
                let segments = u64::from(crate::lighting::wall_light_segments(
                    slice.end - slice.start,
                ));
                wall_quads =
                    wall_quads.saturating_add(segments.saturating_mul(2).saturating_add(2));
            }

            // Cross-section faces appear at slice boundaries. The builder's
            // symmetric difference of the solid intervals on either side can
            // emit at most one merged interval per interval present, so the
            // number of intervals meeting at a boundary is a safe bound.
            let mut boundaries: Vec<f32> =
                Vec::with_capacity(slices.len().saturating_mul(2).saturating_add(2));
            boundaries.push(0.0);
            boundaries.push(wall.length());
            for slice in &slices {
                boundaries.push(slice.start);
                boundaries.push(slice.end);
            }
            boundaries.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            boundaries.dedup_by(|a, b| (*a - *b).abs() <= 1e-3);
            for position in boundaries {
                let ending = slices
                    .iter()
                    .filter(|slice| (slice.end - position).abs() <= 1e-3)
                    .count() as u64;
                let starting = slices
                    .iter()
                    .filter(|slice| (slice.start - position).abs() <= 1e-3)
                    .count() as u64;
                wall_quads = wall_quads.saturating_add(ending.saturating_add(starting));
            }
        }
        let light_quads = self.ceiling_lights.iter().fold(0u64, |total, light| {
            total.saturating_add(crate::lighting::fixture_profile(&light.fixture).quads)
        });
        let prop_quads = (self.props.len() as u64).saturating_mul(MAX_PROP_QUADS);
        let decal_quads = (self.decals.len() as u64).saturating_mul(MAX_DECAL_QUADS);
        let total_quads = floor_quads
            .saturating_add(ceiling_quads)
            .saturating_add(wall_quads)
            .saturating_add(light_quads)
            .saturating_add(prop_quads)
            .saturating_add(decal_quads);

        GeometryEstimate {
            floor_area_m2,
            floor_quads,
            ceiling_quads,
            wall_quads,
            light_quads,
            prop_quads,
            decal_quads,
            total_vertices: total_quads.saturating_mul(6),
        }
    }

    /// Returns collision bounding boxes for all solid level geometry.
    ///
    /// Walls contribute one box per solid slice, so doorways and other openings
    /// are genuinely passable; `solid` props contribute their axis-aligned box,
    /// placed on the local walkable floor so a prop in an elevated room or a
    /// recessed region lands on the surface it was authored against. Floor
    /// regions whose height differs from the surrounding floor by more than a
    /// walkable step contribute a rim box per grid edge, so the vertical faces
    /// the mesh draws under a depression are solid to the player too.
    #[must_use]
    pub fn collision_aabbs(&self) -> Vec<WallAabb> {
        let surfaces = LevelSurfaces::new(self);
        let mut aabbs = Vec::new();

        for wall in &self.walls {
            let breaks = surfaces.wall_profile_breaks(wall);
            let clear = |offset: f32| surfaces.clear_ceiling_height_along(wall, offset);
            let (origin_x, origin_z) = wall.length_origin();
            let (min_x, max_x) = (
                wall.x.min(wall.x + wall.width),
                wall.x.max(wall.x + wall.width),
            );
            let (min_z, max_z) = (
                wall.z.min(wall.z + wall.depth),
                wall.z.max(wall.z + wall.depth),
            );

            for slice in wall_solid_slices_profiled(wall, clear, &breaks) {
                let (slice_width, slice_depth) = match wall.axis() {
                    WallAxis::X => (slice.end - slice.start, max_z - min_z),
                    WallAxis::Z => (max_x - min_x, slice.end - slice.start),
                };
                let (slice_x, slice_z) = match wall.axis() {
                    WallAxis::X => (origin_x + slice.start, min_z),
                    WallAxis::Z => (min_x, origin_z + slice.start),
                };
                aabbs.push(WallAabb::with_y(
                    slice_x,
                    slice.bottom,
                    slice_z,
                    slice_width,
                    slice.top - slice.bottom,
                    slice_depth,
                ));
            }
        }

        for room in self.room_iter() {
            surfaces.floor_grid(room).push_region_rims(room, &mut aabbs);
        }

        for prop in &self.props {
            if !prop.solid {
                continue;
            }
            let size = prop.resolved_size(PROP_FALLBACK_SIZE);
            if !size.iter().all(|v| v.is_finite() && *v > 0.0)
                || !prop.x.is_finite()
                || !prop.y.is_finite()
                || !prop.z.is_finite()
            {
                continue;
            }
            let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
            aabbs.push(WallAabb::with_y(
                size[0].mul_add(-0.5, prop.x),
                base_y + prop.y,
                size[2].mul_add(-0.5, prop.z),
                size[0],
                size[1],
                size[2],
            ));
        }

        aabbs
    }
}

// ---------------------------------------------------------------------------
// Centralised vertical surface queries
// ---------------------------------------------------------------------------

/// One room's floor grid: the axis positions of the tessellation plus the
/// vertical offset of every cell relative to the room's floor.
///
/// The grid is cut at the baked-lighting resolution and at every floor
/// patch/region edge, exactly like the mesh the renderer emits, so rendering,
/// collision and the walkable surface can all answer "how high is the floor
/// here" from the same cells rather than re-deriving the geometry.
#[derive(Debug, Clone, Default)]
pub struct RoomFloorGrid {
    pub xs: Vec<f32>,
    pub zs: Vec<f32>,
    /// One offset in metres per cell, row-major over `xs` × `zs`.
    pub offsets: Vec<f32>,
}

impl RoomFloorGrid {
    /// Number of cells along X.
    #[must_use]
    pub const fn cells_x(&self) -> usize {
        self.xs.len().saturating_sub(1)
    }

    /// Number of cells along Z.
    #[must_use]
    pub const fn cells_z(&self) -> usize {
        self.zs.len().saturating_sub(1)
    }

    /// Vertical offset of cell `(ix, iz)` from the room's floor.
    #[must_use]
    pub fn offset_at(&self, ix: usize, iz: usize) -> f32 {
        self.offsets
            .get(iz.saturating_mul(self.cells_x()).saturating_add(ix))
            .copied()
            .unwrap_or(0.0)
    }

    /// World Y of cell `(ix, iz)`'s floor.
    #[must_use]
    pub fn y_at(&self, room: &RoomDef, ix: usize, iz: usize) -> f32 {
        room.floor_y + self.offset_at(ix, iz)
    }

    /// Emits a solid box for every grid edge where the floor height changes by
    /// more than a walkable step, spanning the edge and the height difference.
    ///
    /// Shallow steps are deliberately *not* solid: the player controller steps
    /// up and down them, which is what makes staircases built from floor regions
    /// work without any stair-specific code.
    ///
    /// Each rim's blocking face sits exactly on the boundary, so a player
    /// standing on the lower side stops one player radius short of the visible
    /// transition face, exactly as they do at an authored wall. The box is
    /// [`RIM_BACKING`] deep *under the higher floor*, which is what stops a
    /// sub-stepped move from tunnelling through a zero-thickness wall.
    pub fn push_region_rims(&self, room: &RoomDef, out: &mut Vec<WallAabb>) {
        let (cells_x, cells_z) = (self.cells_x(), self.cells_z());
        if cells_x == 0 || cells_z == 0 {
            return;
        }
        for (iz, z_span) in self.zs.windows(2).enumerate() {
            let &[z0, z1] = z_span else {
                continue;
            };
            for (ix, x_span) in self.xs.windows(2).enumerate() {
                let &[x0, x1] = x_span else {
                    continue;
                };
                let y = self.y_at(room, ix, iz);
                if ix.saturating_add(1) < cells_x {
                    let right = self.y_at(room, ix.saturating_add(1), iz);
                    if (right - y).abs() > PLAYER_STEP_HEIGHT + 1e-3 {
                        // Extend under the higher side so the blocking face is
                        // the boundary itself.
                        let (rx0, rx1) = if y > right {
                            (x1 - RIM_BACKING, x1)
                        } else {
                            (x1, x1 + RIM_BACKING)
                        };
                        out.push(WallAabb::with_y(
                            rx0,
                            y.min(right),
                            z0,
                            rx1 - rx0,
                            (y - right).abs(),
                            z1 - z0,
                        ));
                    }
                }
                if iz.saturating_add(1) < cells_z {
                    let back = self.y_at(room, ix, iz.saturating_add(1));
                    if (back - y).abs() > PLAYER_STEP_HEIGHT + 1e-3 {
                        let (rz0, rz1) = if y > back {
                            (z1 - RIM_BACKING, z1)
                        } else {
                            (z1, z1 + RIM_BACKING)
                        };
                        out.push(WallAabb::with_y(
                            x0,
                            y.min(back),
                            rz0,
                            x1 - x0,
                            (y - back).abs(),
                            rz1 - rz0,
                        ));
                    }
                }
            }
        }
    }
}

/// Depth of a floor-region rim collider under the higher floor, in metres.
///
/// The rim is a zero-thickness face in the mesh; the collider is a real box so
/// the circle-vs-box test is well-conditioned and a sub-stepped move (at most
/// `PLAYER_RADIUS * 0.5` per step) can never tunnel through it.
pub const RIM_BACKING: f32 = 0.4;

/// The vertical geometry of a level: rooms, their ceiling profiles and their
/// local floor regions, queried through one deterministic ownership rule.
///
/// This is the single source of truth for "where is the floor", "where is the
/// ceiling" and "which room is this". Rendering, collision and lighting all
/// resolve through it (lighting additionally keeps its own baked per-room
/// values), so a formula cannot drift between the mesh and the systems that
/// have to agree with it.
///
/// Ownership follows the historical `ceiling_height_at` rule: the first room in
/// `rooms` then `room` order whose footprint contains the point (with
/// [`ROOM_EDGE_EPS_M`] tolerance) wins. Legacy levels therefore resolve exactly
/// as they always did, including walls sitting on a shared room boundary.
#[derive(Debug, Clone)]
pub struct LevelSurfaces<'a> {
    rooms: Vec<&'a RoomDef>,
    regions: &'a [FloorRegionDef],
    patches: &'a [FloorPatchDef],
}

impl<'a> LevelSurfaces<'a> {
    /// Builds the surface atlas for a level. Cheap: it borrows the rooms and
    /// does not copy any geometry.
    #[must_use]
    pub fn new(level: &'a LevelDef) -> Self {
        Self {
            rooms: level.room_iter().collect(),
            regions: &level.floor_regions,
            patches: &level.floor_patches,
        }
    }

    /// Every room of the level, in resolution order.
    #[must_use]
    pub fn rooms(&self) -> &[&'a RoomDef] {
        &self.rooms
    }

    /// Index of the room containing `(x, z)`, first match in level order.
    #[must_use]
    pub fn room_index_at(&self, x: f32, z: f32) -> Option<usize> {
        self.rooms.iter().position(|room| room.contains(x, z))
    }

    /// The room containing `(x, z)`.
    #[must_use]
    pub fn room_at(&self, x: f32, z: f32) -> Option<&'a RoomDef> {
        self.room_index_at(x, z)
            .and_then(|index| self.rooms.get(index).copied())
    }

    /// The last (highest-precedence) floor region covering `(x, z)`.
    ///
    /// A region is not scoped to a room: it applies to every room it overlaps,
    /// resolved against that room's own floor. When two regions overlap the
    /// later one wins, matching the mesh builder's cell labelling exactly.
    #[must_use]
    pub fn region_at(&self, x: f32, z: f32) -> Option<&'a FloorRegionDef> {
        self.regions
            .iter()
            .rev()
            .find(|region| region.contains(x, z))
    }

    /// Vertical offset of the walkable floor from the containing room's floor
    /// plane at `(x, z)`: zero outside every region.
    #[must_use]
    pub fn floor_offset_at(&self, x: f32, z: f32) -> f32 {
        self.region_at(x, z).map_or(0.0, FloorRegionDef::offset)
    }

    /// World Y of the room's own floor plane at `(x, z)`, ignoring floor
    /// regions. Walls and beams are measured against this plane.
    #[must_use]
    pub fn room_floor_y_at(&self, x: f32, z: f32) -> Option<f32> {
        self.room_at(x, z).map(|room| {
            if room.floor_y.is_finite() {
                room.floor_y
            } else {
                0.0
            }
        })
    }

    /// World Y of the walkable floor surface at `(x, z)`: the containing room's
    /// floor plus any floor region's offset. `None` outside every room.
    #[must_use]
    pub fn floor_y_at(&self, x: f32, z: f32) -> Option<f32> {
        let room = self.room_at(x, z)?;
        let floor_y = if room.floor_y.is_finite() {
            room.floor_y
        } else {
            0.0
        };
        Some(floor_y + self.floor_offset_at(x, z))
    }

    /// World Y of the ceiling surface at `(x, z)`.
    ///
    /// Outside every room the first room's ceiling is used, and with no rooms
    /// at all the historical reference height stands in, so a wall or fixture
    /// that was authored off-room still resolves deterministically.
    #[must_use]
    pub fn ceiling_y_at(&self, x: f32, z: f32) -> f32 {
        if let Some(room) = self.room_at(x, z) {
            return room.ceiling_y_at(x, z);
        }
        self.rooms
            .first()
            .map_or(DEFAULT_CEILING_HEIGHT_M, |room| room.eave_y())
    }

    /// Clear floor-to-ceiling height at `(x, z)`, the value an un-heighted wall
    /// uses as its default height.
    #[must_use]
    pub fn clear_ceiling_height_at(&self, x: f32, z: f32) -> f32 {
        let ceiling = self.ceiling_y_at(x, z);
        let floor = self.room_floor_y_at(x, z).unwrap_or(0.0);
        let clear = ceiling - floor;
        if clear.is_finite() && clear > 0.0 {
            clear
        } else {
            DEFAULT_CEILING_HEIGHT_M
        }
    }

    /// Clear ceiling height at a distance along a wall's length axis.
    #[must_use]
    pub fn clear_ceiling_height_along(&self, wall: &WallDef, offset: f32) -> f32 {
        let (x, z) = wall_point(wall, offset);
        self.clear_ceiling_height_at(x, z)
    }

    /// World Y of the ceiling above a point given as a distance along a wall.
    #[must_use]
    pub fn ceiling_y_along(&self, wall: &WallDef, offset: f32) -> f32 {
        let (x, z) = wall_point(wall, offset);
        self.ceiling_y_at(x, z)
    }

    /// Length offsets along `wall` where its ceiling profile bends: the gable
    /// ridge when the wall crosses it, so wall slices stay within a linear span.
    #[must_use]
    pub fn wall_profile_breaks(&self, wall: &WallDef) -> Vec<f32> {
        let mut breaks = Vec::new();
        let length = wall.length();
        if !length.is_finite() || length <= 0.0 {
            return breaks;
        }
        let axis = wall.axis();
        let Some(room) = self.room_at(
            wall.width.mul_add(0.5, wall.x),
            wall.depth.mul_add(0.5, wall.z),
        ) else {
            return breaks;
        };
        let Some(ridge) = room.ridge_across() else {
            return breaks;
        };
        // The ridge only crosses the wall when it runs across the wall's length
        // axis; a wall parallel to the ridge sees a constant ceiling.
        let crosses = room
            .ceiling
            .ridge_axis()
            .is_some_and(|ridge_axis| ridge_axis != axis);
        if !crosses {
            return breaks;
        }
        let offset = match axis {
            WallAxis::X => ridge - wall.length_origin().0,
            WallAxis::Z => ridge - wall.length_origin().1,
        };
        if offset.is_finite() && offset > 1e-4 && offset < length - 1e-4 {
            breaks.push(offset);
        }
        breaks
    }

    /// True when the ceiling over `(x, z)` is a single horizontal plane.
    #[must_use]
    pub fn ceiling_is_flat_at(&self, x: f32, z: f32) -> bool {
        self.room_at(x, z).is_none_or(|room| room.ceiling.is_flat())
    }

    /// Floor patches overlapping `room`, in authored order.
    #[must_use]
    pub fn patches_for_room(&self, room: &RoomDef) -> Vec<&'a FloorPatchDef> {
        let (x0, x1, z0, z1) = room.bounds();
        self.patches
            .iter()
            .filter(|patch| {
                let (px0, px1, pz0, pz1) = (
                    patch.x.min(patch.x + patch.width),
                    patch.x.max(patch.x + patch.width),
                    patch.z.min(patch.z + patch.depth),
                    patch.z.max(patch.z + patch.depth),
                );
                px1 > x0 && px0 < x1 && pz1 > z0 && pz0 < z1
            })
            .collect()
    }

    /// The resolved floor grid of one room: cut positions and per-cell offsets.
    #[must_use]
    pub fn floor_grid(&self, room: &RoomDef) -> RoomFloorGrid {
        let cells_x = crate::lighting::light_grid_cells(room.width.abs());
        let cells_z = crate::lighting::light_grid_cells(room.depth.abs());
        let mut edges_x: Vec<f32> = Vec::new();
        let mut edges_z: Vec<f32> = Vec::new();
        for region in self.regions_for_room(room) {
            let (x0, x1, z0, z1) = region.bounds();
            edges_x.push(x0);
            edges_x.push(x1);
            edges_z.push(z0);
            edges_z.push(z1);
        }
        for patch in self.patches_for_room(room) {
            edges_x.push(patch.x);
            edges_x.push(patch.x + patch.width);
            edges_z.push(patch.z);
            edges_z.push(patch.z + patch.depth);
        }
        let xs = cut_positions(room.x, room.width, cells_x, &edges_x);
        let zs = cut_positions(room.z, room.depth, cells_z, &edges_z);
        let mut offsets = Vec::with_capacity(
            xs.len()
                .saturating_sub(1)
                .saturating_mul(zs.len().saturating_sub(1)),
        );
        for z_span in zs.windows(2) {
            let &[z0, z1] = z_span else {
                continue;
            };
            for x_span in xs.windows(2) {
                let &[x0, x1] = x_span else {
                    continue;
                };
                offsets.push(self.floor_offset_at(f32::midpoint(x0, x1), f32::midpoint(z0, z1)));
            }
        }
        RoomFloorGrid { xs, zs, offsets }
    }

    /// Rooms whose floor regions overlap `room`, in authored order.
    #[must_use]
    pub fn regions_for_room(&self, room: &RoomDef) -> Vec<&'a FloorRegionDef> {
        let (x0, x1, z0, z1) = room.bounds();
        self.regions
            .iter()
            .filter(|region| {
                let (rx0, rx1, rz0, rz1) = region.bounds();
                rx1 > x0 && rx0 < x1 && rz1 > z0 && rz0 < z1
            })
            .collect()
    }

    /// Axis positions of a room's ceiling grid: the baked-lighting grid plus the
    /// gable ridge, so the ridge lands exactly on a cell edge.
    #[must_use]
    pub fn ceiling_grid(&self, room: &RoomDef) -> (Vec<f32>, Vec<f32>) {
        let cells_x = crate::lighting::light_grid_cells(room.width.abs());
        let cells_z = crate::lighting::light_grid_cells(room.depth.abs());
        let mut xs = axis_positions(room.x, room.width, cells_x);
        let mut zs = axis_positions(room.z, room.depth, cells_z);
        if let Some(ridge) = room.ridge_across() {
            match room.ceiling.ridge_axis() {
                Some(WallAxis::X) => insert_cut(&mut zs, ridge, room.z, room.depth),
                Some(WallAxis::Z) => insert_cut(&mut xs, ridge, room.x, room.width),
                None => {}
            }
        }
        (xs, zs)
    }
}

/// World `(x, z)` of a point at a distance along a wall's length axis.
#[must_use]
pub fn wall_point(wall: &WallDef, offset: f32) -> (f32, f32) {
    let (origin_x, origin_z) = wall.length_origin();
    match wall.axis() {
        WallAxis::X => (
            origin_x + offset,
            f32::midpoint(wall.z, wall.z + wall.depth),
        ),
        WallAxis::Z => (
            f32::midpoint(wall.x, wall.x + wall.width),
            origin_z + offset,
        ),
    }
}

/// Evenly spaced surface positions, `cells + 1` values.
///
/// Every call site passes a baked-lighting cell count, capped at
/// [`crate::lighting::MAX_LIGHT_GRID_CELLS`], so `index` and `cells` both stay
/// far below `f32`'s exact-integer limit of 2^24.
#[must_use]
pub fn axis_positions(origin: f32, extent: f32, cells: u32) -> Vec<f32> {
    #[allow(clippy::cast_precision_loss)]
    let position = |index: u32| origin + extent * (index as f32) / (cells as f32);
    (0..=cells).map(position).collect()
}

/// Tolerance used when merging floor cut lines and matching patch edges.
pub const FLOOR_CUT_EPS: f32 = 1e-3;

/// Surface positions at the lighting resolution plus every supplied edge that
/// falls strictly inside the surface.
#[must_use]
pub fn cut_positions(origin: f32, extent: f32, cells: u32, edges: &[f32]) -> Vec<f32> {
    let mut positions = axis_positions(origin, extent, cells);
    let (low, high) = (origin + FLOOR_CUT_EPS, origin + extent - FLOOR_CUT_EPS);
    for edge in edges {
        if !edge.is_finite() || *edge <= low || *edge >= high {
            continue;
        }
        positions.push(*edge);
    }
    positions.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    positions.dedup_by(|a, b| (*a - *b).abs() <= FLOOR_CUT_EPS);
    positions
}

/// One floor region resolved into the walkable surface model.
#[derive(Debug, Clone, Copy, PartialEq)]
struct WalkableRegion {
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
    /// World Y of the walkable surface inside the region.
    y: f32,
}

/// One room of the walkable surface model.
#[derive(Debug, Clone, PartialEq)]
struct WalkableRoom {
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
    floor_y: f32,
    /// Regions resolved against this room, in authored order (later wins).
    regions: Vec<WalkableRegion>,
}

/// Owned, allocation-light floor model the player controller samples while
/// walking.
///
/// It is built once per level from the same [`LevelSurfaces`] queries the mesh
/// and collision use, so the height the player stands on is by construction the
/// height that was rendered. The player position is the only per-frame input;
/// the lookup is a linear scan over the level's rooms (bounded at 500 by the
/// loader) followed by that room's regions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WalkableFloor {
    rooms: Vec<WalkableRoom>,
}

impl WalkableFloor {
    /// Builds the walkable surface model for a level.
    #[must_use]
    pub fn from_level(level: &LevelDef) -> Self {
        let surfaces = LevelSurfaces::new(level);
        let mut rooms = Vec::with_capacity(surfaces.rooms().len());
        for room in surfaces.rooms() {
            let (x0, x1, z0, z1) = room.bounds();
            let floor_y = if room.floor_y.is_finite() {
                room.floor_y
            } else {
                0.0
            };
            let regions = surfaces
                .regions_for_room(room)
                .into_iter()
                .map(|region| {
                    let (rx0, rx1, rz0, rz1) = region.bounds();
                    WalkableRegion {
                        x0: rx0,
                        x1: rx1,
                        z0: rz0,
                        z1: rz1,
                        y: floor_y + region.offset(),
                    }
                })
                .collect();
            rooms.push(WalkableRoom {
                x0,
                x1,
                z0,
                z1,
                floor_y,
                regions,
            });
        }
        Self { rooms }
    }

    /// True when the level contains no rooms at all.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }

    /// Number of rooms in the model.
    #[must_use]
    pub const fn room_count(&self) -> usize {
        self.rooms.len()
    }

    /// World Y of the walkable floor at `(x, z)`, or `None` outside every room.
    ///
    /// The first room in level order containing the point wins, matching
    /// [`LevelSurfaces::floor_y_at`]; inside it the last authored floor region
    /// covering the point wins.
    #[must_use]
    pub fn height_at(&self, x: f32, z: f32) -> Option<f32> {
        if !x.is_finite() || !z.is_finite() {
            return None;
        }
        for room in &self.rooms {
            if x < room.x0 - ROOM_EDGE_EPS_M
                || x > room.x1 + ROOM_EDGE_EPS_M
                || z < room.z0 - ROOM_EDGE_EPS_M
                || z > room.z1 + ROOM_EDGE_EPS_M
            {
                continue;
            }
            for region in room.regions.iter().rev() {
                if x >= region.x0 && x <= region.x1 && z >= region.z0 && z <= region.z1 {
                    return Some(region.y);
                }
            }
            return Some(room.floor_y);
        }
        None
    }
}

/// Inserts one extra cut position into a surface axis when it is strictly
/// inside the span.
fn insert_cut(positions: &mut Vec<f32>, at: f32, origin: f32, extent: f32) {
    let (low, high) = (origin + FLOOR_CUT_EPS, origin + extent - FLOOR_CUT_EPS);
    if !at.is_finite() || at <= low || at >= high {
        return;
    }
    positions.push(at);
    positions.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    positions.dedup_by(|a, b| (*a - *b).abs() <= FLOOR_CUT_EPS);
}

#[cfg(test)]
mod tests;
