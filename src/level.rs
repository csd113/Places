use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::collision::WallAabb;

fn default_ceiling_height() -> f32 {
    3.5
}

/// Rectangular room section defining floor and ceiling boundaries.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WallAxis {
    X,
    Z,
}

impl WallAxis {
    /// Picks the axis a wall of the given dimensions runs along.
    ///
    /// The wall's length is the larger of `width`/`depth`; ties resolve to `X`.
    pub fn of(width: f32, depth: f32) -> WallAxis {
        if width >= depth {
            WallAxis::X
        } else {
            WallAxis::Z
        }
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
    /// Rectangular cutouts (doors, windows, passages, vents) through this wall.
    #[serde(default)]
    pub openings: Vec<WallOpeningDef>,
}

impl WallDef {
    pub fn resolved_height(&self, default_ceiling: f32) -> f32 {
        self.height.unwrap_or(default_ceiling)
    }

    /// The axis this wall's length runs along (the larger of width/depth).
    pub fn axis(&self) -> WallAxis {
        WallAxis::of(self.width.abs(), self.depth.abs())
    }

    /// Length of the wall's footprint along its length axis, in metres.
    pub fn length(&self) -> f32 {
        match self.axis() {
            WallAxis::X => self.width.abs(),
            WallAxis::Z => self.depth.abs(),
        }
    }

    /// Thickness of the wall across its length axis, in metres.
    pub fn thickness(&self) -> f32 {
        match self.axis() {
            WallAxis::X => self.depth.abs(),
            WallAxis::Z => self.width.abs(),
        }
    }

    /// Minimum (x, z) corner of the wall footprint.
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
    pub fn length_origin(&self) -> (f32, f32) {
        self.min_corner()
    }

    pub fn to_aabb(&self) -> WallAabb {
        let h = self.resolved_height(3.5);
        WallAabb::with_y(self.x, self.y, self.z, self.width, h, self.depth)
    }

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
    pub fn bottom(&self, base_y: f32) -> f32 {
        base_y + self.sill
    }

    /// Absolute Y of the opening's top edge for a wall based at `base_y`.
    pub fn top(&self, base_y: f32) -> f32 {
        base_y + self.sill + self.height
    }

    /// Offset of the opening's far edge along the wall's length axis.
    pub fn end(&self) -> f32 {
        self.offset + self.width
    }

    /// True when the opening reaches the wall base (walk-through doorway).
    pub fn reaches_floor(&self) -> bool {
        self.sill <= 1e-3
    }

    /// True for walk-through openings ("door" and "passage").
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
pub fn wall_solid_slices(wall: &WallDef, ceiling_height: f32) -> Vec<WallSlice> {
    let length = wall.length();
    if !length.is_finite() || length <= WALL_SLICE_EPS {
        return Vec::new();
    }

    let resolved = wall.resolved_height(ceiling_height);
    let base = wall.y.min(wall.y + resolved);
    let ceiling = wall.y.max(wall.y + resolved);
    if !base.is_finite() || !ceiling.is_finite() || ceiling <= base + WALL_SLICE_EPS {
        return Vec::new();
    }

    // Clamp every opening to the wall footprint and vertical range. Malformed
    // entries (non-finite, zero-sized, out of range) are ignored.
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
        let sill = opening.sill.max(0.0);
        let bottom = (base + sill).clamp(base, ceiling);
        let top = (base + sill + opening.height).clamp(base, ceiling);
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

    // Split the wall's length at every opening boundary.
    let mut cuts: Vec<f32> = Vec::with_capacity(openings.len() * 2 + 2);
    cuts.push(0.0);
    cuts.push(length);
    for opening in &openings {
        cuts.push(opening.start);
        cuts.push(opening.end);
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
        if ceiling > cursor + WALL_SLICE_EPS {
            slices.push(WallSlice {
                start,
                end,
                bottom: cursor,
                top: ceiling,
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

/// Ceiling light fixture placement.
///
/// `brightness` is the optional fixture intensity/power. It is the field the
/// level editor already authors and writes, so it stays the canonical key; the
/// more descriptive `intensity` spelling is accepted as an alias so levels
/// written from the design notes load unchanged. Omitted means `1.0`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CeilingLightDef {
    pub fixture: String,
    pub x: f32,
    pub z: f32,
    #[serde(default)]
    pub rotation_degrees: f32,
    #[serde(default, alias = "intensity")]
    pub brightness: Option<f32>,
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
    pub fn intensity(&self) -> f32 {
        match self.brightness {
            None => 1.0,
            Some(value) => crate::lighting::sanitize_intensity(value),
        }
    }
}

/// Fallback prop box extents [width, height, depth] in metres, used whenever
/// neither the placed prop nor the prop catalog provides explicit sizes.
pub const PROP_FALLBACK_SIZE: [f32; 3] = [0.6, 0.9, 0.6];

fn default_prop_scale() -> f32 {
    1.0
}

/// A placed prop / furniture / appliance instance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropDef {
    /// Registry identifier, e.g. "core:couch". Resolved through the prop catalog.
    pub model: String,
    #[serde(default)]
    pub x: f32,
    /// Vertical offset of the prop's base above the floor. Negative values sink the prop into the floor (intentional).
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
    #[serde(default)]
    pub ceiling_lights: Vec<CeilingLightDef>,
    /// Placed props / furniture / appliances.
    #[serde(default)]
    pub props: Vec<PropDef>,
}

/// Number of quads each ceiling light fixture generates (panel plus two bezels).
pub const MAX_LIGHT_QUADS: u64 = 3;
/// Number of quads a prop generates in its placeholder-box form. Real prop
/// geometry is batched separately and bounded by [`MAX_LEVEL_PROP_VERTICES`].
pub const MAX_PROP_QUADS: u64 = 6;
/// Preferred triangle count for one prop model (see `assets/props/README.md`).
pub const PROP_TRIANGLE_TARGET: usize = 500;
/// Triangle count above which a prop model needs an explicit justification.
pub const PROP_TRIANGLE_REVIEW: usize = 800;
/// Hard ceiling on one prop model's triangle count, enforced by the loader.
pub const MAX_PROP_TRIANGLES: usize = 1_500;
/// Hard ceiling on one prop model's vertex count (16-bit indices, PocketCHIP RAM).
pub const MAX_PROP_VERTICES: usize = 65_535;
/// Hard ceiling on prop texture dimensions; 64x64/128x128 are the preferred sizes.
pub const MAX_PROP_TEXTURE_SIZE: u32 = 256;
/// Hard ceiling on the number of distinct prop models a single level may use.
pub const MAX_LEVEL_PROP_MODELS: usize = 256;
/// Upper bound on the summed prop vertex count a level may expand into after
/// instance transforms are baked, keeping one level's prop geometry bounded.
pub const MAX_LEVEL_PROP_VERTICES: usize = 1_500_000;
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
    pub total_vertices: u64,
}

impl LevelDef {
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }

    /// Iterates over all room sections (merging optional `room` and `rooms`)
    /// without cloning or allocating.
    pub fn room_iter(&self) -> impl Iterator<Item = &RoomDef> {
        self.rooms.iter().chain(self.room.iter())
    }

    /// Estimates the generated geometry for this level using saturating
    /// arithmetic, so malformed input cannot overflow the calculation.
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
            // how large a room is.
            let cells = crate::lighting::light_grid_cells(room.width.abs()) as u64
                * crate::lighting::light_grid_cells(room.depth.abs()) as u64;
            floor_quads = floor_quads.saturating_add(cells);
            ceiling_quads = ceiling_quads.saturating_add(cells);
        }

        // Walls are bounded by replaying the same solid-slice decomposition the
        // geometry builder uses (`wall_solid_slices`), so the estimate tracks
        // per-slice segment counts and opening reveals instead of assuming a
        // fixed number of faces per wall. Everything saturates, so malformed
        // dimensions cannot overflow the total.
        let room_refs: Vec<&RoomDef> = self.room_iter().collect();
        let mut wall_quads: u64 = 0;
        for wall in &self.walls {
            let default_height = ceiling_height_at(
                &room_refs,
                wall.x + wall.width * 0.5,
                wall.z + wall.depth * 0.5,
            );
            let slices = wall_solid_slices(wall, default_height);
            for slice in &slices {
                let segments = crate::lighting::wall_light_segments(slice.end - slice.start) as u64;
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
        let light_quads = (self.ceiling_lights.len() as u64).saturating_mul(MAX_LIGHT_QUADS);
        let prop_quads = (self.props.len() as u64).saturating_mul(MAX_PROP_QUADS);
        let total_quads = floor_quads
            .saturating_add(ceiling_quads)
            .saturating_add(wall_quads)
            .saturating_add(light_quads)
            .saturating_add(prop_quads);

        GeometryEstimate {
            floor_area_m2,
            floor_quads,
            ceiling_quads,
            wall_quads,
            light_quads,
            prop_quads,
            total_vertices: total_quads.saturating_mul(6),
        }
    }

    /// Returns collision bounding boxes for all solid level geometry.
    ///
    /// Walls contribute one box per solid slice, so doorways and other openings
    /// are genuinely passable; `solid` props contribute their axis-aligned box.
    pub fn collision_aabbs(&self) -> Vec<WallAabb> {
        let rooms: Vec<&RoomDef> = self.room_iter().collect();
        let mut aabbs = Vec::new();

        for wall in &self.walls {
            let default_h =
                ceiling_height_at(&rooms, wall.x + wall.width * 0.5, wall.z + wall.depth * 0.5);
            let (origin_x, origin_z) = wall.length_origin();
            let (min_x, max_x) = (
                wall.x.min(wall.x + wall.width),
                wall.x.max(wall.x + wall.width),
            );
            let (min_z, max_z) = (
                wall.z.min(wall.z + wall.depth),
                wall.z.max(wall.z + wall.depth),
            );

            for slice in wall_solid_slices(wall, default_h) {
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
            aabbs.push(WallAabb::with_y(
                prop.x - size[0] * 0.5,
                prop.y,
                prop.z - size[2] * 0.5,
                size[0],
                size[1],
                size[2],
            ));
        }

        aabbs
    }
}

/// Returns the room ceiling height for the given (x, z) coordinates.
///
/// Operates on a borrowed room slice so callers can collect the merged room
/// list once and reuse it across many lookups without cloning.
pub fn ceiling_height_at(rooms: &[&RoomDef], x: f32, z: f32) -> f32 {
    for room in rooms {
        let x0 = room.x.min(room.x + room.width);
        let x1 = room.x.max(room.x + room.width);
        let z0 = room.z.min(room.z + room.depth);
        let z1 = room.z.max(room.z + room.depth);
        if x >= x0 - 0.01 && x <= x1 + 0.01 && z >= z0 - 0.01 && z <= z1 + 0.01 {
            return room.height;
        }
    }
    rooms
        .first()
        .map(|r| r.height)
        .unwrap_or_else(default_ceiling_height)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(rooms[0].width, 12.0);
        assert_eq!(rooms[0].height, 3.5);
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
        assert_eq!(aabbs[0].max_x, 10.0);
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
        assert_eq!(level.walls[0].y, 0.0);
        assert_eq!(level.walls[0].height, None);
        assert_eq!(level.walls[0].resolved_height(4.0), 4.0);

        // Wall 1: window sill (half-height)
        assert_eq!(level.walls[1].y, 0.0);
        assert_eq!(level.walls[1].height, Some(1.5));

        // Wall 2: window header (raised)
        assert_eq!(level.walls[2].y, 2.5);
        assert_eq!(level.walls[2].height, Some(1.5));

        let aabbs = level.collision_aabbs();
        assert_eq!(aabbs.len(), 3);
        assert_eq!(aabbs[0].min_y, 0.0);
        assert_eq!(aabbs[0].max_y, 4.0);
        assert_eq!(aabbs[1].min_y, 0.0);
        assert_eq!(aabbs[1].max_y, 1.5);
        assert_eq!(aabbs[2].min_y, 2.5);
        assert_eq!(aabbs[2].max_y, 4.0);
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
        let cap =
            (crate::lighting::MAX_LIGHT_GRID_CELLS * crate::lighting::MAX_LIGHT_GRID_CELLS) as u64;
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
            }],
            spawn: SpawnDef {
                x: 0.0,
                z: 0.0,
                yaw_degrees: 0.0,
            },
            defaults: LevelDefaults::default(),
            walls: Vec::new(),
            floor_patches: Vec::new(),
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
        assert_eq!(door.sill, 0.0);
        assert!(door.is_door());
        assert!(door.reaches_floor());
        assert_eq!(door.end(), 2.0);
        assert_eq!(door.bottom(0.0), 0.0);
        assert_eq!(door.top(0.0), 2.1);
        assert_eq!(door.bottom(1.0), 1.0);
    }

    #[test]
    fn test_wall_axis_and_length_helpers() {
        let x_wall = wall_with_openings("[]");
        assert_eq!(x_wall.axis(), WallAxis::X);
        assert_eq!(x_wall.length(), 4.0);
        assert_eq!(x_wall.thickness(), 0.4);
        assert_eq!(x_wall.min_corner(), (0.0, 0.0));
        assert_eq!(x_wall.length_origin(), (0.0, 0.0));

        let json = r#"{
            "x": 5.0, "z": -3.0, "width": 0.4, "depth": 6.0, "height": 3.5
        }"#;
        let z_wall: WallDef = serde_json::from_str(json).expect("valid z wall");
        assert_eq!(z_wall.axis(), WallAxis::Z);
        assert_eq!(z_wall.length(), 6.0);
        assert_eq!(z_wall.thickness(), 0.4);
        assert_eq!(z_wall.length_origin(), (5.0, -3.0));

        // Negative dimensions still expose a positive length from the min corner.
        let negative: WallDef =
            serde_json::from_str(r#"{ "x": 4.0, "z": 1.0, "width": -4.0, "depth": -0.4 }"#)
                .expect("valid negative wall");
        assert_eq!(negative.axis(), WallAxis::X);
        assert_eq!(negative.length(), 4.0);
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
        assert_eq!(slices[0].start, 0.0);
        assert_eq!(slices[0].end, 1.0);
        assert_eq!((slices[0].bottom, slices[0].top), (0.0, 3.5));
        assert_eq!(slices[1].start, 1.0);
        assert_eq!(slices[1].end, 2.0);
        assert_eq!((slices[1].bottom, slices[1].top), (2.1, 3.5));
        assert_eq!(slices[2].start, 2.0);
        assert_eq!(slices[2].end, 4.0);
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
        assert!(mesh.batches.wall_batch.count as u64 <= estimate.wall_quads * 6);
        let expected_quads = estimate.floor_quads
            + estimate.ceiling_quads
            + estimate.wall_quads
            + estimate.light_quads
            + estimate.prop_quads;
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
        assert!(!header.intersects_player_y());
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
        assert_eq!(aabbs[0].min_x, 1.5);
        assert_eq!(aabbs[0].max_x, 2.5);
        assert_eq!(aabbs[0].min_y, 0.0);
        assert_eq!(aabbs[0].max_y, 1.0);
        assert_eq!(aabbs[0].min_z, 2.5);
        assert_eq!(aabbs[0].max_z, 3.5);
    }
}
