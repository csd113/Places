//! Offline navigation baking.
//!
//! This is the only place walkable space is derived from world geometry. It
//! runs inside the offline compiler, next to the collision build, and writes
//! the [`NavGrid`] the player uploads. The player never calls into this module.
//!
//! The bake rasterises the level's authored walking surfaces (the same
//! [`WalkableFloor`] the movement controller follows, so ramps and stair pitch
//! are sampled exactly as the player walks them) and tests each cell against
//! the real compiled collision boxes for a set of agent classes. Static props
//! collide through the same boxes the player uses, so a solid prop is an
//! obstacle to navigation exactly when it is one to movement. `nav_obstacle`
//! components add boxes explicitly and [`NavBakeInput::walk_proxies`] adds
//! author-declared walkable surfaces; both are reusable inputs for later
//! content without a second format.
//!
//! Two properties matter for correctness:
//!
//! * **Step versus slope.** A neighbour is walkable when the surface rises at
//!   most `step_height` (a step/ledge), or when both cells are part of one
//!   continuous sloped surface and the rise per metre is at most `max_slope`.
//!   That second rule is what makes authored staircases connect: their pitch
//!   slope can exceed a single step per cell even though the player walks them
//!   in fine movement substeps. The movement side keeps its own substep short
//!   enough that `substep * max_slope <= step_height`, asserted by a test, so
//!   the baked rule and the movement rule cannot drift apart.
//! * **Door-open world.** A door leaf is dynamic, so it is never baked as a
//!   static obstacle. The cells a leaf sweeps are recorded as that door's
//!   portal; the runtime blocks them while the door is closed or locked and
//!   re-evaluates them when it opens. Opening a door therefore never rebuilds
//!   the mesh.
//!
//! Independent row tiles are baked in parallel; each worker owns a private
//! collision index and produces a private run that is merged in row order, so
//! a parallel bake is byte-identical to the serial one.

// The navigation/AI runtime is numeric kernel code: bounded `f32` geometry
// over validated finite records, lattice indices converted after their caps
// are enforced, and fixed-size arrays walked by index. Those are exactly the
// shapes the cast/float/index lints flag, so they are allowed here as a unit;
// no other module inherits them, and every allocation and collection access
// still goes through bounds-checked paths.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::imprecise_flops,
    clippy::missing_const_for_fn,
    clippy::needless_range_loop,
    clippy::similar_names,
    clippy::suboptimal_flops,
    clippy::too_many_arguments,
    clippy::too_many_lines
)]

use crate::collision::{DoorCollider, WallAabb};
use crate::collision_index::CollisionIndex;
use crate::door::Doors;
use crate::level::{
    LevelDef, MAX_RAMP_SLOPE, MAX_STAIR_RISER_M, MIN_STAIR_TREAD_M, WalkableCeiling, WalkableFloor,
};
use crate::package::MAX_NAV_CELLS;
use crate::package::navigation::{
    CELL_SURFACE, NO_PORTAL, NO_REGION, NavClass, NavGrid, NavPortal,
};

/// Default cell edge length of a baked navigation grid, in metres.
///
/// A fifth of a metre keeps the tightest authored doorway several cells wide
/// while leaving the grid small: the shipped Demo bakes to roughly sixty
/// thousand cells.
pub const DEFAULT_NAV_CELL_M: f32 = 0.20;

/// The largest slope the bake treats as walkable, in rise per metre of run.
///
/// The loader already bounds ramps at [`MAX_RAMP_SLOPE`] and stairs at
/// `MAX_STAIR_RISER_M / MIN_STAIR_TREAD_M`; this is the larger of the two, so
/// the bake never refuses authored, walkable stair geometry.
pub const NAV_MAX_SLOPE: f32 = {
    let stair = MAX_STAIR_RISER_M / MIN_STAIR_TREAD_M;
    if stair > MAX_RAMP_SLOPE {
        stair
    } else {
        MAX_RAMP_SLOPE
    }
};

/// The movement substep the shared agent mover advances in metres.
///
/// Must satisfy `NAV_MOVE_SUBSTEP_M * NAV_MAX_SLOPE <= step_height` for the
/// default class so a continuous slope is always climbable in one substep; a
/// test asserts the relationship.
pub const NAV_MOVE_SUBSTEP_M: f32 = 0.10;

/// Extra grid margin around the level's floor bounds, in metres.
pub const NAV_BAKE_MARGIN_M: f32 = 0.60;

/// Cell flag bit: the surface under this cell is part of a continuous slope.
pub const CELL_SLOPED: u8 = 1 << 1;

/// Headroom value meaning "no overhead was found above this surface".
pub const HEADROOM_UNBOUNDED_CM: u16 = u16::MAX;

/// The reference agent class: a standing human body.
///
/// Every map bakes this class so a caller never needs an authored `nav_agent`
/// to get a usable mesh.
#[must_use]
pub fn reference_class() -> NavClass {
    NavClass {
        radius: crate::collision::PLAYER_RADIUS,
        height: crate::collision::PLAYER_HEIGHT,
        step_height: crate::collision::PLAYER_STEP_HEIGHT,
        max_slope: NAV_MAX_SLOPE,
    }
}

/// An author-declared walkable surface added to the bake.
///
/// This is the reusable input later content (collision proxies) uses: a
/// rectangle of floor at `y` that the bake treats as walkable in addition to
/// the authored room surfaces. A proxy never removes walkable space; it only
/// adds a surface where one is otherwise absent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavWalkProxy {
    /// World X of the rectangle's minimum corner.
    pub x: f32,
    /// World Z of the rectangle's minimum corner.
    pub z: f32,
    /// Extent along X, in metres.
    pub width: f32,
    /// Extent along Z, in metres.
    pub depth: f32,
    /// Walking-surface height, in world Y.
    pub y: f32,
}

impl NavWalkProxy {
    /// True when the point lies over this proxy's footprint.
    fn contains(&self, x: f32, z: f32) -> bool {
        let (x0, x1) = min_max(self.x, self.x + self.width);
        let (z0, z1) = min_max(self.z, self.z + self.depth);
        x >= x0 && x <= x1 && z >= z0 && z <= z1
    }
}

/// Everything the bake samples.
pub struct NavBakeInput<'a> {
    /// The authored level (rooms, water volumes, door defs).
    pub level: &'a LevelDef,
    /// The compiled static collision boxes, including solid prop boxes.
    pub walls: &'a [WallAabb],
    /// The walking-surface model the movement controller follows.
    pub floor: &'a WalkableFloor,
    /// The ceiling model, for headroom.
    pub ceiling: &'a WalkableCeiling,
    /// The authored doors; leaves are baked as portals, never as static walls.
    pub doors: &'a Doors,
    /// Agent classes to bake, in canonical order.
    pub classes: &'a [NavClass],
    /// Extra collision boxes from `nav_obstacle` components.
    pub obstacles: &'a [WallAabb],
    /// Author-declared walkable surfaces.
    pub walk_proxies: &'a [NavWalkProxy],
}

/// Bake tuning.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavBakeOptions {
    /// Cell edge length in metres.
    pub cell_m: f32,
    /// Worker threads; `1` is a real serial path.
    pub workers: usize,
}

impl Default for NavBakeOptions {
    fn default() -> Self {
        Self {
            cell_m: DEFAULT_NAV_CELL_M,
            workers: 1,
        }
    }
}

/// What one bake did.
#[derive(Debug, Clone, PartialEq)]
pub struct NavBakeReport {
    /// Cell edge length used.
    pub cell_m: f32,
    /// Grid dimensions.
    pub cells_x: u32,
    /// Grid dimensions.
    pub cells_z: u32,
    /// Cells carrying a walking surface.
    pub surface_cells: usize,
    /// Walkable cells per class.
    pub walkable_cells: Vec<usize>,
    /// Connected regions per class.
    pub regions: Vec<u32>,
    /// Door portals recorded.
    pub portals: usize,
    /// Workers actually used.
    pub workers: usize,
}

/// One row run of a bake: `(row, cells, one class mask per class)`.
type RowRun = Vec<(u32, Vec<BakeCell>, Vec<Vec<u8>>)>;

/// One cell's class-independent data.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct BakeCell {
    surface: Option<f32>,
    flags: u8,
    headroom_cm: u16,
    portal: u16,
}

/// Bakes a navigation grid from a level's real geometry.
///
/// # Errors
///
/// Returns a named error when the level has no usable room footprint, when the
/// grid would exceed the record's cell budget, when a class profile is
/// malformed, or when region labels would overflow their 16-bit encoding.
pub fn bake(
    input: &NavBakeInput<'_>,
    options: &NavBakeOptions,
) -> Result<(NavGrid, NavBakeReport), String> {
    if !options.cell_m.is_finite() || options.cell_m <= 0.0 {
        return Err("navigation bake cell size must be finite and positive".to_string());
    }
    if input.classes.is_empty() {
        return Err("navigation bake has no agent classes".to_string());
    }
    for (index, class) in input.classes.iter().enumerate() {
        if !class.is_valid() {
            return Err(format!("navigation bake class {index} is not a valid body"));
        }
    }
    // A level with no rooms has no walkable space. The navigation record is
    // mandatory, so an empty grid is baked and validated like any other; a
    // query against it simply finds nothing.
    let Ok(bounds) = floor_bounds(input.level) else {
        let grid = NavGrid {
            cell_m: options.cell_m,
            origin_x: 0.0,
            origin_z: 0.0,
            cells_x: 0,
            cells_z: 0,
            classes: input.classes.to_vec(),
            cell_y: Vec::new(),
            cell_flags: Vec::new(),
            cell_headroom_cm: Vec::new(),
            cell_portal: Vec::new(),
            portals: Vec::new(),
            walkable: input.classes.iter().map(|_| Vec::new()).collect(),
            region: input.classes.iter().map(|_| Vec::new()).collect(),
        };
        crate::package::navigation::validate_navigation(&grid)?;
        let report = NavBakeReport {
            cell_m: options.cell_m,
            cells_x: 0,
            cells_z: 0,
            surface_cells: 0,
            walkable_cells: input.classes.iter().map(|_| 0).collect(),
            regions: input.classes.iter().map(|_| 0).collect(),
            portals: 0,
            workers: 1,
        };
        return Ok((grid, report));
    };
    let cell_m = options.cell_m;
    let origin_x = ((bounds.0 - NAV_BAKE_MARGIN_M) / cell_m).floor() * cell_m;
    let origin_z = ((bounds.1 - NAV_BAKE_MARGIN_M) / cell_m).floor() * cell_m;
    let span_x = (bounds.2 + NAV_BAKE_MARGIN_M) - origin_x;
    let span_z = (bounds.3 + NAV_BAKE_MARGIN_M) - origin_z;
    let cells_x = clamp_cells((span_x / cell_m).ceil())?;
    let cells_z = clamp_cells((span_z / cell_m).ceil())?;
    let cells = u64::from(cells_x)
        .checked_mul(u64::from(cells_z))
        .ok_or_else(|| "navigation grid dimensions overflow".to_string())?;
    if cells > u64::try_from(MAX_NAV_CELLS).unwrap_or(u64::MAX) {
        return Err(format!(
            "navigation grid would have {cells} cells (limit {MAX_NAV_CELLS}); raise the cell \
             size or split the level"
        ));
    }
    let cell_count = usize::try_from(cells).map_err(|_| "navigation grid is too large")?;

    let mut combined: Vec<WallAabb> =
        Vec::with_capacity(input.walls.len().saturating_add(input.obstacles.len()));
    combined.extend_from_slice(input.walls);
    combined.extend_from_slice(input.obstacles);

    let class_count = input.classes.len();
    let workers = options.workers.max(1).min(cells_z.max(1) as usize);
    let rows_per_worker = cells_z.div_ceil(u32::try_from(workers).unwrap_or(1).max(1));
    let mut ranges: Vec<(u32, u32)> = Vec::with_capacity(workers);
    let mut start = 0u32;
    while start < cells_z {
        let end = start.saturating_add(rows_per_worker).min(cells_z);
        ranges.push((start, end));
        start = end;
    }
    let scene = BakeScene {
        level: input.level,
        walls: &combined,
        floor: input.floor,
        ceiling: input.ceiling,
        doors: input.doors,
        classes: input.classes,
        walk_proxies: input.walk_proxies,
    };

    let work = |start: u32, end: u32| -> RowRun {
        scene.bake_rows(start, end, cell_m, origin_x, origin_z, cells_x)
    };
    let runs: Vec<RowRun> = if ranges.len() <= 1 {
        vec![work(ranges.first().map_or(0, |range| range.0), cells_z)]
    } else {
        std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(ranges.len());
            for (start, end) in &ranges {
                handles.push(scope.spawn(move || work(*start, *end)));
            }
            let mut out = Vec::with_capacity(handles.len());
            for handle in handles {
                match handle.join() {
                    Ok(rows) => out.push(rows),
                    Err(_) => return Err("navigation bake worker panicked".to_string()),
                }
            }
            Ok::<Vec<RowRun>, String>(out)
        })?
    };

    let mut cells_data: Vec<BakeCell> = vec![BakeCell::default(); cell_count];
    let mask_bytes = cell_count.div_ceil(8);
    let mut masks: Vec<Vec<u8>> = (0..class_count).map(|_| vec![0u8; mask_bytes]).collect();
    for run in runs {
        for (row, row_cells, row_masks) in run {
            let row_start = usize::try_from(u64::from(row) * u64::from(cells_x))
                .map_err(|_| "navigation grid is too large")?;
            for (offset, cell) in row_cells.into_iter().enumerate() {
                let target = row_start.saturating_add(offset);
                if let Some(slot) = cells_data.get_mut(target) {
                    *slot = cell;
                }
                for (class, row_mask) in row_masks.iter().enumerate() {
                    let walkable = row_mask
                        .get(offset / 8)
                        .is_some_and(|byte| byte & (1 << (offset % 8)) != 0);
                    if walkable
                        && let Some(mask) = masks.get_mut(class)
                        && let Some(byte) = mask.get_mut(target / 8)
                    {
                        *byte |= 1 << (target % 8);
                    }
                }
            }
        }
    }

    let mut cell_y = Vec::with_capacity(cell_count);
    let mut cell_flags = Vec::with_capacity(cell_count);
    let mut cell_headroom_cm = Vec::with_capacity(cell_count);
    let mut cell_portal = Vec::with_capacity(cell_count);
    let mut surface_cells = 0usize;
    for cell in &cells_data {
        if cell.surface.is_some() {
            surface_cells = surface_cells.saturating_add(1);
        }
        cell_y.push(cell.surface.unwrap_or(0.0));
        cell_flags.push(cell.flags);
        cell_headroom_cm.push(cell.headroom_cm);
        cell_portal.push(cell.portal);
    }

    let portals = collect_portals(input.doors);
    // A cell's portal label arrives from the bake as a door index + 1; resolve
    // it to the portal index the record stores.
    for cell in &mut cell_portal {
        if *cell == NO_PORTAL {
            continue;
        }
        let door_index = usize::from(*cell).saturating_sub(1);
        let door_id = input
            .doors
            .iter()
            .enumerate()
            .find(|(index, _)| *index == door_index)
            .map(|(_, door)| door.def.id.clone());
        *cell = door_id
            .as_deref()
            .and_then(|id| portals.iter().position(|portal| portal.door == id))
            .and_then(|index| u16::try_from(index).ok())
            .unwrap_or(NO_PORTAL);
    }

    // Region labelling per class, using the same neighbour rule the path query
    // uses, so a region label is exactly one traversable component.
    let mut region_runs: Vec<Vec<u16>> = Vec::with_capacity(class_count);
    let mut region_counts: Vec<u32> = Vec::with_capacity(class_count);
    let mut walkable_counts: Vec<usize> = Vec::with_capacity(class_count);
    for (class, mask) in masks.iter().enumerate() {
        let Some(profile) = input.classes.get(class) else {
            continue;
        };
        let (labels, regions, walkable) = label_regions(
            mask,
            &cells_data,
            class,
            cell_count,
            cells_x,
            cell_m,
            profile,
        )?;
        region_runs.push(labels);
        region_counts.push(regions);
        walkable_counts.push(walkable);
    }

    let grid = NavGrid {
        cell_m,
        origin_x,
        origin_z,
        cells_x,
        cells_z,
        classes: input.classes.to_vec(),
        cell_y,
        cell_flags,
        cell_headroom_cm,
        cell_portal,
        portals,
        walkable: masks,
        region: region_runs,
    };
    crate::package::navigation::validate_navigation(&grid)?;
    let report = NavBakeReport {
        cell_m,
        cells_x,
        cells_z,
        surface_cells,
        walkable_cells: walkable_counts,
        regions: region_counts,
        portals: grid.portals.len(),
        workers: ranges.len(),
    };
    Ok((grid, report))
}

/// The immutable per-worker view of the bake inputs.
struct BakeScene<'a> {
    level: &'a LevelDef,
    walls: &'a [WallAabb],
    floor: &'a WalkableFloor,
    ceiling: &'a WalkableCeiling,
    doors: &'a Doors,
    classes: &'a [NavClass],
    walk_proxies: &'a [NavWalkProxy],
}

impl BakeScene<'_> {
    /// Bakes one contiguous run of rows.
    fn bake_rows(
        &self,
        start_row: u32,
        end_row: u32,
        cell_m: f32,
        origin_x: f32,
        origin_z: f32,
        cells_x: u32,
    ) -> RowRun {
        let index = CollisionIndex::build(self.walls);
        let row_count = usize::try_from(end_row.saturating_sub(start_row)).unwrap_or(0);
        let mut rows = Vec::with_capacity(row_count);
        for cz in start_row..end_row {
            let columns = usize::try_from(cells_x).unwrap_or(0);
            let mut cells = Vec::with_capacity(columns);
            let mut masks: Vec<Vec<u8>> = self
                .classes
                .iter()
                .map(|_| vec![0u8; columns.div_ceil(8)])
                .collect();
            for cx in 0..cells_x {
                let half = cell_m * 0.5;
                let x = (cx as f32).mul_add(cell_m, origin_x) + half;
                let z = (cz as f32).mul_add(cell_m, origin_z) + half;
                let cell = self.cell_at(x, z, cell_m, &index);
                for (class, mask) in masks.iter_mut().enumerate() {
                    if self.class_walkable(class, x, z, cell, &index) {
                        let offset = usize::try_from(cx).unwrap_or(0);
                        if let Some(byte) = mask.get_mut(offset / 8) {
                            *byte |= 1 << (offset % 8);
                        }
                    }
                }
                cells.push(cell);
            }
            rows.push((cz, cells, masks));
        }
        rows
    }

    /// One cell's class-independent data.
    fn cell_at(&self, x: f32, z: f32, cell_m: f32, index: &CollisionIndex) -> BakeCell {
        if !x.is_finite() || !z.is_finite() {
            return BakeCell::default();
        }
        let proxy = self.proxy_y(x, z);
        let Some(surface) = self.floor.walk_height_at(x, z).or(proxy) else {
            return BakeCell {
                portal: self.portal_at(x, z, cell_m),
                ..BakeCell::default()
            };
        };
        let surface = match proxy {
            Some(proxy) if proxy > surface + crate::collision::STEP_EPS => proxy,
            _ => surface,
        };
        if !surface.is_finite() {
            return BakeCell::default();
        }
        let flags = CELL_SURFACE | self.slope_flag(x, z, cell_m, surface);
        let headroom = self.headroom_cm(x, z, surface, index);
        BakeCell {
            surface: Some(surface),
            flags,
            headroom_cm: headroom,
            portal: self.portal_at(x, z, cell_m),
        }
    }

    /// True when the cell's surface is part of a continuous slope.
    fn slope_flag(&self, x: f32, z: f32, cell_m: f32, surface: f32) -> u8 {
        let half = cell_m * 0.5;
        let samples = [(x - half, z), (x + half, z), (x, z - half), (x, z + half)];
        for (sx, sz) in samples {
            let mid = self
                .floor
                .walk_height_at(sx, sz)
                .or_else(|| self.proxy_y(sx, sz));
            if let Some(mid) = mid
                && (mid - surface).abs() > 0.02
            {
                return CELL_SLOPED;
            }
        }
        0
    }

    /// The headroom above `surface`, in centimetres.
    fn headroom_cm(&self, x: f32, z: f32, surface: f32, index: &CollisionIndex) -> u16 {
        let mut above = self.ceiling.ceiling_y_at(x, z);
        let radius = self
            .classes
            .iter()
            .map(|class| class.radius)
            .fold(0.0_f32, f32::max);
        if let Some(underside) =
            crate::collision::lowest_underside_indexed(index, x, z, radius, surface, self.walls)
        {
            above = Some(above.map_or(underside, |ceiling| ceiling.min(underside)));
        }
        let Some(above) = above else {
            return HEADROOM_UNBOUNDED_CM;
        };
        if !above.is_finite() {
            return HEADROOM_UNBOUNDED_CM;
        }
        let centimetres = ((above - surface).max(0.0) * 100.0).min(f32::from(u16::MAX - 1));
        centimetres as u16
    }

    /// The proxy surface under a point, when one is authored there.
    fn proxy_y(&self, x: f32, z: f32) -> Option<f32> {
        self.walk_proxies
            .iter()
            .filter(|proxy| proxy.contains(x, z) && proxy.y.is_finite())
            .map(|proxy| proxy.y)
            .reduce(f32::max)
    }

    /// The door portal label of a cell, encoded as `door index + 1`.
    fn portal_at(&self, x: f32, z: f32, cell_m: f32) -> u16 {
        let margin = cell_m * 0.75;
        for (index, door) in self.doors.iter().enumerate() {
            let defined = door.def.width.is_finite()
                && door.def.width > 0.0
                && door.def.height.is_finite()
                && door.def.height > 0.0;
            if !defined {
                continue;
            }
            for step in 0..=4u8 {
                let fraction = f32::from(step) / 4.0;
                let angle = door.def.signed_swing() * fraction;
                let direction = door.def.direction_at(angle);
                let collider = DoorCollider::from_pose(
                    [door.def.x, door.base_y(), door.def.z],
                    direction.into(),
                    door.def.width,
                    door.def.thickness,
                    door.def.height,
                );
                if collider.overlaps_disc(x, z, margin) {
                    return u16::try_from(index.saturating_add(1)).unwrap_or(NO_PORTAL);
                }
            }
        }
        NO_PORTAL
    }

    /// True when `class` can stand at `(x, z)` on this cell.
    fn class_walkable(
        &self,
        class: usize,
        x: f32,
        z: f32,
        cell: BakeCell,
        index: &CollisionIndex,
    ) -> bool {
        let Some(profile) = self.classes.get(class) else {
            return false;
        };
        let Some(surface) = cell.surface else {
            return false;
        };
        if cell.flags & CELL_SURFACE == 0 {
            return false;
        }
        if cell.headroom_cm != HEADROOM_UNBOUNDED_CM
            && f32::from(cell.headroom_cm) < profile.height * 100.0 - 1.0
        {
            return false;
        }
        if self.deep_water_at(x, z, surface, profile.step_height) {
            return false;
        }
        let mut blocked = false;
        index.for_each_disc(x, z, profile.radius, self.walls, |wall| {
            // The index yields a superset of candidates; the exact disc test
            // and the body band both decide.
            if !blocked
                && wall.overlaps_disc(x, z, profile.radius)
                && wall.blocks_body(surface, profile.height)
            {
                blocked = true;
            }
        });
        !blocked
    }

    /// True when a swimming-depth water volume covers the point.
    fn deep_water_at(&self, x: f32, z: f32, surface: f32, step_height: f32) -> bool {
        self.level.water.iter().any(|volume| {
            if !volume.swimming {
                return false;
            }
            let (x0, x1) = min_max(volume.x, volume.x + volume.width);
            let (z0, z1) = min_max(volume.z, volume.z + volume.depth);
            let inside = x >= x0 && x <= x1 && z >= z0 && z <= z1;
            inside && volume.surface_y > surface + step_height
        })
    }
}

/// The label run, region count and walkable count of one class mask.
fn label_regions(
    mask: &[u8],
    cells: &[BakeCell],
    class: usize,
    cell_count: usize,
    cells_x: u32,
    cell_m: f32,
    profile: &NavClass,
) -> Result<(Vec<u16>, u32, usize), String> {
    let bit = |index: usize| -> bool {
        mask.get(index / 8)
            .is_some_and(|byte| byte & (1 << (index % 8)) != 0)
    };
    let mut labels: Vec<u16> = vec![NO_REGION; cell_count];
    let mut walkable = 0usize;
    let mut next_region: u32 = 0;
    let mut stack: Vec<usize> = Vec::new();
    for start in 0..cell_count {
        if labels.get(start).copied() != Some(NO_REGION) || !bit(start) {
            continue;
        }
        if next_region >= u32::from(NO_REGION) {
            return Err(format!(
                "navigation class {class} has more than {} connected regions",
                u16::MAX
            ));
        }
        let region = u16::try_from(next_region).unwrap_or(NO_REGION);
        next_region = next_region.saturating_add(1);
        stack.clear();
        stack.push(start);
        if let Some(slot) = labels.get_mut(start) {
            *slot = region;
        }
        walkable = walkable.saturating_add(1);
        while let Some(index) = stack.pop() {
            let cx = u32::try_from(index).unwrap_or(0) % cells_x.max(1);
            let cz = u32::try_from(index).unwrap_or(0) / cells_x.max(1);
            for (dx, dz) in NEIGHBOURS {
                let Some(nx) = cx.checked_add_signed(dx) else {
                    continue;
                };
                let Some(nz) = cz.checked_add_signed(dz) else {
                    continue;
                };
                if nx >= cells_x {
                    continue;
                }
                let Some(nindex) =
                    usize::try_from(u64::from(nz) * u64::from(cells_x) + u64::from(nx)).ok()
                else {
                    continue;
                };
                if nindex >= cell_count || labels.get(nindex).copied() != Some(NO_REGION) {
                    continue;
                }
                if !bit(nindex) {
                    continue;
                }
                let diagonal = dx != 0 && dz != 0;
                if diagonal {
                    let (Some(a), Some(b)) = (
                        neighbour_index(cx, cz, dx, 0, cells_x, cell_count),
                        neighbour_index(cx, cz, 0, dz, cells_x, cell_count),
                    ) else {
                        continue;
                    };
                    if !bit(a) || !bit(b) {
                        continue;
                    }
                }
                if !neighbour_reachable(cells, index, nindex, diagonal, cell_m, profile) {
                    continue;
                }
                if let Some(slot) = labels.get_mut(nindex) {
                    *slot = region;
                }
                walkable = walkable.saturating_add(1);
                stack.push(nindex);
            }
        }
    }
    Ok((labels, next_region, walkable))
}

/// The row-major index of `(cx + dx, cz + dz)`, when it is inside the grid.
fn neighbour_index(
    cx: u32,
    cz: u32,
    dx: i32,
    dz: i32,
    cells_x: u32,
    cell_count: usize,
) -> Option<usize> {
    let nx = cx.checked_add_signed(dx)?;
    let nz = cz.checked_add_signed(dz)?;
    if nx >= cells_x {
        return None;
    }
    let index = usize::try_from(u64::from(nz) * u64::from(cells_x) + u64::from(nx)).ok()?;
    (index < cell_count).then_some(index)
}

/// The eight cell neighbours, as signed offsets.
const NEIGHBOURS: [(i32, i32); 8] = [
    (-1, 0),
    (1, 0),
    (0, -1),
    (0, 1),
    (-1, -1),
    (1, -1),
    (-1, 1),
    (1, 1),
];

/// True when two adjacent walkable cells connect under the step/slope rule.
fn neighbour_reachable(
    cells: &[BakeCell],
    from: usize,
    to: usize,
    diagonal: bool,
    cell_m: f32,
    profile: &NavClass,
) -> bool {
    let (Some(a), Some(b)) = (cells.get(from), cells.get(to)) else {
        return false;
    };
    let (Some(ya), Some(yb)) = (a.surface, b.surface) else {
        return false;
    };
    let distance = if diagonal {
        cell_m * std::f32::consts::SQRT_2
    } else {
        cell_m
    };
    neighbour_rule(ya, yb, distance, a.flags, b.flags, profile)
}

/// The one step/slope neighbour rule, shared by the bake and the runtime.
///
/// A rise within `profile.step_height` always connects. A larger rise connects
/// only when both cells are part of a continuous slope and the slope is within
/// the profile's bound; a cliff between two flat cells therefore never
/// connects.
#[must_use]
pub fn neighbour_rule(
    ya: f32,
    yb: f32,
    distance: f32,
    flags_a: u8,
    flags_b: u8,
    profile: &NavClass,
) -> bool {
    let rise = (yb - ya).abs();
    if rise <= profile.step_height + crate::collision::STEP_EPS {
        return true;
    }
    if distance <= f32::EPSILON {
        return false;
    }
    let sloped = flags_a & CELL_SLOPED != 0 && flags_b & CELL_SLOPED != 0;
    sloped && rise / distance <= profile.max_slope + 1.0e-4
}

/// Collects one portal per authored door, in authored order.
fn collect_portals(doors: &Doors) -> Vec<NavPortal> {
    doors
        .iter()
        .filter(|door| !door.def.id.trim().is_empty())
        .map(|door| NavPortal {
            door: door.def.id.clone(),
        })
        .collect()
}

/// The union of every room footprint, as `(min_x, min_z, max_x, max_z)`.
fn floor_bounds(level: &LevelDef) -> Result<(f32, f32, f32, f32), String> {
    let mut min_x = f32::INFINITY;
    let mut min_z = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_z = f32::NEG_INFINITY;
    for room in &level.rooms {
        if !room.x.is_finite()
            || !room.z.is_finite()
            || !room.width.is_finite()
            || !room.depth.is_finite()
        {
            continue;
        }
        if room.width <= 0.0 || room.depth <= 0.0 {
            continue;
        }
        let (x0, x1) = min_max(room.x, room.x + room.width);
        let (z0, z1) = min_max(room.z, room.z + room.depth);
        min_x = min_x.min(x0);
        min_z = min_z.min(z0);
        max_x = max_x.max(x1);
        max_z = max_z.max(z1);
    }
    if !min_x.is_finite() || !min_z.is_finite() || !max_x.is_finite() || !max_z.is_finite() {
        return Err("level has no room footprint to bake navigation over".to_string());
    }
    Ok((min_x, min_z, max_x, max_z))
}

/// One grid dimension from a real span, clamped to at least one cell.
fn clamp_cells(value: f32) -> Result<u32, String> {
    if !value.is_finite() || value < 0.0 {
        return Err("navigation grid extent is not finite".to_string());
    }
    let clamped = value.min(f32::from(u16::MAX)).max(1.0);
    Ok(clamped as u32)
}

/// Sorted min/max pair.
fn min_max(a: f32, b: f32) -> (f32, f32) {
    if a <= b { (a, b) } else { (b, a) }
}
