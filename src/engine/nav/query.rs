//! Runtime queries over a layered, baked navigation graph.
//!
//! [`NavMesh`] validates a compiled record once at load and answers three
//! questions for a caller-selected agent class:
//!
//! * `nearest` — the closest navigable point, restricted to the class's
//!   clearance, the same connected region as the query when the query is on
//!   the mesh, and a straight corridor that stays walkable;
//! * `path` — an A\* route honoured with cell topology plus door state,
//!   simplified into collision-safe waypoints by string pulling;
//! * `segment_clear` — the line-of-sight test both use.
//!
//! A closed door never removes topology: its cells are blocked for the query
//! and the route is replanned, while an openable door is crossed only when the
//! caller's profile says the agent may open it. Nothing here mutates the mesh.

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

use std::collections::BinaryHeap;
use std::fmt::Write as _;

use glam::{Vec2, Vec3};

use super::NavDoorState;
use crate::engine::collision::STEP_EPS;
use crate::engine::package::navigation::{CELL_SURFACE, NO_PORTAL, NO_REGION, NavClass, NavGrid};

/// Extra cost of crossing a door portal, in metres, so a route prefers a
/// genuinely shorter non-door corridor when one exists.
pub const PORTAL_COST_M: f32 = 0.25;

/// Extra cost of a closed door a capable agent must open, in metres.
pub const DOOR_OPEN_COST_M: f32 = 2.5;

/// Hard cap on A\* expansions one query may perform.
pub const MAX_PATH_EXPANSIONS: usize = 1 << 20;

/// One navigable point on the mesh.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavPoint {
    /// Walking-surface node index (one of possibly several floors per XZ cell).
    pub cell: usize,
    /// World position (cell surface height).
    pub position: Vec3,
}

/// Optional world-space volume a route must remain inside.
///
/// The caller insets X/Z by its body radius; Y bounds apply to feet surfaces.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NavBounds {
    /// Inclusive low corner.
    pub min: Vec3,
    /// Inclusive high corner.
    pub max: Vec3,
}

impl NavBounds {
    /// True when a finite point is inside every axis of the volume.
    #[must_use]
    pub fn contains(self, point: Vec3) -> bool {
        point.is_finite() && point.cmpge(self.min).all() && point.cmple(self.max).all()
    }
}

/// One query's contract: class, endpoints, door state and budget.
pub struct PathQuery<'a> {
    /// Baked class index the agent's body matches.
    pub class: usize,
    /// Start position (the agent's feet).
    pub start: Vec3,
    /// Goal position.
    pub goal: Vec3,
    /// True when the agent may open a closed, unlocked door on its route.
    pub can_open_doors: bool,
    /// Budget in expanded cells.
    pub max_expansions: usize,
    /// Live door state.
    pub doors: &'a dyn NavDoorState,
}

/// A route result.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    /// Waypoints from the start to the reached point, collision-safe and
    /// simplified. The first point is at or near the start; the last is the
    /// goal when `complete` is true.
    pub waypoints: Vec<Vec3>,
    /// Doors the agent must open on the way, in traversal order, deduplicated.
    pub door_requests: Vec<String>,
    /// True when the search reached the goal cell.
    pub complete: bool,
    /// Cells expanded by the search.
    pub expanded: usize,
}

impl Path {
    /// An empty route.
    #[must_use]
    pub fn new() -> Self {
        Self {
            waypoints: Vec::new(),
            door_requests: Vec::new(),
            complete: false,
            expanded: 0,
        }
    }

    /// Clears every buffer, keeping the allocations.
    pub fn clear(&mut self) {
        self.waypoints.clear();
        self.door_requests.clear();
        self.complete = false;
        self.expanded = 0;
    }

    /// The reached point, if any.
    #[must_use]
    pub fn reached(&self) -> Option<Vec3> {
        self.waypoints.last().copied()
    }
}

impl Default for Path {
    fn default() -> Self {
        Self::new()
    }
}

/// The outcome of one path query.
#[derive(Debug, Clone, PartialEq)]
pub enum PathResult {
    /// A route was produced; check [`Path::complete`] for a partial result.
    Path(Path),
    /// No route exists for this class and door state.
    Unreachable,
    /// The request was malformed (unknown class, or a start off the mesh).
    Invalid(String),
}

/// Reusable A\* storage. One per agent (or per test) keeps pathfinding free of
/// per-frame allocation after warmup.
#[derive(Debug, Default)]
pub struct NavScratch {
    stamp: Vec<u32>,
    cost: Vec<f32>,
    parent: Vec<u32>,
    epoch: u32,
    open: BinaryHeap<OpenNode>,
}

impl NavScratch {
    /// An empty scratch.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sizes the storage for a grid of `cells` cells.
    pub fn ensure(&mut self, cells: usize) {
        if self.stamp.len() < cells {
            self.stamp.resize(cells, 0);
            self.cost.resize(cells, f32::INFINITY);
            self.parent.resize(cells, u32::MAX);
        }
    }
}

/// A min-heap entry: lower `cost` first, lower cell index on ties.
#[derive(Debug, Clone, Copy, PartialEq)]
struct OpenNode {
    cost: f32,
    cell: u32,
}

impl Eq for OpenNode {}

impl PartialOrd for OpenNode {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OpenNode {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| other.cell.cmp(&self.cell))
    }
}

/// One region's statistics, for flee scoring and diagnostics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionStat {
    /// Cells in the region.
    pub cells: u32,
    /// Lowest and highest surface in the region.
    pub min_y: f32,
    /// Lowest and highest surface in the region.
    pub max_y: f32,
    /// Mean position of the region's cells.
    pub centroid_x: f32,
    /// Mean position of the region's cells.
    pub centroid_z: f32,
}

/// A validated navigation mesh: the baked grid plus per-class region
/// statistics.
#[derive(Debug, Clone, Default)]
pub struct NavMesh {
    grid: NavGrid,
    regions: Vec<Vec<RegionStat>>,
}

impl NavMesh {
    /// Validates a compiled record and computes its region statistics.
    ///
    /// # Errors
    ///
    /// Returns the record validation error when the grid is malformed.
    pub fn from_record(grid: NavGrid) -> Result<Self, String> {
        crate::engine::package::navigation::validate_navigation(&grid)?;
        let regions = (0..grid.classes.len())
            .map(|class| region_stats(&grid, class))
            .collect();
        Ok(Self { grid, regions })
    }

    /// The baked grid.
    #[must_use]
    pub const fn grid(&self) -> &NavGrid {
        &self.grid
    }

    /// The class index whose baked body matches `profile`, if any.
    #[must_use]
    pub fn class_index(&self, profile: &NavClass) -> Option<usize> {
        self.grid.class_index(profile)
    }

    /// One region's statistics.
    #[must_use]
    pub fn region_stat(&self, class: usize, region: u16) -> Option<&RegionStat> {
        self.regions
            .get(class)
            .and_then(|stats| stats.get(usize::from(region)))
    }

    /// The number of cells in a class region.
    #[must_use]
    pub fn region_cells(&self, class: usize, region: u16) -> u32 {
        self.region_stat(class, region).map_or(0, |stat| stat.cells)
    }

    /// The cell's surface, when it has one.
    #[must_use]
    pub fn cell_surface(&self, index: usize) -> Option<f32> {
        self.grid.surface(index)
    }

    /// True when this exact floor span is traversable in the current world.
    #[must_use]
    pub fn cell_passable(
        &self,
        class: usize,
        node: usize,
        doors: &dyn NavDoorState,
        can_open_doors: bool,
    ) -> bool {
        self.grid.classes.get(class).is_some_and(|profile| {
            self.cell_region(class, node, profile, doors, can_open_doors)
                .is_some()
        })
    }

    /// How many of a cell's eight neighbours are walkable for `class`.
    ///
    /// A cheap openness measure: an open cell has eight, a corridor four, a
    /// corner or a dead end three or fewer. Flee scoring uses it to prefer
    /// escape routes with options.
    #[must_use]
    pub fn open_neighbours(&self, class: usize, index: usize) -> u8 {
        let (Some(cx), Some(cz)) = (cell_column(&self.grid, index), cell_row(&self.grid, index))
        else {
            return 0;
        };
        let mut open = 0u8;
        for (direction, (dx, dz)) in NEIGHBOUR_STEPS.into_iter().enumerate() {
            let (Some(nx), Some(nz)) = (cx.checked_add_signed(dx), cz.checked_add_signed(dz))
            else {
                continue;
            };
            if self
                .grid
                .link(class, index, direction)
                .is_some_and(|next| self.grid.coordinates(next) == Some((nx, nz)))
            {
                open = open.saturating_add(1);
            }
        }
        open
    }

    /// The nearest navigable point to `position`.
    ///
    /// `vertical_tolerance` bounds how far the candidate's surface may be from
    /// the query's own height, so a point on another floor is never snapped to.
    /// When the query itself stands on the mesh the candidate must be in the
    /// same connected region. A query just off the mesh (a switch on a wall, a
    /// heard stimulus) anchors on the nearest navigable region instead: the
    /// candidate must belong to that region and the corridor from the first
    /// walkable sample must stay walkable, so a point behind a wall or a
    /// locked door is never returned for a superficially shorter distance. A
    /// query *inside* solid geometry has no side to prefer and may anchor on
    /// either side; author queries are live positions, not wall interiors.
    #[must_use]
    pub fn nearest(
        &self,
        class: usize,
        position: Vec3,
        max_distance: f32,
        vertical_tolerance: f32,
        doors: &dyn NavDoorState,
        can_open_doors: bool,
    ) -> Option<NavPoint> {
        let profile = *self.grid.classes.get(class)?;
        if !position.is_finite() || !max_distance.is_finite() || max_distance < 0.0 {
            return None;
        }
        let (cx, cz) = self.grid.cell_at(position.x, position.z)?;
        // The query is a *feet* position: a candidate may differ by the
        // caller's tolerance plus one walkable step, never by a whole body
        // height, so a goal can not snap one floor down.
        let mut max_vertical = vertical_tolerance + profile.step_height + STEP_EPS;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::as_conversions,
            reason = "The nonnegative radius cast saturates even if division overflows; scan_ring caps the result to the furthest grid cell."
        )]
        let max_ring = i32::try_from((max_distance / self.grid.cell_m).ceil() as i64)
            .unwrap_or(i32::MAX)
            .max(0_i32);
        let origin_cx = i32::try_from(cx).unwrap_or(i32::MAX);
        let origin_cz = i32::try_from(cz).unwrap_or(i32::MAX);
        let start_cell = self.grid.column_range(cx, cz).min_by(|a, b| {
            (self.grid.surface(*a).unwrap_or(f32::INFINITY) - position.y)
                .abs()
                .total_cmp(&(self.grid.surface(*b).unwrap_or(f32::INFINITY) - position.y).abs())
        });
        if let Some(anchor_y) = start_cell.and_then(|node| self.grid.surface(node)) {
            // Even an unusually tall step allowance must not project a
            // blocked goal onto another storey in the same XZ column.
            if let Some(gap) = self
                .grid
                .column_range(cx, cz)
                .filter(|node| Some(*node) != start_cell)
                .filter_map(|node| {
                    self.grid
                        .surface(node)
                        .map(|height| (height - anchor_y).abs())
                })
                .min_by(f32::total_cmp)
            {
                max_vertical = max_vertical.min(gap * 0.5);
            }
        }
        let own_region = start_cell
            .and_then(|cell| self.cell_region(class, cell, &profile, doors, can_open_doors))
            .filter(|_| {
                start_cell
                    .and_then(|cell| self.grid.surface(cell))
                    .is_some_and(|y| {
                        (y - position.y).abs()
                            <= vertical_tolerance + profile.step_height + STEP_EPS
                    })
            });
        // Pass 1: when the query is not on the mesh, learn which region the
        // nearest navigable point belongs to before committing to one.
        let scan_any = || {
            self.scan_ring(
                class,
                &profile,
                cx,
                cz,
                origin_cx,
                origin_cz,
                max_ring,
                position,
                max_vertical,
                max_distance,
                None,
                false,
                doors,
                can_open_doors,
            )
            .and_then(|point| self.grid.region_of(class, point.cell))
        };
        let anchor = own_region.or_else(scan_any)?;
        // Pass 2: the nearest point in the anchored region, with the corridor
        // rule when the anchor did not come from the query's own cell.
        self.scan_ring(
            class,
            &profile,
            cx,
            cz,
            origin_cx,
            origin_cz,
            max_ring,
            position,
            max_vertical,
            max_distance,
            Some(anchor),
            own_region.is_none(),
            doors,
            can_open_doors,
        )
    }

    /// One Chebyshev-ring search over the grid around `(origin_cx, origin_cz)`.
    ///
    /// Rings bound the search, but candidates are compared across rings: a
    /// diagonal cell in one ring can be farther than an axial cell in the
    /// next. Equal distances resolve to the lowest cell index.
    #[expect(clippy::too_many_arguments, reason = "one cohesive ring search")] // one cohesive ring search
    fn scan_ring(
        &self,
        class: usize,
        profile: &NavClass,
        cx: u32,
        cz: u32,
        origin_cx: i32,
        origin_cz: i32,
        max_ring: i32,
        position: Vec3,
        max_vertical: f32,
        max_distance: f32,
        region: Option<u16>,
        from_off_mesh: bool,
        doors: &dyn NavDoorState,
        can_open_doors: bool,
    ) -> Option<NavPoint> {
        let bounds = (
            origin_cx.checked_neg()?,
            i32::try_from(self.grid.cells_x.saturating_sub(cx.saturating_add(1)))
                .unwrap_or(i32::MAX),
            origin_cz.checked_neg()?,
            i32::try_from(self.grid.cells_z.saturating_sub(cz.saturating_add(1)))
                .unwrap_or(i32::MAX),
        );
        let grid_radius = bounds
            .0
            .abs()
            .max(bounds.1)
            .max(bounds.2.abs())
            .max(bounds.3);
        let mut best: Option<(f32, NavPoint)> = None;
        for ring in 0_i32..=max_ring.min(grid_radius) {
            for (dx, dz) in ring_offsets(ring, bounds) {
                let Some(nx) = origin_cx.checked_add(dx) else {
                    continue;
                };
                let Some(nz) = origin_cz.checked_add(dz) else {
                    continue;
                };
                let (Ok(column), Ok(row)) = (u32::try_from(nx), u32::try_from(nz)) else {
                    continue;
                };
                for index in self.grid.column_range(column, row) {
                    let Some(cell_region) =
                        self.cell_region(class, index, profile, doors, can_open_doors)
                    else {
                        continue;
                    };
                    if region.is_some_and(|wanted| wanted != cell_region) {
                        continue;
                    }
                    let Some(surface) = self.grid.surface(index) else {
                        continue;
                    };
                    if (surface - position.y).abs() > max_vertical {
                        continue;
                    }
                    let (cell_x, cell_z) = self.grid.cell_center(column, row);
                    let horizontal_distance = (cell_x - position.x).hypot(cell_z - position.z);
                    let distance = horizontal_distance.hypot(surface - position.y);
                    if horizontal_distance > max_distance {
                        continue;
                    }
                    let candidate = Vec3::new(cell_x, surface, cell_z);
                    if from_off_mesh
                        && !self.corridor_from_mesh(
                            class,
                            position,
                            candidate,
                            doors,
                            can_open_doors,
                        )
                    {
                        continue;
                    }
                    if best.is_none_or(|(best_distance, best_point)| {
                        distance < best_distance
                            || (distance == best_distance && index < best_point.cell)
                    }) {
                        best = Some((
                            distance,
                            NavPoint {
                                cell: index,
                                position: candidate,
                            },
                        ));
                    }
                }
            }
            // Every cell outside this ring is at least `ring * cell_m`
            // away on one axis, regardless of the query's cell offset.
            #[expect(
                clippy::cast_precision_loss,
                clippy::as_conversions,
                reason = "Loaded grids contain at most 2^21 cells, so every visited nonnegative ring index converts exactly to f32."
            )]
            let outside_distance = ring as f32 * self.grid.cell_m;
            if let Some((distance, point)) = best
                && (ring == 0_i32 || distance < outside_distance)
            {
                return Some(point);
            }
        }
        best.map(|(_, point)| point)
    }

    /// True when the straight corridor from an off-mesh query point to a
    /// navigable candidate stays walkable after the first walkable sample.
    ///
    /// Leading samples may be off the mesh (the query can be on a wall face);
    /// everything from the first walkable cell on must be walkable and
    /// step-continuous, so a corridor that crosses a wall, a locked door or a
    /// floor still fails.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::as_conversions,
        reason = "The float sample count is clamped to 1..=4096 before saturation/truncation; all visited sample indices and counts have exact f32 representations."
    )]
    fn corridor_from_mesh(
        &self,
        class: usize,
        from: Vec3,
        to: Vec3,
        doors: &dyn NavDoorState,
        can_open_doors: bool,
    ) -> bool {
        let Some(profile) = self.grid.classes.get(class).copied() else {
            return false;
        };
        let cell_m = self.grid.cell_m;
        let distance = (to.x - from.x).hypot(to.z - from.z);
        let steps = (distance / (cell_m * 0.5)).ceil().clamp(1.0, 4096.0) as u32;
        let mut entered = false;
        let mut previous: Option<usize> = None;
        for step in 1..=steps {
            let t = step as f32 / steps as f32;
            let x = (to.x - from.x).mul_add(t, from.x);
            let z = (to.z - from.z).mul_add(t, from.z);
            let Some((cx, cz)) = self.grid.cell_at(x, z) else {
                return false;
            };
            let expected_y = (to.y - from.y).mul_add(t, from.y);
            let Some(index) = self.layer_at(
                class,
                cx,
                cz,
                expected_y,
                previous,
                &profile,
                doors,
                can_open_doors,
            ) else {
                if !entered {
                    continue;
                }
                return false;
            };
            let walkable = self
                .cell_region(class, index, &profile, doors, can_open_doors)
                .is_some();
            if !entered {
                if !walkable {
                    continue;
                }
                entered = true;
            } else if !walkable {
                return false;
            }
            if let Some(previous_cell) = previous
                && !self.segment_transition_clear(
                    class,
                    previous_cell,
                    index,
                    &profile,
                    doors,
                    can_open_doors,
                    None,
                )
            {
                return false;
            }
            previous = Some(index);
        }
        true
    }

    /// Queries a route from `query.start` to `query.goal`.
    #[must_use]
    pub fn path(&self, query: &PathQuery<'_>, scratch: &mut NavScratch) -> PathResult {
        let mut path = Path::new();
        let result = self.path_into(query, scratch, &mut path);
        match result {
            Ok(()) => PathResult::Path(path),
            Err(other) => other,
        }
    }

    /// Fills `out` with a route, reusing its allocations.
    ///
    /// # Errors
    ///
    /// Returns [`PathResult::Invalid`] for a malformed request and
    /// [`PathResult::Unreachable`] when no route exists.
    pub fn path_into(
        &self,
        query: &PathQuery<'_>,
        scratch: &mut NavScratch,
        out: &mut Path,
    ) -> Result<(), PathResult> {
        self.path_into_bounded(query, scratch, out, None)
    }

    /// Fills a route while excluding cells and shortcuts outside `bounds`.
    ///
    /// # Errors
    ///
    /// Returns the same errors as [`Self::path_into`], including an
    /// unreachable result when either endpoint is outside the volume.
    pub fn path_into_bounded(
        &self,
        query: &PathQuery<'_>,
        scratch: &mut NavScratch,
        out: &mut Path,
        bounds: Option<NavBounds>,
    ) -> Result<(), PathResult> {
        out.clear();
        let Some(profile) = self.grid.classes.get(query.class).copied() else {
            return Err(PathResult::Invalid(format!(
                "navigation class {} is not baked",
                query.class
            )));
        };
        if !query.start.is_finite() || !query.goal.is_finite() {
            return Err(PathResult::Invalid(
                "navigation path endpoints are not finite".to_string(),
            ));
        }
        if bounds
            .is_some_and(|volume| !volume.contains(query.start) || !volume.contains(query.goal))
        {
            return Err(PathResult::Unreachable);
        }
        let cell_m = self.grid.cell_m;
        let endpoint_tolerance = STEP_EPS;
        let Some(start) = self.nearest(
            query.class,
            query.start,
            cell_m.max(0.5),
            endpoint_tolerance,
            query.doors,
            query.can_open_doors,
        ) else {
            return Err(PathResult::Invalid(
                "navigation path start is not on the baked mesh".to_string(),
            ));
        };
        let Some(goal) = self.nearest(
            query.class,
            query.goal,
            cell_m.max(1.5),
            endpoint_tolerance,
            query.doors,
            query.can_open_doors,
        ) else {
            return Err(PathResult::Unreachable);
        };
        if bounds.is_some_and(|volume| {
            !volume.contains(start.position) || !volume.contains(goal.position)
        }) {
            return Err(PathResult::Unreachable);
        }
        let cells = self.grid.cell_count();
        scratch.ensure(cells);
        scratch.epoch = scratch.epoch.wrapping_add(1);
        if scratch.epoch == 0 {
            scratch.stamp.fill(0);
            scratch.epoch = 1;
        }
        while scratch.open.pop().is_some() {}
        let epoch = scratch.epoch;
        let max_expansions = query.max_expansions.clamp(1, MAX_PATH_EXPANSIONS);
        set_open(
            &mut scratch.stamp,
            &mut scratch.cost,
            start.cell,
            epoch,
            0.0,
        );
        scratch.open.push(OpenNode {
            cost: 0.0,
            cell: u32::try_from(start.cell).unwrap_or(u32::MAX),
        });
        let mut expanded = 0usize;
        let mut best_cell = start.cell;
        let mut best_h = heuristic(&self.grid, start.cell, goal.position);
        let mut reached = false;
        while let Some(node) = scratch.open.pop() {
            let cell = usize::try_from(node.cell).unwrap_or(usize::MAX);
            if scratch.stamp.get(cell).copied() != Some(epoch) {
                continue;
            }
            let cost = scratch.cost.get(cell).copied().unwrap_or(f32::INFINITY);
            // The heap stores an f-score; a stale entry is one whose f is
            // worse than the current g-score plus the same heuristic.
            let current_f = cost + heuristic(&self.grid, cell, goal.position);
            if node.cost > current_f + 1.0e-4 {
                continue;
            }
            if cell == goal.cell {
                reached = true;
                best_cell = cell;
                break;
            }
            if expanded >= max_expansions {
                break;
            }
            expanded = expanded.saturating_add(1);
            let height = heuristic(&self.grid, cell, goal.position);
            if height < best_h {
                best_h = height;
                best_cell = cell;
            }
            for (direction, (dx, dz)) in NEIGHBOUR_STEPS.into_iter().enumerate() {
                let Some(next) = self.grid.link(query.class, cell, direction) else {
                    continue;
                };
                if !self.cell_in_bounds(next, bounds) {
                    continue;
                }
                if self
                    .cell_region(
                        query.class,
                        next,
                        &profile,
                        query.doors,
                        query.can_open_doors,
                    )
                    .is_none()
                {
                    continue;
                }
                let diagonal = dx != 0_i32 && dz != 0_i32;
                if !self.segment_transition_clear(
                    query.class,
                    cell,
                    next,
                    &profile,
                    query.doors,
                    query.can_open_doors,
                    bounds,
                ) {
                    continue;
                }
                let (Some(from_y), Some(to_y)) = (self.grid.surface(cell), self.grid.surface(next))
                else {
                    continue;
                };
                let distance = if diagonal {
                    cell_m * std::f32::consts::SQRT_2
                } else {
                    cell_m
                };
                let Some((passable, door_cost)) = self.portal_cost(query, next, &profile) else {
                    continue;
                };
                if !passable {
                    continue;
                }
                let step = distance + 0.5 * (to_y - from_y).abs() + door_cost;
                let candidate = cost + step;
                let known = if scratch.stamp.get(next).copied() == Some(epoch) {
                    scratch.cost.get(next).copied().unwrap_or(f32::INFINITY)
                } else {
                    f32::INFINITY
                };
                if candidate + 1.0e-6 < known {
                    set_open(
                        &mut scratch.stamp,
                        &mut scratch.cost,
                        next,
                        epoch,
                        candidate,
                    );
                    if let Some(slot) = scratch.parent.get_mut(next) {
                        *slot = u32::try_from(cell).unwrap_or(u32::MAX);
                    }
                    scratch.open.push(OpenNode {
                        cost: candidate + heuristic(&self.grid, next, goal.position),
                        cell: u32::try_from(next).unwrap_or(u32::MAX),
                    });
                }
            }
        }

        out.expanded = expanded;
        out.complete = reached;
        let target_cell = if reached { goal.cell } else { best_cell };
        if !reached && target_cell == start.cell {
            return Err(PathResult::Unreachable);
        }
        // Reconstruct the cell chain from the target back to the start.
        let mut chain: Vec<usize> = Vec::new();
        let mut cursor = target_cell;
        chain.push(cursor);
        while cursor != start.cell {
            let parent = scratch
                .stamp
                .get(cursor)
                .copied()
                .filter(|stamp| *stamp == epoch)
                .and_then(|_| scratch.parent.get(cursor).copied())
                .unwrap_or(u32::MAX);
            if parent == u32::MAX {
                break;
            }
            let parent_index = usize::try_from(parent).unwrap_or(usize::MAX);
            if parent_index == cursor || chain.len() > cells {
                break;
            }
            chain.push(parent_index);
            cursor = parent_index;
        }
        if chain.last().copied() != Some(start.cell) {
            chain.push(start.cell);
        }
        chain.reverse();

        // String pulling: keep the farthest cell centre reachable in a straight
        // walk from the current anchor.
        out.waypoints
            .push(clamp_to_surface(&self.grid, start.cell, query.start));
        let mut anchor = out.waypoints.first().copied().unwrap_or(query.start);
        let mut index = 1usize;
        // A query's search budget also bounds simplification work. After this
        // cap the certified cell chain is retained without further shortcuts.
        let mut shortcut_attempts = 0usize;
        while index < chain.len() {
            let mut next = index;
            for candidate in (index..chain.len()).rev() {
                if shortcut_attempts >= 128 {
                    break;
                }
                shortcut_attempts = shortcut_attempts.saturating_add(1);
                let Some(cell) = chain.get(candidate).copied() else {
                    continue;
                };
                let point = clamp_to_surface(&self.grid, cell, query.goal);
                if self.segment_clear_bounded(
                    query.class,
                    anchor,
                    point,
                    query.doors,
                    query.can_open_doors,
                    bounds,
                ) && self.segment_in_bounds(anchor, point, bounds)
                {
                    next = candidate;
                    break;
                }
            }
            let Some(point) = chain
                .get(next)
                .copied()
                .map(|cell| clamp_to_surface(&self.grid, cell, query.goal))
            else {
                break;
            };
            if point.distance_squared(anchor) > 1.0e-6 {
                out.waypoints.push(point);
                anchor = point;
            }
            index = next.saturating_add(1);
        }
        if reached {
            let goal_point = if self.segment_clear_bounded(
                query.class,
                anchor,
                query.goal,
                query.doors,
                query.can_open_doors,
                bounds,
            ) && self.segment_in_bounds(anchor, query.goal, bounds)
            {
                query.goal
            } else {
                clamp_to_surface(&self.grid, goal.cell, query.goal)
            };
            if goal_point.distance_squared(anchor) > 1.0e-6 {
                out.waypoints.push(goal_point);
            }
        }
        if out.waypoints.len() <= 1 && !reached {
            return Err(PathResult::Unreachable);
        }
        collect_door_requests(self, query, &chain, out);
        Ok(())
    }

    fn cell_in_bounds(&self, cell: usize, bounds: Option<NavBounds>) -> bool {
        bounds.is_none_or(|volume| volume.contains(clamp_to_surface(&self.grid, cell, Vec3::ZERO)))
    }

    /// Samples the actual surface height so string pulling cannot skip a
    /// forbidden ramp or ledge between two permitted endpoints.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::as_conversions,
        reason = "The finite segment sample count is clamped to 1..=4096 before conversion; every index is exactly representable in f32."
    )]
    fn segment_in_bounds(&self, from: Vec3, to: Vec3, bounds: Option<NavBounds>) -> bool {
        let Some(volume) = bounds else {
            return true;
        };
        if !volume.contains(from) || !volume.contains(to) {
            return false;
        }
        let distance = (to.x - from.x).hypot(to.z - from.z);
        let steps = (distance / (self.grid.cell_m * 0.5))
            .ceil()
            .clamp(1.0, 4096.0) as u32;
        (0..=steps).all(|step| {
            let t = step as f32 / steps as f32;
            let x = (to.x - from.x).mul_add(t, from.x);
            let z = (to.z - from.z).mul_add(t, from.z);
            let y = (to.y - from.y).mul_add(t, from.y);
            self.grid.cell_at(x, z).is_some_and(|(cx, cz)| {
                self.grid
                    .column_range(cx, cz)
                    .filter_map(|node| {
                        self.grid
                            .surface(node)
                            .map(|height| (node, (height - y).abs()))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .is_some_and(|(node, _)| self.cell_in_bounds(node, bounds))
            })
        })
    }

    /// True when a straight walked segment stays on walkable cells for the
    /// class, without a rise a step or a slope cannot explain.
    #[must_use]
    pub fn segment_clear(
        &self,
        class: usize,
        from: Vec3,
        to: Vec3,
        doors: &dyn NavDoorState,
        can_open_doors: bool,
    ) -> bool {
        self.segment_clear_bounded(class, from, to, doors, can_open_doors, None)
    }

    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_precision_loss,
        clippy::cast_sign_loss,
        clippy::as_conversions,
        reason = "The float sample count is clamped to 1..=4096 before saturation/truncation; all visited sample indices and counts have exact f32 representations."
    )]
    fn segment_clear_bounded(
        &self,
        class: usize,
        from: Vec3,
        to: Vec3,
        doors: &dyn NavDoorState,
        can_open_doors: bool,
        bounds: Option<NavBounds>,
    ) -> bool {
        let Some(profile) = self.grid.classes.get(class).copied() else {
            return false;
        };
        if !from.is_finite() || !to.is_finite() {
            return false;
        }
        let cell_m = self.grid.cell_m;
        let Some((end_x, end_z)) = self.grid.cell_at(to.x, to.z) else {
            return false;
        };
        // Endpoint identity is selected before live passability. A blocked
        // upper floor cannot fall through to a free lower span.
        let nearest_span = |cx, cz, height: f32| {
            self.grid.column_range(cx, cz).min_by(|a, b| {
                (self.grid.surface(*a).unwrap_or(f32::INFINITY) - height)
                    .abs()
                    .total_cmp(&(self.grid.surface(*b).unwrap_or(f32::INFINITY) - height).abs())
            })
        };
        let Some(end_node) = nearest_span(end_x, end_z, to.y) else {
            return false;
        };
        let Some((start_x, start_z)) = self.grid.cell_at(from.x, from.z) else {
            return false;
        };
        let Some(start_node) = nearest_span(start_x, start_z, from.y) else {
            return false;
        };
        if self
            .cell_region(class, start_node, &profile, doors, can_open_doors)
            .is_none()
            || self
                .cell_region(class, end_node, &profile, doors, can_open_doors)
                .is_none()
        {
            return false;
        }
        let distance = (to.x - from.x).hypot(to.z - from.z);
        let step_count = (distance / (cell_m * 0.5)).ceil().clamp(1.0, 4096.0) as u32;
        let mut previous: Option<usize> = None;
        let mut previous_position = from;
        for step in 0..=step_count {
            let t = step as f32 / step_count as f32;
            let x = (to.x - from.x).mul_add(t, from.x);
            let z = (to.z - from.z).mul_add(t, from.z);
            let Some((cx, cz)) = self.grid.cell_at(x, z) else {
                return false;
            };
            let expected_y = (to.y - from.y).mul_add(t, from.y);
            let Some(index) = self.layer_at(
                class,
                cx,
                cz,
                expected_y,
                previous,
                &profile,
                doors,
                can_open_doors,
            ) else {
                return false;
            };
            if (step == 0 && index != start_node) || (step == step_count && index != end_node) {
                return false;
            }
            let Some(surface) = self.grid.surface(index) else {
                return false;
            };
            let position = Vec3::new(x, surface, z);
            if doors.obstacle_segment_blocked(
                previous_position,
                position,
                profile.radius,
                profile.height,
            ) {
                return false;
            }
            previous_position = position;
            if (step == 0 || step == step_count)
                && self.grid.surface(index).is_none_or(|height| {
                    (height - expected_y).abs()
                        > profile.step_height + self.grid.cell_m * profile.max_slope + STEP_EPS
                })
            {
                return false;
            }
            if self
                .cell_region(class, index, &profile, doors, can_open_doors)
                .is_none()
            {
                return false;
            }
            if let Some(previous_cell) = previous
                && !self.segment_transition_clear(
                    class,
                    previous_cell,
                    index,
                    &profile,
                    doors,
                    can_open_doors,
                    bounds,
                )
            {
                return false;
            }
            previous = Some(index);
        }
        !doors.obstacle_segment_blocked(previous_position, to, profile.radius, profile.height)
    }

    /// Selects the correct floor in a column, retaining actual baked links.
    fn layer_at(
        &self,
        class: usize,
        cx: u32,
        cz: u32,
        y: f32,
        previous: Option<usize>,
        profile: &NavClass,
        doors: &dyn NavDoorState,
        can_open_doors: bool,
    ) -> Option<usize> {
        self.grid
            .column_range(cx, cz)
            .filter(|node| {
                self.cell_region(class, *node, profile, doors, can_open_doors)
                    .is_some()
            })
            .filter(|node| {
                previous.is_none_or(|from| {
                    from == *node
                        || self.segment_transition_clear(
                            class,
                            from,
                            *node,
                            profile,
                            doors,
                            can_open_doors,
                            None,
                        )
                })
            })
            .min_by(|a, b| {
                (self.grid.surface(*a).unwrap_or(f32::INFINITY) - y)
                    .abs()
                    .total_cmp(&(self.grid.surface(*b).unwrap_or(f32::INFINITY) - y).abs())
            })
    }

    /// The class region label of an in-grid cell, when it is traversable.
    fn cell_region(
        &self,
        class: usize,
        index: usize,
        profile: &NavClass,
        doors: &dyn NavDoorState,
        can_open_doors: bool,
    ) -> Option<u16> {
        if !self.grid.is_walkable(class, index) {
            return None;
        }
        let (Some(cx), Some(cz)) = (cell_column(&self.grid, index), cell_row(&self.grid, index))
        else {
            return None;
        };
        let (x, z) = self.grid.cell_center(cx, cz);
        let surface = self.grid.surface(index)?;
        if doors.obstacle_blocked(Vec3::new(x, surface, z), profile.radius, profile.height) {
            return None;
        }
        let (passable, _) = self.portal_state(index, x, z, profile, doors, can_open_doors);
        if !passable {
            return None;
        }
        self.grid.region_of(class, index)
    }

    /// The portal state of a cell: `(passable, extra cost)`.
    ///
    /// A locked door, or a closed door the agent cannot open, is impassable.
    fn portal_state(
        &self,
        index: usize,
        cell_x: f32,
        cell_z: f32,
        profile: &NavClass,
        doors: &dyn NavDoorState,
        can_open_doors: bool,
    ) -> (bool, f32) {
        let portal = self
            .grid
            .cell_portal
            .get(index)
            .copied()
            .unwrap_or(NO_PORTAL);
        if portal == NO_PORTAL {
            return (true, 0.0);
        }
        let Some(entry) = self.grid.portals.get(usize::from(portal)) else {
            return (true, 0.0);
        };
        if let Some(leaf) = doors.door_leaf(&entry.door)
            && self.grid.surface(index).is_some_and(|feet| {
                leaf.hinge_y + leaf.height <= feet + STEP_EPS
                    || leaf.hinge_y >= feet + profile.height
            })
        {
            return (true, 0.0);
        }
        if doors.door_locked(&entry.door).unwrap_or(false) || doors.door_obstructed(&entry.door) {
            return (false, 0.0);
        }
        if doors.door_open(&entry.door) {
            // An open leaf may still lie across a cell; the mover resolves it,
            // and the planner refuses to route through it.
            if let Some(leaf) = doors.door_leaf(&entry.door)
                && self.grid.surface(index).is_some_and(|feet| {
                    leaf.hinge_y + leaf.height > feet + STEP_EPS
                        && leaf.hinge_y < feet + profile.height
                })
                && leaf.overlaps_disc(cell_x, cell_z, profile.radius)
            {
                return (false, 0.0);
            }
            return (true, PORTAL_COST_M);
        }
        if can_open_doors {
            return (true, PORTAL_COST_M + DOOR_OPEN_COST_M);
        }
        (false, 0.0)
    }

    /// The portal cost when moving into `next`.
    fn portal_cost(
        &self,
        query: &PathQuery<'_>,
        next: usize,
        profile: &NavClass,
    ) -> Option<(bool, f32)> {
        let cx = cell_column(&self.grid, next)?;
        let cz = cell_row(&self.grid, next)?;
        let (x, z) = self.grid.cell_center(cx, cz);
        Some(self.portal_state(next, x, z, profile, query.doors, query.can_open_doors))
    }

    /// Checks the same step and diagonal topology used by A* for a shortcut.
    fn segment_transition_clear(
        &self,
        class: usize,
        from: usize,
        to: usize,
        profile: &NavClass,
        doors: &dyn NavDoorState,
        can_open_doors: bool,
        bounds: Option<NavBounds>,
    ) -> bool {
        if from == to {
            return self.cell_in_bounds(to, bounds);
        }
        let (Some((from_x, from_z)), Some((to_x, to_z))) =
            (self.grid.coordinates(from), self.grid.coordinates(to))
        else {
            return false;
        };
        let Some(direction) = NEIGHBOUR_STEPS.iter().position(|(dx, dz)| {
            from_x.checked_add_signed(*dx) == Some(to_x)
                && from_z.checked_add_signed(*dz) == Some(to_z)
        }) else {
            return false;
        };
        if self.grid.link(class, from, direction) != Some(to) || !self.cell_in_bounds(to, bounds) {
            return false;
        }
        let from_position = clamp_to_surface(&self.grid, from, Vec3::ZERO);
        let to_position = clamp_to_surface(&self.grid, to, Vec3::ZERO);
        if doors.obstacle_segment_blocked(
            from_position,
            to_position,
            profile.radius,
            profile.height,
        ) {
            return false;
        }
        for node in [from, to] {
            let portal = self
                .grid
                .cell_portal
                .get(node)
                .copied()
                .unwrap_or(NO_PORTAL);
            let Some(entry) = self.grid.portals.get(usize::from(portal)) else {
                continue;
            };
            let crosses =
                door_crosses_segment(&entry.door, from_position, to_position, profile, doors);
            if crosses == Some(false) {
                continue;
            }
            if doors.door_open(&entry.door) {
                if crosses == Some(true) {
                    return false;
                }
            } else if !can_open_doors
                || doors.door_locked(&entry.door).unwrap_or(false)
                || doors.door_obstructed(&entry.door)
            {
                return false;
            }
        }
        if from_x == to_x || from_z == to_z {
            return true;
        }
        // Both orthogonal routes must reach this very span, never a different
        // floor in the same XZ column.
        [(to_x, from_z), (from_x, to_z)]
            .into_iter()
            .all(|(cx, cz)| {
                self.grid.column_range(cx, cz).any(|middle| {
                    self.cell_in_bounds(middle, bounds)
                        && self
                            .cell_region(class, middle, profile, doors, can_open_doors)
                            .is_some()
                        && self
                            .grid
                            .links
                            .get(class)
                            .and_then(|links| links.get(from))
                            .is_some_and(|links| {
                                links.contains(&u32::try_from(middle).unwrap_or(u32::MAX))
                            })
                        && self
                            .grid
                            .links
                            .get(class)
                            .and_then(|links| links.get(middle))
                            .is_some_and(|links| {
                                links.contains(&u32::try_from(to).unwrap_or(u32::MAX))
                            })
                })
            })
    }

    /// An ASCII rendering of one class's walkable space for developer
    /// inspection. It is drawn from the same cells the queries use.
    #[must_use]
    pub fn debug_ascii(&self, class: usize, doors: &dyn NavDoorState) -> String {
        let Some(profile) = self.grid.classes.get(class) else {
            return "no such navigation class".to_string();
        };
        let mut out = String::new();
        let _formatted_nav_class = writeln!(
            out,
            "nav class {class} r={:.2} h={:.2} grid {}x{} @ {:.2} m origin ({:.1},{:.1})",
            profile.radius,
            profile.height,
            self.grid.cells_x,
            self.grid.cells_z,
            self.grid.cell_m,
            self.grid.origin_x,
            self.grid.origin_z
        );
        let _layer_legend = writeln!(
            out,
            "M = multiple floors; {} total surface nodes",
            self.grid.cell_count()
        );
        for cz in 0..self.grid.cells_z {
            for cx in 0..self.grid.cells_x {
                let Some(index) = self.grid.index_of(cx, cz) else {
                    out.push('#');
                    continue;
                };
                let ch = if self.grid.column_range(cx, cz).len() > 1 {
                    'M'
                } else if !self.grid.has_surface(index) {
                    '#'
                } else if !self.grid.is_walkable(class, index) {
                    'x'
                } else {
                    match self.cell_region(class, index, profile, doors, true) {
                        Some(_) => {
                            if self
                                .grid
                                .cell_portal
                                .get(index)
                                .is_some_and(|portal| *portal != NO_PORTAL)
                            {
                                'p'
                            } else {
                                '.'
                            }
                        }
                        None => 'd',
                    }
                };
                out.push(ch);
            }
            out.push('\n');
        }
        out
    }
}

/// The A\* heuristic: straight-line distance to the goal position.
fn heuristic(grid: &NavGrid, cell: usize, goal: Vec3) -> f32 {
    let (Some(cx), Some(cz)) = (cell_column(grid, cell), cell_row(grid, cell)) else {
        return f32::INFINITY;
    };
    let (x, z) = grid.cell_center(cx, cz);
    let y = grid.surface(cell).unwrap_or(0.0);
    ((x - goal.x).powi(2) + (y - goal.y).powi(2) + (z - goal.z).powi(2)).sqrt()
}

/// The horizontal column of a walking-surface node.
fn cell_column(grid: &NavGrid, index: usize) -> Option<u32> {
    grid.coordinates(index).map(|(cx, _)| cx)
}

/// The horizontal row of a surface node.
fn cell_row(grid: &NavGrid, index: usize) -> Option<u32> {
    grid.coordinates(index).map(|(_, cz)| cz)
}

/// Writes the open set and cost arrays for one cell.
fn set_open(stamp: &mut [u32], cost: &mut [f32], cell: usize, epoch: u32, value: f32) {
    if let Some(slot) = stamp.get_mut(cell) {
        *slot = epoch;
    }
    if let Some(slot) = cost.get_mut(cell) {
        *slot = value;
    }
}

/// The point at a cell's centre, with the query's Y when it is on the cell.
fn clamp_to_surface(grid: &NavGrid, cell: usize, hint: Vec3) -> Vec3 {
    let (Some(cx), Some(cz)) = (cell_column(grid, cell), cell_row(grid, cell)) else {
        return hint;
    };
    let (x, z) = grid.cell_center(cx, cz);
    let y = grid.surface(cell).unwrap_or(hint.y);
    Vec3::new(x, y, z)
}

/// Exact horizontal leaf sweep over the union of the old/new body bands.
/// A missing collider preserves the policy-only providers used by diagnostics.
fn door_crosses_segment(
    door_id: &str,
    from: Vec3,
    to: Vec3,
    profile: &NavClass,
    doors: &dyn NavDoorState,
) -> Option<bool> {
    let leaf = doors.door_leaf(door_id)?;
    Some(crate::engine::collision::door_segment_blocked(
        Vec2::new(from.x, from.z),
        Vec2::new(to.x, to.z),
        profile.radius,
        from.y.min(to.y),
        profile.height + (to.y - from.y).abs(),
        std::slice::from_ref(&leaf),
    ))
}

/// Records every closed leaf actually crossed by a certified chain edge.
fn collect_door_requests(mesh: &NavMesh, query: &PathQuery<'_>, chain: &[usize], out: &mut Path) {
    if !query.can_open_doors {
        return;
    }
    let Some(profile) = mesh.grid.classes.get(query.class) else {
        return;
    };
    for (offset, &cell) in chain.iter().enumerate() {
        let previous = chain.get(offset.saturating_sub(1)).copied().unwrap_or(cell);
        let from_position = clamp_to_surface(&mesh.grid, previous, query.start);
        let to_position = clamp_to_surface(&mesh.grid, cell, query.goal);
        for node in [previous, cell] {
            let portal = mesh
                .grid
                .cell_portal
                .get(node)
                .copied()
                .unwrap_or(NO_PORTAL);
            let Some(entry) = mesh.grid.portals.get(usize::from(portal)) else {
                continue;
            };
            if query.doors.door_open(&entry.door)
                || door_crosses_segment(
                    &entry.door,
                    from_position,
                    to_position,
                    profile,
                    query.doors,
                ) == Some(false)
                || out.door_requests.iter().any(|id| id == &entry.door)
            {
                continue;
            }
            out.door_requests.push(entry.door.clone());
        }
    }
}

/// The eight neighbour offsets, in the same order as the bake.
const NEIGHBOUR_STEPS: [(i32, i32); 8] = [
    (-1, 0),
    (1, 0),
    (0, -1),
    (0, 1),
    (-1, -1),
    (1, -1),
    (-1, 1),
    (1, 1),
];

/// In-grid offsets on one Chebyshev ring, retaining the query tie order.
fn ring_offsets(
    ring: i32,
    (min_x, max_x, min_z, max_z): (i32, i32, i32, i32),
) -> impl Iterator<Item = (i32, i32)> {
    let radius = ring.max(0_i32);
    let negative = radius.saturating_neg();
    let horizontal = (negative.max(min_x)..=radius.min(max_x)).flat_map(move |dx| {
        [(dx, negative), (dx, radius)]
            .into_iter()
            .take(if radius == 0 { 1 } else { 2 })
            .filter(move |(_, dz)| (min_z..=max_z).contains(dz))
    });
    let vertical = (negative.saturating_add(1).max(min_z)..=radius.saturating_sub(1).min(max_z))
        .flat_map(move |dz| {
            [(negative, dz), (radius, dz)]
                .into_iter()
                .filter(move |(dx, _)| (min_x..=max_x).contains(dx))
        });
    horizontal.chain(vertical)
}

/// Counts cells per region for one class.
#[expect(
    clippy::cast_precision_loss,
    clippy::as_conversions,
    reason = "NavMesh validates the 2^21-cell package cap before computing statistics, so every region population converts exactly to f32."
)]
fn region_stats(grid: &NavGrid, class: usize) -> Vec<RegionStat> {
    let mut stats: Vec<RegionStat> = Vec::new();
    for index in 0..grid.cell_count() {
        let Some(region) = grid.region_of(class, index) else {
            continue;
        };
        let Some(cx) = cell_column(grid, index) else {
            continue;
        };
        let Some(cz) = cell_row(grid, index) else {
            continue;
        };
        let (x, z) = grid.cell_center(cx, cz);
        let y = grid.surface(index).unwrap_or(0.0);
        let slot = usize::from(region);
        while stats.len() <= slot {
            stats.push(RegionStat {
                cells: 0,
                min_y: f32::INFINITY,
                max_y: f32::NEG_INFINITY,
                centroid_x: 0.0,
                centroid_z: 0.0,
            });
        }
        if let Some(stat) = stats.get_mut(slot) {
            stat.cells = stat.cells.saturating_add(1);
            stat.min_y = stat.min_y.min(y);
            stat.max_y = stat.max_y.max(y);
            let count = stat.cells as f32;
            stat.centroid_x += (x - stat.centroid_x) / count.max(1.0);
            stat.centroid_z += (z - stat.centroid_z) / count.max(1.0);
        }
    }
    stats
}

/// True when the region label run mentions a surface bit for a cell.
#[must_use]
pub fn cell_is_surface(flags: u8) -> bool {
    flags & CELL_SURFACE != 0
}

/// The sentinel for "no region" is re-exported for tests and diagnostics.
pub const NO_REGION_CELL: u16 = NO_REGION;
