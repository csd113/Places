//! The map geometry checker: a read-only, deterministic validator for level
//! geometry.
//!
//! Run it from the game binary:
//!
//! ```text
//! places --check-geometry --level assets/levels/places_demo.json
//! places --check-geometry --level levels/level0_pit.json --json report.json
//! places --check-geometry --level places_demo --markers-obj markers.obj
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
//! [`crate::level::GeometryIntentDef`] annotation in `geometry_intent`; errors
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

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fmt::Write as _;
use std::path::PathBuf;

use serde::Serialize;

use crate::collision::{CROUCH_HEIGHT, WallAabb};
use crate::level::{
    ArcWallDef, ArchitectureBox, LevelDef, LevelSurfaces, PillarDef, RoomDef, WallAxis, WallDef,
    wall_solid_slices_profiled,
};
use crate::materials::MaterialTable;
use crate::render::{LevelMesh, SurfaceKey, SurfaceKind, Vertex};

/// Stable format id written into the machine-readable report.
pub const CHECK_FORMAT: &str = "places-geometry-check";
/// Report schema version; bumped when a field changes meaning.
pub const CHECK_VERSION: u32 = 1;

/// Tolerance within which two planes are "the same plane", in metres.
const PLANE_EPS_M: f32 = 1.0e-3;
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
    slices: Vec<crate::level::WallSlice>,
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
    fn span_of(&self, slice: &crate::level::WallSlice) -> (f32, f32) {
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
    first: crate::level::WallSlice,
    second: crate::level::WallSlice,
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
#[allow(clippy::too_many_lines)] // one joint pass over every wall pair
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
            let kind = if coplanar || (ambiguous && kind == "step") {
                "near-step"
            } else {
                kind
            };
            let auto = kind == "step";
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
                    kind,
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
        let _ = writeln!(
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
            let _ = writeln!(out, "  {check}: {errors} error(s), {warnings} warning(s)");
        }
        for (check, count) in &self.suppressed {
            if *count > 0 {
                let _ = writeln!(out, "  {check}: {count} suppressed by geometry_intent");
            }
        }
        for finding in &self.findings {
            if !verbose && finding.severity == Severity::Warning {
                continue;
            }
            let _ = writeln!(
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
        let _ = writeln!(
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
    /// Level path or id (`places_demo`, `assets/levels/places_demo.json`, ...).
    pub level: String,
    /// Write the machine-readable report here, in addition to the human one.
    pub json: Option<PathBuf>,
    /// Write deterministic marker anchors (JSON) here.
    pub markers: Option<PathBuf>,
    /// Write a marker OBJ (a small cross per finding) here.
    pub markers_obj: Option<PathBuf>,
    /// Treat warnings as a failing status.
    pub strict: bool,
    /// Print warnings in the human report too.
    pub verbose: bool,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            level: "places_demo".to_string(),
            json: None,
            markers: None,
            markers_obj: None,
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
            "--quiet" => options.verbose = false,
            other if other.starts_with("--") => {
                return Err(format!("unknown checker argument `{other}`"));
            }
            other => options.level = other.to_string(),
        }
        index = index.saturating_add(1);
    }
    Ok(Some(options))
}

/// Resolves a level argument to a readable source: an explicit path, a
/// shipped level id, or a drop-in level id under `levels/`.
fn resolve_source(level: &str) -> Result<(PathBuf, String), String> {
    let candidates: Vec<PathBuf> = if level.to_ascii_lowercase().ends_with(".json") {
        vec![PathBuf::from(level)]
    } else {
        vec![
            PathBuf::from(format!("assets/levels/{level}.json")),
            PathBuf::from(format!("levels/{level}.json")),
        ]
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

    let materials = crate::render::logical_materials(level);
    let catalog = crate::loader::PropCatalog::builtin();
    let mesh =
        crate::render::build_level_geometry_with_catalog_and_materials(level, &catalog, &materials);

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
    checker.check_props(&crate::loader::PropCatalog::load_default(), &surfaces);
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
#[allow(clippy::print_stdout, clippy::print_stderr)]
pub fn run(options: &CliOptions) -> Result<i32, String> {
    let (path, text) = resolve_source(&options.level)?;
    let mut level = match LevelDef::from_json(&text) {
        Ok(level) => level,
        Err(error) => {
            eprintln!("geometry check: {} does not parse: {error}", path.display());
            return Ok(2);
        }
    };
    let validation = crate::loader::validate_level(&level);
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
            }],
            suppressed: BTreeMap::new(),
        };
        let checked = check_level(&level, &path.display().to_string(), false);
        report.findings.extend(checked.findings);
        report.suppressed = checked.suppressed;
        return finish(&report, options);
    }
    let catalog = crate::assets::AssetCatalog::load_default();
    crate::loader::prepare_level(&mut level, &catalog, None);
    let report = check_level(&level, &path.display().to_string(), validated);
    finish(&report, options)
}

/// Prints and writes one report, returning the exit status.
// This is the CLI's own reporting path; there is no logger in a headless run.
#[allow(clippy::print_stdout, clippy::print_stderr)]
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
            let _ = writeln!(out, "v {} {} {}", low[0], low[1], low[2]);
            let _ = writeln!(out, "v {} {} {}", high[0], high[1], high[2]);
        }
    }
    for line in 0..markers.len() {
        let base = line.saturating_mul(6).saturating_add(1);
        let _ = writeln!(out, "l {base} {}", base.saturating_add(1));
        let _ = writeln!(
            out,
            "l {} {}",
            base.saturating_add(2),
            base.saturating_add(3)
        );
        let _ = writeln!(
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
fn wall_slice_aabb(wall: &WallDef, slice: &crate::level::WallSlice) -> WallAabb {
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
    #[allow(clippy::cast_possible_truncation)]
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
        });
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

impl Checker<'_> {
    /// Confirmed duplicated coplanar surfaces: two triangles on one plane
    /// whose projected areas overlap by more than a threshold.
    #[allow(clippy::too_many_lines)] // one coplanar-overlap pass over the bucket map
    fn check_duplicate_surfaces(&mut self, triangles: &[Tri]) {
        // Canonical plane key: a sign-canonical normal and plane distance,
        // quantised to 1 mm so exactly-coincident planes bucket together.
        let mut planes: HashMap<[i32; 4], Vec<usize>> = HashMap::new();
        for (index, triangle) in triangles.iter().enumerate() {
            if triangle.area <= 1.0e-9 {
                continue;
            }
            let mut normal = triangle.normal;
            let mut distance = dot3(normal, triangle.points[0]);
            let flip = normal[0] < 0.0
                || (normal[0] == 0.0 && normal[1] < 0.0)
                || (normal[0] == 0.0 && normal[1] == 0.0 && normal[2] < 0.0);
            if flip {
                normal = [-normal[0], -normal[1], -normal[2]];
                distance = -distance;
            }
            let key = [
                quantize(normal[0]),
                quantize(normal[1]),
                quantize(normal[2]),
                quantize(distance),
            ];
            planes.entry(key).or_default().push(index);
        }
        // Each triangle belongs to exactly one bucket and slot pairs occur
        // once. Recording all visited pairs would retain quadratic memory.
        for bucket in planes.values() {
            for (slot, first) in bucket.iter().enumerate() {
                for second in bucket.iter().skip(slot.saturating_add(1)) {
                    let (Some(a), Some(b)) = (triangles.get(*first), triangles.get(*second)) else {
                        continue;
                    };
                    // Two planes can quantise together while lying up to 1.5 mm
                    // apart; require a real coplanarity before flagging.
                    let separation = dot3(a.normal, sub3(b.centroid, a.points[0])).abs();
                    if separation > PLANE_EPS_M * 1.5 {
                        continue;
                    }
                    let area = triangle_overlap_area(a, b);
                    if area <= OVERLAP_AREA_EPS_M2 {
                        continue;
                    }
                    if area <= SLIVER_AREA_EPS_M2 {
                        self.push(
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
                            a.centroid,
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
                        self.push(
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
                            a.centroid,
                        );
                        continue;
                    }
                    if dot3(a.normal, b.normal) < -0.5 {
                        // The same plane covered twice with opposite facings is
                        // a reversed face *or* a legitimate back-to-back pair
                        // (two stacked walls' caps at one junction); without
                        // solid semantics the checker cannot tell them apart,
                        // so this stays a heuristic warning.
                        self.push(
                            "reversed-face",
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
                            a.centroid,
                        );
                    } else {
                        self.push(
                            "duplicate-surface",
                            Severity::Error,
                            format!("{:?} m{}", a.key.kind, a.key.material),
                            format!(
                                "coplanar surfaces overlap by {area:.4} m²: a m{} {:?} n{:?} b m{} {:?} n{:?}",
                                a.key.material, a.points, a.normal, b.key.material, b.points, b.normal
                            ),
                            a.centroid,
                        );
                    }
                }
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
    #[allow(clippy::cast_possible_truncation)]
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
    let b = local(b);
    let c = local(c);
    let cross = |o: [f32; 2], u: [f32; 2], v: [f32; 2]| {
        (u[0] - o[0]).mul_add(v[1] - o[1], -((u[1] - o[1]) * (v[0] - o[0])))
    };
    let ab = cross([0.0, 0.0], b, p);
    let bc = cross(b, c, p);
    let ca = cross(c, [0.0, 0.0], p);
    let edge_scale = [b, c].iter().fold(1.0_f32, |largest, value| {
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
    #[allow(clippy::too_many_lines)] // one joint's sampling pass, kept together
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
            for plane in [planes.0, planes.1] {
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
    #[allow(clippy::too_many_lines)] // one collider pass, kept together for the shared index
    fn check_colliders(
        &mut self,
        colliders: &[Collider],
        engine_boxes: &[WallAabb],
        triangles: &[Tri],
        index: &SpatialHash,
    ) {
        // Exact duplicates between two authored elements.
        let mut groups: HashMap<[i32; 6], Vec<usize>> = HashMap::new();
        for (slot, collider) in colliders.iter().enumerate() {
            groups
                .entry(aabb_key(&collider.aabb))
                .or_default()
                .push(slot);
        }
        for (key, group) in &groups {
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
            let _ = key;
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

        // Reversed faces and ghost colliders.
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
                let mut outward = [0.0_f32; 3];
                if let Some(slot) = outward.get_mut(axis) {
                    *slot = sign;
                }
                let mut face_covered = false;
                if let Some(candidates) = index.near(centre) {
                    for slot in candidates {
                        let Some(triangle) = usize::try_from(*slot)
                            .ok()
                            .and_then(|slot| triangles.get(slot))
                        else {
                            continue;
                        };
                        let offset = dot3(triangle.normal, sub3(centre, triangle.points[0]));
                        if offset.abs() > FACE_SAMPLE_EPS_M {
                            continue;
                        }
                        if !triangle_on_face(triangle, boxed, axis) {
                            continue;
                        }
                        if !point_in_triangle(centre, triangle) {
                            continue;
                        }
                        let _ = outward;
                        face_covered = true;
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
    let p = local(p);
    let a = [0.0_f32, 0.0];
    let b = local(b);
    let c = local(c);
    // The tolerance scales with the triangle's own extent: a 20 m face's cross
    // product carries ~mlp(extent²) of rounding, and an absolute 1e-6 would
    // reject an exactly-on-edge face centre (the ghost-collider false positive
    // this fixes).
    let scale = [&p, &a, &b, &c]
        .iter()
        .flat_map(|value| value.iter())
        .fold(1.0_f32, |largest, value| largest.max(value.abs()));
    let epsilon = (scale * scale).mul_add(1.0e-6, 1.0e-9);
    // The projected cross product is the formula, not unchecked arithmetic.
    #[allow(clippy::arithmetic_side_effects)]
    let cross = |o: [f32; 2], u: [f32; 2], v: [f32; 2]| {
        (u[0] - o[0]).mul_add(v[1] - o[1], (u[1] - o[1]) * -(v[0] - o[0]))
    };
    let ab = cross(a, b, p);
    let bc = cross(b, c, p);
    let ca = cross(c, a, p);
    (ab >= -epsilon && bc >= -epsilon && ca >= -epsilon)
        || (ab <= epsilon && bc <= epsilon && ca <= epsilon)
}

/// True when a triangle's centroid lies within a collider's face rectangle.
fn triangle_on_face(triangle: &Tri, boxed: &WallAabb, axis: usize) -> bool {
    let centre = triangle.centroid;
    let mut within = true;
    for (candidate, value) in centre.iter().enumerate() {
        if candidate == axis {
            continue;
        }
        let (low, high) = match candidate {
            0 => (boxed.min_x, boxed.max_x),
            1 => (boxed.min_y, boxed.max_y),
            _ => (boxed.min_z, boxed.max_z),
        };
        within = within && *value >= low - 0.001 && *value <= high + 0.001;
    }
    within
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

/// Props are not part of the emitted architecture mesh, so coincident
/// placements and large dressing layers stacked a few millimetres off a
/// walkable surface are invisible to the mesh checks. These heuristics cover
/// the two recurring classes: an exact duplicate placement, and a slab whose
/// visible top lands within 2 cm of the walkable floor at its own footprint.
impl Checker<'_> {
    fn check_props(&mut self, catalog: &crate::loader::PropCatalog, surfaces: &LevelSurfaces<'_>) {
        let props = &self.level.props;
        let mut by_model: HashMap<&str, Vec<usize>> = HashMap::new();
        for (index, prop) in props.iter().enumerate() {
            by_model.entry(prop.model.as_str()).or_default().push(index);
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
                    #[allow(clippy::arithmetic_side_effects)]
                    let same = (a.x - b.x).abs() <= 1.0e-3
                        && (a.z - b.z).abs() <= 1.0e-3
                        && (a.y - b.y).abs() <= 1.0e-3
                        && (a.rotation_degrees - b.rotation_degrees).abs() <= 0.5
                        && (a.scale - b.scale).abs() <= 1.0e-3;
                    if same {
                        let a_name = a.id.as_deref().unwrap_or(&a.model);
                        let b_name = b.id.as_deref().unwrap_or(&b.model);
                        self.push(
                            "prop-duplicate",
                            Severity::Warning,
                            a_name,
                            format!(
                                "'{b_name}' repeats '{a_name}' ({}) at the same transform; \
                                 the duplicate is redundant and z-fights",
                                a.model
                            ),
                            [a.x, a.y, a.z],
                        );
                    }
                }
            }
        }
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
            #[allow(clippy::arithmetic_side_effects)]
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
    let (x, z) = crate::level::wall_point(wall, along);
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

    #[allow(clippy::too_many_lines)] // one curved primitive's checks, kept together
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
                let (x, z) = crate::level::round_point(piece.x, piece.z, radius, angle);
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

    #[allow(clippy::too_many_lines)] // one curved primitive's checks, kept together
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
        for (index, room) in self.level.room_iter().enumerate() {
            if !room.width.is_finite() || !room.depth.is_finite() {
                continue;
            }
            self.check_room_leak(index, room, surfaces, &boxes);
            self.check_room_perimeter(index, room, surfaces, &boxes);
        }
    }

    fn check_room_leak(
        &mut self,
        index: usize,
        room: &RoomDef,
        surfaces: &LevelSurfaces<'_>,
        boxes: &[WallAabb],
    ) {
        let (x0, x1, z0, z1) = room.bounds();
        let (foot_x, foot_z) = (f32::midpoint(x0, x1), f32::midpoint(z0, z1));
        let Some(foot) = surfaces.floor_y_at(foot_x, foot_z) else {
            return;
        };
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
        let mut queue: VecDeque<(usize, usize)> = VecDeque::new();
        for ix in 0..cells_x {
            for iz in 0..cells_z {
                let (x, z) = point(ix, iz);
                if room.contains(x, z) {
                    let slot = iz.saturating_mul(cells_x).saturating_add(ix);
                    if let Some(entry) = visited.get_mut(slot)
                        && !*entry
                    {
                        *entry = true;
                        queue.push_back((ix, iz));
                    }
                }
            }
        }
        let mut exit: Option<(f32, f32, f32)> = None;
        // Each cell resolves its own walkable floor: a room that contains a
        // deep basin is blocked at the basin's level by the basin's own rims,
        // and a deck-level wall must not be treated as passable just because
        // the room centre happens to be lower.
        let blocked = |x: f32, z: f32, from_foot: f32| {
            // Outside every room there is no floor; the step keeps the foot it
            // came from, so a wall at the room edge still blocks an escape
            // instead of becoming passable because the void resolves to no
            // walkable surface.
            let cell_foot = surfaces.floor_y_at(x, z).unwrap_or(from_foot);
            boxes.iter().any(|boxed| {
                boxed.overlaps_disc(x, z, LEAK_BODY_RADIUS_M)
                    && boxed.blocks_body(cell_foot, LEAK_BODY_HEIGHT_M)
            })
        };
        while let Some((ix, iz)) = queue.pop_front() {
            let (cx, cz) = point(ix, iz);
            let current_foot = surfaces.floor_y_at(cx, cz).unwrap_or(foot);
            for (dx, dz) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
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
                if blocked(px, pz, current_foot) {
                    continue;
                }
                *entry = true;
                queue.push_back((nx, nz));
                // Outside the room and outside every room: the void.
                if !room.contains(px, pz) && surfaces.room_at(px, pz).is_none() {
                    let penetration = (x0 - px).max(px - x1).max(z0 - pz).max(pz - z1);
                    let candidate = (px, pz, penetration);
                    if exit.is_none_or(|(_, _, depth)| penetration > depth) {
                        exit = Some(candidate);
                    }
                }
            }
        }
        if let Some((x, z, depth)) = exit {
            self.push(
                "room-leak",
                Severity::Warning,
                format!("room {index}"),
                format!(
                    "the room's walkable space reaches the void {depth:.2} m outside its footprint; \
                     no wall, opening or annotation explains it"
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
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
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
    #[allow(clippy::cast_precision_loss)] // bounded grid indices
    let converted = value as f32;
    converted
}

/// Prints a usage error and ends the process with the documented status `2`.
///
/// Kept with the checker's process-exit path so the narrow `exit` allow stays
/// in one place.
#[allow(clippy::exit, clippy::print_stderr)]
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
#[allow(clippy::exit, clippy::print_stderr)]
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

/// Stable format id written into the machine-readable repair plan.
pub const REPAIR_FORMAT: &str = "places-geometry-repair-plan";
/// Repair plan schema version.
pub const REPAIR_VERSION: u32 = 1;

/// The CLI options of `--repair-geometry`.
#[derive(Clone, Debug)]
pub struct RepairCliOptions {
    /// Level path or id (`places_demo`, `assets/levels/places_demo.json`, ...).
    pub level: String,
    /// Write the machine-readable plan here, in addition to the human one.
    pub plan: Option<PathBuf>,
    /// Print the machine-readable plan to stdout instead of the human plan.
    pub json: bool,
}

impl Default for RepairCliOptions {
    fn default() -> Self {
        Self {
            level: "places_demo".to_string(),
            plan: None,
            json: false,
        }
    }
}

/// Parses the process arguments, or `None` when this is not a repair run.
///
/// # Errors
///
/// Returns a usage message for an unknown argument or a missing value.
pub fn repair_options_from_args(args: &[String]) -> Result<Option<RepairCliOptions>, String> {
    if !args.iter().any(|arg| arg == "--repair-geometry") {
        return Ok(None);
    }
    let mut options = RepairCliOptions::default();
    let mut index = 0usize;
    while let Some(arg) = args.get(index) {
        match arg.as_str() {
            "--repair-geometry" => {}
            "--level" => {
                index = index.saturating_add(1);
                options.level = args
                    .get(index)
                    .cloned()
                    .ok_or_else(|| "--level needs a value".to_string())?;
            }
            "--plan" => {
                index = index.saturating_add(1);
                options.plan = Some(PathBuf::from(
                    args.get(index)
                        .cloned()
                        .ok_or_else(|| "--plan needs a value".to_string())?,
                ));
            }
            "--json" => options.json = true,
            other if other.starts_with("--") => {
                return Err(format!("unknown repair argument `{other}`"));
            }
            other => options.level = other.to_string(),
        }
        index = index.saturating_add(1);
    }
    Ok(Some(options))
}

/// The machine-readable repair plan.
#[derive(Debug, Serialize)]
struct RepairPlan {
    format: &'static str,
    version: u32,
    level: RepairPlanLevel,
    findings: Vec<WallJoint>,
    edits: Vec<RepairPlanEdit>,
    review: Vec<RepairPlanReview>,
    post_check: RepairPlanPostCheck,
}

/// Where the plan's source came from and what it hashed to.
#[derive(Debug, Serialize)]
struct RepairPlanLevel {
    id: String,
    source: String,
    sha256: String,
}

/// One field edit, addressed by JSON pointer into the level source.
#[derive(Debug, Serialize)]
struct RepairPlanEdit {
    pointer: String,
    old: f32,
    new: f32,
    reason: &'static str,
    finding: usize,
    coupled: &'static str,
}

/// One reviewed, never-applied coupling or refused repair.
#[derive(Debug, Serialize)]
struct RepairPlanReview {
    kind: &'static str,
    pointer: String,
    message: String,
}

/// The post-repair checker summary.
#[derive(Debug, Serialize)]
struct RepairPlanPostCheck {
    errors: usize,
    warnings: usize,
}

/// One authored numeric field an edit writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EditField {
    X,
    Z,
    Length,
    Width,
    Depth,
}

impl EditField {
    const fn name(self) -> &'static str {
        match self {
            Self::X => "x",
            Self::Z => "z",
            Self::Length => "length",
            Self::Width => "width",
            Self::Depth => "depth",
        }
    }
}

/// The authored array element one edit addresses.
#[derive(Clone, Copy, Debug)]
enum EditTarget {
    Wall { index: usize, field: EditField },
    Baseboard { index: usize, field: EditField },
    FloorRegion { index: usize, field: EditField },
}

impl EditTarget {
    fn pointer(self) -> String {
        match self {
            Self::Wall { index, field } => format!("/walls/{index}/{}", field.name()),
            Self::Baseboard { index, field } => format!("/baseboards/{index}/{}", field.name()),
            Self::FloorRegion { index, field } => {
                format!("/floor_regions/{index}/{}", field.name())
            }
        }
    }

    fn write(self, level: &mut LevelDef, value: f32) -> bool {
        match self {
            Self::Wall { index, field } => {
                let Some(wall) = level.walls.get_mut(index) else {
                    return false;
                };
                match field {
                    EditField::X => wall.x = value,
                    EditField::Z => wall.z = value,
                    EditField::Length | EditField::Width | EditField::Depth => return false,
                }
                true
            }
            Self::Baseboard { index, field } => {
                let Some(board) = level.baseboards.get_mut(index) else {
                    return false;
                };
                match field {
                    EditField::X => board.x = value,
                    EditField::Z => board.z = value,
                    EditField::Length => board.length = value,
                    EditField::Width | EditField::Depth => return false,
                }
                true
            }
            Self::FloorRegion { index, field } => {
                let Some(region) = level.floor_regions.get_mut(index) else {
                    return false;
                };
                match field {
                    EditField::X => region.x = value,
                    EditField::Z => region.z = value,
                    EditField::Width => region.width = value,
                    EditField::Depth => region.depth = value,
                    EditField::Length => return false,
                }
                true
            }
        }
    }

    fn same(self, other: Self) -> bool {
        match (self, other) {
            (
                Self::Wall { index, field },
                Self::Wall {
                    index: other_index,
                    field: other_field,
                },
            )
            | (
                Self::Baseboard { index, field },
                Self::Baseboard {
                    index: other_index,
                    field: other_field,
                },
            )
            | (
                Self::FloorRegion { index, field },
                Self::FloorRegion {
                    index: other_index,
                    field: other_field,
                },
            ) => index == other_index && field == other_field,
            _ => false,
        }
    }
}

/// One planned field edit before it is serialized.
#[derive(Clone, Debug)]
struct PlannedEdit {
    target: EditTarget,
    old: f32,
    new: f32,
    reason: &'static str,
    finding: usize,
    coupled: &'static str,
}

impl PlannedEdit {
    fn to_plan(&self) -> RepairPlanEdit {
        RepairPlanEdit {
            pointer: self.target.pointer(),
            old: self.old,
            new: self.new,
            reason: self.reason,
            finding: self.finding,
            coupled: self.coupled,
        }
    }
}

/// Applies every planned edit to a cloned level.
///
/// # Errors
///
/// Returns the first target that no longer resolves.
fn apply_planned_edits(level: &mut LevelDef, edits: &[PlannedEdit]) -> Result<(), String> {
    for edit in edits {
        if !edit.target.write(level, edit.new) {
            return Err(format!(
                "planned edit {} no longer resolves in the source",
                edit.target.pointer()
            ));
        }
    }
    Ok(())
}

/// Hex SHA-256 of the source bytes, the plan's concurrent-change guard.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

/// Error finding counts keyed by `check|id`, for the no-new-error proof.
fn error_fingerprints(report: &CheckReport) -> BTreeMap<String, usize> {
    let mut out: BTreeMap<String, usize> = BTreeMap::new();
    for finding in &report.findings {
        if finding.severity != Severity::Error {
            continue;
        }
        let entry = out
            .entry(format!("{}|{}", finding.check, finding.id))
            .or_insert(0);
        *entry = entry.saturating_add(1);
    }
    out
}

/// A world footprint with an optional absolute Y range, for the coupled-review
/// scan.
struct ElementFootprint {
    label: String,
    pointer: String,
    x: (f32, f32),
    z: (f32, f32),
    y: Option<(f32, f32)>,
}

/// Adds one authored rectangle: `[x, z, width, depth]` with signed extents.
fn footprint_rect(
    out: &mut Vec<ElementFootprint>,
    label: String,
    pointer: String,
    rect: [f32; 4],
    y: Option<(f32, f32)>,
) {
    let [x, z, width, depth] = rect;
    if !x.is_finite() || !z.is_finite() || !width.is_finite() || !depth.is_finite() {
        return;
    }
    out.push(ElementFootprint {
        label,
        pointer,
        x: (x.min(x + width), x.max(x + width)),
        z: (z.min(z + depth), z.max(z + depth)),
        y,
    });
}

/// Adds one centre-anchored, conservatively rotated rectangle.
fn footprint_centred(
    out: &mut Vec<ElementFootprint>,
    label: String,
    pointer: String,
    centre: [f32; 2],
    half: [f32; 2],
    rotation_degrees: f32,
    y: Option<(f32, f32)>,
) {
    let [x, z] = centre;
    let [half_x, half_z] = half;
    if !x.is_finite() || !z.is_finite() || !half_x.is_finite() || !half_z.is_finite() {
        return;
    }
    let (sin, cos) = if rotation_degrees.is_finite() {
        rotation_degrees.to_radians().sin_cos()
    } else {
        (0.0, 1.0)
    };
    let extent_x = half_x.mul_add(cos.abs(), half_z * sin.abs());
    let extent_z = half_x.mul_add(sin.abs(), half_z * cos.abs());
    out.push(ElementFootprint {
        label,
        pointer,
        x: (x - extent_x, x + extent_x),
        z: (z - extent_z, z + extent_z),
        y,
    });
}

/// Adds a line-segment footprint (`start` to `end`) inflated by `pad`.
fn footprint_segment(
    out: &mut Vec<ElementFootprint>,
    label: String,
    pointer: String,
    start: (f32, f32),
    end: (f32, f32),
    pad: f32,
    y: Option<(f32, f32)>,
) {
    if [start.0, start.1, end.0, end.1, pad]
        .iter()
        .any(|value| !value.is_finite())
    {
        return;
    }
    out.push(ElementFootprint {
        label,
        pointer,
        x: (start.0.min(end.0) - pad, start.0.max(end.0) + pad),
        z: (start.1.min(end.1) - pad, start.1.max(end.1) + pad),
        y,
    });
}

/// The end point of a run from `start` along `direction` for `length`.
const fn segment_end(start: (f32, f32), direction: (f32, f32), length: f32) -> (f32, f32) {
    (
        direction.0.mul_add(length, start.0),
        direction.1.mul_add(length, start.1),
    )
}

/// Every non-wall, non-trim authored element that can sit near a wall face.
///
/// Baseboards and floor regions are excluded: their coupling rules are
/// deterministic, not review-only.
#[allow(clippy::too_many_lines)] // one inventory of every placeable family
fn element_footprints(level: &LevelDef) -> Vec<ElementFootprint> {
    let mut out = Vec::new();
    for (index, prop) in level.props.iter().enumerate() {
        let size = prop.resolved_size(crate::level::PROP_FALLBACK_SIZE);
        footprint_centred(
            &mut out,
            format!("prop {index} ({})", prop.model),
            format!("/props/{index}"),
            [prop.x, prop.z],
            [size[0] * 0.5, size[2] * 0.5],
            prop.rotation_degrees,
            None,
        );
    }
    for (index, door) in level.doors.iter().enumerate() {
        footprint_centred(
            &mut out,
            format!("door {index} ({})", door.id),
            format!("/doors/{index}"),
            [door.x, door.z],
            [door.width * 0.5, door.width * 0.5],
            door.rotation_degrees,
            None,
        );
    }
    for (index, decal) in level.decals.iter().enumerate() {
        footprint_centred(
            &mut out,
            format!("decal {index}"),
            format!("/decals/{index}"),
            [decal.x, decal.z],
            decal.half_extents(),
            decal.rotation_degrees,
            None,
        );
    }
    for (index, threshold) in level.thresholds.iter().enumerate() {
        let start = (threshold.x, threshold.z);
        let end = segment_end(start, threshold.direction(), threshold.length);
        footprint_segment(
            &mut out,
            format!("threshold {index}"),
            format!("/thresholds/{index}"),
            start,
            end,
            threshold.thickness() * 0.5,
            None,
        );
    }
    for (index, light) in level.ceiling_lights.iter().enumerate() {
        footprint_rect(
            &mut out,
            format!("light {index}"),
            format!("/ceiling_lights/{index}"),
            [light.x, light.z, 0.0, 0.0],
            None,
        );
    }
    for (index, stair) in level.stairs.iter().enumerate() {
        footprint_rect(
            &mut out,
            format!("stair {index}"),
            format!("/stairs/{index}"),
            [stair.x, stair.z, stair.width, stair.depth],
            None,
        );
    }
    for (index, volume) in level.volumes.iter().enumerate() {
        footprint_rect(
            &mut out,
            format!("volume {index} ({})", volume.id.as_deref().unwrap_or("?")),
            format!("/volumes/{index}"),
            [volume.x, volume.z, volume.width, volume.depth],
            match (volume.bottom_y, volume.top_y) {
                (Some(bottom), Some(top)) => Some((bottom, top)),
                _ => None,
            },
        );
    }
    for (index, water) in level.water.iter().enumerate() {
        let (x0, x1, z0, z1) = water.bounds();
        footprint_rect(
            &mut out,
            format!("water {index}"),
            format!("/water/{index}"),
            [x0, z0, x1 - x0, z1 - z0],
            Some((water.surface_y - 0.5, water.surface_y)),
        );
    }
    for (index, ladder) in level.ladders.iter().enumerate() {
        footprint_rect(
            &mut out,
            format!("ladder {index}"),
            format!("/ladders/{index}"),
            [ladder.x, ladder.z, ladder.width, ladder.depth],
            Some((ladder.bottom_y, ladder.top_y)),
        );
    }
    for (index, ramp) in level.ramps.iter().enumerate() {
        footprint_rect(
            &mut out,
            format!("ramp {index}"),
            format!("/ramps/{index}"),
            [ramp.x, ramp.z, ramp.width, ramp.depth],
            None,
        );
    }
    for (index, patch) in level.floor_patches.iter().enumerate() {
        footprint_rect(
            &mut out,
            format!("floor patch {index}"),
            format!("/floor_patches/{index}"),
            [patch.x, patch.z, patch.width, patch.depth],
            None,
        );
    }
    for (index, effect) in level.effects.iter().enumerate() {
        footprint_rect(
            &mut out,
            format!("effect {index} ({})", effect.id.as_deref().unwrap_or("?")),
            format!("/effects/{index}"),
            [effect.x, effect.z, effect.width, effect.depth],
            None,
        );
    }
    for (index, piece) in level.half_walls.iter().enumerate() {
        footprint_rect(
            &mut out,
            format!("half wall {index}"),
            format!("/half_walls/{index}"),
            [piece.x, piece.z, piece.width, piece.depth],
            None,
        );
    }
    for (index, piece) in level.columns.iter().enumerate() {
        footprint_rect(
            &mut out,
            format!("column {index}"),
            format!("/columns/{index}"),
            [piece.x, piece.z, piece.width, piece.depth],
            None,
        );
    }
    for (index, piece) in level.archways.iter().enumerate() {
        footprint_rect(
            &mut out,
            format!("archway {index}"),
            format!("/archways/{index}"),
            [piece.x, piece.z, piece.width, piece.depth],
            None,
        );
    }
    for (index, piece) in level.guardrails.iter().enumerate() {
        let radians = piece.rotation_degrees.to_radians();
        let start = (piece.x, piece.z);
        let end = segment_end(start, (radians.cos(), -radians.sin()), piece.length);
        footprint_segment(
            &mut out,
            format!("guardrail {index}"),
            format!("/guardrails/{index}"),
            start,
            end,
            0.05,
            None,
        );
    }
    for (index, piece) in level.arc_walls.iter().enumerate() {
        footprint_centred(
            &mut out,
            format!("arc wall {index}"),
            format!("/arc_walls/{index}"),
            [piece.x, piece.z],
            [piece.outer_radius(), piece.outer_radius()],
            0.0,
            None,
        );
    }
    for (index, piece) in level.pillars.iter().enumerate() {
        footprint_centred(
            &mut out,
            format!("pillar {index}"),
            format!("/pillars/{index}"),
            [piece.x, piece.z],
            [piece.radius, piece.radius],
            0.0,
            None,
        );
    }
    out
}

/// A mover wall's geometry in the joint's `(along, across)` frame.
struct MoverGeometry {
    /// World `(low, high)` along the length axis.
    along: (f32, f32),
    /// Authored across planes `(low, high)` before the move.
    planes: (f32, f32),
    /// The axis the wall runs along.
    axis: WallAxis,
    /// Signed shift to add on the across axis.
    delta: f32,
    /// Absolute `(low, high)` Y of the mover wall.
    y: (f32, f32),
}

impl MoverGeometry {
    fn of(level: &LevelDef, joint: &WallJoint) -> Option<Self> {
        let wall = level.walls.get(joint.second)?;
        let axis = wall_axis_of(joint.axis);
        let (min_x, max_x) = (
            wall.x.min(wall.x + wall.width),
            wall.x.max(wall.x + wall.width),
        );
        let (min_z, max_z) = (
            wall.z.min(wall.z + wall.depth),
            wall.z.max(wall.z + wall.depth),
        );
        let (along, planes) = match axis {
            WallAxis::X => ((min_x, max_x), (min_z, max_z)),
            WallAxis::Z => ((min_z, max_z), (min_x, max_x)),
        };
        let height = wall
            .height
            .filter(|value| value.is_finite())
            .unwrap_or(crate::level::DEFAULT_CEILING_HEIGHT_M);
        Some(Self {
            along,
            planes,
            axis,
            delta: joint.shift,
            y: (wall.y, wall.y + height),
        })
    }

    /// The authored across-coordinate field.
    const fn across_field(&self) -> EditField {
        match self.axis {
            WallAxis::X => EditField::Z,
            WallAxis::Z => EditField::X,
        }
    }

    /// The authored extent field on the across axis.
    const fn extent_field(&self) -> EditField {
        match self.axis {
            WallAxis::X => EditField::Depth,
            WallAxis::Z => EditField::Width,
        }
    }

    /// The along/across spans of a world rectangle in this mover's frame.
    const fn spans(&self, footprint: &ElementFootprint) -> ((f32, f32), (f32, f32)) {
        match self.axis {
            WallAxis::X => (footprint.x, footprint.z),
            WallAxis::Z => (footprint.z, footprint.x),
        }
    }
}

/// True when two spans overlap within `margin`.
fn spans_overlap(a: (f32, f32), b: (f32, f32), margin: f32) -> bool {
    a.0 <= b.1 + margin && b.0 <= a.1 + margin
}

/// True when a footprint lies within `tolerance` of either moved face plane
/// and along the mover's length span.
fn footprint_touches_face(
    mover: &MoverGeometry,
    footprint: &ElementFootprint,
    tolerance: f32,
) -> bool {
    let (along, across) = mover.spans(footprint);
    if !spans_overlap(along, mover.along, tolerance) {
        return false;
    }
    let near = across.0 <= mover.planes.1 + tolerance && across.1 >= mover.planes.0 - tolerance;
    if !near {
        return false;
    }
    footprint
        .y
        .is_none_or(|y| y.0 <= mover.y.1 + tolerance && y.1 >= mover.y.0 - tolerance)
}

/// The deterministic coupled edits for one automatic wall shift, plus the
/// review entries for elements that must not be moved automatically.
#[allow(clippy::too_many_lines)] // one linear pass over the coupled element rules
fn coupled_edits(
    level: &LevelDef,
    candidate: &WallJointCandidate,
    finding: usize,
) -> (Vec<PlannedEdit>, Vec<RepairPlanReview>) {
    let mut edits: Vec<PlannedEdit> = Vec::new();
    let mut review: Vec<RepairPlanReview> = Vec::new();
    let joint = &candidate.joint;
    let Some(mover) = MoverGeometry::of(level, joint) else {
        return (edits, review);
    };
    if let Some(wall) = level.walls.get(joint.second) {
        let old = match mover.axis {
            WallAxis::X => wall.z,
            WallAxis::Z => wall.x,
        };
        edits.push(PlannedEdit {
            target: EditTarget::Wall {
                index: joint.second,
                field: mover.across_field(),
            },
            old,
            new: quantise(old + mover.delta),
            reason: "wall-joint-step",
            finding,
            coupled: "wall",
        });
    }

    // Baseboards: a parallel run moves with the face, an end butting into the
    // face follows it, a diagonal run is review only.
    for (index, board) in level.baseboards.iter().enumerate() {
        let (dx, dz) = board.direction();
        let axis_aligned = dx.abs() <= 1.0e-3 || dz.abs() <= 1.0e-3;
        let (dir_along, dir_across) = match mover.axis {
            WallAxis::X => (dx, dz),
            WallAxis::Z => (dz, dx),
        };
        if !axis_aligned {
            if board_touches(board, &mover) {
                review.push(RepairPlanReview {
                    kind: "coupled-review",
                    pointer: format!("/baseboards/{index}"),
                    message: format!(
                        "baseboard {index} runs diagonally and touches the moved wall {} face; \
                         move it by {:.4} m on {} by hand",
                        joint.second,
                        mover.delta,
                        mover.across_field().name()
                    ),
                });
            }
            continue;
        }
        if dir_across.abs() >= 0.999 && dir_along.abs() <= 0.001 {
            // Perpendicular: the near end moves, the other end stays.
            let (start_across, start_along) = match mover.axis {
                WallAxis::X => (board.z, board.x),
                WallAxis::Z => (board.x, board.z),
            };
            let far_across = dir_across.mul_add(board.length, start_across);
            let in_span =
                start_along >= mover.along.0 - 0.05 && start_along <= mover.along.1 + 0.05;
            if !in_span {
                continue;
            }
            let start_near = (start_across - mover.planes.0).abs() <= 0.05
                || (start_across - mover.planes.1).abs() <= 0.05;
            let far_near = (far_across - mover.planes.0).abs() <= 0.05
                || (far_across - mover.planes.1).abs() <= 0.05;
            if start_near && far_near {
                review.push(RepairPlanReview {
                    kind: "coupled-review",
                    pointer: format!("/baseboards/{index}"),
                    message: format!(
                        "baseboard {index} has both ends within 0.05 m of the moved wall {} faces; \
                         review its run by hand",
                        joint.second
                    ),
                });
                continue;
            }
            if start_near {
                let new_start = quantise(start_across + mover.delta);
                let new_length = quantise(dir_across.mul_add(-mover.delta, board.length));
                if new_length > 0.0 {
                    push_unique(
                        &mut edits,
                        PlannedEdit {
                            target: EditTarget::Baseboard {
                                index,
                                field: mover.across_field(),
                            },
                            old: start_across,
                            new: new_start,
                            reason: "wall-joint-step",
                            finding,
                            coupled: "baseboard-end",
                        },
                    );
                    push_unique(
                        &mut edits,
                        PlannedEdit {
                            target: EditTarget::Baseboard {
                                index,
                                field: EditField::Length,
                            },
                            old: board.length,
                            new: new_length,
                            reason: "wall-joint-step",
                            finding,
                            coupled: "baseboard-end",
                        },
                    );
                } else {
                    review.push(RepairPlanReview {
                        kind: "coupled-review",
                        pointer: format!("/baseboards/{index}"),
                        message: format!(
                            "baseboard {index}'s near end would need a non-positive length after \
                             the {:.4} m move; review it by hand",
                            mover.delta
                        ),
                    });
                }
            } else if far_near {
                let new_length = quantise(dir_across.mul_add(mover.delta, board.length));
                if new_length > 0.0 {
                    push_unique(
                        &mut edits,
                        PlannedEdit {
                            target: EditTarget::Baseboard {
                                index,
                                field: EditField::Length,
                            },
                            old: board.length,
                            new: new_length,
                            reason: "wall-joint-step",
                            finding,
                            coupled: "baseboard-end",
                        },
                    );
                } else {
                    review.push(RepairPlanReview {
                        kind: "coupled-review",
                        pointer: format!("/baseboards/{index}"),
                        message: format!(
                            "baseboard {index}'s near end would need a non-positive length after \
                             the {:.4} m move; review it by hand",
                            mover.delta
                        ),
                    });
                }
            }
        } else if dir_along.abs() >= 0.999 && dir_across.abs() <= 0.001 {
            // Parallel: the whole run moves across with the contact face.
            let across = match mover.axis {
                WallAxis::X => board.z,
                WallAxis::Z => board.x,
            };
            let contact_near =
                (across - mover.planes.0).abs() <= 0.05 || (across - mover.planes.1).abs() <= 0.05;
            let (run_low, run_high) = {
                let start = match mover.axis {
                    WallAxis::X => board.x,
                    WallAxis::Z => board.z,
                };
                let end = dir_along.mul_add(board.length, start);
                (start.min(end), start.max(end))
            };
            if contact_near && spans_overlap((run_low, run_high), mover.along, 1.0e-3) {
                push_unique(
                    &mut edits,
                    PlannedEdit {
                        target: EditTarget::Baseboard {
                            index,
                            field: mover.across_field(),
                        },
                        old: across,
                        new: quantise(across + mover.delta),
                        reason: "wall-joint-step",
                        finding,
                        coupled: "baseboard-parallel",
                    },
                );
            }
        }
    }

    // Floor regions: a tucked edge follows the face, keeping a 5 cm tuck.
    for (index, region) in level.floor_regions.iter().enumerate() {
        let (x0, x1, z0, z1) = region.bounds();
        let (edge_low, edge_high, along) = match mover.axis {
            WallAxis::X => (z0, z1, (x0, x1)),
            WallAxis::Z => (x0, x1, (z0, z1)),
        };
        if !spans_overlap(along, mover.along, 0.05) {
            continue;
        }
        let origin = match mover.axis {
            WallAxis::X => region.z,
            WallAxis::Z => region.x,
        };
        let extent = match mover.axis {
            WallAxis::X => region.depth,
            WallAxis::Z => region.width,
        };
        for (plane_old, is_low_face) in [(mover.planes.0, true), (mover.planes.1, false)] {
            let plane_new = plane_old + mover.delta;
            let (edge, is_high_edge) = if is_low_face {
                (edge_high, true)
            } else {
                (edge_low, false)
            };
            let in_window = if is_low_face {
                edge >= plane_old - 0.02 && edge <= plane_new + 0.15
            } else {
                edge >= plane_new - 0.15 && edge <= plane_old + 0.02
            };
            if !in_window {
                continue;
            }
            let target = if is_low_face {
                plane_new + 0.05
            } else {
                plane_new - 0.05
            };
            let grows = if is_low_face {
                target > edge
            } else {
                target < edge
            };
            if !grows {
                continue;
            }
            let extent_moves_edge = if is_high_edge {
                extent >= 0.0
            } else {
                extent < 0.0
            };
            if !extent_moves_edge {
                review.push(RepairPlanReview {
                    kind: "coupled-review",
                    pointer: format!("/floor_regions/{index}"),
                    message: format!(
                        "floor region {index}'s edge at {edge:.3} tucks through the moved wall {} \
                         face but its signed extent cannot express the new edge {target:.3}; \
                         review it by hand",
                        joint.second
                    ),
                });
                continue;
            }
            push_unique(
                &mut edits,
                PlannedEdit {
                    target: EditTarget::FloorRegion {
                        index,
                        field: mover.extent_field(),
                    },
                    old: extent,
                    new: quantise(target - origin),
                    reason: "wall-joint-step",
                    finding,
                    coupled: "floor-tuck",
                },
            );
        }
    }

    // Everything else solid near the moved faces is review only.
    for footprint in element_footprints(level) {
        if footprint_touches_face(&mover, &footprint, 0.15) {
            review.push(RepairPlanReview {
                kind: "coupled-review",
                pointer: footprint.pointer,
                message: format!(
                    "{} lies within 0.15 m of the moved wall {} face; review before applying",
                    footprint.label, joint.second
                ),
            });
        }
    }

    (edits, review)
}

/// True when a non-axis-aligned baseboard plausibly reaches the moved wall.
fn board_touches(board: &crate::level::BaseboardDef, mover: &MoverGeometry) -> bool {
    let (start_along, start_across) = match mover.axis {
        WallAxis::X => (board.x, board.z),
        WallAxis::Z => (board.z, board.x),
    };
    let near_across =
        start_across >= mover.planes.0 - 0.15 && start_across <= mover.planes.1 + 0.15;
    let near_along = start_along >= mover.along.0 - 0.15 && start_along <= mover.along.1 + 0.15;
    near_across && near_along
}

/// Pushes an edit unless the same target is already planned.
fn push_unique(edits: &mut Vec<PlannedEdit>, edit: PlannedEdit) {
    if edits
        .iter()
        .any(|existing| existing.target.same(edit.target))
    {
        return;
    }
    edits.push(edit);
}

/// Planner state accumulated across candidate repairs.
struct PlannerState<'a> {
    catalog: &'a crate::assets::AssetCatalog,
    source: &'a str,
    fingerprints: BTreeMap<String, usize>,
    edits: Vec<PlannedEdit>,
    review: Vec<RepairPlanReview>,
}

impl PlannerState<'_> {
    /// Tries one automatic repair; returns the edited level when the
    /// post-check proves it safe.
    fn attempt(
        &mut self,
        current: &LevelDef,
        candidate: &WallJointCandidate,
        finding: usize,
    ) -> Option<LevelDef> {
        let (planned, coupled_review) = coupled_edits(current, candidate, finding);
        let mut attempt = current.clone();
        if let Err(error) = apply_planned_edits(&mut attempt, &planned) {
            self.review.push(RepairPlanReview {
                kind: "coupled-review",
                pointer: format!("walls[{}]", candidate.joint.second),
                message: format!("repair refused before applying: {error}"),
            });
            return None;
        }
        let mut prepared = attempt.clone();
        crate::loader::prepare_level(&mut prepared, self.catalog, None);
        if let Err(error) = crate::loader::validate_level(&prepared) {
            self.review.push(RepairPlanReview {
                kind: "coupled-review",
                pointer: format!("walls[{}]", candidate.joint.second),
                message: format!("repair refused: the loader now rejects the level: {error}"),
            });
            return None;
        }
        let after = check_level(&prepared, self.source, true);
        let pair_id = format!(
            "wall {} / wall {}",
            candidate.joint.first, candidate.joint.second
        );
        let step_gone = !after
            .findings
            .iter()
            .any(|finding| finding.check == "wall-joint-step" && finding.id == pair_id);
        let after_errors = error_fingerprints(&after);
        let no_new_error = after_errors
            .iter()
            .all(|(key, count)| self.fingerprints.get(key).copied().unwrap_or(0) >= *count);
        if !step_gone || !no_new_error {
            self.review.extend(coupled_review);
            self.review.push(RepairPlanReview {
                kind: "coupled-review",
                pointer: format!("walls[{}]", candidate.joint.second),
                message: format!(
                    "repair refused: the post-check still reports {} error(s) or a new one",
                    after.error_count()
                ),
            });
            return None;
        }
        self.fingerprints = after_errors;
        self.edits.extend(planned);
        self.review.extend(coupled_review);
        Some(attempt)
    }
}

/// Runs the planner end to end and returns the plan without printing it.
///
/// # Errors
///
/// Returns a usage, file or parse error, which the caller reports as status
/// `2`.
fn build_repair_plan(options: &RepairCliOptions) -> Result<(RepairPlan, PathBuf), String> {
    let (path, text) = resolve_source(&options.level)?;
    let sha256 = sha256_hex(text.as_bytes());
    let authored = LevelDef::from_json(&text)
        .map_err(|error| format!("{} does not parse: {error}", path.display()))?;
    if let Err(error) = crate::loader::validate_level(&authored) {
        return Err(format!("{} does not validate: {error}", path.display()));
    }
    let catalog = crate::assets::AssetCatalog::load_default();
    let source = path.display().to_string();
    let mut prepared = authored.clone();
    crate::loader::prepare_level(&mut prepared, &catalog, None);
    let surfaces = LevelSurfaces::new(&prepared);
    let candidates = compute_wall_joints(&prepared, &surfaces);
    let findings: Vec<WallJoint> = candidates
        .iter()
        .map(|candidate| candidate.joint.clone())
        .collect();
    let baseline = check_level(&prepared, &source, true);
    let mut current = authored;
    let mut state = PlannerState {
        catalog: &catalog,
        source: &source,
        fingerprints: error_fingerprints(&baseline),
        edits: Vec::new(),
        review: Vec::new(),
    };
    for (index, candidate) in candidates.iter().enumerate() {
        if !candidate.joint.auto_repairable {
            continue;
        }
        if let Some(attempt) = state.attempt(&current, candidate, index) {
            current = attempt;
        }
    }
    let (edits, review) = (state.edits, state.review);

    let mut final_prepared = current;
    crate::loader::prepare_level(&mut final_prepared, &catalog, None);
    let post = check_level(&final_prepared, &source, true);
    let plan = RepairPlan {
        format: REPAIR_FORMAT,
        version: REPAIR_VERSION,
        level: RepairPlanLevel {
            id: prepared.id.clone(),
            source,
            sha256,
        },
        findings,
        edits: edits.iter().map(PlannedEdit::to_plan).collect(),
        review,
        post_check: RepairPlanPostCheck {
            errors: post.error_count(),
            warnings: post.warning_count(),
        },
    };
    Ok((plan, path))
}

/// The human-readable repair plan.
fn repair_plan_human(plan: &RepairPlan) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "geometry repair plan: {} ({})",
        plan.level.id, plan.level.source
    );
    if plan.findings.is_empty() {
        let _ = writeln!(out, "  no wall-plane joints found; nothing to do");
        return out;
    }
    for (index, finding) in plan.findings.iter().enumerate() {
        let _ = writeln!(
            out,
            "  finding {index}: {} wall {} / wall {}: {:.3} vs {:.3} ({}), shift {:+.4}, \
             authority {}",
            finding.kind,
            finding.first,
            finding.second,
            finding.first_low,
            finding.second_low,
            finding.axis,
            finding.shift,
            finding.authority,
        );
    }
    for edit in &plan.edits {
        let _ = writeln!(
            out,
            "    edit {}: {} -> {} ({})",
            edit.pointer, edit.old, edit.new, edit.coupled
        );
    }
    for entry in &plan.review {
        let _ = writeln!(
            out,
            "  review {} {}: {}",
            entry.kind, entry.pointer, entry.message
        );
    }
    let _ = writeln!(
        out,
        "  post-check: {} error(s), {} warning(s)",
        plan.post_check.errors, plan.post_check.warnings
    );
    out
}

/// Runs the repair planner and prints its plan.
///
/// # Errors
///
/// Returns a usage or file error, which the caller reports as status `2`.
// This is the CLI's own reporting path; there is no logger in a headless run.
#[allow(clippy::print_stdout)]
pub fn run_repair(options: &RepairCliOptions) -> Result<i32, String> {
    let (plan, _path) = build_repair_plan(options)?;
    let machine = serde_json::to_string_pretty(&plan)
        .map_err(|error| format!("cannot serialize the repair plan: {error}"))?;
    if let Some(path) = &options.plan {
        std::fs::write(path, format!("{machine}\n"))
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    if options.json {
        println!("{machine}");
    } else {
        print!("{}", repair_plan_human(&plan));
    }
    Ok(i32::from(!plan.findings.is_empty()))
}

/// Rewrites `--repair-geometry` runs as the process exit point.
///
/// # Errors
///
/// Returns the usage or IO error, reported as status `2`.
// `std::process::exit` is the correct way for a CLI mode to end the process
// from inside `main`; this one narrow allow keeps the crate-wide `exit` lint
// for every other path.
#[allow(clippy::exit, clippy::print_stderr)]
pub fn repair_main(options: &RepairCliOptions) -> Result<(), Box<dyn std::error::Error>> {
    match run_repair(options) {
        Ok(status) => std::process::exit(status),
        Err(error) => {
            eprintln!("geometry repair: {error}");
            std::process::exit(2);
        }
    }
}

#[cfg(test)]
mod tests {
    // Test code: fixture parsing, exact float compares, unwraps and indexing
    // are idiomatic here (the crate's production lints stay enforced above).
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::indexing_slicing,
        clippy::float_cmp,
        clippy::panic,
        clippy::arithmetic_side_effects,
        clippy::cast_precision_loss,
        clippy::redundant_closure_for_method_calls,
        clippy::suboptimal_flops
    )]

    use super::*;
    use std::collections::BTreeSet;

    use crate::collision::{
        CROUCH_HEIGHT, PLAYER_HEIGHT, PLAYER_RADIUS, highest_support_top, lowest_underside,
        resolve_player_collision, resolve_player_collision_for_body,
    };
    use glam::Vec2;

    /// Parses one fixture the way the CLI does: parse, validate, and run the
    /// preparation pass when the loader accepts it.
    fn fixture(text: &str, name: &str) -> (LevelDef, CheckReport) {
        let mut level = LevelDef::from_json(text).expect("the fixture parses");
        let validated = crate::loader::validate_level(&level).is_ok();
        if validated {
            let catalog = crate::assets::AssetCatalog::load_default();
            crate::loader::prepare_level(&mut level, &catalog, None);
        }
        let report = check_level(&level, name, validated);
        (level, report)
    }

    fn counts(report: &CheckReport) -> BTreeMap<&'static str, (usize, usize)> {
        let mut counts: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();
        for finding in &report.findings {
            let entry = counts.entry(finding.check).or_insert((0, 0));
            if finding.severity == Severity::Error {
                entry.0 += 1;
            } else {
                entry.1 += 1;
            }
        }
        counts
    }

    fn has(report: &CheckReport, check: &str, severity: Severity) -> bool {
        report
            .findings
            .iter()
            .any(|finding| finding.check == check && finding.severity == severity)
    }

    /// A minimal one-room level around the given wall JSON fragments.
    fn joint_level(walls: &[String]) -> LevelDef {
        joint_level_room(
            walls,
            r#"{ "x": -3.0, "z": -1.0, "width": 14.0, "depth": 4.5, "height": 3.0 }"#,
        )
    }

    /// A minimal level around the given wall fragments and explicit room.
    fn joint_level_room(walls: &[String], room: &str) -> LevelDef {
        let walls = walls.join(", ");
        let text = format!(
            r#"{{
                "format_version": 3,
                "id": "joint_fixture",
                "name": "Joint Fixture",
                "author": "Places Team",
                "spawn": {{ "x": 2.0, "z": 1.0, "yaw_degrees": 0.0 }},
                "defaults": {{ "wall": "core:wallpaper_yellow_01", "floor": "core:carpet_beige_01", "ceiling": "core:ceiling_panel_01" }},
                "walls": [{walls}],
                "rooms": [{room}]
            }}"#
        );
        LevelDef::from_json(&text).expect("the joint fixture parses")
    }

    /// An X-axis wall fragment (width is the length, depth the thickness).
    fn x_wall(x: f32, z: f32, width: f32, depth: f32) -> String {
        format!(
            r#"{{ "x": {x}, "z": {z}, "width": {width}, "depth": {depth}, "y": 0.0, "height": 3.0 }}"#
        )
    }

    /// A Z-axis wall fragment (depth is the length, width the thickness).
    fn z_wall(x: f32, z: f32, width: f32, depth: f32) -> String {
        format!(
            r#"{{ "x": {x}, "z": {z}, "width": {width}, "depth": {depth}, "y": 0.0, "height": 3.0 }}"#
        )
    }

    /// One emitted X-axis wall face triangle (normal ±Z).
    fn x_axis_triangle(z: f32, a: [f32; 2], b: [f32; 2], c: [f32; 2], material: u32) -> Tri {
        let points = [[a[0], a[1], z], [b[0], b[1], z], [c[0], c[1], z]];
        let cross = cross3(sub3(points[1], points[0]), sub3(points[2], points[0]));
        Tri {
            key: SurfaceKey::new(SurfaceKind::Wall, material),
            points,
            normal: normalized3(cross),
            centroid: [(a[0] + b[0] + c[0]) / 3.0, (a[1] + b[1] + c[1]) / 3.0, z],
            area: 0.5 * length3(cross),
        }
    }

    /// One emitted X-axis wall face quad as two triangles.
    fn x_axis_quad(z: f32, x0: f32, x1: f32, y0: f32, y1: f32, material: u32) -> Vec<Tri> {
        vec![
            x_axis_triangle(z, [x0, y0], [x1, y0], [x1, y1], material),
            x_axis_triangle(z, [x0, y0], [x1, y1], [x0, y1], material),
        ]
    }

    /// True when a plan point lies inside a convex polygon.
    fn inside_polygon(point: Vec2, polygon: &[(f32, f32)]) -> bool {
        let mut sign = 0.0_f32;
        let len = polygon.len();
        for index in 0..len {
            let (ax, az) = polygon[index];
            let (bx, bz) = polygon[(index + 1) % len];
            let cross = (bx - ax) * (point.y - az) - (bz - az) * (point.x - ax);
            if cross.abs() > 1.0e-6 {
                if sign == 0.0 {
                    sign = cross.signum();
                } else if cross.signum() != sign {
                    return false;
                }
            }
        }
        true
    }

    #[test]
    fn the_broken_fixture_reports_its_planted_defects() {
        let (_, report) = fixture(
            include_str!("../tests/fixtures/levels/geometry_broken.json"),
            "geometry_broken",
        );
        assert!(report.validated, "the broken fixture must still load");
        assert!(
            has(&report, "opening-overlap", Severity::Error),
            "overlapping openings must be a confirmed defect: {:?}",
            counts(&report)
        );
        assert!(
            has(&report, "opening-unused", Severity::Warning),
            "an opening above the wall must be an unused-opening warning: {:?}",
            counts(&report)
        );
        assert!(
            has(&report, "collision-duplicate", Severity::Error),
            "two identical guardrails must duplicate collision: {:?}",
            counts(&report)
        );
        assert!(
            has(&report, "duplicate-surface", Severity::Error),
            "two identical guardrails must duplicate coplanar surfaces: {:?}",
            counts(&report)
        );
        assert!(
            has(&report, "missing-wall", Severity::Warning)
                && has(&report, "room-leak", Severity::Warning),
            "the open west edge must be reported as a missing wall and a leak: {:?}",
            counts(&report)
        );
        assert!(
            has(&report, "curve-coarse", Severity::Warning),
            "a 4-segment pillar must be reported as coarse: {:?}",
            counts(&report)
        );
        assert!(
            has(&report, "spawn-outside-room", Severity::Warning),
            "a spawn in the void must be reported: {:?}",
            counts(&report)
        );
    }

    #[test]
    fn the_intentional_fixture_is_clean() {
        let (level, report) = fixture(
            include_str!("../tests/fixtures/levels/geometry_intentional.json"),
            "geometry_intentional",
        );
        assert!(report.validated);
        assert_eq!(
            report.error_count(),
            0,
            "no confirmed defects expected: {:#?}",
            counts(&report)
        );
        // The only warnings left are the sub-square-centimetre coplanar
        // slivers where perpendicular wall caps meet at the fixture's corners;
        // they are reported, not hidden, and are below the confirmed-defect
        // area. Nothing else may warn.
        for finding in &report.findings {
            assert_eq!(
                finding.check, "coplanar-sliver",
                "unexplained warning: {finding:?}"
            );
            assert!(finding.message.contains("m²"), "{finding:?}");
        }
        assert!(
            report.suppressed.get("missing-wall").copied().unwrap_or(0) > 0,
            "the open bay edge must be suppressed by its annotation"
        );

        // The east room has no authored tile frame, so its vent snaps on the
        // world 1 m panel grid; the west room's frame is rotated and offset.
        let decals = &level.decals;
        assert_eq!(decals.len(), 2);
        assert!((decals[0].x - 12.5).abs() < 1.0e-3, "{:?}", decals[0]);
        assert!((decals[0].z - 3.5).abs() < 1.0e-3, "{:?}", decals[0]);
        assert!((level.ceiling_decal_rotation(&decals[0])).abs() < 1.0e-3);
        // The decal authored on a frame lattice point does not move, and its
        // in-plane rotation composes the room's 90 degree tile rotation.
        assert!((decals[1].x - 3.25).abs() < 1.0e-3, "{:?}", decals[1]);
        assert!((decals[1].z - 3.75).abs() < 1.0e-3, "{:?}", decals[1]);
        assert!((level.ceiling_decal_rotation(&decals[1]) - 90.0).abs() < 1.0e-3);
    }

    #[test]
    fn duplicate_placements_and_dressing_layers_are_reported() {
        let text = r#"{
          "format_version": 3,
          "id": "prop_layers",
          "name": "Prop Layers",
          "author": "Places",
          "spawn": { "x": 0.0, "z": 2.0, "yaw_degrees": 0.0 },
          "defaults": { "wall": "outdoor:house_siding_01", "floor": "outdoor:grass_ground_01",
                        "ceiling": "home:ceiling_white_01" },
          "sky": { "texture": "outdoor:tex_sky_stars_01", "brightness": 1.0, "ambient": 0.2 },
          "rooms": [ { "x": -5.0, "z": -5.0, "width": 10.0, "depth": 10.0, "height": 6.0,
                       "ceiling": { "kind": "open" }, "material": "outdoor:grass_ground_01" } ],
          "floor_regions": [ { "x": -2.0, "z": -2.0, "width": 4.0, "depth": 4.0, "offset_y": 0.14,
                               "material": "outdoor:concrete_pavement_01",
                               "edge_material": "outdoor:concrete_pavement_01" } ],
          "props": [
            { "id": "kit_buried", "model": "outdoor:showcase_sidewalk", "x": 0.0, "z": 0.0,
              "y": -0.141, "rotation_degrees": 90.0 },
            { "id": "crate_a", "model": "core:crate", "x": 3.0, "z": 3.0 },
            { "id": "crate_b", "model": "core:crate", "x": 3.0, "z": 3.0 }
          ]
        }"#;
        let (_, report) = fixture(text, "prop_layers");
        assert!(report.validated);
        assert!(
            has(&report, "prop-layer-coplanar", Severity::Warning),
            "a kit layer one millimetre under a raised floor must warn: {:?}",
            counts(&report)
        );
        assert!(
            has(&report, "prop-duplicate", Severity::Warning),
            "two identical transforms must warn: {:?}",
            counts(&report)
        );
    }

    #[test]
    fn the_shipped_demo_reports_only_the_confirmed_wall_step() {
        let (level, report) = fixture(
            include_str!("../assets/levels/places_demo.json"),
            "places_demo",
        );
        assert!(report.validated);
        // Job 06's confirmed defect: wall 37 is half a thickness off the
        // corridor's coplanar chain (walls 16/19/31). Agent C's source repair
        // removes the joint; this test accepts either state and never any
        // other confirmed defect.
        let joints = wall_joints(&level);
        if !joints.is_empty() {
            assert_eq!(joints.len(), 1, "{joints:#?}");
            let joint = &joints[0];
            assert_eq!((joint.first, joint.second), (31, 37), "{joint:#?}");
            assert_eq!(joint.kind, "step");
            assert!(joint.auto_repairable, "{joint:#?}");
            assert!((joint.shift - 0.15).abs() <= 1.0e-4, "{joint:#?}");
            assert_eq!(joint.authority, "chain");
        }
        for finding in &report.findings {
            if finding.severity == Severity::Error {
                assert_eq!(
                    finding.check, "wall-joint-step",
                    "unexpected confirmed defect: {finding:?}"
                );
            }
        }
        assert_eq!(
            report.error_count(),
            joints.len(),
            "one error per wall joint: {:#?}",
            counts(&report)
        );
    }

    #[test]
    fn a_rigid_wall_step_is_an_auto_repairable_error() {
        let mut level = joint_level(&[
            x_wall(1.0, 2.15, 4.0, 0.3),
            x_wall(5.0, 2.0, 4.0, 0.3),
            x_wall(-2.85, 2.15, 3.85, 0.3),
        ]);
        let joints = wall_joints(&level);
        assert_eq!(joints.len(), 1, "{joints:#?}");
        let joint = &joints[0];
        assert_eq!((joint.first, joint.second), (0, 1), "{joint:#?}");
        assert_eq!(joint.kind, "step");
        assert_eq!(joint.axis, "x");
        assert!(joint.auto_repairable, "{joint:#?}");
        assert_eq!(joint.authority, "chain");
        assert_eq!((joint.first_support, joint.second_support), (1, 0));
        assert!((joint.first_low - 2.15).abs() <= 1.0e-4, "{joint:#?}");
        assert!((joint.first_high - 2.45).abs() <= 1.0e-4, "{joint:#?}");
        assert!((joint.second_low - 2.0).abs() <= 1.0e-4, "{joint:#?}");
        assert!((joint.second_high - 2.3).abs() <= 1.0e-4, "{joint:#?}");
        assert!((joint.shift - 0.15).abs() <= 1.0e-4, "{joint:#?}");
        assert_eq!(joint.gap_m, 0.0);
        assert_eq!(joint.overlap_m, 0.0);
        assert!((joint.y_overlap_m - 3.0).abs() <= 1.0e-4, "{joint:#?}");
        assert!((joint.position[0] - 5.0).abs() <= 1.0e-3, "{joint:#?}");

        let report = check_level(&level, "joint_fixture", true);
        assert!(
            has(&report, "wall-joint-step", Severity::Error),
            "{:#?}",
            report.findings
        );
        assert!(
            !has(&report, "wall-joint-emitted-mismatch", Severity::Error),
            "a step whose faces are emitted is not a generator defect: {:#?}",
            report.findings
        );
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.check == "wall-joint-step")
            .expect("the step finding");
        assert_eq!(finding.id, "wall 0 / wall 1");
        assert!(
            finding.message.contains("walls[1].z"),
            "{}",
            finding.message
        );
        assert!(finding.message.contains("+0.1500"), "{}", finding.message);
        assert!(finding.message.contains("authority"), "{}", finding.message);
        assert!(
            finding.message.contains("chain support 1 vs 0"),
            "{}",
            finding.message
        );

        // The classifier's own shift repairs the joint exactly.
        let wall = level.walls.get_mut(1).expect("the mover wall");
        wall.z += joint.shift;
        assert!(wall_joints(&level).is_empty(), "{:#?}", wall_joints(&level));
        let repaired = check_level(&level, "joint_fixture", true);
        assert_eq!(repaired.error_count(), 0, "{:#?}", repaired.findings);
    }

    #[test]
    fn the_authority_can_be_either_side() {
        // The mover has the lower index here: the authority is wall 1 and the
        // delta moves wall 0 onto its plane.
        let level = joint_level(&[
            x_wall(5.0, 2.0, 4.0, 0.3),
            x_wall(1.0, 2.15, 4.0, 0.3),
            x_wall(-2.85, 2.15, 3.85, 0.3),
        ]);
        let joints = wall_joints(&level);
        assert_eq!(joints.len(), 1, "{joints:#?}");
        let joint = &joints[0];
        assert_eq!((joint.first, joint.second), (1, 0), "{joint:#?}");
        assert!((joint.shift - 0.15).abs() <= 1.0e-4, "{joint:#?}");
        assert!(joint.auto_repairable, "{joint:#?}");
        let report = check_level(&level, "joint_fixture", true);
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.check == "wall-joint-step")
            .expect("the step finding");
        assert!(
            finding.message.contains("walls[0].z"),
            "{}",
            finding.message
        );
    }

    #[test]
    fn a_doorway_lintel_participates_in_the_joint() {
        let lintel_wall =
            r#"{ "x": 1.0, "z": 2.0, "width": 5.0, "depth": 0.3, "y": 0.0, "height": 3.0,
            "openings": [ { "kind": "door", "offset": 3.0, "width": 2.0, "height": 2.1 } ] }"#
                .to_string();
        let level = joint_level(&[lintel_wall, x_wall(6.0, 2.15, 4.0, 0.3)]);
        let joints = wall_joints(&level);
        assert_eq!(joints.len(), 1, "{joints:#?}");
        let joint = &joints[0];
        assert_eq!(joint.kind, "step");
        assert_eq!(joint.authority, "length", "{joint:#?}");
        assert_eq!((joint.first, joint.second), (0, 1), "{joint:#?}");
        // The authority (the lintel wall, 5.0 m solid against 4.0 m) keeps
        // z 2.000; the shorter wall is proud at 2.150 and moves back.
        assert!((joint.shift + 0.15).abs() <= 1.0e-4, "{joint:#?}");
        assert!((joint.y_overlap_m - 0.9).abs() <= 1.0e-3, "{joint:#?}");
        assert!((joint.position[0] - 6.0).abs() <= 1.0e-3, "{joint:#?}");
        let report = check_level(&level, "joint_fixture", true);
        assert!(
            has(&report, "wall-joint-step", Severity::Error),
            "{:#?}",
            report.findings
        );
        assert!(
            !has(&report, "wall-joint-emitted-mismatch", Severity::Error),
            "the lintel's own faces are emitted: {:#?}",
            report.findings
        );
    }

    #[test]
    fn a_coplanar_gap_is_a_review_warning_and_a_covered_gap_is_not() {
        let gap = joint_level(&[x_wall(1.0, 2.0, 4.0, 0.3), x_wall(5.25, 2.0, 4.0, 0.3)]);
        let joints = wall_joints(&gap);
        assert_eq!(joints.len(), 1, "{joints:#?}");
        let joint = &joints[0];
        assert_eq!(joint.kind, "near-step");
        assert_eq!(joint.shift, 0.0);
        assert!(!joint.auto_repairable);
        assert!((joint.gap_m - 0.25).abs() <= 1.0e-4, "{joint:#?}");
        let report = check_level(&gap, "joint_fixture", true);
        assert!(
            has(&report, "wall-joint-step-review", Severity::Warning),
            "{:#?}",
            report.findings
        );
        assert_eq!(report.error_count(), 0, "{:#?}", report.findings);
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.check == "wall-joint-step-review")
            .expect("the review finding");
        assert!(finding.message.contains("0.250"), "{}", finding.message);
        assert!(finding.message.contains("gap"), "{}", finding.message);

        // The same geometry with the gap already solid behind it (a lintel
        // meeting the wall body) is not a doorway drawn as two walls.
        let covered_wall =
            r#"{ "x": 1.0, "z": 2.0, "width": 3.0, "depth": 0.3, "y": 0.0, "height": 3.0,
            "openings": [ { "kind": "door", "offset": 2.0, "width": 1.0, "height": 2.1 } ] }"#
                .to_string();
        let covered = joint_level(&[covered_wall, x_wall(3.25, 2.0, 3.0, 0.3)]);
        assert!(
            wall_joints(&covered).is_empty(),
            "{:#?}",
            wall_joints(&covered)
        );
        let report = check_level(&covered, "joint_fixture", true);
        assert!(
            !report
                .findings
                .iter()
                .any(|finding| finding.check.starts_with("wall-joint")),
            "{:#?}",
            report.findings
        );
    }

    #[test]
    fn a_correct_shared_edge_and_a_perpendicular_corner_are_not_joints() {
        let shared = joint_level(&[
            r#"{ "x": 1.0, "z": 2.0, "width": 4.0, "depth": 0.3, "y": 0.0, "height": 3.0, "material": "core:wallpaper_yellow_01" }"#.to_string(),
            r#"{ "x": 5.0, "z": 2.0, "width": 4.0, "depth": 0.3, "y": 0.0, "height": 3.0, "material": "home:wallpaper_offwhite_01" }"#.to_string(),
        ]);
        assert!(
            wall_joints(&shared).is_empty(),
            "{:#?}",
            wall_joints(&shared)
        );
        let report = check_level(&shared, "joint_fixture", true);
        assert!(
            !report
                .findings
                .iter()
                .any(|finding| finding.check.starts_with("wall-joint")),
            "{:#?}",
            report.findings
        );

        let corner = joint_level(&[x_wall(1.0, 2.0, 4.0, 0.3), z_wall(5.0, 2.0, 0.3, 4.0)]);
        assert!(
            wall_joints(&corner).is_empty(),
            "{:#?}",
            wall_joints(&corner)
        );
    }

    #[test]
    fn a_thickness_transition_is_review_only() {
        let level = joint_level(&[x_wall(1.0, 2.0, 4.0, 0.3), x_wall(5.0, 2.0, 4.0, 0.2)]);
        let joints = wall_joints(&level);
        assert_eq!(joints.len(), 1, "{joints:#?}");
        let joint = &joints[0];
        assert_eq!(joint.kind, "thickness-step");
        assert!(!joint.auto_repairable);
        assert_eq!(joint.shift, 0.0);
        let report = check_level(&level, "joint_fixture", true);
        assert!(
            has(&report, "wall-joint-thickness-step", Severity::Warning),
            "{:#?}",
            report.findings
        );
        assert_eq!(report.error_count(), 0, "{:#?}", report.findings);
        let finding = report
            .findings
            .iter()
            .find(|finding| finding.check == "wall-joint-thickness-step")
            .expect("the review finding");
        assert!(
            finding.message.contains("thickness 0.300"),
            "{}",
            finding.message
        );
        assert!(finding.message.contains("0.200"), "{}", finding.message);
    }

    #[test]
    fn ambiguous_authority_and_oversized_shifts_are_review_only() {
        let ambiguous = joint_level(&[x_wall(1.0, 2.0, 4.0, 0.3), x_wall(5.0, 2.15, 4.0, 0.3)]);
        let joints = wall_joints(&ambiguous);
        assert_eq!(joints.len(), 1, "{joints:#?}");
        assert_eq!(joints[0].kind, "near-step");
        assert_eq!(joints[0].authority, "index");
        assert!(!joints[0].auto_repairable);
        let report = check_level(&ambiguous, "joint_fixture", true);
        assert!(
            has(&report, "wall-joint-step-review", Severity::Warning),
            "{:#?}",
            report.findings
        );
        assert_eq!(report.error_count(), 0, "{:#?}", report.findings);

        let oversized = joint_level(&[x_wall(1.0, 2.0, 4.0, 0.3), x_wall(5.0, 2.28, 4.0, 0.3)]);
        let joints = wall_joints(&oversized);
        assert_eq!(joints.len(), 1, "{joints:#?}");
        assert_eq!(joints[0].kind, "near-step");
        assert!(!joints[0].auto_repairable);
        assert!(
            (joints[0].shift.abs() - 0.28).abs() <= 1.0e-4,
            "{joints:#?}"
        );

        // Past the near-adjacency bound the pair is not a joint at all.
        let far = joint_level(&[x_wall(1.0, 2.0, 4.0, 0.3), x_wall(5.0, 2.5, 4.0, 0.3)]);
        assert!(wall_joints(&far).is_empty(), "{:#?}", wall_joints(&far));
    }

    #[test]
    fn large_coordinates_and_small_features_keep_the_classification() {
        let level = joint_level_room(
            &[
                x_wall(2001.0, 2002.15, 4.0, 0.3),
                x_wall(2005.0, 2002.0, 4.0, 0.3),
                x_wall(1997.15, 2002.15, 3.85, 0.3),
            ],
            r#"{ "x": 1997.0, "z": 2001.0, "width": 14.0, "depth": 4.5, "height": 3.0 }"#,
        );
        let joints = wall_joints(&level);
        assert_eq!(joints.len(), 1, "{joints:#?}");
        assert!((joints[0].shift - 0.15).abs() <= 1.0e-4, "{joints:#?}");
        assert!(
            (joints[0].position[0] - 2005.0).abs() <= 1.0e-3,
            "{joints:#?}"
        );

        let small = joint_level(&[
            x_wall(0.9, 2.0, 0.1, 0.1),
            x_wall(1.0, 2.0, 0.1, 0.1),
            x_wall(1.1, 2.15, 0.1, 0.1),
        ]);
        let joints = wall_joints(&small);
        assert_eq!(joints.len(), 1, "{joints:#?}");
        assert_eq!(joints[0].kind, "step", "{joints:#?}");
        assert!(
            (joints[0].shift.abs() - 0.15).abs() <= 1.0e-4,
            "{joints:#?}"
        );
        let report = check_level(&small, "joint_fixture", true);
        assert!(
            has(&report, "wall-joint-step", Severity::Error),
            "{:#?}",
            report.findings
        );
    }

    #[test]
    fn the_plane_tolerance_boundary_is_one_millimetre() {
        // `JOINT_PLANE_TOL_M` is inclusive for "the same plane": 1.0 mm is a
        // correct shared edge (no finding), while 1.1 mm is a rigid step. Both
        // levels carry the same chain supporter so only the boundary moves.
        let within = joint_level(&[
            x_wall(1.0, 3.0, 4.0, 0.3),
            x_wall(5.0, 3.001, 4.0, 0.3),
            x_wall(-2.85, 3.0, 3.85, 0.3),
        ]);
        assert!(
            wall_joints(&within).is_empty(),
            "{:#?}",
            wall_joints(&within)
        );
        let report = check_level(&within, "joint_fixture", true);
        assert!(
            !report
                .findings
                .iter()
                .any(|finding| finding.check.starts_with("wall-joint")),
            "1.0 mm is the same plane: {:#?}",
            report.findings
        );

        let beyond = joint_level(&[
            x_wall(1.0, 3.0, 4.0, 0.3),
            x_wall(5.0, 3.0011, 4.0, 0.3),
            x_wall(-2.85, 3.0, 3.85, 0.3),
        ]);
        let joints = wall_joints(&beyond);
        assert_eq!(joints.len(), 1, "{joints:#?}");
        let joint = &joints[0];
        assert_eq!(joint.kind, "step", "{joint:#?}");
        assert!(joint.auto_repairable, "{joint:#?}");
        assert_eq!(joint.authority, "chain", "{joint:#?}");
        assert!(
            (joint.shift.abs() - 0.0011).abs() <= 1.0e-5,
            "1.1 mm must survive as a step: {joint:#?}"
        );
        let report = check_level(&beyond, "joint_fixture", true);
        assert!(
            has(&report, "wall-joint-step", Severity::Error),
            "{:#?}",
            report.findings
        );
    }

    #[test]
    fn water_basin_skirts_do_not_duplicate_the_divider_faces() {
        let (mut level, report) = fixture(
            include_str!("../tests/fixtures/levels/water_transmission.json"),
            "water_transmission",
        );
        assert!(report.validated);
        assert!(
            !has(&report, "duplicate-surface", Severity::Error),
            "basin skirts already close the divider below the deck: {:#?}",
            report.findings
        );

        // Recreate the observed fault: a second face covers the entire skirt.
        let divider = level.walls.get_mut(4).expect("the basin divider");
        divider.y = -1.2;
        divider.height = Some(4.4);
        let overlapping = check_level(&level, "overlapping_water_divider", true);
        assert!(has(&overlapping, "duplicate-surface", Severity::Error));
    }

    #[test]
    fn rendering_diagnostic_panels_do_not_overlap_the_base_wall() {
        let text = include_str!("../tests/fixtures/levels/rendering_diagnostic.json");
        let (_, report) = fixture(text, "rendering_diagnostic");
        assert!(report.validated);
        assert_eq!(report.error_count(), 0, "{:#?}", report.findings);

        // The original continuous wall also emitted trim behind both panels.
        let mut level = LevelDef::from_json(text).expect("the fixture parses");
        level.walls.drain(1..3);
        level.walls.first_mut().expect("the north wall").width = 31.2;
        let catalog = crate::assets::AssetCatalog::load_default();
        crate::loader::prepare_level(&mut level, &catalog, None);
        let overlapping = check_level(&level, "overlapping_rendering_panels", true);
        assert!(has(&overlapping, "duplicate-surface", Severity::Error));
    }

    #[test]
    fn the_maintained_clean_levels_have_no_wall_joints() {
        for (name, text) in [
            (
                "geometry_intentional",
                include_str!("../tests/fixtures/levels/geometry_intentional.json"),
            ),
            ("model_zoo", include_str!("../assets/levels/model_zoo.json")),
            (
                "home_showcase",
                include_str!("../tests/fixtures/levels/home_showcase.json"),
            ),
            (
                "level0_pit",
                include_str!("../tests/fixtures/levels/level0_pit.json"),
            ),
        ] {
            let level = LevelDef::from_json(text).expect(name);
            let joints = wall_joints(&level);
            assert!(joints.is_empty(), "{name}: {joints:#?}");
        }
    }

    #[test]
    fn the_emitted_matcher_flags_a_missing_or_undeclared_joint_face() {
        let level = joint_level(&[x_wall(1.0, 2.15, 4.0, 0.3), x_wall(5.0, 2.0, 4.0, 0.3)]);
        let surfaces = LevelSurfaces::new(&level);
        let candidates = compute_wall_joints(&level, &surfaces);
        assert_eq!(candidates.len(), 1, "{candidates:#?}");
        let candidate = &candidates[0];
        let walls = level_wall_slices(&level, &surfaces);
        let materials = crate::render::logical_materials(&level);
        let material = materials
            .index_of("core:wallpaper_yellow_01")
            .expect("the default wall material resolves");

        // Every declared plane is present except wall 1's low plane.
        let mut triangles = Vec::new();
        triangles.extend(x_axis_quad(2.15, 1.0, 5.0, 0.0, 3.0, material));
        triangles.extend(x_axis_quad(2.45, 1.0, 5.0, 0.0, 3.0, material));
        triangles.extend(x_axis_quad(2.3, 5.0, 9.0, 0.0, 3.0, material));
        let index = WallTriangleIndex::build(&triangles);
        let mut checker = Checker::new(&level);
        checker.check_joint_emitted(candidate, &walls, &index, &triangles, &materials);
        assert_eq!(checker.findings.len(), 1, "{:#?}", checker.findings);
        assert_eq!(checker.findings[0].check, "wall-joint-emitted-mismatch");
        assert!(
            checker.findings[0].message.contains("2.000"),
            "{}",
            checker.findings[0].message
        );

        // The complete mesh has no mismatch.
        triangles.extend(x_axis_quad(2.0, 5.0, 9.0, 0.0, 3.0, material));
        let index = WallTriangleIndex::build(&triangles);
        let mut checker = Checker::new(&level);
        checker.check_joint_emitted(candidate, &walls, &index, &triangles, &materials);
        assert!(checker.findings.is_empty(), "{:#?}", checker.findings);

        // A face at a plane the source never declares is a generator defect.
        triangles.extend(x_axis_quad(2.08, 4.9, 5.1, 0.25, 2.75, material));
        let index = WallTriangleIndex::build(&triangles);
        let mut checker = Checker::new(&level);
        checker.check_joint_emitted(candidate, &walls, &index, &triangles, &materials);
        assert_eq!(checker.findings.len(), 1, "{:#?}", checker.findings);
        assert!(
            checker.findings[0].message.contains("2.080"),
            "{}",
            checker.findings[0].message
        );
    }

    #[test]
    fn the_planner_repairs_the_fixture_with_the_coupled_edits() {
        let options = RepairCliOptions {
            level: "tests/fixtures/levels/repair/wall_step_x.json".to_string(),
            plan: None,
            json: false,
        };
        let (plan, _path) = build_repair_plan(&options).expect("the fixture plans");
        assert_eq!(plan.findings.len(), 1, "{:#?}", plan.findings);
        let pointers: Vec<&str> = plan
            .edits
            .iter()
            .map(|edit| edit.pointer.as_str())
            .collect();
        assert_eq!(
            pointers,
            [
                "/walls/1/z",
                "/baseboards/0/z",
                "/baseboards/1/length",
                "/floor_regions/0/depth",
            ],
            "{:#?}",
            plan.edits
        );
        assert_eq!(plan.edits[0].old, 2.0);
        assert_eq!(plan.edits[0].new, 2.15);
        assert_eq!(plan.edits[3].old, 1.55);
        assert_eq!(plan.edits[3].new, 1.7);
        assert!(plan.review.is_empty(), "{:#?}", plan.review);
        assert_eq!(plan.post_check.errors, 0);

        // Review-only findings plan no edits at all.
        let review_options = RepairCliOptions {
            level: "tests/fixtures/levels/repair/gap_review.json".to_string(),
            plan: None,
            json: false,
        };
        let (plan, _path) = build_repair_plan(&review_options).expect("the gap fixture plans");
        assert_eq!(plan.findings.len(), 1, "{:#?}", plan.findings);
        assert!(plan.findings[0].kind == "near-step", "{:#?}", plan.findings);
        assert!(!plan.findings[0].auto_repairable, "{:#?}", plan.findings);
        assert!(plan.edits.is_empty(), "{:#?}", plan.edits);
    }

    #[test]
    fn repair_arguments_are_stable() {
        let args: Vec<String> = [
            "--repair-geometry",
            "--level",
            "places_demo",
            "--plan",
            "target/agent-work/plan.json",
            "--json",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        let options = repair_options_from_args(&args)
            .expect("parses")
            .expect("repair mode");
        assert_eq!(options.level, "places_demo");
        assert!(options.json);
        assert_eq!(
            options.plan.as_deref(),
            Some(std::path::Path::new("target/agent-work/plan.json"))
        );
        assert!(
            repair_options_from_args(&["--level".to_string()])
                .expect("no mode")
                .is_none()
        );
        assert!(
            repair_options_from_args(&["--repair-geometry".to_string(), "--bogus".to_string()])
                .is_err()
        );
        assert!(
            options_from_args(&args)
                .expect("the checker must not claim a repair run")
                .is_none()
        );
    }

    #[test]
    fn the_invalid_fixture_is_rejected_and_the_checker_still_names_the_problems() {
        let text = include_str!("../tests/fixtures/levels/invalid/geometry_invalid.json");
        let (_, report) = fixture(text, "geometry_invalid");
        assert!(!report.validated, "the loader must reject the fixture");
        assert!(
            has(&report, "curved-invalid", Severity::Error),
            "every degenerate curve must be named: {:#?}",
            counts(&report)
        );
        let messages: Vec<&str> = report
            .findings
            .iter()
            .filter(|finding| finding.check == "curved-invalid")
            .map(|finding| finding.message.as_str())
            .collect();
        assert!(
            messages.iter().any(|message| message.contains("thinner")),
            "the thickness diagnostic must name the constraint: {messages:?}"
        );
        assert!(
            messages.iter().any(|message| message.contains("sweep")),
            "the sweep diagnostic must name the constraint: {messages:?}"
        );
        assert!(
            messages.iter().any(|message| message.contains("radius")),
            "the radius diagnostic must name the constraint: {messages:?}"
        );
    }

    #[test]
    fn curved_primitives_generate_tight_collision_and_material_variety() {
        let (level, _) = fixture(
            include_str!("../tests/fixtures/levels/geometry_intentional.json"),
            "geometry_intentional",
        );
        let surfaces = LevelSurfaces::new(&level);
        let materials = crate::render::logical_materials(&level);

        // Pillar collision follows the drawn polygon.
        let pillar = &level.pillars[0];
        let boxes = pillar.collision_boxes(&surfaces);
        assert!(
            boxes.len() >= 8,
            "a 24-segment pillar should decompose into several rows, got {}",
            boxes.len()
        );
        // The ring decomposition: one box per rendered segment sub-step per
        // radial band, so no box spans a row of the plan. A 24-segment pillar's
        // 15-degree segments split into 3-degree sub-steps, which keeps every
        // diagonal corner within a few percent of the radius.
        let segments = usize::try_from(pillar.resolved_segments()).unwrap_or(0);
        let ring_boxes = segments.saturating_mul(crate::level::PILLAR_COLLISION_BANDS);
        assert!(ring_boxes > 0, "the fixture pillar resolves segments");
        assert!(
            boxes.len() >= ring_boxes,
            "one ring box per rendered segment per band at least, got {}",
            boxes.len()
        );
        assert_eq!(
            boxes.len() % ring_boxes,
            0,
            "the collision ring is a whole number of boxes per segment"
        );
        let full_square = (pillar.radius * 2.0) * (pillar.radius * 2.0);
        for boxed in &boxes {
            let area = (boxed.max[0] - boxed.min[0]) * (boxed.max[2] - boxed.min[2]);
            assert!(
                area < full_square * 0.6,
                "no collision row may be a square around the whole pillar (area {area:.3} of {full_square:.3})"
            );
            // A round-plan box is a narrow wedge: it never reaches across the
            // disc on *both* axes, which is what keeps the baked shadow round.
            let width = boxed.max[0] - boxed.min[0];
            let depth = boxed.max[2] - boxed.min[2];
            assert!(
                width.min(depth) <= pillar.radius * 0.6,
                "a ring box must be a wedge, not a row: {width:.3} x {depth:.3}"
            );
            for corner in footprint_corners(boxed) {
                let distance = (corner[0] - pillar.x).hypot(corner[1] - pillar.z);
                assert!(
                    distance <= pillar.radius + 0.05,
                    "a collision corner reaches {distance:.3} m, past the {:.2} m radius",
                    pillar.radius
                );
            }
        }
        assert!(
            covered_by_boxes(&boxes, pillar.x, 1.0, pillar.z),
            "the ring must cover the centre so the cap stands on it"
        );
        for (x, z) in pillar.polygon_points() {
            assert!(
                covered_by_boxes(&boxes, x, 1.0, z),
                "the drawn polygon vertex ({x:.2}, {z:.2}) must be collision-covered"
            );
        }

        // Arc-wall collision is one tight box per rendered segment.
        let arc = &level.arc_walls[0];
        let arc_boxes = arc.collision_boxes(&surfaces);
        assert_eq!(
            arc_boxes.len(),
            usize::try_from(arc.resolved_segments()).unwrap() * crate::level::ARC_COLLISION_STEPS,
            "one collision box per rendered arc segment sub-step"
        );

        // Material variety: the fixture's curves together resolve several
        // distinct wall materials in the emitted mesh.
        let mesh = crate::render::build_level_geometry_with_materials(&level, &materials);
        let used: BTreeSet<u32> = mesh
            .ranges
            .iter()
            .filter(|range| range.key.kind == SurfaceKind::Wall)
            .map(|range| range.key.material)
            .collect();
        for id in [
            "core:pool_tile_wall_01",
            "core:metal_brushed_01",
            "core:linoleum_polished_01",
            "core:baseboard_office_01",
            "core:wallpaper_stained_01",
        ] {
            let index = materials.index_of(id).expect(id);
            assert!(
                used.contains(&index),
                "{id} must reach the mesh through a curve face"
            );
        }
    }

    #[test]
    fn walking_crouching_and_landing_at_curved_surfaces() {
        let (level, _) = fixture(
            include_str!("../tests/fixtures/levels/geometry_intentional.json"),
            "geometry_intentional",
        );
        let surfaces = LevelSurfaces::new(&level);
        let boxes = level.collision_aabbs();
        let pillar = &level.pillars[0];
        let polygon = pillar.polygon_points();
        let centre = Vec2::new(pillar.x, pillar.z);

        // Walking: from eight directions the resolved centre never ends up
        // inside the drawn polygon, and the body is pushed back.
        for step in 0..8 {
            let angle = std::f32::consts::TAU * (step as f32 / 8.0);
            let direction = Vec2::new(angle.cos(), angle.sin());
            let start = centre + direction * (pillar.radius + 1.0);
            let resolved = resolve_player_collision(start, PLAYER_RADIUS, 0.0, &boxes);
            assert!(
                !inside_polygon(resolved, &polygon),
                "walking from {start:?} resolved inside the pillar at {resolved:?}"
            );
            assert!(
                resolved.distance(start) < 0.5,
                "the controller must stop at the pillar, not teleport"
            );
        }

        // Crouching changes nothing about a full-height pillar except that the
        // body is shorter; it still cannot enter the drawn polygon.
        let start = centre + Vec2::new(1.0, 0.0);
        let resolved =
            resolve_player_collision_for_body(start, PLAYER_RADIUS, 0.0, CROUCH_HEIGHT, &boxes);
        assert!(!inside_polygon(resolved, &polygon));

        // Jumping: the low pillar's top is the landing surface over its whole
        // polygon.
        let low = &level.pillars[1];
        let top = low.top_y(&surfaces);
        let landing = highest_support_top(low.x, low.z, top + 0.2, &boxes);
        assert_eq!(landing, Some(top));

        // A raised arc wall is a header: a standing body is blocked, a
        // crouched one passes, and head collision resolves its underside.
        let raised = LevelDef::from_json(
            r#"{
                "format_version": 3,
                "id": "raised_arc",
                "name": "Raised Arc",
                "spawn": { "x": 1.0, "z": 1.0 },
                "rooms": [ { "x": 0.0, "z": 0.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
                "arc_walls": [
                    { "x": 4.0, "z": 4.0, "radius": 2.0, "thickness": 0.3,
                      "start_degrees": 90.0, "sweep_degrees": 90.0,
                      "y": 1.5, "height": 0.6, "material": "core:metal_brushed_01" }
                ],
                "ceiling_lights": [
                    { "fixture": "core:fluorescent_panel_01", "x": 4.0, "z": 4.0 }
                ]
            }"#,
        )
        .expect("the raised arc level parses");
        let raised_boxes = raised.collision_aabbs();
        assert!(
            raised_boxes
                .iter()
                .any(|boxed| boxed.min_y >= 1.49 && boxed.max_y <= 2.11),
            "the raised arc must collide from 1.5 m up"
        );
        let head = lowest_underside(5.4, 5.4, PLAYER_RADIUS, 0.0, &raised_boxes);
        assert_eq!(
            head,
            Some(1.5),
            "head collision must resolve the arc underside"
        );
        let inside_arc = Vec2::new(5.6, 4.8);
        let standing = resolve_player_collision_for_body(
            inside_arc,
            PLAYER_RADIUS,
            0.0,
            PLAYER_HEIGHT,
            &raised_boxes,
        );
        assert_ne!(
            standing, inside_arc,
            "standing under the header must be blocked"
        );
        let crouched = resolve_player_collision_for_body(
            inside_arc,
            PLAYER_RADIUS,
            0.0,
            CROUCH_HEIGHT,
            &raised_boxes,
        );
        assert_eq!(
            crouched, inside_arc,
            "a crouched body must pass under the header"
        );
    }

    #[test]
    fn checker_arguments_and_reports_are_stable() {
        let args: Vec<String> = [
            "--check-geometry",
            "--level",
            "levels/level0_pit.json",
            "--json",
            "target/agent-work/pit.json",
            "--strict",
            "--quiet",
        ]
        .iter()
        .map(ToString::to_string)
        .collect();
        let options = options_from_args(&args)
            .expect("parses")
            .expect("checker mode");
        assert_eq!(options.level, "levels/level0_pit.json");
        assert!(options.strict);
        assert!(!options.verbose);
        assert!(
            options_from_args(&["--level".to_string()])
                .expect("no mode")
                .is_none()
        );
        assert!(
            options_from_args(&["--check-geometry".to_string(), "--bogus".to_string()]).is_err()
        );

        let report = CheckReport {
            source: "fixture".to_string(),
            level_id: "fixture".to_string(),
            validated: true,
            findings: vec![Finding {
                check: "degenerate-face",
                severity: Severity::Error,
                id: "wall 0".to_string(),
                message: "zero-area triangle".to_string(),
                position: [1.0, 2.0, 3.0],
            }],
            suppressed: BTreeMap::new(),
        };
        let json = report.to_json().expect("serializes");
        assert!(json.contains(CHECK_FORMAT));
        assert!(json.contains("\"errors\": 1"));
        assert_eq!(report.exit_status(false), 1);
        assert_eq!(report.markers().len(), 1);
        let obj = marker_obj(&report.markers());
        assert!(
            obj.contains("\nl "),
            "the marker OBJ must carry line elements"
        );
    }
}
