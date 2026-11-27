//! Editor-only footing checks for the typed guardrail placement tools.

use crate::engine::{
    collision::CONTACT_EPS,
    level::{
        FloorRegionDef, GuardrailDef, LevelDef, LevelSurfaces, RampDef, RoomDef, StairDef,
        WalkableCeiling, WalkableFloor,
        construction::{rail_neighbors, rail_runs},
    },
};
use glam::{Vec2, Vec3};
use serde_json::{Value, json};

use super::{Document, ObjectId, document::record_array_mut};

pub(super) const FOOTING_CUE: &str = "Place the railing on a floor or walkway edge";
const GRADE_EPS_M: f32 = 0.025;
const PICK_EPS_M: f32 = 0.001;
const EDGE_SNAP_M: f32 = 0.15;
const EDGE_PROBE_M: f32 = 0.001;

mod footing;

pub(super) fn snap_footing(
    level: &LevelDef,
    floor: &WalkableFloor,
    mut hit: Vec3,
    grid: Option<f32>,
) -> [f32; 3] {
    // A ray/triangle hit can differ slightly from the authored flat plane.
    // Normalize the visual pick, not an explicit height in the numeric form.
    if let Some((surface, false)) = floor.attachment_surface_below(hit.x, hit.z, hit.y + PICK_EPS_M)
        && (surface - hit.y).abs() <= PICK_EPS_M
    {
        hit.y = surface;
    }
    let Some(step) = grid.filter(|step| step.is_finite() && *step > 0.0) else {
        return hit.to_array();
    };
    if let Some((_, x, z)) = nearest_edge(level, floor, hit) {
        return [x, hit.y, z];
    }
    let x = (hit.x / step).round() * step;
    let z = (hit.z / step).round() * step;
    if supported_line(level, (hit.x, hit.z), (x, z), |_, at_x, at_z| {
        carries(floor, at_x, at_z, hit.y)
    }) {
        [x, hit.y, z]
    } else {
        hit.to_array()
    }
}

fn nearest_edge(level: &LevelDef, floor: &WalkableFloor, hit: Vec3) -> Option<(f32, f32, f32)> {
    let mut nearest = None;
    for (x0, x1, z0, z1) in floor_bounds(level) {
        let tangent = |value: f32, low: f32, high: f32| {
            let inset = footing::POST_REACH.min((high - low) * 0.5);
            value.clamp(low + inset, (high - inset).max(low + inset))
        };
        let horizontal = tangent(hit.x, x0, x1);
        let vertical = tangent(hit.z, z0, z1);
        for (x, z, outward_x, outward_z) in [
            (x0, vertical, -EDGE_PROBE_M, 0.0),
            (x1, vertical, EDGE_PROBE_M, 0.0),
            (horizontal, z0, 0.0, -EDGE_PROBE_M),
            (horizontal, z1, 0.0, EDGE_PROBE_M),
        ] {
            let distance = (x - hit.x).hypot(z - hit.z);
            if distance <= EDGE_SNAP_M
                && nearest.is_none_or(|(found, _, _)| distance < found)
                && carries(floor, x - outward_x, z - outward_z, hit.y)
                && !carries(floor, x + outward_x, z + outward_z, hit.y)
                && supported_line(level, (hit.x, hit.z), (x, z), |_, at_x, at_z| {
                    carries(floor, at_x, at_z, hit.y)
                })
            {
                nearest = Some((distance, x, z));
            }
        }
    }
    nearest
}

fn carries(floor: &WalkableFloor, x: f32, z: f32, y: f32) -> bool {
    floor
        .attachment_surface_below(x, z, y + GRADE_EPS_M)
        .is_some_and(|(surface, graded)| {
            // A square post crosses a range of heights on a grade. Flat
            // supports need actual contact, not that geometric allowance.
            let tolerance = if graded { GRADE_EPS_M } else { CONTACT_EPS };
            (surface - y).abs() <= tolerance
        })
}

pub(super) fn candidate(
    doc: &Document,
    start: [f32; 3],
    middle: &[[f32; 3]],
    end: [f32; 3],
) -> Result<(Value, ObjectId), String> {
    let mut raw = doc.raw().clone();
    let index = stage_path(&mut raw, start, middle, end)?;
    let checked = doc.validate_candidate(&raw)?;
    let owner = check_path(&checked, doc.level(), index, start)?;
    Ok((raw, owner))
}

pub(super) fn stage_path(
    raw: &mut Value,
    start: [f32; 3],
    middle: &[[f32; 3]],
    end: [f32; 3],
) -> Result<usize, String> {
    let points: Vec<_> = std::iter::once(start)
        .chain(middle.iter().copied())
        .chain([end])
        .collect();
    let rise = points.get(1).ok_or(FOOTING_CUE)?[1] - start[1];
    if points
        .windows(2)
        .any(|pair| matches!(pair, [from,to] if ((to[1]-from[1])-rise).abs() > GRADE_EPS_M))
    {
        return Err("Split the railing into separate runs where the floor grade changes".into());
    }
    let records = record_array_mut(raw, "guardrails")?;
    let index = records.len();
    let path: Vec<_> = points.iter().skip(1).map(|&[x, _, z]| [x, z]).collect();
    records.push(json!({"x":start[0],"z":start[2],"y":start[1],"rise":rise,"length":1_u32,"path":path,"height":1_u32,"post_spacing":1.2_f32}));
    Ok(index)
}

pub(super) fn check_path(
    checked: &LevelDef,
    before: &LevelDef,
    index: usize,
    start: [f32; 3],
) -> Result<ObjectId, String> {
    validate_changed(checked, before, index)?;
    let floor_support = |room: &RoomDef| {
        let (x0, x1, z0, z1) = room.bounds();
        (x0..=x1).contains(&start[0]) && (z0..=z1).contains(&start[2])
    };
    let owner = checked
        .rooms
        .iter()
        .enumerate()
        .filter(|(_, room)| floor_support(room))
        .min_by(|(_, a), (_, b)| {
            (a.floor_y - start[1])
                .abs()
                .total_cmp(&(b.floor_y - start[1]).abs())
        })
        .map(|(i, _)| i)
        .ok_or(FOOTING_CUE)?;
    Ok(ObjectId::Room(owner))
}

pub(super) fn validate_changed(
    level: &LevelDef,
    before: &LevelDef,
    index: usize,
) -> Result<(), String> {
    let rail = level
        .guardrails
        .get(index)
        .ok_or("This railing no longer exists")?;
    if before
        .guardrails
        .get(index)
        .is_some_and(|prior| same_footing(prior, rail))
    {
        return Ok(());
    }
    let floor = WalkableFloor::from_level(level);
    let ceiling = WalkableCeiling::from_level(level);
    let surfaces = LevelSurfaces::new(level);
    let runs = rail_runs(rail, level);
    if runs.is_empty() {
        return Err(FOOTING_CUE.into());
    }
    for (segment, run) in runs.iter().enumerate() {
        let start = run.point_at(0.0, 0.0);
        let end = run.point_at(1.0, 0.0);
        if !supported_line(level, start, end, |t, x, z| {
            carries(&floor, x, z, run.base_y_at(&surfaces, t))
        }) {
            return Err(FOOTING_CUE.into());
        }
        if !footing::posts_have_bearing(
            level,
            &floor,
            &surfaces,
            run,
            rail_neighbors(&runs, segment),
        ) {
            return Err("At least half of each railing post base must rest on the floor".into());
        }
        if !run_has_headroom(level, &floor, &ceiling, &surfaces, run) {
            return Err("Railing height intersects the ceiling or deck on this storey".into());
        }
    }
    Ok(())
}

fn same_footing(prior: &GuardrailDef, rail: &GuardrailDef) -> bool {
    [
        prior.x,
        prior.z,
        prior.length,
        prior.rotation_degrees,
        prior.post_spacing,
    ]
    .map(f32::to_bits)
        == [
            rail.x,
            rail.z,
            rail.length,
            rail.rotation_degrees,
            rail.post_spacing,
        ]
        .map(f32::to_bits)
        && prior.height.to_bits() == rail.height.to_bits()
        && prior.y.map(f32::to_bits) == rail.y.map(f32::to_bits)
        && prior.rise.map(f32::to_bits) == rail.rise.map(f32::to_bits)
        && prior
            .path
            .iter()
            .map(|point| point.map(f32::to_bits))
            .eq(rail.path.iter().map(|point| point.map(f32::to_bits)))
}

fn run_has_headroom(
    level: &LevelDef,
    floor: &WalkableFloor,
    ceiling: &WalkableCeiling,
    surfaces: &LevelSurfaces<'_>,
    run: &GuardrailDef,
) -> bool {
    line_is_clear(
        level,
        run.point_at(0.0, 0.0),
        run.point_at(1.0, 0.0),
        footing::POST_REACH,
        |t, x, z| {
            let at = Vec2::new(x, z);
            let base = run.base_y_at(surfaces, t);
            let mut roof_clear = true;
            ceiling.for_each_hull_contact(at, footing::POST_REACH, base, run.height(), |_| {
                roof_clear = false;
            });
            roof_clear
                && floor
                    .lowest_room_underside_hull(at, footing::POST_REACH, base)
                    .is_none_or(|bottom| bottom + CONTACT_EPS >= base + run.height())
        },
    )
}

fn supported_line(
    level: &LevelDef,
    (x, z): (f32, f32),
    (end_x, end_z): (f32, f32),
    at: impl Fn(f32, f32, f32) -> bool,
) -> bool {
    line_is_clear(level, (x, z), (end_x, end_z), 0.0, at)
}

fn floor_bounds(level: &LevelDef) -> impl Iterator<Item = (f32, f32, f32, f32)> + '_ {
    level
        .rooms
        .iter()
        .map(RoomDef::bounds)
        .chain(level.floor_regions.iter().map(FloorRegionDef::bounds))
        .chain(level.ramps.iter().map(RampDef::bounds))
        .chain(level.stairs.iter().map(StairDef::bounds))
}

fn line_is_clear(
    level: &LevelDef,
    (x, z): (f32, f32),
    (end_x, end_z): (f32, f32),
    padding: f32,
    at: impl Fn(f32, f32, f32) -> bool,
) -> bool {
    let dx = end_x - x;
    let dz = end_z - z;
    let mut cuts = vec![0.0_f32, 1.0];
    for (x0, x1, z0, z1) in floor_bounds(level) {
        for (edge, start, delta) in [
            (x0 - padding, x, dx),
            (x0, x, dx),
            (x1, x, dx),
            (x1 + padding, x, dx),
            (z0 - padding, z, dz),
            (z0, z, dz),
            (z1, z, dz),
            (z1 + padding, z, dz),
        ] {
            if delta.abs() > 1.0e-6 {
                let t = (edge - start) / delta;
                if t > 0.0 && t < 1.0 {
                    cuts.push(t);
                }
            }
        }
    }
    cuts.sort_by(f32::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 1.0e-6);
    let check = |t| at(t, dx.mul_add(t, x), dz.mul_add(t, z));
    // Each interval is bounded by real surface edges. Sample just inside
    // both ends as well as the middle to reject height changes and thin gaps.
    check(0.0) && check(1.0) && cuts.windows(2).all(|pair| matches!(pair, [a,b] if {
        let width = b-a;
        [width.mul_add(0.001,*a), f32::midpoint(*a,*b), width.mul_add(0.999,*a)].into_iter().all(check)
    }))
}
