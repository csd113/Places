//! Deterministic construction attachments, shared by rendering, collision and Safety.
use super::{
    ArchitectureBox, GuardrailDef, LevelDef, LevelSurfaces, MaterialRef, RoomDef, WallAxis,
    WallDef, WallOpeningDef,
};
use crate::engine::collision::{CROUCH_HEIGHT, PLAYER_HEIGHT, PLAYER_RADIUS};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecessDef {
    pub depth: f32,
    #[serde(default)]
    pub positive_face: bool,
    #[serde(default)]
    pub back_material: Option<String>,
    #[serde(default)]
    pub side_material: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurroundDef {
    pub width: f32,
    pub projection: f32,
    #[serde(default)]
    pub sill_projection: f32,
    #[serde(default)]
    pub trim_width: f32,
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default)]
    pub sill_material: Option<String>,
    #[serde(default)]
    pub trim_material: Option<String>,
}

/// Local room coordinates; moving the room carries the passage with it.
/// Endpoints are the centres of the two open mouths, on the room's floor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TunnelDef {
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub width: f32,
    pub clearance: f32,
    pub thickness: f32,
    #[serde(default)]
    pub material: Option<String>,
}

pub struct Piece<'a> {
    pub bounds: ArchitectureBox,
    pub material: Option<MaterialRef<'a>>,
}

fn material(id: Option<&String>) -> Option<MaterialRef<'_>> {
    id.map(|value| MaterialRef::with_shine(value, None))
}

fn wall_piece<'a>(
    wall: &WallDef,
    along: [f32; 2],
    y: [f32; 2],
    across: [f32; 2],
    mat: Option<MaterialRef<'a>>,
) -> Option<Piece<'a>> {
    let (x, z) = wall.length_origin();
    let (low, high) = match wall.axis() {
        WallAxis::X => (
            [x + along[0], y[0], z + across[0]],
            [x + along[1], y[1], z + across[1]],
        ),
        WallAxis::Z => (
            [x + across[0], y[0], z + along[0]],
            [x + across[1], y[1], z + along[1]],
        ),
    };
    ArchitectureBox::from_corners(low, high).map(|bounds| Piece {
        bounds,
        material: mat,
    })
}

pub fn wall_pieces(wall: &WallDef) -> Vec<Piece<'_>> {
    let mut result = Vec::new();
    for opening in &wall.openings {
        let [a, b] = [opening.offset, opening.end()];
        let [bottom, top] = [opening.bottom(wall.y), opening.top(wall.y)];
        let thickness = wall.thickness();
        if let Some(recess) = &opening.recess {
            result.extend(recess_pieces(wall, opening, recess));
        }
        if let Some(frame) = &opening.surround {
            let w = frame.width;
            let mat = material(frame.material.as_ref()).or_else(|| wall.material_ref());
            let faces: &[bool] = opening.recess.as_ref().map_or(&[false, true], |r| {
                if r.positive_face { &[true] } else { &[false] }
            });
            for positive in faces {
                let across = if *positive {
                    [thickness, thickness + frame.projection]
                } else {
                    [-frame.projection, 0.0]
                };
                for (along, vertical) in [
                    ([a - w, a], [bottom, top]),
                    ([b, b + w], [bottom, top]),
                    ([a - w, b + w], [top, top + w]),
                ] {
                    result.extend(wall_piece(wall, along, vertical, across, mat));
                }
                if frame.sill_projection > 0.0 && opening.sill >= w {
                    let sill_across = if *positive {
                        [thickness, thickness + frame.sill_projection]
                    } else {
                        [-frame.sill_projection, 0.0]
                    };
                    result.extend(wall_piece(
                        wall,
                        [a - w, b + w],
                        [bottom - w, bottom],
                        sill_across,
                        material(frame.sill_material.as_ref()).or(mat),
                    ));
                }
                let t = frame.trim_width;
                if t > 0.0 {
                    let trim_across = if *positive {
                        [thickness, frame.projection.mul_add(0.5, thickness)]
                    } else {
                        [-frame.projection * 0.5, 0.0]
                    };
                    for (along, vertical) in [
                        ([a - w - t, a - w], [bottom, top + w]),
                        ([b + w, b + w + t], [bottom, top + w]),
                        ([a - w - t, b + w + t], [top + w, top + w + t]),
                    ] {
                        result.extend(wall_piece(
                            wall,
                            along,
                            vertical,
                            trim_across,
                            material(frame.trim_material.as_ref()).or(mat),
                        ));
                    }
                }
            }
        }
    }
    result
}

fn recess_pieces<'a>(
    wall: &'a WallDef,
    opening: &WallOpeningDef,
    recess: &'a RecessDef,
) -> Vec<Piece<'a>> {
    let mut result = Vec::new();
    let [a, b] = [opening.offset, opening.end()];
    let [bottom, top] = [opening.bottom(wall.y), opening.top(wall.y)];
    let thickness = wall.thickness();
    let lining = 0.005;
    // Only the cavity-facing liner receives the back material. The remaining
    // solid retains the carrier material on the opposite exterior wall face.
    let (back, bulk, cavity) = if recess.positive_face {
        let face = thickness - recess.depth;
        (
            [face - lining, face],
            [0.0, face - lining],
            [face, thickness],
        )
    } else {
        let face = recess.depth;
        (
            [face, face + lining],
            [face + lining, thickness],
            [0.0, face],
        )
    };
    for (across, mat) in [
        (
            back,
            material(recess.back_material.as_ref()).or_else(|| wall.material_ref()),
        ),
        (bulk, wall.material_ref()),
    ] {
        result.extend(wall_piece(wall, [a, b], [bottom, top], across, mat));
    }
    let side = material(recess.side_material.as_ref()).or_else(|| wall.material_ref());
    for (along, vertical) in [
        ([a, a + lining], [bottom, top]),
        ([b - lining, b], [bottom, top]),
        ([a + lining, b - lining], [bottom, bottom + lining]),
        ([a + lining, b - lining], [top - lining, top]),
    ] {
        result.extend(wall_piece(wall, along, vertical, cavity, side));
    }
    result
}

impl TunnelDef {
    pub fn bounds(&self, room: &RoomDef) -> ArchitectureBox {
        let half = self.width * 0.5;
        let along_x = (self.end[0] - self.start[0]).abs() >= (self.end[1] - self.start[1]).abs();
        let (x0, x1, z0, z1) = if along_x {
            (
                self.start[0].min(self.end[0]),
                self.start[0].max(self.end[0]),
                self.start[1] - half,
                self.start[1] + half,
            )
        } else {
            (
                self.start[0] - half,
                self.start[0] + half,
                self.start[1].min(self.end[1]),
                self.start[1].max(self.end[1]),
            )
        };
        ArchitectureBox {
            min: [room.x + x0, room.floor_y, room.z + z0],
            max: [room.x + x1, room.floor_y + self.clearance, room.z + z1],
        }
    }
    pub fn pieces(&self, room: &RoomDef) -> Vec<Piece<'_>> {
        let envelope = self.bounds(room);
        let [x0, y0, z0] = envelope.min;
        let [x1, y1, z1] = envelope.max;
        let t = self.thickness;
        let sides = if (self.end[0] - self.start[0]).abs() >= (self.end[1] - self.start[1]).abs() {
            [
                ([x0, y0, z0 - t], [x1, y1 + t, z0]),
                ([x0, y0, z1], [x1, y1 + t, z1 + t]),
            ]
        } else {
            [
                ([x0 - t, y0, z0], [x0, y1 + t, z1]),
                ([x1, y0, z0], [x1 + t, y1 + t, z1]),
            ]
        };
        sides
            .into_iter()
            .chain([([x0, y1, z0], [x1, y1 + t, z1])])
            .filter_map(|(a, b)| ArchitectureBox::from_corners(a, b))
            .map(|bounds| Piece {
                bounds,
                material: material(self.material.as_ref()),
            })
            .collect()
    }
}

pub fn pieces(level: &LevelDef) -> Vec<Piece<'_>> {
    let mut pieces: Vec<_> = level.walls.iter().flat_map(wall_pieces).collect();
    for room in &level.rooms {
        for tunnel in &room.tunnels {
            pieces.extend(tunnel.pieces(room));
        }
    }
    pieces
}

/// The actual backing slabs as ordinary wall carriers for the existing fitted
/// fixture operation. These are derived views, never serialized child objects.
pub fn recess_mount_carriers(level: &LevelDef) -> Vec<WallDef> {
    let mut result = Vec::new();
    for wall in &level.walls {
        for opening in &wall.openings {
            let Some(recess) = &opening.recess else {
                continue;
            };
            let mut isolated = wall.clone();
            isolated.openings = vec![opening.clone()];
            let Some(bounds) = wall_pieces(&isolated).first().map(|p| p.bounds) else {
                continue;
            };
            let mut carrier = wall.clone();
            carrier.x = bounds.min[0];
            carrier.y = bounds.min[1];
            carrier.z = bounds.min[2];
            carrier.width = bounds.max[0] - bounds.min[0];
            carrier.depth = bounds.max[2] - bounds.min[2];
            carrier.height = Some(bounds.max[1] - bounds.min[1]);
            carrier.openings.clear();
            carrier.trim = None;
            carrier.material = recess
                .back_material
                .clone()
                .or_else(|| wall.material.clone());
            result.push(carrier);
        }
    }
    result
}

/// Expand a path into ordinary guardrail runs. Each corner is an explicit run
/// boundary; the exact existing rail renderer and slope collider consume it.
pub fn rail_runs(rail: &GuardrailDef, level: &LevelDef) -> Vec<GuardrailDef> {
    if rail.path.is_empty() {
        return vec![rail.clone()];
    }
    let surfaces = LevelSurfaces::new(level);
    let mut runs = Vec::new();
    let mut start = [rail.x, rail.z];
    let mut carried_y = rail.y;
    for end in &rail.path {
        let dx = end[0] - start[0];
        let dz = end[1] - start[1];
        let mut cuts = vec![0.0_f32, 1.0];
        if rail.y.is_none() && rail.rise.is_none() {
            for bounds in level
                .stairs
                .iter()
                .map(super::StairDef::bounds)
                .chain(level.ramps.iter().map(super::RampDef::bounds))
            {
                for (edge, origin, delta) in [
                    (bounds.0, start[0], dx),
                    (bounds.1, start[0], dx),
                    (bounds.2, start[1], dz),
                    (bounds.3, start[1], dz),
                ] {
                    if delta.abs() > 0.0001 {
                        let t = (edge - origin) / delta;
                        if t > 0.0001 && t < 0.9999 {
                            cuts.push(t);
                        }
                    }
                }
            }
        }
        cuts.sort_by(f32::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 0.0001);
        if cuts.len() > 130 {
            return Vec::new();
        }
        for pair in cuts.windows(2) {
            let &[near, far] = pair else {
                continue;
            };
            let first = [dx.mul_add(near, start[0]), dz.mul_add(near, start[1])];
            let last = [dx.mul_add(far, start[0]), dz.mul_add(far, start[1])];
            let middle = [
                f32::midpoint(first[0], last[0]),
                f32::midpoint(first[1], last[1]),
            ];
            let floor_at = |point: [f32; 2]| {
                let floor = surfaces.floor_y_at(middle[0], middle[1]).unwrap_or(0.0);
                surfaces.stair_at(middle[0], middle[1]).map_or_else(
                    || {
                        surfaces
                            .floor_y_at(
                                (middle[0] - point[0]).mul_add(0.0001, point[0]),
                                (middle[1] - point[1]).mul_add(0.0001, point[1]),
                            )
                            .unwrap_or(floor)
                    },
                    |stair| {
                        floor - stair.offset_at(middle[0], middle[1])
                            + stair.surface().pitch_offset_at(point[0], point[1])
                    },
                )
            };
            let mut run = rail.clone();
            run.path.clear();
            run.x = first[0];
            run.z = first[1];
            run.length = dx.hypot(dz) * (far - near);
            run.rotation_degrees = (-dz).atan2(dx).to_degrees();
            // Auto mode samples the existing floor profile per section. Explicit
            // Y/rise remains available for manually supported angled geometry.
            run.y = Some(carried_y.unwrap_or_else(|| floor_at(first)));
            if rail.rise.is_none() {
                run.rise = Some(floor_at(last) - floor_at(first));
            }
            runs.push(run);
        }
        if (rail.y.is_some() || rail.rise.is_some())
            && let Some(last) = runs.last()
        {
            carried_y = Some(last.base_y_at(&surfaces, 1.0));
        }
        start = *end;
    }
    runs
}

/// Short swept boxes preserve angled paths and sloped stair clearance instead
/// of filling the entire axis-aligned bounding rectangle of a diagonal run.
pub fn rail_collision(rail: &GuardrailDef, level: &LevelDef) -> Vec<ArchitectureBox> {
    let surfaces = LevelSurfaces::new(level);
    if rail.path.is_empty() {
        return rail.solid_box(&surfaces).into_iter().collect();
    }
    let mut result = Vec::new();
    let runs = rail_runs(rail, level);
    for (index, run) in runs.iter().enumerate() {
        for step in 0..=1000_u16 {
            let along = f32::from(step) * 0.15;
            if along >= run.length {
                break;
            }
            let end = (along + 0.15).min(run.length);
            let mut piece = run.clone();
            let at = run.point_at(along / run.length, 0.0);
            piece.x = at.0;
            piece.z = at.1;
            piece.length = end - along;
            piece.y = Some(run.base_y_at(&surfaces, along / run.length));
            piece.rise = Some(run.resolved_rise(&surfaces) * (end - along) / run.length);
            if let Some(bounds) = piece.solid_box(&surfaces) {
                result.push(bounds);
            }
        }
        if let (_, Some(next)) = rail_neighbors(&runs, index) {
            let (x, z) = run.point_at(1.0, 0.0);
            let miter = rail_miter(run, next).unwrap_or(0.0).abs();
            let half = super::GUARDRAIL_RAIL_WIDTH_M * 0.5 * (1.0 + miter);
            result.extend(ArchitectureBox::from_corners(
                [
                    x - half,
                    run.base_y_at(&surfaces, 1.0).min(next.base_y(&surfaces)) - 0.2,
                    z - half,
                ],
                [
                    x + half,
                    run.base_y_at(&surfaces, 1.0).max(next.base_y(&surfaces)) + run.height(),
                    z + half,
                ],
            ));
        }
    }
    result
}

pub fn rail_neighbors(
    runs: &[GuardrailDef],
    index: usize,
) -> (Option<&GuardrailDef>, Option<&GuardrailDef>) {
    let closed = runs.len() > 2
        && runs.first().zip(runs.last()).is_some_and(|(first, last)| {
            let (x, z) = last.point_at(1.0, 0.0);
            (first.x - x).hypot(first.z - z) < 0.0001
        });
    let previous = index
        .checked_sub(1)
        .and_then(|i| runs.get(i))
        .or_else(|| closed.then(|| runs.last()).flatten());
    let next = runs
        .get(index.saturating_add(1))
        .or_else(|| closed.then(|| runs.first()).flatten());
    (previous, next)
}

/// Tangent bisector in the current run's local frame. One shared formula
/// controls the mitered mesh and its conservative corner collision coverage.
pub fn rail_miter(run: &GuardrailDef, neighbor: &GuardrailDef) -> Option<f32> {
    let (dx, dz) = run.direction();
    let (nx, nz) = neighbor.direction();
    let (ax, az) = run.across();
    let denominator = (dx + nx).mul_add(dx, (dz + nz) * dz);
    (denominator > 0.0001).then(|| -(dx + nx).mul_add(ax, (dz + nz) * az) / denominator)
}

pub fn validate(level: &LevelDef) -> Result<(), String> {
    let surfaces = LevelSurfaces::new(level);
    for (i, wall) in level.walls.iter().enumerate() {
        for (j, opening) in wall.openings.iter().enumerate() {
            if opening.kind == "recess" && opening.recess.is_none() {
                return Err("A recess requires a depth and solid backing; use passage/window for a through-cut".into());
            }
            if opening.recess.is_none() && opening.surround.is_none() {
                continue;
            }
            let prefix = format!(
                "Wall {} opening {}",
                i.saturating_add(1),
                j.saturating_add(1)
            );
            if opening.width < 0.05 || opening.height < 0.05 {
                return Err(format!(
                    "{prefix}: construction openings need at least 0.05 m width and height"
                ));
            }
            let height = wall.height.unwrap_or_else(|| {
                surfaces
                    .ceiling_y_along(wall, opening.offset)
                    .min(surfaces.ceiling_y_along(wall, opening.end()))
                    - wall.y
            });
            if opening.sill + opening.height > height + 0.0001 {
                return Err(format!("{prefix}: opening exceeds supported wall height"));
            }
            if wall.openings.iter().enumerate().any(|(k, other)| {
                k != j
                    && other.offset < opening.end() + 0.01
                    && other.end() > opening.offset - 0.01
                    && other.sill < opening.sill + opening.height + 0.01
                    && other.sill + other.height > opening.sill - 0.01
            }) {
                return Err(format!(
                    "{prefix}: construction attachment overlaps another aperture; adjust its dimensions"
                ));
            }
            if let Some(recess) = &opening.recess {
                if !recess.depth.is_finite()
                    || recess.depth < 0.02
                    || recess.depth > wall.thickness() - 0.05
                {
                    return Err(format!(
                        "{prefix}: recess needs 0.02 m depth and at least 0.05 m solid backing. Thicken this wall explicitly or use Make Doorway for a through-cut"
                    ));
                }
                if opening.glass.is_some() || opening.solid {
                    return Err(format!(
                        "{prefix}: recess backing is solid; remove opening glazing"
                    ));
                }
            }
            if let Some(frame) = &opening.surround {
                validate_surround(wall, opening, frame, height, &prefix)?;
            }
        }
    }
    validate_rails(level)?;
    for (i, room) in level.rooms.iter().enumerate() {
        if room.tunnels.len() > 16 {
            return Err("A room supports at most 16 crouch tunnels".into());
        }
        for (j, tunnel) in room.tunnels.iter().enumerate() {
            validate_tunnel(level, room, tunnel).map_err(|e| {
                format!(
                    "Room {} tunnel {}: {e}",
                    i.saturating_add(1),
                    j.saturating_add(1)
                )
            })?;
        }
    }
    Ok(())
}

fn validate_surround(
    wall: &WallDef,
    opening: &WallOpeningDef,
    frame: &SurroundDef,
    height: f32,
    prefix: &str,
) -> Result<(), String> {
    if [
        frame.width,
        frame.projection,
        frame.sill_projection,
        frame.trim_width,
    ]
    .iter()
    .any(|v| !v.is_finite())
        || !(0.02..=0.3).contains(&frame.width)
        || !(0.005..=0.2).contains(&frame.projection)
        || !(0.0..=0.4).contains(&frame.sill_projection)
        || !(0.0..=0.15).contains(&frame.trim_width)
    {
        return Err(format!(
            "{prefix}: use frame width 0.02..0.30 m, projection 0.005..0.20 m, sill 0..0.40 m and trim 0..0.15 m"
        ));
    }
    let margin = frame.width + frame.trim_width;
    if opening.offset < margin
        || opening.end() + margin > wall.length()
        || opening.sill + opening.height + margin > height
    {
        return Err(format!(
            "{prefix}: frame/trim must fit inside the carrier wall edges"
        ));
    }
    let lower = |o: &WallOpeningDef| {
        o.sill
            - o.surround.as_ref().map_or(0.0, |f| {
                if f.sill_projection > 0.0 && o.sill >= f.width {
                    f.width
                } else {
                    0.0
                }
            })
    };
    if wall.openings.iter().any(|other| {
        let other_margin = other
            .surround
            .as_ref()
            .map_or(0.0, |f| f.width + f.trim_width);
        !std::ptr::eq(opening, other)
            && opening.offset - margin < other.end() + other_margin - 0.0001
            && opening.end() + margin > other.offset - other_margin + 0.0001
            && lower(opening) < other.sill + other.height + other_margin - 0.0001
            && opening.sill + opening.height + margin > lower(other) + 0.0001
    }) {
        return Err(format!(
            "{prefix}: frame/trim overlaps a neighboring aperture or surround; reduce its width or move the openings explicitly"
        ));
    }
    Ok(())
}

fn overlaps(a: ArchitectureBox, b: ArchitectureBox) -> bool {
    a.min[0] < b.max[0] - 0.0001
        && a.max[0] > b.min[0] + 0.0001
        && a.min[1] < b.max[1] - 0.0001
        && a.max[1] > b.min[1] + 0.0001
        && a.min[2] < b.max[2] - 0.0001
        && a.max[2] > b.min[2] + 0.0001
}

fn validate_tunnel(level: &LevelDef, room: &RoomDef, tunnel: &TunnelDef) -> Result<(), String> {
    if tunnel
        .start
        .iter()
        .chain(&tunnel.end)
        .chain([&tunnel.width, &tunnel.clearance, &tunnel.thickness])
        .any(|v| !v.is_finite())
        || !(0.9..=3.0).contains(&tunnel.width)
        || !(CROUCH_HEIGHT + 0.05..=PLAYER_HEIGHT - 0.2).contains(&tunnel.clearance)
        || !(0.05..=0.3).contains(&tunnel.thickness)
    {
        return Err(
            "use width 0.9..3 m, crouch clearance 0.95..1.60 m and shell 0.05..0.30 m".into(),
        );
    }
    let dx = (tunnel.end[0] - tunnel.start[0]).abs();
    let dz = (tunnel.end[1] - tunnel.start[1]).abs();
    if dx.min(dz) > 0.0001 || !(1.0..=30.0).contains(&dx.max(dz)) {
        return Err("choose a straight X or Z route 1..30 m long; angled, bent and vertical tunnels are unsupported".into());
    }
    if !room.ceiling.is_flat()
        || room.height < PLAYER_HEIGHT + 0.1
        || tunnel.clearance + tunnel.thickness > room.height
    {
        return Err(
            "use a flat room with standing headroom at both mouths and room for the roof".into(),
        );
    }
    let b = tunnel.bounds(room);
    let landing = PLAYER_RADIUS.mul_add(2.0, 0.1);
    let mut envelope = b;
    envelope.min[0] -= if dx > dz { landing } else { tunnel.thickness };
    envelope.max[0] += if dx > dz { landing } else { tunnel.thickness };
    envelope.min[2] -= if dx > dz { tunnel.thickness } else { landing };
    envelope.max[2] += if dx > dz { tunnel.thickness } else { landing };
    envelope.max[1] = room.floor_y + PLAYER_HEIGHT + 0.05;
    if envelope.min[0] < room.x
        || envelope.max[0] > room.x + room.width
        || envelope.min[2] < room.z
        || envelope.max[2] > room.z + room.depth
    {
        return Err(
            "keep the full shell and 0.9 m entry/exit landings on the owning room floor".into(),
        );
    }
    // The same world solids used by the player include props and opening backs.
    // Remove generated tunnels from the comparison, then test the whole
    // envelope, including space to finish ducking/standing at either mouth.
    let mut obstacle_level = level.clone();
    for other in &mut obstacle_level.rooms {
        other.tunnels.clear();
    }
    for collider in obstacle_level.collision_aabbs() {
        let solid = ArchitectureBox {
            min: [collider.min_x, collider.min_y, collider.min_z],
            max: [collider.max_x, collider.max_y, collider.max_z],
        };
        if overlaps(envelope, solid) {
            return Err("route or landing intersects existing collision; move the endpoints or obstacle explicitly".into());
        }
    }
    tunnel_obstacles(level, tunnel, envelope)
}

fn rect_overlaps(bounds: (f32, f32, f32, f32), envelope: ArchitectureBox) -> bool {
    bounds.0 < envelope.max[0]
        && bounds.1 > envelope.min[0]
        && bounds.2 < envelope.max[2]
        && bounds.3 > envelope.min[2]
}

fn tunnel_obstacles(
    level: &LevelDef,
    tunnel: &TunnelDef,
    envelope: ArchitectureBox,
) -> Result<(), String> {
    let surfaces = LevelSurfaces::new(level);
    for prop in &level.props {
        let bounds = prop.placement_bounds(
            surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0),
            prop.size.unwrap_or(super::PROP_FALLBACK_SIZE),
        );
        if overlaps(
            envelope,
            ArchitectureBox {
                min: bounds.min,
                max: bounds.max,
            },
        ) {
            return Err(
                "a prop's placement bounds intersect the route or landing; move it explicitly"
                    .into(),
            );
        }
    }
    for light in &level.ceiling_lights {
        let profile = crate::engine::lighting::fixture_profile(&light.fixture);
        let radius = profile.half_width.hypot(profile.half_depth);
        let y = light
            .y
            .unwrap_or_else(|| surfaces.ceiling_y_at(light.x, light.z));
        let bounds = ArchitectureBox {
            min: [light.x - radius, y - 0.4, light.z - radius],
            max: [light.x + radius, y + 0.2, light.z + radius],
        };
        if overlaps(envelope, bounds) {
            return Err("a fixture housing is too close to the route or landing; move the light or passage explicitly".into());
        }
    }
    if level
        .ramps
        .iter()
        .map(super::RampDef::bounds)
        .chain(level.stairs.iter().map(super::StairDef::bounds))
        .chain(
            level
                .floor_regions
                .iter()
                .map(super::FloorRegionDef::bounds),
        )
        .any(|r| rect_overlaps(r, envelope))
    {
        return Err("route and both landings need one level floor; ramps, stairs and floor regions are unsupported".into());
    }
    for other_room in &level.rooms {
        for other in &other_room.tunnels {
            if !std::ptr::eq(other, tunnel)
                && other
                    .pieces(other_room)
                    .iter()
                    .any(|piece| overlaps(envelope, piece.bounds))
            {
                return Err(
                    "another tunnel intersects this shell or its entry/exit landings".into(),
                );
            }
        }
        if other_room.floor_y > envelope.min[1] + 0.001
            && other_room.floor_y < envelope.max[1]
            && rect_overlaps(other_room.bounds(), envelope)
        {
            return Err(
                "an upper floor intersects the passage or standing landing clearance".into(),
            );
        }
    }
    for leaf in crate::engine::door::Doors::from_level(level).colliders() {
        let mut bounds = ArchitectureBox {
            min: [f32::INFINITY, leaf.hinge_y, f32::INFINITY],
            max: [
                f32::NEG_INFINITY,
                leaf.hinge_y + leaf.height,
                f32::NEG_INFINITY,
            ],
        };
        for (t, side) in [(0.0, -1.0), (0.0, 1.0), (1.0, -1.0), (1.0, 1.0)] {
            let (x, z) = leaf.point_at(t, side);
            bounds.min[0] = bounds.min[0].min(x);
            bounds.min[2] = bounds.min[2].min(z);
            bounds.max[0] = bounds.max[0].max(x);
            bounds.max[2] = bounds.max[2].max(z);
        }
        if overlaps(envelope, bounds) {
            return Err("a door leaf's authored pose intersects the route or landing; move the route explicitly".into());
        }
    }
    Ok(())
}

fn validate_rails(level: &LevelDef) -> Result<(), String> {
    for (i, rail) in level.guardrails.iter().enumerate() {
        if rail.path.len() > 32 || rail.path.iter().flatten().any(|v| !v.is_finite()) {
            return Err(format!(
                "Railing {}: use at most 32 finite path endpoints",
                i.saturating_add(1)
            ));
        }
        if rail.path.is_empty() {
            continue;
        }
        let runs = rail_runs(rail, level);
        if runs.is_empty() || runs.len() > 128 || runs.iter().map(|r| r.length).sum::<f32>() > 200.0
        {
            return Err(
                "Railing path is too complex; use at most 128 surface sections and 200 m total"
                    .into(),
            );
        }
        let mut start = [rail.x, rail.z];
        for end in &rail.path {
            if !(0.2..=100.0).contains(&(end[0] - start[0]).hypot(end[1] - start[1])) {
                return Err(format!(
                    "Railing {}: sections must be 0.2..100 m; remove repeated points",
                    i.saturating_add(1)
                ));
            }
            start = *end;
        }
        validate_path_fit(level, rail, &runs)?;
    }
    Ok(())
}

fn validate_path_fit(
    level: &LevelDef,
    rail: &GuardrailDef,
    runs: &[GuardrailDef],
) -> Result<(), String> {
    let surfaces = LevelSurfaces::new(level);
    for (index, run) in runs.iter().enumerate() {
        if let (_, Some(next)) = rail_neighbors(runs, index) {
            let miter = rail_miter(run, next)
                .ok_or("Railing path doubles back at a corner; choose a wider turn")?;
            if miter.abs() * super::GUARDRAIL_RAIL_WIDTH_M * 0.5
                > run.length.min(next.length) * 0.45
            {
                return Err("Railing corner is too tight for its sections; move the corner or lengthen the sections".into());
            }
        }
    }
    for run in runs {
        if run.rise().abs() > super::MAX_RAMP_SLOPE.mul_add(run.length, 0.001) {
            return Err(
                "Railing route is too steep; add a supported landing or change endpoints".into(),
            );
        }
        for step in 0..=1000_u16 {
            let at = (f32::from(step) * 0.1).min(run.length);
            let (x, z) = run.point_at(at / run.length, 0.0);
            let floor = surfaces.floor_y_at(x, z).ok_or(
                "Railing path leaves supported room floors; add a floor or move the endpoints",
            )?;
            if rail.y.is_none()
                && rail.rise.is_none()
                && (run.base_y_at(&surfaces, at / run.length) - floor).abs() > 0.46
            {
                return Err(
                    "Railing path crosses an abrupt floor change; add a corner at the landing"
                        .into(),
                );
            }
            if surfaces.room_at(x, z).is_some_and(|r| !r.ceiling.is_open())
                && surfaces.ceiling_y_at(x, z)
                    < run.base_y_at(&surfaces, at / run.length) + rail.height
            {
                return Err(
                    "Railing/fence height intersects the room ceiling; lower its height".into(),
                );
            }
            if at >= run.length {
                break;
            }
        }
    }
    let mut obstacles = level.clone();
    obstacles.guardrails.clear();
    let colliders = obstacles.collision_aabbs();
    if rail_collision(rail, level).iter().any(|piece| {
        colliders.iter().any(|c| {
            c.max_y > piece.min[1] + 0.21
                && overlaps(
                    *piece,
                    ArchitectureBox {
                        min: [c.min_x, c.min_y, c.min_z],
                        max: [c.max_x, c.max_y, c.max_z],
                    },
                )
        })
    }) {
        return Err("Railing path intersects existing collision; move its path clear of walls, props and passage shells".into());
    }
    Ok(())
}
