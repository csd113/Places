//! Bounded player-sized samples over real floor and collision representations.

use super::{SafetyCategory, SafetyFinding, SafetyReport, SafetySeverity, bounds_center};
use crate::editor::document::ObjectId;
use crate::engine::collision::{CROUCH_HEIGHT, PLAYER_RADIUS, PLAYER_STEP_HEIGHT, WallAabb};
use crate::engine::level::{LevelDef, LevelSurfaces, WalkableFloor, WallAxis};
use crate::engine::spatial::Aabb;

const SAMPLES: u16 = 12;
const EDGE_INSET: f32 = 0.02;

pub(super) fn check(level: &LevelDef, report: &mut SafetyReport) {
    let floor = WalkableFloor::from_level(level);
    let surfaces = LevelSurfaces::new(level);
    let collision = level.collision_aabbs();
    for (index, room) in level.rooms.iter().enumerate() {
        let (x0, x1, z0, z1) = room.bounds();
        for (start, end, outward) in [
            ([x0, z0], [x1, z0], [0.0, -1.0]),
            ([x0, z1], [x1, z1], [0.0, 1.0]),
            ([x0, z0], [x0, z1], [-1.0, 0.0]),
            ([x1, z0], [x1, z1], [1.0, 0.0]),
        ] {
            edge(
                level,
                &floor,
                &collision,
                report,
                &Edge {
                    object: ObjectId::Room(index),
                    label: format!("Floor {} edge", index.saturating_add(1)),
                    start,
                    end,
                    outward,
                    base: room.floor_y,
                    ramp: false,
                },
            );
        }
    }
    for (index, ramp) in level.ramps.iter().enumerate() {
        let (x0, x1, z0, z1) = ramp.bounds();
        let base = surfaces
            .ramp_room(ramp)
            .map_or(0.0, |(_, room)| room.floor_y);
        if [(x0, z0), (x0, z1), (x1, z0), (x1, z1)]
            .iter()
            .any(|(x, z)| surfaces.room_at(*x, *z).is_none())
        {
            unsupported_ramp(level, index, report);
        }
        let sides = match ramp.surface().axis() {
            WallAxis::X => [
                ([x0, z0], [x1, z0], [0.0, -1.0]),
                ([x0, z1], [x1, z1], [0.0, 1.0]),
            ],
            WallAxis::Z => [
                ([x0, z0], [x0, z1], [-1.0, 0.0]),
                ([x1, z0], [x1, z1], [1.0, 0.0]),
            ],
        };
        for (start, end, outward) in sides {
            edge(
                level,
                &floor,
                &collision,
                report,
                &Edge {
                    object: ObjectId::Ramp(index),
                    label: format!("Ramp {} side", index.saturating_add(1)),
                    start,
                    end,
                    outward,
                    base,
                    ramp: true,
                },
            );
        }
    }
    regions(level, report);
}

fn unsupported_ramp(level: &LevelDef, index: usize, report: &mut SafetyReport) {
    let object = ObjectId::Ramp(index);
    let Some(bounds) = super::reference_bounds(level, &[object]) else {
        return;
    };
    report.findings.push(SafetyFinding {
        check: "ramp-carrier-gap", category: SafetyCategory::Void, severity: SafetySeverity::Warning,
        title: "Ramp extends outside a carrier floor".into(),
        risk: "At least one ramp corner lies outside every room footprint. The rendered slope can extend there, but the player has no room-based walking support outside the carriers and can fall. Extend or reshape the carrier deliberately; no floor is added automatically. Corner sampling does not prove complete footprint coverage.".into(),
        position: bounds_center(bounds), bounds, objects: vec![object], surfaces: Vec::new(), fix: None, placement_ignore: None,
    });
}

struct Edge {
    object: ObjectId,
    label: String,
    start: [f32; 2],
    end: [f32; 2],
    outward: [f32; 2],
    base: f32,
    ramp: bool,
}

fn edge(
    level: &LevelDef,
    floor: &WalkableFloor,
    collision: &[WallAabb],
    report: &mut SafetyReport,
    edge: &Edge,
) {
    let [start_x, start_z] = edge.start;
    let [end_x, end_z] = edge.end;
    let [out_x, out_z] = edge.outward;
    let surfaces = LevelSurfaces::new(level);
    let mut evidence = Aabb::EMPTY;
    let mut void = false;
    let mut drop = 0.0_f32;
    let mut count = 0_u16;
    for sample in 0..SAMPLES {
        let t = (f32::from(sample) + 0.5) / f32::from(SAMPLES);
        let x = (end_x - start_x).mul_add(t, start_x);
        let z = (end_z - start_z).mul_add(t, start_z);
        let inside_x = (-out_x).mul_add(EDGE_INSET, x);
        let inside_z = (-out_z).mul_add(EDGE_INSET, z);
        let feet = edge.base + surfaces.walkable_offset_at(inside_x, inside_z);
        let apex = feet + crate::game::JUMP_APEX_M;
        // A low rim may block walking and still allow a crouched jump. This
        // tests the actual authored collider's vertical rule, not its artwork.
        if collision.iter().any(|wall| {
            wall.blocks_body(apex, CROUCH_HEIGHT) && wall.overlaps_disc(x, z, PLAYER_RADIUS)
        }) {
            continue;
        }
        let outside_x = out_x.mul_add(PLAYER_RADIUS + EDGE_INSET, x);
        let outside_z = out_z.mul_add(PLAYER_RADIUS + EDGE_INSET, z);
        let landing = floor.height_below(outside_x, outside_z, apex + PLAYER_STEP_HEIGHT);
        let is_drop = landing.is_none_or(|height| feet - height > PLAYER_STEP_HEIGHT + 0.01);
        let check = if edge.ramp {
            "ramp-side-escape"
        } else {
            "floor-edge-drop"
        };
        if !is_drop
            || level
                .geometry_intent
                .iter()
                .any(|intent| intent.covers(check, x, z))
        {
            continue;
        }
        void |= landing.is_none();
        if let Some(height) = landing {
            drop = drop.max(feet - height);
            evidence.expand([outside_x, height, outside_z]);
        }
        evidence.expand([x, feet, z]);
        evidence.expand([x, apex + CROUCH_HEIGHT, z]);
        count = count.saturating_add(1);
    }
    if count == 0 {
        return;
    }
    let risk = if void {
        "At least one sample has no lower floor beyond the edge; a jump may leave the authored walkable space.".into()
    } else {
        format!(
            "Sampled lower landings are up to {drop:.2} m below this edge, above the {PLAYER_STEP_HEIGHT:.2} m walking step."
        )
    };
    report.findings.push(SafetyFinding {
        check: if edge.ramp { "ramp-side-escape" } else { "floor-edge-drop" },
        category: if void { SafetyCategory::Void } else { SafetyCategory::Collision },
        severity: SafetySeverity::Warning,
        title: format!("{}: jump / drop review", edge.label),
        risk: format!("{risk} {count}/{SAMPLES} samples have no crouched-body barrier at the {:.2} m jump apex; low side walls may be cleared. Sampled risk, not a trajectory proof: check ceiling, approach, water, ladders and intended openings in Play. No wall is added automatically.", crate::game::JUMP_APEX_M),
        position: bounds_center(evidence),
        bounds: evidence,
        objects: vec![edge.object],
        surfaces: Vec::new(),
        fix: None, placement_ignore: None,
    });
}

fn regions(level: &LevelDef, report: &mut SafetyReport) {
    for (index, region) in level.floor_regions.iter().enumerate() {
        let rooms: Vec<_> = level
            .rooms
            .iter()
            .filter(|room| {
                let (x0, x1, z0, z1) = room.bounds();
                let (rx0, rx1, rz0, rz1) = region.bounds();
                rx1 > x0 && rx0 < x1 && rz1 > z0 && rz0 < z1
            })
            .collect();
        if rooms.len() > 1
            && rooms.first().is_some_and(|first| {
                rooms
                    .iter()
                    .any(|room| (room.floor_y - first.floor_y).abs() > 0.01)
            })
        {
            push_region(
                level,
                index,
                "region-multiple-storeys",
                "Region affects multiple floors",
                "Floor regions apply to every overlapping room footprint, relative to each room's own floor. This edit therefore cuts or raises more than one storey. Split the footprint or review each floor in Play; no source is changed automatically.",
                SafetySeverity::Warning,
                report,
            );
        }
        if region.offset() < -(crate::game::JUMP_APEX_M + PLAYER_STEP_HEIGHT) {
            push_region(
                level,
                index,
                "deep-recess-exit",
                "Deep recess: check an exit",
                "The recess is deeper than one jump plus the walkable step. Its vertical rim blocks walking from the basin; a flat-walled basin may trap the player. Confirm a ramp, steps, ladder or swimming exit in Play. The checker does not prove route reachability and does not fill the pool or alter its rim.",
                SafetySeverity::Warning,
                report,
            );
        }
    }
}

fn push_region(
    level: &LevelDef,
    index: usize,
    check: &'static str,
    title: &str,
    risk: &str,
    severity: SafetySeverity,
    report: &mut SafetyReport,
) {
    let object = ObjectId::FloorRegion(index);
    let Some(bounds) = super::reference_bounds(level, &[object]) else {
        return;
    };
    let [x, _, z] = bounds_center(bounds);
    if level
        .geometry_intent
        .iter()
        .any(|intent| intent.covers(check, x, z))
    {
        return;
    }
    report.findings.push(SafetyFinding {
        check,
        category: SafetyCategory::Collision,
        severity,
        title: title.into(),
        risk: risk.into(),
        position: bounds_center(bounds),
        bounds,
        objects: vec![object],
        surfaces: Vec::new(),
        fix: None,
        placement_ignore: None,
    });
}
