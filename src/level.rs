use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::collision::{PLAYER_STEP_HEIGHT, WallAabb};
use crate::lighting::{DEFAULT_LIGHT_COLOR, LightColor};

/// Clear ceiling height of a room whose level JSON omits `height`, in metres.
///
/// Phase 4 raised the standard default from the historical 3.5 m. Levels that
/// author `"height": 3.5` keep it verbatim; only rooms that leave the key out
/// (or that are created without one) receive the new default.
pub const DEFAULT_CEILING_HEIGHT_M: f32 = 4.0;

const fn default_ceiling_height() -> f32 {
    DEFAULT_CEILING_HEIGHT_M
}

/// Tolerance applied when testing whether a point lies inside a room footprint,
/// in metres. Shared by every room ownership lookup so walls, floors, ceilings,
/// fixtures and collision agree on where a room ends.
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
/// `material` and `ceiling_material` are the object-level material overrides of
/// design section 21 (individual surface override -> object-level material ->
/// level default material). Both are optional: an omitted value keeps the
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
    pub fn offset(&self) -> f32 {
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
    /// Object-level material for this wall's length faces (design section 21).
    /// `faces` overrides it per face; an omitted value keeps `defaults.wall`.
    /// Faces are named `north`/`south` on an X-axis wall and `west`/`east` on a
    /// Z-axis wall, matching the design document's example.
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

    // Split the wall's length at every opening boundary and profile break.
    let mut cuts: Vec<f32> = Vec::with_capacity(openings.len() * 2 + breaks.len() + 2);
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
    let mut slices = Vec::new();
    for bounds in cuts.windows(2) {
        let (start, end) = (bounds[0], bounds[1]);
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
/// The wall names match the wall face names of the level format (and design
/// section 21): `north` faces -Z, `south` +Z, `west` -X and `east` +X. Floors
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
pub struct CeilingLightDef {
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
}

impl CeilingLightDef {
    /// Authored fixture intensity, sanitised for rendering.
    ///
    /// * omitted (or `NaN`) -> `1.0`, the standard fixture;
    /// * negative -> `0.0` (no output) rather than invalid negative lighting;
    /// * non-finite -> the finite [`MAX_LIGHT_INTENSITY`] or `0.0`.
    ///
    /// The value is therefore always finite and never negative; baking clamps it
    /// to [`crate::lighting::MAX_LIGHT_INTENSITY`] as well.
    #[must_use]
    pub fn intensity(&self) -> f32 {
        self.brightness
            .map_or(1.0, crate::lighting::sanitize_intensity)
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
}

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

/// Level schema corresponding to Sections 22 and 24 of the design document,
/// supporting both single rooms and multiple connected room sections.
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
    #[serde(default)]
    pub ceiling_lights: Vec<CeilingLightDef>,
    /// Placed props / furniture / appliances.
    #[serde(default)]
    pub props: Vec<PropDef>,
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
/// Hard ceiling on one prop model's triangle count, enforced by the loader.
pub const MAX_PROP_TRIANGLES: usize = 1_500;
/// Hard ceiling on one prop model's vertex count (16-bit indices, `PocketCHIP` RAM).
pub const MAX_PROP_VERTICES: usize = 65_535;
/// Hard ceiling on prop texture dimensions; 64x64/128x128 are the preferred sizes.
pub const MAX_PROP_TEXTURE_SIZE: u32 = 256;
/// Hard ceiling on the number of distinct prop models a single level may use.
pub const MAX_LEVEL_PROP_MODELS: usize = 256;
/// Upper bound on the summed prop vertex count a level may expand into after
/// instance transforms are baked, keeping one level's prop geometry bounded.
pub const MAX_LEVEL_PROP_VERTICES: usize = 1_500_000;
/// Hard ceiling on the number of local floor regions a level may define.
pub const MAX_LEVEL_FLOOR_REGIONS: u64 = 2000;
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
            .map(|patch| patch.bounds())
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
            let w = room.width.clamp(0.0, 1_000_000.0).ceil() as u64;
            let d = room.depth.clamp(0.0, 1_000_000.0).ceil() as u64;
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
            let mut boundaries: Vec<f32> = Vec::with_capacity(slices.len() * 2 + 2);
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
    pub fn cells_x(&self) -> usize {
        self.xs.len().saturating_sub(1)
    }

    /// Number of cells along Z.
    #[must_use]
    pub fn cells_z(&self) -> usize {
        self.zs.len().saturating_sub(1)
    }

    /// Vertical offset of cell `(ix, iz)` from the room's floor.
    #[must_use]
    pub fn offset_at(&self, ix: usize, iz: usize) -> f32 {
        self.offsets
            .get(iz * self.cells_x() + ix)
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
        for iz in 0..cells_z {
            for ix in 0..cells_x {
                let y = self.y_at(room, ix, iz);
                if ix + 1 < cells_x {
                    let right = self.y_at(room, ix + 1, iz);
                    if (right - y).abs() > PLAYER_STEP_HEIGHT + 1e-3 {
                        let at = self.xs[ix + 1];
                        // Extend under the higher side so the blocking face is
                        // the boundary itself.
                        let (x0, x1) = if y > right {
                            (at - RIM_BACKING, at)
                        } else {
                            (at, at + RIM_BACKING)
                        };
                        out.push(WallAabb::with_y(
                            x0,
                            y.min(right),
                            self.zs[iz],
                            x1 - x0,
                            (y - right).abs(),
                            self.zs[iz + 1] - self.zs[iz],
                        ));
                    }
                }
                if iz + 1 < cells_z {
                    let back = self.y_at(room, ix, iz + 1);
                    if (back - y).abs() > PLAYER_STEP_HEIGHT + 1e-3 {
                        let at = self.zs[iz + 1];
                        let (z0, z1) = if y > back {
                            (at - RIM_BACKING, at)
                        } else {
                            (at, at + RIM_BACKING)
                        };
                        out.push(WallAabb::with_y(
                            self.xs[ix],
                            y.min(back),
                            z0,
                            self.xs[ix + 1] - self.xs[ix],
                            (y - back).abs(),
                            z1 - z0,
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
        self.room_index_at(x, z).map(|index| self.rooms[index])
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
        let crosses = match room.ceiling.ridge_axis() {
            Some(ridge_axis) => ridge_axis != axis,
            None => false,
        };
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
        let mut offsets =
            Vec::with_capacity(xs.len().saturating_sub(1) * zs.len().saturating_sub(1));
        for iz in 0..zs.len().saturating_sub(1) {
            for ix in 0..xs.len().saturating_sub(1) {
                let x = f32::midpoint(xs[ix], xs[ix + 1]);
                let z = f32::midpoint(zs[iz], zs[iz + 1]);
                offsets.push(self.floor_offset_at(x, z));
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
#[must_use]
pub fn axis_positions(origin: f32, extent: f32, cells: u32) -> Vec<f32> {
    (0..=cells)
        .map(|index| origin + extent * index as f32 / cells as f32)
        .collect()
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
    pub fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }

    /// Number of rooms in the model.
    #[must_use]
    pub fn room_count(&self) -> usize {
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
mod tests {
    use super::*;
    use crate::test_support::assert_exact;

    #[test]
    fn test_parse_single_room_level() {
        let json = r#"{
            "format_version": 1,
            "id": "test_room",
            "name": "Test Room",
            "spawn": { "x": 0.0, "z": 0.0 },
            "room": { "x": -6.0, "z": -12.0, "width": 12.0, "depth": 16.0, "height": 3.5 }
        }"#;
        let level = LevelDef::from_json(json).expect("valid json");
        let rooms: Vec<&RoomDef> = level.room_iter().collect();
        assert_eq!(rooms.len(), 1);
        assert_exact(rooms[0].width, 12.0);
        assert_exact(rooms[0].height, 3.5);
    }

    #[test]
    fn test_parse_multi_room_level_with_walls() {
        let json = r#"{
            "format_version": 1,
            "id": "multi_room",
            "name": "Connected Rooms",
            "spawn": { "x": 2.0, "z": 2.0, "yaw_degrees": 90.0 },
            "rooms": [
                { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0 },
                { "x": 10.0, "z": 2.0, "width": 8.0, "depth": 6.0 }
            ],
            "walls": [
                { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 0.35 }
            ]
        }"#;
        let level = LevelDef::from_json(json).expect("valid multi room json");
        assert_eq!(level.room_iter().count(), 2);
        assert_eq!(level.walls.len(), 1);
        let aabbs = level.collision_aabbs();
        assert_eq!(aabbs.len(), 1);
        assert_exact(aabbs[0].max_x, 10.0);
    }

    #[test]
    fn test_parse_variable_wall_properties() {
        let json = r#"{
            "format_version": 1,
            "id": "variable_walls",
            "name": "Variable Walls Test",
            "spawn": { "x": 0.0, "z": 0.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0 },
            "walls": [
                { "x": 1.0, "z": 1.0, "width": 2.0, "depth": 0.2 },
                { "x": 5.0, "z": 5.0, "width": 3.0, "depth": 0.2, "y": 0.0, "height": 1.5 },
                { "x": 5.0, "z": 5.0, "width": 3.0, "depth": 0.2, "y": 2.5, "height": 1.5 }
            ]
        }"#;
        let level = LevelDef::from_json(json).expect("valid variable walls json");
        assert_eq!(level.walls.len(), 3);

        // Wall 0: y omitted (defaults to 0.0), height omitted (defaults to room height 4.0)
        assert_exact(level.walls[0].y, 0.0);
        assert_eq!(level.walls[0].height, None);
        assert_exact(level.walls[0].resolved_height(4.0), 4.0);

        // Wall 1: window sill (half-height)
        assert_exact(level.walls[1].y, 0.0);
        assert_eq!(level.walls[1].height, Some(1.5));

        // Wall 2: window header (raised)
        assert_exact(level.walls[2].y, 2.5);
        assert_eq!(level.walls[2].height, Some(1.5));

        let aabbs = level.collision_aabbs();
        assert_eq!(aabbs.len(), 3);
        assert_exact(aabbs[0].min_y, 0.0);
        assert_exact(aabbs[0].max_y, 4.0);
        assert_exact(aabbs[1].min_y, 0.0);
        assert_exact(aabbs[1].max_y, 1.5);
        assert_exact(aabbs[2].min_y, 2.5);
        assert_exact(aabbs[2].max_y, 4.0);
    }

    #[test]
    fn test_estimate_geometry_scales_with_rooms_not_area() {
        // A 100x100 m room must stay a bounded number of floor/ceiling quads,
        // and a 400x400 m room must not cost any more: the baked-lighting grid
        // is capped per axis, so geometry never scales with floor area.
        let json = r#"{
            "format_version": 1,
            "id": "big",
            "name": "Big",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 100.0, "depth": 100.0, "height": 3.5 }]
        }"#;
        let level = LevelDef::from_json(json).expect("valid json");
        let estimate = level.estimate_geometry();
        let cap = u64::from(
            crate::lighting::MAX_LIGHT_GRID_CELLS * crate::lighting::MAX_LIGHT_GRID_CELLS,
        );
        assert!(
            estimate.floor_quads <= cap,
            "floor geometry must stay bounded, got {} quads",
            estimate.floor_quads
        );
        assert!(estimate.floor_quads > 1, "a large room is subdivided");
        assert_eq!(estimate.ceiling_quads, estimate.floor_quads);
        assert_eq!(estimate.floor_area_m2, 10_000);

        let huge = r#"{
            "format_version": 1,
            "id": "huge",
            "name": "Huge",
            "spawn": { "x": 0.0, "z": 0.0 },
            "rooms": [{ "x": 0.0, "z": 0.0, "width": 400.0, "depth": 400.0, "height": 3.5 }]
        }"#;
        let huge = LevelDef::from_json(huge).expect("valid json");
        assert_eq!(huge.estimate_geometry().floor_quads, estimate.floor_quads);

        // A small room that needs no lighting resolution stays a single quad.
        let small = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "small",
                "name": "Small",
                "spawn": { "x": 0.0, "z": 0.0 },
                "rooms": [{ "x": 0.0, "z": 0.0, "width": 2.0, "depth": 2.0 }]
            }"#,
        )
        .expect("valid json");
        let small = small.estimate_geometry();
        assert_eq!(small.floor_quads, 1);
        assert_eq!(small.ceiling_quads, 1);
    }

    #[test]
    fn test_estimate_geometry_saturates_on_extreme_input() {
        // Direct construction with absurd dimensions must not overflow or panic.
        let level = LevelDef {
            format_version: 1,
            id: "extreme".into(),
            name: "Extreme".into(),
            author: String::new(),
            room: None,
            rooms: vec![RoomDef {
                x: f32::MAX,
                z: -f32::MAX,
                width: 1.0e30,
                depth: 1.0e30,
                height: 3.5,
                floor_y: 0.0,
                ceiling: CeilingProfileDef::Flat,
                material: None,
                ceiling_material: None,
            }],
            spawn: SpawnDef {
                x: 0.0,
                z: 0.0,
                yaw_degrees: 0.0,
            },
            defaults: LevelDefaults::default(),
            walls: Vec::new(),
            floor_patches: Vec::new(),
            floor_regions: Vec::new(),
            decals: Vec::new(),
            ceiling_lights: Vec::new(),
            props: Vec::new(),
        };
        let estimate = level.estimate_geometry();
        // Values are clamped before multiplication, so no wrap-around occurs and
        // the absurd area is still reported as over budget.
        assert!(estimate.floor_area_m2 >= MAX_LEVEL_FLOOR_AREA_M2);
        assert!(estimate.floor_quads >= 1);
        assert!(estimate.total_vertices < MAX_LEVEL_VERTICES);
    }

    fn wall_with_openings(openings_json: &str) -> WallDef {
        let json = format!(
            r#"{{
                "x": 0.0, "y": 0.0, "z": 0.0,
                "width": 4.0, "depth": 0.4, "height": 3.5,
                "openings": {openings_json}
            }}"#
        );
        serde_json::from_str(&json).expect("valid wall json")
    }

    #[test]
    fn test_parse_wall_with_doorway_opening() {
        let wall = wall_with_openings(r#"[{ "offset": 1.0, "width": 1.0, "height": 2.1 }]"#);
        assert_eq!(wall.openings.len(), 1);
        let door = &wall.openings[0];
        // `kind` defaults to "door" and `sill` to a walk-through doorway.
        assert_eq!(door.kind, "door");
        assert_exact(door.sill, 0.0);
        assert!(door.is_door());
        assert!(door.reaches_floor());
        assert_exact(door.end(), 2.0);
        assert_exact(door.bottom(0.0), 0.0);
        assert_exact(door.top(0.0), 2.1);
        assert_exact(door.bottom(1.0), 1.0);
    }

    #[test]
    fn test_wall_axis_and_length_helpers() {
        let x_wall = wall_with_openings("[]");
        assert_eq!(x_wall.axis(), WallAxis::X);
        assert_exact(x_wall.length(), 4.0);
        assert_exact(x_wall.thickness(), 0.4);
        assert_eq!(x_wall.min_corner(), (0.0, 0.0));
        assert_eq!(x_wall.length_origin(), (0.0, 0.0));

        let json = r#"{
            "x": 5.0, "z": -3.0, "width": 0.4, "depth": 6.0, "height": 3.5
        }"#;
        let z_wall: WallDef = serde_json::from_str(json).expect("valid z wall");
        assert_eq!(z_wall.axis(), WallAxis::Z);
        assert_exact(z_wall.length(), 6.0);
        assert_exact(z_wall.thickness(), 0.4);
        assert_eq!(z_wall.length_origin(), (5.0, -3.0));

        // Negative dimensions still expose a positive length from the min corner.
        let negative: WallDef =
            serde_json::from_str(r#"{ "x": 4.0, "z": 1.0, "width": -4.0, "depth": -0.4 }"#)
                .expect("valid negative wall");
        assert_eq!(negative.axis(), WallAxis::X);
        assert_exact(negative.length(), 4.0);
        assert_eq!(negative.min_corner(), (0.0, 0.6));
    }

    #[test]
    fn test_wall_solid_slices_without_openings_is_one_full_slice() {
        let wall = wall_with_openings("[]");
        let slices = wall_solid_slices(&wall, 3.5);
        assert_eq!(
            slices,
            vec![WallSlice {
                start: 0.0,
                end: 4.0,
                bottom: 0.0,
                top: 3.5,
            }]
        );
    }

    #[test]
    fn test_wall_solid_slices_with_doorway() {
        let wall = wall_with_openings(
            r#"[{ "kind": "door", "offset": 1.0, "width": 1.0, "height": 2.1 }]"#,
        );
        let slices = wall_solid_slices(&wall, 3.5);
        assert_eq!(slices.len(), 3);
        // Left jamb, door header, right jamb.
        assert_exact(slices[0].start, 0.0);
        assert_exact(slices[0].end, 1.0);
        assert_eq!((slices[0].bottom, slices[0].top), (0.0, 3.5));
        assert_exact(slices[1].start, 1.0);
        assert_exact(slices[1].end, 2.0);
        assert_eq!((slices[1].bottom, slices[1].top), (2.1, 3.5));
        assert_exact(slices[2].start, 2.0);
        assert_exact(slices[2].end, 4.0);
        assert_eq!((slices[2].bottom, slices[2].top), (0.0, 3.5));
    }

    #[test]
    fn test_wall_solid_slices_with_window_above_floor() {
        // A window spanning the whole wall leaves only a sill and a header.
        let json = r#"{
            "x": 0.0, "z": 0.0, "width": 3.0, "depth": 0.4, "height": 3.5,
            "openings": [{ "kind": "window", "offset": 0.0, "width": 3.0, "height": 1.2, "sill": 1.0 }]
        }"#;
        let wall: WallDef = serde_json::from_str(json).expect("valid wall");
        let slices = wall_solid_slices(&wall, 3.5);
        assert_eq!(slices.len(), 2);
        assert_eq!((slices[0].bottom, slices[0].top), (0.0, 1.0));
        assert_eq!((slices[1].bottom, slices[1].top), (2.2, 3.5));
        assert!(!wall.openings[0].reaches_floor());
    }

    #[test]
    fn test_wall_solid_slices_with_two_openings() {
        let wall = wall_with_openings(
            r#"[
                { "kind": "door", "offset": 1.0, "width": 1.0, "height": 2.1 },
                { "kind": "window", "offset": 2.5, "width": 1.0, "height": 1.0, "sill": 1.0 }
            ]"#,
        );
        let slices = wall_solid_slices(&wall, 3.5);
        // [0,1] full, [1,2] header, [2,2.5] full, [2.5,3.5] sill+header, [3.5,4] full.
        assert_eq!(slices.len(), 6);
        let sill = slices
            .iter()
            .find(|s| (s.start - 2.5).abs() < 1e-4 && s.top <= 1.0 + 1e-4)
            .expect("window sill slice");
        assert_eq!((sill.bottom, sill.top), (0.0, 1.0));
        let header = slices
            .iter()
            .find(|s| (s.start - 2.5).abs() < 1e-4 && s.bottom >= 2.0 - 1e-4)
            .expect("window header slice");
        assert_eq!((header.bottom, header.top), (2.0, 3.5));
        // Slices are ordered by start.
        assert!(slices.windows(2).all(|w| w[0].start <= w[1].start));
    }

    #[test]
    fn test_wall_solid_slices_with_opening_flush_to_wall_end() {
        let wall = wall_with_openings(
            r#"[{ "kind": "passage", "offset": 0.0, "width": 1.0, "height": 2.1 }]"#,
        );
        let slices = wall_solid_slices(&wall, 3.5);
        assert_eq!(slices.len(), 2);
        assert_eq!((slices[0].start, slices[0].end), (0.0, 1.0));
        assert_eq!((slices[0].bottom, slices[0].top), (2.1, 3.5));
        assert_eq!((slices[1].start, slices[1].end), (1.0, 4.0));
        assert_eq!((slices[1].bottom, slices[1].top), (0.0, 3.5));
        assert!(wall.openings[0].is_door());
    }

    #[test]
    fn test_wall_solid_slices_ignores_out_of_range_openings() {
        // Beyond the wall end, NaN values, a zero width and a sill above the
        // wall top must all be ignored without panicking.
        let wall = wall_with_openings(
            r#"[
                { "offset": 10.0, "width": 1.0, "height": 2.1 },
                { "offset": 0.0, "width": 0.0, "height": 2.1 },
                { "offset": 0.5, "width": 1.0, "height": 2.1, "sill": 100.0 }
            ]"#,
        );
        let slices = wall_solid_slices(&wall, 3.5);
        assert_eq!(
            slices,
            vec![WallSlice {
                start: 0.0,
                end: 4.0,
                bottom: 0.0,
                top: 3.5,
            }]
        );

        let nan_wall =
            wall_with_openings(r#"[{ "offset": 1.0, "width": 1.0, "height": 2.1, "sill": 0.0 }]"#);
        let mut nan_wall = nan_wall;
        nan_wall.openings[0].offset = f32::NAN;
        assert_eq!(wall_solid_slices(&nan_wall, 3.5).len(), 1);

        // A wall with no length produces no slices at all.
        let mut empty = nan_wall.clone();
        empty.openings.clear();
        empty.width = 0.0;
        empty.depth = 0.0;
        assert!(wall_solid_slices(&empty, 3.5).is_empty());
    }

    #[test]
    fn test_wall_solid_slices_clamps_oversized_opening() {
        // An opening larger than the wall removes it entirely from collision.
        let wall = wall_with_openings(r#"[{ "offset": -1.0, "width": 10.0, "height": 10.0 }]"#);
        assert!(wall_solid_slices(&wall, 3.5).is_empty());
    }

    #[test]
    fn test_wall_solid_slices_z_axis_wall() {
        // A wall whose length runs along Z uses depth as its length.
        let json = r#"{
            "x": 4.8, "z": 0.0, "width": 0.4, "depth": 6.0, "height": 3.5,
            "openings": [{ "kind": "door", "offset": 2.0, "width": 1.0, "height": 2.1 }]
        }"#;
        let wall: WallDef = serde_json::from_str(json).expect("valid z wall");
        let slices = wall_solid_slices(&wall, 3.5);
        assert_eq!(slices.len(), 3);
        assert_eq!((slices[0].start, slices[0].end), (0.0, 2.0));
        assert_eq!((slices[0].bottom, slices[0].top), (0.0, 3.5));
        assert_eq!((slices[1].start, slices[1].end), (2.0, 3.0));
        assert_eq!((slices[1].bottom, slices[1].top), (2.1, 3.5));
        assert_eq!((slices[2].start, slices[2].end), (3.0, 6.0));
    }

    #[test]
    fn test_estimate_geometry_accounts_for_openings_and_props() {
        let json = r#"{
            "format_version": 1,
            "id": "estimate",
            "name": "Estimate",
            "spawn": { "x": 0.0, "z": 0.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 },
            "walls": [{
                "x": 0.0, "z": 0.0, "width": 4.0, "depth": 0.4,
                "openings": [{ "offset": 1.0, "width": 1.0, "height": 2.1 }]
            }],
            "props": [{ "model": "core:crate", "x": 1.0, "z": 1.0 }]
        }"#;
        let level = LevelDef::from_json(json).expect("valid json");
        let estimate = level.estimate_geometry();
        assert_eq!(estimate.prop_quads, MAX_PROP_QUADS);
        // The wall estimate follows the real solid slices: three slices (left
        // jamb, door header, right jamb), each one segment long, plus the
        // boundary reveals. It must bound what the builder emits.
        assert!(
            estimate.wall_quads >= 6,
            "a wall with one door must account for its slices and reveals, got {}",
            estimate.wall_quads
        );
        assert!(estimate.wall_quads <= 64, "estimate unexpectedly loose");
        // The estimate must bound the geometry that is actually generated.
        let mesh = crate::render::build_level_geometry(&level);
        assert!(
            u64::try_from(mesh.batches.wall_batch.count.max(0)).unwrap_or(0)
                <= estimate.wall_quads * 6
        );
        let expected_quads = estimate.floor_quads
            + estimate.ceiling_quads
            + estimate.wall_quads
            + estimate.light_quads
            + estimate.prop_quads
            + estimate.decal_quads;
        assert_eq!(estimate.total_vertices, expected_quads * 6);
    }

    #[test]
    fn test_collision_aabbs_for_z_axis_wall_follow_depth() {
        // A wall running along Z: the slice spans must follow depth, not width.
        let json = r#"{
            "format_version": 1,
            "id": "z_wall",
            "name": "Z Wall",
            "spawn": { "x": 5.0, "z": 5.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 },
            "walls": [{
                "x": 4.8, "z": 0.0, "width": 0.4, "depth": 10.0, "height": 3.5,
                "openings": [{ "kind": "door", "offset": 4.0, "width": 2.0, "height": 2.1 }]
            }]
        }"#;
        let level = LevelDef::from_json(json).expect("valid json");
        let aabbs = level.collision_aabbs();
        assert_eq!(aabbs.len(), 3);

        // The door header only spans the door's Z range, full thickness in X.
        let header = aabbs
            .iter()
            .find(|a| a.min_y > 2.0)
            .expect("door header slice");
        assert!((header.min_x - 4.8).abs() < 1e-4);
        assert!((header.max_x - 5.2).abs() < 1e-4);
        assert_eq!((header.min_z, header.max_z), (4.0, 6.0));
        assert!(!header.intersects_player_y(0.0));
    }

    #[test]
    fn test_collision_aabbs_include_solid_props_only() {
        let json = r#"{
            "format_version": 1,
            "id": "props",
            "name": "Props",
            "spawn": { "x": 0.0, "z": 0.0 },
            "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 3.5 },
            "props": [
                { "model": "core:crate", "x": 2.0, "y": 0.0, "z": 3.0, "size": [1.0, 1.0, 1.0], "solid": true },
                { "model": "core:rug", "x": 5.0, "z": 5.0, "solid": false }
            ]
        }"#;
        let level = LevelDef::from_json(json).expect("valid json");
        let aabbs = level.collision_aabbs();
        assert_eq!(aabbs.len(), 1);
        assert_exact(aabbs[0].min_x, 1.5);
        assert_exact(aabbs[0].max_x, 2.5);
        assert_exact(aabbs[0].min_y, 0.0);
        assert_exact(aabbs[0].max_y, 1.0);
        assert_exact(aabbs[0].min_z, 2.5);
        assert_exact(aabbs[0].max_z, 3.5);
    }

    // ------------------------------------------------- vertical geometry (4.0)

    #[test]
    fn test_legacy_room_gets_zero_elevation_flat_ceiling_and_the_new_default_height() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "legacy",
                "name": "Legacy",
                "spawn": { "x": 0.0, "z": 0.0 },
                "room": { "x": -6.0, "z": -6.0, "width": 12.0, "depth": 12.0 }
            }"#,
        )
        .expect("legacy json");
        let room = level.room_iter().next().expect("one room");
        assert_exact(room.height, DEFAULT_CEILING_HEIGHT_M);
        assert_exact(room.floor_y, 0.0);
        assert_eq!(room.ceiling, CeilingProfileDef::Flat);
        assert_exact(room.eave_y(), DEFAULT_CEILING_HEIGHT_M);
        assert_exact(room.ceiling_y_at(0.0, 0.0), DEFAULT_CEILING_HEIGHT_M);
    }

    #[test]
    fn test_room_elevation_moves_floor_and_ceiling_together() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "raised",
                "name": "Raised",
                "spawn": { "x": 0.0, "z": 0.0 },
                "rooms": [
                    { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0,
                      "height": 3.0, "floor_y": 2.0 },
                    { "x": 20.0, "z": 0.0, "width": 10.0, "depth": 10.0,
                      "height": 3.0, "floor_y": -1.0 }
                ]
            }"#,
        )
        .expect("vertical json");
        let surfaces = LevelSurfaces::new(&level);
        assert_eq!(surfaces.floor_y_at(5.0, 5.0), Some(2.0));
        assert_exact(surfaces.ceiling_y_at(5.0, 5.0), 5.0);
        assert_eq!(surfaces.floor_y_at(25.0, 5.0), Some(-1.0));
        assert_exact(surfaces.ceiling_y_at(25.0, 5.0), 2.0);
        // Outside every room the historical first-room fallback still applies.
        assert_eq!(surfaces.floor_y_at(100.0, 100.0), None);
        assert_exact(surfaces.ceiling_y_at(100.0, 100.0), 5.0);
    }

    #[test]
    fn test_gable_ceiling_interpolates_eave_to_ridge_on_both_axes() {
        let ridge_x = CeilingProfileDef::Gable {
            ridge: WallAxis::X,
            ridge_rise: 2.0,
        };
        let bounds = (0.0, 10.0, 0.0, 8.0);
        // Ridge runs along X, so the ceiling slopes across Z.
        assert_exact(
            ceiling_y_for_volume(bounds, 0.0, 3.0, ridge_x, 5.0, 0.0),
            3.0,
        );
        assert_exact(
            ceiling_y_for_volume(bounds, 0.0, 3.0, ridge_x, 5.0, 4.0),
            5.0,
        );
        assert_exact(
            ceiling_y_for_volume(bounds, 0.0, 3.0, ridge_x, 5.0, 2.0),
            4.0,
        );
        assert_exact(
            ceiling_y_for_volume(bounds, 0.0, 3.0, ridge_x, 0.0, 8.0),
            3.0,
        );

        let ridge_z = CeilingProfileDef::Gable {
            ridge: WallAxis::Z,
            ridge_rise: 1.5,
        };
        // Ridge runs along Z, so the ceiling slopes across X.
        assert_exact(
            ceiling_y_for_volume(bounds, 1.0, 3.0, ridge_z, 0.0, 4.0),
            4.0,
        );
        assert_exact(
            ceiling_y_for_volume(bounds, 1.0, 3.0, ridge_z, 5.0, 4.0),
            5.5,
        );
        assert_exact(
            ceiling_y_for_volume(bounds, 1.0, 3.0, ridge_z, 2.5, 4.0),
            4.75,
        );
    }

    #[test]
    fn test_malformed_gable_profiles_degrade_to_the_eave_plane() {
        let bounds = (0.0, 10.0, 0.0, 8.0);
        for rise in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let profile = CeilingProfileDef::Gable {
                ridge: WallAxis::X,
                ridge_rise: rise,
            };
            assert_exact(
                ceiling_y_for_volume(bounds, 0.0, 3.0, profile, 5.0, 4.0),
                3.0,
            );
        }
        // A degenerate footprint has no ridge to interpolate towards.
        assert_exact(
            ceiling_y_for_volume(
                (0.0, 10.0, 4.0, 4.0),
                0.0,
                3.0,
                CeilingProfileDef::Gable {
                    ridge: WallAxis::X,
                    ridge_rise: 2.0,
                },
                5.0,
                4.0,
            ),
            3.0,
        );
    }

    #[test]
    fn test_gable_room_helpers_report_the_ridge() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "gable",
                "name": "Gable",
                "spawn": { "x": 5.0, "z": 4.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 8.0,
                          "height": 3.0,
                          "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 2.0 } }
            }"#,
        )
        .expect("gable json");
        let room = level.room_iter().next().expect("one room");
        assert_eq!(room.ceiling.ridge_axis(), Some(WallAxis::X));
        assert_exact(room.ridge_across().expect("ridge line"), 4.0);
        assert_exact(room.ridge_y().expect("ridge height"), 5.0);
        assert_exact(room.ceiling_y_at(5.0, 4.0), 5.0);
    }

    #[test]
    fn test_floor_region_offsets_resolve_inside_outside_and_last_wins() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "regions",
                "name": "Regions",
                "spawn": { "x": 5.0, "z": 5.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 10.0, "depth": 10.0, "height": 4.0 },
                "floor_regions": [
                    { "x": 2.0, "z": 2.0, "width": 4.0, "depth": 4.0, "offset_y": -1.0 },
                    { "x": 3.0, "z": 3.0, "width": 1.0, "depth": 1.0, "offset_y": -2.0 }
                ]
            }"#,
        )
        .expect("region json");
        let surfaces = LevelSurfaces::new(&level);
        assert_eq!(surfaces.floor_y_at(0.5, 0.5), Some(0.0));
        assert_eq!(surfaces.floor_y_at(2.5, 2.5), Some(-1.0));
        // Overlapping regions: the later authored one wins.
        assert_eq!(surfaces.floor_y_at(3.5, 3.5), Some(-2.0));
        assert_eq!(
            surfaces
                .region_at(3.5, 3.5)
                .and_then(|r| r.material.as_deref()),
            None
        );
    }

    #[test]
    fn test_floor_grid_cuts_at_region_edges_and_carries_offsets() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "grid",
                "name": "Grid",
                "spawn": { "x": 5.0, "z": 5.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 4.0 },
                "floor_regions": [
                    { "x": 2.0, "z": 2.0, "width": 2.0, "depth": 2.0, "offset_y": -0.5 }
                ]
            }"#,
        )
        .expect("grid json");
        let surfaces = LevelSurfaces::new(&level);
        let room = level.room_iter().next().expect("one room");
        let grid = surfaces.floor_grid(room);
        assert!(grid.xs.contains(&2.0), "region edge is a cut line");
        assert!(grid.xs.contains(&4.0));
        for iz in 0..grid.cells_z() {
            for ix in 0..grid.cells_x() {
                let x = f32::midpoint(grid.xs[ix], grid.xs[ix + 1]);
                let z = f32::midpoint(grid.zs[iz], grid.zs[iz + 1]);
                let inside = (2.0..4.0).contains(&x) && (2.0..4.0).contains(&z);
                let expected = if inside { -0.5 } else { 0.0 };
                assert_exact(grid.offset_at(ix, iz), expected);
            }
        }
    }

    #[test]
    fn test_floor_region_rims_are_solid_only_for_unwalkable_steps() {
        let deep = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "deep",
                "name": "Deep",
                "spawn": { "x": 5.0, "z": 5.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 4.0 },
                "floor_regions": [
                    { "x": 2.0, "z": 2.0, "width": 2.0, "depth": 2.0, "offset_y": -1.2 }
                ]
            }"#,
        )
        .expect("deep json");
        let deep_surfaces = LevelSurfaces::new(&deep);
        let room = deep.room_iter().next().expect("one room");
        let mut rims = Vec::new();
        deep_surfaces
            .floor_grid(room)
            .push_region_rims(room, &mut rims);
        assert!(!rims.is_empty(), "a 1.2 m recess needs solid walls");
        for rim in &rims {
            assert_exact(rim.min_y, -1.2);
            assert_exact(rim.max_y, 0.0);
        }

        // A step the controller can walk is deliberately not a wall.
        let shallow = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "shallow",
                "name": "Shallow",
                "spawn": { "x": 5.0, "z": 5.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 4.0 },
                "floor_regions": [
                    { "x": 2.0, "z": 2.0, "width": 2.0, "depth": 2.0, "offset_y": -0.3 }
                ]
            }"#,
        )
        .expect("shallow json");
        let shallow_surfaces = LevelSurfaces::new(&shallow);
        let room = shallow.room_iter().next().expect("one room");
        let mut rims = Vec::new();
        shallow_surfaces
            .floor_grid(room)
            .push_region_rims(room, &mut rims);
        assert!(rims.is_empty(), "a walkable step must stay walkable");
    }

    #[test]
    fn test_walkable_floor_matches_the_surface_queries() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "walkable",
                "name": "Walkable",
                "spawn": { "x": 1.0, "z": 1.0 },
                "rooms": [
                    { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                      "height": 4.0, "floor_y": 2.0 },
                    { "x": 8.0, "z": 0.0, "width": 8.0, "depth": 8.0,
                      "height": 3.0, "floor_y": 0.0 }
                ],
                "floor_regions": [
                    { "x": 1.0, "z": 1.0, "width": 2.0, "depth": 2.0, "offset_y": -0.35 }
                ]
            }"#,
        )
        .expect("walkable json");
        let surfaces = LevelSurfaces::new(&level);
        let floor = WalkableFloor::from_level(&level);
        for x in [-1.0_f32, 1.5, 3.0, 7.9, 8.5, 12.0, 17.0] {
            for z in [-1.0_f32, 1.5, 5.0, 7.9, 12.0] {
                assert_eq!(
                    floor.height_at(x, z),
                    surfaces.floor_y_at(x, z),
                    "walkable floor disagrees at ({x}, {z})"
                );
            }
        }
        assert_eq!(floor.height_at(1.5, 1.5), Some(1.65));
        assert_eq!(floor.height_at(4.0, 4.0), Some(2.0));
        assert_eq!(floor.height_at(9.0, 4.0), Some(0.0));
        assert_eq!(floor.height_at(-5.0, -5.0), None);
    }

    #[test]
    fn test_estimate_geometry_accounts_for_regions_and_gables() {
        let plain = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "plain",
                "name": "Plain",
                "spawn": { "x": 5.0, "z": 5.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0 }
            }"#,
        )
        .expect("plain json");
        let plain_estimate = plain.estimate_geometry();

        let complex = LevelDef::from_json(
            r#"{
                "format_version": 1,
                "id": "complex",
                "name": "Complex",
                "spawn": { "x": 5.0, "z": 5.0 },
                "room": { "x": 0.0, "z": 0.0, "width": 20.0, "depth": 20.0, "height": 4.0,
                          "ceiling": { "kind": "gable", "ridge": "x", "ridge_rise": 2.0 } },
                "floor_regions": [
                    { "x": 4.0, "z": 4.0, "width": 4.0, "depth": 3.0, "offset_y": -1.5 }
                ]
            }"#,
        )
        .expect("complex json");
        let complex_estimate = complex.estimate_geometry();

        assert!(
            complex_estimate.floor_quads > plain_estimate.floor_quads,
            "a recess adds transition faces and cut lines"
        );
        assert!(
            complex_estimate.ceiling_quads >= plain_estimate.ceiling_quads,
            "the ridge cut line never lowers the ceiling cell count"
        );
        // The estimate remains an upper bound on what a build emits.
        let mesh = crate::render::build_level_geometry(&complex);
        assert!(
            u64::try_from(mesh.vertex_count).unwrap_or(u64::MAX) <= complex_estimate.total_vertices
        );
    }
}
