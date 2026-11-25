//! Bearing area of the actual rotated square posts on architectural floors.

use crate::engine::level::{
    GUARDRAIL_POST_SIZE_M, GuardrailDef, LevelDef, LevelSurfaces, WalkableFloor,
    construction::rail_post_positions,
};
use glam::Vec2;

const POST_HALF: f32 = GUARDRAIL_POST_SIZE_M * 0.5;
pub(super) const POST_REACH: f32 = POST_HALF * std::f32::consts::SQRT_2;
const HALF_POST_AREA: f32 = GUARDRAIL_POST_SIZE_M * GUARDRAIL_POST_SIZE_M * 0.5;
const AREA_ROUNDING: f32 = 1.0e-6;

pub(super) fn posts_have_bearing(
    level: &LevelDef,
    floor: &WalkableFloor,
    surfaces: &LevelSurfaces<'_>,
    run: &GuardrailDef,
    neighbors: (Option<&GuardrailDef>, Option<&GuardrailDef>),
) -> bool {
    let (previous, next) = neighbors;
    let corners = post_corners(run);
    rail_post_positions(run).into_iter().all(|along| {
        if previous.is_some() && along < 0.0001 {
            return true;
        }
        let progress = along / run.length;
        let mut base = run.base_y_at(surfaces, progress);
        // The shared corner post is emitted by the preceding run, and starts
        // at the lower of the two intersecting base lines.
        if run.length - along < 0.0001
            && let Some(neighbor) = next
        {
            base = base.min(neighbor.base_y(surfaces));
        }
        let (x, z) = run.point_at(progress, 0.0);
        has_bearing(level, floor, &corners, Vec2::new(x, z), base)
    })
}

fn post_corners(run: &GuardrailDef) -> [Vec2; 4] {
    let (dx, dz) = run.direction();
    let (ax, az) = run.across();
    [
        (-POST_HALF, -POST_HALF),
        (POST_HALF, -POST_HALF),
        (POST_HALF, POST_HALF),
        (-POST_HALF, POST_HALF),
    ]
    .map(|(along, across)| {
        Vec2::new(
            dx.mul_add(along, ax * across),
            dz.mul_add(along, az * across),
        )
    })
}

fn has_bearing(
    level: &LevelDef,
    floor: &WalkableFloor,
    corners: &[Vec2; 4],
    center: Vec2,
    base: f32,
) -> bool {
    let (low, high) = corners.iter().fold(
        (Vec2::splat(f32::INFINITY), Vec2::splat(f32::NEG_INFINITY)),
        |(low, high), &point| (low.min(point), high.max(point)),
    );
    let mut xs = vec![low.x, 0.0, high.x];
    let mut zs = vec![low.y, 0.0, high.y];
    for (x0, x1, z0, z1) in super::floor_bounds(level) {
        let [left, right, bottom, top] =
            [x0 - center.x, x1 - center.x, z0 - center.y, z1 - center.y];
        if left >= high.x || right <= low.x || bottom >= high.y || top <= low.y {
            continue;
        }
        xs.extend(
            [left, right]
                .into_iter()
                .filter(|x| *x > low.x && *x < high.x),
        );
        zs.extend(
            [bottom, top]
                .into_iter()
                .filter(|z| *z > low.y && *z < high.y),
        );
    }
    for cuts in [&mut xs, &mut zs] {
        cuts.sort_by(f32::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1.0e-6);
        if cuts.len() > 64 {
            return false;
        }
    }
    let mut area = 0.0;
    for &[left, right] in xs.array_windows::<2>() {
        for &[bottom, top] in zs.array_windows::<2>() {
            let middle_x = f32::midpoint(left, right);
            let middle_z = f32::midpoint(bottom, top);
            // Cuts isolate actual room, deck and region ownership. For graded
            // pieces, also check towards all four corners before claiming a cell.
            let supported = [
                (middle_x, middle_z),
                (left, bottom),
                (right, bottom),
                (left, top),
                (right, top),
            ]
            .into_iter()
            .all(|(x, z)| {
                super::carries(
                    floor,
                    center.x + (x - middle_x).mul_add(0.999, middle_x),
                    center.y + (z - middle_z).mul_add(0.999, middle_z),
                    base,
                )
            });
            if supported {
                area += clipped_area(corners, [left, right, bottom, top]);
            }
            if area + AREA_ROUNDING >= HALF_POST_AREA {
                return true;
            }
        }
    }
    false
}

fn clipped_area(corners: &[Vec2; 4], [left, right, bottom, top]: [f32; 4]) -> f32 {
    let mut polygon = corners.to_vec();
    for (axis, limit, sign) in [
        (Vec2::X, left, 1.0),
        (Vec2::X, right, -1.0),
        (Vec2::Y, bottom, 1.0),
        (Vec2::Y, top, -1.0),
    ] {
        let distance = |vertex: Vec2| (vertex.dot(axis) - limit) * sign;
        let mut clipped = Vec::with_capacity(polygon.len().saturating_add(1));
        for (&a, &b) in polygon.iter().zip(polygon.iter().cycle().skip(1)) {
            let (before, after) = (distance(a), distance(b));
            if before >= 0.0 {
                clipped.push(a);
            }
            if (before >= 0.0) != (after >= 0.0) {
                clipped.push(a.lerp(b, before / (before - after)));
            }
        }
        polygon = clipped;
    }
    polygon
        .iter()
        .zip(polygon.iter().cycle().skip(1))
        .map(|(&a, &b)| a.perp_dot(b))
        .sum::<f32>()
        .abs()
        * 0.5
}
