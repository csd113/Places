//! Shared source evidence for endpoint joins and surface seams. Rectangular
//! walls only: curved/rotated architecture retains its authored geometry.
use crate::engine::level::{LevelDef, LevelSurfaces, WallAxis, WallDef};

const EPS: f32 = super::JOINT_PLANE_TOL_M;
const WELD: f32 = super::JOINT_ADJACENCY_M;

#[derive(Clone, Debug)]
pub struct EndpointIssue {
    pub wall: usize,
    pub other: usize,
    pub start: bool,
    pub target: f32,
    pub position: [f32; 3],
    pub check: &'static str,
    pub auto_repairable: bool,
    pub explanation: String,
}

fn spans(wall: &WallDef) -> ((f32, f32), (f32, f32)) {
    let (x, z) = wall.min_corner();
    match wall.axis() {
        WallAxis::X => ((x, x + wall.width.abs()), (z, z + wall.depth.abs())),
        WallAxis::Z => ((z, z + wall.depth.abs()), (x, x + wall.width.abs())),
    }
}
fn same(a: f32, b: f32) -> bool {
    (a - b).abs() <= EPS
}

/// Finds short, solid endpoint gaps or tails.
///
/// A junction already
/// ending inside its carrier thickness is correct; overlapping L/T solids are
/// resolved by the existing covered-face emitter and must not be "repaired".
#[must_use]
pub fn endpoints(level: &LevelDef) -> Vec<EndpointIssue> {
    let surfaces = LevelSurfaces::new(level);
    let mut issues = Vec::new();
    for (index, wall) in level.walls.iter().enumerate() {
        if wall.width <= 0.0 || wall.depth <= 0.0 || wall.length() < 0.5 {
            continue;
        }
        let (along, across) = spans(wall);
        for start in [true, false] {
            let at = if start { along.0 } else { along.1 };
            let mut candidates = Vec::new();
            for (other_index, other) in level.walls.iter().enumerate() {
                if index == other_index || !same(wall.y, other.y) {
                    continue;
                }
                let sample = if start { 0.0 } else { wall.length() };
                let own_height =
                    wall.resolved_height(surfaces.clear_ceiling_height_along(wall, sample));
                let other_height = other.resolved_height(
                    surfaces.clear_ceiling_height_along(other, other.length() * 0.5),
                );
                if !same(own_height, other_height) {
                    continue;
                }
                let (other_along, _) = spans(other);
                let Some((target, check, auto_repairable)) = target_for(wall, other, start) else {
                    continue;
                };
                let contact = if wall.axis() == other.axis() {
                    if start { other.length() } else { 0.0 }
                } else {
                    f32::midpoint(across.0, across.1) - other_along.0
                };
                if other.openings.iter().any(|opening| {
                    wall.thickness().mul_add(0.5, contact) >= opening.offset
                        && wall.thickness().mul_add(-0.5, contact) <= opening.end()
                }) {
                    continue;
                }
                let endpoint = if start { 0.0 } else { wall.length() };
                if wall
                    .openings
                    .iter()
                    .any(|o| endpoint >= o.offset - WELD && endpoint <= o.end() + WELD)
                {
                    continue;
                }
                // A third solid already spanning this tiny gap explains the join.
                let middle = f32::midpoint(at, target);
                let cross = f32::midpoint(across.0, across.1);
                let (x, z) = match wall.axis() {
                    WallAxis::X => (middle, cross),
                    WallAxis::Z => (cross, middle),
                };
                if level.walls.iter().enumerate().any(|(third, w)| {
                    if third == index || third == other_index || !same(w.y, wall.y) {
                        return false;
                    }
                    let (left, near) = w.min_corner();
                    w.openings.is_empty()
                        && x > left + EPS
                        && x < left + w.width.abs() - EPS
                        && z > near + EPS
                        && z < near + w.depth.abs() - EPS
                }) {
                    continue;
                }
                candidates.push(EndpointIssue { wall:index, other:other_index, start, target:super::quantise(target), position:[x,own_height.mul_add(0.5, wall.y),z], check, auto_repairable,
                    explanation:format!("The rectangular endpoint differs by {:.4} m from Wall {}'s solid face. Preview {} this endpoint to that face; overlapping L/T contacts inside the carrier thickness are already valid. Openings and existing fittings must remain in world position.", (at-target).abs(), other_index.saturating_add(1), if check == "wall-corner-tail" {"trimming"} else {"extending"}) });
            }
            // Several carriers or targets make automatic intent ambiguous.
            if candidates.len() > 1 {
                if let Some(mut issue) = candidates.into_iter().next() {
                    issue.auto_repairable = false;
                    issue.check = "wall-corner-review";
                    issue.explanation="Several nearby carriers can explain this endpoint. Review the wall planes and thicknesses; no automatic endpoint target is chosen.".into();
                    issues.push(issue);
                }
            } else if let Some(mut issue) = candidates.pop() {
                if !issue.auto_repairable {
                    issue.explanation="Nearby perpendicular endpoints leave a gap or tail within 35 cm, but the 5 cm automatic weld tolerance or full-thickness carrier requirement is not met. Review the inside/outside corner planes. The offset can be deliberate; no automatic alignment chooses a plane for you.".into();
                }
                // Collinear gaps are emitted once; the earlier wall extends.
                if issue.check != "wall-endpoint-gap" || index < issue.other {
                    issues.push(issue);
                }
            }
        }
    }
    issues
}

pub(super) fn check(checker: &mut super::Checker<'_>) {
    for issue in endpoints(checker.level) {
        checker.push(
            issue.check,
            super::Severity::Warning,
            format!("wall {} / wall {}", issue.wall, issue.other),
            issue.explanation,
            issue.position,
        );
    }
    let level = checker.level;
    let surfaces = LevelSurfaces::new(level);
    for (index, wall) in level.walls.iter().enumerate() {
        let (along, across) = spans(wall);
        let (x, z) = match wall.axis() {
            WallAxis::X => (
                f32::midpoint(along.0, along.1),
                f32::midpoint(across.0, across.1),
            ),
            WallAxis::Z => (
                f32::midpoint(across.0, across.1),
                f32::midpoint(along.0, along.1),
            ),
        };
        let Some(room) = surfaces.room_at(x, z) else {
            continue;
        };
        let gap = wall.y - room.floor_y;
        if gap > EPS && gap <= WELD {
            checker.push("wall-floor-seam",super::Severity::Warning,format!("wall {index}"),format!("Wall base is {gap:.4} m above its room floor. This may be a deliberate floating panel; review the visible crack before moving it or its attachments."),[x,wall.y,z]);
        }
        if let Some(height) = wall.height
            && !room.ceiling.is_open()
        {
            let top = wall.y + height;
            let ceiling = room.ceiling_y_at(x, z);
            if ceiling - top > EPS && ceiling - top <= WELD {
                checker.push("wall-ceiling-seam",super::Severity::Warning,format!("wall {index}"),format!("Wall top is {:.4} m below the roof at this point. Partial-height walls are valid; inspect the span and roof profile before extending it.",ceiling-top),[x,top,z]);
            }
        }
    }
}

fn target_for(wall: &WallDef, other: &WallDef, start: bool) -> Option<(f32, &'static str, bool)> {
    let (along, across) = spans(wall);
    let (other_along, other_across) = spans(other);
    let at = if start { along.0 } else { along.1 };
    if wall.axis() == other.axis() {
        if !same(across.0, other_across.0) || !same(across.1, other_across.1) {
            return None;
        }
        let target = if start { other_along.1 } else { other_along.0 };
        let gap = if start { at - target } else { target - at };
        if gap <= EPS || gap > WELD {
            return None;
        }
        Some((target, "wall-endpoint-gap", true))
    } else {
        // The carrier runs across this wall; require its length
        // to cover our full thickness, avoiding diagonal near misses.
        if other_along.0 >= across.1 - EPS || other_along.1 <= across.0 + EPS {
            return None;
        }
        if at >= other_across.0 - EPS && at <= other_across.1 + EPS {
            return None;
        }
        let (target, gap) = if start {
            if at > other_across.1 {
                (other_across.1, true)
            } else {
                (other_across.0, false)
            }
        } else if at < other_across.0 {
            (other_across.0, true)
        } else {
            (other_across.1, false)
        };
        if (at - target).abs() > super::JOINT_NEAR_ADJACENCY_M {
            return None;
        }
        // The body must approach from the appropriate side.
        if (start && along.1 <= other_across.1) || (!start && along.0 >= other_across.0) {
            return None;
        }
        let covered = other_along.0 <= across.0 + EPS && other_along.1 >= across.1 - EPS;
        let automatic = covered && (at - target).abs() <= WELD;
        Some((
            target,
            if !automatic {
                "wall-corner-review"
            } else if gap {
                "wall-corner-gap"
            } else {
                "wall-corner-tail"
            },
            automatic,
        ))
    }
}
