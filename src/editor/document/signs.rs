//! Author the hub's existing file-backed decals; artwork owns the lettering.
use glam::Vec3;
#[cfg(test)]
#[path = "../tests/signs.rs"]
mod tests;
use super::{Document, ObjectId, Transform, objects};
use crate::engine::level::{
    DecalDef, DecalSurface, LevelDef, LevelSurfaces, WallAxis, wall_solid_slices_profiled,
};
use serde_json::{Value, json};

pub(in crate::editor) const ART: [(&str, &str); 10] = [
    ("home", "HOME"),
    ("pool", "POOL"),
    ("office", "OFFICE"),
    ("arcade", "COSMIC LANES"),
    ("lantern_hollow", "LANTERN HOLLOW"),
    ("winter", "WINTER"),
    ("beach", "BEACH"),
    ("aero", "FRUTIGER AERO"),
    ("hub", "CENTRAL HUB"),
    ("welcome", "PLACES"),
];
pub(super) fn transform(level: &LevelDef, index: usize) -> Result<Transform, String> {
    let decal = level.decals.get(index).ok_or("Sign no longer exists")?;
    Ok(Transform {
        position: [decal.x, decal.y, decal.z],
        size: [decal.width, decal.height, 1.0],
        yaw_degrees: decal.rotation_degrees,
        pitch_degrees: 0.0,
        roll_degrees: 0.0,
    })
}
pub(super) fn set_transform(
    raw: &mut Value,
    level: &LevelDef,
    index: usize,
    pose: Transform,
) -> Result<(), String> {
    let object = objects::object_mut(raw, ObjectId::Decal(index))?;
    let decal = level.decals.get(index).ok_or("Sign no longer exists")?;
    if (pose.size[2] - 1.0).abs() > 0.0001 {
        return Err("A flat sign has width and height, without depth".into());
    }
    if decal.material.starts_with("collection:sign_")
        && pose.size[1].mul_add(-4.0, pose.size[0]).abs() > 0.0001
    {
        return Err("Hub sign artwork requires Width = 4 x Height. Use the sign size buttons or change both in Pose.".into());
    }
    objects::set_position(object, pose.position, "y");
    objects::set_number(object, "width", pose.size[0]);
    objects::set_number(object, "height", pose.size[1]);
    objects::set_number(object, "rotation_degrees", pose.yaw_degrees);
    Ok(())
}

/// Exact quad frame shared with the production decal emitter, including bias.
pub(in crate::editor) fn corners(decal: &DecalDef) -> Option<[Vec3; 4]> {
    let normal = Vec3::from_array(decal.surface.normal());
    crate::engine::render::decal_quad_points(decal).map(|points| {
        points.map(|point| {
            normal.mul_add(
                Vec3::splat(crate::engine::render::DECAL_SURFACE_OFFSET_M),
                Vec3::from_array(point),
            )
        })
    })
}

fn overlaps_rect(bounds: [f32; 4], x: f32, z: f32, width: f32, depth: f32) -> bool {
    let [x0, x1, z0, z1] = bounds;
    x1 > x.min(x + width) + 0.00001
        && x0 < x.max(x + width) - 0.00001
        && z1 > z.min(z + depth) + 0.00001
        && z0 < z.max(z + depth) - 0.00001
}
pub(in crate::editor) fn floor_fit(
    level: &LevelDef,
    candidate: &DecalDef,
    selected: ObjectId,
) -> bool {
    let Some(points) = crate::engine::render::decal_quad_points(candidate) else {
        return false;
    };
    let bounds = points.iter().fold(
        [
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ],
        |[x0, x1, z0, z1], point| {
            [
                x0.min(point[0]),
                x1.max(point[0]),
                z0.min(point[2]),
                z1.max(point[2]),
            ]
        },
    );
    let [x0, x1, z0, z1] = bounds;
    // The entire conservative rotated footprint must lie on one real room.
    // Reject terrain/step intersections rather than projecting a quad across them.
    let room_fits = |room: &crate::engine::level::RoomDef| {
        room.contains(x0, z0) && room.contains(x1, z1) && (room.floor_y - candidate.y).abs() < 0.001
    };
    if !level.rooms.iter().any(room_fits)
        || matches!(selected, ObjectId::Room(index) if !level.rooms.get(index).is_some_and(room_fits))
        || level
            .floor_regions
            .iter()
            .any(|r| overlaps_rect(bounds, r.x, r.z, r.width, r.depth))
        || level
            .ramps
            .iter()
            .any(|r| overlaps_rect(bounds, r.x, r.z, r.width, r.depth))
        || level
            .stairs
            .iter()
            .any(|r| overlaps_rect(bounds, r.x, r.z, r.width, r.depth))
    {
        return false;
    }
    let surfaces = LevelSurfaces::new(level);
    if !points.iter().all(|p| {
        surfaces
            .floor_y_at(p[0], p[2])
            .is_some_and(|y| (y - candidate.y).abs() < 0.001)
    }) {
        return false;
    }
    for wall in &level.walls {
        if !overlaps_rect(bounds, wall.x, wall.z, wall.width, wall.depth) {
            continue;
        }
        let (wx, wz) = wall.min_corner();
        let (lo, hi) = if wall.axis() == WallAxis::X {
            (x0 - wx, x1 - wx)
        } else {
            (z0 - wz, z1 - wz)
        };
        if wall_solid_slices_profiled(
            wall,
            |offset| surfaces.clear_ceiling_height_along(wall, offset),
            &surfaces.wall_profile_breaks(wall),
        )
        .iter()
        .any(|slice| {
            slice.start < hi
                && slice.end > lo
                && slice.bottom <= candidate.y + 0.01
                && slice.top > candidate.y
        }) {
            return false;
        }
    }
    true
}
/// Real emitted horizontal planes, also used for authoring selection.
pub(in crate::editor) fn receiving_corners(
    level: &LevelDef,
    decal: &DecalDef,
) -> Option<[Vec3; 4]> {
    let surfaces = LevelSurfaces::new(level);
    let normal = decal.surface.normal();
    crate::engine::render::decal_quad_points_rotated(decal, level.ceiling_decal_rotation(decal))
        .map(|points| {
            points.map(|mut point| {
                if decal.surface == DecalSurface::Floor {
                    point[1] = surfaces.floor_y_at(point[0], point[2]).unwrap_or(point[1]);
                } else if decal.surface == DecalSurface::Ceiling {
                    point[1] = surfaces.ceiling_y_at(point[0], point[2]);
                }
                Vec3::new(
                    normal[0].mul_add(crate::engine::render::DECAL_SURFACE_OFFSET_M, point[0]),
                    normal[1].mul_add(crate::engine::render::DECAL_SURFACE_OFFSET_M, point[1]),
                    normal[2].mul_add(crate::engine::render::DECAL_SURFACE_OFFSET_M, point[2]),
                )
            })
        })
}
fn receiving_plane(level: &LevelDef, decal: &DecalDef) -> f32 {
    let surfaces = LevelSurfaces::new(level);
    match decal.surface {
        DecalSurface::Floor => surfaces.floor_y_at(decal.x, decal.z).unwrap_or(decal.y),
        DecalSurface::Ceiling => surfaces.ceiling_y_at(decal.x, decal.z),
        DecalSurface::WallNorth
        | DecalSurface::WallSouth
        | DecalSurface::WallWest
        | DecalSurface::WallEast => normal_coordinate(decal),
    }
}
fn duplicate_floor(level: &LevelDef, index: usize) -> Result<[f32; 3], String> {
    let original = level.decals.get(index).ok_or("Decal no longer exists")?;
    let (half_x, half_z) = half_span(original);
    for (dx, dz) in [
        (half_x.mul_add(2.0, 0.1), 0.0),
        (-half_x.mul_add(2.0, 0.1), 0.0),
        (0.0, half_z.mul_add(2.0, 0.1)),
        (0.0, -half_z.mul_add(2.0, 0.1)),
    ] {
        let mut copy = original.clone();
        copy.x += dx;
        copy.z += dz;
        copy.y = receiving_plane(level, original);
        if floor_fit(level, &copy, ObjectId::Decal(index)) && clear_of_signs(level, &copy) {
            return Ok([copy.x, copy.y, copy.z]);
        }
    }
    Err("No clear, supported floor area fits this copy. Move or shrink the decal first.".into())
}

fn half_span(decal: &DecalDef) -> (f32, f32) {
    let (sine, cosine) = decal.rotation_degrees.to_radians().sin_cos();
    (
        f32::midpoint(decal.width * cosine.abs(), decal.height * sine.abs()),
        f32::midpoint(decal.width * sine.abs(), decal.height * cosine.abs()),
    )
}
const fn normal_coordinate(decal: &DecalDef) -> f32 {
    if decal.surface.is_horizontal() {
        decal.y
    } else if decal.surface.normal()[0].abs() > 0.5 {
        decal.x
    } else {
        decal.z
    }
}
const fn along_coordinate(decal: &DecalDef) -> f32 {
    if decal.surface.is_horizontal() {
        decal.x
    } else if decal.surface.normal()[0].abs() > 0.5 {
        decal.z
    } else {
        decal.x
    }
}
pub(in crate::editor) fn clear_of_signs(level: &LevelDef, candidate: &DecalDef) -> bool {
    let (width, height) = half_span(candidate);
    level
        .decals
        .iter()
        .filter(|other| {
            other.surface == candidate.surface
                && (receiving_plane(level, other) - receiving_plane(level, candidate)).abs()
                    < 0.0001
        })
        .all(|other| {
            let (other_width, other_height) = half_span(other);
            let across = |decal: &DecalDef| {
                if decal.surface.is_horizontal() {
                    decal.z
                } else {
                    decal.y
                }
            };
            (along_coordinate(other) - along_coordinate(candidate)).abs()
                >= width + other_width + 0.01
                || (across(other) - across(candidate)).abs() >= height + other_height + 0.01
        })
}
/// A copied plate must fit its carrier without overlapping another sign.
pub(super) fn duplicate_position(level: &LevelDef, index: usize) -> Result<[f32; 3], String> {
    let original = level.decals.get(index).ok_or("Sign no longer exists")?;
    if original.surface == DecalSurface::Floor {
        return duplicate_floor(level, index);
    }
    if original.surface == DecalSurface::Ceiling {
        return Err("Duplicate ceiling artwork manually on a checked carrier".into());
    }
    let (half_width, half_height) = half_span(original);
    let origin = Vec3::new(original.x, original.y, original.z);
    let wall_axis = if original.surface.normal()[0].abs() > 0.5 {
        WallAxis::Z
    } else {
        WallAxis::X
    };
    let mut best: Option<(f32, [f32; 3])> = None;
    for (horizontal, vertical) in [
        (half_width.mul_add(2.0, 0.1), 0.0),
        (-half_width.mul_add(2.0, 0.1), 0.0),
        (0.0, half_height.mul_add(2.0, 0.1)),
        (0.0, -half_height.mul_add(2.0, 0.1)),
    ] {
        let at = match wall_axis {
            WallAxis::X => Vec3::new(original.x + horizontal, original.y + vertical, original.z),
            WallAxis::Z => Vec3::new(original.x, original.y + vertical, original.z + horizontal),
        };
        for (wall_index, _) in level
            .walls
            .iter()
            .enumerate()
            .filter(|(_, wall)| wall.axis() == wall_axis)
        {
            let mut candidate = original.clone();
            if fit(
                level,
                &mut candidate,
                wall_index,
                original.surface,
                at,
                None,
            )
            .is_err()
                || (normal_coordinate(&candidate) - normal_coordinate(original)).abs() > 0.0001
                || !clear_of_signs(level, &candidate)
            {
                continue;
            }
            let position = Vec3::new(candidate.x, candidate.y, candidate.z);
            let distance = origin.distance_squared(position);
            if best.is_none_or(|(prior, _)| distance < prior) {
                best = Some((distance, position.to_array()));
            }
        }
    }
    best.map(|(_,position)|position).ok_or_else(||"No clear supported wall area fits this copy. Move or shrink the sign before duplicating.".into())
}

/// Fit one whole decal to a real solid wall slice. Openings are never bridged.
pub(in crate::editor) fn fit(
    level: &LevelDef,
    decal: &mut DecalDef,
    wall_index: usize,
    face: DecalSurface,
    at: Vec3,
    step: Option<f32>,
) -> Result<(), String> {
    let wall = level.walls.get(wall_index).ok_or("Point at a wall")?;
    let surfaces = LevelSurfaces::new(level);
    let (x, z) = wall.min_corner();
    let (hw, hh) = half_span(decal);
    let along = if wall.axis() == WallAxis::X {
        at.x - x
    } else {
        at.z - z
    };
    let snap = |v: f32| step.map_or(v, |s| (v / s).round() * s);
    let slices = wall_solid_slices_profiled(
        wall,
        |offset| surfaces.clear_ceiling_height_along(wall, offset),
        &surfaces.wall_profile_breaks(wall),
    );
    if hw * 2.0 > wall.length() {
        return Err("The sign is wider than this wall; choose a smaller size".into());
    }
    let requested = snap(along).clamp(hw, wall.length() - hw);
    let covered = |a: f32, y: f32| {
        let mut intervals: Vec<_> = slices
            .iter()
            .filter(|slice| slice.bottom <= y - hh + 0.00001 && slice.top >= y + hh - 0.00001)
            .map(|slice| (slice.start, slice.end))
            .collect();
        intervals.sort_by(|first, second| first.0.total_cmp(&second.0));
        let mut edge = a - hw;
        for (lo, hi) in intervals {
            if lo > edge + 0.00001 {
                break;
            }
            if hi >= edge {
                edge = edge.max(hi);
            }
            if edge >= a + hw - 0.00001 {
                return true;
            }
        }
        false
    };
    let mut best = None;
    for slice in &slices {
        let bottom = slice.bottom + hh;
        let top = slice.top - hh;
        if bottom > top {
            continue;
        }
        let y = snap(at.y).clamp(bottom, top);
        let lo = slice.start + hw;
        let hi = slice.end - hw;
        let alternatives = [
            requested,
            if lo <= hi {
                snap(along).clamp(lo, hi)
            } else {
                requested
            },
        ];
        for a in alternatives {
            let distance = (a - along).mul_add(a - along, (y - at.y).powi(2));
            if best.is_none_or(|(prior, _, _)| distance < prior) && covered(a, y) {
                best = Some((distance, a, y));
            }
        }
    }
    let (_,fitted_along,y)=best.ok_or("This wall has no solid area large enough for the sign; choose a smaller size or another wall")?;
    decal.y = y;
    decal.surface = face;
    match face {
        DecalSurface::WallNorth => {
            decal.x = x + fitted_along;
            decal.z = z;
        }
        DecalSurface::WallSouth => {
            decal.x = x + fitted_along;
            decal.z = z + wall.depth.abs();
        }
        DecalSurface::WallWest => {
            decal.x = x;
            decal.z = z + fitted_along;
        }
        DecalSurface::WallEast => {
            decal.x = x + wall.width.abs();
            decal.z = z + fitted_along;
        }
        DecalSurface::Floor | DecalSurface::Ceiling => return Err("Choose a wall face".into()),
    }
    Ok(())
}

pub(in crate::editor) fn placement(
    doc: &Document,
    origin: Vec3,
    direction: Vec3,
    step: Option<f32>,
    artwork: usize,
) -> Result<(Value, ObjectId), String> {
    let (art, _) = ART.get(artwork).ok_or("Choose sign artwork")?;
    let mut decal = DecalDef {
        x: 0.0,
        y: 0.0,
        z: 0.0,
        width: 2.4,
        height: 0.6,
        rotation_degrees: 0.0,
        opacity: 1.0,
        material: format!("collection:sign_{art}"),
        surface: DecalSurface::WallSouth,
        align: crate::engine::level::DecalAlign::None,
    };
    let mut best = None;
    for (i, wall) in doc.level().walls.iter().enumerate() {
        let (x, z) = wall.min_corner();
        let faces = if wall.axis() == WallAxis::X {
            [
                (DecalSurface::WallNorth, z),
                (DecalSurface::WallSouth, z + wall.depth.abs()),
            ]
        } else {
            [
                (DecalSurface::WallWest, x),
                (DecalSurface::WallEast, x + wall.width.abs()),
            ]
        };
        for (face, plane) in faces {
            let n = Vec3::from_array(face.normal());
            if direction.dot(n) >= -0.0001 {
                continue;
            }
            let component = if wall.axis() == WallAxis::X { 2 } else { 0 };
            let distance = (plane - origin[component]) / direction[component];
            if !distance.is_finite() || !(0.0..=500.0).contains(&distance) {
                continue;
            }
            let at = direction.mul_add(Vec3::splat(distance), origin);
            let along = if wall.axis() == WallAxis::X {
                at.x - x
            } else {
                at.z - z
            };
            let surfaces = LevelSurfaces::new(doc.level());
            if along < 0.0
                || along > wall.length()
                || at.y < wall.y
                || at.y
                    > wall.y
                        + wall.resolved_height(surfaces.clear_ceiling_height_along(wall, along))
            {
                continue;
            }
            if best.is_none_or(|(prior, _, _, _)| distance < prior) {
                best = Some((distance, i, face, at));
            }
        }
    }
    let (_, i, face, at) = best.ok_or("Point at the broad face of a rectangular wall")?;
    fit(doc.level(), &mut decal, i, face, at, step)?;
    if !clear_of_signs(doc.level(), &decal) {
        return Err(
            "This sign would overlap an existing wall decal. Choose a clear wall area.".into(),
        );
    }
    let mut raw = doc.raw().clone();
    let entries = objects::array_mut(&mut raw, "decals")?;
    let id = ObjectId::Decal(entries.len());
    entries.push(serde_json::to_value(decal).map_err(|e| e.to_string())?);
    Ok((raw, id))
}

impl Document {
    pub(in crate::editor) fn sign_artwork(
        &mut self,
        id: ObjectId,
        artwork: usize,
    ) -> Result<(), String> {
        let ObjectId::Decal(_) = id else {
            return Err("Select a wall sign".into());
        };
        let (art, _) = ART.get(artwork).ok_or("Choose sign artwork")?;
        let mut raw = self.raw().clone();
        let record = objects::object_mut(&mut raw, id)?;
        drop(record.insert("material".into(), format!("collection:sign_{art}").into()));
        let width = record
            .get("width")
            .and_then(Value::as_f64)
            .ok_or("Sign width missing")?;
        drop(record.insert("height".into(), json!(width / 4.0_f64)));
        self.accept_edit(raw)
    }
    pub(in crate::editor) fn mount_sign(
        &mut self,
        id: ObjectId,
        step: Option<f32>,
    ) -> Result<(), String> {
        self.mount_sign_facing(id, step, None)
    }
    pub(in crate::editor) fn mount_sign_facing(
        &mut self,
        id: ObjectId,
        step: Option<f32>,
        requested_face: Option<DecalSurface>,
    ) -> Result<(), String> {
        let ObjectId::Decal(i) = id else {
            return Err("Select a sign".into());
        };
        let mut decal = self
            .level()
            .decals
            .get(i)
            .ok_or("Sign no longer exists")?
            .clone();
        decal.surface = requested_face.unwrap_or(decal.surface);
        if decal.surface.is_horizontal() {
            return Err("Floor/ceiling decals keep their existing placement".into());
        }
        let at = Vec3::new(decal.x, decal.y, decal.z);
        let mut best = None;
        for (wall_index, _) in self.level().walls.iter().enumerate() {
            let mut candidate = decal.clone();
            let face = decal.surface;
            let compatible = matches!(
                (
                    self.level()
                        .walls
                        .get(wall_index)
                        .map(crate::engine::level::WallDef::axis),
                    face
                ),
                (
                    Some(WallAxis::X),
                    DecalSurface::WallNorth | DecalSurface::WallSouth
                ) | (
                    Some(WallAxis::Z),
                    DecalSurface::WallWest | DecalSurface::WallEast
                )
            );
            if compatible && fit(self.level(), &mut candidate, wall_index, face, at, step).is_ok() {
                let distance =
                    at.distance_squared(Vec3::new(candidate.x, candidate.y, candidate.z));
                if best.as_ref().is_none_or(|(prior, _)| distance < *prior) {
                    best = Some((distance, candidate));
                }
            }
        }
        decal = best.ok_or("No compatible wall face fits this sign")?.1;
        let mut raw = self.raw().clone();
        let record = objects::object_mut(&mut raw, id)?;
        for (key, value) in [("x", decal.x), ("y", decal.y), ("z", decal.z)] {
            objects::set_number(record, key, value);
        }
        drop(record.insert(
            "surface".into(),
            serde_json::to_value(decal.surface).map_err(|e| e.to_string())?,
        ));
        self.accept_edit(raw)
    }
}
