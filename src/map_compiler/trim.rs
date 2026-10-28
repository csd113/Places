//! Saved trim attachments use the ordinary baseboard geometry and material path.
//! Fit admission is shared by source loading, editor previews and compilation.

use crate::engine::level::{
    BaseboardDef, BaseboardUvProjection, LevelDef, LevelSurfaces, TrimDef, TrimPlacement, WallAxis,
    round_point,
};

const FIT_EPS: f32 = 0.0001;

struct Plan {
    boards: Vec<BaseboardDef>,
    issues: Vec<String>,
}

/// Refuses unsupported contacts before source/history or package publication moves.
///
/// # Errors
/// Names the owning piece and the unsupported surface or invalid setting.
pub fn validate_attachments(level: &LevelDef) -> Result<(), String> {
    let plan = plan(level);
    if let Some(issue) = plan.issues.first() {
        return Err(issue.clone());
    }
    let authored = level
        .baseboards
        .iter()
        .filter(|board| !board.generated)
        .count();
    if u64::try_from(authored.saturating_add(plan.boards.len())).unwrap_or(u64::MAX)
        > crate::engine::level::MAX_LEVEL_BASEBOARDS
    {
        return Err(
            "Attached trim exceeds the level baseboard budget; reduce tessellation or selection"
                .into(),
        );
    }
    Ok(())
}

pub(super) fn attached_baseboards(level: &LevelDef) -> Vec<BaseboardDef> {
    plan(level).boards
}

fn valid(trim: &TrimDef, owner: &str, issues: &mut Vec<String>) -> bool {
    if !trim.height.is_finite()
        || !(0.01..=1.0).contains(&trim.height)
        || !trim.thickness.is_finite()
        || !(0.004..=0.2).contains(&trim.thickness)
        || trim
            .material
            .as_ref()
            .is_some_and(|id| id.trim().is_empty())
    {
        issues.push(format!(
            "{owner} trim: use height 0.01..1m, thickness 0.004..0.2m and a valid material"
        ));
        return false;
    }
    true
}

fn band_bases(trim: &TrimDef, base: f32, top: f32) -> Vec<f32> {
    match trim.placement {
        TrimPlacement::None => Vec::new(),
        TrimPlacement::Bottom => vec![base],
        TrimPlacement::Top => vec![top - trim.height],
        TrimPlacement::Both => vec![base, top - trim.height],
    }
}

fn board(
    trim: &TrimDef,
    start: (f32, f32),
    end: (f32, f32),
    base: f32,
    material: &str,
) -> BaseboardDef {
    let (dx, dz) = (end.0 - start.0, end.1 - start.1);
    BaseboardDef {
        generated: true,
        miters: None,
        uv_offset: 0.0,
        x: start.0,
        z: start.1,
        length: dx.hypot(dz),
        rotation_degrees: (-dz).atan2(dx).to_degrees(),
        y: Some(base),
        height: trim.height,
        thickness: trim.thickness,
        material: Some(trim.material.as_deref().unwrap_or(material).into()),
        shine: None,
        uv_projection: BaseboardUvProjection::Wall,
    }
}

fn plan(level: &LevelDef) -> Plan {
    let mut out = Plan {
        boards: Vec::new(),
        issues: Vec::new(),
    };
    let surfaces = LevelSurfaces::new(level);
    plan_walls(level, &surfaces, &mut out);
    for (index, column) in level.columns.iter().enumerate() {
        let Some(trim) = &column.trim else { continue };
        let (x0, x1, z0, z1) = column.bounds();
        polygon_bands(
            &mut out,
            trim,
            &format!("Rectangular pillar {}", index.saturating_add(1)),
            &[(x0, z1), (x1, z1), (x1, z0), (x0, z0)],
            true,
            column.base_y(&surfaces),
            column.top_y(&surfaces),
            column.material.as_deref().unwrap_or(&level.defaults.wall),
            (&surfaces, column.height.is_none()),
        );
    }
    for (index, pillar) in level.pillars.iter().enumerate() {
        let Some(trim) = &pillar.trim else { continue };
        let mut points = pillar.polygon_points();
        points.reverse();
        polygon_bands(
            &mut out,
            trim,
            &format!("Circular pillar {}", index.saturating_add(1)),
            &points,
            true,
            pillar.base_y(&surfaces),
            pillar.top_y(&surfaces),
            pillar.material.as_deref().unwrap_or(&level.defaults.wall),
            (&surfaces, pillar.height.is_none()),
        );
    }
    plan_arcs(level, &surfaces, &mut out);
    out
}

fn plan_arcs(level: &LevelDef, surfaces: &LevelSurfaces<'_>, out: &mut Plan) {
    for (index, arc) in level.arc_walls.iter().enumerate() {
        let Some(trim) = &arc.trim else { continue };
        if trim.placement == TrimPlacement::None {
            continue;
        }
        let count = arc.resolved_segments();
        let divisor = f32::from(u16::try_from(count).unwrap_or(u16::MAX));
        for (radius, reverse) in [
            (arc.outer_radius(), arc.sweep_degrees > 0.0),
            (arc.inner_radius(), arc.sweep_degrees < 0.0),
        ] {
            let mut points: Vec<_> = (0..=count)
                .map(|step| {
                    let fraction = f32::from(u16::try_from(step).unwrap_or(u16::MAX)) / divisor;
                    round_point(
                        arc.x,
                        arc.z,
                        radius,
                        arc.sweep_degrees.mul_add(fraction, arc.start_degrees),
                    )
                })
                .collect();
            if arc.is_full_ring() {
                let _last = points.pop();
            }
            if reverse {
                points.reverse();
            }
            let base = arc.base_y(surfaces);
            let top = arc.top_y_at(surfaces, arc.x, arc.z);
            if points
                .iter()
                .any(|(x, z)| (arc.top_y_at(surfaces, *x, *z) - top).abs() > FIT_EPS)
            {
                out.issues.push(format!(
                    "Curved wall {} trim: varying roof height requires a flat wall height",
                    index.saturating_add(1)
                ));
                continue;
            }
            polygon_bands(
                out,
                trim,
                &format!("Curved wall {}", index.saturating_add(1)),
                &points,
                arc.is_full_ring(),
                base,
                top,
                arc.material.as_deref().unwrap_or(&level.defaults.wall),
                (surfaces, true),
            );
        }
    }
}

fn plan_walls(level: &LevelDef, surfaces: &LevelSurfaces<'_>, out: &mut Plan) {
    for (index, wall) in level.walls.iter().enumerate() {
        let Some(trim) = &wall.trim else { continue };
        let owner = format!("Wall {}", index.saturating_add(1));
        if !valid(trim, &owner, &mut out.issues) || trim.placement == TrimPlacement::None {
            continue;
        }
        let (x, z) = wall.min_corner();
        let (right, far) = (x + wall.width.abs(), z + wall.depth.abs());
        let faces = match wall.axis() {
            WallAxis::X => [
                ("south", (x, far), (right, far)),
                ("north", (right, z), (x, z)),
            ],
            WallAxis::Z => [
                ("east", (right, far), (right, z)),
                ("west", (x, z), (x, far)),
            ],
        };
        let mut visible = false;
        for (name, start, end) in faces {
            visible |= wall_face(level, surfaces, out, wall, trim, &owner, (name, start, end));
        }
        if !visible {
            out.issues.push(format!(
                "{owner} trim: no adjacent room surface; create its floor/ceiling first"
            ));
        }
    }
}

fn wall_face(
    level: &LevelDef,
    surfaces: &LevelSurfaces<'_>,
    out: &mut Plan,
    wall: &crate::engine::level::WallDef,
    trim: &TrimDef,
    owner: &str,
    (name, start, end): (&str, (f32, f32), (f32, f32)),
) -> bool {
    let material = wall
        .face_ref(name)
        .map_or(level.defaults.wall.as_str(), |r| r.id);
    let probe = board(trim, start, end, wall.y, material);
    let samples: Vec<_> = surface_fractions(level, &probe)
        .into_iter()
        .map(|fraction| {
            let (px, pz) = probe.point_at(fraction, trim.thickness + 0.001);
            (surfaces.floor_y_at(px, pz), surfaces.ceiling_y_at(px, pz))
        })
        .collect();
    if samples.iter().all(|(floor, _)| floor.is_none()) {
        return false;
    }

    let Some((Some(floor), ceiling)) = samples.first().copied() else {
        out.issues.push(format!(
            "{owner} {name} trim: incomplete floor support; split the wall at the surface boundary"
        ));
        return false;
    };
    let top = wall.height.map_or(ceiling, |height| wall.y + height);
    let bottom_requested = matches!(trim.placement, TrimPlacement::Bottom | TrimPlacement::Both);
    let top_requested = matches!(trim.placement, TrimPlacement::Top | TrimPlacement::Both);
    if top_requested
        && surface_fractions(level, &probe)
            .into_iter()
            .any(|fraction| {
                let (px, pz) = probe.point_at(fraction, trim.thickness + 0.001);
                !surfaces.ceiling_is_flat_at(px, pz)
            })
    {
        out.issues.push(format!("{owner} {name} trim: ceiling trim requires a flat closed ceiling; keep bottom trim or split the wall"));
        return false;
    }
    if samples.iter().any(|(sample_floor, sample_ceiling)| {
        sample_floor.is_none()
            || (bottom_requested
                && sample_floor.is_some_and(|value| {
                    (value - wall.y).abs() > FIT_EPS || (value - floor).abs() > FIT_EPS
                }))
            || (top_requested
                && (!sample_ceiling.is_finite() || (*sample_ceiling - top).abs() > FIT_EPS))
    }) {
        out.issues.push(format!("{owner} {name} trim: wall must meet one flat floor/ceiling exactly; split at steps, slopes or open ceilings"));
        return false;
    }
    if top - wall.y
        <= trim.height
            * if trim.placement == TrimPlacement::Both {
                2.0
            } else {
                1.0
            }
    {
        out.issues.push(format!(
            "{owner} trim bands overlap or exceed the wall height"
        ));
        return false;
    }
    for base in band_bases(trim, wall.y, top) {
        let forward = match wall.axis() {
            WallAxis::X => end.0 > start.0,
            WallAxis::Z => end.1 > start.1,
        };
        for (low, high) in super::source::baseboard_segments_at(
            wall,
            probe.length,
            forward,
            base,
            base + trim.height,
        ) {
            let a = probe.point_at(low / probe.length, 0.0);
            let b = probe.point_at(high / probe.length, 0.0);
            let mut run = board(trim, a, b, base, material);
            run.uv_offset = low;
            out.boards.push(run);
        }
    }
    true
}

#[expect(
    clippy::too_many_arguments,
    reason = "one shared polygon attachment with an explicit owner, vertical range and material"
)]
fn polygon_bands(
    out: &mut Plan,
    trim: &TrimDef,
    owner: &str,
    points: &[(f32, f32)],
    closed: bool,
    base: f32,
    top: f32,
    material: &str,
    (surfaces, follows_ceiling): (&LevelSurfaces<'_>, bool),
) {
    if !valid(trim, owner, &mut out.issues) || trim.placement == TrimPlacement::None {
        return;
    }
    if top - base
        <= trim.height
            * if trim.placement == TrimPlacement::Both {
                2.0
            } else {
                1.0
            }
    {
        out.issues.push(format!(
            "{owner} trim bands overlap or exceed the pillar height"
        ));
        return;
    }
    let Some(first) = points.first().copied() else {
        return;
    };
    let mut edges: Vec<_> = points
        .windows(2)
        .filter_map(|pair| {
            let &[a, b] = pair else { return None };
            Some((a, b))
        })
        .collect();
    if closed && let Some(last) = points.last().copied() {
        edges.push((last, first));
    }
    for y in band_bases(trim, base, top) {
        let mut runs: Vec<_> = edges
            .iter()
            .map(|(a, b)| board(trim, *a, *b, y, material))
            .collect();
        let snapshot = runs.clone();
        let last_index = runs.len().saturating_sub(1);
        let mut distance = 0.0;
        for (index, run) in runs.iter_mut().enumerate() {
            let previous = if index == 0 {
                if closed { snapshot.last() } else { None }
            } else {
                snapshot.get(index.saturating_sub(1))
            };
            let next = if index == last_index {
                if closed { snapshot.first() } else { None }
            } else {
                snapshot.get(index.saturating_add(1))
            };
            let tangent = |other: &BaseboardDef| {
                let (dx, dz) = run.direction();
                let (ox, oz) = other.direction();
                dz.mul_add(-ox, dx * oz) / (1.0 + dx.mul_add(ox, dz * oz)).max(0.0001)
            };
            let start = previous.map_or(0.0, |other| -tangent(other));
            let end = next.map_or(0.0, |other| -tangent(other));
            run.miters = Some([start, end]);
            let (low, high) = run.span_at(trim.thickness);
            // Unroll the exposed front, including the miter extensions. Back
            // edge lengths would shift the visible print at every corner.
            run.uv_offset = distance - low;
            distance += high - low;
            if high - low <= FIT_EPS || start.abs() > 8.0 || end.abs() > 8.0 {
                out.issues.push(format!("{owner} trim: thickness cannot fit this corner; reduce thickness or increase radius"));
            }
        }
        check_perimeter(
            out,
            &runs,
            owner,
            (base, top, y),
            (surfaces, follows_ceiling),
        );
        out.boards.extend(runs);
    }
}

fn check_perimeter(
    out: &mut Plan,
    runs: &[BaseboardDef],
    owner: &str,
    (base, top, y): (f32, f32, f32),
    (surfaces, follows_ceiling): (&LevelSurfaces<'_>, bool),
) {
    let bottom_band = (y - base).abs() < FIT_EPS;
    for run in runs {
        for fraction in [0.000_001, 0.5, 0.999_999] {
            let (x, z) = run.point_at(fraction, run.thickness() + 0.001);
            if bottom_band
                && surfaces
                    .floor_y_at(x, z)
                    .is_none_or(|floor| (floor - base).abs() > FIT_EPS)
            {
                out.issues.push(format!("{owner} trim: perimeter must meet one flat floor; move the pillar clear of steps/slopes or disable bottom trim"));
                break;
            }
            if !bottom_band
                && follows_ceiling
                && (!surfaces.ceiling_is_flat_at(x, z)
                    || (surfaces.ceiling_y_at(x, z) - top).abs() > FIT_EPS)
            {
                out.issues.push(format!("{owner} trim: perimeter must meet one flat ceiling; use an explicit shorter post height or disable top trim"));
                break;
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/trim.rs"]
mod tests;

fn surface_fractions(level: &LevelDef, run: &BaseboardDef) -> Vec<f32> {
    let (dx, dz) = run.direction();
    let mut cuts = vec![0.0, 1.0];
    for (x0, x1, z0, z1) in level
        .rooms
        .iter()
        .map(crate::engine::level::RoomDef::bounds)
        .chain(
            level
                .floor_regions
                .iter()
                .map(crate::engine::level::FloorRegionDef::bounds),
        )
        .chain(
            level
                .ramps
                .iter()
                .map(crate::engine::level::RampDef::bounds),
        )
        .chain(
            level
                .stairs
                .iter()
                .map(crate::engine::level::StairDef::bounds),
        )
    {
        for (edges, direction, origin) in [([x0, x1], dx, run.x), ([z0, z1], dz, run.z)] {
            if direction.abs() < 0.00001 {
                continue;
            }
            cuts.extend(
                edges
                    .into_iter()
                    .map(|edge| (edge - origin) / (direction * run.length))
                    .filter(|value| *value > 0.0 && *value < 1.0),
            );
        }
    }
    cuts.sort_by(f32::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 0.000_001);
    let mut probes = vec![0.000_001, 0.999_999];
    probes.extend(cuts.windows(2).filter_map(|pair| {
        let &[a, b] = pair else { return None };
        Some(f32::midpoint(a, b))
    }));
    probes
}
