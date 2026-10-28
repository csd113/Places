//! Read-only geometry repair planning and its mandatory post-check safety gate.

use std::fmt::Write as _;

use super::{
    BTreeMap, CheckReport, LevelDef, LevelSurfaces, PathBuf, Serialize, Severity, WallAxis,
    WallJoint, WallJointCandidate, check_level, compute_wall_joints, quantise, resolve_source,
    sha256_hex, wall_axis_of,
};

/// Stable format id written into the machine-readable repair plan.
pub const REPAIR_FORMAT: &str = "places-geometry-repair-plan";
/// Repair plan schema version.
pub const REPAIR_VERSION: u32 = 1;

/// The CLI options of `--repair-geometry`.
#[derive(Clone, Debug, Default)]
pub struct RepairCliOptions {
    /// Level path or id (`places_demo`, `levels/places_demo.json`, ...).
    pub level: String,
    /// Write the machine-readable plan here, in addition to the human one.
    pub plan: Option<PathBuf>,
    /// Print the machine-readable plan to stdout instead of the human plan.
    pub json: bool,
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
    if options.level.trim().is_empty() {
        return Err("--level needs an explicit source path or installed source id".to_string());
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
            }
        }
        true
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
#[expect(
    clippy::too_many_lines,
    reason = "one inventory of every placeable family"
)] // one inventory of every placeable family
fn element_footprints(level: &LevelDef) -> Vec<ElementFootprint> {
    let mut out = Vec::new();
    for (index, prop) in level.props.iter().enumerate() {
        let bounds = prop.placement_bounds(
            0.0,
            prop.size
                .unwrap_or(crate::engine::level::PROP_FALLBACK_SIZE),
        );
        out.push(ElementFootprint {
            label: format!("prop {index} ({})", prop.model),
            pointer: format!("/props/{index}"),
            x: (bounds.min[0], bounds.max[0]),
            z: (bounds.min[2], bounds.max[2]),
            y: None,
        });
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
            .unwrap_or(crate::engine::level::DEFAULT_CEILING_HEIGHT_M);
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
#[expect(
    clippy::too_many_lines,
    reason = "one linear pass over the coupled element rules"
)] // one linear pass over the coupled element rules
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
fn board_touches(board: &crate::engine::level::BaseboardDef, mover: &MoverGeometry) -> bool {
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
    catalog: &'a crate::engine::assets::AssetCatalog,
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
        crate::map_compiler::prepare_level(&mut prepared, self.catalog, None);
        if let Err(error) = crate::engine::loader::validate_level(&prepared) {
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
        let step_gone = !after.findings.iter().any(|candidate_finding| {
            candidate_finding.check == "wall-joint-step" && candidate_finding.id == pair_id
        });
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
    if let Err(error) = crate::engine::loader::validate_level(&authored) {
        return Err(format!("{} does not validate: {error}", path.display()));
    }
    let catalog = crate::engine::assets::AssetCatalog::load_default();
    let source = path.display().to_string();
    let mut prepared = authored.clone();
    crate::map_compiler::prepare_level(&mut prepared, &catalog, None);
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
    crate::map_compiler::prepare_level(&mut final_prepared, &catalog, None);
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
    let _formatted_geometry_repair = writeln!(
        out,
        "geometry repair plan: {} ({})",
        plan.level.id, plan.level.source
    );
    if plan.findings.is_empty() {
        let _formatted_no_wall = writeln!(out, "  no wall-plane joints found; nothing to do");
        return out;
    }
    for (index, finding) in plan.findings.iter().enumerate() {
        let _formatted_text = writeln!(
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
        let _formatted_edit = writeln!(
            out,
            "    edit {}: {} -> {} ({})",
            edit.pointer, edit.old, edit.new, edit.coupled
        );
    }
    for entry in &plan.review {
        let _formatted_review = writeln!(
            out,
            "  review {} {}: {}",
            entry.kind, entry.pointer, entry.message
        );
    }
    let _formatted_post_check = writeln!(
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
#[expect(
    clippy::print_stdout,
    reason = "This is the CLI's own reporting path; there is no logger in a headless run."
)]
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
#[expect(
    clippy::exit,
    clippy::print_stderr,
    reason = "`std::process::exit` is the correct way for a CLI mode to end the process from inside `main`; this one narrow allow keeps the crate-wide `exit` lint for every other path."
)]
pub fn repair_main(options: &RepairCliOptions) -> Result<(), Box<dyn std::error::Error>> {
    match run_repair(options) {
        Ok(status) => std::process::exit(status),
        Err(error) => {
            eprintln!("geometry repair: {error}");
            std::process::exit(2);
        }
    }
}

/// In-memory editor adapter for the existing planner. Raw metadata survives;
/// ambiguous couplings are refused before its mandatory no-new-error check.
pub fn preview_alignment(
    raw: &serde_json::Value,
    first: usize,
    second: usize,
) -> Result<Vec<(String, serde_json::Value)>, String> {
    let level: LevelDef = serde_json::from_value(raw.clone()).map_err(|e| e.to_string())?;
    let surfaces = LevelSurfaces::new(&level);
    let candidate = compute_wall_joints(&level, &surfaces)
        .into_iter()
        .find(|c| c.joint.first == first && c.joint.second == second && c.joint.auto_repairable)
        .ok_or("This joint is not an unambiguous automatic alignment")?;
    let (_, review) = coupled_edits(&level, &candidate, 0);
    if let Some(reason) = review.first() {
        return Err(reason.message.clone());
    }
    let catalog = crate::engine::assets::AssetCatalog::load_default();
    let mut prepared = level.clone();
    crate::map_compiler::prepare_level(&mut prepared, &catalog, None);
    let baseline = check_level(&prepared, "editor alignment", true);
    let mut state = PlannerState {
        catalog: &catalog,
        source: "editor alignment",
        fingerprints: error_fingerprints(&baseline),
        edits: Vec::new(),
        review: Vec::new(),
    };
    if state.attempt(&level, &candidate, 0).is_none() {
        return Err(state.review.first().map_or_else(
            || "Alignment post-check refused".into(),
            |r| r.message.clone(),
        ));
    }
    Ok(state
        .edits
        .iter()
        .map(|edit| (edit.target.pointer(), serde_json::json!(edit.new)))
        .collect())
}
