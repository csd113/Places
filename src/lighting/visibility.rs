//! Static light visibility: does opaque wall geometry stand between a light and
//! a surface sample?
//!
//! The baked lighting model sums a room baseline and a local pool per fixture.
//! A pool is a distance falloff, so without this module a fixture could light
//! any surface inside [`super::LOCAL_LIGHT_RADIUS_M`] even with an opaque wall in
//! between — the cross-wall bleed and RGB contamination that the wall-boundary
//! repair exists to remove. This module answers one question, once per level
//! load and never per frame:
//!
//! ```text
//! can the straight segment from a fixture's panel to a surface sample pass
//! through an opaque wall?
//! ```
//!
//! The geometry it tests is exactly the solid geometry the renderer emits and
//! collision walks through. Every wall is split by
//! [`crate::level::wall_solid_slices_profiled`] into the same solid columns the
//! wall mesh and collision use, and each patch of solid wall becomes one
//! world-space axis-aligned box. A door, window, passage or vent removes the box
//! it cuts, so:
//!
//! - a segment that crosses a solid box is blocked;
//! - a segment that passes through an opening's own footprint and height is not
//!   blocked, which is what keeps doorways transmitting light;
//! - the solid header above a door still blocks a segment that would have to
//!   cross it, so an opening is never upgraded to "the whole wall is
//!   transparent";
//! - a low wall or a raised wall blocks only up to its real top, so light can
//!   still pass over a sill or under a beam.
//!
//! Boxes are collected per *query site*: one range of box indices per fixture,
//! holding only the boxes whose horizontal bounds reach the fixture's pool
//! radius. A segment between two points that are both inside the pool radius
//! cannot leave that disc, so the prefilter is exact rather than approximate.
//! That keeps one visibility query proportional to the few walls near the
//! fixture instead of to the whole level.
//!
//! Everything here is deterministic: boxes are built in wall order, columns in
//! ascending length order and spans in ascending height order, and a query
//! walks its range in index order.

use crate::level::{LevelDef, LevelSurfaces, WallAxis, wall_solid_slices_profiled};

/// How much every opaque box is shrunk on all six sides before it is tested.
///
/// A light mounted flush with a wall face and a surface sample sitting exactly
/// on a wall face both lie on a box boundary; without this margin the slab clip
/// counts them as inside the wall and a sconce would light nothing. The margin
/// is far thinner than any authored wall.
const SURFACE_EPS_M: f32 = 5.0e-3;

/// Cell size of the uniform grid that answers "is this point inside a wall?".
///
/// Only surface samples need it: a room's floor and ceiling grids sample the
/// room's own boundary, which a wall straddling that boundary encloses. The
/// grid keeps the answer to a couple of boxes instead of the whole level.
const POINT_GRID_CELL_M: f32 = 4.0;

/// Tolerance used when grouping a wall's solid slices into length columns, in
/// metres. Slice boundaries come from the same computation, so this only has to
/// absorb float noise.
const WALL_COLUMN_EPS_M: f32 = 1e-4;

/// One opaque axis-aligned box in world space, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Blocker {
    min: [f32; 3],
    max: [f32; 3],
}

impl Blocker {
    /// True when the two X/Z footprint squares touch or overlap.
    fn overlaps_footprint(&self, x0: f32, x1: f32, z0: f32, z1: f32) -> bool {
        self.min[0] <= x1 && self.max[0] >= x0 && self.min[2] <= z1 && self.max[2] >= z0
    }

    /// Horizontal distance from `(x, z)` to this box's footprint, in metres
    /// (zero when the point is over the footprint).
    fn footprint_distance(&self, x: f32, z: f32) -> f32 {
        let dx = (self.min[0] - x).max(x - self.max[0]).max(0.0);
        let dz = (self.min[2] - z).max(z - self.max[2]).max(0.0);
        dx.hypot(dz)
    }

    /// True when the point lies inside the box in plan view.
    ///
    /// The box has already been shrunk by [`SURFACE_EPS_M`], so a surfel that
    /// merely touches the wall's plane does not count as buried in it.
    fn contains_xz(&self, x: f32, z: f32) -> bool {
        x >= self.min[0] && x <= self.max[0] && z >= self.min[2] && z <= self.max[2]
    }
}

/// One place a visibility question is asked from: a fixture panel or a doorway
/// blend, with the horizontal radius its contributions can reach.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct QuerySite {
    pub x: f32,
    pub z: f32,
    pub radius: f32,
}

impl QuerySite {
    #[must_use]
    pub const fn new(x: f32, z: f32, radius: f32) -> Self {
        Self { x, z, radius }
    }
}

/// Extra horizontal reach allowed for a query's start point beyond its site
/// centre, in metres.
///
/// A fixture's segment starts at the closest point of its panel rather than at
/// the panel's centre, so the segment can reach this much further from the site
/// than the sample does. One metre covers the largest fixture footprint the
/// fixture table defines.
const SITE_REACH_MARGIN_M: f32 = 1.0;

/// One opaque box a site can reach, with the horizontal distance from the site
/// centre to the box's footprint.
#[derive(Clone, Copy, Debug, PartialEq)]
struct SiteBlocker {
    blocker: u32,
    /// Distance from the site centre to the box's X/Z rectangle, in metres.
    near: f32,
}

/// The level's opaque wall geometry, prepared for segment queries.
#[derive(Clone, Debug, Default)]
pub struct Visibility {
    blockers: Vec<Blocker>,
    /// Boxes reachable from each query site, concatenated and ordered by
    /// distance from the site.
    pool: Vec<SiteBlocker>,
    /// `(start, end)` into `pool`, one entry per query site in site order.
    ranges: Vec<(u32, u32)>,
    /// Site centre of each range, for the reach cut-off.
    sites: Vec<(f32, f32)>,
    /// Uniform X/Z grid over the boxes, for [`Self::contains_point`].
    point_grid: PointGrid,
}

/// A uniform grid over the level's X/Z extent mapping a cell to the boxes whose
/// footprint overlaps it.
#[derive(Clone, Debug, Default)]
struct PointGrid {
    min_x: f32,
    min_z: f32,
    cells_x: u32,
    cells_z: u32,
    /// `(start, end)` into `items`, one entry per cell in row-major order.
    ranges: Vec<(u32, u32)>,
    items: Vec<u32>,
}

impl PointGrid {
    fn build(blockers: &[Blocker]) -> Self {
        if blockers.is_empty() {
            return Self::default();
        }
        let mut min_x = f32::INFINITY;
        let mut min_z = f32::INFINITY;
        let mut max_x = f32::NEG_INFINITY;
        let mut max_z = f32::NEG_INFINITY;
        for blocker in blockers {
            min_x = min_x.min(blocker.min[0]);
            min_z = min_z.min(blocker.min[2]);
            max_x = max_x.max(blocker.max[0]);
            max_z = max_z.max(blocker.max[2]);
        }
        if !min_x.is_finite() || !min_z.is_finite() || !max_x.is_finite() || !max_z.is_finite() {
            return Self::default();
        }
        // Both spans are finite and non-negative here, so each ceiling is a
        // finite integral value; the cast saturates rather than wraps on an
        // absurd span and the clamp bounds each axis to `1..=1024` cells.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (cells_x, cells_z) = (
            ((max_x - min_x) / POINT_GRID_CELL_M).ceil() as u32,
            ((max_z - min_z) / POINT_GRID_CELL_M).ceil() as u32,
        );
        let cells_x = cells_x.saturating_add(1).clamp(1, 1024);
        let cells_z = cells_z.saturating_add(1).clamp(1, 1024);

        let cell_of = |x: f32, z: f32| -> Option<(u32, u32)> {
            let ix = ((x - min_x) / POINT_GRID_CELL_M).floor();
            let iz = ((z - min_z) / POINT_GRID_CELL_M).floor();
            if ix < 0.0 || iz < 0.0 {
                return None;
            }
            // `floor` leaves non-negative integral values; the saturating cast
            // and the bounds check reject everything outside the grid.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let (ix, iz) = (ix as u32, iz as u32);
            if ix >= cells_x || iz >= cells_z {
                return None;
            }
            Some((ix, iz))
        };

        let mut items: Vec<u32> = Vec::new();
        let mut ranges: Vec<(u32, u32)> =
            Vec::with_capacity(cells_x.saturating_mul(cells_z) as usize);
        for iz in 0..cells_z {
            for ix in 0..cells_x {
                let start = u32::try_from(items.len()).unwrap_or(u32::MAX);
                for (index, blocker) in blockers.iter().enumerate() {
                    // A box is listed in every cell its footprint touches.
                    let low = cell_of(blocker.min[0], blocker.min[2]);
                    let high = cell_of(blocker.max[0], blocker.max[2]);
                    let (Some((low_x, low_z)), Some((high_x, high_z))) = (low, high) else {
                        continue;
                    };
                    if (low_x..=high_x).contains(&ix) && (low_z..=high_z).contains(&iz) {
                        items.push(u32::try_from(index).unwrap_or(u32::MAX));
                    }
                }
                let end = u32::try_from(items.len()).unwrap_or(u32::MAX);
                ranges.push((start, end));
            }
        }

        Self {
            min_x,
            min_z,
            cells_x,
            cells_z,
            ranges,
            items,
        }
    }

    fn contains(&self, blockers: &[Blocker], x: f32, z: f32) -> bool {
        if self.cells_x == 0 || self.cells_z == 0 || !x.is_finite() || !z.is_finite() {
            return false;
        }
        let ix = ((x - self.min_x) / POINT_GRID_CELL_M).floor();
        let iz = ((z - self.min_z) / POINT_GRID_CELL_M).floor();
        if ix < 0.0 || iz < 0.0 {
            return false;
        }
        // `floor` leaves non-negative integral values; the saturating cast and
        // the bounds check reject everything outside the grid.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (ix, iz) = (ix as u32, iz as u32);
        if ix >= self.cells_x || iz >= self.cells_z {
            return false;
        }
        let index = iz.saturating_mul(self.cells_x).saturating_add(ix) as usize;
        let Some(&(start, end)) = self.ranges.get(index) else {
            return false;
        };
        self.items
            .get(start as usize..end as usize)
            .is_some_and(|items| {
                items.iter().any(|item| {
                    blockers
                        .get(*item as usize)
                        .is_some_and(|blocker| blocker.contains_xz(x, z))
                })
            })
    }
}

impl Visibility {
    /// Builds the box set from a level's walls and one query site per light or
    /// opening that needs a visibility answer.
    #[must_use]
    pub fn build(level: &LevelDef, sites: &[QuerySite]) -> Self {
        let surfaces = LevelSurfaces::new(level);
        let mut blockers: Vec<Blocker> = Vec::new();
        for wall in &level.walls {
            append_wall_blockers(&mut blockers, wall, &surfaces);
        }

        let mut pool: Vec<SiteBlocker> =
            Vec::with_capacity(blockers.len().saturating_mul(sites.len().min(4)));
        let mut ranges: Vec<(u32, u32)> = Vec::with_capacity(sites.len());
        let mut site_centres: Vec<(f32, f32)> = Vec::with_capacity(sites.len());
        for site in sites {
            let start = u32::try_from(pool.len()).unwrap_or(u32::MAX);
            if site.x.is_finite() && site.z.is_finite() && site.radius.is_finite() {
                let radius = site.radius.max(0.0);
                let (x0, x1) = (site.x - radius, site.x + radius);
                let (z0, z1) = (site.z - radius, site.z + radius);
                for (index, blocker) in blockers.iter().enumerate() {
                    if blocker.overlaps_footprint(x0, x1, z0, z1) {
                        pool.push(SiteBlocker {
                            blocker: u32::try_from(index).unwrap_or(u32::MAX),
                            near: blocker.footprint_distance(site.x, site.z),
                        });
                    }
                }
                // Nearest first, so a query can stop as soon as the next box is
                // further away than its own reach. Sorting by a partial order is
                // safe: every distance is finite and non-negative.
                if let Some(added) = pool.get_mut(start as usize..) {
                    added.sort_by(|a, b| {
                        a.near
                            .partial_cmp(&b.near)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then(a.blocker.cmp(&b.blocker))
                    });
                }
            }
            let end = u32::try_from(pool.len()).unwrap_or(u32::MAX);
            ranges.push((start, end));
            site_centres.push((site.x, site.z));
        }

        let point_grid = PointGrid::build(&blockers);
        Self {
            blockers,
            pool,
            ranges,
            sites: site_centres,
            point_grid,
        }
    }

    /// Number of opaque boxes the level contributes. Reported by the lighting
    /// summary so a level's blocker count is visible in the developer log.
    #[must_use]
    pub const fn blocker_count(&self) -> usize {
        self.blockers.len()
    }

    /// Number of query sites the set was built for.
    #[must_use]
    pub const fn site_count(&self) -> usize {
        self.ranges.len()
    }

    /// True when `(x, z)` lies inside a solid wall, ignoring height.
    ///
    /// A room's floor and ceiling are sampled on the room's own footprint, and
    /// a wall authored across that boundary encloses the outermost sample row.
    /// The bake asks this so it can move such a sample out of the solid before
    /// it measures light, instead of leaving a dark strip along the wall base.
    #[must_use]
    pub fn contains_point(&self, x: f32, z: f32) -> bool {
        self.point_grid.contains(&self.blockers, x, z)
    }

    /// True when opaque wall geometry crosses the segment from `from` to `to`.
    ///
    /// `site` selects the prefiltered range of the fixture or opening the query
    /// belongs to; a site that was never registered (or one with no reachable
    /// wall) blocks nothing.
    #[must_use]
    pub fn occludes(&self, site: u32, from: [f32; 3], to: [f32; 3]) -> bool {
        if !from.iter().chain(to.iter()).all(|value| value.is_finite()) {
            // A non-finite query is refused rather than answered: refusing
            // would drop a legitimate contribution, and answering "blocked"
            // keeps a malformed sample dark instead of accepting an unknown
            // path.
            return true;
        }
        let Some(&(start, end)) = self.ranges.get(site as usize) else {
            return false;
        };
        let Some(&(site_x, site_z)) = self.sites.get(site as usize) else {
            return false;
        };
        // The segment can only reach as far from the site centre as the sample
        // does, plus the offset from the centre to the panel point the segment
        // starts at. Boxes past that are skipped without a geometric test:
        // every point of the segment lies within `max(distance(from),
        // distance(to))` of the site centre, and the start point is within
        // `SITE_REACH_MARGIN_M` of it by construction, so a box whose nearest
        // footprint point is beyond `reach` cannot be crossed.
        let reach = (to[0] - site_x).hypot(to[2] - site_z) + SITE_REACH_MARGIN_M;
        let Some(entries) = self.pool.get(start as usize..end as usize) else {
            return false;
        };
        for entry in entries {
            if entry.near > reach {
                break;
            }
            if let Some(blocker) = self.blockers.get(entry.blocker as usize)
                && segment_hits_box(*blocker, from, to)
            {
                return true;
            }
        }
        false
    }

    /// [`Self::occludes`] over every registered box, for a query that does not
    /// belong to one fixture (used by tests and diagnostics).
    #[must_use]
    pub fn occludes_anywhere(&self, from: [f32; 3], to: [f32; 3]) -> bool {
        if !from.iter().chain(to.iter()).all(|value| value.is_finite()) {
            return true;
        }
        self.blockers
            .iter()
            .any(|blocker| segment_hits_box(*blocker, from, to))
    }
}

/// One length column of a wall: a contiguous span along the wall's length axis
/// and every vertical span of solid wall that survives there. A window leaves
/// two spans in one column (the wall below its sill and above its header).
#[derive(Debug)]
struct BlockerColumn {
    start: f32,
    end: f32,
    spans: Vec<(f32, f32)>,
}

/// Appends one opaque box per solid patch of one wall.
fn append_wall_blockers(
    blockers: &mut Vec<Blocker>,
    wall: &crate::level::WallDef,
    surfaces: &LevelSurfaces<'_>,
) {
    let length = wall.length();
    if !length.is_finite() || length <= 0.0 {
        return;
    }
    let (x0, x1) = (
        wall.x.min(wall.x + wall.width),
        wall.x.max(wall.x + wall.width),
    );
    let (z0, z1) = (
        wall.z.min(wall.z + wall.depth),
        wall.z.max(wall.z + wall.depth),
    );
    let axis = wall.axis();
    let (origin_x, origin_z) = wall.length_origin();
    let breaks = surfaces.wall_profile_breaks(wall);
    let slices = wall_solid_slices_profiled(
        wall,
        |offset| surfaces.clear_ceiling_height_along(wall, offset),
        &breaks,
    );

    // Group the slices into length columns: one column per contiguous length
    // range, carrying every solid vertical span that survives there (the wall
    // below a window and the wall above it are two spans of one column).
    let mut columns: Vec<BlockerColumn> = Vec::new();
    for slice in &slices {
        match columns.last_mut() {
            Some(column)
                if (column.start - slice.start).abs() <= WALL_COLUMN_EPS_M
                    && (column.end - slice.end).abs() <= WALL_COLUMN_EPS_M =>
            {
                column.spans.push((slice.bottom, slice.top));
            }
            _ => columns.push(BlockerColumn {
                start: slice.start,
                end: slice.end,
                spans: vec![(slice.bottom, slice.top)],
            }),
        }
    }

    for column in columns {
        let (start, end) = (column.start, column.end);
        let (length_min, length_max) = match axis {
            WallAxis::X => (origin_x + start, origin_x + end),
            WallAxis::Z => (origin_z + start, origin_z + end),
        };
        let (across_min, across_max) = match axis {
            WallAxis::X => (z0, z1),
            WallAxis::Z => (x0, x1),
        };
        for (bottom, top) in column.spans {
            if !bottom.is_finite() || !top.is_finite() || top <= bottom {
                continue;
            }
            // Every box is shrunk by `SURFACE_EPS_M` on all six sides, so a
            // light or a surface sample sitting exactly on a wall plane is
            // outside the solid rather than on its boundary.
            let blocker = match axis {
                WallAxis::X => Blocker {
                    min: [
                        length_min + SURFACE_EPS_M,
                        bottom + SURFACE_EPS_M,
                        across_min + SURFACE_EPS_M,
                    ],
                    max: [
                        length_max - SURFACE_EPS_M,
                        top - SURFACE_EPS_M,
                        across_max - SURFACE_EPS_M,
                    ],
                },
                WallAxis::Z => Blocker {
                    min: [
                        across_min + SURFACE_EPS_M,
                        bottom + SURFACE_EPS_M,
                        length_min + SURFACE_EPS_M,
                    ],
                    max: [
                        across_max - SURFACE_EPS_M,
                        top - SURFACE_EPS_M,
                        length_max - SURFACE_EPS_M,
                    ],
                },
            };
            if blocker.min[0] < blocker.max[0]
                && blocker.min[1] < blocker.max[1]
                && blocker.min[2] < blocker.max[2]
            {
                blockers.push(blocker);
            }
        }
    }
}

/// True when the segment `from`-`to` intersects the box `blocker`.
///
/// The standard slab clip against the segment's own `[0, 1]` parameter range.
/// Boxes are pre-shrunk by [`SURFACE_EPS_M`], so neither a light nor a surface
/// mounted flush with a wall is blocked by the wall it sits on.
fn segment_hits_box(blocker: Blocker, from: [f32; 3], to: [f32; 3]) -> bool {
    let mut enter = 0.0_f32;
    let mut exit = 1.0_f32;
    for ((&start, &end), (&low, &high)) in from
        .iter()
        .zip(to.iter())
        .zip(blocker.min.iter().zip(blocker.max.iter()))
    {
        let delta = end - start;
        if delta.abs() <= f32::EPSILON {
            if start < low || start > high {
                return false;
            }
            continue;
        }
        let inverse = 1.0 / delta;
        let mut near = (low - start) * inverse;
        let mut far = (high - start) * inverse;
        if near > far {
            std::mem::swap(&mut near, &mut far);
        }
        enter = enter.max(near);
        exit = exit.min(far);
        if enter > exit {
            return false;
        }
    }
    true
}

#[cfg(test)]
// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests.
#[allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::missing_const_for_fn,
    clippy::panic
)]
mod tests {
    use super::*;
    use crate::level::LevelDef;

    fn level(json: &str) -> LevelDef {
        LevelDef::from_json(json).unwrap_or_else(|error| panic!("test level must parse: {error}"))
    }

    /// One 4 x 4 m room whose only wall is a full-height partition at x = 2.
    fn split_room() -> LevelDef {
        level(
            r#"{
                "format_version": 1,
                "id": "visibility",
                "name": "Visibility",
                "spawn": { "x": 1.0, "z": 1.0 },
                "rooms": [
                    { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 }
                ],
                "walls": [
                    { "x": 1.9, "z": 0.0, "width": 0.2, "depth": 4.0, "height": 3.0 }
                ]
            }"#,
        )
    }

    #[test]
    fn segment_is_blocked_by_a_solid_wall() {
        let level = split_room();
        let visibility = Visibility::build(&level, &[QuerySite::new(0.5, 2.0, 6.0)]);
        assert_eq!(visibility.blocker_count(), 1);
        assert!(visibility.occludes(0, [0.5, 1.5, 2.0], [3.5, 1.5, 2.0]));
        // Over the wall top the same segment is clear.
        assert!(!visibility.occludes(0, [0.5, 3.4, 2.0], [3.5, 3.4, 2.0]));
        // Around the wall end it is clear.
        assert!(!visibility.occludes(0, [0.5, 1.5, -0.6], [3.5, 1.5, -0.6]));
    }

    #[test]
    fn doorway_span_passes_but_the_header_blocks() {
        let mut level = split_room();
        level.walls[0].openings.push(crate::level::WallOpeningDef {
            kind: "door".into(),
            offset: 1.0,
            width: 1.0,
            height: 2.1,
            sill: 0.0,
        });
        let visibility = Visibility::build(&level, &[QuerySite::new(0.5, 2.0, 6.0)]);
        // Through the doorway.
        assert!(!visibility.occludes(0, [0.5, 1.0, 1.5], [3.5, 1.0, 1.5]));
        // Through the solid wall beside the doorway.
        assert!(visibility.occludes(0, [0.5, 1.0, 3.5], [3.5, 1.0, 3.5]));
        // Through the header above the doorway.
        assert!(visibility.occludes(0, [0.5, 2.5, 1.5], [3.5, 2.5, 1.5]));
    }

    #[test]
    fn a_site_that_never_registered_blocks_nothing() {
        let level = split_room();
        let visibility = Visibility::build(&level, &[QuerySite::new(0.5, 2.0, 0.5)]);
        assert!(!visibility.occludes(0, [0.5, 1.5, 2.0], [3.5, 1.5, 2.0]));
        assert!(!visibility.occludes(u32::MAX, [0.5, 1.5, 2.0], [3.5, 1.5, 2.0]));
    }
}
