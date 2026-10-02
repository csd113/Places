use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::collision::{PLAYER_STEP_HEIGHT, WallAabb};
use crate::entities::components::StateValue;
use crate::entities::sequences::SequenceDef;
use crate::entities::spawn::{SpawnGroupDef, SpawnPointDef, SpawnTemplateDef};
use crate::entities::timers::TimerDef;
use crate::lighting::{DEFAULT_LIGHT_COLOR, LightColor};
use crate::package::binary::{Reader, Writer};
use crate::package::collision::{
    MAX_COLLISION_LADDERS, MAX_COLLISION_WATER_MATERIAL_BYTES, MAX_COLLISION_WATER_VOLUMES,
};

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
    /// No ceiling surface: an open-air room.
    ///
    /// Floors, walls, architecture, props and lights are emitted exactly like a
    /// flat room — `height` stays the open volume's eave height, so walls and
    /// fixtures resolve against it — but no ceiling batch is generated, which
    /// is what lets a level's `sky` show above an exterior. The baked lighting
    /// treats the room as a flat-ceiling volume for its fill and fixture
    /// geometry; the only environment term a sky adds to the solve is its
    /// explicit `ambient`. A ceiling-less room is an authoring choice per room:
    /// every room without it keeps its ceiling exactly as before.
    Open,
}

impl CeilingProfileDef {
    /// True for the historical flat ceiling.
    #[must_use]
    pub const fn is_flat(self) -> bool {
        matches!(self, Self::Flat)
    }

    /// True when the room declares no ceiling surface.
    #[must_use]
    pub const fn is_open(self) -> bool {
        matches!(self, Self::Open)
    }

    /// Ridge axis of a gable ceiling, `None` for a flat or open one.
    #[must_use]
    pub const fn ridge_axis(self) -> Option<WallAxis> {
        match self {
            Self::Flat | Self::Open => None,
            Self::Gable { ridge, .. } => Some(ridge),
        }
    }

    /// Ridge rise in metres, sanitised to zero for anything malformed.
    #[must_use]
    pub fn ridge_rise_m(self) -> f32 {
        match self {
            Self::Flat | Self::Open => 0.0,
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
    f32::mul_add(rise, tent, eave)
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
/// level's `defaults.floor` / `defaults.ceiling`. The per-room keys let a
/// single room's floor or ceiling be damp or
/// stained without changing the whole level.
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
    /// World Y of this room's floor plane. Omitted means `0.0`, the level's
    /// global ground plane.
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
    /// Local origin `[x, z]` of this room's ceiling tile pattern, in world
    /// X/Z. Omitted means the world origin, the unshifted tile frame.
    ///
    /// The ceiling material still tiles in world-scale metres; this only moves
    /// the phase of the pattern (and with `ceiling_tile_rotation_degrees`, its
    /// orientation), which is what lets a room with its own ceiling module
    /// place its tiles and decals without a global grid assumption.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ceiling_tile_origin: Option<[f32; 2]>,
    /// Rotation of this room's ceiling tile pattern about
    /// [`Self::ceiling_tile_origin`], in degrees, in the level's own yaw sense
    /// (0 keeps the pattern axis-aligned; +90 turns it a quarter turn).
    /// Omitted means `0.0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ceiling_tile_rotation_degrees: Option<f32>,
}

impl RoomDef {
    /// The room's ceiling tile frame: `(origin_x, origin_z, rotation_degrees)`,
    /// sanitised to the world origin / zero rotation when unauthored or
    /// non-finite.
    #[must_use]
    pub const fn ceiling_tile_frame(&self) -> (f32, f32, f32) {
        let (origin_x, origin_z) = match self.ceiling_tile_origin {
            Some([x, z]) if x.is_finite() && z.is_finite() => (x, z),
            _ => (0.0, 0.0),
        };
        let rotation = match self.ceiling_tile_rotation_degrees {
            Some(rotation) if rotation.is_finite() => rotation,
            _ => 0.0,
        };
        (origin_x, origin_z, rotation)
    }

    /// World `(x, z)` expressed in this room's ceiling tile space, in metres:
    /// the ceiling emitter tiles `tiled_uv(local_x, local_z, tile)`, and
    /// ceiling decal snapping uses the same coordinates so the two agree.
    #[must_use]
    pub fn ceiling_tile_local(&self, x: f32, z: f32) -> (f32, f32) {
        let (origin_x, origin_z, rotation) = self.ceiling_tile_frame();
        if rotation == 0.0 {
            return (x - origin_x, z - origin_z);
        }
        let radians = (-rotation).to_radians();
        let (sin, cos) = radians.sin_cos();
        let (dx, dz) = (x - origin_x, z - origin_z);
        (dz.mul_add(sin, cos * dx), dz.mul_add(cos, -(sin * dx)))
    }

    /// The world `(x, z)` of a point given in this room's ceiling tile space.
    #[must_use]
    pub fn ceiling_tile_world(&self, local_x: f32, local_z: f32) -> (f32, f32) {
        let (origin_x, origin_z, rotation) = self.ceiling_tile_frame();
        if rotation == 0.0 {
            return (origin_x + local_x, origin_z + local_z);
        }
        let radians = rotation.to_radians();
        let (sin, cos) = radians.sin_cos();
        (
            local_z.mul_add(sin, cos * local_x) + origin_x,
            local_z.mul_add(cos, -(sin * local_x)) + origin_z,
        )
    }

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

/// The footprint shape of one authored water volume.
///
/// `rect` is the historical rectangular footprint from `width` × `depth`.
/// `circle` is a disc of `radius` inscribed in its bounding box: `x`/`z` stay
/// the minimum corner of that box, so the centre is `(x + radius, z + radius)`
/// and the footprint spans `x..x + 2 * radius`. The disc is what the surface
/// draws and what the controller samples, so a circle has no invisible square
/// swimming area.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaterShape {
    /// A rectangular footprint: `width` × `depth` from the `x`/`z` corner.
    #[default]
    Rect,
    /// A circular footprint: a disc of `radius` around the bounding box centre.
    Circle,
}

/// One body of water: a footprint, a horizontal surface and the vertical
/// extent below it. The footprint is a rectangle (`shape: "rect"`, the
/// default) or a disc (`shape: "circle"`).
///
/// The volume is the authoring form of the engine's water feature. The surface
/// is drawn from the `material` (default [`DEFAULT_WATER_MATERIAL`]); the same
/// footprint and surface height are what the player controller samples to
/// decide between walking, wading and swimming, so what an author draws is
/// exactly what the player swims in.
///
/// `x`/`z` are always the minimum corner of the footprint's bounding box
/// (normalised, like a floor region): a rectangle spans `x..x + width`, and a
/// circle is the disc of `radius` inscribed in `x..x + 2 * radius`. A circle
/// may omit `width`/`depth`; if authored they must equal `2 * radius` (they
/// are the bounding box) or the volume is rejected.
///
/// `surface_y` is an absolute world height, like a fixture's `y`, so a pool
/// whose water level should sit below its deck can say so directly. `bottom_y`
/// is optional metadata for the volume's depth; the physics floor under the
/// water is always the level's own walkable floor, so a volume never needs to
/// describe geometry it does not own.
///
/// ```json
/// { "x": 8.0, "z": 10.0, "width": 12.0, "depth": 6.0,
///   "surface_y": -1.65, "bottom_y": -3.0 }
/// ```
/// ```json
/// { "shape": "circle", "x": 8.0, "z": 10.0, "radius": 2.5,
///   "surface_y": -1.65, "bottom_y": -3.0 }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaterVolumeDef {
    /// Footprint shape; defaults to the historical `rect`.
    #[serde(default)]
    pub shape: WaterShape,
    /// Minimum X of the footprint's bounding box.
    pub x: f32,
    /// Minimum Z of the footprint's bounding box.
    pub z: f32,
    /// Rectangle extent along X, `> 0`. Required for `rect`; for `circle`
    /// omitted, or exactly `2 * radius`.
    #[serde(default)]
    pub width: Option<f32>,
    /// Rectangle extent along Z, `> 0`. Required for `rect`; for `circle`
    /// omitted, or exactly `2 * radius`.
    #[serde(default)]
    pub depth: Option<f32>,
    /// Circle radius in metres, `> 0`. Required for `circle`, rejected for
    /// `rect`.
    #[serde(default)]
    pub radius: Option<f32>,
    /// World Y of the free surface.
    pub surface_y: f32,
    /// World Y of the volume's bottom; `None` resolves the lowest walkable
    /// floor under the footprint and falls back to `surface_y - 2.0`.
    #[serde(default)]
    pub bottom_y: Option<f32>,
    /// Surface material id; `None` uses [`DEFAULT_WATER_MATERIAL`].
    #[serde(default)]
    pub material: Option<String>,
    /// Effective opacity of the surface, `0.0..=1.0`. `None` uses
    /// [`DEFAULT_WATER_OPACITY`]; an authored value overrides the material.
    #[serde(default)]
    pub opacity: Option<f32>,
    /// Whether the player swims in this volume. `false` keeps it decorative:
    /// the surface draws and the player walks or falls through it normally.
    #[serde(default = "default_true")]
    pub swimming: bool,
}

const fn default_true() -> bool {
    true
}

impl WaterVolumeDef {
    /// Footprint as `(x0, x1, z0, z1)`, normalised.
    ///
    /// A rectangle's fallback `0.0` extent or a circle's fallback `0.0` radius
    /// is a malformed volume the loader rejects; resolution skips a degenerate
    /// footprint so it can never draw or answer a query.
    #[must_use]
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        let (width, depth) = match self.shape {
            WaterShape::Rect => (self.width.unwrap_or(0.0), self.depth.unwrap_or(0.0)),
            WaterShape::Circle => {
                let diameter = 2.0 * self.radius.unwrap_or(0.0);
                (diameter, diameter)
            }
        };
        (
            self.x.min(self.x + width),
            self.x.max(self.x + width),
            self.z.min(self.z + depth),
            self.z.max(self.z + depth),
        )
    }

    /// True when `(x, z)` lies inside the footprint: the rectangle, or the
    /// disc of [`WaterVolumeDef::radius`] around the bounding box centre.
    ///
    /// A circle's rim is the wall line and is **dry**: membership is strictly
    /// inside the radius, so the four axis extremes of the bounding box are not
    /// water the way a square's edges would be.
    #[must_use]
    pub fn contains(&self, x: f32, z: f32) -> bool {
        if !x.is_finite() || !z.is_finite() {
            return false;
        }
        let (x0, x1, z0, z1) = self.bounds();
        match self.shape {
            WaterShape::Rect => x >= x0 && x <= x1 && z >= z0 && z <= z1,
            WaterShape::Circle => {
                let radius = 0.5 * (x1 - x0);
                let dx = x - f32::midpoint(x0, x1);
                let dz = z - f32::midpoint(z0, z1);
                dx.mul_add(dx, dz * dz) < radius * radius
            }
        }
    }

    /// Surface material id: the authored one, or [`DEFAULT_WATER_MATERIAL`].
    #[must_use]
    pub fn material_id(&self) -> &str {
        self.material.as_deref().unwrap_or(DEFAULT_WATER_MATERIAL)
    }

    /// Effective surface opacity, sanitised to `0.0..=1.0`.
    #[must_use]
    pub const fn opacity(&self) -> f32 {
        match self.opacity {
            Some(opacity) if opacity.is_finite() => opacity.clamp(0.0, 1.0),
            _ => DEFAULT_WATER_OPACITY,
        }
    }
}

/// The material a water volume draws with when it names none.
pub const DEFAULT_WATER_MATERIAL: &str = "core:water_pool_01";

/// Effective surface opacity of a water volume that names none.
///
/// Opaque enough to read as a distinct surface, translucent enough that the
/// basin floor and walls stay visible through it.
pub const DEFAULT_WATER_OPACITY: f32 = 0.62;

/// A climbable ladder volume.
///
/// The ladder is authored as the space the player climbs through, not as the
/// prop's render mesh: a footprint, a bottom and a top world Y, and the yaw the
/// climber faces while climbing. The controller attaches when the player's
/// cylinder overlaps the footprint, the player is on the ladder's approach side
/// (behind the facing direction) and movement input points along `facing`; it
/// then climbs while that input is held. There is no climb key.
///
/// `facing_degrees` uses the same convention as camera yaw: `0` climbs towards
/// `-Z`, `90` towards `+X`, `180` towards `+Z`, `270` towards `-X`. The
/// default `0` means "climb towards -Z".
///
/// ```json
/// { "x": 19.35, "z": 11.7, "width": 0.55, "depth": 0.6,
///   "bottom_y": -3.0, "top_y": -1.5, "facing_degrees": 90.0 }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LadderDef {
    pub x: f32,
    pub z: f32,
    pub width: f32,
    pub depth: f32,
    /// World Y of the lowest climbable point (usually the floor at the base).
    pub bottom_y: f32,
    /// World Y the feet reach at the top (usually the exit surface's height).
    pub top_y: f32,
    /// Yaw the climber faces while climbing, in degrees.
    #[serde(default)]
    pub facing_degrees: f32,
}

impl LadderDef {
    /// Footprint as `(x0, x1, z0, z1)`, normalised.
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

/// One map-authored action the interaction dispatcher can perform.
///
/// Actions are a closed, typed set — never an unrestricted script. They are
/// authored inside an event binding (`on: interact`, `on: enter_volume`, ...)
/// or as a sequence step, and executed in order by the single dispatcher. An
/// unknown `action` tag is a JSON parse error, and an action the engine cannot
/// perform on its target is a named validation error, so a map can never load
/// with a silently ignored effect.
///
/// A `target` names the entity the action acts on. Every target except
/// `reset_to_start`, `spawn_entity` and `start_sequence` is `Option<String>`:
/// omitted means the acting entity (the entity that emitted the event, or the
/// entity a sequence runs on).
///
/// ```json
/// { "action": "toggle" }
/// { "action": "set_light", "target": "sauna_light", "on": false }
/// { "action": "start_sequence", "sequence": "sauna_warmup" }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ActionDef {
    /// Drive a door or movable entity to its open end.
    Open {
        /// Entity id; omitted means the acting entity.
        #[serde(default)]
        target: Option<String>,
    },
    /// Drive a door or movable entity to its closed end.
    Close {
        /// Entity id; omitted means the acting entity.
        #[serde(default)]
        target: Option<String>,
    },
    /// Flip a door, light or any entity with a toggleable capability.
    Toggle {
        /// Entity id; omitted means the acting entity.
        #[serde(default)]
        target: Option<String>,
    },
    /// Enable an entity: aiming, emission, animation and audio all resume.
    Enable {
        /// Entity id; omitted means the acting entity.
        #[serde(default)]
        target: Option<String>,
    },
    /// Disable an entity without removing it.
    Disable {
        /// Entity id; omitted means the acting entity.
        #[serde(default)]
        target: Option<String>,
    },
    /// Set a light's illumination state explicitly.
    ///
    /// The fixture's switch state is the engine's one supported runtime light
    /// control: the renderer selects the prepared switchable lightmap layers
    /// for it. There is no per-switch shader branch and no runtime rebake.
    SetLight {
        /// Light fixture or prop light entity id; omitted means the actor.
        #[serde(default)]
        target: Option<String>,
        /// True turns the light on.
        on: bool,
    },
    /// Lock a door: it refuses to open until unlocked.
    Lock {
        /// Door entity id; omitted means the actor.
        #[serde(default)]
        target: Option<String>,
    },
    /// Unlock a door.
    Unlock {
        /// Door entity id; omitted means the actor.
        #[serde(default)]
        target: Option<String>,
    },
    /// Play a named animation clip on a placed entity instance.
    ///
    /// A pose/one-shot clip (the default) holds its last pose until another
    /// action or a reset replaces it; `loop` keeps it cycling.
    PlayAnimation {
        /// Instance id of the actor; omitted means the acting instance.
        #[serde(default)]
        target: Option<String>,
        /// Clip name the animation system plays.
        #[serde(default)]
        clip: Option<String>,
        /// Repeat the clip instead of holding its last pose.
        #[serde(default, rename = "loop")]
        looped: bool,
    },
    /// Move a named clip of a placed prop to the opposite end of its timeline.
    ///
    /// The clip is *scrubbed*, not played: the runtime eases its time toward
    /// one end at a constant rate, so pressing again mid-movement reverses
    /// from the current pose without snapping or restarting at an endpoint.
    ToggleAnimation {
        /// Instance id of the actor; omitted means the acting instance.
        #[serde(default)]
        target: Option<String>,
        /// Clip name the toggle scrubs.
        #[serde(default)]
        clip: Option<String>,
    },
    /// Start a sound on an audio emitter.
    PlaySound {
        /// Entity id of the emitter; omitted means the acting entity.
        #[serde(default)]
        target: Option<String>,
        /// Sound asset id; omitted uses the emitter's authored sound.
        #[serde(default)]
        sound: Option<String>,
        /// Loop until stopped.
        #[serde(default, rename = "loop")]
        looped: bool,
    },
    /// Stop a sound on an audio emitter.
    StopSound {
        /// Entity id of the emitter; omitted means the acting entity.
        #[serde(default)]
        target: Option<String>,
    },
    /// Select a named material variant on a runtime-visual entity.
    ///
    /// The only per-object runtime material property this renderer supports is
    /// the dynamic object's emission profile, so a material variant selects an
    /// emitted-light response (a screen that turns off, a sign that lights up).
    /// Swapping the surface material of baked static geometry is not
    /// expressible at runtime and is rejected at compile time.
    ChangeMaterial {
        /// Entity id; omitted means the acting entity.
        #[serde(default)]
        target: Option<String>,
        /// Variant name declared by the entity's `material` component.
        variant: String,
    },
    /// Move a runtime entity to a world position, collision-respecting.
    MoveObject {
        /// Entity id; omitted means the acting entity.
        #[serde(default)]
        target: Option<String>,
        /// Target world X.
        x: f32,
        /// Target world Y; omitted keeps the entity's current base height.
        #[serde(default)]
        y: Option<f32>,
        /// Target world Z.
        z: f32,
        /// Movement speed in m/s; omitted uses the entity's configured speed.
        #[serde(default)]
        speed: Option<f32>,
    },
    /// Write a typed state value.
    SetState {
        /// Entity id; omitted means the acting entity.
        #[serde(default)]
        target: Option<String>,
        /// State name.
        name: String,
        /// New value.
        value: StateValue,
    },
    /// Show or hide a placed instance's floating display name.
    ToggleLabel {
        /// Instance id; omitted means the acting instance.
        #[serde(default)]
        target: Option<String>,
    },
    /// Start a named sequence on an entity.
    StartSequence {
        /// Sequence id from the level's `sequences`.
        sequence: String,
        /// Entity the sequence controls; omitted means the acting entity.
        #[serde(default)]
        target: Option<String>,
    },
    /// Stop the sequence running on an entity.
    StopSequence {
        /// Entity id; omitted means the acting entity.
        #[serde(default)]
        target: Option<String>,
    },
    /// Start or reconfigure a timer.
    StartTimer {
        /// Timer entity id; omitted means the acting entity.
        #[serde(default)]
        target: Option<String>,
        /// Period override in seconds.
        #[serde(default)]
        seconds: Option<f32>,
        /// Repeat override.
        #[serde(default)]
        repeat: Option<bool>,
    },
    /// Stop a timer without changing its period.
    StopTimer {
        /// Timer entity id; omitted means the acting entity.
        #[serde(default)]
        target: Option<String>,
    },
    /// Spawn one typed template at a point, optionally under a group.
    SpawnEntity {
        /// Spawn template id; omitted uses the point's own template.
        #[serde(default)]
        template: Option<String>,
        /// Spawn point id.
        #[serde(default)]
        point: Option<String>,
        /// Spawn group id; omitted uses the point's authored group.
        #[serde(default)]
        group: Option<String>,
        /// Runtime name for the spawned instance, so later actions can address
        /// it. Omitted derives `<point>#<n>`; a name already live fails the
        /// spawn with a diagnostic.
        #[serde(default)]
        name: Option<String>,
    },
    /// Despawn a runtime entity, a spawn group's live member, or a named spawn.
    DespawnEntity {
        /// Entity id, spawn group id or runtime name.
        target: String,
    },
    /// Return the player to the level's authored spawn and re-arm triggers.
    ResetToStart,
}

impl ActionDef {
    /// The serialized tag of this action, for diagnostics.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Open { .. } => "open",
            Self::Close { .. } => "close",
            Self::Toggle { .. } => "toggle",
            Self::Enable { .. } => "enable",
            Self::Disable { .. } => "disable",
            Self::SetLight { .. } => "set_light",
            Self::Lock { .. } => "lock",
            Self::Unlock { .. } => "unlock",
            Self::PlayAnimation { .. } => "play_animation",
            Self::ToggleAnimation { .. } => "toggle_animation",
            Self::PlaySound { .. } => "play_sound",
            Self::StopSound { .. } => "stop_sound",
            Self::ChangeMaterial { .. } => "change_material",
            Self::MoveObject { .. } => "move_object",
            Self::SetState { .. } => "set_state",
            Self::ToggleLabel { .. } => "toggle_label",
            Self::StartSequence { .. } => "start_sequence",
            Self::StopSequence { .. } => "stop_sequence",
            Self::StartTimer { .. } => "start_timer",
            Self::StopTimer { .. } => "stop_timer",
            Self::SpawnEntity { .. } => "spawn_entity",
            Self::DespawnEntity { .. } => "despawn_entity",
            Self::ResetToStart => "reset_to_start",
        }
    }

    /// The explicit target this action names, if any.
    ///
    /// `None` means the action acts on the acting entity; `Some` must resolve
    /// on its own, exactly as validation promised.
    #[must_use]
    pub fn target(&self) -> Option<&str> {
        match self {
            Self::Open { target }
            | Self::Close { target }
            | Self::Toggle { target }
            | Self::Enable { target }
            | Self::Disable { target }
            | Self::SetLight { target, .. }
            | Self::Lock { target }
            | Self::Unlock { target }
            | Self::PlayAnimation { target, .. }
            | Self::ToggleAnimation { target, .. }
            | Self::PlaySound { target, .. }
            | Self::StopSound { target }
            | Self::ChangeMaterial { target, .. }
            | Self::MoveObject { target, .. }
            | Self::SetState { target, .. }
            | Self::ToggleLabel { target }
            | Self::StopSequence { target }
            | Self::StartTimer { target, .. }
            | Self::StopTimer { target }
            | Self::StartSequence { target, .. } => target.as_deref(),
            Self::SpawnEntity { .. } | Self::DespawnEntity { .. } | Self::ResetToStart => None,
        }
    }
}

/// One typed condition a binding checks before running its actions.
///
/// A condition whose target does not exist evaluates false; the compiler
/// rejects an unknown target first, so a shipped map never relies on it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "check", rename_all = "snake_case")]
pub enum ConditionDef {
    /// The named state equals `value` exactly (same variant and value).
    State {
        /// Entity id.
        target: String,
        /// State name.
        name: String,
        /// Required value.
        equals: StateValue,
    },
    /// The target exists and is enabled.
    Enabled {
        /// Entity id.
        target: String,
    },
    /// The target exists and is disabled.
    Disabled {
        /// Entity id.
        target: String,
    },
    /// The target door is locked.
    Locked {
        /// Door entity id.
        target: String,
    },
    /// The target door is unlocked.
    Unlocked {
        /// Door entity id.
        target: String,
    },
    /// The target door is at its open end.
    DoorOpen {
        /// Door entity id.
        target: String,
    },
    /// The target door is at its closed end.
    DoorClosed {
        /// Door entity id.
        target: String,
    },
    /// A sequence is running on the target.
    SequenceRunning {
        /// Entity id.
        target: String,
    },
    /// No sequence is running on the target.
    SequenceIdle {
        /// Entity id.
        target: String,
    },
}

impl ConditionDef {
    /// The entity id this condition reads.
    #[must_use]
    pub fn target(&self) -> &str {
        match self {
            Self::State { target, .. }
            | Self::Enabled { target }
            | Self::Disabled { target }
            | Self::Locked { target }
            | Self::Unlocked { target }
            | Self::DoorOpen { target }
            | Self::DoorClosed { target }
            | Self::SequenceRunning { target }
            | Self::SequenceIdle { target } => target,
        }
    }

    /// Stable diagnostic name of the check.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::State { .. } => "state",
            Self::Enabled { .. } => "enabled",
            Self::Disabled { .. } => "disabled",
            Self::Locked { .. } => "locked",
            Self::Unlocked { .. } => "unlocked",
            Self::DoorOpen { .. } => "door_open",
            Self::DoorClosed { .. } => "door_closed",
            Self::SequenceRunning { .. } => "sequence_running",
            Self::SequenceIdle { .. } => "sequence_idle",
        }
    }
}

/// The authored name of one event kind.
///
/// The runtime [`crate::entities::events::EventKind`] is the same vocabulary
/// without the authored spelling; `EventKind::parse` maps one to the other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKindName {
    /// The player interacted with the entity.
    Interact,
    /// The player's feet entered the entity's trigger volume.
    EnterVolume,
    /// The player's feet left the entity's trigger volume.
    ExitVolume,
    /// A timer on the entity elapsed.
    Timer,
    /// One of the entity's typed states changed.
    ObjectState,
    /// A sequence on the entity completed or stopped.
    SequenceComplete,
    /// The entity was spawned.
    Spawn,
    /// An animation on the entity completed.
    AnimationComplete,
    /// An AI agent changed state.
    AiState,
    /// An AI agent caught its prey.
    Caught,
}

impl EventKindName {
    /// Stable lowercase name, matching the serialized spelling.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Interact => "interact",
            Self::EnterVolume => "enter_volume",
            Self::ExitVolume => "exit_volume",
            Self::Timer => "timer",
            Self::ObjectState => "object_state",
            Self::SequenceComplete => "sequence_complete",
            Self::Spawn => "spawn",
            Self::AnimationComplete => "animation_complete",
            Self::AiState => "ai_state",
            Self::Caught => "caught",
        }
    }
}

/// One authored event binding: `on <event>` (matching `key`) `when <conditions>`
/// run `<actions>`, at most `once` per run and no more often than
/// `cooldown_seconds`.
///
/// Bindings live on the entity that emits the event, so a map's wiring is
/// local: the switch's press lists what the press does, the volume's entry
/// lists what entering does. There is exactly one binding mechanism.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventBindingDef {
    /// Optional binding id, for diagnostics and disarm-by-name.
    #[serde(default)]
    pub id: Option<String>,
    /// The event kind this binding listens for.
    pub on: EventKindName,
    /// Optional event key filter: a timer fires with its id, a sequence with
    /// its id, an animation with its clip name. `None` accepts every key.
    #[serde(default)]
    pub key: Option<String>,
    /// Every condition must hold for the binding to run.
    #[serde(default)]
    pub when: Vec<ConditionDef>,
    /// Run at most once per run; a reset re-arms it.
    #[serde(default)]
    pub once: bool,
    /// Seconds after a run before this binding may run again.
    #[serde(default)]
    pub cooldown_seconds: f32,
    /// The actions one fire performs, in order.
    #[serde(default)]
    pub actions: Vec<ActionDef>,
}

/// Largest number of actions one event binding or one sequence step may
/// declare.
///
/// A bound, not a tuning knob: composition is allowed, but one event can never
/// fan out into unbounded work, and validation names the limit.
pub const MAX_ACTIONS_PER_SOURCE: usize = 8;

/// Largest number of bindings one entity may declare.
pub const MAX_BINDINGS_PER_ENTITY: usize = 16;

/// Prompt shown for an interaction that names none.
pub const DEFAULT_INTERACTION_PROMPT: &str = "Interact";

/// One named option of a `material` component.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialVariantDef {
    /// Variant name an action selects.
    pub name: String,
    /// Emission scale this variant selects; `0.0` makes the entity unlit.
    #[serde(default = "default_material_emission")]
    pub emission_scale: f32,
}

const fn default_material_emission() -> f32 {
    1.0
}

/// One typed component an authored entity carries.
///
/// Components are the reusable capabilities the runtime provides: an
/// `interactable` object can be aimed at, a `light` can be switched, a
/// `state` bag can be read by conditions, and so on. A component never carries
/// object-specific actions: the event bindings do that.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "component", rename_all = "snake_case")]
pub enum ComponentDef {
    /// The entity can be aimed at and interacted with.
    Interactable {
        /// Prompt shown while aimed at; defaults to
        /// [`DEFAULT_INTERACTION_PROMPT`].
        #[serde(default)]
        prompt: Option<String>,
        /// Reach in metres; defaults to
        /// [`crate::interact::DEFAULT_INTERACTION_REACH_M`] and is capped at
        /// [`crate::interact::MAX_INTERACTION_REACH_M`].
        #[serde(default)]
        reach: Option<f32>,
        /// False starts the entity disabled: it is not aimable until an
        /// `enable` action runs.
        #[serde(default = "default_true")]
        enabled: bool,
        /// Display name a `toggle_label` action shows and hides.
        #[serde(default)]
        label: Option<String>,
    },
    /// The entity plays a named clip from its model's animation set.
    Animation {
        /// Clip name.
        clip: String,
        /// Playback rate multiplier; defaults to `1.0`.
        #[serde(default = "default_animation_speed")]
        speed: f32,
        /// Loop the clip; defaults to false (hold the last pose).
        #[serde(default)]
        looped: bool,
        /// Start playing immediately; defaults to false.
        #[serde(default)]
        playing: bool,
    },
    /// The entity emits a sound.
    Audio {
        /// Sound asset id.
        sound: String,
        /// Linear gain; defaults to `1.0`.
        #[serde(default = "default_audio_gain")]
        gain: f32,
        /// Loop until stopped.
        #[serde(default)]
        looped: bool,
        /// False starts muted.
        #[serde(default = "default_true")]
        enabled: bool,
        /// Start playing immediately.
        #[serde(default)]
        playing: bool,
    },
    /// The entity is a light.
    Light {
        /// Initial illumination state; defaults to on.
        #[serde(default = "default_true")]
        enabled: bool,
        /// Whether an action may switch it.
        ///
        /// Defaults to false: only a ceiling fixture has prepared switchable
        /// lightmap layers, and validation rejects a switchable `light`
        /// component on any other record.
        #[serde(default)]
        switchable: bool,
        /// Emission scale of the visible face; defaults to `1.0`.
        #[serde(default = "default_material_emission")]
        emission_scale: f32,
    },
    /// The entity selects one of several emission profiles at runtime.
    Material {
        /// Every selectable variant; the first is the initial one when
        /// `current` is omitted.
        variants: Vec<MaterialVariantDef>,
        /// Initially selected variant; defaults to the first variant.
        #[serde(default)]
        current: Option<String>,
    },
    /// One typed state value on the entity.
    State {
        /// State name.
        name: String,
        /// Initial value.
        value: StateValue,
    },
    /// The entity removes itself after a bounded lifetime.
    Lifetime {
        /// Seconds until expiry; must be finite and positive.
        seconds: f32,
    },
    /// The entity is a steam emitter.
    Steam {
        /// Initial emitter state; defaults to on.
        #[serde(default = "default_true")]
        enabled: bool,
    },
    /// The entity is a water volume.
    Water {
        /// Initial swimming state; defaults to on.
        #[serde(default = "default_true")]
        enabled: bool,
    },
    /// Navigation agent metadata: the body this entity navigates with.
    NavAgent {
        /// Body radius in metres.
        radius: f32,
        /// Preferred speed in m/s.
        speed_mps: f32,
        /// Body height in metres; defaults to the player's standing height.
        #[serde(default = "default_nav_height")]
        height: f32,
        /// Largest surface rise the body walks up, in metres.
        #[serde(default = "default_nav_step_height")]
        step_height: f32,
        /// Largest walkable rise per metre of run.
        #[serde(default = "default_nav_max_slope")]
        max_slope: f32,
    },
    /// Navigation obstacle metadata: an explicit obstacle box for the bake.
    NavObstacle {
        /// `[width, height, depth]`; defaults to the entity's resolved size.
        #[serde(default)]
        size: Option<[f32; 3]>,
        /// False marks a purely decorative obstacle.
        #[serde(default = "default_true")]
        affects_nav: bool,
    },
    /// The entity runs a shared AI behavior.
    Ai(crate::ai::AiDef),
    /// The entity's opacity cycles on a looping authored period.
    Fade(FadeDef),
    /// One attached dynamic light on the entity.
    Glow(GlowDef),
}

/// One authored `fade` cycle or proximity fade: a looping opacity animation,
/// or an opacity driven by the player's distance from the entity.
///
/// The cycle form (the historical one):
///
/// ```json
/// { "component": "fade", "period_seconds": 6.0, "phase": 0.25,
///   "min_opacity": 0.0, "max_opacity": 1.0, "enabled": true }
/// ```
///
/// The proximity form: when `near_radius` and `far_radius` are authored, the
/// entity fades out as the player approaches and fades back in as the player
/// retreats. `x`/`z` are irrelevant here; the distance is the player's
/// horizontal distance to the entity's live position.
///
/// ```json
/// { "component": "fade", "near_radius": 3.0, "far_radius": 6.0,
///   "fade_out_seconds": 1.5, "fade_in_seconds": 3.0 }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FadeDef {
    /// Cycle length in seconds; finite, `0 < p` and at most
    /// [`MAX_FADE_PERIOD_SECONDS`]. Defaults to
    /// [`DEFAULT_FADE_PERIOD_SECONDS`]; ignored when the proximity fields are
    /// authored.
    #[serde(default = "default_fade_period")]
    pub period_seconds: f32,
    /// Cycle phase in `0..=1`; omitted resolves the deterministic
    /// per-instance phase from [`default_fade_phase`].
    #[serde(default)]
    pub phase: Option<f32>,
    /// Lowest opacity of the cycle; defaults to `0.0`.
    #[serde(default = "default_fade_min_opacity")]
    pub min_opacity: f32,
    /// Highest opacity of the cycle; defaults to `1.0`.
    #[serde(default = "default_fade_max_opacity")]
    pub max_opacity: f32,
    /// False holds [`FadeDef::max_opacity`] instead of cycling; defaults to
    /// true.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Distance in metres at which the entity begins fading out as the player
    /// approaches; finite and `> 0`. Omitted keeps the cosine cycle.
    #[serde(default)]
    pub near_radius: Option<f32>,
    /// Distance in metres beyond which the entity begins fading back in;
    /// finite and `> near_radius`. Required exactly when `near_radius` is
    /// authored.
    #[serde(default)]
    pub far_radius: Option<f32>,
    /// Seconds the fade out takes across the full opacity range; finite,
    /// `> 0` and at most [`MAX_FADE_SECONDS`]. Defaults to
    /// [`DEFAULT_FADE_OUT_SECONDS`].
    #[serde(default)]
    pub fade_out_seconds: Option<f32>,
    /// Seconds the fade in takes across the full opacity range; finite, `> 0`
    /// and at most [`MAX_FADE_SECONDS`]. Defaults to
    /// [`DEFAULT_FADE_IN_SECONDS`].
    #[serde(default)]
    pub fade_in_seconds: Option<f32>,
}

impl FadeDef {
    /// The proximity contract, when both radii are authored.
    ///
    /// The two fade times resolve their documented defaults, so a validated
    /// level always carries a complete contract.
    #[must_use]
    pub fn proximity(&self) -> Option<ProximityFade> {
        let near_radius = self.near_radius?;
        let far_radius = self.far_radius?;
        Some(ProximityFade {
            near_radius,
            far_radius,
            fade_out_seconds: self.fade_out_seconds.unwrap_or(DEFAULT_FADE_OUT_SECONDS),
            fade_in_seconds: self.fade_in_seconds.unwrap_or(DEFAULT_FADE_IN_SECONDS),
        })
    }
}

/// The proximity half of a `fade` component: two hysteresis radii and the two
/// linear fade times.
///
/// The contract is the same the runtime controller implements: the entity
/// starts fading out once the player is inside `near_radius`, starts fading in
/// only once the player is beyond `far_radius`, and holds its current
/// direction between the two, so walking across the band never flaps. A fade
/// always advances from its *current* opacity at `1 / seconds` of the
/// `min_opacity..=max_opacity` range per second, so an interrupted fade
/// reverses with no jump.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProximityFade {
    /// Distance in metres at which the entity begins fading out.
    pub near_radius: f32,
    /// Distance in metres beyond which the entity begins fading back in.
    pub far_radius: f32,
    /// Seconds the fade out takes across the full opacity range.
    pub fade_out_seconds: f32,
    /// Seconds the fade in takes across the full opacity range.
    pub fade_in_seconds: f32,
}

/// Largest authored fade period, in seconds.
///
/// A bound, not a tuning knob: a fade shorter than a frame would alias and a
/// cycle longer than an hour is a typo that reads as a stuck entity.
pub const MAX_FADE_PERIOD_SECONDS: f32 = 3600.0;

/// Largest authored proximity fade time, in seconds.
///
/// A bound, not a tuning knob: ten minutes is already slower than any ghost
/// entrance, and a longer value is a typo that reads as a stuck entity.
pub const MAX_FADE_SECONDS: f32 = 600.0;

/// Cycle length a `fade` component uses when it authors none. The cycle is
/// ignored in proximity mode, so a proximity fade may omit it.
pub const DEFAULT_FADE_PERIOD_SECONDS: f32 = 6.0;

/// How long the proximity fade out takes when it authors no time.
pub const DEFAULT_FADE_OUT_SECONDS: f32 = 1.5;

/// How long the proximity fade in takes when it authors no time.
pub const DEFAULT_FADE_IN_SECONDS: f32 = 3.0;

const fn default_fade_period() -> f32 {
    DEFAULT_FADE_PERIOD_SECONDS
}

/// One authored `glow` component: a light attached to the entity or one of
/// its animated sockets.
///
/// ```json
/// { "component": "glow", "color": [0.45, 0.95, 1.0], "intensity": 0.6,
///   "range": 3.5, "socket": "flame", "offset": [0.0, 0.1, 0.0],
///   "fade": true }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GlowDef {
    /// Linear colour; each channel finite in `0..=1`; defaults to warm white.
    #[serde(default = "default_glow_color")]
    pub color: [f32; 3],
    /// Intensity; defaults to `0.5`, finite between `0` and
    /// [`MAX_GLOW_INTENSITY`].
    #[serde(default = "default_glow_intensity")]
    pub intensity: f32,
    /// Reach in metres; defaults to `3.0`, finite between
    /// [`MIN_GLOW_RANGE_M`] and [`MAX_GLOW_RANGE_M`].
    #[serde(default = "default_glow_range")]
    pub range: f32,
    /// Animated joint/node name the light attaches to; omitted uses
    /// [`GlowDef::offset`].
    #[serde(default)]
    pub socket: Option<String>,
    /// Entity-local offset in metres; omitted is the entity origin. Each axis
    /// is finite with magnitude at most [`MAX_GLOW_OFFSET_M`].
    #[serde(default)]
    pub offset: [f32; 3],
    /// True multiplies the intensity by the entity's fade opacity; defaults
    /// to true.
    #[serde(default = "default_true")]
    pub fade: bool,
}

/// Largest authored glow intensity.
pub const MAX_GLOW_INTENSITY: f32 = 8.0;

/// Smallest authored glow range, in metres.
pub const MIN_GLOW_RANGE_M: f32 = 0.05;

/// Largest authored glow range, in metres.
pub const MAX_GLOW_RANGE_M: f32 = 64.0;

/// Largest absolute glow offset per entity-local axis, in metres.
pub const MAX_GLOW_OFFSET_M: f32 = 4.0;

const fn default_fade_min_opacity() -> f32 {
    0.0
}

const fn default_fade_max_opacity() -> f32 {
    1.0
}

const fn default_glow_color() -> [f32; 3] {
    [1.0, 0.86, 0.6]
}

const fn default_glow_intensity() -> f32 {
    0.5
}

const fn default_glow_range() -> f32 {
    3.0
}

/// Deterministic default fade phase for an instance id, in `0..1`.
///
/// The hash is FNV-1a over the id's bytes, reduced to its top 24 bits so the
/// fraction is exact in `f32` and can never reach `1.0`. Two different ids get
/// independent phases, and one id always gets the same phase, so a group of
/// ghosts desynchronises without an authored `phase` and a reload never
/// re-rolls it.
#[must_use]
pub fn default_fade_phase(instance_id: &str) -> f32 {
    const FNV_OFFSET_BASIS: u32 = 0x811c_9dc5;
    const FNV_PRIME: u32 = 0x0100_0193;
    // 2^24, exactly representable in f32.
    const DENOMINATOR: f32 = 16_777_216.0;
    let mut hash = FNV_OFFSET_BASIS;
    for byte in instance_id.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    #[allow(clippy::cast_precision_loss)] // the top 24 bits are exact in f32
    let numerator = (hash >> 8) as f32;
    numerator / DENOMINATOR
}

const fn default_animation_speed() -> f32 {
    1.0
}

const fn default_nav_height() -> f32 {
    crate::collision::PLAYER_HEIGHT
}

const fn default_nav_step_height() -> f32 {
    crate::collision::PLAYER_STEP_HEIGHT
}

const fn default_nav_max_slope() -> f32 {
    crate::nav::NAV_MAX_SLOPE
}

const fn default_audio_gain() -> f32 {
    1.0
}

/// One authored trigger volume.
///
/// The volume is an axis-aligned box: a rectangular `(x, z)` footprint and a
/// vertical `bottom_y..top_y` band. The controller tests the player's feet
/// against it every frame and emits `enter_volume` / `exit_volume` events on
/// the edges; the volume's own bindings decide what those events do. Leaving
/// the volume re-arms it.
///
/// ```json
/// { "id": "pit_hole_1", "x": 9.6, "z": -26.2, "width": 1.6, "depth": 1.6,
///   "bottom_y": -3.2, "top_y": -0.05,
///   "bindings": [{ "on": "enter_volume",
///                  "actions": [{ "action": "reset_to_start" }] }] }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TriggerVolumeDef {
    /// Stable instance id. Omitted means the deterministic default
    /// `trigger_<n>` with `n` the 1-based authored position.
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub z: f32,
    pub width: f32,
    pub depth: f32,
    /// Lowest world Y of the volume. Omitted resolves the walkable floor under
    /// the footprint centre (`0.0` outside every room).
    #[serde(default)]
    pub bottom_y: Option<f32>,
    /// Highest world Y of the volume. Omitted resolves
    /// `bottom_y + DEFAULT_TRIGGER_HEIGHT_M`.
    #[serde(default)]
    pub top_y: Option<f32>,
    /// What the volume's occupancy edges do.
    #[serde(default)]
    pub bindings: Vec<EventBindingDef>,
}

impl TriggerVolumeDef {
    /// Footprint as `(x0, x1, z0, z1)`, normalised.
    #[must_use]
    pub fn bounds(&self) -> (f32, f32, f32, f32) {
        (
            self.x.min(self.x + self.width),
            self.x.max(self.x + self.width),
            self.z.min(self.z + self.depth),
            self.z.max(self.z + self.depth),
        )
    }

    /// Resolved vertical bounds: authored values, or the level floor under the
    /// footprint centre plus [`DEFAULT_TRIGGER_HEIGHT_M`].
    #[must_use]
    pub fn resolved_y_bounds(&self, level: &LevelDef) -> (f32, f32) {
        let (x0, x1, z0, z1) = self.bounds();
        let floor = LevelSurfaces::new(level)
            .floor_y_at(f32::midpoint(x0, x1), f32::midpoint(z0, z1))
            .unwrap_or(0.0);
        let bottom = self.bottom_y.filter(|y| y.is_finite()).unwrap_or(floor);
        let top = self
            .top_y
            .filter(|y| y.is_finite())
            .unwrap_or(bottom + DEFAULT_TRIGGER_HEIGHT_M);
        (bottom, top)
    }
}

/// Default height of an area trigger whose `top_y` is omitted, in metres.
pub const DEFAULT_TRIGGER_HEIGHT_M: f32 = 2.0;

impl ComponentDef {
    /// Stable diagnostic name of this component kind.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Interactable { .. } => "interactable",
            Self::Animation { .. } => "animation",
            Self::Audio { .. } => "audio",
            Self::Light { .. } => "light",
            Self::Material { .. } => "material",
            Self::State { .. } => "state",
            Self::Lifetime { .. } => "lifetime",
            Self::Steam { .. } => "steam",
            Self::Water { .. } => "water",
            Self::NavAgent { .. } => "nav_agent",
            Self::NavObstacle { .. } => "nav_obstacle",
            Self::Ai(_) => "ai",
            Self::Fade(_) => "fade",
            Self::Glow(_) => "glow",
        }
    }
}

/// Which way a door leaf swings about its hinge, seen from above with the
/// closed leaf running from the hinge to the latch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DoorSwing {
    /// Positive rotation about the hinge: the latch moves to the hinge's left.
    #[default]
    Left,
    /// Negative rotation about the hinge: the latch moves to the hinge's right.
    Right,
}

/// A door's initial state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DoorStartState {
    /// The leaf begins closed (angle 0).
    #[default]
    Closed,
    /// The leaf begins fully open (angle `swing_degrees`).
    Open,
}

/// What a door does when its sweep meets the player or solid geometry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DoorObstruction {
    /// Hold the current angle; resume when the obstruction clears.
    #[default]
    Stop,
    /// Reverse direction once per obstruction.
    Reverse,
}

/// The door's visual build: an interior painted leaf or a sauna leaf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DoorKind {
    /// A white painted interior door with a round brass handle.
    #[default]
    Interior,
    /// A wooden-framed sauna door with a glass panel and a wooden handle.
    Sauna,
}

/// Material a door kind uses when the map authors no override.
pub struct DoorMaterials {
    /// The moving leaf's material.
    pub slab: &'static str,
    /// The static frame's material.
    pub frame: &'static str,
    /// The handle's material.
    pub handle: &'static str,
}

/// The glass a sauna leaf's panel uses.
pub const SAUNA_DOOR_GLASS_MATERIAL: &str = "core:glass_window_clear_01";

/// Liner depth a door draws when no wall opening resolves for its leaf.
///
/// A door with a resolved wall uses that wall's thickness instead, so this is
/// only the fallback for a leaf standing in geometry the frame resolver cannot
/// see (a fixture, a hand-cut hall). In metres.
pub const STANDALONE_DOOR_FRAME_DEPTH_M: f32 = 0.12;

/// Upper bound on an authored [`DoorDef::frame_depth`], in metres.
pub const MAX_DOOR_FRAME_DEPTH_M: f32 = 2.0;

/// The wall tunnel a door leaf is installed in: what the frame build needs.
///
/// The door's frame is a reveal liner that runs the whole tunnel plus casings
/// on the tunnel's two end faces, so both numbers below come from the map (or
/// from explicit authoring on the leaf) rather than from a per-level
/// constant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoorFrame {
    /// Total depth of the reveal liner along the leaf's closed normal, in
    /// metres.
    pub depth: f32,
    /// Signed offset of the tunnel's centre from the leaf's centre plane,
    /// measured along the leaf's closed normal (hinge space `+Z`), in metres.
    pub center: f32,
}

impl DoorFrame {
    /// The frame of a leaf with no resolved wall: the liner only just clears
    /// the leaf so both casings still show.
    #[must_use]
    pub fn standalone(thickness: f32) -> Self {
        Self {
            depth: STANDALONE_DOOR_FRAME_DEPTH_M.max(thickness + 0.04),
            center: 0.0,
        }
    }

    /// The tunnel's two end planes in hinge space, low `z` first.
    #[must_use]
    pub fn span(&self) -> (f32, f32) {
        let half = self.depth.abs() * 0.5;
        (self.center - half, self.center + half)
    }
}

/// The axis-aligned half-extents of a yaw-rotated rectangle's covering box.
///
/// A solid prop's collider is conservative in exactly the way its visible
/// model needs: every rotated corner is inside the box, so the player can never
/// clip a corner the model shows.
#[must_use]
fn rotated_half_extents_local(
    half_width: f32,
    half_depth: f32,
    rotation_degrees: f32,
) -> (f32, f32) {
    let (sin, cos) = rotation_degrees.to_radians().sin_cos();
    let sin = sin.abs();
    let cos = cos.abs();
    (
        half_width.mul_add(cos, half_depth * sin),
        half_width.mul_add(sin, half_depth * cos),
    )
}

/// The default material set for one door kind.
#[must_use]
pub const fn door_materials(kind: DoorKind) -> DoorMaterials {
    match kind {
        DoorKind::Interior => DoorMaterials {
            slab: "home:door_white_01",
            frame: "home:baseboard_white_01",
            handle: "core:metal_brass_01",
        },
        DoorKind::Sauna => DoorMaterials {
            slab: "home:sauna_wood_01",
            frame: "home:baseboard_wood_01",
            handle: "home:sauna_wood_01",
        },
    }
}

/// One interactive door leaf.
///
/// A door is placed by its **hinge edge**: `(x, y, z)` is the bottom of the
/// hinge jamb, `rotation_degrees` aims the closed leaf (0 runs toward `+X`,
/// 90 toward `-Z`), and the leaf extends `width` metres from the hinge along
/// that direction. `swing_degrees` and `open_direction` describe where it goes
/// when it opens. The wall opening that the door fills is authored separately
/// on the wall, exactly like any other opening; validation proves the closed
/// leaf does not start inside a solid.
///
/// ```json
/// { "id": "office_door", "x": 2.0, "y": 0.0, "z": 0.15,
///   "rotation_degrees": 0.0, "width": 0.9, "height": 2.1, "thickness": 0.045,
///   "open_direction": "left", "swing_degrees": 90.0,
///   "open_speed_degrees": 120.0, "initial_state": "closed",
///   "components": [ { "component": "interactable" } ],
///   "bindings": [ { "on": "interact", "actions": [ { "action": "toggle" } ] } ] }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DoorDef {
    /// Stable entity id: the name triggers and switches address.
    pub id: String,
    /// World X of the hinge edge.
    pub x: f32,
    /// Height of the leaf's bottom above the walkable floor under the hinge.
    #[serde(default)]
    pub y: f32,
    /// World Z of the hinge edge.
    pub z: f32,
    /// Yaw of the closed leaf in degrees (0 runs toward `+X`, 90 toward `-Z`).
    #[serde(default)]
    pub rotation_degrees: f32,
    /// Leaf width, in metres, from the hinge to the latch edge.
    pub width: f32,
    /// Leaf height, in metres.
    pub height: f32,
    /// Leaf thickness, in metres.
    #[serde(default = "default_door_thickness_m")]
    pub thickness: f32,
    /// Which way the leaf swings.
    #[serde(default)]
    pub open_direction: DoorSwing,
    /// Opening angle, in degrees. Negative means the opposite swing.
    #[serde(default = "default_door_swing_degrees")]
    pub swing_degrees: f32,
    /// Angular speed while opening, in degrees per second.
    #[serde(default = "default_door_speed_degrees")]
    pub open_speed_degrees: f32,
    /// Angular speed while closing. Defaults to `open_speed_degrees`.
    #[serde(default)]
    pub close_speed_degrees: Option<f32>,
    /// Authored starting state.
    #[serde(default)]
    pub initial_state: DoorStartState,
    /// True starts the leaf locked: it refuses to open (and reports the refusal
    /// once) until an `unlock` action runs.
    #[serde(default)]
    pub locked: bool,
    /// Typed components this leaf carries. A manually interactable door authors
    /// an `interactable` component and its own `on: "interact"` binding; an
    /// externally controlled door carries neither.
    #[serde(default)]
    pub components: Vec<ComponentDef>,
    /// What this leaf's events do.
    #[serde(default)]
    pub bindings: Vec<EventBindingDef>,
    /// What happens when the sweep is obstructed.
    #[serde(default)]
    pub obstruction: DoorObstruction,
    /// Visual build.
    #[serde(default)]
    pub kind: DoorKind,
    /// Slab material override.
    #[serde(default)]
    pub material: Option<String>,
    /// Frame material override.
    #[serde(default)]
    pub frame_material: Option<String>,
    /// Handle material override.
    #[serde(default)]
    pub handle_material: Option<String>,
    /// Explicit depth of this leaf's frame reveal liner, in metres.
    ///
    /// The frame is a liner through the wall the leaf is installed in, with a
    /// casing on each end face. That tunnel is normally resolved from the wall
    /// opening the leaf fills; a leaf whose visible tunnel is built by
    /// something the resolver cannot see (a façade doorway panel in front of
    /// the wall, for example) authors the real depth here. `None` keeps the
    /// resolved wall.
    #[serde(default)]
    pub frame_depth: Option<f32>,
    /// Explicit offset of the frame liner's centre from the leaf's centre
    /// plane, along the leaf's closed normal, in metres.
    ///
    /// The sign follows hinge space: a leaf whose tunnel is mostly on its
    /// `+Z` side authors a positive value. `None` centres the liner on the
    /// resolved wall.
    #[serde(default)]
    pub frame_center: Option<f32>,
}

const fn default_door_thickness_m() -> f32 {
    0.045
}
const fn default_door_swing_degrees() -> f32 {
    90.0
}
const fn default_door_speed_degrees() -> f32 {
    120.0
}

impl DoorDef {
    /// Resolved close speed: the authored value, else the open speed.
    #[must_use]
    pub fn close_speed(&self) -> f32 {
        self.close_speed_degrees
            .filter(|speed| speed.is_finite() && *speed > 0.0)
            .unwrap_or(self.open_speed_degrees)
    }

    /// Signed swing in degrees: positive for a left-hand swing.
    #[must_use]
    pub fn signed_swing(&self) -> f32 {
        match self.open_direction {
            DoorSwing::Left => self.swing_degrees,
            DoorSwing::Right => -self.swing_degrees,
        }
    }

    /// The door's base world Y: the walkable floor under the hinge plus `y`.
    #[must_use]
    pub fn base_y(&self, level: &LevelDef) -> f32 {
        LevelSurfaces::new(level)
            .floor_y_at(self.x, self.z)
            .unwrap_or(0.0)
            + self.y
    }

    /// The closed leaf's direction in world `(x, z)`: a unit vector.
    #[must_use]
    pub fn closed_direction(&self) -> (f32, f32) {
        let yaw = self.rotation_degrees.to_radians();
        (yaw.cos(), -yaw.sin())
    }

    /// The leaf's current direction at `angle_degrees` of opening.
    #[must_use]
    pub fn direction_at(&self, angle_degrees: f32) -> (f32, f32) {
        let yaw = (self.rotation_degrees + angle_degrees).to_radians();
        (yaw.cos(), -yaw.sin())
    }

    /// The closed leaf's normal in world `(x, z)`: a unit vector along hinge
    /// space `+Z`, the direction the frame's liner is measured along.
    #[must_use]
    pub fn closed_normal(&self) -> (f32, f32) {
        let yaw = self.rotation_degrees.to_radians();
        (yaw.sin(), yaw.cos())
    }
}

impl LevelDef {
    /// The wall tunnel this leaf is installed in.
    ///
    /// A door is placed by its hinge and its wall opening is authored
    /// separately, so the two are matched here: the walls whose footprint
    /// contains the closed leaf's midpoint and whose walk-through opening
    /// overlaps the leaf span decide the tunnel's depth and its offset from the
    /// leaf plane. An explicit [`DoorDef::frame_depth`] /
    /// [`DoorDef::frame_center`] overrides the resolved value, which is how a
    /// leaf whose visible reveal is built by a prop (an outdoor doorway panel)
    /// gets a frame that reaches the surface the player actually sees.
    ///
    /// The result is stable for a given level and leaf: walls are searched in
    /// authored order and the first match wins.
    #[must_use]
    pub fn door_frame(&self, def: &DoorDef) -> DoorFrame {
        let mut frame = self
            .resolved_door_wall_frame(def)
            .unwrap_or_else(|| DoorFrame::standalone(def.thickness));
        if let Some(depth) = def
            .frame_depth
            .filter(|value| value.is_finite() && *value > 0.0)
        {
            frame.depth = depth;
        }
        if let Some(center) = def.frame_center.filter(|value| value.is_finite()) {
            frame.center = center;
        }
        frame
    }

    /// The frame resolved from the wall opening the leaf fills, if one matches.
    fn resolved_door_wall_frame(&self, def: &DoorDef) -> Option<DoorFrame> {
        let (dir_x, dir_z) = def.closed_direction();
        let (normal_x, normal_z) = def.closed_normal();
        let half = def.width * 0.5;
        let mid = (dir_x.mul_add(half, def.x), dir_z.mul_add(half, def.z));
        let leaf_end = (
            dir_x.mul_add(def.width, def.x),
            dir_z.mul_add(def.width, def.z),
        );
        for wall in &self.walls {
            let (x0, x1) = (
                wall.x.min(wall.x + wall.width),
                wall.x.max(wall.x + wall.width),
            );
            let (z0, z1) = (
                wall.z.min(wall.z + wall.depth),
                wall.z.max(wall.z + wall.depth),
            );
            let margin = 0.05;
            if mid.0 < x0 - margin
                || mid.0 > x1 + margin
                || mid.1 < z0 - margin
                || mid.1 > z1 + margin
            {
                continue;
            }
            let axis = wall.axis();
            let (leaf_lo, leaf_hi) = match axis {
                WallAxis::X => (def.x.min(leaf_end.0), def.x.max(leaf_end.0)),
                WallAxis::Z => (def.z.min(leaf_end.1), def.z.max(leaf_end.1)),
            };
            // The opening must admit the leaf: a walk-through kind that the
            // leaf's own base stands at or above. A raised doorway whose sill
            // meets a raised floor (the pool-side sauna leaf is the maintained
            // example) still frames the leaf; a window or a high vent does not.
            let leaf_base = def.base_y(self);
            let filled = wall.openings.iter().any(|opening| {
                if !opening.is_door() {
                    return false;
                }
                if opening.bottom(wall.y) > leaf_base + 1e-3 {
                    return false;
                }
                let (open_lo, open_hi) = match axis {
                    WallAxis::X => (x0 + opening.offset, x0 + opening.end()),
                    WallAxis::Z => (z0 + opening.offset, z0 + opening.end()),
                };
                let overlap = leaf_hi.min(open_hi) - leaf_lo.max(open_lo);
                overlap >= def.width.min(opening.width) * 0.5
            });
            if !filled {
                continue;
            }
            // The wall's extent along the leaf's normal is the tunnel depth;
            // its centre's signed distance from the hinge is the liner offset.
            let depth = normal_x
                .abs()
                .mul_add(wall.width.abs(), normal_z.abs() * wall.depth.abs());
            if !depth.is_finite() || depth <= 0.0 {
                continue;
            }
            let wall_mid = (f32::midpoint(x0, x1), f32::midpoint(z0, z1));
            let center = (wall_mid.0 - def.x).mul_add(normal_x, (wall_mid.1 - def.z) * normal_z);
            return Some(DoorFrame {
                depth,
                center: if center.is_finite() { center } else { 0.0 },
            });
        }
        None
    }
}

impl Default for DoorDef {
    fn default() -> Self {
        Self {
            id: String::new(),
            x: 0.0,
            y: 0.0,
            z: 0.0,
            rotation_degrees: 0.0,
            width: 0.9,
            height: 2.1,
            thickness: default_door_thickness_m(),
            open_direction: DoorSwing::default(),
            swing_degrees: default_door_swing_degrees(),
            open_speed_degrees: default_door_speed_degrees(),
            close_speed_degrees: None,
            initial_state: DoorStartState::default(),
            locked: false,
            components: Vec::new(),
            bindings: Vec::new(),
            obstruction: DoorObstruction::default(),
            kind: DoorKind::default(),
            material: None,
            frame_material: None,
            handle_material: None,
            frame_depth: None,
            frame_center: None,
        }
    }
}

/// One localized ambient effect emitter.
///
/// Effects are presentation-only: they never collide, never occlude and are
/// not part of the lighting bake. The only current kind is `steam`, a bounded
/// plume of drifting translucent billboards.
///
/// ```json
/// { "kind": "steam", "x": 4.0, "y": 0.0, "z": 2.0,
///   "width": 1.2, "depth": 0.8, "height": 1.8,
///   "count": 24, "size": 0.35, "drift": 0.25 }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectDef {
    /// Optional stable id, for diagnostics and future actions.
    #[serde(default)]
    pub id: Option<String>,
    /// Effect kind. Only `steam` exists.
    pub kind: String,
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default)]
    pub z: f32,
    /// Emitter footprint width along X, in metres.
    #[serde(default = "default_effect_footprint_m")]
    pub width: f32,
    /// Emitter footprint depth along Z, in metres.
    #[serde(default = "default_effect_footprint_m")]
    pub depth: f32,
    /// Plume height above the emitter, in metres.
    #[serde(default = "default_effect_height_m")]
    pub height: f32,
    /// Particle budget. Bounded by [`MAX_EFFECT_PARTICLES`].
    #[serde(default = "default_effect_count")]
    pub count: u32,
    /// Particle billboard size, in metres.
    #[serde(default = "default_effect_size_m")]
    pub size: f32,
    /// Horizontal wander amplitude, in metres.
    #[serde(default)]
    pub drift: f32,
    /// Seconds one particle takes to cross the plume.
    #[serde(default = "default_effect_lifetime_s")]
    pub lifetime_seconds: f32,
    /// Material used for the billboards. Defaults to the built-in steam
    /// material.
    #[serde(default)]
    pub material: Option<String>,
    /// Initial emitter state; defaults to on.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// What this emitter's events do.
    #[serde(default)]
    pub bindings: Vec<EventBindingDef>,
}

const fn default_effect_footprint_m() -> f32 {
    0.8
}
const fn default_effect_height_m() -> f32 {
    1.6
}
const fn default_effect_count() -> u32 {
    24
}
const fn default_effect_size_m() -> f32 {
    0.35
}
const fn default_effect_lifetime_s() -> f32 {
    3.0
}

/// The material an effect uses when it names none.
pub const DEFAULT_STEAM_MATERIAL: &str = "core:steam_01";

/// The only effect kind the engine implements.
pub const EFFECT_KIND_STEAM: &str = "steam";

/// Hard cap on one effect's particle count.
pub const MAX_EFFECT_PARTICLES: u32 = 128;
/// Hard cap on the effects one level may declare.
pub const MAX_LEVEL_EFFECTS: usize = 64;

/// Collision thickness of a glazed opening that authors `solid`: the visible
/// pane is a zero-thickness surface, so the physical slab needs a small,
/// invisible depth to be a stable collider.
pub const OPENING_GLASS_THICKNESS_M: f32 = 0.06;

/// First material index a switchable fixture's luminous face uses.
///
/// A fixture face normally binds its family's shared sheet material, because
/// every fixture of one family draws identically. A `switchable` fixture needs
/// its face to turn off on its own, so it gets its own material identity in a
/// slot after the family sheets; see [`fixture_face_material_index`].
///
/// Equal to the fixture family count (asserted by a test so the two tables
/// cannot drift). 32 bits wide like every other material/sheet slot; the
/// highest slot a level can reach is this base plus the level's last fixture
/// index, which stays far inside [`MAX_LEVEL_MATERIALS`].
pub const FIXTURE_SWITCHABLE_MATERIAL_BASE: u32 = 4;

/// Material index of one ceiling fixture's luminous face.
///
/// `fixture_index` is the fixture's position in `ceiling_lights`; a
/// non-switchable fixture uses its family's shared sheet (every fixture of the
/// family draws one material), while a switchable fixture gets the stable slot
/// `FIXTURE_SWITCHABLE_MATERIAL_BASE + fixture_index`, so its runtime on/off
/// state never touches another fixture's face. The fixture-sheet table the
/// renderer uploads is padded to cover every index this returns.
#[must_use]
pub fn fixture_face_material_index(
    fixture_index: usize,
    switchable: bool,
    kind: crate::lighting::FixtureKind,
) -> u32 {
    if switchable {
        FIXTURE_SWITCHABLE_MATERIAL_BASE
            .saturating_add(u32::try_from(fixture_index).unwrap_or(u32::MAX))
    } else {
        u32::try_from(kind.index()).unwrap_or(0)
    }
}

/// Hard cap on the doors one level may declare.
///
/// A door draws its frame and its leaf as two dynamic objects (the moving
/// geometry path), and one level's dynamic scene is bounded by
/// [`crate::render::MAX_DYNAMIC_OBJECTS`]. This cap reserves room in that
/// budget for the level's other dynamic content (floating props, the
/// demonstration drum) while keeping a door-heavy level's draw count sane.
pub const MAX_LEVEL_DOORS: u64 = 24;
/// Largest door dimension (width, height or thickness) in metres.
pub const MAX_DOOR_DIMENSION_M: f32 = 12.0;
/// Smallest meaningful opening swing, in degrees.
pub const MIN_DOOR_SWING_DEGREES: f32 = 5.0;
/// Largest opening swing, in degrees.
pub const MAX_DOOR_SWING_DEGREES: f32 = 179.0;
/// Largest angular door speed, in degrees per second.
pub const MAX_DOOR_SPEED_DEGREES: f32 = 720.0;

/// One authored animation route for a placed entity instance.
///
/// A route is a bounded, ordered list of steps the runtime plays through
/// against the collision world: walk to a waypoint, turn, wait, or play a
/// clip. Routes are authored by placed-instance id, exactly like interactions
/// and labels, so two copies of one model run independently.
///
/// ```json
/// { "id": "rat_1", "loop": true,
///   "steps": [
///     { "step": "move_to", "x": 18.0, "z": 6.0, "speed": 0.35 },
///     { "step": "move_to", "x": 22.0, "z": 9.0, "speed": 1.2 },
///     { "step": "wait", "seconds": 1.0 }
///   ] }
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityRouteDef {
    /// Instance id of the placed prop/entity this route drives. Required: a
    /// route with no id could never resolve to a character.
    #[serde(default)]
    pub id: String,
    /// Restart from step 0 after the last step completes.
    #[serde(default, rename = "loop")]
    pub looped: bool,
    /// The steps, in order; between 1 and [`MAX_ROUTE_STEPS`].
    pub steps: Vec<RouteStepDef>,
}

/// One step of an [`EntityRouteDef`].
///
/// The JSON tag is `step`, so an unknown step kind is a parse error and a
/// malformed step can never be silently skipped.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "snake_case")]
pub enum RouteStepDef {
    /// Walk in a straight line to a floor waypoint at `speed` m/s.
    MoveTo { x: f32, z: f32, speed: f32 },
    /// Turn in place to `yaw_degrees` (0 faces `+Z`, matching placements).
    Face { yaw_degrees: f32 },
    /// Stand still for `seconds`.
    Wait { seconds: f32 },
    /// Play a named clip for `seconds`; `loop` repeats it, otherwise a
    /// one-shot holds its last pose.
    Play {
        clip: String,
        seconds: f32,
        #[serde(default, rename = "loop")]
        looped: bool,
    },
}

/// Hard ceiling on the routes one level may declare.
///
/// A route drives one placed entity through a bounded step list; the runtime
/// samples them per frame, so this is an authoring bound rather than a
/// rendering budget.
pub const MAX_LEVEL_ROUTES: u64 = 256;
/// Largest number of steps one route may declare.
///
/// The bound keeps one route a short authored sequence, never an open-ended
/// script.
pub const MAX_ROUTE_STEPS: usize = 64;
/// Fastest authored route speed, in metres per second.
///
/// The character path re-skins on the CPU and the animation stride is tuned
/// for walking and running; a faster route would be a teleporting prop, not a
/// creature, so the loader refuses it.
pub const MAX_ROUTE_SPEED_MPS: f32 = 6.0;
/// Longest authored wait, in seconds.
pub const MAX_ROUTE_WAIT_SECONDS: f32 = 3600.0;
/// Longest authored clip playback, in seconds.
pub const MAX_ROUTE_PLAY_SECONDS: f32 = 3600.0;

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

    /// Vertical offset of the **walking** surface at `(x, z)`: the line
    /// through the flight's nosings, relative to the containing room's floor.
    ///
    /// The line meets every nosing at the height of the tread it fronts --
    /// at run fraction `f` it is `offset_y + rise * min(f + 1/steps, 1)` --
    /// and is level across the top tread, so it ends exactly on the far floor.
    /// A player following it always stands between the tread underfoot and the
    /// one ahead: never below the rendered tread, never above the next one.
    /// Walking onto the flight from the room floor is still the one real riser
    /// of the first step; every tread boundary inside the flight is continuous.
    #[must_use]
    pub fn pitch_offset_at(&self, x: f32, z: f32) -> f32 {
        if self.steps == 0 {
            return self.offset_y;
        }
        #[allow(clippy::cast_precision_loss)] // step counts are bounded by validation
        let count = self.steps as f32;
        let climbed = (self.fraction_at(x, z) + 1.0 / count).min(1.0);
        self.rise.mul_add(climbed, self.offset_y)
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
        let (x0, x1, z0, z1) = self.bounds();
        let (origin, end) = match self.axis() {
            WallAxis::X => (x0, x1),
            WallAxis::Z => (z0, z1),
        };
        // Every shared tread/riser boundary must use the same operation. Adding
        // one depth to the previous start rounds differently from the next
        // tread's fused multiply-add, leaving tiny cracks in the shadow mesh.
        let boundary = |step: u32| {
            if step >= self.steps {
                end
            } else {
                #[allow(clippy::cast_precision_loss)] // step counts are bounded by validation
                let step = step as f32;
                self.tread_depth().mul_add(step, origin)
            }
        };
        (boundary(index), boundary(index.saturating_add(1)))
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
}

/// A rectangular cutout through a wall's thickness: doorway, window, passage, vent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WallOpeningDef {
    /// Opening type: "door", "window", "passage" or "vent". An unknown kind loads
    /// as a generic opening, so a future kind never breaks an older fixture.
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
    /// a bare hole.
    #[serde(default)]
    pub glass: Option<String>,
    /// Per-surface shine override for [`Self::glass`]'s material.
    #[serde(default)]
    pub glass_shine: Option<f32>,
    /// Whether the glazed opening physically blocks the player.
    ///
    /// Rendering and collision are independent: a `blend` glass draws
    /// transparently in the translucent pass, and this flag decides whether the
    /// pane is also a solid slab. `true` requires [`Self::glass`] — an
    /// invisible solid barrier is a wall, not an opening. The shipped maps mark
    /// windows and glass walls `solid: true`; a purely decorative pane authors
    /// `false` (the default).
    #[serde(default)]
    pub solid: bool,
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

// ---------------------------------------------------------------------------
// Round architecture: arc walls and circular pillars
// ---------------------------------------------------------------------------

/// Default tessellation of a full 360° round primitive, in segments.
///
/// Deliberately matching the fixed archway vocabulary: low-poly, smooth
/// enough to read as a curve and cheap enough for the baked lightmap.
pub const ROUND_SEGMENTS_DEFAULT: u32 = 24;
/// Fewest segments a round primitive may declare.
pub const ROUND_SEGMENTS_MIN: u32 = 3;
/// Most segments a round primitive may declare.
pub const ROUND_SEGMENTS_MAX: u32 = 128;
/// Default wall thickness of an arc wall, in metres.
pub const DEFAULT_ARC_WALL_THICKNESS_M: f32 = 0.3;
/// Default sweep of an arc wall, in degrees.
pub const DEFAULT_ARC_SWEEP_DEGREES: f32 = 90.0;
/// How many axis-aligned collision sub-boxes each rendered arc wall segment
/// contributes.
///
/// An AABB around a curved segment over-covers its ring at the diagonals;
/// splitting the segment keeps that slack to a few centimetres while collision
/// still comes from the same radii, base and top the emitter draws.
pub const ARC_COLLISION_STEPS: usize = 4;
/// How many concentric radial bands a circular pillar's collision ring is
/// split into.
///
/// One axis-aligned box per rendered segment sub-step per band, so the
/// collision and occluder silhouette follows the tessellated polygon instead
/// of spanning a full-width row. Together the bands cover the whole disc (the
/// innermost starts at the centre), which is what supports a player standing
/// anywhere on the cap, while every box stays within the polygon's own sagitta
/// of the circle at the rim.
pub const PILLAR_COLLISION_BANDS: usize = 2;
/// Largest angular span one pillar collision box may cover, in degrees.
///
/// A single axis-aligned box around a wedge over-covers at the box's diagonal
/// corners: the corner that combines one boundary's greatest X extent with the
/// other's greatest Z extent reaches about `radius * (1 + 0.0086 * span)` at
/// 45 degrees. Keeping every sub-step at no more than three degrees bounds
/// that slack to under three percent of the radius — tighter than the row
/// decomposition it replaced — and never emits more than a couple of hundred
/// boxes per pillar whatever the tessellation.
pub const PILLAR_COLLISION_MAX_SPAN_DEGREES: f32 = 3.0;
/// Hard ceiling on the number of arc walls a level may define.
///
/// Raised to 4000 from 1000: an arc wall is tessellated into at most
/// [`MAX_ROUND_SEGMENTS`](crate::level::round_segments) quads per band and is
/// otherwise ordinary static geometry, so the count bound is the same kind of
/// authoring budget as the wall cap.
pub const MAX_LEVEL_ARC_WALLS: u64 = 4_000;
/// Hard ceiling on the number of circular pillars a level may define.
///
/// Raised to 8000 from 2000, matching the column budget: a pillar is a
/// tessellated round body with a bounded per-primitive segment count, so the
/// count is an authoring bound, not a per-frame cost.
pub const MAX_LEVEL_PILLARS: u64 = 8_000;

/// World `(x, z)` of a point at `angle_degrees` around `(origin_x, origin_z)`.
///
/// The angle uses the level's compass yaw convention: `0` points −Z (north)
/// from the origin, `+90` points +X (east), `+180` +Z (south). Increasing
/// angles therefore sweep north → east → south → west seen from above.
#[must_use]
pub fn round_point(origin_x: f32, origin_z: f32, radius: f32, angle_degrees: f32) -> (f32, f32) {
    let radians = angle_degrees.to_radians();
    (
        radius.mul_add(radians.sin(), origin_x),
        (-radius).mul_add(radians.cos(), origin_z),
    )
}

/// The segment count a round primitive resolves to.
///
/// The authored count when it is inside the supported range, otherwise a
/// default that follows the sweep so a small arc stays cheap and a full ring
/// stays smooth.
#[must_use]
pub fn round_segments_for(sweep_degrees: f32, authored: Option<u32>) -> u32 {
    if let Some(segments) = authored
        && (ROUND_SEGMENTS_MIN..=ROUND_SEGMENTS_MAX).contains(&segments)
    {
        return segments;
    }
    let sweep = if sweep_degrees.is_finite() {
        sweep_degrees.abs()
    } else {
        360.0
    };
    let fraction = sweep / 360.0;
    let desired = fraction.mul_add(
        f32::from(u16::try_from(ROUND_SEGMENTS_DEFAULT).unwrap_or(u16::MAX)),
        0.0,
    );
    let max = f32::from(u16::try_from(ROUND_SEGMENTS_MAX).unwrap_or(u16::MAX));
    let count = clamped_ceil_u64(desired, max);
    u32::try_from(count.clamp(u64::from(ROUND_SEGMENTS_MIN), u64::from(ROUND_SEGMENTS_MAX)))
        .unwrap_or(ROUND_SEGMENTS_DEFAULT)
}

/// A data-authored arc wall: a curved wall slab on a circular plan.
///
/// The piece is placed by the **centre of its circle** (`x`, `z`), like a
/// pillar, and spans `start_degrees` .. `start_degrees + sweep_degrees` around
/// that centre at the **centreline radius**. Its solid is the ring between
/// `radius - thickness/2` and `radius + thickness/2`, `height` metres above its
/// base. Any catalog material id may be named; the inner face (concave, facing
/// the circle's centre), the outer face (convex), the top/bottom caps and the
/// two radial ends each take their own optional override, all falling back to
/// `material`.
///
/// Geometry, collision and the lightmap occluders are generated from the same
/// interpretation: the renderer draws the resolved segments and the collision
/// uses the same segment boxes, so a curved wall is exactly as solid as it
/// looks and never a rectangle around its bounding circle. See
/// [`ArcWallDef::collision_boxes`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArcWallDef {
    /// World X of the arc circle's centre.
    pub x: f32,
    /// World Z of the arc circle's centre.
    pub z: f32,
    /// Centreline radius, in metres (`> 0`).
    pub radius: f32,
    /// Absolute world Y of the wall base. Omitted means the walkable floor
    /// under the arc's mid-span centreline point.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    /// Wall thickness across the ring, in metres (`> 0`, `< 2 × radius`).
    #[serde(default = "default_arc_wall_thickness")]
    pub thickness: f32,
    /// Height above the base; omitted follows the local ceiling at every
    /// segment, exactly like a wall without an authored height.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    /// Compass angle of the wall's first end, in degrees (0 = north, +90 =
    /// east).
    #[serde(default)]
    pub start_degrees: f32,
    /// Signed sweep around the circle in degrees; positive sweeps
    /// north → east → south → west. Non-zero, at most 360.
    #[serde(default = "default_arc_sweep_degrees")]
    pub sweep_degrees: f32,
    /// Tessellation across the whole sweep; defaults to a 24-segment full
    /// circle scaled to the sweep. Between 3 and 128 when authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segments: Option<u32>,
    /// Face material for both length faces, the caps and the ends; falls back
    /// to `defaults.wall`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    /// Per-surface shine override for [`Self::material`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shine: Option<f32>,
    /// Material of the concave inner face; falls back to [`Self::material`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_material: Option<String>,
    /// Per-surface shine override for [`Self::inner_material`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inner_shine: Option<f32>,
    /// Material of the convex outer face; falls back to [`Self::material`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outer_material: Option<String>,
    /// Per-surface shine override for [`Self::outer_material`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outer_shine: Option<f32>,
    /// Material of the top cap; falls back to [`Self::material`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap_material: Option<String>,
    /// Per-surface shine override for [`Self::cap_material`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap_shine: Option<f32>,
    /// Material of the two radial ends; falls back to [`Self::material`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_material: Option<String>,
    /// Per-surface shine override for [`Self::end_material`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_shine: Option<f32>,
}

const fn default_arc_wall_thickness() -> f32 {
    DEFAULT_ARC_WALL_THICKNESS_M
}

const fn default_arc_sweep_degrees() -> f32 {
    DEFAULT_ARC_SWEEP_DEGREES
}

impl ArcWallDef {
    /// Radius of the concave face.
    #[must_use]
    pub const fn inner_radius(&self) -> f32 {
        self.thickness.mul_add(-0.5, self.radius)
    }

    /// Radius of the convex face.
    #[must_use]
    pub const fn outer_radius(&self) -> f32 {
        self.thickness.mul_add(0.5, self.radius)
    }

    /// Segment count this arc resolves to.
    #[must_use]
    pub fn resolved_segments(&self) -> u32 {
        round_segments_for(self.sweep_degrees, self.segments)
    }

    /// True when the arc closes on itself: a full ring with no radial ends.
    #[must_use]
    pub fn is_full_ring(&self) -> bool {
        self.sweep_degrees.is_finite() && self.sweep_degrees.abs() >= 360.0 - 1.0e-3
    }

    /// World `(x, z)` of the centreline at `fraction` of the sweep.
    #[must_use]
    pub fn centreline_point(&self, fraction: f32) -> (f32, f32) {
        let angle = self
            .sweep_degrees
            .mul_add(fraction.clamp(0.0, 1.0), self.start_degrees);
        round_point(self.x, self.z, self.radius, angle)
    }

    /// World Y of the base, resolved against the level's floors when the level
    /// does not author one.
    #[must_use]
    pub fn base_y(&self, surfaces: &LevelSurfaces<'_>) -> f32 {
        if let Some(y) = self.y.filter(|value| value.is_finite()) {
            return y;
        }
        let (x, z) = self.centreline_point(0.5);
        surfaces.floor_y_at(x, z).unwrap_or(0.0)
    }

    /// World Y of the wall's top at `(x, z)`: the authored height, else the
    /// local ceiling, with the historical fallback height when the ceiling is
    /// unusable.
    #[must_use]
    pub fn top_y_at(&self, surfaces: &LevelSurfaces<'_>, x: f32, z: f32) -> f32 {
        let base = self.base_y(surfaces);
        if let Some(height) = self
            .height
            .filter(|value| value.is_finite() && *value > 0.0)
        {
            return base + height;
        }
        let ceiling = surfaces.ceiling_y_at(x, z);
        if ceiling.is_finite() && ceiling > base {
            ceiling
        } else {
            base + DEFAULT_CEILING_HEIGHT_M
        }
    }

    /// The arc's solid as one collision box per rendered segment.
    ///
    /// This is the same interpretation the emitter draws: the segment's four
    /// plan corners at the inner and outer radius, from the wall base to the
    /// segment's top. A curved wall therefore blocks where it is drawn — it is
    /// never one oversized rectangle around the whole sweep.
    #[must_use]
    pub fn collision_boxes(&self, surfaces: &LevelSurfaces<'_>) -> Vec<ArchitectureBox> {
        let segments = self.resolved_segments();
        let inner = self.inner_radius();
        let outer = self.outer_radius();
        let base = self.base_y(surfaces);
        let count = f32::from(u16::try_from(segments).unwrap_or(u16::MAX));
        let steps = f32::from(u16::try_from(ARC_COLLISION_STEPS).unwrap_or(u16::MAX));
        let mut boxes = Vec::with_capacity(
            usize::try_from(segments)
                .unwrap_or(0)
                .saturating_mul(ARC_COLLISION_STEPS),
        );
        for index in 0..segments {
            let index_f = f32::from(u16::try_from(index).unwrap_or(u16::MAX));
            for step in 0..ARC_COLLISION_STEPS {
                let step_f = f32::from(u16::try_from(step).unwrap_or(u16::MAX));
                let f0 = (index_f + step_f / steps) / count;
                let last = index.saturating_add(1) >= segments
                    && step.saturating_add(1) >= ARC_COLLISION_STEPS;
                let f1 = if last {
                    1.0
                } else {
                    (index_f + (step_f + 1.0) / steps) / count
                };
                let a0 = self.sweep_degrees.mul_add(f0, self.start_degrees);
                let a1 = self.sweep_degrees.mul_add(f1, self.start_degrees);
                let corners = [
                    round_point(self.x, self.z, inner, a0),
                    round_point(self.x, self.z, outer, a0),
                    round_point(self.x, self.z, outer, a1),
                    round_point(self.x, self.z, inner, a1),
                ];
                let (min_x, max_x) = corners
                    .iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), (x, _)| {
                        (lo.min(*x), hi.max(*x))
                    });
                let (min_z, max_z) = corners
                    .iter()
                    .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), (_, z)| {
                        (lo.min(*z), hi.max(*z))
                    });
                let (x0, z0) = round_point(self.x, self.z, self.radius, a0);
                let (x1, z1) = round_point(self.x, self.z, self.radius, a1);
                let top = self
                    .top_y_at(surfaces, x0, z0)
                    .max(self.top_y_at(surfaces, x1, z1));
                if let Some(boxed) =
                    ArchitectureBox::from_corners([min_x, base, min_z], [max_x, top, max_z])
                {
                    boxes.push(boxed);
                }
            }
        }
        boxes
    }

    /// Material reference of the inner (concave) face.
    #[must_use]
    pub fn inner_ref(&self) -> Option<MaterialRef<'_>> {
        self.inner_material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.inner_shine))
            .or_else(|| self.material_ref())
    }

    /// Material reference of the outer (convex) face.
    #[must_use]
    pub fn outer_ref(&self) -> Option<MaterialRef<'_>> {
        self.outer_material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.outer_shine))
            .or_else(|| self.material_ref())
    }

    /// Material reference of the top cap.
    #[must_use]
    pub fn cap_ref(&self) -> Option<MaterialRef<'_>> {
        self.cap_material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.cap_shine))
            .or_else(|| self.material_ref())
    }

    /// Material reference of the radial ends.
    #[must_use]
    pub fn end_ref(&self) -> Option<MaterialRef<'_>> {
        self.end_material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.end_shine))
            .or_else(|| self.material_ref())
    }

    /// Body material reference, if it overrides `defaults.wall`.
    #[must_use]
    pub fn material_ref(&self) -> Option<MaterialRef<'_>> {
        self.material
            .as_deref()
            .map(|id| MaterialRef::with_shine(id, self.shine))
    }
}

/// A data-authored circular pillar: a solid round post.
///
/// Placed by its centre `(x, z)` with a solid radius. `height` is authored or
/// follows the local clear ceiling, exactly like a square `columns[]` entry,
/// and the body, cap and any visible bottom take ordinary material ids. The
/// rendered polygon is the same one collision and the lightmap occluders use:
/// [`PillarDef::collision_boxes`] decomposes it into a ring of per-segment
/// radial-band boxes, so the piece never collides as one oversized square
/// around its bounding circle and a baked shadow around it stays round.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PillarDef {
    /// World X of the pillar's centre.
    pub x: f32,
    /// World Z of the pillar's centre.
    pub z: f32,
    /// Solid radius, in metres (`> 0`).
    pub radius: f32,
    /// Absolute world Y of the base. Omitted means the walkable floor under
    /// the centre.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f32>,
    /// Height above the base; omitted means the local clear ceiling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f32>,
    /// Tessellation of the full circle; defaults to
    /// [`ROUND_SEGMENTS_DEFAULT`]. Between 3 and 128 when authored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segments: Option<u32>,
    /// Body material id; falls back to `defaults.wall`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub material: Option<String>,
    /// Per-surface shine override for [`Self::material`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shine: Option<f32>,
    /// Top cap material id; falls back to [`Self::material`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap_material: Option<String>,
    /// Per-surface shine override for [`Self::cap_material`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap_shine: Option<f32>,
}

impl PillarDef {
    /// Segment count this pillar resolves to.
    #[must_use]
    pub fn resolved_segments(&self) -> u32 {
        round_segments_for(360.0, self.segments)
    }

    /// The rendered polygon's world `(x, z)` vertices, in sweep order.
    #[must_use]
    pub fn polygon_points(&self) -> Vec<(f32, f32)> {
        let segments = self.resolved_segments();
        let count = f32::from(u16::try_from(segments).unwrap_or(u16::MAX));
        (0..segments)
            .map(|index| {
                let index_f = f32::from(u16::try_from(index).unwrap_or(u16::MAX));
                let angle = 360.0 * (index_f / count);
                round_point(self.x, self.z, self.radius, angle)
            })
            .collect()
    }

    /// World Y of the base, resolved against the level's floors when the level
    /// does not author one.
    #[must_use]
    pub fn base_y(&self, surfaces: &LevelSurfaces<'_>) -> f32 {
        if let Some(y) = self.y.filter(|value| value.is_finite()) {
            return y;
        }
        surfaces.floor_y_at(self.x, self.z).unwrap_or(0.0)
    }

    /// World Y of the pillar's top: the authored height, else the local clear
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
        let ceiling = surfaces.ceiling_y_at(self.x, self.z);
        if ceiling.is_finite() && ceiling > base {
            ceiling
        } else {
            base + DEFAULT_CEILING_HEIGHT_M
        }
    }

    /// The pillar's solid as one collision box per rendered segment sub-step
    /// per concentric radial band.
    ///
    /// The bands tile the disc, so their union covers the drawn polygon
    /// (including the centre, which supports a player standing on the cap) and
    /// no box ever spans a full-width row of the plan. Each rendered segment
    /// is split until its sub-steps are no wider than
    /// [`PILLAR_COLLISION_MAX_SPAN_DEGREES`], which keeps every diagonal box
    /// corner within a fraction of a percent of the radius — so the collision
    /// and occluder silhouette reads round instead of square, and a baked
    /// shadow around the pillar stays round too. The same decomposition is what
    /// [`LevelDef::architecture_solids`] hands the lighting bake.
    #[must_use]
    pub fn collision_boxes(&self, surfaces: &LevelSurfaces<'_>) -> Vec<ArchitectureBox> {
        let points = self.polygon_points();
        let len = points.len();
        if len < 3 {
            return Vec::new();
        }
        let base = self.base_y(surfaces);
        let top = self.top_y(surfaces);
        let segments = self.resolved_segments();
        let count = f32::from(u16::try_from(segments).unwrap_or(u16::MAX));
        let band_count = f32::from(u16::try_from(PILLAR_COLLISION_BANDS).unwrap_or(u16::MAX));
        // A whole segment spans `360 / segments` degrees; each is split again
        // until every sub-step is at most `PILLAR_COLLISION_MAX_SPAN_DEGREES`.
        let segment_span = 360.0 / count;
        let steps = u32::try_from(clamped_ceil_u64(
            segment_span / PILLAR_COLLISION_MAX_SPAN_DEGREES,
            64.0,
        ))
        .unwrap_or(1)
        .max(1);
        let steps_f = f32::from(u16::try_from(steps).unwrap_or(u16::MAX));
        let mut boxes = Vec::with_capacity(
            len.saturating_mul(PILLAR_COLLISION_BANDS)
                .saturating_mul(usize::try_from(steps).unwrap_or(1)),
        );
        for (index, start) in points.iter().enumerate() {
            let next = if index.saturating_add(1) >= len {
                0
            } else {
                index.saturating_add(1)
            };
            let Some(end) = points.get(next) else {
                continue;
            };
            let index_f = f32::from(u16::try_from(index).unwrap_or(u16::MAX));
            for step in 0..steps {
                let step_fraction = f32::from(u16::try_from(step).unwrap_or(u16::MAX));
                let fraction0 = (index_f + step_fraction / steps_f) / count;
                let fraction1 = (index_f + (step_fraction + 1.0) / steps_f) / count;
                let angle0 = 360.0 * fraction0;
                let angle1 = 360.0 * fraction1;
                let first_step = step == 0;
                let last_step = step.saturating_add(1) >= steps;
                for band in 0..PILLAR_COLLISION_BANDS {
                    let band_f = f32::from(u16::try_from(band).unwrap_or(u16::MAX));
                    let inner_radius = self.radius * (band_f / band_count);
                    let outer_radius = self.radius * ((band_f + 1.0) / band_count);
                    // The outermost band's first and last corners are exactly
                    // the polygon's own vertices, so the coverage test and the
                    // drawn edge agree to the last bit; interior sub-step
                    // boundaries reuse the same angles at both radii.
                    let outer = band.saturating_add(1) >= PILLAR_COLLISION_BANDS;
                    let corner_start = if outer && first_step {
                        *start
                    } else {
                        round_point(self.x, self.z, outer_radius, angle0)
                    };
                    let corner_end = if outer && last_step {
                        *end
                    } else {
                        round_point(self.x, self.z, outer_radius, angle1)
                    };
                    let inner_start = round_point(self.x, self.z, inner_radius, angle0);
                    let inner_end = round_point(self.x, self.z, inner_radius, angle1);
                    let corners = [inner_start, corner_start, corner_end, inner_end];
                    let min_x = corners
                        .iter()
                        .fold(f32::INFINITY, |low, corner| low.min(corner.0));
                    let max_x = corners
                        .iter()
                        .fold(f32::NEG_INFINITY, |high, corner| high.max(corner.0));
                    let min_z = corners
                        .iter()
                        .fold(f32::INFINITY, |low, corner| low.min(corner.1));
                    let max_z = corners
                        .iter()
                        .fold(f32::NEG_INFINITY, |high, corner| high.max(corner.1));
                    if let Some(boxed) =
                        ArchitectureBox::from_corners([min_x, base, min_z], [max_x, top, max_z])
                    {
                        boxes.push(boxed);
                    }
                }
            }
        }
        boxes
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
        let shape = x.mul_add(-x, 1.0).max(0.0).sqrt();
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

/// How a ceiling decal resolves its horizontal position at level load.
///
/// Ceiling artwork is a world-space material tile, but the tile frame is not
/// necessarily the world origin: a room may author its own
/// `ceiling_tile_origin` and `ceiling_tile_rotation_degrees`, and this enum is
/// how a decal opts into that frame. The default `None` keeps the authored
/// coordinates and rotation exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum DecalAlign {
    /// Keep the authored `x`/`z` and rotation exactly (the default).
    #[default]
    None,
    /// Snap the centre onto the nearest ceiling tile centre of the ceiling
    /// material above the decal, in that room's ceiling tile frame, and
    /// compose the room's tile rotation into the decal's in-plane rotation.
    /// Only meaningful for `surface: "ceiling"`; every other surface keeps the
    /// authored coordinates.
    CeilingGrid,
}

impl DecalAlign {
    /// True for the default, so serde can skip the key.
    #[must_use]
    pub const fn is_none(self) -> bool {
        matches!(self, Self::None)
    }
}

/// `skip_serializing_if` helper: the default align is never written, so a
/// round-tripped level keeps its serialized content byte-identical.
///
/// Takes a reference because that is serde's `skip_serializing_if` contract;
/// the type is a one-byte enum, and the signature is not ours to choose.
#[must_use]
#[allow(clippy::trivially_copy_pass_by_ref)]
const fn decal_align_is_none(align: &DecalAlign) -> bool {
    align.is_none()
}

/// Largest decal edge the loader accepts, in metres.
///
/// Decals are surface decoration, not architecture; anything larger than a
/// normal sign or floor marking is almost certainly a malformed level rather
/// than an intentional overlay.
pub const MAX_DECAL_SIZE_M: f32 = 10.0;
/// Hard ceiling on the number of decals a level may place.
///
/// Raised to 20 000 from 5000: one decal is one quad in the decal pass, drawn
/// through the same per-sheet batching as every other range, and the extended
/// capacity fixture validates 20 000 decals across a handful of sheets.
pub const MAX_LEVEL_DECALS: u64 = 20_000;
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
    /// How the decal resolves its horizontal position. See [`DecalAlign`];
    /// omitted keeps the historical authored coordinates.
    #[serde(default, skip_serializing_if = "decal_align_is_none")]
    pub align: DecalAlign,
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
/// The level's `ceiling_lights` array holds every fixture, including
/// wall-mounted ones, which author `"mount": "wall"` plus a world-space `y`.
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

/// How a ceiling fixture resolves its horizontal position at level load.
///
/// Ceiling appearance is a world-space material tile: the sheet repeats every
/// `tile_metres` from the world origin, and the artwork paints its panel grid
/// at `grid_metres` (four 1 m panels inside the office's 2 m tile, for
/// example). A fluorescent panel that does not sit on the panel grid crosses
/// the T-bar, which reads as a misplaced fitting rather than a built one — so
/// grid alignment is the default and snaps a panel's centre to the nearest
/// panel centre. Levels that want exact authored coordinates (an off-grid
/// installation, a prop-backed look) author `"align": "none"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FixtureAlign {
    /// Snap onto the ceiling material's tile grid (the default).
    #[default]
    Grid,
    /// Keep the authored `x`/`z` exactly.
    None,
}

/// Ceiling light fixture placement.
///
/// `brightness` is the optional fixture intensity/power. Omitted means `1.0`.
///
/// `color` is the optional emitted light colour as an `[r, g, b]` array of
/// `0.0..=1.0` fractions. It drives the coloured illumination the bake applies
/// to surrounding geometry; the fixture's visible face is texture-first and is
/// never tinted by it. An omitted colour emits [`DEFAULT_LIGHT_COLOR`], the
/// restrained warm fluorescent the game's lighting model is calibrated around.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightFixtureDef {
    /// Stable per-instance id, distinct from the shared `fixture` catalog id.
    ///
    /// Omitted means the deterministic default `<fixture short name>_<n>`,
    /// counted like [`PropDef::id`]. The id names the fixture for duplicate
    /// validation and for `toggle` actions on [`Self::switchable`] fixtures.
    #[serde(default)]
    pub id: Option<String>,
    pub fixture: String,
    pub x: f32,
    pub z: f32,
    #[serde(default)]
    pub rotation_degrees: f32,
    #[serde(default)]
    pub brightness: Option<f32>,
    /// Emitted light colour; omitted means [`DEFAULT_LIGHT_COLOR`].
    ///
    /// The colour is a property of the *illumination*: it tints the baked
    /// light and its local pool. It never repaints the fixture's visible face,
    /// which is texture-first — the catalog sheet defines the fixture's own
    /// colour and the face carries only a neutral emission brightness.
    #[serde(default)]
    pub color: Option<LightColor>,
    /// Horizontal alignment applied at load; omitted means
    /// [`FixtureAlign::Grid`].
    ///
    /// Grid alignment only moves grid-panel fixtures (the office fluorescent
    /// family) under a flat ceiling; it snaps `x`/`z` to the ceiling
    /// material's visible panel centres and leaves every other field
    /// untouched. See [`LevelDef::align_ceiling_fixtures`].
    #[serde(default)]
    pub align: FixtureAlign,
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
    /// Whether a map action can switch this fixture on and off at runtime.
    ///
    /// A switchable fixture is excluded from its room's baked *baseline*
    /// illumination and contributes only its local pool; toggling it then
    /// re-fills exactly the affected lightmap charts, and its visible face
    /// turns off with it. A fixture that is not switchable is baked once and
    /// never changes — the default, and the cheapest.
    #[serde(default)]
    pub switchable: bool,
    /// What this fixture's events do.
    #[serde(default)]
    pub bindings: Vec<EventBindingDef>,
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

/// A prop participates in baked light occlusion unless it opts out.
const fn default_prop_occludes() -> bool {
    true
}

/// A placed prop / furniture / appliance instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropDef {
    /// Stable per-instance id, distinct from the shared `model`/catalog id.
    ///
    /// Omitted means the deterministic default `<model short name>_<n>`, where
    /// `n` counts placements sharing that short name in array order (so the
    /// first `core:plant` is `plant_1`). An explicit id wins and must be unique
    /// across the level's whole instance-id namespace.
    #[serde(default)]
    pub id: Option<String>,
    /// Name shown by a `toggle_label` action. Omitted means the `model` id.
    #[serde(default)]
    pub display_name: Option<String>,
    /// Registry identifier, e.g. "core:couch". Resolved through the prop catalog.
    pub model: String,
    #[serde(default)]
    pub x: f32,
    /// Vertical offset of the prop's base above the local walkable floor (the
    /// containing room's `floor_y` plus any floor region). Negative values sink
    /// the prop into the floor (intentional). A room whose floor is at world
    /// Y `0.0` makes this the prop's absolute world Y.
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
    /// Whether this placement contributes its model's silhouette to the baked
    /// lighting as an occluder. Defaults to true, the historical behaviour.
    ///
    /// Decorative scenery made of alpha-cutout cards (grass tufts, a tree's
    /// leaf canopy) sets this false: the bake derives coarse solid boxes from
    /// triangles and would otherwise turn thousands of blade cards into solid
    /// shadow volumes. Collision is unaffected — that is `solid` plus level
    /// `size` — and the renderer's drawn geometry is unaffected too.
    #[serde(default = "default_prop_occludes")]
    pub occludes: bool,
    /// Typed components this instance carries — `interactable`, `animation`,
    /// `light`, `state`, `audio`, and so on. Omitted means the prop is scenery
    /// and carries no capabilities.
    #[serde(default)]
    pub components: Vec<ComponentDef>,
    /// What this instance's events do. See [`EventBindingDef`].
    #[serde(default)]
    pub bindings: Vec<EventBindingDef>,
    /// Generic light sources this object owns, positioned in its local frame.
    ///
    /// Zero by default: an object glows only through its material unless a
    /// light is authored here. Nothing about the object's model, material or
    /// category decides whether it lights a room.
    #[serde(default)]
    pub lights: Vec<LightDef>,
    /// Bounded, water-driven motion for a prop that floats (a pool toy, a
    /// buoy). Omitted means the prop stands where it was placed.
    #[serde(default)]
    pub float: Option<PropFloatDef>,
}

/// Bounded, water-driven motion of a floating prop.
///
/// The prop follows the water surface at its authored `(x, z)` with no
/// horizontal drift; `bob` and `heel` are authored amplitudes, never
/// accumulated deltas, and the level validator proves the whole swept
/// footprint stays inside one water volume, so the hull can never touch the
/// rim. State is per instance: each placed float owns its own phase.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PropFloatDef {
    /// Metres of hull below the waterline at rest. Required, `> 0`.
    #[serde(default)]
    pub draft: f32,
    /// Peak vertical displacement above/below the calm waterline, metres.
    #[serde(default)]
    pub bob: f32,
    /// Bob period, seconds; defaults to [`DEFAULT_FLOAT_PERIOD_S`].
    #[serde(default = "default_float_period")]
    pub bob_seconds: f32,
    /// Peak roll about the model's forward axis, degrees.
    #[serde(default)]
    pub heel_degrees: f32,
    /// Heel period, seconds; defaults to [`DEFAULT_FLOAT_PERIOD_S`].
    #[serde(default = "default_float_period")]
    pub heel_seconds: f32,
    /// Phase offset in `0..=1`. Omitted uses a deterministic golden-ratio
    /// phase per placement, so two floats never bob in lockstep.
    #[serde(default)]
    pub phase: Option<f32>,
}

/// Default period of a float's authored bob and heel, in seconds.
pub const DEFAULT_FLOAT_PERIOD_S: f32 = 2.4;

/// Largest heel a floating prop may author, in degrees.
pub const MAX_FLOAT_HEEL_DEGREES: f32 = 45.0;

/// Largest number of floating props one level may declare.
pub const MAX_LEVEL_FLOAT_PROPS: usize = 32;

const fn default_float_period() -> f32 {
    DEFAULT_FLOAT_PERIOD_S
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

/// One intent annotation for the map geometry checker.
///
/// A plan rectangle where a named **heuristic** finding is deliberate — an
/// open-plan edge between two rooms, a carpet hole, a pit — and must not be
/// reported. The checker reads these; the runtime ignores them. An annotation
/// never suppresses a confirmed defect and only covers its own rectangle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeometryIntentDef {
    /// Check id to suppress (`room-leak`, `missing-wall`, `ghost-collider`,
    /// `curve-coarse`, ...); omitted suppresses every heuristic check inside
    /// the rectangle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
    /// Minimum corner of the annotation's footprint, like a wall.
    pub x: f32,
    pub z: f32,
    pub width: f32,
    pub depth: f32,
    /// Why the space is intentional. Free text for the report.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl GeometryIntentDef {
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

    /// True when this annotation covers `(x, z)` for `check`.
    #[must_use]
    pub fn covers(&self, check: &str, x: f32, z: f32) -> bool {
        let (x0, x1, z0, z1) = self.bounds();
        x >= x0
            && x <= x1
            && z >= z0
            && z <= z1
            && self.check.as_deref().is_none_or(|name| name == check)
    }
}

/// The only level format version the engine reads.
///
/// The engine is pre-release and there is exactly one current schema; a level
/// whose `format_version` differs is rejected by name rather than migrated.
pub const LEVEL_FORMAT_VERSION: u32 = 3;

/// Largest accepted sky brightness multiplier.
pub const MAX_SKY_BRIGHTNESS: f32 = 4.0;

/// Largest accepted sky ambient radiance.
pub const MAX_SKY_AMBIENT: f32 = 1.0;

/// A level's optional night-sky background.
///
/// `texture` names a catalog `texture` asset whose PNG is an equirectangular
/// 2:1 sheet (U wraps around the horizon, V runs pole to pole). `brightness`
/// scales the sheet at draw time and nothing else: a starry sheet at
/// `brightness` 1.0 is the authored artwork, not an exposure. `ambient` is the
/// one term that can reach baked lighting: it is the radiance an escaping ray
/// sees in the prepared (lightmap) solve, so the night sky can add a faint
/// directional-free fill to an exterior without any fixture. Both are
/// independent of each other and of the level's real lights.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkyDef {
    /// Catalog texture id of the equirectangular sheet.
    pub texture: String,
    /// Visual brightness multiplier, between 0.0 and
    /// [`MAX_SKY_BRIGHTNESS`].
    #[serde(default = "default_sky_brightness")]
    pub brightness: f32,
    /// Radiance an escaping ray sees in the prepared solve, between 0.0 and
    /// [`MAX_SKY_AMBIENT`]. Zero (the default) leaves the solve exactly as it
    /// was: an escaping ray contributes nothing.
    #[serde(default)]
    pub ambient: f32,
}

/// One-to-one brightness: the sheet is the authored exposure.
const fn default_sky_brightness() -> f32 {
    1.0
}

// ---------------------------------------------------------------------------
// Regional fog
// ---------------------------------------------------------------------------

/// Hard ceiling on the number of fog regions a level may declare.
///
/// The renderer uploads a fixed-size uniform array of this many regions, so the
/// cap is a shader-budget bound rather than a parse bound: a level above it is
/// genuinely outside the verified envelope. The quality presets upload only a
/// prefix of the authored list (see `render::common::atmosphere`).
pub const MAX_FOG_REGIONS: usize = 16;

/// Largest per-metre extinction one fog region may author.
///
/// A regional layer is weather, not a blackout: at 0.5 per metre a 10 m view
/// is already `1 - exp(-25)` opaque, and the global atmosphere is meant to
/// stay readable through the layer.
pub const MAX_FOG_REGION_DENSITY: f32 = 0.5;

/// Longest fog region id the loader accepts, in characters.
pub const MAX_FOG_REGION_ID_CHARS: usize = 64;

/// Default horizontal soft edge of a fog region, in metres.
///
/// The authored box is the region's full-extinction core; the last
/// [`DEFAULT_FOG_FALLOFF_M`] metres before a side face ramp the contribution
/// down, so a region reads as a rolling layer rather than a cut-out.
pub const DEFAULT_FOG_FALLOFF_M: f32 = 2.0;

/// One authored regional fog volume.
///
/// A region is a world-space box that thickens the air *inside* it: the
/// contribution is `density * horizontal_edge_factor * vertical_factor` per
/// fragment. The horizontal factor ramps from zero at the box side to one
/// `falloff_m` inside it; the vertical factor is one at and below `ground_y`
/// and fades linearly to zero at `top_y`. Regions never sum: the greatest
/// effective contribution wins (ties keep the lowest authoring index) and the
/// global atmosphere's own density is added to the winner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FogRegionDef {
    /// Stable authoring id, required, non-empty, at most
    /// [`MAX_FOG_REGION_ID_CHARS`] characters and unique in the level.
    pub id: String,
    /// Minimum world corner of the box.
    pub min: [f32; 3],
    /// Maximum world corner of the box; strictly above `min` on every axis.
    pub max: [f32; 3],
    /// Extinction per metre inside the region; at most
    /// [`MAX_FOG_REGION_DENSITY`].
    pub density: f32,
    /// Colour the region mixes towards, each channel `0.0..=1.0`. Omitted uses
    /// the global fog colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<[f32; 3]>,
    /// Horizontal soft edge inside the box, in metres. Omitted is
    /// [`DEFAULT_FOG_FALLOFF_M`]; `0.0` is a hard edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub falloff_m: Option<f32>,
    /// World Y the full-density ground layer starts at. Omitted is `min.y`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ground_y: Option<f32>,
    /// World Y the density has faded to zero at. Omitted is `max.y`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_y: Option<f32>,
}

// ---------------------------------------------------------------------------
// Void walls
// ---------------------------------------------------------------------------

/// Hard ceiling on the number of void walls a level may declare.
///
/// Each box is six quads (twelve for `faces: "both"`) and at most one occluder
/// box, so this bounds the emitted geometry and the bake's extra blockers
/// without a new data structure.
pub const MAX_VOID_WALLS: usize = 256;

/// Longest void wall id the loader accepts, in characters.
pub const MAX_VOID_WALL_ID_CHARS: usize = 64;

/// Upper bound on the quads one void wall emits: six faces, two quads each for
/// `faces: "both"`.
pub const MAX_VOID_WALL_QUADS: u64 = 12;

/// Which of a void wall box's faces are emitted, and which way their normals
/// point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoidWallFaces {
    /// The box seen from inside it: every emitted face's normal points into
    /// the box (a shell around a space the camera stands in).
    #[default]
    Inward,
    /// The box seen from outside it: every emitted face's normal points away
    /// from the box (a slab or plate the camera looks at).
    Outward,
    /// Both directions: two quads per face, one per direction.
    Both,
}

impl VoidWallFaces {
    /// The default, `inward`, so serde can skip the key.
    #[must_use]
    pub const fn is_inward(self) -> bool {
        matches!(self, Self::Inward)
    }
}

/// `skip_serializing_if` helper: the default `inward` facing is never written,
/// so a level that omits it round-trips byte-identically.
///
/// Takes a reference because that is serde's `skip_serializing_if` contract;
/// the type is a one-byte enum, and the signature is not ours to choose.
#[must_use]
#[allow(clippy::trivially_copy_pass_by_ref)]
const fn void_wall_faces_is_inward(faces: &VoidWallFaces) -> bool {
    faces.is_inward()
}

/// `skip_serializing_if` helper: a default-true flag is never written.
///
/// Takes a reference because that is serde's `skip_serializing_if` contract.
#[must_use]
#[allow(clippy::trivially_copy_pass_by_ref)]
const fn bool_is_true(value: &bool) -> bool {
    *value
}

/// One opaque void wall / floor box: a real surface slab that hides the void
/// the level did not build.
///
/// Unlike a `wall` slab, a void wall is not tied to a room, its footprint can
/// lie anywhere in the world, and each of its box faces is emitted with an
/// explicitly authored normal ([`VoidWallFaces`]). It is ordinary opaque
/// geometry: it blocks sight because it is drawn, it is fogged exactly once
/// like every other surface, and it shades through the same material pipeline.
/// It is never a fade or a black overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoidWallDef {
    /// Optional authoring id; non-empty, at most [`MAX_VOID_WALL_ID_CHARS`]
    /// characters and unique when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Minimum world corner of the box.
    pub min: [f32; 3],
    /// Maximum world corner of the box; strictly above `min` on every axis.
    pub max: [f32; 3],
    /// Level material id every face draws.
    pub material: String,
    /// Which faces are emitted and which way their normals point.
    #[serde(default, skip_serializing_if = "void_wall_faces_is_inward")]
    pub faces: VoidWallFaces,
    /// Whether the authored box collides, exactly like a solid prop's `size`
    /// box. Defaults to true.
    #[serde(default = "default_true", skip_serializing_if = "bool_is_true")]
    pub solid: bool,
    /// Whether the authored box joins the baked-light occluder set exactly
    /// like a solid prop's occluder boxes. Defaults to true.
    #[serde(default = "default_true", skip_serializing_if = "bool_is_true")]
    pub occludes: bool,
}

/// One planar face of a void wall box: the quad's corners in winding order
/// (its front points along `normal`) and its unit normal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoidWallFace {
    /// Corners in winding order.
    pub points: [[f32; 3]; 4],
    /// Outward unit normal of the emitted quad.
    pub normal: [f32; 3],
}

impl VoidWallDef {
    /// The box's two corners, normalised so `min` is the low corner.
    #[must_use]
    pub const fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        (
            [
                self.min[0].min(self.max[0]),
                self.min[1].min(self.max[1]),
                self.min[2].min(self.max[2]),
            ],
            [
                self.min[0].max(self.max[0]),
                self.min[1].max(self.max[1]),
                self.min[2].max(self.max[2]),
            ],
        )
    }

    /// The box as a solid volume, or `None` when it has no usable extent.
    #[must_use]
    pub fn resolved_box(&self) -> Option<ArchitectureBox> {
        ArchitectureBox::from_corners(self.min, self.max)
    }

    /// The box's faces as drawn, in a fixed order: the three negative faces
    /// (-X, -Y, -Z) then the three positive ones (+X, +Y, +Z), each with its
    /// authored facing.
    ///
    /// `inward` and `outward` return one quad per face (6 total); `both`
    /// returns the outward set then the inward set (12 total). Inward quads
    /// are the same rectangles with reversed winding, and their `normal`
    /// points into the box.
    #[must_use]
    pub fn faces(&self) -> Vec<VoidWallFace> {
        let (min, max) = self.bounds();
        let outward = outward_faces(min, max);
        match self.faces {
            VoidWallFaces::Outward => outward.to_vec(),
            VoidWallFaces::Inward => outward.iter().map(reversed).collect(),
            VoidWallFaces::Both => {
                let mut faces: Vec<VoidWallFace> = outward.to_vec();
                faces.extend(outward.iter().map(reversed));
                faces
            }
        }
    }
}

/// A face with the same rectangle wound the other way and its normal negated.
fn reversed(face: &VoidWallFace) -> VoidWallFace {
    let [a, b, c, d] = face.points;
    VoidWallFace {
        points: [a, d, c, b],
        normal: [-face.normal[0], -face.normal[1], -face.normal[2]],
    }
}

/// The six outward-wound faces of an axis-aligned box.
///
/// Every quad's front (`p0 -> p1 -> p2` right-hand rule) points along its own
/// outward normal, matching the winding the box-like architecture emitters
/// already use.
#[must_use]
const fn outward_faces(min: [f32; 3], max: [f32; 3]) -> [VoidWallFace; 6] {
    let [x0, y0, z0] = min;
    let [x1, y1, z1] = max;
    [
        VoidWallFace {
            points: [[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]],
            normal: [-1.0, 0.0, 0.0],
        },
        VoidWallFace {
            points: [[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]],
            normal: [0.0, -1.0, 0.0],
        },
        VoidWallFace {
            points: [[x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]],
            normal: [0.0, 0.0, -1.0],
        },
        VoidWallFace {
            points: [[x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]],
            normal: [1.0, 0.0, 0.0],
        },
        VoidWallFace {
            points: [[x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]],
            normal: [0.0, 1.0, 0.0],
        },
        VoidWallFace {
            points: [[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]],
            normal: [0.0, 0.0, 1.0],
        },
    ]
}

/// The level definition: rooms, geometry, props, fixtures and interactions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelDef {
    pub format_version: u32,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub author: String,
    /// Optional night-sky background. Omitted keeps the historical behaviour:
    /// no background object, and the surface clear colour wherever geometry
    /// does not cover a pixel. The sky never illuminates anything by itself —
    /// its optional `ambient` term is the only path from the sheet to baked
    /// light, and it is a separate, explicit authoring choice.
    #[serde(default)]
    pub sky: Option<SkyDef>,
    /// Optional regional fog volumes, in authoring order.
    ///
    /// Each region thickens the air inside its own world-space box. Empty on
    /// every level that does not ask for one, and omitted from the serialized
    /// level entirely, so a fog-less level round-trips byte-identically.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fog_regions: Vec<FogRegionDef>,
    /// Every room section, in ownership order.
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
    /// raised platforms).
    #[serde(default)]
    pub floor_regions: Vec<FloorRegionDef>,
    /// Rectangular bodies of water: surface, footprint and swimming contract.
    #[serde(default)]
    pub water: Vec<WaterVolumeDef>,
    /// Climbable ladder volumes: footprint, vertical reach and the direction
    /// the climber faces.
    #[serde(default)]
    pub ladders: Vec<LadderDef>,
    /// Straight sloped walking surfaces (ramps).
    #[serde(default)]
    pub ramps: Vec<RampDef>,
    /// Straight stepped walking surfaces (staircases).
    #[serde(default)]
    pub stairs: Vec<StairDef>,
    /// Solid half-height walls: partitions, parapets and knee walls.
    #[serde(default)]
    pub half_walls: Vec<HalfWallDef>,
    /// Solid square or rectangular columns/posts.
    #[serde(default)]
    pub columns: Vec<ColumnDef>,
    /// Opaque void wall / floor boxes: real surfaces that hide the void.
    ///
    /// Omitted from the serialized level when empty, so a level without one
    /// round-trips byte-identically.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub void_walls: Vec<VoidWallDef>,
    /// Data-authored arc (curved) walls on a circular plan.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arc_walls: Vec<ArcWallDef>,
    /// Solid circular pillars.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pillars: Vec<PillarDef>,
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
    /// The key holds every fixture, including wall-mounted ones, which author
    /// `"mount": "wall"` plus a world-space `y`. A fixture is visible geometry
    /// that owns one generic light; lights attached to props live on the prop
    /// instead (see [`PropDef::lights`]).
    #[serde(default)]
    pub ceiling_lights: Vec<LightFixtureDef>,
    /// Placed props / furniture / appliances.
    #[serde(default)]
    pub props: Vec<PropDef>,
    /// Interactive door leaves with their own state machine, collision and
    /// map-wireable actions.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub doors: Vec<DoorDef>,
    /// Localized ambient effects (sauna steam and future emitters).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effects: Vec<EffectDef>,
    /// Authored trigger volumes: entering or leaving emits an event.
    #[serde(default)]
    pub volumes: Vec<TriggerVolumeDef>,
    /// Authored timers: one entity each, whose `on: "timer"` bindings run when
    /// the timer fires.
    #[serde(default)]
    pub timers: Vec<TimerDef>,
    /// Authored sequences: ordered steps an entity runs as one operation.
    #[serde(default)]
    pub sequences: Vec<SequenceDef>,
    /// Authored spawn templates: typed prefabs an action can instantiate.
    #[serde(default)]
    pub spawn_templates: Vec<SpawnTemplateDef>,
    /// Authored spawn points: where a template appears.
    #[serde(default)]
    pub spawn_points: Vec<SpawnPointDef>,
    /// Authored spawn groups: at-most-one-active rules for spawn points.
    #[serde(default)]
    pub spawn_groups: Vec<SpawnGroupDef>,
    /// Authored movement/pose routes for placed entities, keyed by instance
    /// id.
    #[serde(default)]
    pub routes: Vec<EntityRouteDef>,
    /// Surfaces whose *emission* moves over time: a breathing illuminated sign,
    /// a failing tube. Empty on every level that does not ask for one.
    ///
    /// The animation scales the additive emissive term only. The baked
    /// illumination is static by design, so a flickering panel keeps lighting
    /// the room exactly as it was baked.
    #[serde(default)]
    pub animated_emissions: Vec<AnimatedEmissionDef>,
    /// One intent annotation for the map geometry checker.
    ///
    /// A plan rectangle where a named **heuristic** finding is deliberate — an
    /// open-plan edge between two rooms, a carpet hole, a pit — and must not be
    /// reported. The checker reads these; the runtime ignores them. An
    /// annotation never suppresses a confirmed defect and only covers its own
    /// rectangle.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub geometry_intent: Vec<GeometryIntentDef>,
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
/// Art budget for one **skinned character** model's triangle count.
///
/// The prop art budget (1,500) is enforced by the prop toolkit and is the
/// right target for static architecture; a rigged creature legitimately
/// carries more detail because its limbs, head and face deform as one skin
/// (the shipped pumpkin-head skeleton is 2,278 triangles across 94 joints).
/// This is still half the engine ceiling and applies only to models that
/// declare a skin; an unskinned prop keeps the tighter budget.
pub const ENTITY_TRIANGLE_BUDGET: usize = 3_000;
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
/// Engine ceiling on the joints one prop model's skin may declare.
///
/// A production character rig is a few dozen joints; the cap exists so a
/// pathological file cannot turn the per-frame pose evaluation into unbounded
/// work. The character path refuses a rig above this budget by name.
pub const MAX_PROP_JOINTS: usize = 128;
/// Engine ceiling on the animation clips one prop model may declare.
pub const MAX_PROP_ANIMATIONS: usize = 64;
/// Engine ceiling on the animation channels one prop model may declare,
/// summed over every clip.
pub const MAX_ANIMATION_CHANNELS: usize = 4_096;
/// Engine ceiling on the morph targets one prop primitive may declare.
///
/// Morph deltas are stored parallel to the model's vertices, so the cap keeps
/// a pathological file from multiplying the vertex arrays without bound.
pub const MAX_PROP_MORPH_TARGETS: usize = 32;
/// Engine ceiling on the morph targets one model may declare in total.
///
/// Deltas are stored parallel to the model's vertices, so the total cap (not
/// the per-primitive one) bounds the memory a morph-heavy asset can demand.
pub const MAX_PROP_MORPH_TARGETS_PER_MODEL: usize = 128;
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
/// runtime quality level decides what actually reaches the GPU.
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
///
/// Raised to 4096 from the previous 1024: the cap is a lookup-table bound
/// (one decoded-and-uploaded model set), not a per-frame cost, and a generated
/// level may legitimately reference every registered model plus user imports.
/// Like [`MAX_LEVEL_PROP_VERTICES`] this is *not* a level rejection: placements
/// past the cap draw their placeholder boxes, exactly as they did at 256, and
/// the level still loads and collides. The cap exists so one level's decoded
/// model set stays bounded.
pub const MAX_LEVEL_PROP_MODELS: usize = 4_096;
/// Upper bound on the summed prop vertex count a level may expand into after
/// instance transforms are baked, keeping one level's prop geometry bounded.
///
/// Raised to 24 000 000 from the previous 6 000 000, in lockstep with
/// [`MAX_LEVEL_VERTICES`]: prop vertices are one half of the level's generated
/// geometry budget, and the extended capacity fixture
/// (`capacity_beyond_former_limits.json`, 20 001+ props) expands to roughly
/// 6.3 M prop vertices — the cap is four times that measured workload. The
/// loader reports the count when a level exceeds it, and the remaining
/// placements draw their placeholder boxes instead of disappearing.
pub const MAX_LEVEL_PROP_VERTICES: usize = 24_000_000;
/// Hard ceiling on the number of rooms a level may define.
///
/// Raised to 8000 from 2000 in the 2026 capacity pass: the extended capacity
/// fixture authors 8 000 rooms and validates, and rooms are `Vec`-backed
/// footprints read through [`crate::level::LevelSurfaces`]' lookup grid, so
/// the cap is an authoring bound rather than a per-frame cost. It stays a
/// *rejection*: a file above it is outside the measured envelope.
pub const MAX_LEVEL_ROOMS: u64 = 8_000;
/// Hard ceiling on the number of walls a level may define.
///
/// Raised to 60 000 from 20 000: collision goes through the spatial
/// [`crate::collision_index::CollisionIndex`], so per-frame wall queries no
/// longer scale with the wall count, and the extended capacity fixture
/// validates 60 000 walls while `zoo_audit` pins the indexed query equal to
/// the linear scan.
pub const MAX_LEVEL_WALLS: u64 = 60_000;
/// Hard ceiling on the number of ceiling light fixtures a level may define.
///
/// Raised to 50 000 from 20 000. Fixtures are opaque static geometry plus one
/// baked-light record each; the lightmap atlas bounds what can actually be
/// baked, and a level past that budget falls back to vertex lighting by name
/// rather than silently dropping fixtures.
pub const MAX_LEVEL_CEILING_LIGHTS: u64 = 50_000;
/// Hard ceiling on the number of placed props a level may define.
///
/// Raised to 100 000 from 20 000: a prop placement is one `PropDef` and one
/// instanced transform, and the extended capacity fixture validates 100 000
/// placements, expanding to about 600 000 vertices with the shipped prop
/// geometry budget. The expansion itself stays bounded by
/// [`MAX_LEVEL_PROP_VERTICES`] and [`MAX_LEVEL_PROP_MODELS`], which are not
/// rejections.
pub const MAX_LEVEL_PROPS: u64 = 100_000;
/// Largest room width or depth a level may author, in metres.
///
/// The historical cap was 2000 m. The sparse capacity fixture spans ±4 km with
/// geometry and gameplay in every quadrant, so the cap is raised to 8192 m —
/// still inside `f32`'s exact-integer range and far below the point where world
/// space loses centimetre precision. A single larger room is refused by name.
pub const MAX_ROOM_EXTENT_M: f32 = 8_192.0;
/// Largest room clear height a level may author, in metres.
pub const MAX_ROOM_HEIGHT_M: f32 = 50.0;
/// Hard ceiling on the number of local floor regions a level may define.
///
/// Raised to 8000 from 2000 in the 2026 capacity pass; regions are queried
/// through the floor's own lookup rather than scanned linearly, so the cap is
/// an authoring bound.
pub const MAX_LEVEL_FLOOR_REGIONS: u64 = 8_000;
/// Hard ceiling on the number of floor patches a level may define.
///
/// A patch is a material override, not geometry, so this only bounds parse and
/// lookup cost; it is deliberately the same order as the region budget.
pub const MAX_LEVEL_FLOOR_PATCHES: u64 = 8_000;
/// Hard ceiling on the number of water volumes a level may define.
///
/// One volume draws one surface quad, so this only bounds the per-frame water
/// sample loop and the static mesh's footprint; it matches the floor-region
/// budget deliberately.
pub const MAX_LEVEL_WATER_VOLUMES: u64 = 8_000;
/// Hard ceiling on the number of ladders a level may define.
///
/// Ladders are sampled linearly by the controller like water volumes, and they
/// do not draw geometry of their own (the visual ladder is a prop), so this is
/// a generous authoring bound rather than a rendering budget. Raised to 1024
/// from 256: even the linear scan is a handful of comparisons per ladder.
pub const MAX_LEVEL_LADDERS: u64 = 1_024;
/// Hard ceiling on the number of timers a level may define.
///
/// Timers are advanced linearly every tick and each carries one small runtime
/// record, so this is an authoring bound rather than a performance budget.
pub const MAX_LEVEL_TIMERS: usize = 256;
/// Hard ceiling on the number of area triggers a level may define.
///
/// Triggers are sampled linearly by the controller (a swept box test per
/// frame), do not draw geometry and are not in the collision world, so this is
/// an authoring bound, not a rendering budget. Raised to 4000 from 1000 in the
/// 2026 capacity pass; 4000 swept tests per frame stay well inside one frame's
/// budget, and [`crate::entities::events::EventQueue::MAX_QUEUED_EVENTS`] is
/// sized to hold one occurrence per trigger so a busy tick cannot drop work.
pub const MAX_LEVEL_AREA_TRIGGERS: u64 = 4_000;
/// Hard ceiling on the number of openings a single wall may declare.
pub const MAX_WALL_OPENINGS: usize = 64;
/// Hard ceiling on the number of ramps a level may define.
pub const MAX_LEVEL_RAMPS: u64 = 2_000;
/// Hard ceiling on the number of staircases a level may define.
pub const MAX_LEVEL_STAIRS: u64 = 2_000;
/// Hard ceiling on the number of half walls a level may define.
pub const MAX_LEVEL_HALF_WALLS: u64 = 8_000;
/// Hard ceiling on the number of columns a level may define.
pub const MAX_LEVEL_COLUMNS: u64 = 8_000;
/// Hard ceiling on the number of archways a level may define.
pub const MAX_LEVEL_ARCHWAYS: u64 = 2_000;
/// Hard ceiling on the number of guardrails a level may define.
pub const MAX_LEVEL_GUARDRAILS: u64 = 8_000;
/// Hard ceiling on the number of threshold strips a level may define.
pub const MAX_LEVEL_THRESHOLDS: u64 = 4_000;
/// Hard ceiling on the number of baseboard runs a level may define.
pub const MAX_LEVEL_BASEBOARDS: u64 = 8_000;
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
/// The shipped demo's source is about 0.8 MB and the extended capacity
/// fixture (`capacity_beyond_former_limits.json`) about 14.4 MB, so this is
/// ample headroom for a hand-authored or generated level while still refusing
/// an accidentally huge file before it is read into memory. The embedded
/// fallback demo is exempt (it is compiled in).
pub const MAX_LEVEL_JSON_BYTES: u64 = 32 * 1024 * 1024;
/// Sanity budget for total authored floor area, in square metres.
///
/// Floor rendering no longer scales with area, but absurdly large levels still
/// stress collision, fill rate and level-design tooling, so a generous cap is
/// kept as a sanity guard. Raised to 64 km² from 16 km² in the 2026 capacity
/// pass, so four 4 km × 4 km quadrant rooms (the sparse coordinate regression)
/// plus a dense interior fit together with the same headroom the previous cap
/// gave the sparse fixture alone.
pub const MAX_LEVEL_FLOOR_AREA_M2: u64 = 64_000_000;
/// Sanity budget on the estimated number of generated vertices.
///
/// Raised to 24 000 000 from the previous 8 000 000, in lockstep with
/// [`MAX_LEVEL_PROP_VERTICES`]: the estimate counts six vertices per generated
/// quad (an upper bound for a quad emitted without index sharing), so this is
/// four million quads. The extended capacity fixture's estimate is measured in
/// `target/agent-work/places-expansion-compact/01/lane-b/evidence.md`. It
/// remains the loader's upper-bound estimate, not an allocation.
pub const MAX_LEVEL_VERTICES: u64 = 24_000_000;
/// Hard ceiling on the number of distinct materials a level may reference.
///
/// This is the explicit replacement for the silent `u16` collapse the
/// material index used to have: [`crate::render::MaterialIndex`] is 32 bits,
/// but 131 072 (2 × 65 536) distinct materials in one map is already far
/// beyond any authored level, and refusing the count by name — before a
/// single image is decoded or uploaded — is honest where saturation is not.
/// The loader counts exactly the ids
/// [`crate::materials::referenced_material_ids`] resolves, which is the set
/// the renderer binds, so the validator and the draw path agree by
/// construction.
pub const MAX_LEVEL_MATERIALS: u64 = 131_072;
/// Hard ceiling on the distinct decoded RGBA texture bytes one level's
/// resolved material table may hold.
///
/// Total map *capacity* is what a package may declare — the counts above bound
/// authored data. This constant is the simultaneous GPU-residency guard for
/// the texture half of that data: the renderer uploads the shared
/// [`crate::materials::MaterialTable`] texture set at once, and one level
/// declaring more than 1 GiB of decoded images (1 073 741 824 bytes: 256
/// images at the 1024×1024 hard edge, or tens of thousands of ordinary
/// sheets) is refused by name by
/// [`crate::materials::check_texture_budget`] before any upload. The other
/// resident budgets are separate and unchanged: the lightmap atlas page budget
/// ([`crate::package::MAX_LIGHTMAP_PAGES`]), probe cubemaps, and the
/// dynamic-object/character caps.
pub const MAX_LEVEL_TEXTURE_BYTES: usize = 1 << 30;

/// Estimated generated geometry for a level, used to bound memory use before
/// building vertex data. This conservative upper bound is not a reservation.
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

/// Distance below which a grid-aligned fixture counts as already settled: a
/// snap that moves a centre by less than this is float residue, not a move.
pub const ALIGN_SETTLED_EPS_M: f32 = 1.0e-4;

/// The snapped centre of one grid-aligned ceiling fixture, or `None` when the
/// fixture does not qualify for grid alignment.
///
/// See [`LevelDef::align_ceiling_fixtures`] for the qualification rules and the
/// snap formula. `default_ceiling` is [`LevelDefaults::ceiling`], used when the
/// room over the fixture does not override its ceiling material.
fn grid_aligned_fixture_centre(
    surfaces: &LevelSurfaces<'_>,
    default_ceiling: &str,
    light: &LightFixtureDef,
    materials: &crate::materials::MaterialTable,
) -> Option<(f32, f32)> {
    if light.align != FixtureAlign::Grid || light.mount != LightMount::Ceiling {
        return None;
    }
    if !light.x.is_finite() || !light.z.is_finite() {
        return None;
    }
    if crate::lighting::fixture_profile(&light.fixture).kind
        != crate::lighting::FixtureKind::FluorescentPanel
    {
        return None;
    }
    if !surfaces.ceiling_is_flat_at(light.x, light.z) {
        return None;
    }
    let room = surfaces.room_at(light.x, light.z)?;
    // The geometry builder resolves a room ceiling exactly this way: the
    // room's own `ceiling_material` first, else the level default. Only the
    // material's id matters for tiling, so the table lookup below sees the
    // same period the ceiling mesh tiles at.
    let material_id = room
        .ceiling_material
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| default_ceiling.trim());
    // The visible panel module, not the sheet repeat: a ceiling sheet may
    // paint four 1 m panels inside a 2 m texture tile, and a fixture belongs
    // at a panel's centre rather than on the T-bar between four of them.
    let period = materials.entry_of(material_id)?.grid_metres;
    if !period.is_finite() || period <= 0.0 {
        return None;
    }
    let half = period * 0.5;
    let snap = |value: f32| period.mul_add(((value - half) / period).round(), half);
    // Snap in the room's own ceiling tile frame, so a room that authored an
    // origin/rotation is honoured; an unauthored frame is the world origin at
    // zero rotation, which is exactly the historical formula.
    let (local_x, local_z) = room.ceiling_tile_local(light.x, light.z);
    Some(room.ceiling_tile_world(snap(local_x), snap(local_z)))
}

/// The snapped centre of one `align: "ceiling_grid"` ceiling decal, or `None`
/// when the decal does not qualify for grid alignment.
///
/// Qualification mirrors the fixture rule: a flat ceiling, inside a room whose
/// resolved ceiling material declares a positive finite period, and a finite
/// authored centre. The snap period is the material's visible panel module
/// (`grid_metres`, falling back to `tile_metres` when the sheet paints one
/// panel per repeat), and the snapped position is a panel **centre** in the
/// room's ceiling tile frame.
fn ceiling_grid_decal_centre(
    surfaces: &LevelSurfaces<'_>,
    default_ceiling: &str,
    decal: &DecalDef,
    materials: &crate::materials::MaterialTable,
) -> Option<(f32, f32)> {
    if decal.surface != DecalSurface::Ceiling || !decal.x.is_finite() || !decal.z.is_finite() {
        return None;
    }
    if !surfaces.ceiling_is_flat_at(decal.x, decal.z) {
        return None;
    }
    let room = surfaces.room_at(decal.x, decal.z)?;
    let material_id = room
        .ceiling_material
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| default_ceiling.trim());
    let entry = materials.entry_of(material_id)?;
    // The visible panel module, which falls back to the sheet repeat for a
    // sheet that paints one panel per tile; a vent should sit inside one
    // panel, not straddle the T-bar between four.
    let period = if entry.grid_metres.is_finite() && entry.grid_metres > 0.0 {
        entry.grid_metres
    } else {
        entry.tile_metres
    };
    if !period.is_finite() || period <= 0.0 {
        return None;
    }
    let half = period * 0.5;
    let snap = |value: f32| period.mul_add(((value - half) / period).round(), half);
    let (local_x, local_z) = room.ceiling_tile_local(decal.x, decal.z);
    Some(room.ceiling_tile_world(snap(local_x), snap(local_z)))
}

impl LevelDef {
    /// # Errors
    ///
    /// Returns the `serde_json` error when the document is not valid JSON or
    /// does not match the level schema.
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }

    /// Iterates over every room section without cloning or allocating.
    pub fn room_iter(&self) -> std::slice::Iter<'_, RoomDef> {
        self.rooms.iter()
    }

    /// Stable per-instance id for every placed prop, in array order.
    ///
    /// An authored `id` wins (trimmed). Otherwise the default is
    /// `<model short name>_<n>`, where the short name is the text after the
    /// last `:` and `n` counts, in array order, the placements that do not
    /// author an id — `<short>_<n>` is the same shape the level tooling
    /// writes. Deterministic for a given document, never derived from time,
    /// randomness or a mutable object's model filename.
    #[must_use]
    pub fn prop_instance_ids(&self) -> Vec<String> {
        let mut counters: HashMap<&str, usize> = HashMap::new();
        self.props
            .iter()
            .map(|prop| {
                if let Some(id) = prop
                    .id
                    .as_deref()
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                {
                    return id.to_string();
                }
                let short = prop.model.rsplit(':').next().unwrap_or(&prop.model);
                let count = counters.entry(short).or_insert(0);
                *count = count.saturating_add(1);
                format!("{short}_{count}")
            })
            .collect()
    }

    /// Stable per-instance id for every door, in array order.
    ///
    /// A door's id is required and authored; this accessor exists so id
    /// validation and action resolution can treat doors exactly like props and
    /// fixtures.
    #[must_use]
    pub fn door_instance_ids(&self) -> Vec<String> {
        self.doors.iter().map(|door| door.id.clone()).collect()
    }

    /// Stable per-instance id for every light fixture, in array order.
    ///
    /// Same scheme as [`Self::prop_instance_ids`], counting per fixture short
    /// name. A switchable fixture's id is the name a `toggle` action addresses.
    #[must_use]
    pub fn light_instance_ids(&self) -> Vec<String> {
        let mut counters: HashMap<&str, usize> = HashMap::new();
        self.ceiling_lights
            .iter()
            .map(|fixture| {
                if let Some(id) = fixture
                    .id
                    .as_deref()
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                {
                    return id.to_string();
                }
                let short = fixture
                    .fixture
                    .rsplit(':')
                    .next()
                    .unwrap_or(&fixture.fixture);
                let count = counters.entry(short).or_insert(0);
                *count = count.saturating_add(1);
                format!("{short}_{count}")
            })
            .collect()
    }

    /// Stable per-instance id for every trigger volume, in array order.
    ///
    /// An authored `id` wins; otherwise `trigger_<n>` with `n` the 1-based
    /// authored position.
    #[must_use]
    pub fn volume_instance_ids(&self) -> Vec<String> {
        self.volumes
            .iter()
            .enumerate()
            .map(|(index, volume)| {
                volume
                    .id
                    .as_deref()
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                    .map_or_else(
                        || format!("trigger_{}", index.saturating_add(1)),
                        str::to_string,
                    )
            })
            .collect()
    }

    /// Stable per-instance id for every water volume, in array order.
    #[must_use]
    pub fn water_instance_ids(&self) -> Vec<String> {
        self.water
            .iter()
            .enumerate()
            .map(|(index, _)| format!("water_{}", index.saturating_add(1)))
            .collect()
    }

    /// Stable id for every timer, in array order.
    #[must_use]
    pub fn timer_instance_ids(&self) -> Vec<String> {
        self.timers
            .iter()
            .map(|timer| timer.id.trim().to_string())
            .collect()
    }

    /// Stable id for every spawn point, in array order.
    #[must_use]
    pub fn spawn_point_instance_ids(&self) -> Vec<String> {
        self.spawn_points
            .iter()
            .map(|point| point.id.trim().to_string())
            .collect()
    }

    /// Every binding authored anywhere in the level, in a stable order:
    /// props, doors, fixtures, volumes, effects, timers, spawn points and
    /// spawn templates.
    #[must_use]
    pub fn all_bindings(&self) -> Vec<&[EventBindingDef]> {
        let mut all: Vec<&[EventBindingDef]> = Vec::new();
        for prop in &self.props {
            all.push(&prop.bindings);
        }
        for door in &self.doors {
            all.push(&door.bindings);
        }
        for fixture in &self.ceiling_lights {
            all.push(&fixture.bindings);
        }
        for volume in &self.volumes {
            all.push(&volume.bindings);
        }
        for effect in &self.effects {
            all.push(&effect.bindings);
        }
        for timer in &self.timers {
            all.push(&timer.bindings);
        }
        for point in &self.spawn_points {
            all.push(&point.bindings);
        }
        for template in &self.spawn_templates {
            all.push(&template.bindings);
        }
        all
    }

    /// Snaps grid-aligned fluorescent panels onto their ceiling material's
    /// world tile grid, returning how many fixture centres moved.
    ///
    /// A fixture qualifies when all of the following hold:
    ///
    /// * [`LightFixtureDef::align`] is [`FixtureAlign::Grid`] (the default);
    /// * its family is the grid panel
    ///   ([`crate::lighting::FixtureKind::FluorescentPanel`]) and it mounts to
    ///   the ceiling;
    /// * it stands inside a room whose ceiling is flat
    ///   ([`LevelSurfaces::ceiling_is_flat_at`]) — a gable or an off-room
    ///   fixture has no single ceiling plane to align to;
    /// * the room's `ceiling_material` (else `defaults.ceiling`) resolves a
    ///   positive finite `grid_metres` through `materials`, the table the
    ///   geometry builder samples its tiling from (`grid_metres` defaults to
    ///   the material's `tile_metres`).
    ///
    /// The centre snaps per axis to the nearest cell centre of the material's
    /// world panel grid:
    ///
    /// ```text
    /// snapped = ((v - T / 2) / T).round() * T + T / 2
    /// ```
    ///
    /// where `T` is the material's `grid_metres` and `v` is the authored `x`
    /// or `z`. `f32::round` is half-away-from-zero, so a value exactly on a
    /// cell boundary deterministically moves to the higher cell. Only `x`/`z`
    /// change; rotation, brightness, colour, range, falloff and emission are
    /// untouched. This keeps the bake, the mesh, the fixture probe and
    /// collision on one position: everything downstream reads the same
    /// `ceiling_lights` array.
    ///
    /// Levels that author `"align": "none"` are never moved, and a level with
    /// no ceiling material period (an unresolved or blank ceiling id) stays
    /// exactly where it was authored.
    pub fn align_ceiling_fixtures(&mut self, materials: &crate::materials::MaterialTable) -> usize {
        let mut moved = 0usize;
        let mut targets: Vec<(usize, f32, f32)> = Vec::new();
        {
            let surfaces = LevelSurfaces::new(self);
            for (index, light) in self.ceiling_lights.iter().enumerate() {
                let Some((x, z)) = grid_aligned_fixture_centre(
                    &surfaces,
                    &self.defaults.ceiling,
                    light,
                    materials,
                ) else {
                    continue;
                };
                if (x - light.x).abs() > ALIGN_SETTLED_EPS_M
                    || (z - light.z).abs() > ALIGN_SETTLED_EPS_M
                {
                    moved = moved.saturating_add(1);
                }
                targets.push((index, x, z));
            }
        }
        for (index, x, z) in targets {
            if let Some(light) = self.ceiling_lights.get_mut(index) {
                light.x = x;
                light.z = z;
            }
        }
        moved
    }

    /// Snaps every `align: "ceiling_grid"` ceiling decal onto its ceiling's
    /// tile grid, in the room's own tile frame. Returns how many decals moved.
    ///
    /// A decal qualifies when it is a ceiling decal on a flat ceiling, inside a
    /// room whose resolved ceiling material declares a positive finite period
    /// (its visible panel module, falling back to the sheet repeat). The snap
    /// formula is the light-fixture one applied in the room's ceiling tile
    /// frame (origin and rotation), so a rotated or offset grid places its
    /// decals on its own panels rather than on an assumed global grid. The in-plane rotation composition happens at
    /// emission ([`Self::ceiling_decal_rotation`]), so this pass is idempotent
    /// and the authored rotation is never rewritten.
    pub fn snap_ceiling_decals(&mut self, materials: &crate::materials::MaterialTable) -> usize {
        let mut moved = 0usize;
        let mut targets: Vec<(usize, f32, f32)> = Vec::new();
        {
            let surfaces = LevelSurfaces::new(self);
            for (index, decal) in self.decals.iter().enumerate() {
                if decal.align != DecalAlign::CeilingGrid {
                    continue;
                }
                let Some((x, z)) =
                    ceiling_grid_decal_centre(&surfaces, &self.defaults.ceiling, decal, materials)
                else {
                    continue;
                };
                if (x - decal.x).abs() > ALIGN_SETTLED_EPS_M
                    || (z - decal.z).abs() > ALIGN_SETTLED_EPS_M
                {
                    moved = moved.saturating_add(1);
                }
                targets.push((index, x, z));
            }
        }
        for (index, x, z) in targets {
            if let Some(decal) = self.decals.get_mut(index) {
                decal.x = x;
                decal.z = z;
            }
        }
        moved
    }

    /// The in-plane rotation one decal draws with.
    ///
    /// A `ceiling_grid` decal composes the ceiling tile frame's own rotation
    /// with its authored `rotation_degrees`, so vents stay square to a rotated
    /// tile grid; every other decal draws its authored value. Composition lives
    /// here rather than in [`Self::snap_ceiling_decals`] so the snap pass is
    /// idempotent and the level file keeps the author's intent.
    #[must_use]
    pub fn ceiling_decal_rotation(&self, decal: &DecalDef) -> f32 {
        if decal.align != DecalAlign::CeilingGrid
            || decal.surface != DecalSurface::Ceiling
            || !decal.rotation_degrees.is_finite()
        {
            return decal.rotation_degrees;
        }
        let surfaces = LevelSurfaces::new(self);
        surfaces
            .room_at(decal.x, decal.z)
            .map_or(decal.rotation_degrees, |room| {
                let (_, _, rotation) = room.ceiling_tile_frame();
                decal.rotation_degrees + rotation
            })
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
        for piece in &self.arc_walls {
            boxes.extend(piece.collision_boxes(&surfaces));
        }
        for piece in &self.pillars {
            boxes.extend(piece.collision_boxes(&surfaces));
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
        for piece in &self.arc_walls {
            let segments = u64::from(piece.resolved_segments());
            let ends = if piece.is_full_ring() { 0 } else { 2 };
            wall_quads = wall_quads
                .saturating_add(segments.saturating_mul(4))
                .saturating_add(ends);
        }
        for piece in &self.pillars {
            wall_quads = wall_quads
                .saturating_add(u64::from(piece.resolved_segments()))
                .saturating_add(2);
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
        // A void wall is six faces of one quad (twelve for `faces: "both"`),
        // all wall-like.
        for _ in &self.void_walls {
            wall_quads = wall_quads.saturating_add(MAX_VOID_WALL_QUADS);
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

            // A glazed opening marked `solid` blocks the player: rendering
            // transparency and collision are independent, so the pane is a
            // thin slab on the wall's centre plane. `solid` without `glass` is
            // refused by validation (an invisible barrier is a wall).
            for opening in &wall.openings {
                if let Some(pane) = opening_glass_collider(wall, opening) {
                    aabbs.push(pane);
                }
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

        // Void wall / floor boxes: a `solid` box collides as its whole
        // authored volume, exactly like a solid prop's `size` box. A box that
        // encloses a walkable space is therefore a solid block; build a hollow
        // enclosure from thin slabs, or set `solid: false` for a sight-only
        // shell the player walks through.
        for piece in &self.void_walls {
            if !piece.solid {
                continue;
            }
            if let Some(boxed) = piece.resolved_box() {
                aabbs.push(boxed.to_wall_aabb());
            }
        }

        for prop in &self.props {
            if let Some(collider) = prop.solid_collider(&surfaces) {
                aabbs.push(collider);
            }
        }

        aabbs
    }
}

/// The collision slab of a glazed opening that authors `solid`, if any.
///
/// The visible pane is a zero-thickness quad on the wall's centre plane; the
/// physical slab is [`OPENING_GLASS_THICKNESS_M`] thick around it, spanning the
/// opening's rectangle. An opening without `glass`, or with `solid: false`, is
/// the bare hole.
#[must_use]
fn opening_glass_collider(wall: &WallDef, opening: &WallOpeningDef) -> Option<WallAabb> {
    if !opening.solid || opening.glass_material().is_none() {
        return None;
    }
    if !opening.offset.is_finite()
        || !opening.width.is_finite()
        || !opening.height.is_finite()
        || !opening.sill.is_finite()
        || opening.width <= 0.0
        || opening.height <= 0.0
    {
        return None;
    }
    let (min_x, max_x) = (
        wall.x.min(wall.x + wall.width),
        wall.x.max(wall.x + wall.width),
    );
    let (min_z, max_z) = (
        wall.z.min(wall.z + wall.depth),
        wall.z.max(wall.z + wall.depth),
    );
    let (origin_x, origin_z) = wall.length_origin();
    let bottom = opening.bottom(wall.y);
    let half_thickness = OPENING_GLASS_THICKNESS_M * 0.5;
    let (pane_x, pane_z, pane_w, pane_d) = match wall.axis() {
        WallAxis::X => (
            origin_x + opening.offset,
            f32::midpoint(min_z, max_z) - half_thickness,
            opening.width,
            OPENING_GLASS_THICKNESS_M,
        ),
        WallAxis::Z => (
            f32::midpoint(min_x, max_x) - half_thickness,
            origin_z + opening.offset,
            OPENING_GLASS_THICKNESS_M,
            opening.width,
        ),
    };
    Some(WallAabb::with_y(
        pane_x,
        bottom,
        pane_z,
        pane_w,
        opening.height,
        pane_d,
    ))
}

impl PropDef {
    /// The prop's collision box when it is solid, placed on its local floor.
    ///
    /// A yaw-rotated prop's box is conservatively covered by the axis-aligned
    /// box of the rotated rectangle: the collider then contains every visible
    /// corner at any angle, instead of colliding where the model is not and
    /// passing through where it is.
    #[must_use]
    fn solid_collider(&self, surfaces: &LevelSurfaces<'_>) -> Option<WallAabb> {
        if !self.solid {
            return None;
        }
        let size = self.resolved_size(PROP_FALLBACK_SIZE);
        if !size.iter().all(|v| v.is_finite() && *v > 0.0)
            || !self.x.is_finite()
            || !self.y.is_finite()
            || !self.z.is_finite()
        {
            return None;
        }
        let base_y = surfaces.floor_y_at(self.x, self.z).unwrap_or(0.0);
        let (extent_x, extent_z) =
            rotated_half_extents_local(size[0] * 0.5, size[2] * 0.5, self.rotation_degrees);
        Some(WallAabb::with_y(
            self.x - extent_x,
            base_y + self.y,
            self.z - extent_z,
            extent_x * 2.0,
            size[1],
            extent_z * 2.0,
        ))
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
    /// [`RIM_BACKING`] deep *under the higher floor* to give the boundary a
    /// finite box; the controller's radius-bounded sweep prevents tunnelling. Because
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
/// the circle-vs-box test is well-conditioned. The player's radius-bounded
/// sweep already prevents tunnelling through a thin slab. Thick backing would
/// incorrectly fill narrow ramps from both sides and block their low end.
pub const RIM_BACKING: f32 = 0.01;

/// The vertical geometry of a level: rooms, their ceiling profiles and their
/// local floor regions, queried through one deterministic ownership rule.
///
/// This is the single source of truth for "where is the floor", "where is the
/// ceiling" and "which room is this". Rendering, collision and lighting all
/// resolve through it (lighting additionally keeps its own baked per-room
/// values), so a formula cannot drift between the mesh and the systems that
/// have to agree with it.
///
/// Ownership follows the clear-ceiling query: the first room in `rooms` order
/// whose footprint contains the point (with [`ROOM_EDGE_EPS_M`] tolerance)
/// wins, including walls sitting on a shared room boundary.
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
    /// World Y of the stepped surface at `(x, z)` (the rendered treads).
    fn height_at(&self, x: f32, z: f32) -> f32 {
        self.floor_y + self.surface.offset_at(x, z)
    }

    /// World Y of the surface the controller walks at `(x, z)`: the line
    /// through the nosings ([`StairSurface::pitch_offset_at`]).
    fn pitch_height_at(&self, x: f32, z: f32) -> f32 {
        self.floor_y + self.surface.pitch_offset_at(x, z)
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

/// Which vertical surface a staircase answers with.
///
/// The rendered treads and the controller's walking surface differ: the mesh,
/// the floor atlas and prop placement need the exact stepped geometry, while a
/// player must move continuously from one tread to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StairSampling {
    /// The rendered treads: the step the point is on.
    Stepped,
    /// The line through the flight's nosings, level on the top tread.
    PitchLine,
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

    /// World Y of the walkable floor at `(x, z)`, or `None` outside every room.
    ///
    /// This is the *rendered* surface: on a staircase it answers with the tread
    /// the point is on, exactly like [`LevelSurfaces::floor_y_at`]. Use
    /// [`Self::walk_height_at`] for the surface a walking player's feet follow,
    /// which is continuous across a flight.
    ///
    /// The first room in level order containing the point wins, matching
    /// [`LevelSurfaces::floor_y_at`]; inside it a ramp or staircase covering the
    /// point wins over the last authored floor region, exactly as the surface
    /// queries resolve it.
    #[must_use]
    pub fn height_at(&self, x: f32, z: f32) -> Option<f32> {
        self.resolve_height_at(x, z, StairSampling::Stepped)
    }

    /// World Y of the surface the player's feet follow while walking at
    /// `(x, z)`, or `None` outside every room.
    ///
    /// Resolves exactly like [`Self::height_at`] except on a staircase, where
    /// it answers with the line through the flight's nosings
    /// ([`StairSurface::pitch_offset_at`]). Sampling that line during movement
    /// makes the foot height rise and fall continuously from tread to tread
    /// instead of jumping one riser per boundary, while the rendered treads,
    /// collision rims and prop placement keep using the stepped surface.
    #[must_use]
    pub fn walk_height_at(&self, x: f32, z: f32) -> Option<f32> {
        self.resolve_height_at(x, z, StairSampling::PitchLine)
    }

    /// The height resolution shared by [`Self::height_at`] and
    /// [`Self::walk_height_at`]; `stair_sampling` selects the staircase's
    /// surface, everything else resolves identically.
    fn resolve_height_at(&self, x: f32, z: f32, stair_sampling: StairSampling) -> Option<f32> {
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
            // Edge tolerance belongs to room selection. Clamp surface samples to
            // that room so a region ending at the shared edge cannot disappear
            // in the epsilon strip and briefly expose the lower base floor.
            let x = x.clamp(room.x0, room.x1);
            let z = z.clamp(room.z0, room.z1);
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
                return Some(match stair_sampling {
                    StairSampling::Stepped => stair.height_at(x, z),
                    StairSampling::PitchLine => stair.pitch_height_at(x, z),
                });
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

    /// Encodes this floor model into a compiled collision record: the room
    /// count, then each room's footprint, ramps, staircases and regions.
    ///
    /// # Errors
    ///
    /// Returns a named error when the model holds more rooms than
    /// [`crate::package::MAX_COLLISION_BOXES`], or a room holds more ramps,
    /// stairs or regions than that same bound.
    pub(crate) fn write_compiled(&self, writer: &mut Writer) -> Result<(), String> {
        write_collision_count(
            writer,
            self.rooms.len(),
            collision_box_limit(),
            "walkable floor room count",
        )?;
        for room in &self.rooms {
            write_walkable_room(writer, room)?;
        }
        Ok(())
    }

    /// Decodes a floor model from a compiled collision record.
    ///
    /// # Errors
    ///
    /// Returns a named error when the record is truncated, declares an
    /// out-of-range count, or holds a non-finite or inverted value.
    pub(crate) fn read_compiled(reader: &mut Reader<'_>) -> Result<Self, String> {
        let room_count = reader.count(collision_box_limit(), "walkable floor room count")?;
        let mut rooms = Vec::with_capacity(room_count.min(1024));
        for _ in 0..room_count {
            rooms.push(read_walkable_room(reader)?);
        }
        Ok(Self { rooms })
    }
}

/// The collision-record entry bound as a `u64` reader limit.
fn collision_box_limit() -> u64 {
    u64::try_from(crate::package::MAX_COLLISION_BOXES).unwrap_or(u64::MAX)
}

/// True when every value is finite.
fn collision_values_finite(values: &[f32]) -> bool {
    values.iter().all(|value| value.is_finite())
}

/// Writes one counted section header, rejecting counts over `limit`.
fn write_collision_count(
    writer: &mut Writer,
    length: usize,
    limit: u64,
    what: &str,
) -> Result<(), String> {
    if u64::try_from(length).unwrap_or(u64::MAX) > limit {
        return Err(format!("{what} {length} exceeds the limit {limit}"));
    }
    writer.u32(u32::try_from(length).map_err(|_| format!("{what} is too large"))?);
    Ok(())
}

/// Encodes one walkable room: its footprint and floor, then its ramps,
/// staircases and regions.
fn write_walkable_room(writer: &mut Writer, room: &WalkableRoom) -> Result<(), String> {
    writer.f32(room.x0);
    writer.f32(room.x1);
    writer.f32(room.z0);
    writer.f32(room.z1);
    writer.f32(room.floor_y);
    write_collision_count(
        writer,
        room.ramps.len(),
        collision_box_limit(),
        "walkable ramp count",
    )?;
    for ramp in &room.ramps {
        write_walkable_ramp(writer, ramp);
    }
    write_collision_count(
        writer,
        room.stairs.len(),
        collision_box_limit(),
        "walkable stair count",
    )?;
    for stair in &room.stairs {
        write_walkable_stair(writer, stair);
    }
    write_collision_count(
        writer,
        room.regions.len(),
        collision_box_limit(),
        "walkable region count",
    )?;
    for region in &room.regions {
        write_walkable_region(writer, region);
    }
    Ok(())
}

/// Decodes one walkable room.
fn read_walkable_room(reader: &mut Reader<'_>) -> Result<WalkableRoom, String> {
    let x0 = reader.f32()?;
    let x1 = reader.f32()?;
    let z0 = reader.f32()?;
    let z1 = reader.f32()?;
    let floor_y = reader.f32()?;
    if !collision_values_finite(&[x0, x1, z0, z1, floor_y]) {
        return Err("walkable room has a non-finite value".to_string());
    }
    if x0 > x1 || z0 > z1 {
        return Err("walkable room bounds are inverted".to_string());
    }
    let ramp_count = reader.count(collision_box_limit(), "walkable ramp count")?;
    let mut ramps = Vec::with_capacity(ramp_count.min(1024));
    for _ in 0..ramp_count {
        ramps.push(read_walkable_ramp(reader)?);
    }
    let stair_count = reader.count(collision_box_limit(), "walkable stair count")?;
    let mut stairs = Vec::with_capacity(stair_count.min(1024));
    for _ in 0..stair_count {
        stairs.push(read_walkable_stair(reader)?);
    }
    let region_count = reader.count(collision_box_limit(), "walkable region count")?;
    let mut regions = Vec::with_capacity(region_count.min(1024));
    for _ in 0..region_count {
        regions.push(read_walkable_region(reader)?);
    }
    Ok(WalkableRoom {
        x0,
        x1,
        z0,
        z1,
        floor_y,
        ramps,
        stairs,
        regions,
    })
}

/// Encodes one ramp's detached surface and the floor plane it sits on.
fn write_walkable_ramp(writer: &mut Writer, ramp: &WalkableRamp) {
    writer.f32(ramp.surface.x);
    writer.f32(ramp.surface.z);
    writer.f32(ramp.surface.width);
    writer.f32(ramp.surface.depth);
    writer.f32(ramp.surface.offset_y);
    writer.f32(ramp.surface.rise);
    writer.f32(ramp.floor_y);
}

/// Decodes one walkable ramp, rejecting malformed surfaces.
fn read_walkable_ramp(reader: &mut Reader<'_>) -> Result<WalkableRamp, String> {
    let x = reader.f32()?;
    let z = reader.f32()?;
    let width = reader.f32()?;
    let depth = reader.f32()?;
    let offset_y = reader.f32()?;
    let rise = reader.f32()?;
    let floor_y = reader.f32()?;
    if !collision_values_finite(&[x, z, width, depth, offset_y, rise, floor_y]) {
        return Err("walkable ramp has a non-finite value".to_string());
    }
    if width == 0.0 || depth == 0.0 {
        return Err("walkable ramp has a zero width or depth".to_string());
    }
    Ok(WalkableRamp {
        surface: RampSurface {
            x,
            z,
            width,
            depth,
            offset_y,
            rise,
        },
        floor_y,
    })
}

/// Encodes one staircase's detached surface and the floor plane it sits on.
fn write_walkable_stair(writer: &mut Writer, stair: &WalkableStair) {
    writer.f32(stair.surface.x);
    writer.f32(stair.surface.z);
    writer.f32(stair.surface.width);
    writer.f32(stair.surface.depth);
    writer.f32(stair.surface.offset_y);
    writer.f32(stair.surface.rise);
    writer.u32(stair.surface.steps);
    writer.f32(stair.floor_y);
}

/// Decodes one walkable staircase, rejecting malformed surfaces.
fn read_walkable_stair(reader: &mut Reader<'_>) -> Result<WalkableStair, String> {
    let x = reader.f32()?;
    let z = reader.f32()?;
    let width = reader.f32()?;
    let depth = reader.f32()?;
    let offset_y = reader.f32()?;
    let rise = reader.f32()?;
    // `steps` is a `u32`, so a decoded count can never be negative.
    let steps = reader.u32()?;
    let floor_y = reader.f32()?;
    if !collision_values_finite(&[x, z, width, depth, offset_y, rise, floor_y]) {
        return Err("walkable stair has a non-finite value".to_string());
    }
    if width == 0.0 || depth == 0.0 {
        return Err("walkable stair has a zero width or depth".to_string());
    }
    Ok(WalkableStair {
        surface: StairSurface {
            x,
            z,
            width,
            depth,
            offset_y,
            rise,
            steps,
        },
        floor_y,
    })
}

/// Encodes one floor region.
fn write_walkable_region(writer: &mut Writer, region: &WalkableRegion) {
    writer.f32(region.x0);
    writer.f32(region.x1);
    writer.f32(region.z0);
    writer.f32(region.z1);
    writer.f32(region.y);
}

/// Decodes one floor region.
fn read_walkable_region(reader: &mut Reader<'_>) -> Result<WalkableRegion, String> {
    let x0 = reader.f32()?;
    let x1 = reader.f32()?;
    let z0 = reader.f32()?;
    let z1 = reader.f32()?;
    let y = reader.f32()?;
    if !collision_values_finite(&[x0, x1, z0, z1, y]) {
        return Err("walkable region has a non-finite value".to_string());
    }
    if x0 > x1 || z0 > z1 {
        return Err("walkable region bounds are inverted".to_string());
    }
    Ok(WalkableRegion { x0, x1, z0, z1, y })
}

/// One room of the walkable ceiling model.
///
/// The room's profile maths stay in [`ceiling_y_for_volume`], so the ceiling a
/// jumping player bumps against is by construction the same surface the mesh
/// and the fixtures resolved.
#[derive(Debug, Clone, PartialEq)]
struct WalkableCeilingRoom {
    /// Room footprint `(x0, x1, z0, z1)`, normalised.
    bounds: (f32, f32, f32, f32),
    floor_y: f32,
    height: f32,
    profile: CeilingProfileDef,
}

impl WalkableCeilingRoom {
    /// True when `(x, z)` lies inside the footprint, with the same edge
    /// tolerance every other surface query uses.
    fn contains(&self, x: f32, z: f32) -> bool {
        if !x.is_finite() || !z.is_finite() {
            return false;
        }
        let (x0, x1, z0, z1) = self.bounds;
        x >= x0 - ROOM_EDGE_EPS_M
            && x <= x1 + ROOM_EDGE_EPS_M
            && z >= z0 - ROOM_EDGE_EPS_M
            && z <= z1 + ROOM_EDGE_EPS_M
    }

    /// World Y of this room's ceiling at `(x, z)`.
    fn ceiling_y_at(&self, x: f32, z: f32) -> f32 {
        ceiling_y_for_volume(self.bounds, self.floor_y, self.height, self.profile, x, z)
    }
}

/// Owned, allocation-light ceiling model a controller samples while jumping.
///
/// It is built once per level from the same rooms and profiles
/// [`LevelSurfaces::ceiling_y_at`] resolves against, including a gable's
/// ridge, but borrows nothing from the definition. Unlike the borrowing query a
/// point outside every room answers `None`: an off-room spawn or a walkable
/// void has no ceiling to clamp against.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WalkableCeiling {
    rooms: Vec<WalkableCeilingRoom>,
}

impl WalkableCeiling {
    /// Builds the ceiling model for a level.
    #[must_use]
    pub fn from_level(level: &LevelDef) -> Self {
        let rooms = level
            .room_iter()
            // An open-ceiling room has no surface to clamp a jumping player
            // against; it contributes nothing to this model.
            .filter(|room| !room.ceiling.is_open())
            .map(|room| WalkableCeilingRoom {
                bounds: room.bounds(),
                floor_y: if room.floor_y.is_finite() {
                    room.floor_y
                } else {
                    0.0
                },
                height: room.height,
                profile: room.ceiling,
            })
            .collect();
        Self { rooms }
    }

    /// True when the level contains no rooms at all.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }

    /// World Y of the ceiling over `(x, z)`, or `None` outside every room.
    ///
    /// The first room in level order containing the point wins, matching the
    /// ownership rule of [`LevelSurfaces::ceiling_y_at`].
    #[must_use]
    pub fn ceiling_y_at(&self, x: f32, z: f32) -> Option<f32> {
        self.rooms
            .iter()
            .find(|room| room.contains(x, z))
            .map(|room| room.ceiling_y_at(x, z))
    }

    /// Encodes this ceiling model into a compiled collision record: the room
    /// count, then each room's footprint, eave and profile.
    ///
    /// # Errors
    ///
    /// Returns a named error when the model holds more rooms than
    /// [`crate::package::MAX_COLLISION_BOXES`].
    pub(crate) fn write_compiled(&self, writer: &mut Writer) -> Result<(), String> {
        write_collision_count(
            writer,
            self.rooms.len(),
            collision_box_limit(),
            "walkable ceiling room count",
        )?;
        for room in &self.rooms {
            write_ceiling_room(writer, room);
        }
        Ok(())
    }

    /// Decodes a ceiling model from a compiled collision record.
    ///
    /// # Errors
    ///
    /// Returns a named error when the record is truncated, declares an
    /// out-of-range count, or holds a malformed room or profile.
    pub(crate) fn read_compiled(reader: &mut Reader<'_>) -> Result<Self, String> {
        let room_count = reader.count(collision_box_limit(), "walkable ceiling room count")?;
        let mut rooms = Vec::with_capacity(room_count.min(1024));
        for _ in 0..room_count {
            rooms.push(read_ceiling_room(reader)?);
        }
        Ok(Self { rooms })
    }
}

/// Profile byte code of a flat ceiling.
const CEILING_PROFILE_FLAT: u8 = 0;

/// Profile byte code of a gable ceiling.
const CEILING_PROFILE_GABLE: u8 = 1;

/// Ridge-axis byte code of a gable running along X.
const CEILING_RIDGE_X: u8 = 0;

/// Ridge-axis byte code of a gable running along Z.
const CEILING_RIDGE_Z: u8 = 1;

/// Encodes one walkable ceiling room: its footprint, eave and profile.
fn write_ceiling_room(writer: &mut Writer, room: &WalkableCeilingRoom) {
    let (x0, x1, z0, z1) = room.bounds;
    writer.f32(x0);
    writer.f32(x1);
    writer.f32(z0);
    writer.f32(z1);
    writer.f32(room.floor_y);
    writer.f32(room.height);
    match room.profile {
        // An open room never reaches the record: `WalkableCeiling::from_level`
        // filters it. A stray one degrades to its eave (flat), never a panic.
        CeilingProfileDef::Flat | CeilingProfileDef::Open => writer.u8(CEILING_PROFILE_FLAT),
        CeilingProfileDef::Gable { ridge, ridge_rise } => {
            writer.u8(CEILING_PROFILE_GABLE);
            writer.u8(match ridge {
                WallAxis::X => CEILING_RIDGE_X,
                WallAxis::Z => CEILING_RIDGE_Z,
            });
            writer.f32(ridge_rise);
        }
    }
}

/// Decodes one walkable ceiling room.
fn read_ceiling_room(reader: &mut Reader<'_>) -> Result<WalkableCeilingRoom, String> {
    let x0 = reader.f32()?;
    let x1 = reader.f32()?;
    let z0 = reader.f32()?;
    let z1 = reader.f32()?;
    let floor_y = reader.f32()?;
    let height = reader.f32()?;
    if !collision_values_finite(&[x0, x1, z0, z1, floor_y, height]) {
        return Err("walkable ceiling room has a non-finite value".to_string());
    }
    if x0 > x1 || z0 > z1 {
        return Err("walkable ceiling room bounds are inverted".to_string());
    }
    let profile = match reader.u8()? {
        CEILING_PROFILE_FLAT => CeilingProfileDef::Flat,
        CEILING_PROFILE_GABLE => {
            let ridge = match reader.u8()? {
                CEILING_RIDGE_X => WallAxis::X,
                CEILING_RIDGE_Z => WallAxis::Z,
                other => return Err(format!("unknown gable ridge axis code {other}")),
            };
            let ridge_rise = reader.f32()?;
            if !ridge_rise.is_finite() {
                return Err("walkable gable has a non-finite ridge rise".to_string());
            }
            CeilingProfileDef::Gable { ridge, ridge_rise }
        }
        other => return Err(format!("unknown ceiling profile code {other}")),
    };
    Ok(WalkableCeilingRoom {
        bounds: (x0, x1, z0, z1),
        floor_y,
        height,
        profile,
    })
}

/// One water volume resolved against the level's floors: a footprint, a
/// horizontal surface, a bottom and the author's material/opacity/swimming
/// contract.
///
/// The bottom is resolved once, at level load: an authored `bottom_y` is kept,
/// and an omitted one becomes the lowest walkable floor under the footprint
/// (so a basin's water is as deep as the basin) with a `surface_y - 2.0`
/// fallback for a volume that floats over the void. Physics always stands on
/// the level's own walkable floor; the resolved bottom exists so authors and
/// renders can reason about the body as a whole.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterVolume {
    /// Footprint shape: the rectangle `x0..x1` × `z0..z1`, or the disc of
    /// [`WaterVolume::radius`] inscribed in that bounding box.
    pub shape: WaterShape,
    pub x0: f32,
    pub x1: f32,
    pub z0: f32,
    pub z1: f32,
    /// Circle radius in metres; `0.0` for a rectangle.
    pub radius: f32,
    /// World Y of the free surface.
    pub surface_y: f32,
    /// World Y of the resolved bottom, always below `surface_y`.
    pub bottom_y: f32,
    /// Authored surface material id, or `None` for [`DEFAULT_WATER_MATERIAL`].
    pub material: Option<String>,
    /// Effective opacity of the surface, `0.0..=1.0`.
    pub opacity: f32,
    /// Whether the controller swims in this volume.
    pub swimming: bool,
    /// Whether the volume participates in the water sampling at all.
    ///
    /// A `disable` action clears this: the surface still draws (it is baked
    /// geometry) but the controller walks or falls through the volume.
    pub enabled: bool,
}

impl WaterVolume {
    /// Footprint centre `(x, z)`: the bounding box midpoint for a rectangle
    /// and the disc's centre for a circle.
    #[must_use]
    pub const fn center(&self) -> (f32, f32) {
        (
            f32::midpoint(self.x0, self.x1),
            f32::midpoint(self.z0, self.z1),
        )
    }

    /// True when `(x, z)` lies inside the footprint: the rectangle, or the
    /// disc.
    ///
    /// A circle's rim is the wall line and is **dry** (strictly inside the
    /// radius), so its bounding box's axis extremes are not water.
    #[must_use]
    pub fn contains(&self, x: f32, z: f32) -> bool {
        if !x.is_finite() || !z.is_finite() {
            return false;
        }
        match self.shape {
            WaterShape::Rect => x >= self.x0 && x <= self.x1 && z >= self.z0 && z <= self.z1,
            WaterShape::Circle => {
                let (cx, cz) = self.center();
                let dx = x - cx;
                let dz = z - cz;
                dx.mul_add(dx, dz * dz) < self.radius * self.radius
            }
        }
    }

    /// True when the whole disc of `radius` around `(x, z)` lies inside this
    /// volume's own footprint.
    ///
    /// A rectangle keeps the historical box containment; a circle requires the
    /// disc to fit inside the circle, so a floating prop can never poke past
    /// the rim of a round pool.
    #[must_use]
    pub fn contains_disc(&self, x: f32, z: f32, radius: f32) -> bool {
        if !x.is_finite() || !z.is_finite() || !radius.is_finite() || radius < 0.0 {
            return false;
        }
        match self.shape {
            WaterShape::Rect => {
                x - radius >= self.x0
                    && x + radius <= self.x1
                    && z - radius >= self.z0
                    && z + radius <= self.z1
            }
            WaterShape::Circle => {
                let (cx, cz) = self.center();
                let dx = x - cx;
                let dz = z - cz;
                let clearance = self.radius - radius;
                clearance >= 0.0 && dx.mul_add(dx, dz * dz) <= clearance * clearance
            }
        }
    }

    /// Resolved depth below the surface, in metres; always positive.
    #[must_use]
    pub fn depth(&self) -> f32 {
        (self.surface_y - self.bottom_y).max(0.0)
    }

    /// Surface material id: the authored one, or [`DEFAULT_WATER_MATERIAL`].
    #[must_use]
    pub fn material_id(&self) -> &str {
        self.material.as_deref().unwrap_or(DEFAULT_WATER_MATERIAL)
    }
}

/// What one point inside a water volume reports to the controller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaterSample {
    /// World Y of the free surface.
    pub surface_y: f32,
    /// World Y of the volume's resolved bottom.
    pub bottom_y: f32,
    /// Surface-to-bottom depth, in metres.
    pub depth: f32,
    /// Whether the volume allows swimming.
    pub swimming: bool,
}

/// The level's water volumes, resolved once at load time.
///
/// Lookups are linear over the level's authored volumes (bounded by
/// [`MAX_LEVEL_WATER_VOLUMES`]) and allocation-free, exactly like the walkable
/// floor's room scan; a level with no `water` array is empty and every query
/// returns `None`, which is the historical dry level.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WaterVolumes {
    volumes: Vec<WaterVolume>,
}

/// Points sampled per rectangle when resolving an omitted `bottom_y`: the
/// centre and the four footprint corners.
const WATER_BOTTOM_SAMPLES: [(f32, f32); 5] =
    [(0.5, 0.5), (0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];

/// Points sampled per circle when resolving an omitted `bottom_y`: the centre
/// and four points at half the radius, on the diagonals. They lie strictly
/// inside the disc, so the resolution samples the circle's own footprint
/// rather than the bounding box corners a square would wrongly include.
const WATER_CIRCLE_SAMPLES: [(f32, f32); 5] = [
    (0.0, 0.0),
    (-0.353_553_4, -0.353_553_4),
    (0.353_553_4, -0.353_553_4),
    (0.353_553_4, 0.353_553_4),
    (-0.353_553_4, 0.353_553_4),
];

impl WaterVolumes {
    /// An empty set: every query misses.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            volumes: Vec::new(),
        }
    }

    /// Resolves every volume against the level's walkable floors.
    #[must_use]
    pub fn from_level(level: &LevelDef) -> Self {
        let surfaces = LevelSurfaces::new(level);
        let mut volumes = Vec::with_capacity(level.water.len());
        for def in &level.water {
            let (x0, x1, z0, z1) = def.bounds();
            if !x0.is_finite() || !x1.is_finite() || !z0.is_finite() || !z1.is_finite() {
                continue;
            }
            // A degenerate footprint is malformed input the loader rejects;
            // resolution skips it so it can never draw or answer a query.
            if x1 <= x0 || z1 <= z0 {
                continue;
            }
            let radius = match def.shape {
                WaterShape::Circle => match def.radius {
                    Some(radius) if radius.is_finite() && radius > 0.0 => radius,
                    _ => continue,
                },
                WaterShape::Rect => 0.0,
            };
            let surface_y = def.surface_y;
            if !surface_y.is_finite() {
                continue;
            }
            let bottom = def
                .bottom_y
                .filter(|value| value.is_finite())
                .unwrap_or_else(|| {
                    let mut lowest = f32::INFINITY;
                    match def.shape {
                        WaterShape::Rect => {
                            for (u, v) in WATER_BOTTOM_SAMPLES {
                                let x = (x1 - x0).mul_add(u, x0);
                                let z = (z1 - z0).mul_add(v, z0);
                                if let Some(floor) = surfaces.floor_y_at(x, z) {
                                    lowest = lowest.min(floor);
                                }
                            }
                        }
                        WaterShape::Circle => {
                            let (cx, cz) = (f32::midpoint(x0, x1), f32::midpoint(z0, z1));
                            for (du, dv) in WATER_CIRCLE_SAMPLES {
                                let x = radius.mul_add(du, cx);
                                let z = radius.mul_add(dv, cz);
                                if let Some(floor) = surfaces.floor_y_at(x, z) {
                                    lowest = lowest.min(floor);
                                }
                            }
                        }
                    }
                    if lowest.is_finite() {
                        lowest
                    } else {
                        surface_y - 2.0
                    }
                });
            // A bottom at or above the surface has no body to swim in; keep it
            // strictly below so depth() is always positive.
            let bottom_y = if bottom < surface_y {
                bottom
            } else {
                surface_y - 0.05
            };
            volumes.push(WaterVolume {
                shape: def.shape,
                x0,
                x1,
                z0,
                z1,
                radius,
                surface_y,
                bottom_y,
                material: def.material.clone(),
                opacity: def.opacity(),
                swimming: def.swimming,
                enabled: true,
            });
        }
        Self { volumes }
    }

    /// Enables or disables the volume at `index`.
    ///
    /// Returns whether the state changed; an out-of-range index returns false.
    /// A disabled volume is skipped by every query, so the controller treats
    /// its footprint as dry.
    pub fn set_enabled(&mut self, index: usize, enabled: bool) -> bool {
        let Some(volume) = self.volumes.get_mut(index) else {
            return false;
        };
        if volume.enabled == enabled {
            return false;
        }
        volume.enabled = enabled;
        true
    }

    /// True when the level defines no volumes.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.volumes.is_empty()
    }

    /// Number of resolved volumes.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.volumes.len()
    }

    /// Every resolved volume, in authored order.
    #[must_use]
    pub fn volumes(&self) -> &[WaterVolume] {
        &self.volumes
    }

    /// The water at `(x, z, y)`, or `None` when the point is outside every
    /// footprint or above its surface.
    ///
    /// Overlapping volumes resolve like overlapping floor regions: the last one
    /// authored wins.
    #[must_use]
    pub fn sample(&self, x: f32, z: f32, y: f32) -> Option<WaterSample> {
        if !y.is_finite() {
            return None;
        }
        for volume in self.volumes.iter().rev() {
            if !volume.enabled || !volume.contains(x, z) || y > volume.surface_y {
                continue;
            }
            return Some(WaterSample {
                surface_y: volume.surface_y,
                bottom_y: volume.bottom_y,
                depth: volume.depth(),
                swimming: volume.swimming,
            });
        }
        None
    }

    /// World Y of the topmost surface covering `(x, z)`, ignoring height.
    #[must_use]
    pub fn surface_y_at(&self, x: f32, z: f32) -> Option<f32> {
        self.volumes
            .iter()
            .filter(|volume| volume.enabled && volume.contains(x, z))
            .map(|volume| volume.surface_y)
            .reduce(f32::max)
    }

    /// True when some volume contains the whole horizontal disc of `radius`
    /// around `(x, z)`.
    ///
    /// This is the containment guarantee behind a floating prop: the authored
    /// footprint (plus its heel excursion) must fit inside one basin, so the
    /// hull can never poke through a rim, whatever the phase. A circle
    /// requires the disc to fit inside the circle, not merely inside its
    /// bounding box.
    #[must_use]
    pub fn contains_disc(&self, x: f32, z: f32, radius: f32) -> bool {
        if !x.is_finite() || !z.is_finite() || !radius.is_finite() || radius < 0.0 {
            return false;
        }
        self.volumes
            .iter()
            .filter(|volume| volume.enabled)
            .any(|volume| volume.contains_disc(x, z, radius))
    }

    /// Encodes this volume set into a compiled collision record: the volume
    /// count, then each volume's footprint, surface, bottom and contract.
    ///
    /// # Errors
    ///
    /// Returns a named error when the set holds more volumes than the format
    /// admits or a material id is longer than the record bound.
    pub(crate) fn write_compiled(&self, writer: &mut Writer) -> Result<(), String> {
        write_collision_count(
            writer,
            self.volumes.len(),
            MAX_COLLISION_WATER_VOLUMES,
            "water volume count",
        )?;
        for volume in &self.volumes {
            write_water_volume(writer, volume)?;
        }
        Ok(())
    }

    /// Decodes a volume set from a compiled collision record.
    ///
    /// # Errors
    ///
    /// Returns a named error when the record is truncated, declares an
    /// out-of-range count, or holds a malformed volume.
    pub(crate) fn read_compiled(reader: &mut Reader<'_>) -> Result<Self, String> {
        let volume_count = reader.count(MAX_COLLISION_WATER_VOLUMES, "water volume count")?;
        let mut volumes = Vec::with_capacity(volume_count.min(256));
        for _ in 0..volume_count {
            volumes.push(read_water_volume(reader)?);
        }
        Ok(Self { volumes })
    }
}

/// Shape code of a rectangular compiled water volume.
const WATER_SHAPE_RECT: u8 = 0;
/// Shape code of a circular compiled water volume.
const WATER_SHAPE_CIRCLE: u8 = 1;

/// Encodes one water volume: footprint, shape, surface, bottom, contract and
/// material.
///
/// Record version 2 adds the shape byte and the circle radius after
/// `opacity`; a version-1 record would misread the shape byte as `swimming`,
/// so it is refused by name.
fn write_water_volume(writer: &mut Writer, volume: &WaterVolume) -> Result<(), String> {
    writer.f32(volume.x0);
    writer.f32(volume.x1);
    writer.f32(volume.z0);
    writer.f32(volume.z1);
    writer.f32(volume.surface_y);
    writer.f32(volume.bottom_y);
    writer.f32(volume.opacity);
    match volume.shape {
        WaterShape::Rect => {
            writer.u8(WATER_SHAPE_RECT);
            writer.f32(0.0);
        }
        WaterShape::Circle => {
            writer.u8(WATER_SHAPE_CIRCLE);
            writer.f32(volume.radius);
        }
    }
    writer.bool(volume.swimming);
    match volume.material.as_deref() {
        None => writer.u8(0),
        Some(material) => {
            if u64::try_from(material.len()).unwrap_or(u64::MAX)
                > MAX_COLLISION_WATER_MATERIAL_BYTES
            {
                return Err(format!(
                    "water volume material id is {} bytes \
                     (limit {MAX_COLLISION_WATER_MATERIAL_BYTES})",
                    material.len()
                ));
            }
            writer.u8(1);
            writer.str(material)?;
        }
    }
    Ok(())
}

/// Decodes one water volume, rejecting a malformed shape/radius combination.
fn read_water_volume(reader: &mut Reader<'_>) -> Result<WaterVolume, String> {
    let x0 = reader.f32()?;
    let x1 = reader.f32()?;
    let z0 = reader.f32()?;
    let z1 = reader.f32()?;
    let surface_y = reader.f32()?;
    let bottom_y = reader.f32()?;
    let opacity = reader.f32()?;
    if !collision_values_finite(&[x0, x1, z0, z1, surface_y, bottom_y, opacity]) {
        return Err("water volume has a non-finite value".to_string());
    }
    if x1 <= x0 || z1 <= z0 {
        return Err("water volume bounds are inverted or empty".to_string());
    }
    let shape = match reader.u8()? {
        WATER_SHAPE_RECT => WaterShape::Rect,
        WATER_SHAPE_CIRCLE => WaterShape::Circle,
        other => return Err(format!("unknown water volume shape code {other}")),
    };
    let radius = reader.f32()?;
    if !radius.is_finite() {
        return Err("water volume has a non-finite radius".to_string());
    }
    match shape {
        WaterShape::Rect => {
            if radius != 0.0 {
                return Err("rectangular water volume carries a radius".to_string());
            }
        }
        WaterShape::Circle => {
            if radius <= 0.0 {
                return Err("circular water volume radius must be positive".to_string());
            }
            // The bounding box and the radius describe one disc; a record
            // whose box does not match a radius would make the drawn surface
            // and the membership test disagree.
            let diameter = 2.0 * radius;
            if (x1 - x0 - diameter).abs() > 1.0e-3 || (z1 - z0 - diameter).abs() > 1.0e-3 {
                return Err("circular water volume bounds do not match its radius".to_string());
            }
        }
    }
    let swimming = reader.bool()?;
    let material = match reader.u8()? {
        0 => None,
        1 => Some(reader.str(MAX_COLLISION_WATER_MATERIAL_BYTES)?),
        other => return Err(format!("invalid water material marker {other}")),
    };
    Ok(WaterVolume {
        shape,
        x0,
        x1,
        z0,
        z1,
        radius,
        surface_y,
        bottom_y,
        material,
        opacity,
        swimming,
        enabled: true,
    })
}

/// One ladder resolved against the level, ready for the controller.
///
/// The facing vector is the direction the climber moves while climbing, in the
/// same convention as player movement: `(sin(yaw), -cos(yaw))` in `(x, z)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ladder {
    pub x0: f32,
    pub x1: f32,
    pub z0: f32,
    pub z1: f32,
    pub bottom_y: f32,
    pub top_y: f32,
    pub facing_x: f32,
    pub facing_z: f32,
}

impl Ladder {
    /// The footprint's centre `(x, z)`.
    #[must_use]
    pub const fn center(&self) -> (f32, f32) {
        (
            f32::midpoint(self.x0, self.x1),
            f32::midpoint(self.z0, self.z1),
        )
    }

    /// True when the player's disc at `(x, z)` touches the footprint.
    #[must_use]
    pub fn overlaps_disc(&self, x: f32, z: f32, radius: f32) -> bool {
        let closest_x = x.clamp(self.x0, self.x1);
        let closest_z = z.clamp(self.z0, self.z1);
        let dx = x - closest_x;
        let dz = z - closest_z;
        dx.mul_add(dx, dz * dz) < radius * radius
    }

    /// True when the player is on the approach side of the ladder: the side the
    /// climber comes from, opposite the climb direction.
    ///
    /// Incidental contact from the exit side never attaches: a player standing
    /// on the deck beyond the ladder is past its centre along `facing`.
    #[must_use]
    pub fn approach_side(&self, x: f32, z: f32) -> bool {
        let (cx, cz) = self.center();
        (x - cx).mul_add(self.facing_x, (z - cz) * self.facing_z) <= 0.0
    }

    /// True when the body spanning `[eye - body_height, eye]` overlaps the
    /// ladder's authored vertical reach.
    #[must_use]
    pub fn overlaps_body_y(&self, eye: f32, body_height: f32) -> bool {
        eye > self.bottom_y && eye - body_height < self.top_y
    }

    /// The component of a movement direction along the climb direction.
    ///
    /// The direction need not be normalised; the caller normalises once. A
    /// positive value means "climb up", negative "climb down".
    #[must_use]
    pub fn climb_intent(&self, dir_x: f32, dir_z: f32) -> f32 {
        dir_x.mul_add(self.facing_x, dir_z * self.facing_z)
    }
}

/// The level's ladders, resolved once at load time.
///
/// Lookups are linear over the authored list, like the water volumes; a level
/// with no `ladders` array is empty and every query misses.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ladders {
    ladders: Vec<Ladder>,
}

impl Ladders {
    /// An empty set: every query misses.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ladders: Vec::new(),
        }
    }

    /// Resolves every authored ladder.
    #[must_use]
    pub fn from_level(level: &LevelDef) -> Self {
        let mut ladders = Vec::with_capacity(level.ladders.len());
        for def in &level.ladders {
            let (x0, x1, z0, z1) = def.bounds();
            if !x0.is_finite()
                || !x1.is_finite()
                || !z0.is_finite()
                || !z1.is_finite()
                || !def.bottom_y.is_finite()
                || !def.top_y.is_finite()
                || def.top_y <= def.bottom_y
            {
                continue;
            }
            let yaw = if def.facing_degrees.is_finite() {
                def.facing_degrees.to_radians()
            } else {
                0.0
            };
            ladders.push(Ladder {
                x0,
                x1,
                z0,
                z1,
                bottom_y: def.bottom_y,
                top_y: def.top_y,
                facing_x: yaw.sin(),
                facing_z: -yaw.cos(),
            });
        }
        Self { ladders }
    }

    /// True when the level defines no ladders.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.ladders.is_empty()
    }

    /// Number of resolved ladders.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.ladders.len()
    }

    /// The ladder at `index`, if it exists.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Ladder> {
        self.ladders.get(index)
    }

    /// The first ladder whose footprint the disc at `(x, z)` touches.
    #[must_use]
    pub fn overlapping(&self, x: f32, z: f32, radius: f32) -> Option<usize> {
        self.ladders
            .iter()
            .position(|ladder| ladder.overlaps_disc(x, z, radius))
    }

    /// Encodes this ladder set into a compiled collision record: the count,
    /// then each ladder's footprint, reach and facing.
    ///
    /// # Errors
    ///
    /// Returns a named error when the set holds more ladders than the format
    /// admits.
    pub(crate) fn write_compiled(&self, writer: &mut Writer) -> Result<(), String> {
        write_collision_count(
            writer,
            self.ladders.len(),
            MAX_COLLISION_LADDERS,
            "ladder count",
        )?;
        for ladder in &self.ladders {
            write_ladder(writer, ladder);
        }
        Ok(())
    }

    /// Decodes a ladder set from a compiled collision record.
    ///
    /// # Errors
    ///
    /// Returns a named error when the record is truncated, declares an
    /// out-of-range count, or holds a malformed ladder.
    pub(crate) fn read_compiled(reader: &mut Reader<'_>) -> Result<Self, String> {
        let count = reader.count(MAX_COLLISION_LADDERS, "ladder count")?;
        let mut ladders = Vec::with_capacity(count.min(256));
        for _ in 0..count {
            ladders.push(read_ladder(reader)?);
        }
        Ok(Self { ladders })
    }
}

/// Encodes one ladder volume.
fn write_ladder(writer: &mut Writer, ladder: &Ladder) {
    writer.f32(ladder.x0);
    writer.f32(ladder.x1);
    writer.f32(ladder.z0);
    writer.f32(ladder.z1);
    writer.f32(ladder.bottom_y);
    writer.f32(ladder.top_y);
    writer.f32(ladder.facing_x);
    writer.f32(ladder.facing_z);
}

/// Decodes one ladder volume.
fn read_ladder(reader: &mut Reader<'_>) -> Result<Ladder, String> {
    let x0 = reader.f32()?;
    let x1 = reader.f32()?;
    let z0 = reader.f32()?;
    let z1 = reader.f32()?;
    let bottom_y = reader.f32()?;
    let top_y = reader.f32()?;
    let facing_x = reader.f32()?;
    let facing_z = reader.f32()?;
    if !collision_values_finite(&[x0, x1, z0, z1, bottom_y, top_y, facing_x, facing_z]) {
        return Err("ladder has a non-finite value".to_string());
    }
    if x0 > x1 || z0 > z1 {
        return Err("ladder bounds are inverted".to_string());
    }
    Ok(Ladder {
        x0,
        x1,
        z0,
        z1,
        bottom_y,
        top_y,
        facing_x,
        facing_z,
    })
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
