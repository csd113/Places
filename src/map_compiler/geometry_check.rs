//! The map geometry checker: a read-only, deterministic validator for level
//! geometry.
//!
//! Run it from the game binary:
//!
//! ```text
//! places --check-geometry --level levels/home_showcase.json
//! places --check-geometry --level tests/fixtures/levels/level0_pit.json --json report.json
//! places --check-geometry --level home_showcase --markers-obj markers.obj
//! places --repair-geometry --level places_demo --plan plan.json
//! ```
//!
//! The repair planner is read-only: it shares the exact [`wall_joints`]
//! classification the checker reports and emits a machine-readable edit plan
//! for `tools/levels/repair_alignment.py` to apply.
//!
//! It builds the level exactly the way the game does — the loader validation,
//! the level preparation pass (fixture/decal snapping and automatic trim), the
//! static geometry emitter and the collision derivation — and then reports
//! what is measurably wrong in that interpretation. Nothing here re-implements
//! an emitter: the checks read the produced [`LevelMesh`], the engine's own
//! collision boxes and the level data.
//!
//! # Confirmed defects and heuristic warnings
//!
//! Every finding is either an **error** (a confirmed, measurable defect such as
//! a degenerate triangle, a duplicated coplanar surface, an overlapping
//! opening pair or an uncovered curved primitive) or a **warning** (a
//! heuristic such as a face pair with opposing normals, a room leaking into
//! the void, a perimeter run with no wall or opening, a collider with no
//! nearby mesh, a duplicated prop placement or a large prop layer sitting a
//! few millimetres off the walkable floor under it). Warnings
//! can be deliberate and are then suppressed by a narrow
//! [`crate::engine::level::GeometryIntentDef`] annotation in `geometry_intent`; errors
//! are never suppressed.
//!
//! # Honest limitations
//!
//! The checker proves nothing about levels it does not read, and it is not a
//! watertightness proof: open-plan edges, doorways, stair openings, pools,
//! carpet holes and every other intentionally open space are legitimate and
//! are only flagged when they read as a leak into the *void*. It sees the
//! asset-less mesh (prop placeholders, no GLB geometry) and does not validate
//! prop models, textures or the rendered image. It reports findings, not
//! fixes; the repair planner only *plans* them and never writes a source.
//!
//! Exit statuses: `0` no confirmed defects, `1` confirmed defects, `2` usage or
//! file/parse failure.

use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fmt::Write as _;
use std::path::PathBuf;

use serde::Serialize;

use crate::engine::collision::{CROUCH_HEIGHT, DoorCollider, PLAYER_STEP_HEIGHT, WallAabb};
use crate::engine::level::{
    ArcWallDef, ArchitectureBox, LevelDef, LevelSurfaces, PillarDef, RoomDef, WalkableFloor,
    WallAxis, WallDef, wall_solid_slices_profiled,
};
use crate::engine::materials::MaterialTable;
use crate::engine::render::{LevelMesh, SurfaceKey, SurfaceKind, Vertex};

#[cfg(test)]
#[path = "tests/geometry_surfaces.rs"]
mod surface_tests;

#[cfg(test)]
#[path = "tests/geometry_props.rs"]
mod prop_tests;

/// Stable format id written into the machine-readable report.
pub const CHECK_FORMAT: &str = "places-geometry-check";
/// Report schema version; bumped when a field changes meaning.
pub const CHECK_VERSION: u32 = 1;

/// Inclusive plane verification margin retained by the architecture checker.
const OVERLAP_PLANE_EPS_M: f32 = 1.5e-3;
/// Dominant-axis projection can amplify coordinate separation by sqrt(3).
const OVERLAP_BOUNDS_EPS_M: f32 = OVERLAP_PLANE_EPS_M * 1.732_050_8;
/// Minimum real overlap area before two coplanar surfaces are reported at all.
const OVERLAP_AREA_EPS_M2: f32 = 1.0e-4;
/// Coplanar overlaps up to this area are reported as `coplanar-sliver`
/// warnings rather than confirmed errors: a square centimetre of coincident
/// geometry at a joint is measurable but sub-pixel in normal play, while a
/// patch above this is worth a confirmed finding.
const SLIVER_AREA_EPS_M2: f32 = 1.0e-3;
/// Distance within which a mesh triangle counts as lying on a collider face.
const FACE_SAMPLE_EPS_M: f32 = 0.003;
/// The player body used for the room-leak flood fill: the crouched height, so
/// a route a crouched player could take still counts as connected.
const LEAK_BODY_HEIGHT_M: f32 = CROUCH_HEIGHT;
/// Body radius used for the room-leak flood fill, slightly under the player's,
/// so a cell gap narrower than the real body is still blocked.
const LEAK_BODY_RADIUS_M: f32 = 0.25;
/// Cell size of the leak flood fill, in metres.
const LEAK_CELL_M: f32 = 0.25;
/// How far outside its own footprint a room's flood fill may spread, in metres.
const LEAK_MARGIN_M: f32 = 2.0;
/// Spacing of the perimeter coverage samples, in metres.
const PERIMETER_STEP_M: f32 = 0.5;
/// Uncovered perimeter run length that is worth reporting, in metres.
const MISSING_RUN_M: f32 = 1.0;
/// Curve sagitta above which a tessellation is reported as coarse, in metres.
const CURVE_COARSE_SAGITTA_M: f32 = 0.02;
/// Caps on how many findings one check lists; the summary still counts every
/// finding, and one final entry names the truncation.
const MAX_FINDINGS_PER_CHECK: usize = 200;
/// Cell size of the triangle spatial hash, in metres.
const SPATIAL_CELL_M: f32 = 1.0;

/// Two thickness faces at or below this offset are the same plane, in metres.
///
/// It matches the emitter's own `WALL_COINCIDENCE_EPS = 1e-3`; at or under it
/// the emitter resolves the faces as one.
pub const JOINT_PLANE_TOL_M: f32 = 1.0e-3;
/// Maximum length gap between two slices that still counts as a touching joint
/// for the automatic class, in metres.
pub const JOINT_ADJACENCY_M: f32 = 0.05;
/// Maximum length gap for a review-only joint candidate, in metres — a
/// doorway drawn as two walls.
pub const JOINT_NEAR_ADJACENCY_M: f32 = 0.35;
/// Largest automatic correction of one wall plane, in metres.
///
/// Half of the thickest wall (0.4 m) plus margin; typical architectural
/// reveals are at most 0.05 m. Above it a joint is review only.
pub const JOINT_MAX_AUTO_SHIFT_M: f32 = 0.25;
/// Minimum vertical overlap for a joint, in metres.
pub const JOINT_MIN_Y_M: f32 = 0.30;
/// Minimum solid slice length to consider, in metres.
pub const JOINT_MIN_LENGTH_M: f32 = 0.05;
/// Parallel-face angular tolerance for emitted triangles, in degrees.
pub const JOINT_ANGULAR_TOL_DEG: f32 = 0.5;

/// Quantises a repair value to `1.0e-4 m`, so repaired sources stay decimal.
#[must_use]
pub fn quantise(value: f32) -> f32 {
    (value * 1.0e4).round() / 1.0e4
}

/// One candidate (or confirmed) wall-plane joint between two authored walls.
///
/// Both the checker and the repair planner read this classification, so a
/// finding and its planned repair can never disagree about what is broken.
#[derive(Clone, Debug, Serialize)]
pub struct WallJoint {
    /// Authored wall index of the side that keeps its plane (the authority) on
    /// `step`/`near-step` joints; the lower index on classes without a mover.
    pub first: usize,
    /// Authored wall index of the side that follows `shift` (the mover) on
    /// `step`/`near-step` joints.
    pub second: usize,
    /// `"x"` or `"z"`: the axis the walls run along.
    pub axis: &'static str,
    /// `"step"`, `"thickness-step"` or `"near-step"`.
    pub kind: &'static str,
    /// First wall's two thickness planes (low, high).
    pub first_low: f32,
    pub first_high: f32,
    /// Second wall's two thickness planes (low, high).
    pub second_low: f32,
    pub second_high: f32,
    /// Signed delta to add to the *second* wall on the across axis.
    pub shift: f32,
    /// Positive length separation between the two slices (0 when they touch or
    /// overlap).
    pub gap_m: f32,
    /// Positive length overlap between the two slices (0 when they are apart).
    pub overlap_m: f32,
    /// Vertical overlap between the two solid slices, in metres.
    pub y_overlap_m: f32,
    /// `"chain"`, `"length"` or `"index"`: why this side is the authority.
    pub authority: &'static str,
    /// Count of other coplanar wall slices touching the first slice.
    pub first_support: usize,
    /// Count of other coplanar wall slices touching the second slice.
    pub second_support: usize,
    /// True only for an unambiguous `step` within the automatic shift limit.
    pub auto_repairable: bool,
    /// World anchor at the joint, in metres.
    pub position: [f32; 3],
}

/// One wall's solid decomposition in world coordinates along its length axis.
#[derive(Clone, Debug)]
struct WallSlices {
    index: usize,
    axis: WallAxis,
    /// Across-thickness planes `(low, high)`.
    planes: (f32, f32),
    /// World coordinate of local slice offset 0 (the footprint's min corner).
    origin: f32,
    slices: Vec<crate::engine::level::WallSlice>,
}

impl WallSlices {
    /// Total solid length of every slice of this wall, in metres.
    fn solid_span(&self) -> f32 {
        self.slices
            .iter()
            .map(|slice| slice.end - slice.start)
            .sum()
    }

    /// World length interval of one slice.
    fn span_of(&self, slice: &crate::engine::level::WallSlice) -> (f32, f32) {
        (self.origin + slice.start, self.origin + slice.end)
    }
}

/// One classified joint plus the slice spans the emitted verification samples.
#[derive(Clone, Debug)]
struct WallJointCandidate {
    joint: WallJoint,
    /// World length interval of the joint's first slice.
    first_span: (f32, f32),
    /// World length interval of the joint's second slice.
    second_span: (f32, f32),
    /// Absolute Y interval shared by the two slices.
    y: (f32, f32),
}

/// Replays the engine's own wall decomposition for every authored wall, the
/// same way `authored_colliders` does (profile breaks and the clear-ceiling
/// closure), so lintels and headers participate in the classification.
fn level_wall_slices(level: &LevelDef, surfaces: &LevelSurfaces<'_>) -> Vec<WallSlices> {
    let mut out = Vec::new();
    for (index, wall) in level.walls.iter().enumerate() {
        let breaks = surfaces.wall_profile_breaks(wall);
        let clear = |offset: f32| surfaces.clear_ceiling_height_along(wall, offset);
        let slices = wall_solid_slices_profiled(wall, clear, &breaks);
        let (min_x, max_x) = (
            wall.x.min(wall.x + wall.width),
            wall.x.max(wall.x + wall.width),
        );
        let (min_z, max_z) = (
            wall.z.min(wall.z + wall.depth),
            wall.z.max(wall.z + wall.depth),
        );
        let (planes, origin) = match wall.axis() {
            WallAxis::X => ((min_z, max_z), min_x),
            WallAxis::Z => ((min_x, max_x), min_z),
        };
        out.push(WallSlices {
            index,
            axis: wall.axis(),
            planes,
            origin,
            slices,
        });
    }
    out
}

/// Positive separation and overlap of two length intervals, in metres.
fn interval_relationship(a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    let gap = (a.0.max(b.0) - a.1.min(b.1)).max(0.0);
    let overlap = (a.1.min(b.1) - a.0.max(b.0)).max(0.0);
    (gap, overlap)
}

/// Counts other walls' solid slices that are exactly coplanar with `wall` and
/// touch `span`, the evidence the authority rule compares.
fn slice_support(
    walls: &[WallSlices],
    wall: &WallSlices,
    span: (f32, f32),
    y: (f32, f32),
) -> usize {
    let mut count = 0usize;
    for other in walls {
        if other.index == wall.index || other.axis != wall.axis {
            continue;
        }
        if (other.planes.0 - wall.planes.0).abs() > JOINT_PLANE_TOL_M
            || (other.planes.1 - wall.planes.1).abs() > JOINT_PLANE_TOL_M
        {
            continue;
        }
        for slice in &other.slices {
            if slice.end - slice.start < JOINT_MIN_LENGTH_M {
                continue;
            }
            let other_span = other.span_of(slice);
            let (gap, overlap) = interval_relationship(span, other_span);
            if gap > JOINT_ADJACENCY_M {
                continue;
            }
            let half = 0.5 * (span.1 - span.0).min(other_span.1 - other_span.0);
            if overlap > half + 1.0e-4 {
                continue;
            }
            let y_overlap = y.1.min(slice.top) - y.0.max(slice.bottom);
            if y_overlap < JOINT_MIN_Y_M {
                continue;
            }
            count = count.saturating_add(1);
        }
    }
    count
}

/// True when a solid slice of either joint wall fills the length gap between
/// the two slices over their shared height.
///
/// A gap whose span is already solid at the same height is an internal slice
/// boundary (a lintel meeting the wall body behind it), not a doorway drawn as
/// two walls, so it must not be reported as a review candidate.
fn gap_is_covered(
    first: &WallSlices,
    second: &WallSlices,
    first_span: (f32, f32),
    second_span: (f32, f32),
    y: (f32, f32),
) -> bool {
    let (low, high) = if first_span.1 <= second_span.0 {
        (first_span.1, second_span.0)
    } else if second_span.1 <= first_span.0 {
        (second_span.1, first_span.0)
    } else {
        return true;
    };
    for wall in [first, second] {
        for slice in &wall.slices {
            let span = wall.span_of(slice);
            if span.0 > low + JOINT_PLANE_TOL_M || span.1 < high - JOINT_PLANE_TOL_M {
                continue;
            }
            let y_overlap = y.1.min(slice.top) - y.0.max(slice.bottom);
            if y_overlap > JOINT_PLANE_TOL_M {
                return true;
            }
        }
    }
    false
}

/// The kind one pair of wall planes implies, or `None` when the pair is not a
/// candidate of this class at all.
fn plane_kind(d_low: f32, d_high: f32) -> Option<&'static str> {
    let max_shift = d_low.abs().max(d_high.abs());
    if max_shift > JOINT_NEAR_ADJACENCY_M {
        return None;
    }
    let rigid = (d_low - d_high).abs() <= JOINT_PLANE_TOL_M;
    if rigid {
        if max_shift <= JOINT_PLANE_TOL_M {
            Some("coplanar")
        } else if max_shift <= JOINT_MAX_AUTO_SHIFT_M {
            Some("step")
        } else {
            Some("near-step")
        }
    } else if d_low.abs().min(d_high.abs()) <= JOINT_PLANE_TOL_M {
        // One face aligned, the other offset: a thickness transition or a
        // misplaced sliver. Both look identical to a rigid rule, so it is a
        // review-only class.
        Some("thickness-step")
    } else {
        None
    }
}

/// True when the two walls' plane pair counts as coplanar.
fn is_coplanar(kind: &str) -> bool {
    kind == "coplanar"
}

/// Builds a joint candidate's world anchor from the two slice spans.
const fn joint_position(axis: WallAxis, along: f32, across: f32, y: f32) -> [f32; 3] {
    match axis {
        WallAxis::X => [along, y, across],
        WallAxis::Z => [across, y, along],
    }
}

/// One length-adjacent pair of solid slices between two walls.
#[derive(Clone, Copy, Debug)]
struct SlicePair {
    first: crate::engine::level::WallSlice,
    second: crate::engine::level::WallSlice,
    first_span: (f32, f32),
    second_span: (f32, f32),
    gap: f32,
    overlap: f32,
    y_overlap: f32,
}

/// Collects every slice pair of two walls that passes the joint filters.
///
/// `coplanar` additionally admits length-separated pairs (a doorway drawn as
/// two walls); a shifted pair must actually touch.
fn slice_pairs(first: &WallSlices, second: &WallSlices, coplanar: bool) -> Vec<SlicePair> {
    let mut out = Vec::new();
    for slice_first in &first.slices {
        if slice_first.end - slice_first.start < JOINT_MIN_LENGTH_M {
            continue;
        }
        let first_span = first.span_of(slice_first);
        for slice_second in &second.slices {
            if slice_second.end - slice_second.start < JOINT_MIN_LENGTH_M {
                continue;
            }
            let second_span = second.span_of(slice_second);
            let (gap, overlap) = interval_relationship(first_span, second_span);
            if gap > JOINT_NEAR_ADJACENCY_M {
                continue;
            }
            // End-to-end only: an overlay (a board sitting on a face) fails.
            let half = 0.5 * (first_span.1 - first_span.0).min(second_span.1 - second_span.0);
            if overlap > half + 1.0e-4 {
                continue;
            }
            if !coplanar && gap > JOINT_ADJACENCY_M {
                continue;
            }
            let y_overlap =
                slice_first.top.min(slice_second.top) - slice_first.bottom.max(slice_second.bottom);
            if y_overlap < JOINT_MIN_Y_M {
                continue;
            }
            out.push(SlicePair {
                first: *slice_first,
                second: *slice_second,
                first_span,
                second_span,
                gap,
                overlap,
                y_overlap,
            });
        }
    }
    out
}

/// The deterministic joint candidates of one level, with their slice spans.
///
/// This is the single classification the checker and the repair planner share:
/// [`wall_joints`] strips the internal slice detail, the checker keeps it for
/// the emitted-mesh verification and the planner for its coupled edits.
#[expect(clippy::too_many_lines, reason = "one joint pass over every wall pair")] // one joint pass over every wall pair
fn compute_wall_joints(level: &LevelDef, surfaces: &LevelSurfaces<'_>) -> Vec<WallJointCandidate> {
    let walls = level_wall_slices(level, surfaces);
    let mut out: Vec<WallJointCandidate> = Vec::new();
    for (first_index, first) in walls.iter().enumerate() {
        for second in walls.iter().skip(first_index.saturating_add(1)) {
            if first.axis != second.axis {
                continue;
            }
            let d_low = second.planes.0 - first.planes.0;
            let d_high = second.planes.1 - first.planes.1;
            let Some(kind) = plane_kind(d_low, d_high) else {
                continue;
            };
            let coplanar = is_coplanar(kind);
            let pairs = slice_pairs(first, second, coplanar);
            let touching = pairs.iter().any(|pair| pair.gap <= JOINT_ADJACENCY_M);
            if coplanar && touching {
                // The walls share a plane and touch somewhere: a correct shared
                // edge. A gapped lintel pair between the same walls is an
                // internal slice boundary, not a doorway.
                continue;
            }
            let mut eligible: Vec<&SlicePair> = pairs
                .iter()
                .filter(|pair| {
                    if coplanar {
                        pair.gap > JOINT_ADJACENCY_M
                            && !gap_is_covered(
                                first,
                                second,
                                pair.first_span,
                                pair.second_span,
                                (
                                    pair.first.bottom.max(pair.second.bottom),
                                    pair.first.top.min(pair.second.top),
                                ),
                            )
                    } else {
                        true
                    }
                })
                .collect();
            eligible.sort_by(|a, b| {
                a.y_overlap
                    .total_cmp(&b.y_overlap)
                    .then_with(|| a.first_span.0.total_cmp(&b.first_span.0))
                    .then_with(|| a.second_span.0.total_cmp(&b.second_span.0))
            });
            let Some(best) = eligible.last().copied().copied() else {
                continue;
            };
            let (span_first, span_second) = (best.first_span, best.second_span);
            let (y_low, y_high) = (
                best.first.bottom.max(best.second.bottom),
                best.first.top.min(best.second.top),
            );
            let (gap, overlap) = (best.gap, best.overlap);
            let y_overlap = y_high - y_low;
            let (
                first_wall,
                second_wall,
                authority,
                first_support,
                second_support,
                shift,
                ambiguous,
            ) = if coplanar || is_thickness_step(kind) {
                // No mover: report in authored index order, no rigid delta.
                (
                    first.index,
                    second.index,
                    "index",
                    0usize,
                    0usize,
                    0.0,
                    false,
                )
            } else {
                let y = (y_low, y_high);
                let first_support = slice_support(&walls, first, span_first, y);
                let second_support = slice_support(&walls, second, span_second, y);
                let (winner, authority, repairable) = if first_support != second_support {
                    let winner = if first_support > second_support {
                        first.index
                    } else {
                        second.index
                    };
                    (winner, "chain", true)
                } else if relative_difference(first.solid_span(), second.solid_span()) > 0.01 {
                    let winner = if first.solid_span() > second.solid_span() {
                        first.index
                    } else {
                        second.index
                    };
                    (winner, "length", true)
                } else {
                    // Equal support and equal solid span: the authority is
                    // ambiguous, so the joint is review only.
                    (first.index.min(second.index), "index", false)
                };
                // The mover is always `second`; `shift` is the delta that
                // lands its low plane on the winner's low plane.
                if winner == second.index {
                    let shift = second.planes.0 - first.planes.0;
                    (
                        second.index,
                        first.index,
                        authority,
                        first_support,
                        second_support,
                        shift,
                        !repairable,
                    )
                } else {
                    let shift = first.planes.0 - second.planes.0;
                    (
                        first.index,
                        second.index,
                        authority,
                        first_support,
                        second_support,
                        shift,
                        !repairable,
                    )
                }
            };
            // A coplanar gap is a review-only near-step candidate; an
            // ambiguous authority is never an automatic step either.
            let reported_kind = if coplanar || (ambiguous && kind == "step") {
                "near-step"
            } else {
                kind
            };
            let auto = reported_kind == "step";
            let along = anchor_along(span_first, span_second);
            let across = f32::midpoint(
                first.planes.0.min(second.planes.0),
                first.planes.1.max(second.planes.1),
            );
            let y_mid = f32::midpoint(y_low, y_high);
            let (first_low, first_high, second_low, second_high) = if first_wall == first.index {
                (
                    first.planes.0,
                    first.planes.1,
                    second.planes.0,
                    second.planes.1,
                )
            } else {
                (
                    second.planes.0,
                    second.planes.1,
                    first.planes.0,
                    first.planes.1,
                )
            };
            out.push(WallJointCandidate {
                joint: WallJoint {
                    first: first_wall,
                    second: second_wall,
                    axis: match first.axis {
                        WallAxis::X => "x",
                        WallAxis::Z => "z",
                    },
                    kind: reported_kind,
                    first_low: quantise(first_low),
                    first_high: quantise(first_high),
                    second_low: quantise(second_low),
                    second_high: quantise(second_high),
                    shift: quantise(shift),
                    gap_m: gap,
                    overlap_m: overlap,
                    y_overlap_m: y_overlap,
                    authority,
                    first_support,
                    second_support,
                    auto_repairable: auto,
                    position: joint_position(first.axis, along, across, y_mid),
                },
                first_span: span_first,
                second_span: span_second,
                y: (y_low, y_high),
            });
        }
    }
    // Deterministic order: by the unordered wall pair.
    out.sort_by_key(|candidate| {
        (
            candidate.joint.first.min(candidate.joint.second),
            candidate.joint.first.max(candidate.joint.second),
        )
    });
    out
}

/// True for the review-only thickness transition class.
fn is_thickness_step(kind: &str) -> bool {
    kind == "thickness-step"
}

/// Relative difference of two positive spans, 0 when both are 0.
fn relative_difference(a: f32, b: f32) -> f32 {
    let largest = a.abs().max(b.abs());
    if largest <= f32::EPSILON {
        return 0.0;
    }
    (a - b).abs() / largest
}

/// The anchor along the length axis: the middle of the gap for separated
/// slices, the meeting point for touching ones and the overlap centre for
/// overlapping ones.
fn anchor_along(a: (f32, f32), b: (f32, f32)) -> f32 {
    if a.1 <= b.0 + JOINT_ADJACENCY_M {
        f32::midpoint(a.1, b.0)
    } else if b.1 <= a.0 + JOINT_ADJACENCY_M {
        f32::midpoint(b.1, a.0)
    } else {
        f32::midpoint(a.1.min(b.1), a.0.max(b.0))
    }
}

/// The deterministic wall-plane joint candidates of one level.
///
/// The checker and the repair planner both call this function, so a finding
/// and its planned repair can never disagree. It replays the engine's own
/// solid slicing, so openings, lintels and sloped ceiling profiles participate.
#[must_use]
pub fn wall_joints(level: &LevelDef) -> Vec<WallJoint> {
    let surfaces = LevelSurfaces::new(level);
    compute_wall_joints(level, &surfaces)
        .into_iter()
        .map(|candidate| candidate.joint)
        .collect()
}

/// How serious a finding is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum Severity {
    /// A confirmed defect.
    Error,
    /// A heuristic warning; may be intentional and can be annotated.
    Warning,
}

impl Severity {
    /// Lower-case name used in the reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
        }
    }
}

/// One reported finding: its check, severity, element id and a locatable
/// position.
#[derive(Clone, Debug, Serialize)]
pub struct Finding {
    /// Stable check id, e.g. `duplicate-surface`.
    pub check: &'static str,
    /// `error` or `warning`.
    pub severity: Severity,
    /// The element the finding belongs to (`wall 3`, `room 2`, `pillar 0`, ...).
    pub id: String,
    /// Human-readable explanation, including the numbers that proved it.
    pub message: String,
    /// Anchor position for the finding, world `(x, y, z)`.
    pub position: [f32; 3],
    /// Optional measured surface triangles for editor highlights. Empty for
    /// non-surface diagnostics; additive in checker protocol version 1.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub surfaces: Vec<[[f32; 3]; 3]>,
}

/// One reproducible marker anchor, for a viewer or an overlay.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Marker {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// Everything one checker run found.
#[derive(Clone, Debug)]
pub struct CheckReport {
    /// Where the level was read from (a path, or a labelled embedded source).
    pub source: String,
    /// The level's own `id`.
    pub level_id: String,
    /// Whether the loader's own validation accepted the level.
    pub validated: bool,
    /// Every finding, in deterministic order.
    pub findings: Vec<Finding>,
    /// How many findings were suppressed by a `geometry_intent` annotation.
    pub suppressed: BTreeMap<String, usize>,
}

impl CheckReport {
    /// Confirmed defects.
    #[must_use]
    pub fn error_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == Severity::Error)
            .count()
    }

    /// Heuristic warnings.
    #[must_use]
    pub fn warning_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.severity == Severity::Warning)
            .count()
    }

    /// The process exit status the report implies.
    #[must_use]
    pub fn exit_status(&self, strict: bool) -> i32 {
        i32::from(self.error_count() > 0 || (strict && self.warning_count() > 0))
    }

    /// Marker anchors for every finding (errors first).
    #[must_use]
    pub fn markers(&self) -> Vec<Marker> {
        self.findings
            .iter()
            .map(|finding| Marker {
                x: finding.position[0],
                y: finding.position[1],
                z: finding.position[2],
            })
            .collect()
    }

    /// The human-readable report.
    #[must_use]
    pub fn to_human(&self, verbose: bool) -> String {
        let mut out = String::new();
        let _formatted_geometry_check = writeln!(
            out,
            "geometry check: {} ({}){}",
            self.level_id,
            self.source,
            if self.validated {
                ""
            } else {
                " [loader validation failed]"
            }
        );
        let mut by_check: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for finding in &self.findings {
            let entry = by_check.entry(finding.check).or_insert((0, 0));
            if finding.severity == Severity::Error {
                entry.0 = entry.0.saturating_add(1);
            } else {
                entry.1 = entry.1.saturating_add(1);
            }
        }
        for (check, (errors, warnings)) in &by_check {
            let _formatted_check_errors =
                writeln!(out, "  {check}: {errors} error(s), {warnings} warning(s)");
        }
        for (check, count) in &self.suppressed {
            if *count > 0 {
                let _formatted_check_count =
                    writeln!(out, "  {check}: {count} suppressed by geometry_intent");
            }
        }
        for finding in &self.findings {
            if !verbose && finding.severity == Severity::Warning {
                continue;
            }
            let _formatted_record = writeln!(
                out,
                "{} [{}] {}: {} @ ({:.3}, {:.3}, {:.3})",
                match finding.severity {
                    Severity::Error => "ERROR  ",
                    Severity::Warning => "WARNING",
                },
                finding.check,
                finding.id,
                finding.message,
                finding.position[0],
                finding.position[1],
                finding.position[2]
            );
        }
        let _formatted_summary_error = writeln!(
            out,
            "summary: {} error(s), {} warning(s){}",
            self.error_count(),
            self.warning_count(),
            if self.suppressed.values().sum::<usize>() > 0 {
                ", some warnings suppressed by intent annotations"
            } else {
                ""
            }
        );
        out
    }

    /// The machine-readable report.
    ///
    /// # Errors
    ///
    /// Returns the `serde_json` error if the report cannot be serialized,
    /// which cannot happen for finite geometry; callers may treat it as fatal.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut checks: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
        for finding in &self.findings {
            let entry = checks.entry(finding.check).or_insert((0, 0));
            if finding.severity == Severity::Error {
                entry.0 = entry.0.saturating_add(1);
            } else {
                entry.1 = entry.1.saturating_add(1);
            }
        }
        let report = serde_json::json!({
            "format": CHECK_FORMAT,
            "version": CHECK_VERSION,
            "level": { "id": self.level_id, "source": self.source, "validated": self.validated },
            "summary": {
                "errors": self.error_count(),
                "warnings": self.warning_count(),
                "findings": self.findings.len(),
                "checks": checks
                    .into_iter()
                    .map(|(check, (errors, warnings))| {
                        (check.to_string(), serde_json::json!({ "errors": errors, "warnings": warnings }))
                    })
                    .collect::<serde_json::Map<_, _>>(),
                "suppressed": self.suppressed,
            },
            "findings": self.findings,
            "markers": self.markers(),
        });
        serde_json::to_string_pretty(&report)
    }
}

/// The CLI options of `--check-geometry`.
#[derive(Clone, Debug)]
pub struct CliOptions {
    /// Level path or id (`places_demo`, `levels/places_demo.json`, ...).
    pub level: String,
    /// Write the machine-readable report here, in addition to the human one.
    pub json: Option<PathBuf>,
    /// Write deterministic marker anchors (JSON) here.
    pub markers: Option<PathBuf>,
    /// Write a marker OBJ (a small cross per finding) here.
    pub markers_obj: Option<PathBuf>,
    /// Export engine-resolved static prop placements for the offline GLB audit.
    pub prop_instances: Option<PathBuf>,
    /// Treat warnings as a failing status.
    pub strict: bool,
    /// Print warnings in the human report too.
    pub verbose: bool,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            level: String::new(),
            json: None,
            markers: None,
            markers_obj: None,
            prop_instances: None,
            strict: false,
            verbose: true,
        }
    }
}

/// Parses the process arguments, or `None` when this is not a checker run.
///
/// # Errors
///
/// Returns a usage message for an unknown argument or a missing value.
pub fn options_from_args(args: &[String]) -> Result<Option<CliOptions>, String> {
    if !args.iter().any(|arg| arg == "--check-geometry") {
        return Ok(None);
    }
    let mut options = CliOptions::default();
    let mut index = 0usize;
    while let Some(arg) = args.get(index) {
        match arg.as_str() {
            "--check-geometry" => {}
            "--level" => {
                index = index.saturating_add(1);
                options.level = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| "--level needs a value".to_string())?;
            }
            "--json" => {
                index = index.saturating_add(1);
                options.json = Some(PathBuf::from(
                    args.get(index)
                        .cloned()
                        .ok_or_else(|| "--json needs a value".to_string())?,
                ));
            }
            "--markers" => {
                index = index.saturating_add(1);
                options.markers = Some(PathBuf::from(
                    args.get(index)
                        .cloned()
                        .ok_or_else(|| "--markers needs a value".to_string())?,
                ));
            }
            "--markers-obj" => {
                index = index.saturating_add(1);
                options.markers_obj = Some(PathBuf::from(
                    args.get(index)
                        .cloned()
                        .ok_or_else(|| "--markers-obj needs a value".to_string())?,
                ));
            }
            "--strict" => options.strict = true,
            "--prop-instances" => {
                index = index.saturating_add(1);
                options.prop_instances =
                    Some(PathBuf::from(args.get(index).cloned().ok_or_else(
                        || "--prop-instances needs a path".to_string(),
                    )?));
            }
            "--quiet" => options.verbose = false,
            other if other.starts_with("--") => {
                return Err(format!("unknown checker argument `{other}`"));
            }
            other => options.level = other.to_string(),
        }
        index = index.saturating_add(1);
    }
    if options.level.trim().is_empty() {
        return Err("--level needs an explicit source path or installed source id".to_string());
    }
    Ok(Some(options))
}

/// Resolves a level argument to a readable source: an explicit path, a
/// shipped level id, or a drop-in level id under `levels/`.
fn resolve_source(level: &str) -> Result<(PathBuf, String), String> {
    let candidates: Vec<PathBuf> = if level.to_ascii_lowercase().ends_with(".json") {
        vec![PathBuf::from(level)]
    } else {
        vec![PathBuf::from(format!("levels/{level}.json"))]
    };
    for candidate in candidates {
        match std::fs::read_to_string(&candidate) {
            Ok(text) => return Ok((candidate, text)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!("cannot read {}: {error}", candidate.display()));
            }
        }
    }
    Err(format!("no level matched `{level}`"))
}

/// Runs the checker for one already-parsed level.
#[must_use]
pub fn check_level(level: &LevelDef, source: &str, validated: bool) -> CheckReport {
    let mut checker = Checker::new(level);
    let surfaces = LevelSurfaces::new(level);

    let materials = crate::engine::render::logical_materials(level);
    let catalog = crate::engine::loader::PropCatalog::builtin();
    let mesh = crate::engine::render::build_level_geometry_with_catalog_and_materials(
        level, &catalog, &materials,
    );

    checker.check_mesh(&mesh);
    let triangles = collect_triangles(&mesh);
    checker.check_duplicate_surfaces(&triangles);
    checker.check_wall_joints(&surfaces, &triangles, &materials);

    let colliders = authored_colliders(level, &surfaces);
    let engine_boxes = level.collision_aabbs();
    let index = SpatialHash::build(&triangles);
    checker.check_colliders(&colliders, &engine_boxes, &triangles, &index);
    // The placeholder mesh uses the fallback catalog, but prop sizes are read
    // from the shipped catalog so the layering heuristic sees real extents.
    checker.check_props(
        &crate::engine::loader::PropCatalog::load_default(),
        &surfaces,
    );
    checker.check_openings(&surfaces);
    checker.check_curves(&surfaces);
    checker.check_rooms(&surfaces, &colliders);
    checker.check_spawn(&surfaces);

    let mut report = CheckReport {
        source: source.to_string(),
        level_id: level.id.clone(),
        validated,
        findings: checker.findings,
        suppressed: checker.suppressed,
    };
    report.findings.sort_by(|a, b| {
        a.severity
            .cmp(&b.severity)
            .then_with(|| a.check.cmp(b.check))
            .then_with(|| a.id.cmp(&b.id))
            .then_with(|| a.message.cmp(&b.message))
    });
    report
}

/// Runs the checker end to end and returns the exit status.
///
/// # Errors
///
/// Returns a usage or file error, which the caller reports as status `2`.
// This is the CLI's own reporting path; there is no logger in a headless run.
#[expect(
    clippy::print_stderr,
    reason = "This is the CLI's own reporting path; there is no logger in a headless run."
)]
pub fn run(options: &CliOptions) -> Result<i32, String> {
    let (path, text) = resolve_source(&options.level)?;
    let mut level = match LevelDef::from_json(&text) {
        Ok(level) => level,
        Err(error) => {
            eprintln!("geometry check: {} does not parse: {error}", path.display());
            return Ok(2);
        }
    };
    let validation = crate::engine::loader::validate_level(&level);
    let validated = validation.is_ok();
    if let Err(error) = validation {
        // The loader rejected the level; still report every measurable defect
        // that can be read from the parsed document, but do not run the
        // preparation pass the game would.
        let mut report = CheckReport {
            source: path.display().to_string(),
            level_id: level.id.clone(),
            validated: false,
            findings: vec![Finding {
                check: "level-invalid",
                severity: Severity::Error,
                id: level.id.clone(),
                message: format!("loader validation rejected the level: {error}"),
                position: [0.0, 0.0, 0.0],
                surfaces: Vec::new(),
            }],
            suppressed: BTreeMap::new(),
        };
        let checked = check_level(&level, &path.display().to_string(), false);
        report.findings.extend(checked.findings);
        report.suppressed = checked.suppressed;
        return finish(&report, options);
    }
    let catalog = crate::engine::assets::AssetCatalog::load_default();
    crate::map_compiler::prepare_level(&mut level, &catalog, None);
    if let Some(output) = &options.prop_instances {
        write_prop_instance_manifest(&level, &text, output)?;
    }
    let report = check_level(&level, &path.display().to_string(), validated);
    finish(&report, options)
}

/// Export only engine-resolved fixed placements; no GLB loading or map bake.
fn write_prop_instance_manifest(
    level: &LevelDef,
    source: &str,
    output: &std::path::Path,
) -> Result<(), String> {
    let surfaces = LevelSurfaces::new(level);
    let catalog = crate::engine::loader::PropCatalog::load_default();
    let root = crate::engine::props::resolve_prop_root().unwrap_or_else(|| PathBuf::from("assets"));
    let mut instances = Vec::new();
    let mut excluded = Vec::new();
    for (index, prop) in level.props.iter().enumerate() {
        let owner = format!(
            "props/{index}/{}",
            prop.id.as_deref().unwrap_or(&prop.model)
        );
        if prop.float.is_some() || prop.is_physics_movable() {
            excluded
                .push(serde_json::json!({"owner": owner, "reason": "live movable/floating pose"}));
            continue;
        }
        let Some(model) = catalog.get(&prop.model).model else {
            excluded.push(serde_json::json!({"owner": owner, "reason": "no static GLB model"}));
            continue;
        };
        let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
        instances.push(serde_json::json!({
            "owner": owner,
            "model": root.join(model),
            "matrix": prop.placement_matrix(base_y).to_cols_array(),
            "character_texture": prop.character_texture,
            "material_overrides": prop.material_overrides,
        }));
    }
    let document = serde_json::json!({"format": "places-prop-instances", "version": 1_u32,
        "level": level.id, "source_sha256": sha256_hex(source.as_bytes()),
        "instances": instances, "excluded": excluded});
    let payload = serde_json::to_vec_pretty(&document).map_err(|error| error.to_string())?;
    std::fs::write(output, payload)
        .map_err(|error| format!("cannot write {}: {error}", output.display()))
}

/// Prints and writes one report, returning the exit status.
// This is the CLI's own reporting path; there is no logger in a headless run.
#[expect(
    clippy::print_stdout,
    reason = "This is the CLI's own reporting path; there is no logger in a headless run."
)]
fn finish(report: &CheckReport, options: &CliOptions) -> Result<i32, String> {
    let status = report.exit_status(options.strict);
    if options.verbose || report.error_count() > 0 {
        print!("{}", report.to_human(options.verbose));
    }
    if let Some(path) = &options.json {
        let json = report
            .to_json()
            .map_err(|error| format!("cannot serialize the report: {error}"))?;
        std::fs::write(path, json)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    if let Some(path) = &options.markers {
        let json = serde_json::to_string_pretty(&report.markers())
            .map_err(|error| format!("cannot serialize markers: {error}"))?;
        std::fs::write(path, json)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    if let Some(path) = &options.markers_obj {
        let obj = marker_obj(&report.markers());
        std::fs::write(path, obj)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    Ok(status)
}

/// A tiny OBJ of axis-aligned crosses, one per marker.
fn marker_obj(markers: &[Marker]) -> String {
    let mut out = String::from("# places geometry-check markers\n");
    if markers.is_empty() {
        return out;
    }
    for marker in markers {
        let size = 0.2_f32;
        let anchor = [marker.x, marker.y, marker.z];
        for axis in 0..3usize {
            let mut low = anchor;
            let mut high = anchor;
            if let Some(slot) = low.get_mut(axis) {
                *slot -= size;
            }
            if let Some(slot) = high.get_mut(axis) {
                *slot += size;
            }
            let _formatted_v = writeln!(out, "v {} {} {}", low[0], low[1], low[2]);
            let _formatted_v_2 = writeln!(out, "v {} {} {}", high[0], high[1], high[2]);
        }
    }
    for line in 0..markers.len() {
        let base = line.saturating_mul(6).saturating_add(1);
        let _formatted_l_base = writeln!(out, "l {base} {}", base.saturating_add(1));
        let _formatted_l = writeln!(
            out,
            "l {} {}",
            base.saturating_add(2),
            base.saturating_add(3)
        );
        let _formatted_l_2 = writeln!(
            out,
            "l {} {}",
            base.saturating_add(4),
            base.saturating_add(5)
        );
    }
    out
}

/// Which surface families count as architecture for the duplicate check.
const fn is_architecture_kind(kind: SurfaceKind) -> bool {
    matches!(
        kind,
        SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall
    )
}

/// One emitted triangle, reduced to what the checks read.
#[derive(Clone, Copy, Debug)]
struct Tri {
    key: SurfaceKey,
    points: [[f32; 3]; 3],
    normal: [f32; 3],
    centroid: [f32; 3],
    area: f32,
}

/// Collects every triangle of the architecture families.
fn collect_triangles(mesh: &LevelMesh) -> Vec<Tri> {
    let mut out = Vec::new();
    for range in &mesh.ranges {
        if !is_architecture_kind(range.key.kind) {
            continue;
        }
        let mut chunk = [0u16; 3];
        let mut filled = 0usize;
        for index in &range.indices {
            if let Some(slot) = chunk.get_mut(filled) {
                *slot = *index;
            }
            filled = filled.saturating_add(1);
            if filled < 3 {
                continue;
            }
            filled = 0;
            let a = range
                .vertices
                .get(usize::from(chunk[0]))
                .copied()
                .unwrap_or(Vertex::UNLIT);
            let b = range
                .vertices
                .get(usize::from(chunk[1]))
                .copied()
                .unwrap_or(Vertex::UNLIT);
            let c = range
                .vertices
                .get(usize::from(chunk[2]))
                .copied()
                .unwrap_or(Vertex::UNLIT);
            let points = [a.pos, b.pos, c.pos];
            let cross = cross3(sub3(points[1], points[0]), sub3(points[2], points[0]));
            let area = 0.5 * length3(cross);
            let normal = normalized3(cross);
            let centroid = [
                (points[0][0] + points[1][0] + points[2][0]) / 3.0,
                (points[0][1] + points[1][1] + points[2][1]) / 3.0,
                (points[0][2] + points[1][2] + points[2][2]) / 3.0,
            ];
            out.push(Tri {
                key: range.key,
                points,
                normal,
                centroid,
                area,
            });
        }
    }
    out
}

/// A collider with the authored element it came from.
#[derive(Clone, Debug)]
struct Collider {
    /// Element label (`wall 3`, `arc wall 0`, ...).
    source: String,
    aabb: WallAabb,
    /// True for a solid whose AABB is *not* surface-tight by design — a
    /// curved primitive (over-covering at the tessellation corners) or a
    /// guardrail (its barrier box deliberately reaches below the rails). Its
    /// coverage is proven by its own checks instead of by ghost probing.
    ghost_checked: bool,
}

/// Every wall and architectural solid as a collider, with provenance.
///
/// This replays the engine's own decomposition (`wall_solid_slices_profiled`
/// plus the architecture solids) so each box can be named; the checker then
/// compares the result against the engine's own `collision_aabbs()` to prove
/// the two interpretations agree.
fn authored_colliders(level: &LevelDef, surfaces: &LevelSurfaces<'_>) -> Vec<Collider> {
    let mut out = Vec::new();
    for (index, wall) in level.walls.iter().enumerate() {
        let breaks = surfaces.wall_profile_breaks(wall);
        let clear = |offset: f32| surfaces.clear_ceiling_height_along(wall, offset);
        for slice in wall_solid_slices_profiled(wall, clear, &breaks) {
            out.push(Collider {
                source: format!("wall {index}"),
                aabb: wall_slice_aabb(wall, &slice),
                ghost_checked: true,
            });
        }
    }
    for (index, piece) in level.half_walls.iter().enumerate() {
        if let Some(boxed) = piece.solid_box(surfaces) {
            out.push(Collider {
                source: format!("half wall {index}"),
                aabb: boxed.to_wall_aabb(),
                ghost_checked: true,
            });
        }
    }
    for (index, piece) in level.columns.iter().enumerate() {
        if let Some(boxed) = piece.solid_box(surfaces) {
            out.push(Collider {
                source: format!("column {index}"),
                aabb: boxed.to_wall_aabb(),
                ghost_checked: true,
            });
        }
    }
    for (index, piece) in level.arc_walls.iter().enumerate() {
        for boxed in piece.collision_boxes(surfaces) {
            out.push(Collider {
                source: format!("arc wall {index}"),
                aabb: boxed.to_wall_aabb(),
                ghost_checked: false,
            });
        }
    }
    for (index, piece) in level.pillars.iter().enumerate() {
        for boxed in piece.collision_boxes(surfaces) {
            out.push(Collider {
                source: format!("pillar {index}"),
                aabb: boxed.to_wall_aabb(),
                ghost_checked: false,
            });
        }
    }
    for (index, piece) in level.archways.iter().enumerate() {
        for boxed in piece.solid_boxes(surfaces) {
            out.push(Collider {
                source: format!("archway {index}"),
                aabb: boxed.to_wall_aabb(),
                ghost_checked: true,
            });
        }
    }
    for (index, piece) in level.guardrails.iter().enumerate() {
        if let Some(boxed) = piece.solid_box(surfaces) {
            out.push(Collider {
                source: format!("guardrail {index}"),
                aabb: boxed.to_wall_aabb(),
                ghost_checked: false,
            });
        }
    }
    out
}

/// One solid wall slice as an AABB, mirroring the engine's collision walk.
fn wall_slice_aabb(wall: &WallDef, slice: &crate::engine::level::WallSlice) -> WallAabb {
    let (min_x, max_x) = (
        wall.x.min(wall.x + wall.width),
        wall.x.max(wall.x + wall.width),
    );
    let (min_z, max_z) = (
        wall.z.min(wall.z + wall.depth),
        wall.z.max(wall.z + wall.depth),
    );
    let (origin_x, origin_z) = wall.length_origin();
    let (x, z, width, depth) = match wall.axis() {
        WallAxis::X => (
            origin_x + slice.start,
            min_z,
            slice.end - slice.start,
            max_z - min_z,
        ),
        WallAxis::Z => (
            min_x,
            origin_z + slice.start,
            max_x - min_x,
            slice.end - slice.start,
        ),
    };
    WallAabb::with_y(x, slice.bottom, z, width, slice.top - slice.bottom, depth)
}

/// A uniform-grid index from world position to triangle indices.
struct SpatialHash {
    cells: HashMap<(i32, i32, i32), Vec<u32>>,
}

impl SpatialHash {
    fn build(triangles: &[Tri]) -> Self {
        let mut cells: HashMap<(i32, i32, i32), Vec<u32>> = HashMap::new();
        for (index, triangle) in triangles.iter().enumerate() {
            let Some(slot) = u32::try_from(index).ok() else {
                continue;
            };
            for cell in triangle_cells(triangle.points) {
                cells.entry(cell).or_default().push(slot);
            }
        }
        Self { cells }
    }

    fn near(&self, point: [f32; 3]) -> Option<&[u32]> {
        self.cells
            .get(&cell_of(point[0], point[1], point[2]))
            .map(Vec::as_slice)
    }
}

/// The grid cells one triangle touches.
///
/// A triangle is filed in *every* cell its bounding box covers, not just the
/// cell holding its centroid: a wall face can be tens of metres long (the
/// lightmap chart cap alone allows 63.75 m), and a face centre that falls
/// between its file cells reads as an uncovered collider face. The per-axis cap
/// is therefore wider than any legal static triangle, and the total cap bounds
/// the transient index a pathological level can build. A triangle that still
/// exceeds the cap keeps the cells nearest its minimum corner; that can only
/// lose a face *sample*, never report a false defect the geometry does not have.
fn triangle_cells(points: [[f32; 3]; 3]) -> Vec<(i32, i32, i32)> {
    /// Total cells one triangle may be filed in.
    const MAX_CELLS_PER_TRIANGLE: usize = 16_384;
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for point in points {
        for (axis, value) in point.iter().enumerate() {
            if let (Some(lo), Some(hi)) = (min.get_mut(axis), max.get_mut(axis)) {
                *lo = lo.min(*value);
                *hi = hi.max(*value);
            }
        }
    }
    let mut out = Vec::new();
    for ix in cell_range(min[0], max[0]) {
        for iy in cell_range(min[1], max[1]) {
            for iz in cell_range(min[2], max[2]) {
                out.push((ix, iy, iz));
                if out.len() >= MAX_CELLS_PER_TRIANGLE {
                    return out;
                }
            }
        }
    }
    out
}

/// The inclusive cell indices one span touches, capped per axis.
fn cell_range(low: f32, high: f32) -> std::ops::RangeInclusive<i32> {
    let first = cell_index(low);
    let last = cell_index(high).max(first);
    first..=last.min(first.saturating_add(MAX_CELLS_PER_AXIS - 1))
}

/// Cells one axis of a triangle's bounding box may span.
///
/// Wider than any legal static triangle (a 63.75 m lightmap chart is 65 cells),
/// so a face centre always lands in a cell its own triangle was filed in.
const MAX_CELLS_PER_AXIS: i32 = 256;

/// Grid cell index of one world coordinate.
fn cell_index(value: f32) -> i32 {
    let scaled = (value / SPATIAL_CELL_M).floor();
    if !scaled.is_finite() {
        return 0;
    }
    // Bounded to a sane world extent before the cast so a wild coordinate can
    // never wrap.
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        reason = "Bounded to a sane world extent before the cast so a wild coordinate can never wrap."
    )]
    let index = scaled.clamp(-1.0e6, 1.0e6) as i32;
    index
}

/// Grid cell containing a point.
fn cell_of(x: f32, y: f32, z: f32) -> (i32, i32, i32) {
    (cell_index(x), cell_index(y), cell_index(z))
}

/// The checker's own state while one level is examined.
struct Checker<'a> {
    level: &'a LevelDef,
    findings: Vec<Finding>,
    suppressed: BTreeMap<String, usize>,
    counts: HashMap<&'static str, usize>,
}

impl<'a> Checker<'a> {
    fn new(level: &'a LevelDef) -> Self {
        Self {
            level,
            findings: Vec::new(),
            suppressed: BTreeMap::new(),
            counts: HashMap::new(),
        }
    }

    /// True when an intent annotation covers this heuristic position.
    fn annotated(&self, check: &str, x: f32, z: f32) -> bool {
        self.level
            .geometry_intent
            .iter()
            .any(|annotation| annotation.covers(check, x, z))
    }

    /// Records one finding, applying the caps and the intent annotations.
    fn push(
        &mut self,
        check: &'static str,
        severity: Severity,
        id: impl Into<String>,
        message: impl Into<String>,
        position: [f32; 3],
    ) {
        if severity == Severity::Warning && self.annotated(check, position[0], position[2]) {
            let entry = self.suppressed.entry(check.to_string()).or_insert(0);
            *entry = entry.saturating_add(1);
            return;
        }
        let count = self.counts.entry(check).or_insert(0);
        *count = count.saturating_add(1);
        if *count > MAX_FINDINGS_PER_CHECK {
            if *count == MAX_FINDINGS_PER_CHECK.saturating_add(1) {
                self.findings.push(Finding {
                    check,
                    severity,
                    id: id.into(),
                    message: format!(
                        "more than {MAX_FINDINGS_PER_CHECK} `{check}` findings; the summary counts them all"
                    ),
                    position,
                    surfaces: Vec::new(),
                });
            }
            return;
        }
        self.findings.push(Finding {
            check,
            severity,
            id: id.into(),
            message: message.into(),
            position,
            surfaces: Vec::new(),
        });
    }

    /// Carries the two measured patches only when this call actually emitted
    /// a finding. Suppressed warnings and cap summaries never borrow a prior
    /// finding's surface highlights.
    fn push_surfaces(
        &mut self,
        check: &'static str,
        severity: Severity,
        id: impl Into<String>,
        message: impl Into<String>,
        triangles: [&Tri; 2],
    ) {
        let [first, second] = triangles;
        let previous = self.findings.len();
        self.push(check, severity, id, message, first.centroid);
        if self.findings.len() > previous
            && self
                .counts
                .get(check)
                .is_some_and(|count| *count <= MAX_FINDINGS_PER_CHECK)
            && let Some(finding) = self.findings.last_mut()
        {
            finding.surfaces = vec![first.points, second.points];
        }
    }
}

// ---------------------------------------------------------------------------
// Mesh integrity
// ---------------------------------------------------------------------------

impl Checker<'_> {
    /// Non-finite vertices and degenerate triangles in the emitted mesh.
    fn check_mesh(&mut self, mesh: &LevelMesh) {
        for vertex in mesh.all_vertices() {
            let finite = vertex.pos.iter().all(|value| value.is_finite())
                && vertex.uv.iter().all(|value| value.is_finite())
                && vertex.color.iter().all(|value| value.is_finite())
                && vertex.normal.iter().all(|value| value.is_finite())
                && vertex.tangent.iter().all(|value| value.is_finite());
            if !finite {
                self.push(
                    "non-finite-vertex",
                    Severity::Error,
                    "mesh",
                    "a generated vertex carries a non-finite position, uv, colour or frame",
                    vertex.pos,
                );
                break;
            }
        }
        for range in &mesh.ranges {
            if !is_architecture_kind(range.key.kind) {
                continue;
            }
            let mut chunk: [[f32; 3]; 3] = [[0.0; 3]; 3];
            let mut filled = 0usize;
            for index in &range.indices {
                let Some(vertex) = range.vertices.get(usize::from(*index)) else {
                    continue;
                };
                if let Some(slot) = chunk.get_mut(filled) {
                    *slot = vertex.pos;
                }
                filled = filled.saturating_add(1);
                if filled < 3 {
                    continue;
                }
                filled = 0;
                let area =
                    0.5 * length3(cross3(sub3(chunk[1], chunk[0]), sub3(chunk[2], chunk[0])));
                let coincident = distance_sq(chunk[0], chunk[1]) <= 1.0e-12
                    || distance_sq(chunk[1], chunk[2]) <= 1.0e-12
                    || distance_sq(chunk[0], chunk[2]) <= 1.0e-12;
                if area <= 1.0e-9 || coincident {
                    self.push(
                        "degenerate-face",
                        Severity::Error,
                        format!("{:?} m{}", range.key.kind, range.key.material),
                        format!(
                            "zero-area or repeated-corner triangle (area {area:.3e} m², points {chunk:?})"
                        ),
                        chunk[0],
                    );
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Duplicate and coplanar surfaces
// ---------------------------------------------------------------------------

/// Conservative spatial pairs, independent of plane bins and world origin.
///
/// Sorted X intervals stop at the first disjoint later span. Every visited
/// pair consumes the work budget before normal, Y/Z or polygon rejection;
/// neither coincident sheets nor unrelated active spans permit unbounded work.
fn coplanar_candidates(triangles: &[Tri], limit: usize) -> (Vec<(usize, usize)>, bool) {
    let mut ordered: Vec<_> = triangles
        .iter()
        .enumerate()
        .filter(|(_, triangle)| triangle.area > 1.0e-9)
        .map(|(index, triangle)| {
            let bounds = std::array::from_fn::<_, 3, _>(|axis| {
                triangle.points.iter().fold(
                    (f32::INFINITY, f32::NEG_INFINITY),
                    |(low, high), point| {
                        let value = point.get(axis).copied().unwrap_or(0.0);
                        (low.min(value), high.max(value))
                    },
                )
            });
            (index, bounds)
        })
        .collect();
    ordered.sort_by(|first, second| {
        first.1[0]
            .0
            .total_cmp(&second.1[0].0)
            .then(first.0.cmp(&second.0))
    });
    let mut pairs = Vec::new();
    let mut work = 0usize;
    for (slot, (first, first_bounds)) in ordered.iter().enumerate() {
        for (second, second_bounds) in ordered.iter().skip(slot.saturating_add(1)) {
            if second_bounds[0].0 > first_bounds[0].1 + OVERLAP_BOUNDS_EPS_M {
                break;
            }
            if work >= limit {
                return (pairs, false);
            }
            work = work.saturating_add(1);
            if first_bounds
                .iter()
                .zip(second_bounds)
                .skip(1)
                .any(|(a, b)| a.0 > b.1 + OVERLAP_BOUNDS_EPS_M || b.0 > a.1 + OVERLAP_BOUNDS_EPS_M)
            {
                continue;
            }
            pairs.push(((*first).min(*second), (*first).max(*second)));
        }
    }
    pairs.sort_unstable();
    (pairs, true)
}

impl Checker<'_> {
    fn incomplete_overlap_audit(&mut self) {
        self.push(
            "geometry-check-incomplete",
            Severity::Error,
            "coplanar candidate budget",
            "coplanar candidate budget exhausted; this audit is incomplete",
            [0.0; 3],
        );
    }

    /// Confirmed duplicated coplanar surfaces: two triangles on one plane
    /// whose projected areas overlap by more than a threshold.
    fn check_duplicate_surfaces(&mut self, triangles: &[Tri]) {
        let (pairs, complete) = coplanar_candidates(triangles, 2_000_000);
        if !complete {
            self.incomplete_overlap_audit();
        }
        for (first, second) in pairs {
            let (Some(a), Some(b)) = (triangles.get(first), triangles.get(second)) else {
                continue;
            };
            // The conservative spatial pass admits tilted and nearby faces;
            // require actual parallel planes before measuring overlap.
            if dot3(a.normal, b.normal).abs() < 0.999_999 {
                continue;
            }
            let separation = b
                .points
                .iter()
                .map(|point| dot3(a.normal, sub3(*point, a.points[0])).abs())
                .fold(0.0_f32, f32::max);
            if separation > OVERLAP_PLANE_EPS_M {
                continue;
            }
            let area = triangle_overlap_area(a, b);
            if area <= OVERLAP_AREA_EPS_M2 {
                continue;
            }
            if area <= SLIVER_AREA_EPS_M2 {
                self.push_surfaces(
                    "coplanar-sliver",
                    Severity::Warning,
                    format!("{:?} m{}", a.key.kind, a.key.material),
                    format!(
                        "a {area:.5} m² coplanar overlap sliver at a joint \
                                 (a at {:.2},{:.2},{:.2}; b at {:.2},{:.2},{:.2})",
                        a.centroid[0],
                        a.centroid[1],
                        a.centroid[2],
                        b.centroid[0],
                        b.centroid[1],
                        b.centroid[2]
                    ),
                    [a, b],
                );
                continue;
            }
            let same_family_horizontal = a.key.kind == b.key.kind
                && matches!(a.key.kind, SurfaceKind::Floor | SurfaceKind::Ceiling);
            if same_family_horizontal {
                // Overlapping rooms emit both floors/ceilings by design
                // (documented), so this is a heuristic: unless an
                // author annotates the area, it is reported so a
                // genuinely doubled slab is still visible in the
                // report.
                self.push_surfaces(
                    "overlap-emission",
                    Severity::Warning,
                    format!("{:?} m{}", a.key.kind, a.key.material),
                    format!(
                        "two {:?} surfaces share a plane and overlap by {area:.4} m² \
                                 (a at {:.2},{:.2},{:.2}; b at {:.2},{:.2},{:.2}); \
                                 overlapping rooms emit both, a doubled slab is a defect",
                        a.key.kind,
                        a.centroid[0],
                        a.centroid[1],
                        a.centroid[2],
                        b.centroid[0],
                        b.centroid[1],
                        b.centroid[2]
                    ),
                    [a, b],
                );
                continue;
            }
            if dot3(a.normal, b.normal) < -0.5 {
                // The same plane covered twice with opposite facings is
                // a reversed face *or* a legitimate back-to-back pair
                // (two stacked walls' caps at one junction); without
                // solid semantics the checker cannot tell them apart,
                // so this stays a heuristic warning.
                self.push_surfaces(
                    opposite_plane_check(a, b),
                    Severity::Warning,
                    format!("{:?} m{}", a.key.kind, a.key.material),
                    format!(
                        "two coplanar faces overlap by {area:.4} m² with opposite normals \
                                 (a m{} at {:.2},{:.2},{:.2}; b m{} at {:.2},{:.2},{:.2})",
                        a.key.material,
                        a.centroid[0],
                        a.centroid[1],
                        a.centroid[2],
                        b.key.material,
                        b.centroid[0],
                        b.centroid[1],
                        b.centroid[2]
                    ),
                    [a, b],
                );
            } else {
                self.push_surfaces(
                            "duplicate-surface",
                            Severity::Error,
                            format!("{:?} m{}", a.key.kind, a.key.material),
                            format!(
                                "coplanar surfaces overlap by {area:.4} m²: a m{} {:?} n{:?} b m{} {:?} n{:?}",
                                a.key.material, a.points, a.normal, b.key.material, b.points, b.normal
                            ),
                            [a, b],
                        );
            }
        }
    }
}

/// Quantises a float to whole millimetres as an integer key.
fn quantize(value: f32) -> i32 {
    let scaled = (value * 1000.0).round();
    if !scaled.is_finite() {
        return 0;
    }
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        reason = "Rounded coordinates are checked finite and clamped within i32 range before the integral conversion."
    )]
    let key = scaled.clamp(-2.0e9, 2.0e9) as i32;
    key
}

/// Overlap area of two triangles projected onto their shared plane.
fn triangle_overlap_area(a: &Tri, b: &Tri) -> f32 {
    let (axis, _) = dominant_axis(a.normal);
    let mut subject: Vec<[f32; 2]> = project_triangle(a.points, axis);
    let mut clip: Vec<[f32; 2]> = project_triangle(b.points, axis);
    // Both the clip intersections and the shoelace area difference products of
    // world coordinates; at a world position of a few hundred metres those
    // products' rounding dwarfs a real zero-area contact and the checker would
    // report a clean joint as an overlap. Subtracting one shared local origin
    // makes the computation translation-invariant: the same joint reports the
    // same area at the origin and at 2 km.
    let origin = subject.first().copied().unwrap_or([0.0, 0.0]);
    for point in subject.iter_mut().chain(clip.iter_mut()) {
        point[0] -= origin[0];
        point[1] -= origin[1];
    }
    if polygon_area(&subject) < 0.0 {
        subject.reverse();
    }
    if polygon_area(&clip) < 0.0 {
        clip.reverse();
    }
    let len = clip.len();
    for index in 0..len {
        let (Some(from), Some(to)) = (clip.get(index), clip.get(wrap_next(index, len))) else {
            continue;
        };
        subject = clip_polygon_left(&subject, *from, *to);
        if subject.len() < 3 {
            return 0.0;
        }
    }
    polygon_area(&subject).abs()
}

/// Projects a triangle to 2D by dropping `axis`.
fn project_triangle(points: [[f32; 3]; 3], axis: usize) -> Vec<[f32; 2]> {
    points
        .iter()
        .map(|point| match axis {
            0 => [point[1], point[2]],
            1 => [point[0], point[2]],
            _ => [point[0], point[1]],
        })
        .collect()
}

/// The next index in a cyclic polygon.
const fn wrap_next(index: usize, len: usize) -> usize {
    let next = index.saturating_add(1);
    if next >= len { 0 } else { next }
}

/// Keeps the part of `polygon` left of `from -> to`.
fn clip_polygon_left(polygon: &[[f32; 2]], from: [f32; 2], to: [f32; 2]) -> Vec<[f32; 2]> {
    let mut out = Vec::with_capacity(polygon.len().saturating_add(2));
    let edge = [to[0] - from[0], to[1] - from[1]];
    let inside = |point: [f32; 2]| {
        let relative = [point[0] - from[0], point[1] - from[1]];
        edge[0].mul_add(relative[1], -(edge[1] * relative[0])) >= -1.0e-6
    };
    let len = polygon.len();
    for index in 0..len {
        let (Some(current), Some(next)) = (polygon.get(index), polygon.get(wrap_next(index, len)))
        else {
            continue;
        };
        let current_inside = inside(*current);
        let next_inside = inside(*next);
        if current_inside {
            out.push(*current);
        }
        if current_inside != next_inside {
            let relative = [next[0] - current[0], next[1] - current[1]];
            let denominator = edge[0].mul_add(relative[1], -(edge[1] * relative[0]));
            if denominator.abs() > 1.0e-9 {
                let numerator =
                    edge[0].mul_add(from[1] - current[1], -(edge[1] * (from[0] - current[0])));
                let t = numerator / denominator;
                out.push([
                    t.mul_add(relative[0], current[0]),
                    t.mul_add(relative[1], current[1]),
                ]);
            }
        }
    }
    out
}

/// Signed area of a 2D polygon (positive counter-clockwise).
fn polygon_area(polygon: &[[f32; 2]]) -> f32 {
    let mut sum = 0.0_f32;
    let len = polygon.len();
    for index in 0..len {
        let (Some(a), Some(b)) = (polygon.get(index), polygon.get(wrap_next(index, len))) else {
            continue;
        };
        sum += a[0].mul_add(b[1], -(b[0] * a[1]));
    }
    sum * 0.5
}

/// The axis a plane's normal is most aligned with.
fn dominant_axis(normal: [f32; 3]) -> (usize, f32) {
    let mut axis = 0usize;
    let mut best = normal[0].abs();
    for (candidate, value) in normal.iter().enumerate().skip(1) {
        if value.abs() > best {
            best = value.abs();
            axis = candidate;
        }
    }
    (axis, best)
}

// ---------------------------------------------------------------------------
// Wall-plane joints
// ---------------------------------------------------------------------------

/// Wall length-face triangles bucketed by their across-plane coordinate, so
/// the emitted verification never scans every face for a joint.
struct WallTriangleIndex {
    /// `(across plane, triangle index)` for X-axis wall faces (normal ±Z).
    planes_x: Vec<(f32, usize)>,
    /// `(across plane, triangle index)` for Z-axis wall faces (normal ±X).
    planes_z: Vec<(f32, usize)>,
}

impl WallTriangleIndex {
    fn build(triangles: &[Tri]) -> Self {
        let parallel = JOINT_ANGULAR_TOL_DEG.to_radians().cos();
        let mut planes_x = Vec::new();
        let mut planes_z = Vec::new();
        for (index, triangle) in triangles.iter().enumerate() {
            if triangle.key.kind != SurfaceKind::Wall {
                continue;
            }
            if triangle.normal[2].abs() >= parallel {
                planes_x.push((triangle.points[0][2], index));
            } else if triangle.normal[0].abs() >= parallel {
                planes_z.push((triangle.points[0][0], index));
            }
        }
        planes_x.sort_by(|a, b| a.0.total_cmp(&b.0));
        planes_z.sort_by(|a, b| a.0.total_cmp(&b.0));
        Self { planes_x, planes_z }
    }

    fn planes(&self, axis: WallAxis) -> &[(f32, usize)] {
        match axis {
            WallAxis::X => &self.planes_x,
            WallAxis::Z => &self.planes_z,
        }
    }

    /// The entries whose plane falls in `[low, high]`.
    fn within(&self, axis: WallAxis, low: f32, high: f32) -> &[(f32, usize)] {
        let planes = self.planes(axis);
        let start = planes.partition_point(|(plane, _)| *plane < low);
        let end = planes.partition_point(|(plane, _)| *plane <= high);
        planes.get(start..end).unwrap_or_default()
    }
}

/// The source field a wall's across coordinate is authored as.
const fn across_field(axis: WallAxis) -> &'static str {
    match axis {
        WallAxis::X => "z",
        WallAxis::Z => "x",
    }
}

/// The axis of a serialized joint.
fn wall_axis_of(name: &str) -> WallAxis {
    if name == "z" {
        WallAxis::Z
    } else {
        WallAxis::X
    }
}

/// The two length-face names of a wall on this axis.
const fn wall_face_names(axis: WallAxis) -> [&'static str; 2] {
    match axis {
        WallAxis::X => ["north", "south"],
        WallAxis::Z => ["west", "east"],
    }
}

/// A sample 1 cm inside a wall's junction end, on the side facing `other`.
fn inside_end(span: (f32, f32), other: (f32, f32)) -> f32 {
    let own_centre = f32::midpoint(span.0, span.1);
    let other_centre = f32::midpoint(other.0, other.1);
    if other_centre >= own_centre {
        (span.1 - 0.01).max(span.0)
    } else {
        (span.0 + 0.01).min(span.1)
    }
}

/// True when a point in the `(length, y)` projection lies inside a triangle,
/// with `tolerance` metres of slack at the edges.
fn projected_point_in_triangle(
    triangle: &Tri,
    length_axis: usize,
    point: [f32; 2],
    tolerance: f32,
) -> bool {
    let project = |value: [f32; 3]| match length_axis {
        0 => [value[0], value[1]],
        _ => [value[2], value[1]],
    };
    let a = project(triangle.points[0]);
    let b = project(triangle.points[1]);
    let c = project(triangle.points[2]);
    let local = |value: [f32; 2]| [value[0] - a[0], value[1] - a[1]];
    let p = local(point);
    let local_b = local(b);
    let local_c = local(c);
    let cross = |o: [f32; 2], u: [f32; 2], v: [f32; 2]| {
        (u[0] - o[0]).mul_add(v[1] - o[1], -((u[1] - o[1]) * (v[0] - o[0])))
    };
    let ab = cross([0.0, 0.0], local_b, p);
    let bc = cross(local_b, local_c, p);
    let ca = cross(local_c, [0.0, 0.0], p);
    let edge_scale = [local_b, local_c].iter().fold(1.0_f32, |largest, value| {
        largest.max(value[0].abs().max(value[1].abs()))
    });
    let epsilon = tolerance.mul_add(edge_scale, 1.0e-6);
    (ab >= -epsilon && bc >= -epsilon && ca >= -epsilon)
        || (ab <= epsilon && bc <= epsilon && ca <= epsilon)
}

/// The human explanation of one wall joint: both indices, both plane spans,
/// the signed shift, the contact measurements and the repair safety reason.
fn wall_joint_message(joint: &WallJoint) -> String {
    let axis_label = joint.axis;
    let axis = wall_axis_of(axis_label);
    let field = across_field(axis);
    let first_span = format!(
        "walls[{}].{field} {:.3}..{:.3}",
        joint.first, joint.first_low, joint.first_high
    );
    let second_span = format!(
        "walls[{}].{field} {:.3}..{:.3}",
        joint.second, joint.second_low, joint.second_high
    );
    let evidence = match joint.authority {
        "chain" => format!(
            "coplanar chain support {} vs {}",
            joint.first_support, joint.second_support
        ),
        "length" => format!(
            "equal support ({} vs {}), longer solid span",
            joint.first_support, joint.second_support
        ),
        _ => format!(
            "ambiguous authority: equal support ({} vs {}) and equal solid span",
            joint.first_support, joint.second_support
        ),
    };
    match joint.kind {
        "step" => format!(
            "walls {} and {} meet end-to-end along {axis_label} (length gap {:.3} m, overlap {:.3} m, \
             y overlap {:.3} m): {first_span} vs {second_span} is a rigid {:+.3} m thickness-plane \
             shift; authority {evidence}; safe repair: add {:+.4} to walls[{}].{field}",
            joint.first,
            joint.second,
            joint.gap_m,
            joint.overlap_m,
            joint.y_overlap_m,
            joint.shift,
            joint.shift,
            joint.second,
        ),
        "thickness-step" => format!(
            "walls {} and {} meet end-to-end along {axis_label} (length gap {:.3} m, y overlap {:.3} m): \
             {first_span} (thickness {:.3}) vs {second_span} (thickness {:.3}) share one plane but \
             not the other; a valid thickness transition or a misplaced sliver; manual review, \
             never moved automatically",
            joint.first,
            joint.second,
            joint.gap_m,
            joint.y_overlap_m,
            joint.first_high - joint.first_low,
            joint.second_high - joint.second_low,
        ),
        _ if joint.shift.abs() <= JOINT_PLANE_TOL_M => format!(
            "walls {} and {} run coplanar along {axis_label} ({first_span}) but are separated by a \
             {:.3} m length gap no solid of either wall covers; a doorway drawn as two walls or \
             an unintended gap; manual review, never automatic",
            joint.first, joint.second, joint.gap_m,
        ),
        _ if joint.shift.abs() > JOINT_MAX_AUTO_SHIFT_M => format!(
            "walls {} and {} meet end-to-end along {axis_label} (length gap {:.3} m, y overlap {:.3} m): \
             {first_span} vs {second_span} is a rigid {:+.3} m shift, beyond the automatic limit \
             {:.3} m; would move walls[{}].{field}; manual review, never automatic",
            joint.first,
            joint.second,
            joint.gap_m,
            joint.y_overlap_m,
            joint.shift,
            JOINT_MAX_AUTO_SHIFT_M,
            joint.second,
        ),
        _ => format!(
            "walls {} and {} meet end-to-end along {axis_label} (length gap {:.3} m, y overlap {:.3} m): \
             {first_span} vs {second_span} is a rigid {:+.3} m shift with {evidence}; would move \
             walls[{}].{field}; manual review, never automatic",
            joint.first, joint.second, joint.gap_m, joint.y_overlap_m, joint.shift, joint.second,
        ),
    }
}

impl Checker<'_> {
    /// Confirmed wall-plane steps, review-only joint candidates and the
    /// emitted-mesh cross-check for every one of them.
    fn check_wall_joints(
        &mut self,
        surfaces: &LevelSurfaces<'_>,
        triangles: &[Tri],
        materials: &MaterialTable,
    ) {
        let candidates = compute_wall_joints(self.level, surfaces);
        if candidates.is_empty() {
            return;
        }
        let walls = level_wall_slices(self.level, surfaces);
        let index = WallTriangleIndex::build(triangles);
        for candidate in &candidates {
            let joint = &candidate.joint;
            let (check, severity) = match joint.kind {
                "step" => ("wall-joint-step", Severity::Error),
                "thickness-step" => ("wall-joint-thickness-step", Severity::Warning),
                _ => ("wall-joint-step-review", Severity::Warning),
            };
            self.push(
                check,
                severity,
                format!("wall {} / wall {}", joint.first, joint.second),
                wall_joint_message(joint),
                joint.position,
            );
            self.check_joint_emitted(candidate, &walls, &index, triangles, materials);
        }
    }

    /// The resolved material indices a wall's two length faces can emit.
    fn resolved_wall_materials(&self, materials: &MaterialTable, index: usize) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        let Some(wall) = self.level.walls.get(index) else {
            return out;
        };
        for name in wall_face_names(wall.axis()) {
            if let Some(reference) = wall.face_ref(name)
                && let Some(material) = materials.index_of(reference.id)
                && !out.contains(&material)
            {
                out.push(material);
            }
        }
        if let Some(material) = materials.index_of(&self.level.defaults.wall)
            && !out.contains(&material)
        {
            out.push(material);
        }
        out
    }

    /// For every joint candidate the emitted mesh must show a triangle on each
    /// declared thickness plane at 1 cm inside each junction end, and no wall
    /// face may sit on an undeclared plane near the junction.
    #[expect(
        clippy::too_many_lines,
        reason = "one joint's sampling pass, kept together"
    )] // one joint's sampling pass, kept together
    fn check_joint_emitted(
        &mut self,
        candidate: &WallJointCandidate,
        walls: &[WallSlices],
        index: &WallTriangleIndex,
        triangles: &[Tri],
        materials: &MaterialTable,
    ) {
        let joint = &candidate.joint;
        let axis = wall_axis_of(joint.axis);
        let length_axis = match axis {
            WallAxis::X => 0usize,
            WallAxis::Z => 2,
        };
        let declared = [
            joint.first_low,
            joint.first_high,
            joint.second_low,
            joint.second_high,
        ];
        let first_materials = self.resolved_wall_materials(materials, joint.first);
        let second_materials = self.resolved_wall_materials(materials, joint.second);
        let first_sample = inside_end(candidate.first_span, candidate.second_span);
        let second_sample = inside_end(candidate.second_span, candidate.first_span);
        let sampled = [
            (
                joint.first,
                first_sample,
                (joint.first_low, joint.first_high),
                &first_materials,
            ),
            (
                joint.second,
                second_sample,
                (joint.second_low, joint.second_high),
                &second_materials,
            ),
        ];
        for (wall, sample, planes, allowed) in sampled {
            for plane in <[_; 2]>::from(planes) {
                let mut missing: Option<f32> = None;
                for fraction in [0.25_f32, 0.5, 0.75] {
                    let y = candidate
                        .y
                        .0
                        .mul_add(1.0 - fraction, candidate.y.1 * fraction);
                    let point = match axis {
                        WallAxis::X => [sample, y, plane],
                        WallAxis::Z => [plane, y, sample],
                    };
                    if point_in_wall_solids(walls, point, wall) {
                        // Another authored solid legitimately covers the
                        // missing face at this sample.
                        continue;
                    }
                    let found = index
                        .within(axis, plane - JOINT_PLANE_TOL_M, plane + JOINT_PLANE_TOL_M)
                        .iter()
                        .any(|(_, slot)| {
                            let Some(triangle) = triangles.get(*slot) else {
                                return false;
                            };
                            allowed.contains(&triangle.key.material)
                                && projected_point_in_triangle(
                                    triangle,
                                    length_axis,
                                    [sample, y],
                                    0.01,
                                )
                        });
                    if !found {
                        missing = Some(y);
                        break;
                    }
                }
                if let Some(y) = missing {
                    self.push(
                        "wall-joint-emitted-mismatch",
                        Severity::Error,
                        format!("wall {wall}"),
                        format!(
                            "the emitted mesh has no wall face on the declared plane {plane:.3} at \
                             the wall {wall} junction end (sample {sample:.3}, y {y:.3}); the \
                             source decomposition and the built mesh disagree",
                        ),
                        match axis {
                            WallAxis::X => [sample, y, plane],
                            WallAxis::Z => [plane, y, sample],
                        },
                    );
                }
            }
        }

        // No emitted wall face may sit on an undeclared plane near the joint.
        let anchor = anchor_along(candidate.first_span, candidate.second_span);
        let low = declared
            .iter()
            .fold(f32::INFINITY, |value, plane| value.min(*plane))
            - JOINT_MAX_AUTO_SHIFT_M
            - JOINT_PLANE_TOL_M;
        let high = declared
            .iter()
            .fold(f32::NEG_INFINITY, |value, plane| value.max(*plane))
            + JOINT_MAX_AUTO_SHIFT_M
            + JOINT_PLANE_TOL_M;
        let mut reported: Vec<f32> = Vec::new();
        let mut allowed: Vec<u32> = first_materials.clone();
        allowed.extend(second_materials.iter().copied());
        for (plane, slot) in index.within(axis, low, high) {
            let Some(triangle) = triangles.get(*slot) else {
                continue;
            };
            if !allowed.contains(&triangle.key.material) {
                continue;
            }
            let (along_low, along_high) = triangle_projection_span(triangle, length_axis);
            let (y_low, y_high) = triangle_y_span(triangle);
            if along_high < anchor - 0.05
                || along_low > anchor + 0.05
                || y_high < candidate.y.0
                || y_low > candidate.y.1
            {
                continue;
            }
            let nearest = declared
                .iter()
                .fold(f32::INFINITY, |value, declared_plane| {
                    value.min((*plane - *declared_plane).abs())
                });
            if nearest <= JOINT_PLANE_TOL_M || nearest > JOINT_MAX_AUTO_SHIFT_M {
                continue;
            }
            let quantised = quantise(*plane);
            if reported.contains(&quantised) {
                continue;
            }
            reported.push(quantised);
            self.push(
                "wall-joint-emitted-mismatch",
                Severity::Error,
                format!("wall {} / wall {}", joint.first, joint.second),
                format!(
                    "an emitted wall face at {:.3} lies {nearest:.3} m from every declared \
                     thickness plane at the wall {} / wall {} junction; the mesh carries a step \
                     the source does not declare",
                    plane, joint.first, joint.second,
                ),
                match axis {
                    WallAxis::X => [anchor, candidate.y.0, *plane],
                    WallAxis::Z => [*plane, candidate.y.0, anchor],
                },
            );
        }
    }
}

/// The along-length span of a triangle in the joint's length axis.
fn triangle_projection_span(triangle: &Tri, length_axis: usize) -> (f32, f32) {
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    for point in triangle.points {
        let value = match length_axis {
            0 => point[0],
            _ => point[2],
        };
        low = low.min(value);
        high = high.max(value);
    }
    (low, high)
}

/// The Y span of a triangle.
fn triangle_y_span(triangle: &Tri) -> (f32, f32) {
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    for point in triangle.points {
        low = low.min(point[1]);
        high = high.max(point[1]);
    }
    (low, high)
}

/// True when a world point lies inside any authored wall's solid slice,
/// ignoring one wall index (the wall whose face is expected there).
///
/// This is the documented exception for a legitimately covered face: a
/// perpendicular return wall or a second slab sharing the footprint hides the
/// face by construction, and the emitter then omits it.
fn point_in_wall_solids(walls: &[WallSlices], point: [f32; 3], exclude: usize) -> bool {
    for wall in walls {
        if wall.index == exclude {
            continue;
        }
        let across = match wall.axis {
            WallAxis::X => point[2],
            WallAxis::Z => point[0],
        };
        if across < wall.planes.0 - JOINT_PLANE_TOL_M || across > wall.planes.1 + JOINT_PLANE_TOL_M
        {
            continue;
        }
        let along = match wall.axis {
            WallAxis::X => point[0],
            WallAxis::Z => point[2],
        };
        let covered = wall.slices.iter().any(|slice| {
            along >= wall.origin + slice.start - JOINT_PLANE_TOL_M
                && along <= wall.origin + slice.end + JOINT_PLANE_TOL_M
                && point[1] >= slice.bottom - JOINT_PLANE_TOL_M
                && point[1] <= slice.top + JOINT_PLANE_TOL_M
        });
        if covered {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Collision
// ---------------------------------------------------------------------------

impl Checker<'_> {
    /// Collider duplicates, missing engine boxes and ghost colliders.
    ///
    /// Reversed faces are found by the coplanar overlap pass (an opposing
    /// facing pair on one plane), never by proximity to a collider face: an
    /// archway's soffit or a threshold top inside a bounding box is ordinary
    /// geometry, and a box AABB cannot tell ownership.
    // one collider pass, kept together for the shared index
    fn check_colliders(
        &mut self,
        colliders: &[Collider],
        engine_boxes: &[WallAabb],
        triangles: &[Tri],
        index: &SpatialHash,
    ) {
        // Exact duplicates between two authored elements.
        let mut groups: BTreeMap<[i32; 6], Vec<usize>> = BTreeMap::new();
        for (slot, collider) in colliders.iter().enumerate() {
            groups
                .entry(aabb_key(&collider.aabb))
                .or_default()
                .push(slot);
        }
        for group in groups.values() {
            if group.len() < 2 {
                continue;
            }
            let names: Vec<String> = group
                .iter()
                .filter_map(|slot| colliders.get(*slot).map(|c| c.source.clone()))
                .collect();
            if let Some(first) = group.first().and_then(|slot| colliders.get(*slot)) {
                self.push(
                    "collision-duplicate",
                    Severity::Error,
                    names.join(" / "),
                    format!(
                        "{} authored solids share an identical collision box",
                        group.len()
                    ),
                    aabb_centre(&first.aabb),
                );
            }
        }

        // Every authored solid must exist in the engine's own collision set.
        for collider in colliders {
            if !engine_boxes
                .iter()
                .any(|engine| aabb_close(engine, &collider.aabb, 1.0e-3))
            {
                self.push(
                    "collision-mismatch",
                    Severity::Error,
                    collider.source.clone(),
                    "the engine's collision set has no box matching this authored solid",
                    aabb_centre(&collider.aabb),
                );
            }
        }

        // Ghost colliders: require real coplanar support at a face centre.
        for collider in colliders {
            if !collider.ghost_checked {
                continue;
            }
            let boxed = &collider.aabb;
            let mut covered_faces = 0usize;
            for (axis, sign) in [
                (0usize, -1.0_f32),
                (0, 1.0),
                (1, -1.0),
                (1, 1.0),
                (2, -1.0),
                (2, 1.0),
            ] {
                let centre = aabb_face_centre(boxed, axis, sign);
                let mut face_covered = false;
                if let Some(candidates) = index.near(centre) {
                    for slot in candidates {
                        let Some(triangle) = usize::try_from(*slot)
                            .ok()
                            .and_then(|triangle_slot| triangles.get(triangle_slot))
                        else {
                            continue;
                        };
                        if !triangle_on_face(triangle, boxed, axis, sign) {
                            continue;
                        }
                        if !point_in_triangle(centre, triangle) {
                            continue;
                        }
                        face_covered = true;
                        break;
                    }
                }
                if face_covered {
                    covered_faces = covered_faces.saturating_add(1);
                }
            }
            if covered_faces == 0 {
                self.push(
                    "ghost-collider",
                    Severity::Warning,
                    collider.source.clone(),
                    "no static mesh surface lies on any face of this collider",
                    aabb_centre(boxed),
                );
            }
        }
    }
}

/// True when a point lies inside a triangle, tested on the triangle's own
/// dominant projection plane.
fn point_in_triangle(point: [f32; 3], triangle: &Tri) -> bool {
    let (axis, _) = dominant_axis(triangle.normal);
    let project = |p: [f32; 3]| match axis {
        0 => [p[1], p[2]],
        1 => [p[0], p[2]],
        _ => [p[0], p[1]],
    };
    let p = project(point);
    let a = project(triangle.points[0]);
    let b = project(triangle.points[1]);
    let c = project(triangle.points[2]);
    // Work in the triangle's own frame: the edge cross products difference
    // products of world coordinates, so at a few hundred metres of world offset
    // the rounding of such terms (ulp of ~50 is ~4e-6) dwarfs an exact
    // on-edge zero. Subtracting one shared corner makes the test
    // translation-invariant and leaves only the triangle's own size in the
    // error term.
    let local = |value: [f32; 2]| [value[0] - a[0], value[1] - a[1]];
    let local_p = local(p);
    let local_origin = [0.0_f32, 0.0];
    let local_b = local(b);
    let local_c = local(c);
    // The tolerance scales with the triangle's own extent: a 20 m face's cross
    // product carries ~mlp(extent²) of rounding, and an absolute 1e-6 would
    // reject an exactly-on-edge face centre (the ghost-collider false positive
    // this fixes).
    let scale = [&local_p, &local_origin, &local_b, &local_c]
        .iter()
        .flat_map(|value| value.iter())
        .fold(1.0_f32, |largest, value| largest.max(value.abs()));
    let epsilon = (scale * scale).mul_add(1.0e-6, 1.0e-9);
    // The projected cross product is the formula, not unchecked arithmetic.

    let cross = |o: [f32; 2], u: [f32; 2], v: [f32; 2]| {
        (u[0] - o[0]).mul_add(v[1] - o[1], (u[1] - o[1]) * -(v[0] - o[0]))
    };
    let ab = cross(local_origin, local_b, local_p);
    let bc = cross(local_b, local_c, local_p);
    let ca = cross(local_c, local_origin, local_p);
    (ab >= -epsilon && bc >= -epsilon && ca >= -epsilon)
        || (ab <= epsilon && bc <= epsilon && ca <= epsilon)
}

/// True when a triangle lies on and overlaps the actual collider face.
///
/// Collider decomposition and mesh triangulation need not share partitions: a
/// supporting triangle may contain the entire face with its centroid outside
/// the box. Clip the triangle to the face instead. Positive area alone is not
/// coverage: the caller also checks the real face centre against each triangle,
/// retaining holes between disconnected fragments. Roof-following conservative
/// AABBs need only one supported face, never an invented horizontal roof cap.
fn triangle_on_face(triangle: &Tri, boxed: &WallAabb, axis: usize, sign: f32) -> bool {
    let centre = aabb_face_centre(boxed, axis, sign);
    let Some(plane) = centre.get(axis) else {
        return false;
    };
    if triangle.points.iter().any(|point| {
        point
            .get(axis)
            .is_none_or(|coordinate| (*coordinate - *plane).abs() > FACE_SAMPLE_EPS_M)
    }) {
        return false;
    }
    let (low, high) = match axis {
        0 => ([boxed.min_y, boxed.min_z], [boxed.max_y, boxed.max_z]),
        1 => ([boxed.min_x, boxed.min_z], [boxed.max_x, boxed.max_z]),
        _ => ([boxed.min_x, boxed.min_y], [boxed.max_x, boxed.max_y]),
    };
    let extent = [high[0] - low[0], high[1] - low[1]];
    let face_area = extent[0] * extent[1];
    if face_area <= 0.0 {
        return false;
    }
    let mut overlap = project_triangle(triangle.points, axis);
    for point in &mut overlap {
        point[0] -= low[0];
        point[1] -= low[1];
    }
    let rectangle = [[0.0_f32, 0.0], [extent[0], 0.0], extent, [0.0, extent[1]]];
    for (slot, from) in rectangle.iter().enumerate() {
        let Some(to) = rectangle.get(wrap_next(slot, rectangle.len())) else {
            return false;
        };
        overlap = clip_polygon_left(&overlap, *from, *to);
        if overlap.len() < 3 {
            return false;
        }
    }
    polygon_area(&overlap).abs() > (face_area * 1.0e-6).max(1.0e-9)
}

/// Exact-box key for duplicate detection, in millimetres.
fn aabb_key(aabb: &WallAabb) -> [i32; 6] {
    [
        quantize(aabb.min_x),
        quantize(aabb.min_y),
        quantize(aabb.min_z),
        quantize(aabb.max_x),
        quantize(aabb.max_y),
        quantize(aabb.max_z),
    ]
}

/// True when two boxes agree within `tolerance`.
fn aabb_close(a: &WallAabb, b: &WallAabb, tolerance: f32) -> bool {
    (a.min_x - b.min_x).abs() <= tolerance
        && (a.min_y - b.min_y).abs() <= tolerance
        && (a.min_z - b.min_z).abs() <= tolerance
        && (a.max_x - b.max_x).abs() <= tolerance
        && (a.max_y - b.max_y).abs() <= tolerance
        && (a.max_z - b.max_z).abs() <= tolerance
}

/// The centre of a box, for reporting.
const fn aabb_centre(aabb: &WallAabb) -> [f32; 3] {
    [
        f32::midpoint(aabb.min_x, aabb.max_x),
        f32::midpoint(aabb.min_y, aabb.max_y),
        f32::midpoint(aabb.min_z, aabb.max_z),
    ]
}

/// The centre of one box face.
fn aabb_face_centre(aabb: &WallAabb, axis: usize, sign: f32) -> [f32; 3] {
    let mut centre = aabb_centre(aabb);
    let value = match axis {
        0 => {
            if sign < 0.0 {
                aabb.min_x
            } else {
                aabb.max_x
            }
        }
        1 => {
            if sign < 0.0 {
                aabb.min_y
            } else {
                aabb.max_y
            }
        }
        _ => {
            if sign < 0.0 {
                aabb.min_z
            } else {
                aabb.max_z
            }
        }
    };
    if let Some(slot) = centre.get_mut(axis) {
        *slot = value;
    }
    centre
}

// ---------------------------------------------------------------------------
// Prop placements
// ---------------------------------------------------------------------------

/// Eligibility for a static-placement identity heuristic.
///
/// Runtime-owned poses cannot be compared through their saved anchors. Balloon
/// components supply offsets/scales/strings; AI, animation, routes, physics and
/// floats own independent live poses. Other components, bindings, light owners
/// and explicitly targeted instances can also have distinct runtime behavior.
/// Without evaluating that behavior, exclude them instead of asserting that a
/// placement is redundant. Catalogued entities have intrinsic posed resources.
/// Ordinary component-free immutable props and fallback boxes remain checked.
fn static_duplicate_candidate(
    prop: &crate::engine::level::PropDef,
    id: &str,
    controlled: &BTreeSet<String>,
    catalog: &crate::engine::loader::PropCatalog,
) -> bool {
    prop.float.is_none()
        && !prop.is_physics_movable()
        && !prop.shadow_only
        && prop.components.is_empty()
        && prop.bindings.is_empty()
        && prop.lights.is_empty()
        && prop.switch_group.is_none()
        && !controlled.contains(id)
        && !catalog
            .assets()
            .placeable(&prop.model)
            .is_some_and(|entry| {
                entry.asset_type.as_str() == crate::engine::assets::AssetType::ENTITY
            })
}

/// Props are not part of the emitted architecture mesh, so coincident
/// placements and large dressing layers stacked a few millimetres off a
/// walkable surface are invisible to the mesh checks. These heuristics cover
/// the two recurring classes: an exact duplicate placement, and a slab whose
/// visible top lands within 2 cm of the walkable floor at its own footprint.
impl Checker<'_> {
    fn check_duplicate_props(&mut self, catalog: &crate::engine::loader::PropCatalog) {
        let props = &self.level.props;
        let identities = self.level.prop_instance_ids();
        let mut controlled: BTreeSet<String> =
            crate::engine::interact::referenced_targets(self.level)
                .into_iter()
                .collect();
        controlled.extend(
            self.level
                .routes
                .iter()
                .map(|route| route.id.trim().to_string()),
        );
        let mut by_model: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (index, prop) in props.iter().enumerate() {
            if identities
                .get(index)
                .is_some_and(|id| static_duplicate_candidate(prop, id, &controlled, catalog))
            {
                by_model.entry(prop.model.as_str()).or_default().push(index);
            }
        }
        for indices in by_model.values() {
            for (slot, first) in indices.iter().enumerate() {
                let Some(a) = props.get(*first) else {
                    continue;
                };
                for second in indices.iter().skip(slot.saturating_add(1)) {
                    let Some(b) = props.get(*second) else {
                        continue;
                    };
                    // The tolerances are the identity test itself.

                    let same = (a.x - b.x).abs() <= 1.0e-3
                        && (a.z - b.z).abs() <= 1.0e-3
                        && (a.y - b.y).abs() <= 1.0e-3
                        && (a.rotation_degrees - b.rotation_degrees).abs() <= 0.5
                        && (a.pitch_degrees - b.pitch_degrees).abs() <= 0.5
                        && (a.roll_degrees - b.roll_degrees).abs() <= 0.5
                        && (a.scale - b.scale).abs() <= 1.0e-3
                        && a.span == b.span
                        && a.size == b.size
                        && a.character_texture == b.character_texture
                        && a.material_overrides == b.material_overrides
                        && a.emission_overrides == b.emission_overrides;
                    if same {
                        let a_name = a.id.as_deref().unwrap_or(&a.model);
                        let b_name = b.id.as_deref().unwrap_or(&b.model);
                        self.push(
                            "prop-duplicate",
                            Severity::Warning,
                            a_name,
                            format!(
                                "'{b_name}' repeats static '{a_name}' ({}) with the same authored transform and appearance; \
                                 the overlapping geometry can z-fight",
                                a.model
                            ),
                            [a.x, a.y, a.z],
                        );
                    }
                }
            }
        }
    }

    fn check_props(
        &mut self,
        catalog: &crate::engine::loader::PropCatalog,
        surfaces: &LevelSurfaces<'_>,
    ) {
        self.check_duplicate_props(catalog);
        let props = &self.level.props;
        for prop in props {
            let entry = catalog.get(&prop.model);
            if entry.model.is_none() {
                continue;
            }
            let Some(floor) = surfaces.floor_y_at(prop.x, prop.z) else {
                continue;
            };
            // An authored collision size is the author's declared extent; with
            // none, the catalog's own model size stands in.
            let [width, height, depth] = prop.size.unwrap_or(entry.size);
            let scale = prop.scale;
            let base = floor + prop.y;
            let top = height.mul_add(scale, base);
            // Only visible dressing slabs matter: a tiny fixture laid on the
            // floor cannot hide a floor's worth of flicker.
            if width * scale * depth * scale < 0.25 {
                continue;
            }

            let layered = (top - floor).abs() < 0.02 && (base - floor).abs() > 0.02;
            if layered {
                self.push(
                    "prop-layer-coplanar",
                    Severity::Warning,
                    prop.id.as_deref().unwrap_or(&prop.model),
                    format!(
                        "'{}' top sits {:.1} mm from the walkable floor at ({:.1}, {:.1}); \
                         a dressing layer this close z-fights with the real surface",
                        prop.model,
                        (top - floor).abs() * 1000.0,
                        prop.x,
                        prop.z
                    ),
                    [prop.x, top, prop.z],
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Openings
// ---------------------------------------------------------------------------

impl Checker<'_> {
    /// Overlapping opening pairs and openings that miss the wall solid.
    fn check_openings(&mut self, surfaces: &LevelSurfaces<'_>) {
        for (index, wall) in self.level.walls.iter().enumerate() {
            let (wall_base, wall_top) = wall_extent_pair(wall, surfaces);
            let length = wall.length();
            for (first, opening) in wall.openings.iter().enumerate() {
                let start = opening.offset;
                let end = opening.end();
                let bottom = opening.bottom(wall_base);
                let top = opening.top(wall_base);
                if end <= start || top <= bottom {
                    continue;
                }
                let outside = end < -1.0e-3
                    || start > length + 1.0e-3
                    || top < wall_base - 1.0e-3
                    || bottom > wall_top + 1.0e-3;
                if outside {
                    self.push(
                        "opening-unused",
                        Severity::Warning,
                        format!("wall {index} opening {first}"),
                        "the opening does not intersect the wall solid; it cuts nothing",
                        wall_point(wall, f32::midpoint(start, end), wall_base),
                    );
                    continue;
                }
                for (second, other) in wall
                    .openings
                    .iter()
                    .enumerate()
                    .skip(first.saturating_add(1))
                {
                    let other_start = other.offset;
                    let other_end = other.end();
                    let overlaps_length = end > other_start + 1.0e-3 && start < other_end - 1.0e-3;
                    let other_bottom = other.bottom(wall_base);
                    let other_top = other.top(wall_base);
                    let overlaps_vertically =
                        top > other_bottom + 1.0e-3 && bottom < other_top - 1.0e-3;
                    if overlaps_length && overlaps_vertically {
                        self.push(
                            "opening-overlap",
                            Severity::Error,
                            format!("wall {index} openings {first}/{second}"),
                            "two openings overlap; the wall resolves them as one merged hole",
                            wall_point(
                                wall,
                                f32::midpoint(start.max(other_start), end.min(other_end)),
                                bottom,
                            ),
                        );
                    }
                }
            }
        }
    }
}

/// A wall's base and top, mirroring the engine's conservative vertical extent.
fn wall_extent_pair(wall: &WallDef, surfaces: &LevelSurfaces<'_>) -> (f32, f32) {
    let length = wall.length();
    let breaks = surfaces.wall_profile_breaks(wall);
    let mut base = wall.y;
    let mut top = f32::NEG_INFINITY;
    let probes = std::iter::once(0.0)
        .chain(std::iter::once(length))
        .chain(breaks.iter().copied());
    for at in probes {
        let clear = wall
            .height
            .unwrap_or_else(|| surfaces.clear_ceiling_height_along(wall, at));
        let candidate = wall.y + clear;
        base = base.min(candidate);
        top = top.max(candidate);
    }
    (base, top)
}

/// A world point on the wall's centre plane.
fn wall_point(wall: &WallDef, along: f32, y: f32) -> [f32; 3] {
    let (x, z) = crate::engine::level::wall_point(wall, along);
    [x, y, z]
}

// ---------------------------------------------------------------------------
// Curved primitives
// ---------------------------------------------------------------------------

impl Checker<'_> {
    /// Invalid, coarse or collision-uncovered arc walls and circular pillars.
    fn check_curves(&mut self, surfaces: &LevelSurfaces<'_>) {
        for (index, piece) in self.level.arc_walls.iter().enumerate() {
            self.check_arc_wall(index, piece, surfaces);
        }
        for (index, piece) in self.level.pillars.iter().enumerate() {
            self.check_pillar(index, piece, surfaces);
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one curved primitive's checks, kept together"
    )] // one curved primitive's checks, kept together
    fn check_arc_wall(&mut self, index: usize, piece: &ArcWallDef, surfaces: &LevelSurfaces<'_>) {
        let label = format!("arc wall {index}");
        let centre = [piece.x, piece.base_y(surfaces), piece.z];
        if !piece.x.is_finite()
            || !piece.z.is_finite()
            || !piece.radius.is_finite()
            || !piece.thickness.is_finite()
        {
            self.push(
                "curved-invalid",
                Severity::Error,
                label,
                "centre, radius and thickness must be finite numbers",
                centre,
            );
            return;
        }
        if piece.radius <= 0.0 || piece.thickness <= 0.0 || piece.inner_radius() <= 0.0 {
            self.push(
                "curved-invalid",
                Severity::Error,
                label,
                format!(
                    "thickness {} must be positive and thinner than twice the radius {}",
                    piece.thickness, piece.radius
                ),
                centre,
            );
            return;
        }
        if !piece.sweep_degrees.is_finite()
            || piece.sweep_degrees.abs() <= 1.0e-3
            || piece.sweep_degrees.abs() > 360.0 + 1.0e-3
        {
            self.push(
                "curved-invalid",
                Severity::Error,
                label,
                format!(
                    "sweep must be a non-zero angle up to 360 degrees, found {}",
                    piece.sweep_degrees
                ),
                centre,
            );
            return;
        }
        let segments = piece.resolved_segments();
        let segment_angle = piece.sweep_degrees.abs().to_radians()
            / f32::from(u16::try_from(segments).unwrap_or(u16::MAX));
        let sagitta = piece.outer_radius() * (1.0 - (segment_angle * 0.5).cos());
        let coarse = sagitta > CURVE_COARSE_SAGITTA_M;
        if coarse {
            self.push(
                "curve-coarse",
                Severity::Warning,
                label.clone(),
                format!(
                    "{segments} segments leave a {:.3} m sagitta on a {:.2} m outer radius; raise `segments`",
                    sagitta,
                    piece.outer_radius()
                ),
                centre,
            );
        }
        let boxes: Vec<ArchitectureBox> = piece.collision_boxes(surfaces);
        if boxes.is_empty() {
            self.push(
                "curved-invalid",
                Severity::Error,
                label,
                "the primitive resolved to no collision geometry",
                centre,
            );
            return;
        }
        let base = piece.base_y(surfaces);
        let top = piece.top_y_at(surfaces, piece.x, piece.z);
        let probe_y = if top > base {
            f32::midpoint(base, top)
        } else {
            base + 0.5
        };
        for segment in 0..segments {
            let count = f32::from(u16::try_from(segments).unwrap_or(u16::MAX));
            let fraction = (f32::from(u16::try_from(segment).unwrap_or(u16::MAX)) + 0.5) / count;
            let angle = piece.sweep_degrees.mul_add(fraction, piece.start_degrees);
            for radius in [piece.inner_radius(), piece.radius, piece.outer_radius()] {
                let (x, z) = crate::engine::level::round_point(piece.x, piece.z, radius, angle);
                if !covered_by_boxes(&boxes, x, probe_y, z) {
                    self.push(
                        "curved-collision-gap",
                        Severity::Error,
                        label.clone(),
                        format!(
                            "collision does not cover the drawn ring at segment {segment} (radius {radius:.2})"
                        ),
                        [x, probe_y, z],
                    );
                    break;
                }
            }
        }
        // A coarse tessellation already gets the `curve-coarse` warning, and
        // its row-AABB slack is the direct consequence of that tessellation;
        // only a fine curve is checked for grossly oversized collision.
        for boxed in &boxes {
            if coarse {
                break;
            }
            let tolerance = (0.05_f32).max(piece.outer_radius() * 0.05);
            let limit = piece.outer_radius() + tolerance;
            for corner in footprint_corners(boxed) {
                let distance = (corner[0] - piece.x).hypot(corner[1] - piece.z);
                if distance > limit {
                    self.push(
                        "curved-collision-overshoot",
                        Severity::Error,
                        label.clone(),
                        format!(
                            "a collision box corner reaches {distance:.3} m from the axis, past the {:.2} m outer radius",
                            piece.outer_radius()
                        ),
                        [corner[0], probe_y, corner[1]],
                    );
                    break;
                }
            }
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one curved primitive's checks, kept together"
    )] // one curved primitive's checks, kept together
    fn check_pillar(&mut self, index: usize, piece: &PillarDef, surfaces: &LevelSurfaces<'_>) {
        let label = format!("pillar {index}");
        let centre = [piece.x, piece.base_y(surfaces), piece.z];
        if !piece.x.is_finite() || !piece.z.is_finite() || !piece.radius.is_finite() {
            self.push(
                "curved-invalid",
                Severity::Error,
                label,
                "centre and radius must be finite numbers",
                centre,
            );
            return;
        }
        if piece.radius <= 0.0 {
            self.push(
                "curved-invalid",
                Severity::Error,
                label,
                format!("radius must be positive, found {}", piece.radius),
                centre,
            );
            return;
        }
        let segments = piece.resolved_segments();
        let half = std::f32::consts::PI / f32::from(u16::try_from(segments).unwrap_or(u16::MAX));
        let sagitta = piece.radius * (1.0 - half.cos());
        let coarse = sagitta > CURVE_COARSE_SAGITTA_M;
        if coarse {
            self.push(
                "curve-coarse",
                Severity::Warning,
                label.clone(),
                format!(
                    "{segments} segments leave a {:.3} m sagitta on a {:.2} m radius; raise `segments`",
                    sagitta, piece.radius
                ),
                centre,
            );
        }
        let boxes = piece.collision_boxes(surfaces);
        if boxes.is_empty() {
            self.push(
                "curved-invalid",
                Severity::Error,
                label,
                "the primitive resolved to no collision geometry",
                centre,
            );
            return;
        }
        let base = piece.base_y(surfaces);
        let top = piece.top_y(surfaces);
        let probe_y = if top > base {
            f32::midpoint(base, top)
        } else {
            base + 0.5
        };
        let points = piece.polygon_points();
        if !covered_by_boxes(&boxes, piece.x, probe_y, piece.z) {
            self.push(
                "curved-collision-gap",
                Severity::Error,
                label.clone(),
                "collision does not cover the pillar's centre",
                centre,
            );
        }
        let len = points.len();
        for slot in 0..len {
            let (Some(a), Some(b)) = (points.get(slot), points.get(wrap_next(slot, len))) else {
                continue;
            };
            let mid = (f32::midpoint(a.0, b.0), f32::midpoint(a.1, b.1));
            for probe in [*a, *b, mid] {
                if !covered_by_boxes(&boxes, probe.0, probe_y, probe.1) {
                    self.push(
                        "curved-collision-gap",
                        Severity::Error,
                        label,
                        format!(
                            "collision does not cover the drawn polygon near ({:.2}, {:.2})",
                            probe.0, probe.1
                        ),
                        [probe.0, probe_y, probe.1],
                    );
                    return;
                }
            }
        }
        // See the arc-wall note: a coarse tessellation explains its own slack.
        let limit = piece.radius + (0.05_f32).max(piece.radius * 0.12);
        for boxed in &boxes {
            if coarse {
                break;
            }
            for corner in footprint_corners(boxed) {
                let distance = (corner[0] - piece.x).hypot(corner[1] - piece.z);
                if distance > limit {
                    self.push(
                        "curved-collision-overshoot",
                        Severity::Error,
                        label,
                        format!(
                            "a collision box corner reaches {distance:.3} m from the centre, past the {:.2} m radius",
                            piece.radius
                        ),
                        [corner[0], probe_y, corner[1]],
                    );
                    return;
                }
            }
        }
    }
}

/// The four plan corners of an architecture box.
const fn footprint_corners(boxed: &ArchitectureBox) -> [[f32; 2]; 4] {
    [
        [boxed.min[0], boxed.min[2]],
        [boxed.max[0], boxed.min[2]],
        [boxed.max[0], boxed.max[2]],
        [boxed.min[0], boxed.max[2]],
    ]
}

/// True when a point lies inside any architecture box.
fn covered_by_boxes(boxes: &[ArchitectureBox], x: f32, y: f32, z: f32) -> bool {
    boxes.iter().any(|boxed| {
        x >= boxed.min[0] - 1.0e-4
            && x <= boxed.max[0] + 1.0e-4
            && z >= boxed.min[2] - 1.0e-4
            && z <= boxed.max[2] + 1.0e-4
            && y >= boxed.min[1] - 1.0e-4
            && y <= boxed.max[1] + 1.0e-4
    })
}

// ---------------------------------------------------------------------------
// Rooms: leaks, perimeter coverage and spawn
// ---------------------------------------------------------------------------

impl Checker<'_> {
    /// Suspicious void leaks and wall runs with no wall or opening.
    fn check_rooms(&mut self, surfaces: &LevelSurfaces<'_>, colliders: &[Collider]) {
        let boxes: Vec<WallAabb> = colliders.iter().map(|collider| collider.aabb).collect();
        let floor = WalkableFloor::from_level(self.level);
        let doors: Vec<_> = self
            .level
            .doors
            .iter()
            .map(|door| crate::engine::door::DoorRuntime::new(door, self.level).collider())
            .collect();
        for (index, room) in self.level.room_iter().enumerate() {
            if !room.width.is_finite() || !room.depth.is_finite() {
                continue;
            }
            self.check_room_leak(index, room, &floor, &boxes, &doors);
            self.check_room_perimeter(index, room, surfaces, &boxes, &doors);
        }
    }

    fn check_room_leak(
        &mut self,
        index: usize,
        room: &RoomDef,
        floor: &WalkableFloor,
        boxes: &[WallAabb],
        doors: &[DoorCollider],
    ) {
        let (x0, x1, z0, z1) = room.bounds();
        let ex0 = x0 - LEAK_MARGIN_M;
        let ex1 = x1 + LEAK_MARGIN_M;
        let ez0 = z0 - LEAK_MARGIN_M;
        let ez1 = z1 + LEAK_MARGIN_M;
        let mut cell = LEAK_CELL_M;
        // Bound the grid: coarsen until the cell budget fits.
        while cell < 4.0 && grid_cells(ex0, ex1, ez0, ez1, cell) > 250_000 {
            cell *= 2.0;
        }
        let cells_x = span_cells(ex0, ex1, cell);
        let cells_z = span_cells(ez0, ez1, cell);
        let point = |ix: usize, iz: usize| -> (f32, f32) {
            (
                (usize_to_f32(ix) + 0.5).mul_add(cell, ex0),
                (usize_to_f32(iz) + 0.5).mul_add(cell, ez0),
            )
        };
        // Seeds: every cell inside the room's footprint.
        let mut visited = vec![false; cells_x.saturating_mul(cells_z)];
        let mut queue: VecDeque<(usize, usize, f32)> = VecDeque::new();
        for ix in 0..cells_x {
            for iz in 0..cells_z {
                let (x, z) = point(ix, iz);
                if let Some(seed_foot) = floor.height_in_room(index, x, z) {
                    let slot = iz.saturating_mul(cells_x).saturating_add(ix);
                    if let Some(entry) = visited.get_mut(slot)
                        && !*entry
                    {
                        *entry = true;
                        queue.push_back((ix, iz, seed_foot));
                    }
                }
            }
        }
        let mut exit: Option<(f32, f32, f32, f32)> = None;
        // Each cell resolves its own walkable floor: a room that contains a
        // deep basin is blocked at the basin's level by the basin's own rims,
        // and a deck-level wall must not be treated as passable just because
        // the room centre happens to be lower.
        let blocked = |x: f32, z: f32, cell_foot: f32| {
            // Outside every room there is no floor; the step keeps the foot it
            // came from, so a wall at the room edge still blocks an escape
            // instead of becoming passable because the void resolves to no
            // walkable surface.
            boxes.iter().any(|boxed| {
                boxed.overlaps_disc(x, z, LEAK_BODY_RADIUS_M)
                    && boxed.blocks_body(cell_foot, LEAK_BODY_HEIGHT_M)
            }) || doors.iter().any(|door| {
                door.overlaps_body_y(cell_foot, LEAK_BODY_HEIGHT_M)
                    && door.depenetrate(x, z, LEAK_BODY_RADIUS_M).is_some()
            })
        };
        while let Some((ix, iz, current_foot)) = queue.pop_front() {
            for (dx, dz) in [
                (1i32, 0i32),
                (-1_i32, 0_i32),
                (0_i32, 1_i32),
                (0_i32, -1_i32),
            ] {
                let Some(nx) = step_index(ix, dx) else {
                    continue;
                };
                let Some(nz) = step_index(iz, dz) else {
                    continue;
                };
                if nx >= cells_x || nz >= cells_z {
                    continue;
                }
                let slot = nz.saturating_mul(cells_x).saturating_add(nx);
                let Some(entry) = visited.get_mut(slot) else {
                    continue;
                };
                if *entry {
                    continue;
                }
                let (px, pz) = point(nx, nz);
                let landing = floor.height_below(px, pz, current_foot + PLAYER_STEP_HEIGHT);
                let next_foot = landing.unwrap_or(current_foot);
                if blocked(px, pz, next_foot) {
                    continue;
                }
                *entry = true;
                queue.push_back((nx, nz, next_foot));
                // Outside the room and outside every room: the void.
                if !room.contains(px, pz) && landing.is_none() {
                    let penetration = (x0 - px).max(px - x1).max(z0 - pz).max(pz - z1);
                    let candidate = (px, next_foot, pz, penetration);
                    if exit.is_none_or(|(_, _, _, depth)| penetration > depth) {
                        exit = Some(candidate);
                    }
                }
            }
        }
        if let Some((x, foot, z, depth)) = exit {
            self.push(
                "room-leak",
                Severity::Warning,
                format!("room {index}"),
                format!(
                    "the room's walkable space reaches the void {depth:.2} m outside its footprint; \
                     no authored-pose barrier or supporting storey explains it. Doors use their starting pose; opening a leaf can expose a route. The bounded flood samples one reachable height per cell, not every motion or storey transition"
                ),
                [x, foot, z],
            );
        }
    }

    fn check_room_perimeter(
        &mut self,
        index: usize,
        room: &RoomDef,
        surfaces: &LevelSurfaces<'_>,
        boxes: &[WallAabb],
        doors: &[DoorCollider],
    ) {
        let (x0, x1, z0, z1) = room.bounds();
        if x1 - x0 <= 0.0 || z1 - z0 <= 0.0 {
            return;
        }
        // The perimeter is sampled at the room's own floor plane, which is the
        // plane walls and openings are authored against; a recess deeper in
        // the room does not move its walls.
        let foot = if room.floor_y.is_finite() {
            room.floor_y
        } else {
            0.0
        };
        let probe_y = foot + 0.5;
        let mut runs: Vec<([f32; 3], [f32; 3], f32)> = Vec::new();
        for (ex0, ez0, ex1, ez1) in [
            (x0, z0, x1, z0),
            (x0, z1, x1, z1),
            (x0, z0, x0, z1),
            (x1, z0, x1, z1),
        ] {
            let length = (ex1 - ex0).abs().max((ez1 - ez0).abs());
            let steps = span_cells(0.0, length, PERIMETER_STEP_M).max(1);
            let mut open_run: Option<([f32; 3], [f32; 3], f32)> = None;
            for step in 0..=steps {
                let fraction = usize_to_f32(step) / usize_to_f32(steps);
                let x = (ex1 - ex0).mul_add(fraction, ex0);
                let z = (ez1 - ez0).mul_add(fraction, ez0);
                let covered = boxes.iter().any(|boxed| {
                    boxed.overlaps_disc(x, z, 0.05) && boxed.blocks_body(foot, LEAK_BODY_HEIGHT_M)
                }) || doors.iter().any(|door| {
                    door.overlaps_body_y(foot, LEAK_BODY_HEIGHT_M)
                        && door.depenetrate(x, z, 0.05).is_some()
                }) || opening_covers(self.level, surfaces, x, z, probe_y);
                let piece = length / usize_to_f32(steps);
                if covered {
                    if let Some((start, end, run_length)) = open_run.take()
                        && run_length >= MISSING_RUN_M
                    {
                        runs.push((start, end, run_length));
                    }
                    continue;
                }
                match open_run.as_mut() {
                    Some((_, end, run_length)) => {
                        *end = [x, probe_y, z];
                        *run_length += piece;
                    }
                    None => {
                        open_run = Some(([x, probe_y, z], [x, probe_y, z], piece));
                    }
                }
            }
            if let Some((start, end, run_length)) = open_run
                && run_length >= MISSING_RUN_M
            {
                runs.push((start, end, run_length));
            }
        }
        for (start, end, length) in runs {
            let position = [
                f32::midpoint(start[0], end[0]),
                probe_y,
                f32::midpoint(start[2], end[2]),
            ];
            self.push(
                "missing-wall",
                Severity::Warning,
                format!("room {index}"),
                format!("a {length:.2} m perimeter run has no wall solid and no authored opening"),
                position,
            );
        }
    }

    /// The spawn point should be inside a room, or the player boots over the
    /// void at the fallback floor.
    fn check_spawn(&mut self, surfaces: &LevelSurfaces<'_>) {
        let spawn = &self.level.spawn;
        if !spawn.x.is_finite() || !spawn.z.is_finite() {
            self.push(
                "spawn-invalid",
                Severity::Error,
                "spawn",
                "spawn coordinates must be finite",
                [spawn.x, 0.0, spawn.z],
            );
            return;
        }
        if surfaces.room_at(spawn.x, spawn.z).is_none() {
            self.push(
                "spawn-outside-room",
                Severity::Warning,
                "spawn",
                "the spawn point is outside every room; the walkable floor falls back to y = 0",
                [spawn.x, 0.0, spawn.z],
            );
        }
    }
}

/// Number of cells a span resolves to.
fn span_cells(low: f32, high: f32, cell: f32) -> usize {
    let value = ((high - low) / cell).ceil();
    if !value.is_finite() || value <= 0.0 {
        return 1;
    }
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "The finite integral geometry sample count is clamped to 1..=1000000 before converting to u64."
    )]
    // Bounded below by 1 and above by the caller's own cell budget.
    let cells = value.clamp(1.0, 1.0e6) as u64;
    usize::try_from(cells).unwrap_or(1)
}

/// Cell count of a 2D grid over a span pair.
fn grid_cells(x0: f32, x1: f32, z0: f32, z1: f32, cell: f32) -> usize {
    span_cells(x0, x1, cell).saturating_mul(span_cells(z0, z1, cell))
}

/// `index + step` as a `usize`, or `None` when it would go negative.
fn step_index(index: usize, step: i32) -> Option<usize> {
    let value = i64::try_from(index).unwrap_or(0);
    let stepped = value.checked_add(i64::from(step))?;
    usize::try_from(stepped).ok()
}

/// True when a point at `y` passes through some wall opening or archway.
fn opening_covers(level: &LevelDef, surfaces: &LevelSurfaces<'_>, x: f32, z: f32, y: f32) -> bool {
    for wall in &level.walls {
        let (origin_x, origin_z) = wall.length_origin();
        let (along, across) = match wall.axis() {
            WallAxis::X => (x - origin_x, z),
            WallAxis::Z => (z - origin_z, x),
        };
        let (t0, t1) = wall_t0_t1(wall);
        if across < t0 - 0.3 || across > t1 + 0.3 {
            continue;
        }
        let local_y = y - wall.y;
        for opening in &wall.openings {
            if along >= opening.offset - 0.05
                && along <= opening.end() + 0.05
                && local_y >= opening.sill - 0.05
                && local_y <= opening.sill + opening.height + 0.05
            {
                return true;
            }
        }
    }
    for archway in &level.archways {
        let (x0, x1, z0, z1) = archway.bounds();
        if x < x0 - 0.1 || x > x1 + 0.1 || z < z0 - 0.1 || z > z1 + 0.1 {
            continue;
        }
        let along = match archway.axis() {
            WallAxis::X => x - x0,
            WallAxis::Z => z - z0,
        };
        let (open_start, open_end) = archway.opening_span();
        let base = archway.base_y(surfaces);
        let height = archway.arch_height_at(along);
        if along >= open_start - 0.05 && along <= open_end + 0.05 && y <= base + height + 0.05 {
            return true;
        }
    }
    false
}

/// A wall's two across-thickness planes.
fn wall_t0_t1(wall: &WallDef) -> (f32, f32) {
    let (min_x, max_x) = (
        wall.x.min(wall.x + wall.width),
        wall.x.max(wall.x + wall.width),
    );
    let (min_z, max_z) = (
        wall.z.min(wall.z + wall.depth),
        wall.z.max(wall.z + wall.depth),
    );
    match wall.axis() {
        WallAxis::X => (min_z, max_z),
        WallAxis::Z => (min_x, max_x),
    }
}

// ---------------------------------------------------------------------------
// Small vector helpers
// ---------------------------------------------------------------------------

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1].mul_add(b[2], -(a[2] * b[1])),
        a[2].mul_add(b[0], -(a[0] * b[2])),
        a[0].mul_add(b[1], -(a[1] * b[0])),
    ]
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[2].mul_add(b[2], a[1].mul_add(b[1], a[0] * b[0]))
}

fn length3(value: [f32; 3]) -> f32 {
    dot3(value, value).sqrt()
}

fn normalized3(value: [f32; 3]) -> [f32; 3] {
    let length = length3(value);
    if !length.is_finite() || length <= 1.0e-12 {
        return [0.0, 0.0, 1.0];
    }
    [value[0] / length, value[1] / length, value[2] / length]
}

fn distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = sub3(a, b);
    dot3(d, d)
}

const fn usize_to_f32(value: usize) -> f32 {
    #[expect(
        clippy::as_conversions,
        clippy::cast_precision_loss,
        reason = "bounded grid indices"
    )] // bounded grid indices
    let converted = value as f32;
    converted
}

/// Prints a usage error and ends the process with the documented status `2`.
///
/// Kept with the checker's process-exit path so the narrow `exit` allow stays
/// in one place.
#[expect(
    clippy::exit,
    clippy::print_stderr,
    reason = "The headless CLI reports usage on stderr and exits with its command-line failure code."
)]
pub fn exit_usage(error: &str) -> ! {
    eprintln!("geometry check: {error}");
    std::process::exit(2);
}

/// Rewrites `--check-geometry` runs as the process exit point.
///
/// # Errors
///
/// Returns the usage or IO error, reported as status `2`.
// `std::process::exit` is the correct way for a CLI mode to end the process
// from inside `main`; this one narrow allow keeps the crate-wide `exit` lint
// for every other path.
#[expect(
    clippy::print_stderr,
    reason = "`std::process::exit` is the correct way for a CLI mode to end the process from inside `main`; this one narrow allow keeps the crate-wide `exit` lint for every other path."
)]
pub fn main(options: &CliOptions) -> Result<(), Box<dyn std::error::Error>> {
    match run(options) {
        Ok(status) => std::process::exit(status),
        Err(error) => {
            eprintln!("geometry check: {error}");
            std::process::exit(2);
        }
    }
}

// ---------------------------------------------------------------------------
// Repair planner (`--repair-geometry`)
// ---------------------------------------------------------------------------

/// Hex SHA-256 of the source bytes, the plan's concurrent-change guard.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    crate::engine::package::hash::digest_hex(&hasher.finalize())
}

mod repair;

pub use repair::{
    REPAIR_FORMAT, REPAIR_VERSION, RepairCliOptions, repair_main, repair_options_from_args,
    run_repair,
};

/// Runs the headless geometry CLI modes, which must precede any SDL or wgpu
/// bootstrap: `--check-geometry` and `--repair-geometry` exit the process
/// themselves, and a malformed option exits with usage.
///
/// # Errors
/// Returns a geometry-check error without creating a window.
pub fn dispatch_cli(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    match options_from_args(args) {
        Ok(Some(options)) => return main(&options),
        Ok(None) => {}
        Err(error) => exit_usage(&error),
    }
    match repair_options_from_args(args) {
        Ok(Some(options)) => return repair_main(&options),
        Ok(None) => {}
        Err(error) => exit_usage(&error),
    }
    Ok(())
}

const fn opposite_plane_check(a: &Tri, b: &Tri) -> &'static str {
    if matches!(
        (a.key.kind, b.key.kind),
        (SurfaceKind::Floor, SurfaceKind::Ceiling) | (SurfaceKind::Ceiling, SurfaceKind::Floor)
    ) {
        "shared-storey-plane"
    } else {
        "reversed-face"
    }
}
