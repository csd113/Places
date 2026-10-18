//! Offline layered navigation baking from the same surfaces as movement.
//!
//! Every XZ column retains all distinct authored storeys, ramps and stair
//! pitch lines. Physical clearance is checked per body, including upper floor
//! undersides and the correct storey's ceiling. Per-class directional links
//! are probed in movement-sized substeps with exact swept solid collision.
//! Diagonals require both orthogonal routes to the identical destination
//! span. Unproven or one-way links are discarded; drops are conservative and
//! use the body's step limit unless a continuous slope proves traversal.
//!
//! Door-open topology stores height-aware portals. Live leaves and moving
//! obstacles are queried at runtime, without rebuilding the baked graph.
//! Sampling rows is bounded, parallel and deterministically merged.

// Preserve exact sentinel comparisons, floating-point operation order and
// cohesive geometry/query stages. Numeric conversions and integer arithmetic
// are audited at their local expressions instead of exempting the module.
#![allow(
    clippy::float_cmp,
    clippy::imprecise_flops,
    clippy::missing_const_for_fn,
    clippy::needless_range_loop,
    clippy::similar_names,
    clippy::suboptimal_flops,
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "Preserve exact sentinel comparisons and established floating-point operation order; named geometry stages and cohesive query parameters keep these numeric kernels readable. Numeric conversions and integer arithmetic exceptions are documented locally."
)]

use crate::engine::collision::{DoorCollider, WallAabb};
use crate::engine::collision_index::CollisionIndex;
use crate::engine::door::Doors;
use crate::engine::level::{LevelDef, WalkableCeiling, WalkableFloor};
use crate::engine::nav::{CELL_SLOPED, neighbour_rule};
use crate::engine::package::navigation::{
    CELL_SURFACE, NAV_DIRECTIONS, NO_NAV_LINK, NO_PORTAL, NO_REGION, NavClass, NavGrid, NavPortal,
    validate_record_budget,
};
use glam::{Vec2, Vec3};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Default cell edge length of a baked navigation grid, in metres.
///
/// A fifth of a metre keeps the tightest authored doorway several cells wide
/// while leaving the grid small: the shipped Demo bakes to roughly sixty
/// thousand cells.
pub const DEFAULT_NAV_CELL_M: f32 = 0.20;

/// Extra grid margin around the level's floor bounds, in metres.
pub const NAV_BAKE_MARGIN_M: f32 = 0.60;

/// Headroom value meaning "no overhead was found above this surface".
pub const HEADROOM_UNBOUNDED_CM: u16 = u16::MAX;

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

/// One node's class-independent data. Columns retain every distinct floor.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct BakeCell {
    surface: Option<f32>,
    flags: u8,
    headroom_cm: u16,
    portal: u16,
}

/// Bakes layered navigation using actual movement surfaces and body probes.
///
/// # Errors
/// Returns a named error for invalid metadata, bounded allocation limits or
/// region overflow. No shipped-map build is required by ordinary Rust tests.
pub fn bake(
    input: &NavBakeInput<'_>,
    options: &NavBakeOptions,
) -> Result<(NavGrid, NavBakeReport), String> {
    if !options.cell_m.is_finite() || options.cell_m < 0.01 {
        return Err("navigation bake cell size must be finite and at least 0.01 m".to_string());
    }
    if input.classes.is_empty() || input.classes.iter().any(|class| !class.is_valid()) {
        return Err("navigation bake requires valid agent classes".to_string());
    }
    let cell_m = options.cell_m;
    let footprint = floor_bounds(input.level).ok();
    let (origin_x, origin_z, cells_x, cells_z) = if let Some(bounds) = footprint {
        let x = ((bounds.0 - NAV_BAKE_MARGIN_M) / cell_m).floor() * cell_m;
        let z = ((bounds.1 - NAV_BAKE_MARGIN_M) / cell_m).floor() * cell_m;
        (
            x,
            z,
            clamp_cells(((bounds.2 + NAV_BAKE_MARGIN_M - x) / cell_m).ceil())?,
            clamp_cells(((bounds.3 + NAV_BAKE_MARGIN_M - z) / cell_m).ceil())?,
        )
    } else {
        (0.0, 0.0, 0, 0)
    };
    let columns = usize::try_from(u64::from(cells_x).saturating_mul(u64::from(cells_z)))
        .map_err(|error| format!("navigation column count is too large: {error}"))?;
    validate_record_budget(columns, 0, input.classes.len())?;
    let mut walls = Vec::with_capacity(input.walls.len().saturating_add(input.obstacles.len()));
    walls.extend_from_slice(input.walls);
    walls.extend_from_slice(input.obstacles);
    let scene = BakeScene {
        level: input.level,
        walls: &walls,
        floor: input.floor,
        ceiling: input.ceiling,
        doors: input.doors,
        classes: input.classes,
        walk_proxies: input.walk_proxies,
    };
    let workers = options
        .workers
        .clamp(1, crate::map_compiler::MAX_WORKERS)
        .min(usize::try_from(cells_z.max(1)).unwrap_or(1));
    let rows_per_worker = cells_z.div_ceil(u32::try_from(workers).unwrap_or(1));
    let node_budget = AtomicUsize::new(0);
    let mut ranges = Vec::with_capacity(workers);
    let mut first = 0u32;
    while first < cells_z {
        let end = first.saturating_add(rows_per_worker).min(cells_z);
        ranges.push((first, end));
        first = end;
    }
    let runs = std::thread::scope(|scope| {
        let mut handles = Vec::with_capacity(ranges.len());
        for &(start, end) in &ranges {
            let scene_ref = &scene;
            let budget_ref = &node_budget;
            handles.push(scope.spawn(move || {
                scene_ref.bake_rows(
                    start, end, cell_m, origin_x, origin_z, cells_x, columns, budget_ref,
                )
            }));
        }
        let mut runs = Vec::with_capacity(handles.len());
        for handle in handles {
            runs.push(
                handle
                    .join()
                    .map_err(|_panic| "navigation bake worker panicked".to_string())??,
            );
        }
        Ok::<_, String>(runs)
    })?;
    let nodes = node_budget.load(Ordering::Relaxed);
    validate_record_budget(columns, nodes, input.classes.len())?;
    let portals = collect_portals(input.doors);
    let mut grid = NavGrid {
        cell_m,
        origin_x,
        origin_z,
        cells_x,
        cells_z,
        column_offsets: Vec::with_capacity(columns.saturating_add(1)),
        links: input
            .classes
            .iter()
            .map(|_| vec![[NO_NAV_LINK; 8]; nodes])
            .collect(),
        classes: input.classes.to_vec(),
        cell_y: Vec::with_capacity(nodes),
        cell_flags: Vec::with_capacity(nodes),
        cell_headroom_cm: Vec::with_capacity(nodes),
        cell_portal: Vec::with_capacity(nodes),
        portals,
        walkable: input
            .classes
            .iter()
            .map(|_| vec![0u8; nodes.div_ceil(8)])
            .collect(),
        region: input
            .classes
            .iter()
            .map(|_| vec![NO_REGION; nodes])
            .collect(),
    };
    grid.column_offsets.push(0);
    for run in runs {
        for column in run {
            for cell in column {
                grid.cell_y.push(cell.surface.unwrap_or(0.0));
                grid.cell_flags.push(cell.flags);
                grid.cell_headroom_cm.push(cell.headroom_cm);
                grid.cell_portal.push(cell.portal);
            }
            grid.column_offsets.push(
                u32::try_from(grid.cell_y.len())
                    .map_err(|error| format!("navigation node count is too large: {error}"))?,
            );
        }
    }
    let index = CollisionIndex::build(&walls);
    for node in 0..nodes {
        let Some((cx, cz)) = grid.coordinates(node) else {
            continue;
        };
        let (x, z) = grid.cell_center(cx, cz);
        let y = grid.surface(node).unwrap_or(0.0);
        for (class, profile) in input.classes.iter().enumerate() {
            if scene.body_clear(x, z, y, profile, &index)
                && let Some(byte) = grid
                    .walkable
                    .get_mut(class)
                    .and_then(|mask| mask.get_mut(node / 8))
            {
                *byte |= 1 << (node % 8);
            }
        }
    }
    // Links are deliberately stored. Runtime queries never infer an edge
    // merely because two XZ columns happen to contain walkable spans.
    for (class, profile) in input.classes.iter().enumerate() {
        for node in 0..nodes {
            if !grid.is_walkable(class, node) {
                continue;
            }
            let Some((cx, cz)) = grid.coordinates(node) else {
                continue;
            };
            let (x, z) = grid.cell_center(cx, cz);
            let from = Vec3::new(x, grid.surface(node).unwrap_or(0.0), z);
            for (direction, (dx, dz)) in NAV_DIRECTIONS.into_iter().enumerate() {
                let Some((nx, nz)) = cx.checked_add_signed(dx).zip(cz.checked_add_signed(dz))
                else {
                    continue;
                };
                let (next_x, next_z) = grid.cell_center(nx, nz);
                let candidate_target = grid
                    .column_range(nx, nz)
                    .filter(|next| grid.is_walkable(class, *next))
                    .filter(|next| {
                        scene.edge_clear(
                            from,
                            Vec3::new(next_x, grid.surface(*next).unwrap_or(0.0), next_z),
                            profile,
                            &index,
                        )
                    })
                    .min_by(|a, b| {
                        (grid.surface(*a).unwrap_or(f32::INFINITY) - from.y)
                            .abs()
                            .total_cmp(&(grid.surface(*b).unwrap_or(f32::INFINITY) - from.y).abs())
                    });
                if let Some(target) = candidate_target
                    && let Some(slot) = grid
                        .links
                        .get_mut(class)
                        .and_then(|links| links.get_mut(node))
                        .and_then(|links| links.get_mut(direction))
                {
                    *slot = u32::try_from(target).unwrap_or(NO_NAV_LINK);
                }
            }
        }
        // Conservative drops are the same size as steps; traversal is
        // bidirectional. Discard any ambiguous layer selection that failed
        // the reverse probe before deriving connected components.
        let mut one_way = Vec::new();
        for node in 0..nodes {
            for direction in 0..8 {
                if let Some(next) = grid.link(class, node, direction)
                    && !linked(&grid, class, next, node)
                {
                    one_way.push((node, direction));
                }
            }
        }
        for (node, direction) in one_way {
            if let Some(slot) = grid
                .links
                .get_mut(class)
                .and_then(|links| links.get_mut(node))
                .and_then(|links| links.get_mut(direction))
            {
                *slot = NO_NAV_LINK;
            }
        }
        // Reject diagonal corner cuts unless both orthogonal routes reach
        // the identical destination floor.
        let mut removed = Vec::new();
        for node in 0..nodes {
            let Some((cx, cz)) = grid.coordinates(node) else {
                continue;
            };
            for direction in 4..8 {
                let Some(next) = grid.link(class, node, direction) else {
                    continue;
                };
                let Some((nx, nz)) = grid.coordinates(next) else {
                    continue;
                };
                if ![(nx, cz), (cx, nz)].into_iter().all(|(mx, mz)| {
                    grid.column_range(mx, mz).any(|middle| {
                        linked(&grid, class, node, middle) && linked(&grid, class, middle, next)
                    })
                }) {
                    removed.push((node, direction));
                }
            }
        }
        for (node, direction) in removed {
            if let Some(slot) = grid
                .links
                .get_mut(class)
                .and_then(|links| links.get_mut(node))
                .and_then(|links| links.get_mut(direction))
            {
                *slot = NO_NAV_LINK;
            }
        }
    }
    let mut regions = Vec::with_capacity(input.classes.len());
    let mut walkable_cells = Vec::with_capacity(input.classes.len());
    for class in 0..input.classes.len() {
        let (labels, count, walkable) = label_regions(&grid, class)?;
        if let Some(slot) = grid.region.get_mut(class) {
            *slot = labels;
        }
        regions.push(count);
        walkable_cells.push(walkable);
    }
    crate::engine::package::navigation::validate_navigation(&grid)?;
    let report = NavBakeReport {
        cell_m,
        cells_x,
        cells_z,
        surface_cells: nodes,
        walkable_cells,
        regions,
        portals: grid.portals.len(),
        workers: workers.max(1),
    };
    Ok((grid, report))
}

fn linked(grid: &NavGrid, class: usize, from: usize, to: usize) -> bool {
    grid.links
        .get(class)
        .and_then(|links| links.get(from))
        .is_some_and(|links| links.contains(&u32::try_from(to).unwrap_or(NO_NAV_LINK)))
}

/// Immutable geometry shared by workers and continuous edge probes.
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
    fn surfaces(&self, x: f32, z: f32) -> Vec<f32> {
        let mut heights = Vec::new();
        self.floor.for_each_walk_surface(x, z, |height| {
            if height.is_finite() {
                heights.push(height);
            }
        });
        heights.extend(
            self.walk_proxies
                .iter()
                .filter(|proxy| proxy.contains(x, z) && proxy.y.is_finite())
                .map(|proxy| proxy.y),
        );
        heights.sort_unstable_by(f32::total_cmp);
        heights.dedup_by(|a, b| (*a - *b).abs() <= 1.0e-4);
        heights
    }

    #[expect(
        clippy::cast_precision_loss,
        clippy::as_conversions,
        reason = "grid indices are bounded by 2^21 columns and exactly representable in f32"
    )]
    fn bake_rows(
        &self,
        start: u32,
        end: u32,
        cell_m: f32,
        origin_x: f32,
        origin_z: f32,
        cells_x: u32,
        columns: usize,
        node_budget: &AtomicUsize,
    ) -> Result<Vec<Vec<BakeCell>>, String> {
        let index = CollisionIndex::build(self.walls);
        let mut output = Vec::new();
        for cz in start..end {
            for cx in 0..cells_x {
                let x = (cx as f32).mul_add(cell_m, origin_x) + cell_m * 0.5;
                let z = (cz as f32).mul_add(cell_m, origin_z) + cell_m * 0.5;
                let heights = self.surfaces(x, z);
                let total = node_budget
                    .fetch_add(heights.len(), Ordering::Relaxed)
                    .saturating_add(heights.len());
                validate_record_budget(columns, total, self.classes.len())?;
                let mut column = Vec::with_capacity(heights.len());
                for y in heights {
                    column.push(BakeCell {
                        surface: Some(y),
                        flags: CELL_SURFACE | self.slope_flag(x, z, cell_m, y),
                        headroom_cm: self.headroom_cm(x, z, y, 0.0, 0.0, &index),
                        portal: self.portal_at(x, z, y, cell_m),
                    });
                }
                output.push(column);
            }
        }
        Ok(output)
    }

    fn slope_flag(&self, x: f32, z: f32, _spacing: f32, y: f32) -> u8 {
        if self.floor.walk_surface_sloped(x, z, y) {
            CELL_SLOPED
        } else {
            0
        }
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::as_conversions,
        reason = "headroom centimetres are bounded and rounded conservatively down"
    )]
    fn headroom_cm(
        &self,
        x: f32,
        z: f32,
        y: f32,
        radius: f32,
        step_height: f32,
        index: &CollisionIndex,
    ) -> u16 {
        let ceiling = self.ceiling.ceiling_above_disc(x, z, radius.max(1.0e-4), y);
        let floor =
            self.floor
                .lowest_room_underside(Vec2::new(x, z), radius.max(1.0e-4), y + step_height);
        let directly_above = self.floor.lowest_room_underside(Vec2::new(x, z), 1.0e-4, y);
        let wall =
            crate::engine::collision::lowest_underside_indexed(index, x, z, radius, y, self.walls);
        let above = [ceiling, floor, directly_above, wall]
            .into_iter()
            .flatten()
            .min_by(f32::total_cmp);
        above.map_or(HEADROOM_UNBOUNDED_CM, |height| {
            (((height - y).max(0.0) * 100.0).min(f32::from(u16::MAX - 1))) as u16
        })
    }

    fn portal_at(&self, x: f32, z: f32, y: f32, cell_m: f32) -> u16 {
        let portals = self
            .doors
            .iter()
            .filter(|door| !door.def.id.trim().is_empty());
        for (index, door) in portals.enumerate() {
            if door.base_y() + door.def.height <= y + crate::engine::collision::STEP_EPS
                || door.base_y()
                    > y + self
                        .classes
                        .iter()
                        .map(|class| class.height)
                        .fold(0.0_f32, f32::max)
            {
                continue;
            }
            for step in 0..=8u8 {
                let direction = door
                    .def
                    .direction_at(door.def.signed_swing() * f32::from(step) / 8.0);
                let leaf = DoorCollider::from_pose(
                    [door.def.x, door.base_y(), door.def.z],
                    direction.into(),
                    door.def.width,
                    door.def.thickness,
                    door.def.height,
                );
                let radius = self
                    .classes
                    .iter()
                    .map(|class| class.radius)
                    .fold(0.0_f32, f32::max);
                if leaf.overlaps_disc(x, z, radius + cell_m * 0.75) {
                    return u16::try_from(index).unwrap_or(NO_PORTAL);
                }
            }
        }
        NO_PORTAL
    }

    fn body_clear(
        &self,
        x: f32,
        z: f32,
        y: f32,
        profile: &NavClass,
        index: &CollisionIndex,
    ) -> bool {
        if self.level.water.iter().any(|water| {
            water.swimming && water.contains(x, z) && water.surface_y > y + profile.step_height
        }) {
            return false;
        }
        let headroom = self.headroom_cm(x, z, y, profile.radius, profile.step_height, index);
        if headroom != HEADROOM_UNBOUNDED_CM
            && f32::from(headroom) + 1.0e-3 < profile.height * 100.0
        {
            return false;
        }
        !crate::engine::collision::disc_segment_blocked(
            Vec2::new(x, z),
            Vec2::new(x, z),
            profile.radius,
            y,
            profile.height,
            profile.step_height,
            index,
            self.walls,
        )
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::as_conversions,
        reason = "adjacent-cell probe counts are bounded to 65535 substeps before conversion"
    )]
    fn edge_clear(&self, from: Vec3, to: Vec3, profile: &NavClass, index: &CollisionIndex) -> bool {
        let distance = (to.x - from.x).hypot(to.z - from.z);
        let steps = (distance / crate::engine::nav::NAV_MOVE_SUBSTEP_M)
            .ceil()
            .clamp(1.0, 65535.0) as u32;
        let mut previous = from;
        let mut previous_flags = self.slope_flag(from.x, from.z, 0.05, from.y);
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let x = (to.x - from.x).mul_add(t, from.x);
            let z = (to.z - from.z).mul_add(t, from.z);
            let run = (x - previous.x).hypot(z - previous.z);
            let mut support: Option<(f32, u8)> = None;
            let mut consider = |y: f32| {
                let flags = self.slope_flag(x, z, 0.05, y);
                let sweep_feet = previous.y.min(y);
                let sweep_head = previous.y.max(y) + profile.height;
                let ceiling_clear =
                    [(previous.x, previous.z), (x, z)]
                        .into_iter()
                        .all(|(probe_x, probe_z)| {
                            self.ceiling
                                .ceiling_above_disc(probe_x, probe_z, profile.radius, sweep_feet)
                                .is_none_or(|ceiling| {
                                    ceiling + crate::engine::collision::CONTACT_EPS >= sweep_head
                                })
                        });
                if neighbour_rule(previous.y, y, run, previous_flags, flags, profile)
                    && ceiling_clear
                    && self.body_clear(x, z, y, profile, index)
                    && !crate::engine::collision::disc_segment_blocked(
                        Vec2::new(previous.x, previous.z),
                        Vec2::new(x, z),
                        profile.radius,
                        previous.y.min(y),
                        profile.height + (y - previous.y).abs(),
                        profile.step_height,
                        index,
                        self.walls,
                    )
                    && support.is_none_or(|(height, _)| {
                        (y - previous.y).abs() < (height - previous.y).abs()
                    })
                {
                    support = Some((y, flags));
                }
            };
            self.floor.for_each_walk_surface(x, z, &mut consider);
            for proxy in self.walk_proxies {
                if proxy.contains(x, z) {
                    consider(proxy.y);
                }
            }
            let Some((y, flags)) = support else {
                return false;
            };
            previous = Vec3::new(x, y, z);
            previous_flags = flags;
        }
        (previous.y - to.y).abs() <= 1.0e-3
    }
}

/// Connected components are labelled from the same stored links queries use.
fn label_regions(grid: &NavGrid, class: usize) -> Result<(Vec<u16>, u32, usize), String> {
    let mut labels = vec![NO_REGION; grid.cell_count()];
    let mut next_region = 0u32;
    let mut walkable = 0usize;
    let mut stack = Vec::new();
    for start in 0..grid.cell_count() {
        if !grid.is_walkable(class, start) || labels.get(start).copied() != Some(NO_REGION) {
            continue;
        }
        if next_region >= u32::from(NO_REGION) {
            return Err(format!(
                "navigation class {class} has too many connected regions"
            ));
        }
        let region = u16::try_from(next_region).unwrap_or(NO_REGION);
        next_region = next_region.saturating_add(1);
        stack.push(start);
        if let Some(slot) = labels.get_mut(start) {
            *slot = region;
        }
        while let Some(node) = stack.pop() {
            walkable = walkable.saturating_add(1);
            for direction in 0..8 {
                let Some(next) = grid.link(class, node, direction) else {
                    continue;
                };
                if labels.get(next).copied() == Some(NO_REGION) {
                    if let Some(slot) = labels.get_mut(next) {
                        *slot = region;
                    }
                    stack.push(next);
                }
            }
        }
    }
    Ok((labels, next_region, walkable))
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
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::as_conversions,
    reason = "The input is checked finite and nonnegative, then clamped to 1..=65535 before the native float-to-u32 conversion."
)]
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
