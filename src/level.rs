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

/// A level's reference to one surface material: the material id plus an
/// optional per-surface `shine` override.
///
/// The id keeps its meaning from the material definition (its texture, tint,
/// sheen colour and reflection behaviour); the override only changes how
/// glossy *this* surface is, so a level can lay matte institutional linoleum
/// without a second catalog material. `None` keeps the material's own default.
///
/// Authoring is a sibling key in the level JSON:
///
/// ```json
/// { "material": "core:linoleum_polished_01", "shine": 0.05 }
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MaterialRef<'a> {
    /// Material id exactly as the level wrote it.
    pub id: &'a str,
    /// Author-facing per-surface shine, `0.0..=1.0`; `None` keeps the
    /// material's default.
    pub shine: Option<f32>,
}

impl<'a> MaterialRef<'a> {
    /// A reference that keeps the material's default shine.
    #[must_use]
    pub const fn id(id: &'a str) -> Self {
        Self { id, shine: None }
    }

    /// A reference with an optional per-surface shine override.
    #[must_use]
    pub const fn with_shine(id: &'a str, shine: Option<f32>) -> Self {
        Self { id, shine }
    }

    /// True when a shine value has been authored for this surface.
    #[must_use]
    pub const fn has_shine(&self) -> bool {
        self.shine.is_some()
    }
}

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
    /// Per-surface shine override for [`Self::material`], `0.0..=1.0`.
    /// Omitted keeps the material's default.
    #[serde(default)]
    pub shine: Option<f32>,
    /// Ceiling material id for this room. Falls back to `defaults.ceiling`.
    #[serde(default)]
    pub ceiling_material: Option<String>,
    /// Per-surface shine override for [`Self::ceiling_material`].
    #[serde(default)]
    pub ceiling_shine: Option<f32>,
}

impl RoomDef {
    /// This room's floor material reference, if it overrides the level default.
    #[must_use]
    pub fn floor_ref(&self) -> Option<MaterialRef<'_>> {
        self.material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.shine))
    }

    /// This room's ceiling material reference, if it overrides the default.
    #[must_use]
    pub fn ceiling_ref(&self) -> Option<MaterialRef<'_>> {
        self.ceiling_material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.ceiling_shine))
    }

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
    /// Per-surface shine override for [`Self::material`].
    #[serde(default)]
    pub shine: Option<f32>,
    /// Material for the vertical transition faces around the region; falls back
    /// to the room's wall material.
    #[serde(default)]
    pub edge_material: Option<String>,
    /// Per-surface shine override for [`Self::edge_material`].
    #[serde(default)]
    pub edge_shine: Option<f32>,
}

impl FloorRegionDef {
    /// The region's floor material reference, if it overrides the room's floor.
    #[must_use]
    pub fn floor_ref(&self) -> Option<MaterialRef<'_>> {
        self.material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.shine))
    }

    /// The region's transition-face material reference, if authored.
    #[must_use]
    pub fn edge_ref(&self) -> Option<MaterialRef<'_>> {
        self.edge_material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.edge_shine))
    }

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

/// Steepest walkable ramp slope, as rise per metre of run.
///
/// The player controller moves in sub-steps of at most half a player radius and
/// refuses any step taller than [`PLAYER_STEP_HEIGHT`]. A slope above this
/// limit would make the controller stall part-way up, so the loader rejects it
/// with a named error instead of shipping a ramp that cannot be climbed.
pub const MAX_RAMP_SLOPE: f32 = 2.0;

/// Hard ceiling on a ramp's or staircase's total rise, in metres.
pub const MAX_RAMP_RISE_M: f32 = 50.0;

/// Tallest riser a staircase may author, in metres.
///
/// A staircase is walked by the same step rule as a chain of floor regions: a
/// riser above [`PLAYER_STEP_HEIGHT`] would refuse the player instead of
/// letting them climb. The loader rejects anything above this plus a
/// millimetre of float tolerance, and the controller accepts anything within
/// this plus `STEP_EPS`: the accepted set is therefore always traversable,
/// including a riser of exactly `0.4 m` resolved at a non-zero floor height
/// (where the two floats can differ by a few ulps).
pub const MAX_STAIR_RISER_M: f32 = PLAYER_STEP_HEIGHT;

/// Shallowest staircase tread the loader accepts, in metres.
///
/// A tread shorter than a foot is not a step; it is a malformed staircase, and
/// it would also make the walkable sampler staircase-shaped at a finer scale
/// than the controller's sub-step.
pub const MIN_STAIR_TREAD_M: f32 = 0.15;

/// A straight sloped floor surface: the level's ramp primitive.
///
/// A ramp is a rectangle in plan whose walking surface rises (or falls)
/// **linearly** along its length axis. It is a floor surface, not a prop: the
/// player walks up and down it at a continuous height, collision answers with
/// the slope, and the renderer draws it as a real surface with the level's own
/// materials.
///
/// * `offset_y` is the surface offset at the ramp's **low** end, relative to
///   the containing room's `floor_y` (exactly like a [`FloorRegionDef`]).
/// * `rise` is the signed height change from the low end to the far end:
///   positive climbs toward the far end of the length axis, negative descends
///   toward it. Both are relative to the room floor.
///
/// A ramp is only ever a *walking* surface: the space underneath it is not
/// walkable (the walkable floor at a point is the ramp's own height), and its
/// ends are meant to meet the floors they connect: the low end usually meets
/// the room floor or a floor region, and the high end a raised platform of the
/// same height. A floor region may not overlap a ramp's footprint; the two
/// would draw two floors through the same space.
///
/// `material` overrides the ramp's top surface; `edge_material` overrides the
/// vertical side faces (both fall back to the room's floor/wall material).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RampDef {
    pub x: f32,
    pub z: f32,
    pub width: f32,
    pub depth: f32,
    /// Surface offset at the low end, relative to the room's `floor_y`.
    #[serde(default)]
    pub offset_y: f32,
    /// Signed height change to the far end along the length axis, in metres.
    pub rise: f32,
    /// Top-surface material id; falls back to the room's floor material.
    #[serde(default)]
    pub material: Option<String>,
    /// Per-surface shine override for [`Self::material`].
    #[serde(default)]
    pub shine: Option<f32>,
    /// Material for the ramp's side faces; falls back to the room's wall
    /// material.
    #[serde(default)]
    pub edge_material: Option<String>,
    /// Per-surface shine override for [`Self::edge_material`].
    #[serde(default)]
    pub edge_shine: Option<f32>,
}

/// A finite value, or `0.0` when non-finite.
///
/// The surface value types collapse the height fields the way the authoring
/// types' getters always have, so a malformed level renders and walks the same
/// way on every path instead of one path sanitising and another not.
const fn sanitized(value: f32) -> f32 {
    if value.is_finite() { value } else { 0.0 }
}

/// The walking surface of a ramp, detached from its authored definition.
///
/// [`RampDef`] and the player's [`WalkableFloor`] both resolve their heights
/// through this one value, so the sloped surface the geometry draws and the
/// surface the controller stands on cannot drift apart: they are the same
/// arithmetic over the same fields. The controller outlives the level's
/// `RampDef`, which is why this is a small owned value rather than a borrowed
/// view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RampSurface {
    x: f32,
    z: f32,
    width: f32,
    depth: f32,
    offset_y: f32,
    rise: f32,
}

impl RampSurface {
    /// The surface of one authored ramp.
    #[must_use]
    pub const fn new(ramp: &RampDef) -> Self {
        Self {
            x: ramp.x,
            z: ramp.z,
            width: ramp.width,
            depth: ramp.depth,
            offset_y: sanitized(ramp.offset_y),
            rise: sanitized(ramp.rise),
        }
    }

    /// Ramp footprint as `(x0, x1, z0, z1)`, normalised.
    #[must_use]
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        (
            self.x.min(self.x + self.width),
            self.x.max(self.x + self.width),
            self.z.min(self.z + self.depth),
            self.z.max(self.z + self.depth),
        )
    }

    /// True when `(x, z)` lies inside the ramp footprint.
    #[must_use]
    pub fn contains(&self, x: f32, z: f32) -> bool {
        if !x.is_finite() || !z.is_finite() {
            return false;
        }
        let (x0, x1, z0, z1) = self.bounds();
        x >= x0 && x <= x1 && z >= z0 && z <= z1
    }

    /// The axis the run follows: the longer of width/depth.
    #[must_use]
    pub fn axis(&self) -> WallAxis {
        WallAxis::of(self.width.abs(), self.depth.abs())
    }

    /// Length of the run along [`Self::axis`], in metres.
    #[must_use]
    pub fn length(&self) -> f32 {
        match self.axis() {
            WallAxis::X => self.width.abs(),
            WallAxis::Z => self.depth.abs(),
        }
    }

    /// Signed rise, sanitised to `0.0` for non-finite values.
    #[must_use]
    pub const fn rise(&self) -> f32 {
        self.rise
    }

    /// Base offset at the run's low end, sanitised.
    #[must_use]
    pub const fn base_offset(&self) -> f32 {
        self.offset_y
    }

    /// Fraction of the way along the run at `(x, z)`, clamped to `0.0..=1.0`.
    #[must_use]
    pub fn fraction_at(&self, x: f32, z: f32) -> f32 {
        let length = self.length();
        if !length.is_finite() || length <= 0.0 {
            return 0.0;
        }
        let (x0, _x1, z0, _z1) = self.bounds();
        let along = match self.axis() {
            WallAxis::X => x - x0,
            WallAxis::Z => z - z0,
        };
        (along / length).clamp(0.0, 1.0)
    }

    /// Vertical offset of the walking surface at `(x, z)`, relative to the
    /// containing room's floor.
    #[must_use]
    pub fn offset_at(&self, x: f32, z: f32) -> f32 {
        self.rise.mul_add(self.fraction_at(x, z), self.offset_y)
    }

    /// Vertical offset at the run's low end (the lower of the two ends).
    #[must_use]
    pub fn low_offset(&self) -> f32 {
        self.offset_y + self.rise.min(0.0)
    }

    /// Vertical offset at the run's high end (the higher of the two ends).
    #[must_use]
    pub fn high_offset(&self) -> f32 {
        self.offset_y + self.rise.max(0.0)
    }

    /// World `(x, z)` of the end at the high (`true`) or low (`false`) side of
    /// the run.
    #[must_use]
    pub fn end_point(&self, high: bool) -> (f32, f32) {
        let (x0, x1, z0, z1) = self.bounds();
        // The far end of the run is the high end for a positive rise, and the
        // low end for a negative one.
        let far = high == (self.rise >= 0.0);
        match self.axis() {
            WallAxis::X => (if far { x1 } else { x0 }, f32::midpoint(z0, z1)),
            WallAxis::Z => (f32::midpoint(x0, x1), if far { z1 } else { z0 }),
        }
    }

    /// A world point just outside one side of the ramp, at run fraction
    /// `fraction`, used to sample the floor the ramp's side faces meet.
    ///
    /// `side` is `-1.0` for the low-coordinate side (north/west) and `1.0` for
    /// the high-coordinate side.
    #[must_use]
    pub fn side_probe(&self, side: f32, fraction: f32, probe: f32) -> (f32, f32) {
        let (x0, x1, z0, z1) = self.bounds();
        let fraction = fraction.clamp(0.0, 1.0);
        match self.axis() {
            WallAxis::X => (
                fraction.mul_add(x1 - x0, x0),
                if side < 0.0 { z0 - probe } else { z1 + probe },
            ),
            WallAxis::Z => (
                if side < 0.0 { x0 - probe } else { x1 + probe },
                fraction.mul_add(z1 - z0, z0),
            ),
        }
    }
}

impl RampDef {
    /// The ramp's walking surface as a detached value; the single definition
    /// the renderer, the collision rims and the controller all resolve.
    #[must_use]
    pub const fn surface(&self) -> RampSurface {
        RampSurface::new(self)
    }

    /// The ramp's top material reference, if it overrides the room's floor.
    #[must_use]
    pub fn floor_ref(&self) -> Option<MaterialRef<'_>> {
        self.material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.shine))
    }

    /// The ramp's side-face material reference, if authored.
    #[must_use]
    pub fn edge_ref(&self) -> Option<MaterialRef<'_>> {
        self.edge_material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.edge_shine))
    }

    /// Ramp footprint as `(x0, x1, z0, z1)`, normalised.
    #[must_use]
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        self.surface().bounds()
    }

    /// True when `(x, z)` lies inside the ramp footprint.
    #[must_use]
    pub fn contains(&self, x: f32, z: f32) -> bool {
        self.surface().contains(x, z)
    }

    /// The axis the ramp's run follows: the longer of width/depth.
    #[must_use]
    pub fn axis(&self) -> WallAxis {
        self.surface().axis()
    }

    /// Length of the run along [`Self::axis`], in metres.
    #[must_use]
    pub fn length(&self) -> f32 {
        self.surface().length()
    }

    /// Fraction of the way along the run at `(x, z)`, clamped to `0.0..=1.0`.
    #[must_use]
    pub fn fraction_at(&self, x: f32, z: f32) -> f32 {
        self.surface().fraction_at(x, z)
    }

    /// Signed rise, sanitised to `0.0` for non-finite values.
    #[must_use]
    pub const fn rise(&self) -> f32 {
        self.surface().rise()
    }

    /// Base offset at the ramp's low end, sanitised.
    #[must_use]
    pub const fn base_offset(&self) -> f32 {
        self.surface().base_offset()
    }

    /// Vertical offset of the walking surface at `(x, z)`, relative to the
    /// containing room's floor.
    #[must_use]
    pub fn offset_at(&self, x: f32, z: f32) -> f32 {
        self.surface().offset_at(x, z)
    }

    /// Vertical offset at the ramp's low end (the lower of the two ends).
    #[must_use]
    pub fn low_offset(&self) -> f32 {
        self.surface().low_offset()
    }

    /// Vertical offset at the ramp's high end (the higher of the two ends).
    #[must_use]
    pub fn high_offset(&self) -> f32 {
        self.surface().high_offset()
    }

    /// World `(x, z)` of the end at the high (`true`) or low (`false`) side of
    /// the run.
    #[must_use]
    pub fn end_point(&self, high: bool) -> (f32, f32) {
        self.surface().end_point(high)
    }

    /// A world point just outside one side of the ramp, at run fraction
    /// `fraction`, used to sample the floor the ramp's side faces meet.
    #[must_use]
    pub fn side_probe(&self, side: f32, fraction: f32, probe: f32) -> (f32, f32) {
        self.surface().side_probe(side, fraction, probe)
    }
}

/// A straight residential staircase: the level's stepped floor primitive.
///
/// A staircase is a rectangle in plan whose walking surface climbs in equal
/// steps along its length axis. It is drawn as real treads, risers and closed
/// sides, and collision resolves the same stepped heights, so the player walks
/// it one step at a time with the ordinary 0.4 m walkable step rule.
///
/// * `offset_y` is the walking-surface offset at the **foot** of the flight
///   (the first riser's base), relative to the containing room's `floor_y`.
/// * `rise` is the total height climbed over `steps` risers, so the riser
///   height is `rise / steps` and the tread depth is `length / steps`.
/// * `steps` counts risers *and* treads: a flight of 16 steps climbs 16 risers
///   and stands on 16 treads, the last of which is level with the far floor.
///
/// Tread material is `material`, risers use `riser_material` (falling back to
/// the tread material) and the closed stringer sides use `side_material`
/// (falling back to the riser material).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StairDef {
    pub x: f32,
    pub z: f32,
    pub width: f32,
    pub depth: f32,
    /// Walking-surface offset at the foot, relative to the room's `floor_y`.
    #[serde(default)]
    pub offset_y: f32,
    /// Total rise from the foot to the top tread, in metres; must be positive.
    pub rise: f32,
    /// Number of risers and treads; at least 2.
    pub steps: u32,
    /// Tread material id; falls back to the room's floor material.
    #[serde(default)]
    pub material: Option<String>,
    /// Per-surface shine override for [`Self::material`].
    #[serde(default)]
    pub shine: Option<f32>,
    /// Riser material id; falls back to [`Self::material`].
    #[serde(default)]
    pub riser_material: Option<String>,
    /// Per-surface shine override for [`Self::riser_material`].
    #[serde(default)]
    pub riser_shine: Option<f32>,
    /// Closed side material id; falls back to [`Self::riser_material`].
    #[serde(default)]
    pub side_material: Option<String>,
    /// Per-surface shine override for [`Self::side_material`].
    #[serde(default)]
    pub side_shine: Option<f32>,
}

/// The walking surface of a straight staircase, detached from its authored
/// definition.
///
/// [`StairDef`] and the player's [`WalkableFloor`] both resolve their stepped
/// heights through this one value, so the treads the geometry draws and the
/// treads the controller stands on cannot drift apart. The earlier walkable
/// model re-derived the run from `bounds()` (`z1 - z0`) while the renderer used
/// the authored `depth`; a one-ulp difference flipped the last step boundary
/// and left the player standing a whole riser above the drawn tread.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StairSurface {
    x: f32,
    z: f32,
    width: f32,
    depth: f32,
    offset_y: f32,
    rise: f32,
    steps: u32,
}

impl StairSurface {
    /// The surface of one authored staircase.
    #[must_use]
    pub const fn new(stair: &StairDef) -> Self {
        Self {
            x: stair.x,
            z: stair.z,
            width: stair.width,
            depth: stair.depth,
            offset_y: sanitized(stair.offset_y),
            rise: sanitized(stair.rise),
            steps: stair.steps,
        }
    }

    /// Stair footprint as `(x0, x1, z0, z1)`, normalised.
    #[must_use]
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        (
            self.x.min(self.x + self.width),
            self.x.max(self.x + self.width),
            self.z.min(self.z + self.depth),
            self.z.max(self.z + self.depth),
        )
    }

    /// True when `(x, z)` lies inside the stair footprint.
    #[must_use]
    pub fn contains(&self, x: f32, z: f32) -> bool {
        if !x.is_finite() || !z.is_finite() {
            return false;
        }
        let (x0, x1, z0, z1) = self.bounds();
        x >= x0 && x <= x1 && z >= z0 && z <= z1
    }

    /// The axis the flight climbs along: the longer of width/depth.
    #[must_use]
    pub fn axis(&self) -> WallAxis {
        WallAxis::of(self.width.abs(), self.depth.abs())
    }

    /// Run of the flight along [`Self::axis`], in metres.
    #[must_use]
    pub fn length(&self) -> f32 {
        match self.axis() {
            WallAxis::X => self.width.abs(),
            WallAxis::Z => self.depth.abs(),
        }
    }

    /// Number of steps.
    #[must_use]
    pub const fn step_count(&self) -> u32 {
        self.steps
    }

    /// Total rise, sanitised to `0.0` for non-finite values.
    #[must_use]
    pub const fn rise(&self) -> f32 {
        self.rise
    }

    /// Walking-surface offset at the foot, sanitised.
    #[must_use]
    pub const fn base_offset(&self) -> f32 {
        self.offset_y
    }

    /// Run fraction at `(x, z)`, clamped to `0.0..=1.0`.
    #[must_use]
    pub fn fraction_at(&self, x: f32, z: f32) -> f32 {
        let length = self.length();
        if !length.is_finite() || length <= 0.0 {
            return 0.0;
        }
        let (x0, _x1, z0, _z1) = self.bounds();
        let along = match self.axis() {
            WallAxis::X => x - x0,
            WallAxis::Z => z - z0,
        };
        (along / length).clamp(0.0, 1.0)
    }

    /// Index of the tread carrying run fraction `fraction`, `0..steps`.
    #[must_use]
    pub fn step_index(&self, fraction: f32) -> u32 {
        if self.steps == 0 {
            return 0;
        }
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        // The clamped value is far inside u32's range and the count is bounded.
        let index = (fraction.clamp(0.0, 1.0) * self.steps as f32).floor() as u32;
        index.min(self.steps.saturating_sub(1))
    }

    /// Height of one riser, in metres (zero when malformed).
    #[must_use]
    pub fn riser_height(&self) -> f32 {
        if self.steps == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)] // step counts are bounded by validation
        let count = self.steps as f32;
        self.rise / count
    }

    /// Depth of one tread, in metres (zero when malformed).
    #[must_use]
    pub fn tread_depth(&self) -> f32 {
        if self.steps == 0 {
            return 0.0;
        }
        #[allow(clippy::cast_precision_loss)] // step counts are bounded by validation
        let count = self.steps as f32;
        self.length() / count
    }

    /// Vertical offset of the walking surface at `(x, z)`, relative to the
    /// containing room's floor.
    ///
    /// The first tread stands one riser above the foot, so walking onto the
    /// flight from the room floor is one ordinary step.
    #[must_use]
    pub fn offset_at(&self, x: f32, z: f32) -> f32 {
        let step = self.step_index(self.fraction_at(x, z));
        #[allow(clippy::cast_precision_loss)] // step counts are bounded by validation
        let risers = (step.saturating_add(1)) as f32;
        self.riser_height().mul_add(risers, self.offset_y)
    }

    /// Vertical offset of the top tread (level with the far floor).
    #[must_use]
    pub fn top_offset(&self) -> f32 {
        #[allow(clippy::cast_precision_loss)] // step counts are bounded by validation
        let count = self.steps as f32;
        self.riser_height().mul_add(count, self.offset_y)
    }

    /// Run span `[start, end]` of tread `index` as world coordinates along the
    /// length axis.
    #[must_use]
    pub fn tread_span(&self, index: u32) -> (f32, f32) {
        let (x0, _x1, z0, _z1) = self.bounds();
        let origin = match self.axis() {
            WallAxis::X => x0,
            WallAxis::Z => z0,
        };
        #[allow(clippy::cast_precision_loss)] // step counts are bounded by validation
        let start = self.tread_depth().mul_add(index as f32, origin);
        (start, start + self.tread_depth())
    }

    /// A world point just outside one side of the flight, at run fraction
    /// `fraction`, used to sample the floor the closed sides meet.
    #[must_use]
    pub fn side_probe(&self, side: f32, fraction: f32, probe: f32) -> (f32, f32) {
        let (x0, x1, z0, z1) = self.bounds();
        let fraction = fraction.clamp(0.0, 1.0);
        match self.axis() {
            WallAxis::X => (
                fraction.mul_add(x1 - x0, x0),
                if side < 0.0 { z0 - probe } else { z1 + probe },
            ),
            WallAxis::Z => (
                if side < 0.0 { x0 - probe } else { x1 + probe },
                fraction.mul_add(z1 - z0, z0),
            ),
        }
    }
}

impl StairDef {
    /// The tread material reference, if it overrides the room's floor.
    #[must_use]
    pub fn tread_ref(&self) -> Option<MaterialRef<'_>> {
        self.material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.shine))
    }

    /// The riser material reference: its own, else the tread's.
    #[must_use]
    pub fn riser_ref(&self) -> Option<MaterialRef<'_>> {
        self.riser_material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.riser_shine))
            .or_else(|| self.tread_ref())
    }

    /// The closed-side material reference: its own, else the riser's, else the
    /// tread's.
    #[must_use]
    pub fn side_ref(&self) -> Option<MaterialRef<'_>> {
        self.side_material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.side_shine))
            .or_else(|| self.riser_ref())
    }

    /// The staircase's walking surface as a detached value; the single
    /// definition the renderer, the collision rims and the controller resolve.
    #[must_use]
    pub const fn surface(&self) -> StairSurface {
        StairSurface::new(self)
    }

    /// Stair footprint as `(x0, x1, z0, z1)`, normalised.
    #[must_use]
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        self.surface().bounds()
    }

    /// True when `(x, z)` lies inside the stair footprint.
    #[must_use]
    pub fn contains(&self, x: f32, z: f32) -> bool {
        self.surface().contains(x, z)
    }

    /// The axis the flight climbs along: the longer of width/depth.
    #[must_use]
    pub fn axis(&self) -> WallAxis {
        self.surface().axis()
    }

    /// Run of the flight along [`Self::axis`], in metres.
    #[must_use]
    pub fn length(&self) -> f32 {
        self.surface().length()
    }

    /// Number of steps, sanitised to zero when malformed.
    #[must_use]
    pub const fn step_count(&self) -> u32 {
        self.surface().step_count()
    }

    /// Total rise, sanitised to `0.0` for non-finite values.
    #[must_use]
    pub const fn rise(&self) -> f32 {
        self.surface().rise()
    }

    /// Walking-surface offset at the foot, sanitised.
    #[must_use]
    pub const fn base_offset(&self) -> f32 {
        self.surface().base_offset()
    }

    /// Height of one riser, in metres (zero when malformed).
    #[must_use]
    pub fn riser_height(&self) -> f32 {
        self.surface().riser_height()
    }

    /// Depth of one tread, in metres (zero when malformed).
    #[must_use]
    pub fn tread_depth(&self) -> f32 {
        self.surface().tread_depth()
    }

    /// Run fraction at `(x, z)`, clamped to `0.0..=1.0`.
    #[must_use]
    pub fn fraction_at(&self, x: f32, z: f32) -> f32 {
        self.surface().fraction_at(x, z)
    }

    /// Index of the tread carrying run fraction `fraction`, `0..steps`.
    #[must_use]
    pub fn step_index(&self, fraction: f32) -> u32 {
        self.surface().step_index(fraction)
    }

    /// Vertical offset of the walking surface at `(x, z)`, relative to the
    /// containing room's floor.
    ///
    /// The first tread stands one riser above the foot, so walking onto the
    /// flight from the room floor is one ordinary step.
    #[must_use]
    pub fn offset_at(&self, x: f32, z: f32) -> f32 {
        self.surface().offset_at(x, z)
    }

    /// Vertical offset of the top tread (level with the far floor).
    #[must_use]
    pub fn top_offset(&self) -> f32 {
        self.surface().top_offset()
    }

    /// Run span `[start, end]` of tread `index` as world coordinates along the
    /// length axis.
    #[must_use]
    pub fn tread_span(&self, index: u32) -> (f32, f32) {
        self.surface().tread_span(index)
    }

    /// A world point just outside one side of the flight, at run fraction
    /// `fraction`, used to sample the floor the closed sides meet.
    #[must_use]
    pub fn side_probe(&self, side: f32, fraction: f32, probe: f32) -> (f32, f32) {
        self.surface().side_probe(side, fraction, probe)
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
///
/// Each id has a matching `*_shine` override so a level can make every default
/// floor matte (or every default wall slightly satin) in one place; an omitted
/// shine keeps the material's own default.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelDefaults {
    #[serde(default)]
    pub wall: String,
    #[serde(default)]
    pub floor: String,
    #[serde(default)]
    pub ceiling: String,
    /// Per-surface shine override for [`Self::wall`].
    #[serde(default)]
    pub wall_shine: Option<f32>,
    /// Per-surface shine override for [`Self::floor`].
    #[serde(default)]
    pub floor_shine: Option<f32>,
    /// Per-surface shine override for [`Self::ceiling`].
    #[serde(default)]
    pub ceiling_shine: Option<f32>,
}

impl LevelDefaults {
    /// The default wall material reference.
    #[must_use]
    pub fn wall_ref(&self) -> MaterialRef<'_> {
        MaterialRef::with_shine(&self.wall, self.wall_shine)
    }

    /// The default floor material reference.
    #[must_use]
    pub fn floor_ref(&self) -> MaterialRef<'_> {
        MaterialRef::with_shine(&self.floor, self.floor_shine)
    }

    /// The default ceiling material reference.
    #[must_use]
    pub fn ceiling_ref(&self) -> MaterialRef<'_> {
        MaterialRef::with_shine(&self.ceiling, self.ceiling_shine)
    }
}

impl Default for LevelDefaults {
    fn default() -> Self {
        Self {
            wall: "core:wallpaper_yellow_01".into(),
            floor: "core:carpet_beige_01".into(),
            ceiling: "core:ceiling_panel_01".into(),
            wall_shine: None,
            floor_shine: None,
            ceiling_shine: None,
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
    /// Per-surface shine override for [`Self::material`] and for the faces that
    /// do not author their own in [`Self::face_shine`].
    #[serde(default)]
    pub shine: Option<f32>,
    /// Per-face shine overrides, keyed exactly like [`Self::faces`]. A face
    /// keeps [`Self::shine`], then the material's default, when it authors
    /// none.
    #[serde(default)]
    pub face_shine: HashMap<String, f32>,
    /// Rectangular cutouts (doors, windows, passages, vents) through this wall.
    #[serde(default)]
    pub openings: Vec<WallOpeningDef>,
}

impl WallDef {
    /// The wall's own length-face material reference, if it overrides
    /// `defaults.wall`.
    #[must_use]
    pub fn material_ref(&self) -> Option<MaterialRef<'_>> {
        self.material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.shine))
    }

    /// The material reference for one named length face.
    ///
    /// `faces` wins over the wall's own [`Self::material`], exactly as before.
    /// Shine resolves independently: the face's own `face_shine` wins; else the
    /// wall's `shine` applies to every face that draws the wall's own material
    /// (or falls back to the level default); a face with a different material
    /// keeps that material's default.
    #[must_use]
    pub fn face_ref(&self, name: &str) -> Option<MaterialRef<'_>> {
        let face_material = self.faces.get(name).map(String::as_str);
        let id = face_material.or(self.material.as_deref())?;
        let shine = match self.face_shine.get(name) {
            Some(value) => Some(*value),
            // A face with no override of its own — or one that names the same
            // material — keeps the wall's own shine; a face with a different
            // material keeps that material's default.
            None if face_material.is_none() || face_material == self.material.as_deref() => {
                self.shine
            }
            None => None,
        };
        Some(MaterialRef::with_shine(id, shine))
    }

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
    /// Per-surface shine override for [`Self::glass`]'s material.
    #[serde(default)]
    pub glass_shine: Option<f32>,
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

    /// The pane's material reference, with its optional shine override.
    #[must_use]
    pub fn glass_ref(&self) -> Option<MaterialRef<'_>> {
        self.glass_material()
            .map(|id| MaterialRef::with_shine(id, self.glass_shine))
    }
}

// ---------------------------------------------------------------------------
// Generic architectural pieces
// ---------------------------------------------------------------------------
//
// Half walls, columns, archways, guardrails, thresholds and baseboards are
// theme-independent architecture: each one is a small solid or trim piece whose
// surfaces are ordinary material ids, so a level draws it with whatever
// materials it already uses. They carry no built-in textures of their own.

/// One solid axis-aligned architectural box, in world coordinates.
///
/// The shared shape behind the pieces that physically exist (half walls,
/// columns, archway piers and headers, guardrails): collision turns them into
/// [`WallAabb`]s and the lighting bake turns them into blockers, so a piece
/// that is drawn is also solid and also occludes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArchitectureBox {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl ArchitectureBox {
    /// A box from two corners, normalised so `min` is the low corner.
    #[must_use]
    pub fn from_corners(a: [f32; 3], b: [f32; 3]) -> Option<Self> {
        if !a.iter().chain(b.iter()).all(|value| value.is_finite()) {
            return None;
        }
        let min = [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])];
        let max = [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])];
        if max[0] <= min[0] || max[1] <= min[1] || max[2] <= min[2] {
            return None;
        }
        Some(Self { min, max })
    }

    /// The box as a collision box.
    #[must_use]
    pub fn to_wall_aabb(&self) -> WallAabb {
        WallAabb::with_y(
            self.min[0],
            self.min[1],
            self.min[2],
            self.max[0] - self.min[0],
            self.max[1] - self.min[1],
            self.max[2] - self.min[2],
        )
    }
}

/// Default height of a guardrail's top rail above its base line, in metres.
pub const GUARDRAIL_DEFAULT_HEIGHT_M: f32 = 1.0;
/// Default spacing between guardrail posts, in metres.
pub const GUARDRAIL_DEFAULT_POST_SPACING_M: f32 = 1.2;
/// Vertical size of a guardrail's top rail, in metres.
pub const GUARDRAIL_RAIL_THICKNESS_M: f32 = 0.045;
/// Across-the-run width of a guardrail's rails, in metres.
pub const GUARDRAIL_RAIL_WIDTH_M: f32 = 0.07;
/// Height of the guardrail's lower rail's top edge above its base line, in m.
pub const GUARDRAIL_MIDRAIL_TOP_M: f32 = 0.33;
/// Vertical size of a guardrail's lower rail, in metres.
pub const GUARDRAIL_MIDRAIL_THICKNESS_M: f32 = 0.03;
/// Square section of a guardrail post, in metres.
///
/// Deliberately slimmer than [`GUARDRAIL_RAIL_WIDTH_M`] so a post's sides never
/// lie in the same plane as the rail they carry.
pub const GUARDRAIL_POST_SIZE_M: f32 = 0.06;

/// Default height of a baseboard above its base line, in metres.
pub const BASEBOARD_DEFAULT_HEIGHT_M: f32 = 0.09;
/// Default thickness (how far a baseboard stands proud of the wall), in metres.
pub const BASEBOARD_DEFAULT_THICKNESS_M: f32 = 0.018;
/// Default height of a threshold strip above the floor, in metres.
pub const THRESHOLD_DEFAULT_HEIGHT_M: f32 = 0.012;
/// Default width of a threshold strip across the doorway, in metres.
pub const THRESHOLD_DEFAULT_THICKNESS_M: f32 = 0.06;

/// A reusable half-height wall: a solid rectangular knee wall with its own
/// length-face, end and cap materials.
///
/// The footprint is placed by its **minimum corner** exactly like a wall, and
/// `height` is authored (a half wall without a height has no meaning). The
/// piece is a real solid: it blocks the player, occludes baked light and draws
/// a capped top, which is what makes it usable as a partition, a kitchen
/// division, a stair-landing parapet or a planter edge. It is deliberately
/// theme-independent: give it a Home wallpaper, an office panel or an
/// industrial metal by naming the material.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HalfWallDef {
    pub x: f32,
    pub z: f32,
    pub width: f32,
    pub depth: f32,
    /// Height above the piece's base, in metres.
    pub height: f32,
    /// Absolute world Y of the base. Omitted means the walkable floor under the
    /// footprint's centre.
    #[serde(default)]
    pub y: Option<f32>,
    /// Length-face material id; falls back to `defaults.wall`.
    #[serde(default)]
    pub material: Option<String>,
    /// Per-surface shine override for [`Self::material`].
    #[serde(default)]
    pub shine: Option<f32>,
    /// The two short end faces' material id; falls back to [`Self::material`].
    #[serde(default)]
    pub end_material: Option<String>,
    /// Per-surface shine override for [`Self::end_material`].
    #[serde(default)]
    pub end_shine: Option<f32>,
    /// Top cap material id; falls back to [`Self::material`].
    #[serde(default)]
    pub cap_material: Option<String>,
    /// Per-surface shine override for [`Self::cap_material`].
    #[serde(default)]
    pub cap_shine: Option<f32>,
}

impl HalfWallDef {
    /// Footprint `(x0, x1, z0, z1)`, normalised.
    #[must_use]
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        (
            self.x.min(self.x + self.width),
            self.x.max(self.x + self.width),
            self.z.min(self.z + self.depth),
            self.z.max(self.z + self.depth),
        )
    }

    /// The axis the piece's length runs along: the longer of width/depth.
    #[must_use]
    pub fn axis(&self) -> WallAxis {
        WallAxis::of(self.width.abs(), self.depth.abs())
    }

    /// Length along [`Self::axis`], in metres.
    #[must_use]
    pub fn length(&self) -> f32 {
        match self.axis() {
            WallAxis::X => self.width.abs(),
            WallAxis::Z => self.depth.abs(),
        }
    }

    /// Thickness across the length axis, in metres.
    #[must_use]
    pub fn thickness(&self) -> f32 {
        match self.axis() {
            WallAxis::X => self.depth.abs(),
            WallAxis::Z => self.width.abs(),
        }
    }

    /// World Y of the piece's base, resolved against the level's floors when
    /// the level does not author one.
    #[must_use]
    pub fn base_y(&self, surfaces: &LevelSurfaces<'_>) -> f32 {
        if let Some(y) = self.y.filter(|value| value.is_finite()) {
            return y;
        }
        let (x0, x1, z0, z1) = self.bounds();
        surfaces
            .floor_y_at(f32::midpoint(x0, x1), f32::midpoint(z0, z1))
            .unwrap_or(0.0)
    }

    /// Length-face material reference, if it overrides `defaults.wall`.
    #[must_use]
    pub fn material_ref(&self) -> Option<MaterialRef<'_>> {
        self.material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.shine))
    }

    /// End-face material reference: its own, else the length faces'.
    #[must_use]
    pub fn end_ref(&self) -> Option<MaterialRef<'_>> {
        self.end_material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.end_shine))
            .or_else(|| self.material_ref())
    }

    /// Cap material reference: its own, else the length faces'.
    #[must_use]
    pub fn cap_ref(&self) -> Option<MaterialRef<'_>> {
        self.cap_material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.cap_shine))
            .or_else(|| self.material_ref())
    }

    /// The piece's solid box, if its dimensions are usable.
    #[must_use]
    pub fn solid_box(&self, surfaces: &LevelSurfaces<'_>) -> Option<ArchitectureBox> {
        let (x0, x1, z0, z1) = self.bounds();
        if !self.height.is_finite() || self.height <= 0.0 {
            return None;
        }
        let base = self.base_y(surfaces);
        ArchitectureBox::from_corners([x0, base, z0], [x1, base + self.height, z1])
    }
}

/// A reusable square or rectangular column: a solid post with a selectable
/// body material and an optional cap.
///
/// Place by minimum corner like a wall. With `height` omitted the post runs
/// from its base to the local clear ceiling, and its top cap is skipped when it
/// meets the ceiling exactly (so a full-height column never z-fights the
/// ceiling plane); author a smaller `height` for a post with a visible capital.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDef {
    pub x: f32,
    pub z: f32,
    pub width: f32,
    pub depth: f32,
    /// Height above the base; omitted means the local clear ceiling height.
    #[serde(default)]
    pub height: Option<f32>,
    /// Absolute world Y of the base. Omitted means the walkable floor under the
    /// footprint's centre.
    #[serde(default)]
    pub y: Option<f32>,
    /// Body material id; falls back to `defaults.wall`.
    #[serde(default)]
    pub material: Option<String>,
    /// Per-surface shine override for [`Self::material`].
    #[serde(default)]
    pub shine: Option<f32>,
    /// Top cap material id; falls back to [`Self::material`].
    #[serde(default)]
    pub cap_material: Option<String>,
    /// Per-surface shine override for [`Self::cap_material`].
    #[serde(default)]
    pub cap_shine: Option<f32>,
}

impl ColumnDef {
    /// Footprint `(x0, x1, z0, z1)`, normalised.
    #[must_use]
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        (
            self.x.min(self.x + self.width),
            self.x.max(self.x + self.width),
            self.z.min(self.z + self.depth),
            self.z.max(self.z + self.depth),
        )
    }

    /// World Y of the piece's base, resolved against the level's floors when
    /// the level does not author one.
    #[must_use]
    pub fn base_y(&self, surfaces: &LevelSurfaces<'_>) -> f32 {
        if let Some(y) = self.y.filter(|value| value.is_finite()) {
            return y;
        }
        let (x0, x1, z0, z1) = self.bounds();
        surfaces
            .floor_y_at(f32::midpoint(x0, x1), f32::midpoint(z0, z1))
            .unwrap_or(0.0)
    }

    /// World Y of the post's top: the authored height, else the local clear
    /// ceiling.
    #[must_use]
    pub fn top_y(&self, surfaces: &LevelSurfaces<'_>) -> f32 {
        let base = self.base_y(surfaces);
        if let Some(height) = self
            .height
            .filter(|value| value.is_finite() && *value > 0.0)
        {
            return base + height;
        }
        let (x0, x1, z0, z1) = self.bounds();
        let (x, z) = (f32::midpoint(x0, x1), f32::midpoint(z0, z1));
        let ceiling = surfaces.ceiling_y_at(x, z);
        if ceiling.is_finite() && ceiling > base {
            ceiling
        } else {
            base + DEFAULT_CEILING_HEIGHT_M
        }
    }

    /// Body material reference, if it overrides `defaults.wall`.
    #[must_use]
    pub fn material_ref(&self) -> Option<MaterialRef<'_>> {
        self.material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.shine))
    }

    /// Cap material reference: its own, else the body's.
    #[must_use]
    pub fn cap_ref(&self) -> Option<MaterialRef<'_>> {
        self.cap_material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.cap_shine))
            .or_else(|| self.material_ref())
    }

    /// The piece's solid box, if its dimensions are usable.
    #[must_use]
    pub fn solid_box(&self, surfaces: &LevelSurfaces<'_>) -> Option<ArchitectureBox> {
        let (x0, x1, z0, z1) = self.bounds();
        let base = self.base_y(surfaces);
        let top = self.top_y(surfaces);
        ArchitectureBox::from_corners([x0, base, z0], [x1, top, z1])
    }
}

/// A reusable archway: a wall block with a centred opening capped by an arch.
///
/// The footprint is the whole block (placed by minimum corner like a wall).
/// `opening_height` is the clear height at the **crown**, `arch_rise` how much
/// higher the crown is than the springing line where the arch leaves the jambs
/// (`arch_rise: 0` gives a flat lintel), and `height` the block's own height;
/// the block must be at least as tall as the opening.
///
/// The arch is drawn as a small number of flat segments — low-poly, smooth
/// enough to read as a curve and cheap enough for the renderer. Collision only
/// covers the two piers and the spandrel above the opening, so the opening is
/// never blocked and the player never catches on the curve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchwayDef {
    pub x: f32,
    pub z: f32,
    pub width: f32,
    pub depth: f32,
    /// Height of the block above its base, in metres.
    pub height: f32,
    /// Clear width of the opening, in metres.
    pub opening_width: f32,
    /// Clear height of the opening at its crown, in metres.
    pub opening_height: f32,
    /// Crown rise above the springing line, in metres; `0.0` is a flat lintel.
    #[serde(default)]
    pub arch_rise: f32,
    /// Absolute world Y of the base. Omitted means the walkable floor under the
    /// footprint's centre.
    #[serde(default)]
    pub y: Option<f32>,
    /// Face material id; falls back to `defaults.wall`.
    #[serde(default)]
    pub material: Option<String>,
    /// Per-surface shine override for [`Self::material`].
    #[serde(default)]
    pub shine: Option<f32>,
    /// Material for the reveals and the arch soffit; falls back to
    /// [`Self::material`].
    #[serde(default)]
    pub reveal_material: Option<String>,
    /// Per-surface shine override for [`Self::reveal_material`].
    #[serde(default)]
    pub reveal_shine: Option<f32>,
}

/// Number of flat segments the arch curve is approximated with.
pub const ARCHWAY_SEGMENTS: u32 = 8;

/// Smallest pier an archway must keep beside its opening, in metres.
pub const ARCHWAY_MIN_PIER_M: f32 = 0.08;

impl ArchwayDef {
    /// Footprint `(x0, x1, z0, z1)`, normalised.
    #[must_use]
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        (
            self.x.min(self.x + self.width),
            self.x.max(self.x + self.width),
            self.z.min(self.z + self.depth),
            self.z.max(self.z + self.depth),
        )
    }

    /// The axis the block's length runs along: the longer of width/depth.
    #[must_use]
    pub fn axis(&self) -> WallAxis {
        WallAxis::of(self.width.abs(), self.depth.abs())
    }

    /// Length of the block along [`Self::axis`], in metres.
    #[must_use]
    pub fn length(&self) -> f32 {
        match self.axis() {
            WallAxis::X => self.width.abs(),
            WallAxis::Z => self.depth.abs(),
        }
    }

    /// Thickness of the block across [`Self::axis`], in metres.
    #[must_use]
    pub fn thickness(&self) -> f32 {
        match self.axis() {
            WallAxis::X => self.depth.abs(),
            WallAxis::Z => self.width.abs(),
        }
    }

    /// World Y of the piece's base, resolved against the level's floors when
    /// the level does not author one.
    #[must_use]
    pub fn base_y(&self, surfaces: &LevelSurfaces<'_>) -> f32 {
        if let Some(y) = self.y.filter(|value| value.is_finite()) {
            return y;
        }
        let (x0, x1, z0, z1) = self.bounds();
        surfaces
            .floor_y_at(f32::midpoint(x0, x1), f32::midpoint(z0, z1))
            .unwrap_or(0.0)
    }

    /// Arch rise, sanitised to `0.0` for a flat lintel.
    #[must_use]
    pub const fn rise(&self) -> f32 {
        if self.arch_rise.is_finite() && self.arch_rise > 0.0 {
            self.arch_rise
        } else {
            0.0
        }
    }

    /// Clear height of the opening at the jambs, in metres.
    #[must_use]
    pub fn spring_height(&self) -> f32 {
        self.opening_height - self.rise()
    }

    /// Start and end of the opening along the block's length axis, measured
    /// from the minimum corner.
    #[must_use]
    pub fn opening_span(&self) -> (f32, f32) {
        let length = self.length();
        let half = self.opening_width * 0.5;
        let centre = length * 0.5;
        (centre - half, centre + half)
    }

    /// World `(x, z)` of the arch curve's crown at height offset `y`, for the
    /// segment boundary at run offset `along`.
    #[must_use]
    pub fn arch_height_at(&self, along: f32) -> f32 {
        let (start, end) = self.opening_span();
        let half = self.opening_width * 0.5;
        let rise = self.rise();
        if half <= 0.0 || rise <= 0.0 {
            return self.opening_height;
        }
        // A circular segment through the two springing points and the crown:
        // solving for the circle's rise above the chord gives the curve.
        let x = (along - f32::midpoint(start, end)).clamp(-half, half) / half;
        let shape = (1.0 - x * x).max(0.0).sqrt();
        rise.mul_add(shape, self.spring_height())
    }

    /// Face material reference, if it overrides `defaults.wall`.
    #[must_use]
    pub fn material_ref(&self) -> Option<MaterialRef<'_>> {
        self.material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.shine))
    }

    /// Reveal/soffit material reference: its own, else the faces'.
    #[must_use]
    pub fn reveal_ref(&self) -> Option<MaterialRef<'_>> {
        self.reveal_material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.reveal_shine))
            .or_else(|| self.material_ref())
    }

    /// The solid boxes the archway contributes: one per pier, plus the
    /// spandrel above the opening.
    #[must_use]
    pub fn solid_boxes(&self, surfaces: &LevelSurfaces<'_>) -> Vec<ArchitectureBox> {
        let (x0, x1, z0, z1) = self.bounds();
        let base = self.base_y(surfaces);
        let top = base + self.height;
        let (open_start, open_end) = self.opening_span();
        let spring = base + self.spring_height();
        let mut boxes = Vec::with_capacity(3);
        // In length/across space: the first pier, the second pier and the
        // spandrel the arch leaves above the opening.
        let piers = [
            (0.0, open_start, base, top),
            (open_end, self.length(), base, top),
            (open_start, open_end, spring, top),
        ];
        for (start, end, bottom, ceiling) in piers {
            let piece_box = match self.axis() {
                WallAxis::X => {
                    ArchitectureBox::from_corners([x0 + start, bottom, z0], [x0 + end, ceiling, z1])
                }
                WallAxis::Z => {
                    ArchitectureBox::from_corners([x0, bottom, z0 + start], [x1, ceiling, z0 + end])
                }
            };
            if let Some(piece_box) = piece_box {
                boxes.push(piece_box);
            }
        }
        boxes
    }
}

/// A reusable guardrail or stair handrail: a wooden rail run with posts.
///
/// The rail runs along its own local `+X` axis from `(x, z)`, rotated by
/// `rotation_degrees` about Y (0 runs east, 90 north, 180 west, 270 south), and
/// `rise` slopes it for a staircase or ramp (`rise: 0` is a level landing
/// rail). It stands `height` tall with a top rail, a lower rail and square
/// posts at `post_spacing`, and it is solid: a guardrail is a barrier, so it
/// blocks the player rather than merely being drawn.
///
/// `material` is the rail timber and `post_material` the posts' (falling back
/// to the rail's). Nothing about the piece is residential: name a metal or
/// painted material and it is an industrial or office rail.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardrailDef {
    /// World X of the rail's start point.
    pub x: f32,
    /// World Z of the rail's start point.
    pub z: f32,
    /// Length of the run, in metres.
    pub length: f32,
    /// Yaw about Y in degrees; 0 runs east (+X).
    #[serde(default)]
    pub rotation_degrees: f32,
    /// Height of the top rail's top edge above the base line, in metres.
    #[serde(default = "default_guardrail_height")]
    pub height: f32,
    /// Height change along the run, in metres (a stair or ramp rail). Omitted
    /// means the rail follows the walkable floor from its start point to its
    /// end point — a handrail beside a flight or ramp keeps a constant height
    /// above the sloped surface without the author having to compute the
    /// difference; author `rise` to override that line (a level rail on a
    /// slope, or a known rise).
    #[serde(default)]
    pub rise: Option<f32>,
    /// Distance between posts, in metres.
    #[serde(default = "default_post_spacing")]
    pub post_spacing: f32,
    /// Absolute world Y of the base line at the start point. Omitted means the
    /// walkable floor under the start point.
    #[serde(default)]
    pub y: Option<f32>,
    /// Rail material id; falls back to `defaults.wall`.
    #[serde(default)]
    pub material: Option<String>,
    /// Per-surface shine override for [`Self::material`].
    #[serde(default)]
    pub shine: Option<f32>,
    /// Post material id; falls back to [`Self::material`].
    #[serde(default)]
    pub post_material: Option<String>,
    /// Per-surface shine override for [`Self::post_material`].
    #[serde(default)]
    pub post_shine: Option<f32>,
}

const fn default_guardrail_height() -> f32 {
    GUARDRAIL_DEFAULT_HEIGHT_M
}

const fn default_post_spacing() -> f32 {
    GUARDRAIL_DEFAULT_POST_SPACING_M
}

impl GuardrailDef {
    /// Direction of the run as a `(x, z)` unit vector.
    #[must_use]
    pub fn direction(&self) -> (f32, f32) {
        let radians = self.rotation_degrees.to_radians();
        (radians.cos(), -radians.sin())
    }

    /// Across-the-run direction as a `(x, z)` unit vector.
    #[must_use]
    pub fn across(&self) -> (f32, f32) {
        let radians = self.rotation_degrees.to_radians();
        (radians.sin(), radians.cos())
    }

    /// Signed rise as authored, sanitised to `0.0` for non-finite values.
    ///
    /// This is the authored override only; [`Self::resolved_rise`] is the value
    /// the geometry and collision use, which follows the floor when no rise is
    /// authored.
    #[must_use]
    pub const fn rise(&self) -> f32 {
        match self.rise {
            Some(rise) if rise.is_finite() => rise,
            _ => 0.0,
        }
    }

    /// The rise the rail actually runs with: the authored value, or the
    /// walkable floor's change from the start point to the end point.
    ///
    /// Following the floors is what makes the primitive usable as a handrail
    /// beside a staircase or a ramp: the rail's base line stays the walkable
    /// surface's own slope, so the top rail keeps a constant height above the
    /// nosings. A level rail on sloping ground authors `rise: 0.0` explicitly.
    #[must_use]
    pub fn resolved_rise(&self, surfaces: &LevelSurfaces<'_>) -> f32 {
        if self.rise.is_some() {
            return self.rise();
        }
        let (end_x, end_z) = self.point_at(1.0, 0.0);
        let start = surfaces.floor_y_at(self.x, self.z).unwrap_or(0.0);
        let end = surfaces.floor_y_at(end_x, end_z).unwrap_or(start);
        let rise = end - start;
        if rise.is_finite() { rise } else { 0.0 }
    }

    /// Top-rail height, sanitised to the default.
    #[must_use]
    pub fn height(&self) -> f32 {
        if self.height.is_finite() && self.height > 0.0 {
            self.height
        } else {
            GUARDRAIL_DEFAULT_HEIGHT_M
        }
    }

    /// Post spacing, sanitised to the default.
    #[must_use]
    pub fn post_spacing(&self) -> f32 {
        if self.post_spacing.is_finite() && self.post_spacing > 0.0 {
            self.post_spacing
        } else {
            GUARDRAIL_DEFAULT_POST_SPACING_M
        }
    }

    /// World `(x, z)` of a point at run fraction `fraction`, offset across the
    /// run by `across` metres.
    #[must_use]
    pub fn point_at(&self, fraction: f32, across: f32) -> (f32, f32) {
        let (dx, dz) = self.direction();
        let (ax, az) = self.across();
        let along = self.length * fraction.clamp(0.0, 1.0);
        (
            dx.mul_add(along, ax.mul_add(across, self.x)),
            dz.mul_add(along, az.mul_add(across, self.z)),
        )
    }

    /// World Y of the base line at run fraction `fraction`.
    ///
    /// An authored `y` pins the start; an omitted one resolves the walkable
    /// floor under the start point, and an omitted `rise` follows the floor's
    /// change to the end point.
    #[must_use]
    pub fn base_y_at(&self, surfaces: &LevelSurfaces<'_>, fraction: f32) -> f32 {
        let base = self.base_y(surfaces);
        self.resolved_rise(surfaces)
            .mul_add(fraction.clamp(0.0, 1.0), base)
    }

    /// World Y of the base line at the run's start point, resolved against the
    /// level's floors when the level does not author one.
    #[must_use]
    pub fn base_y(&self, surfaces: &LevelSurfaces<'_>) -> f32 {
        if let Some(y) = self.y.filter(|value| value.is_finite()) {
            return y;
        }
        surfaces.floor_y_at(self.x, self.z).unwrap_or(0.0)
    }

    /// Rail material reference, if it overrides `defaults.wall`.
    #[must_use]
    pub fn material_ref(&self) -> Option<MaterialRef<'_>> {
        self.material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.shine))
    }

    /// Post material reference: its own, else the rail's.
    #[must_use]
    pub fn post_ref(&self) -> Option<MaterialRef<'_>> {
        self.post_material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.post_shine))
            .or_else(|| self.material_ref())
    }

    /// The barrier's solid box: the run's whole swept volume, from just below
    /// the base line to the top of the rail.
    #[must_use]
    pub fn solid_box(&self, surfaces: &LevelSurfaces<'_>) -> Option<ArchitectureBox> {
        let corners = [
            self.point_at(0.0, -GUARDRAIL_RAIL_WIDTH_M * 0.5),
            self.point_at(0.0, GUARDRAIL_RAIL_WIDTH_M * 0.5),
            self.point_at(1.0, -GUARDRAIL_RAIL_WIDTH_M * 0.5),
            self.point_at(1.0, GUARDRAIL_RAIL_WIDTH_M * 0.5),
        ];
        let (mut x0, mut x1, mut z0, mut z1) = (
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
        );
        for (x, z) in corners {
            x0 = x0.min(x);
            x1 = x1.max(x);
            z0 = z0.min(z);
            z1 = z1.max(z);
        }
        let base_low = self
            .base_y_at(surfaces, 0.0)
            .min(self.base_y_at(surfaces, 1.0));
        let base_high = self
            .base_y_at(surfaces, 0.0)
            .max(self.base_y_at(surfaces, 1.0));
        // Back the barrier a little below the base line so a player standing on
        // a lower floor still meets it.
        ArchitectureBox::from_corners(
            [x0, base_low - 0.2, z0],
            [x1, base_high + self.height(), z1],
        )
    }
}

/// A reusable floor threshold strip: the narrow transition piece between two
/// floor materials at a doorway.
///
/// It is a decorative strip, not architecture: it sits on the floor, is raised
/// by `height` (a centimetre or so) and takes a material of its own, so a
/// hardwood-to-carpet doorway has a real painted or wooden transition instead
/// of two floors meeting in a line. It deliberately carries **no collision**:
/// the player walks over it, and a trip-hazard collider under a doorway is
/// exactly the kind of decoration the movement code should ignore.
///
/// It is placed by its centre and runs along its own local `+X` axis, rotated
/// by `rotation_degrees` (0 runs east). Author `length` a few centimetres wider
/// than the opening so the strip's ends tuck into the jambs rather than
/// touching them face to face, and keep it over a level floor: the loader
/// rejects a threshold whose ends stand at different heights.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdDef {
    pub x: f32,
    pub z: f32,
    /// Length along the strip's own axis, in metres.
    pub length: f32,
    /// Width across the strip (the doorway's depth direction), in metres.
    #[serde(default = "default_threshold_thickness")]
    pub thickness: f32,
    /// How far the strip stands above the floor, in metres.
    #[serde(default = "default_threshold_height")]
    pub height: f32,
    /// Yaw about Y in degrees; 0 runs east (+X).
    #[serde(default)]
    pub rotation_degrees: f32,
    /// Absolute world Y of the strip's base. Omitted means the walkable floor
    /// under the strip's centre.
    #[serde(default)]
    pub y: Option<f32>,
    /// Strip material id; falls back to `defaults.floor`.
    #[serde(default)]
    pub material: Option<String>,
    /// Per-surface shine override for [`Self::material`].
    #[serde(default)]
    pub shine: Option<f32>,
}

const fn default_threshold_thickness() -> f32 {
    THRESHOLD_DEFAULT_THICKNESS_M
}

const fn default_threshold_height() -> f32 {
    THRESHOLD_DEFAULT_HEIGHT_M
}

impl ThresholdDef {
    /// Strip thickness, sanitised to the default.
    #[must_use]
    pub fn thickness(&self) -> f32 {
        if self.thickness.is_finite() && self.thickness > 0.0 {
            self.thickness
        } else {
            THRESHOLD_DEFAULT_THICKNESS_M
        }
    }

    /// Strip height above the floor, sanitised to the default.
    #[must_use]
    pub fn height(&self) -> f32 {
        if self.height.is_finite() && self.height > 0.0 {
            self.height
        } else {
            THRESHOLD_DEFAULT_HEIGHT_M
        }
    }

    /// Direction of the strip as a `(x, z)` unit vector.
    #[must_use]
    pub fn direction(&self) -> (f32, f32) {
        let radians = self.rotation_degrees.to_radians();
        (radians.cos(), -radians.sin())
    }

    /// Across-the-strip direction as a `(x, z)` unit vector.
    #[must_use]
    pub fn across(&self) -> (f32, f32) {
        let radians = self.rotation_degrees.to_radians();
        (radians.sin(), radians.cos())
    }

    /// World `(x, z)` at run offset `along` metres from the strip's centre and
    /// `across` metres across it.
    #[must_use]
    pub fn point_at_offset(&self, along: f32, across: f32) -> (f32, f32) {
        let (dx, dz) = self.direction();
        let (ax, az) = self.across();
        (
            dx.mul_add(along, ax.mul_add(across, self.x)),
            dz.mul_add(along, az.mul_add(across, self.z)),
        )
    }

    /// World `(x, z)` of a point at run fraction `fraction` (0 at the strip's
    /// centre, 1 at one end) and across offset `across`, in metres.
    #[must_use]
    pub fn point_at(&self, fraction: f32, across: f32) -> (f32, f32) {
        self.point_at_offset(self.length * fraction.clamp(0.0, 1.0), across)
    }

    /// World Y of the strip's base, resolved against the level's floors when
    /// the level does not author one.
    #[must_use]
    pub fn base_y(&self, surfaces: &LevelSurfaces<'_>) -> f32 {
        if let Some(y) = self.y.filter(|value| value.is_finite()) {
            return y;
        }
        surfaces.floor_y_at(self.x, self.z).unwrap_or(0.0)
    }

    /// Strip material reference, if it overrides `defaults.floor`.
    #[must_use]
    pub fn material_ref(&self) -> Option<MaterialRef<'_>> {
        self.material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.shine))
    }
}

/// A reusable baseboard / skirting run: a thin trim board along the bottom of a
/// wall.
///
/// It runs along its own local `+X` axis from `(x, z)`, rotated by
/// `rotation_degrees` (0 runs east, 90 north, 180 west, 270 south), stands
/// `height` tall and `thickness` proud of the wall plane it is placed against.
/// It carries **no collision**: a nine-centimetre board is decoration, and the
/// player's own radius already keeps them clear of it.
///
/// Corners are made the way trim is fitted: run two boards so they overlap at
/// the corner by about their own thickness, leaving the ends buried inside each
/// other, or stop one against the other's face. The material is whatever the
/// level names, so the same geometry is a painted Home skirting or an
/// industrial kick plate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaseboardDef {
    /// World X of the run's start point.
    pub x: f32,
    /// World Z of the run's start point.
    pub z: f32,
    /// Length of the run, in metres.
    pub length: f32,
    /// Yaw about Y in degrees; 0 runs east (+X).
    #[serde(default)]
    pub rotation_degrees: f32,
    /// Height of the board above its base line, in metres.
    #[serde(default = "default_baseboard_height")]
    pub height: f32,
    /// How far the board stands proud of the wall, in metres.
    #[serde(default = "default_baseboard_thickness")]
    pub thickness: f32,
    /// Absolute world Y of the board's base. Omitted means the walkable floor
    /// under the run's start point.
    #[serde(default)]
    pub y: Option<f32>,
    /// Board material id; falls back to `defaults.wall`.
    #[serde(default)]
    pub material: Option<String>,
    /// Per-surface shine override for [`Self::material`].
    #[serde(default)]
    pub shine: Option<f32>,
}

const fn default_baseboard_height() -> f32 {
    BASEBOARD_DEFAULT_HEIGHT_M
}

const fn default_baseboard_thickness() -> f32 {
    BASEBOARD_DEFAULT_THICKNESS_M
}

impl BaseboardDef {
    /// Board height, sanitised to the default.
    #[must_use]
    pub fn height(&self) -> f32 {
        if self.height.is_finite() && self.height > 0.0 {
            self.height
        } else {
            BASEBOARD_DEFAULT_HEIGHT_M
        }
    }

    /// Board thickness, sanitised to the default.
    #[must_use]
    pub fn thickness(&self) -> f32 {
        if self.thickness.is_finite() && self.thickness > 0.0 {
            self.thickness
        } else {
            BASEBOARD_DEFAULT_THICKNESS_M
        }
    }

    /// Direction of the run as a `(x, z)` unit vector.
    #[must_use]
    pub fn direction(&self) -> (f32, f32) {
        let radians = self.rotation_degrees.to_radians();
        (radians.cos(), -radians.sin())
    }

    /// Across-the-board direction as a `(x, z)` unit vector, pointing out of
    /// the wall's face.
    #[must_use]
    pub fn across(&self) -> (f32, f32) {
        let radians = self.rotation_degrees.to_radians();
        (radians.sin(), radians.cos())
    }

    /// World `(x, z)` of a point at run fraction `fraction` and across offset
    /// `across` metres from the wall plane.
    #[must_use]
    pub fn point_at(&self, fraction: f32, across: f32) -> (f32, f32) {
        let (dx, dz) = self.direction();
        let (ax, az) = self.across();
        let along = self.length * fraction.clamp(0.0, 1.0);
        (
            dx.mul_add(along, ax.mul_add(across, self.x)),
            dz.mul_add(along, az.mul_add(across, self.z)),
        )
    }

    /// World Y of the board's base, resolved against the level's floors when
    /// the level does not author one.
    #[must_use]
    pub fn base_y(&self, surfaces: &LevelSurfaces<'_>) -> f32 {
        if let Some(y) = self.y.filter(|value| value.is_finite()) {
            return y;
        }
        surfaces.floor_y_at(self.x, self.z).unwrap_or(0.0)
    }

    /// Board material reference, if it overrides `defaults.wall`.
    #[must_use]
    pub fn material_ref(&self) -> Option<MaterialRef<'_>> {
        self.material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.shine))
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
///
/// Also the exclusive-boundary tolerance for "is this point inside a wall
/// solid", used by the loader's buried-trim checks.
pub(crate) const WALL_SLICE_EPS: f32 = 1e-4;

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
    /// Per-surface shine override, `0.0..=1.0`; omitted keeps the material's
    /// default.
    #[serde(default)]
    pub shine: Option<f32>,
}

impl FloorPatchDef {
    /// The patch's material reference, with its optional shine override.
    #[must_use]
    pub fn material_ref(&self) -> MaterialRef<'_> {
        MaterialRef::with_shine(&self.material, self.shine)
    }

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

/// Ceiling light fixture placement.
///
/// `brightness` is the optional fixture intensity/power. It is the field the
/// level editor already authors and writes, so it stays the canonical key; the
/// more descriptive `intensity` spelling is accepted as an alias so levels
/// written from the design notes load unchanged. Omitted means `1.0`.
///
/// `color` is the optional emitted light colour as an `[r, g, b]` array of
/// `0.0..=1.0` fractions. It drives the coloured illumination the bake applies
/// to surrounding geometry; the fixture's visible face is texture-first and is
/// never tinted by it. Levels that omit it keep loading: they emit
/// [`DEFAULT_LIGHT_COLOR`], the restrained warm fluorescent the game has always
/// implied.
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
    ///
    /// The colour is a property of the *illumination*: it tints the baked
    /// light and its local pool. It never repaints the fixture's visible face,
    /// which is texture-first — the catalog sheet defines the fixture's own
    /// colour and the face carries only a neutral emission brightness.
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
    /// still glows at its neutral emission brightness, while the bake skips it
    /// entirely. This is the authored half of the separation between material
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
    /// [`LightColor::sanitized`]). This is the single source of truth for the
    /// coloured environmental illumination (the bake and its local pools); the
    /// fixture's visible face takes only a neutral emission brightness from the
    /// light and never this colour.
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
    /// Straight sloped walking surfaces (ramps). Empty on every legacy level.
    #[serde(default)]
    pub ramps: Vec<RampDef>,
    /// Straight stepped walking surfaces (staircases). Empty on every legacy
    /// level.
    #[serde(default)]
    pub stairs: Vec<StairDef>,
    /// Solid half-height walls: partitions, parapets and knee walls.
    #[serde(default)]
    pub half_walls: Vec<HalfWallDef>,
    /// Solid square or rectangular columns/posts.
    #[serde(default)]
    pub columns: Vec<ColumnDef>,
    /// Arched openings through a wall block.
    #[serde(default)]
    pub archways: Vec<ArchwayDef>,
    /// Guardrails and stair handrails.
    #[serde(default)]
    pub guardrails: Vec<GuardrailDef>,
    /// Floor threshold strips: the transition between two floor materials.
    #[serde(default)]
    pub thresholds: Vec<ThresholdDef>,
    /// Baseboard / skirting runs along a wall.
    #[serde(default)]
    pub baseboards: Vec<BaseboardDef>,
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

/// Number of quads the office fluorescent panel generates (its single
/// luminous face).
///
/// The sheet is the whole fixture. Other fixture families declare their own
/// budget on [`crate::lighting::FixtureProfile::quads`].
pub const MAX_LIGHT_QUADS: u64 = 1;
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
/// instance budget stay bounded on a desktop. A file above this is genuinely
/// unsupported rather than merely over budget.
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
/// Hard ceiling on one prop model's vertex count (16-bit indices).
pub const MAX_PROP_VERTICES: usize = 65_535;
/// The normal native edge length of a shipped prop texture.
///
/// 256x256 is the standard prop atlas size, not a special high-quality
/// variant: the refreshed pack ships at it, the prop toolkit treats it as the
/// unremarkable default, and `Full` uploads it unchanged. Larger embedded
/// textures from third-party GLBs still load (up to [`MAX_PROP_TEXTURE_SIZE`])
/// and are downscaled to the active profile's budget; no *shipped* atlas needs
/// more, because `Full` never samples a prop sheet above this size.
pub const PROP_TEXTURE_NATIVE_SIZE: u32 = 256;
/// Hard engine ceiling on a prop texture's edge length.
///
/// Matches the surface decoder's [`crate::assets::MAX_TEXTURE_DIMENSION`]: a
/// GLB may carry a texture up to the same size any other asset may, and the
/// runtime quality profile decides what actually reaches the GPU.
pub const MAX_PROP_TEXTURE_SIZE: u32 = 1_024;
/// Decoded RGBA8 memory one prop texture may hold at the engine ceiling.
///
/// One 1024x1024 image (4 MiB). The parser rejects a larger edge before any
/// decode, so this is the largest allocation one embedded image can request.
pub const MAX_PROP_TEXTURE_BYTES: usize =
    crate::assets::decoded_rgba_bytes(MAX_PROP_TEXTURE_SIZE, MAX_PROP_TEXTURE_SIZE);
/// Decoded RGBA8 budget for the whole shipped prop pack.
///
/// A deliberately desktop-scale limit: 64 MiB holds 256 native 256x256 sheets
/// (the current 33-prop pack decodes to under 4 MiB), so ordinary content
/// growth never has to trade texture quality against the budget. It still
/// refuses a pathological or accidentally duplicated multi-gigabyte set
/// before it can become resident.
pub const PROP_TEXTURE_PACK_BUDGET_BYTES: usize = 64 * 1024 * 1024;

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
/// Hard ceiling on the number of ramps a level may define.
pub const MAX_LEVEL_RAMPS: u64 = 500;
/// Hard ceiling on the number of staircases a level may define.
pub const MAX_LEVEL_STAIRS: u64 = 500;
/// Hard ceiling on the number of half walls a level may define.
pub const MAX_LEVEL_HALF_WALLS: u64 = 2000;
/// Hard ceiling on the number of columns a level may define.
pub const MAX_LEVEL_COLUMNS: u64 = 2000;
/// Hard ceiling on the number of archways a level may define.
pub const MAX_LEVEL_ARCHWAYS: u64 = 500;
/// Hard ceiling on the number of guardrails a level may define.
pub const MAX_LEVEL_GUARDRAILS: u64 = 2000;
/// Hard ceiling on the number of threshold strips a level may define.
pub const MAX_LEVEL_THRESHOLDS: u64 = 1000;
/// Hard ceiling on the number of baseboard runs a level may define.
pub const MAX_LEVEL_BASEBOARDS: u64 = 2000;
/// Upper bound on the quads a ramp emits beyond its top-surface cells: two
/// side skirts, two end faces and their lightmap tiling.
pub const MAX_RAMP_EXTRA_QUADS: u64 = 12;
/// Upper bound on the quads a staircase emits beyond its treads and risers.
pub const MAX_STAIR_EXTRA_QUADS: u64 = 8;
/// Upper bound on the quads one half wall emits: length faces, ends and caps.
pub const MAX_HALF_WALL_QUADS: u64 = 6;
/// Upper bound on the quads one column emits.
pub const MAX_COLUMN_QUADS: u64 = 6;
/// Upper bound on the quads an archway emits beyond its two arch curves (front
/// and back).
pub const MAX_ARCHWAY_EXTRA_QUADS: u64 = 16;
/// Upper bound on the quads a guardrail emits beyond its posts.
pub const MAX_GUARDRAIL_EXTRA_QUADS: u64 = 16;
/// Upper bound on the quads one threshold strip emits.
pub const MAX_THRESHOLD_QUADS: u64 = 5;
/// Upper bound on the quads one baseboard run emits: a front run, a cap split
/// into up to two trimmed pieces (each a triangle fan of at most two triangles)
/// and two end faces.
pub const MAX_BASEBOARD_QUADS: u64 = 8;
/// Hard byte ceiling on a standalone level JSON file before it is parsed.
///
/// The shipped demo is about 23 KB, so this is three orders of magnitude of
/// headroom for a hand-authored or generated level while still refusing an
/// accidentally huge file before it is read into memory.
pub const MAX_LEVEL_JSON_BYTES: u64 = 8 * 1024 * 1024;
/// Sanity budget for total authored floor area, in square metres.
///
/// Floor rendering no longer scales with area, but absurdly large levels still
/// stress collision, fill rate and level-design tooling, so a generous cap is
/// kept as a sanity guard.
pub const MAX_LEVEL_FLOOR_AREA_M2: u64 = 1_000_000;
/// Sanity budget on the estimated number of generated vertices.
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

    /// Ramps overlapping the given room's footprint, in authored order.
    #[must_use]
    pub fn ramps_for_room(&self, room: &RoomDef) -> Vec<&RampDef> {
        let (x0, x1, z0, z1) = room.bounds();
        self.ramps
            .iter()
            .filter(|ramp| {
                let (rx0, rx1, rz0, rz1) = ramp.bounds();
                rx1 > x0 && rx0 < x1 && rz1 > z0 && rz0 < z1
            })
            .collect()
    }

    /// Staircases overlapping the given room's footprint, in authored order.
    #[must_use]
    pub fn stairs_for_room(&self, room: &RoomDef) -> Vec<&StairDef> {
        let (x0, x1, z0, z1) = room.bounds();
        self.stairs
            .iter()
            .filter(|stair| {
                let (sx0, sx1, sz0, sz1) = stair.bounds();
                sx1 > x0 && sx0 < x1 && sz1 > z0 && sz0 < z1
            })
            .collect()
    }

    /// Every solid architectural piece as one axis-aligned box, in authored
    /// order: half walls, columns, the piers and spandrel of every archway, and
    /// every guardrail's barrier volume.
    ///
    /// This is the single list collision and the lighting bake share, so a
    /// piece that is drawn is also solid and also occludes. The decorative
    /// pieces (thresholds, baseboards) are deliberately absent: they are trim,
    /// not barriers.
    #[must_use]
    pub fn architecture_solids(&self) -> Vec<ArchitectureBox> {
        let surfaces = LevelSurfaces::new(self);
        let mut boxes: Vec<ArchitectureBox> = Vec::new();
        for piece in &self.half_walls {
            if let Some(boxed) = piece.solid_box(&surfaces) {
                boxes.push(boxed);
            }
        }
        for piece in &self.columns {
            if let Some(boxed) = piece.solid_box(&surfaces) {
                boxes.push(boxed);
            }
        }
        for piece in &self.archways {
            boxes.extend(piece.solid_boxes(&surfaces));
        }
        for piece in &self.guardrails {
            if let Some(boxed) = piece.solid_box(&surfaces) {
                boxes.push(boxed);
            }
        }
        boxes
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

    /// Upper bound on the extra floor and wall quads the architectural pieces
    /// contribute: ramps and staircases count as walking surfaces, everything
    /// else as wall-like geometry.
    fn architecture_estimate(&self) -> (u64, u64) {
        let mut floor_quads: u64 = 0;
        let mut wall_quads: u64 = 0;
        for ramp in &self.ramps {
            let cells = u64::from(crate::lighting::light_grid_cells(ramp.width.abs()))
                .saturating_mul(u64::from(crate::lighting::light_grid_cells(
                    ramp.depth.abs(),
                )));
            floor_quads = floor_quads
                .saturating_add(cells)
                .saturating_add(MAX_RAMP_EXTRA_QUADS);
        }
        for stair in &self.stairs {
            wall_quads = wall_quads
                .saturating_add(u64::from(stair.step_count()).saturating_mul(4))
                .saturating_add(MAX_STAIR_EXTRA_QUADS);
        }
        for _ in &self.half_walls {
            wall_quads = wall_quads.saturating_add(MAX_HALF_WALL_QUADS);
        }
        for _ in &self.columns {
            wall_quads = wall_quads.saturating_add(MAX_COLUMN_QUADS);
        }
        for _ in &self.archways {
            wall_quads = wall_quads.saturating_add(
                u64::from(ARCHWAY_SEGMENTS)
                    .saturating_mul(2)
                    .saturating_add(MAX_ARCHWAY_EXTRA_QUADS),
            );
        }
        for rail in &self.guardrails {
            let posts = clamped_ceil_u64(rail.length / rail.post_spacing(), 1024.0);
            wall_quads = wall_quads
                .saturating_add(posts.saturating_add(2).saturating_mul(4))
                .saturating_add(MAX_GUARDRAIL_EXTRA_QUADS);
        }
        for _ in &self.thresholds {
            wall_quads = wall_quads.saturating_add(MAX_THRESHOLD_QUADS);
        }
        for _ in &self.baseboards {
            wall_quads = wall_quads.saturating_add(MAX_BASEBOARD_QUADS);
        }
        (floor_quads, wall_quads)
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
        // Architectural pieces. Ramps and staircases are walking surfaces (their
        // top faces join the floor count); half walls, columns, archways,
        // guardrails, thresholds and baseboards are wall-like solids and trim.
        // Every count is an upper bound, so the estimate keeps bounding what the
        // builder emits.
        let (arch_floor_quads, arch_wall_quads) = self.architecture_estimate();
        floor_quads = floor_quads.saturating_add(arch_floor_quads);
        wall_quads = wall_quads.saturating_add(arch_wall_quads);

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
            let grid = surfaces.floor_grid(room);
            // Rims compare the *walking* surface on either side of a grid edge,
            // so a staircase or ramp arriving at a raised platform is not walled
            // off by that platform's rim.
            grid.push_region_rims(
                |x, z| surfaces.floor_y_at(x, z).unwrap_or(room.floor_y),
                &mut aabbs,
            );
        }

        // Architectural solids: half walls, columns, archway piers and
        // spandrels and guardrails are real barriers, so they collide exactly
        // like a wall slice. Thresholds and baseboards are deliberately absent:
        // they are trim the player walks over.
        for boxed in self.architecture_solids() {
            aabbs.push(boxed.to_wall_aabb());
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

    /// World Y of the cell containing `(x, z)`, sampled from its centre.
    ///
    /// The lookup is the inverse of the grid's own construction: a point is
    /// resolved to the cell whose span contains it, and a point outside every
    /// cell resolves to the nearest cell.
    #[must_use]
    pub fn height_at(&self, room: &RoomDef, x: f32, z: f32) -> f32 {
        let index_of = |positions: &[f32], value: f32, cells: usize| -> usize {
            positions
                .windows(2)
                .position(|span| {
                    let (&low, &high) = (
                        span.first().unwrap_or(&value),
                        span.get(1).unwrap_or(&value),
                    );
                    value >= low && value < high
                })
                .unwrap_or_else(|| cells.saturating_sub(1))
        };
        let ix = index_of(&self.xs, x, self.cells_x());
        let iz = index_of(&self.zs, z, self.cells_z());
        self.y_at(room, ix, iz)
    }

    /// Emits a solid box for every grid edge where the walking surface changes
    /// by more than a walkable step, spanning the edge and the height
    /// difference.
    ///
    /// Shallow steps are deliberately *not* solid: the player controller steps
    /// up and down them, which is what makes staircases built from floor regions
    /// work without any stair-specific code.
    ///
    /// `heights` supplies the walking-surface height at a point, and is what
    /// lets the rim rule see the *walking* surface rather than the bare region
    /// grid: a staircase or ramp crossing a grid edge raises one side of the
    /// edge, so a flight or slope that arrives at a raised platform is not
    /// walled off by that platform's rim. Callers that want the region grid
    /// alone pass the grid's own cell heights. Each edge is sampled
    /// [`RIM_PROBE_M`] either side of it, which reads the two sides of a cliff
    /// without blurring a gradual slope into the same answer.
    ///
    /// Each rim's blocking face sits exactly on the boundary, so a player
    /// standing on the lower side stops one player radius short of the visible
    /// transition face, exactly as they do at an authored wall. The box is
    /// [`RIM_BACKING`] deep *under the higher floor*, which is what stops a
    /// sub-stepped move from tunnelling through a zero-thickness wall; because
    /// the box also carries [`crate::collision::PLAYER_STEP_HEIGHT`] of
    /// `step_up`, it never blocks a player whose feet are already within a
    /// walkable step of the rim's top (a ramp or staircase arriving beside the
    /// platform), which is what a rim is not allowed to do.
    pub fn push_region_rims(&self, heights: impl Fn(f32, f32) -> f32, out: &mut Vec<WallAabb>) {
        let (cells_x, cells_z) = (self.cells_x(), self.cells_z());
        if cells_x == 0 || cells_z == 0 {
            return;
        }
        // Every interior grid line across X, one rim per cell row.
        for (ix, &at) in self.xs.iter().enumerate().skip(1) {
            if ix >= cells_x {
                break;
            }
            for z_span in self.zs.windows(2) {
                let &[z0, z1] = z_span else {
                    continue;
                };
                for (sz0, sz1) in split_rim_span(z0, z1) {
                    let mid = f32::midpoint(sz0, sz1);
                    let near = heights(at - RIM_PROBE_M, mid);
                    let far = heights(at + RIM_PROBE_M, mid);
                    if (far - near).abs() <= PLAYER_STEP_HEIGHT + 1e-3 {
                        continue;
                    }
                    // Extend under the higher side so the blocking face is the
                    // boundary itself.
                    let (rx0, rx1) = if near > far {
                        (at - RIM_BACKING, at)
                    } else {
                        (at, at + RIM_BACKING)
                    };
                    out.push(
                        WallAabb::with_y(
                            rx0,
                            near.min(far),
                            sz0,
                            rx1 - rx0,
                            (near - far).abs(),
                            sz1 - sz0,
                        )
                        .allowing_step(),
                    );
                }
            }
        }
        // Every interior grid line across Z, the mirror case.
        for (iz, &at) in self.zs.iter().enumerate().skip(1) {
            if iz >= cells_z {
                break;
            }
            for x_span in self.xs.windows(2) {
                let &[x0, x1] = x_span else {
                    continue;
                };
                for (sx0, sx1) in split_rim_span(x0, x1) {
                    let mid = f32::midpoint(sx0, sx1);
                    let near = heights(mid, at - RIM_PROBE_M);
                    let far = heights(mid, at + RIM_PROBE_M);
                    if (far - near).abs() <= PLAYER_STEP_HEIGHT + 1e-3 {
                        continue;
                    }
                    let (rz0, rz1) = if near > far {
                        (at - RIM_BACKING, at)
                    } else {
                        (at, at + RIM_BACKING)
                    };
                    out.push(
                        WallAabb::with_y(
                            sx0,
                            near.min(far),
                            rz0,
                            sx1 - sx0,
                            (near - far).abs(),
                            rz1 - rz0,
                        )
                        .allowing_step(),
                    );
                }
            }
        }
    }
}

/// Distance either side of a floor-grid edge at which a rim samples the
/// walking surface, in metres.
///
/// Close enough that a rising slope is read at the edge itself rather than at
/// its cell's average, far enough that the sample lands clearly on one side.
pub const RIM_PROBE_M: f32 = 0.01;

/// Longest run of a floor-region rim segment along the boundary, in metres.
///
/// A rim's height is sampled at the segment's midpoint, and a sloped walking
/// surface changes height across the segment. At the loader's maximum slope
/// (2 m per metre) a 0.25 m segment is within 0.25 m of the local surface
/// anywhere inside it, which is inside the walkable-step tolerance a rim
/// carries ([`WallAabb::allowing_step`]); one rim per whole grid cell used a
/// ceiling sampled from the cell's middle and blocked a player climbing the
/// lower half of a steep ramp that ran beside a platform edge.
pub const RIM_SEGMENT_M: f32 = 0.25;

/// Splits one rim span into sub-spans of at most [`RIM_SEGMENT_M`].
///
/// At least one span is returned for a non-finite or empty input so callers
/// never drop a rim entirely.
fn split_rim_span(low: f32, high: f32) -> Vec<(f32, f32)> {
    let span = high - low;
    if !span.is_finite() || span <= RIM_SEGMENT_M {
        return vec![(low, high)];
    }
    let pieces = (span / RIM_SEGMENT_M).ceil().clamp(1.0, 4096.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // `pieces` is clamped to [1, 4096] before the cast.
    let count = pieces as u32;
    // Fits u16 by the clamp above, so the conversions below are exact.
    let count_f = f32::from(u16::try_from(count).unwrap_or(u16::MAX));
    let step = span / count_f;
    (0..count)
        .map(|index| {
            let index_f = f32::from(u16::try_from(index).unwrap_or(u16::MAX));
            let start = step.mul_add(index_f, low);
            let end = if index.saturating_add(1) >= count {
                high
            } else {
                step.mul_add(index_f + 1.0, low)
            };
            (start, end)
        })
        .collect()
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
    ramps: &'a [RampDef],
    stairs: &'a [StairDef],
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
            ramps: &level.ramps,
            stairs: &level.stairs,
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
    ///
    /// This is the **floor region** lookup only; it is what the floor grid,
    /// its cells and its skirts are built from, so a sloped ramp or a
    /// staircase never distorts the room's own tessellation. Use
    /// [`Self::walkable_offset_at`] for "what height does the player stand
    /// at".
    #[must_use]
    pub fn floor_offset_at(&self, x: f32, z: f32) -> f32 {
        self.region_at(x, z).map_or(0.0, FloorRegionDef::offset)
    }

    /// The last (highest-precedence) ramp covering `(x, z)`.
    #[must_use]
    pub fn ramp_at(&self, x: f32, z: f32) -> Option<&'a RampDef> {
        self.ramps.iter().rev().find(|ramp| ramp.contains(x, z))
    }

    /// The last (highest-precedence) staircase covering `(x, z)`.
    #[must_use]
    pub fn stair_at(&self, x: f32, z: f32) -> Option<&'a StairDef> {
        self.stairs.iter().rev().find(|stair| stair.contains(x, z))
    }

    /// Vertical offset of the **walkable** floor at `(x, z)` from the
    /// containing room's floor plane.
    ///
    /// A ramp wins over a staircase, and both win over a floor region: they are
    /// authored walking surfaces, and a level that overlaps them is asking for
    /// the sloped or stepped surface to be the one underfoot. With none of
    /// them, the region offset applies, and with no region the room's own floor
    /// is the walking surface (offset zero).
    #[must_use]
    pub fn walkable_offset_at(&self, x: f32, z: f32) -> f32 {
        if let Some(ramp) = self.ramp_at(x, z) {
            return ramp.offset_at(x, z);
        }
        if let Some(stair) = self.stair_at(x, z) {
            return stair.offset_at(x, z);
        }
        self.floor_offset_at(x, z)
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
    /// floor plus any ramp, staircase or floor region offset. `None` outside
    /// every room.
    #[must_use]
    pub fn floor_y_at(&self, x: f32, z: f32) -> Option<f32> {
        let room = self.room_at(x, z)?;
        let floor_y = if room.floor_y.is_finite() {
            room.floor_y
        } else {
            0.0
        };
        Some(floor_y + self.walkable_offset_at(x, z))
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

    /// Ramps whose footprint overlaps `room`, in authored order.
    #[must_use]
    pub fn ramps_for_room(&self, room: &RoomDef) -> Vec<&'a RampDef> {
        let (x0, x1, z0, z1) = room.bounds();
        self.ramps
            .iter()
            .filter(|ramp| {
                let (rx0, rx1, rz0, rz1) = ramp.bounds();
                rx1 > x0 && rx0 < x1 && rz1 > z0 && rz0 < z1
            })
            .collect()
    }

    /// Staircases whose footprint overlaps `room`, in authored order.
    #[must_use]
    pub fn stairs_for_room(&self, room: &RoomDef) -> Vec<&'a StairDef> {
        let (x0, x1, z0, z1) = room.bounds();
        self.stairs
            .iter()
            .filter(|stair| {
                let (sx0, sx1, sz0, sz1) = stair.bounds();
                sx1 > x0 && sx0 < x1 && sz1 > z0 && sz0 < z1
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

/// One ramp resolved into the walkable surface model.
///
/// It owns the ramp's [`RampSurface`] and its room's floor plane and answers
/// with `floor_y + surface.offset_at(...)`, so the controller resolves the exact
/// arithmetic the renderer used.
#[derive(Debug, Clone, Copy, PartialEq)]
struct WalkableRamp {
    surface: RampSurface,
    floor_y: f32,
}

impl WalkableRamp {
    /// World Y of the sloped surface at `(x, z)`.
    fn height_at(&self, x: f32, z: f32) -> f32 {
        self.floor_y + self.surface.offset_at(x, z)
    }
}

/// One staircase resolved into the walkable surface model.
#[derive(Debug, Clone, Copy, PartialEq)]
struct WalkableStair {
    surface: StairSurface,
    floor_y: f32,
}

impl WalkableStair {
    /// World Y of the stepped surface at `(x, z)`.
    fn height_at(&self, x: f32, z: f32) -> f32 {
        self.floor_y + self.surface.offset_at(x, z)
    }
}

/// One room of the walkable surface model.
#[derive(Debug, Clone, PartialEq)]
struct WalkableRoom {
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
    floor_y: f32,
    /// Ramps resolved against this room, in authored order (later wins).
    ramps: Vec<WalkableRamp>,
    /// Staircases resolved against this room, in authored order (later wins).
    stairs: Vec<WalkableStair>,
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
            let ramps = surfaces
                .ramps_for_room(room)
                .into_iter()
                .map(|ramp| WalkableRamp {
                    surface: ramp.surface(),
                    floor_y,
                })
                .collect();
            let stairs = surfaces
                .stairs_for_room(room)
                .into_iter()
                .map(|stair| WalkableStair {
                    surface: stair.surface(),
                    floor_y,
                })
                .collect();
            rooms.push(WalkableRoom {
                x0,
                x1,
                z0,
                z1,
                floor_y,
                ramps,
                stairs,
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
    /// [`LevelSurfaces::floor_y_at`]; inside it a ramp or staircase covering the
    /// point wins over the last authored floor region, exactly as the surface
    /// queries resolve it.
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
            if let Some(ramp) = room
                .ramps
                .iter()
                .rev()
                .find(|ramp| ramp.surface.contains(x, z))
            {
                return Some(ramp.height_at(x, z));
            }
            if let Some(stair) = room
                .stairs
                .iter()
                .rev()
                .find(|stair| stair.surface.contains(x, z))
            {
                return Some(stair.height_at(x, z));
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
