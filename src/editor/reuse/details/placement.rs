//! Features propose locations; the shared decal mounting frame admits them.
use super::{DecalDef, ObjectId, Parameters};
use crate::engine::level::{
    DecalAlign, DecalSurface, LevelDef, LevelSurfaces, WallAxis, WallDef,
    wall_solid_slices_profiled,
};
use glam::Vec3;

const BUDGET: usize = 4_096;
type Proposal = (String, DecalDef);

fn decal(
    at: Vec3,
    size: f32,
    surface: DecalSurface,
    material: &str,
    rotation: f32,
    intensity: f32,
) -> DecalDef {
    DecalDef {
        x: at.x,
        y: at.y,
        z: at.z,
        width: size,
        height: size,
        rotation_degrees: rotation,
        opacity: intensity,
        material: material.into(),
        surface,
        align: DecalAlign::None,
    }
}
fn size(parameters: Parameters, key: &str, base: f32) -> f32 {
    base * parameters.scale
        * ((parameters.random(&format!("{key}:size")) - 0.5)
            .mul_add(0.3 * parameters.variation, 1.0))
}
fn face_point(wall: &WallDef, face: DecalSurface, along: f32, y: f32) -> Vec3 {
    let (x, z) = wall.min_corner();
    match face {
        DecalSurface::WallNorth => Vec3::new(x + along, y, z),
        DecalSurface::WallSouth => Vec3::new(x + along, y, z + wall.depth.abs()),
        DecalSurface::WallWest => Vec3::new(x, y, z + along),
        DecalSurface::WallEast => Vec3::new(x + wall.width.abs(), y, z + along),
        DecalSurface::Floor | DecalSurface::Ceiling => Vec3::ZERO,
    }
}
fn faces(wall: &WallDef) -> [DecalSurface; 2] {
    if wall.axis() == WallAxis::X {
        [DecalSurface::WallNorth, DecalSurface::WallSouth]
    } else {
        [DecalSurface::WallWest, DecalSurface::WallEast]
    }
}
fn key(wall: &WallDef, face: DecalSurface, suffix: &str) -> String {
    let (x, z) = wall.min_corner();
    format!(
        "{x}:{z}:{}:{}:{}:{face:?}:{suffix}",
        wall.y, wall.width, wall.depth
    )
}
fn selected_at(
    level: &LevelDef,
    selected: ObjectId,
    wall_index: usize,
    at: Vec3,
    face: DecalSurface,
) -> bool {
    let inside = Vec3::from_array(face.normal()).mul_add(Vec3::splat(0.03), at);
    match selected {
        ObjectId::Room(index) => level.rooms.get(index).is_some_and(|room| {
            room.contains(inside.x, inside.z)
                && at.y >= room.floor_y - 0.01
                && at.y <= room.floor_y + room.height
        }),
        ObjectId::Wall(index) => index == wall_index,
        ObjectId::Door(index) => level.doors.get(index).is_some_and(|door| {
            let floor = LevelSurfaces::new(level)
                .floor_y_at(door.x, door.z)
                .unwrap_or(0.0);
            if (at.y - (floor + door.y)).abs() > 0.01 {
                return false;
            }
            let yaw = door.rotation_degrees.to_radians();
            let center = Vec3::new(
                yaw.cos().mul_add(door.width * 0.5, door.x),
                at.y,
                (-yaw.sin()).mul_add(door.width * 0.5, door.z),
            );
            center.distance(at) <= 0.35
        }),
        ObjectId::FloorRegion(index) => level.floor_regions.get(index).is_some_and(|region| {
            inside.x >= region.x
                && inside.x <= region.x + region.width
                && inside.z >= region.z
                && inside.z <= region.z + region.depth
        }),
        ObjectId::Decal(_)
        | ObjectId::Column(_)
        | ObjectId::Pillar(_)
        | ObjectId::ArcWall(_)
        | ObjectId::Prop(_)
        | ObjectId::Ramp(_)
        | ObjectId::Light(_)
        | ObjectId::Audio(_)
        | ObjectId::Trigger(_)
        | ObjectId::FogRegion(_)
        | ObjectId::GroundFog(_)
        | ObjectId::SpawnPoint(_) => false,
    }
}

fn push_floor(
    level: &LevelDef,
    selected: ObjectId,
    candidate: DecalDef,
    key: String,
    output: &mut Vec<Proposal>,
) {
    if crate::editor::document::signs::floor_fit(level, &candidate, selected) {
        output.push((key, candidate));
    }
}
fn push_wall(
    level: &LevelDef,
    selected: ObjectId,
    wall_index: usize,
    mut candidate: DecalDef,
    key: String,
    output: &mut Vec<Proposal>,
) {
    let original = Vec3::new(candidate.x, candidate.y, candidate.z);
    let face = candidate.surface;
    if crate::editor::document::signs::fit(level, &mut candidate, wall_index, face, original, None)
        .is_ok()
        && original.distance(Vec3::new(candidate.x, candidate.y, candidate.z)) < 0.0001
        && room_wall_fit(level, selected, &candidate)
    {
        output.push((key, candidate));
    }
}
fn room_wall_fit(level: &LevelDef, selected: ObjectId, decal: &DecalDef) -> bool {
    let ObjectId::Room(index) = selected else {
        return true;
    };
    let Some(room) = level.rooms.get(index) else {
        return false;
    };
    let normal = Vec3::from_array(decal.surface.normal());
    crate::engine::render::decal_quad_points(decal).is_some_and(|points| {
        points.iter().all(|point| {
            let probe = normal.mul_add(Vec3::splat(0.03), Vec3::from_array(*point));
            room.contains(probe.x, probe.z)
                && probe.y >= room.floor_y
                && probe.y <= room.floor_y + room.height
        })
    })
}
fn floor_rotation(face: DecalSurface) -> f32 {
    let normal = Vec3::from_array(face.normal());
    (-normal.x).atan2(-normal.z).to_degrees()
}
fn floor_edges(
    level: &LevelDef,
    selected: ObjectId,
    parameters: Parameters,
    index: usize,
    face: DecalSurface,
    output: &mut Vec<Proposal>,
    attempts: &mut usize,
) -> Result<(), String> {
    let wall = level.walls.get(index).ok_or("Missing wall")?;
    let surfaces = LevelSurfaces::new(level);
    let slices = wall_solid_slices_profiled(
        wall,
        |offset| surfaces.clear_ceiling_height_along(wall, offset),
        &surfaces.wall_profile_breaks(wall),
    );
    let spacing = 0.85 * parameters.scale;
    for slot in 0_u16..=4_096 {
        let along = f32::from(slot).mul_add(spacing, spacing * 0.5);
        if along >= wall.length() {
            break;
        }
        *attempts = attempts.saturating_add(1);
        if *attempts > BUDGET {
            return Err("Detail scope exceeds 4096 feature samples; select a smaller room or individual wall".into());
        }
        let identity = key(wall, face, &format!("edge:{slot}"));
        let side = size(parameters, &identity, 0.65);
        let jitter =
            (parameters.random(&format!("{identity}:along")) - 0.5) * 0.08 * parameters.variation;
        let position = face_point(wall, face, along + jitter, wall.y);
        let inward =
            Vec3::from_array(face.normal()).mul_add(Vec3::splat(side.mul_add(0.5, 0.03)), position);
        if selected_at(level, selected, index, position, face)
            && parameters.random(&identity) <= parameters.density
            && slices.iter().any(|slice| {
                slice.start <= side.mul_add(-0.5, along + jitter)
                    && slice.end >= side.mul_add(0.5, along + jitter)
                    && slice.bottom <= wall.y + 0.001
                    && slice.top >= wall.y + 0.1
            })
        {
            push_floor(
                level,
                selected,
                decal(
                    inward,
                    side,
                    DecalSurface::Floor,
                    "core:detail_floor_grime_01",
                    floor_rotation(face),
                    parameters.intensity,
                ),
                identity,
                output,
            );
        }
    }
    Ok(())
}

fn openings(
    level: &LevelDef,
    selected: ObjectId,
    parameters: Parameters,
    index: usize,
    face: DecalSurface,
    output: &mut Vec<Proposal>,
) -> Result<(), String> {
    let wall = level.walls.get(index).ok_or("Missing wall")?;
    for opening in &wall.openings {
        let along = opening.width.mul_add(0.5, opening.offset);
        let at = face_point(wall, face, along, wall.y);
        if !selected_at(level, selected, index, at, face) {
            continue;
        }
        let identity = key(
            wall,
            face,
            &format!(
                "{}:{}:{}:{}",
                opening.kind, opening.offset, opening.width, opening.sill
            ),
        );
        if parameters.random(&identity) > parameters.density {
            continue;
        }
        if opening.kind == "vent" && opening.recess.is_none() && parameters.includes(2) {
            let side = size(parameters, &identity, 0.65).min(opening.width);
            let center = face_point(
                wall,
                face,
                along,
                side.mul_add(-0.5, opening.bottom(wall.y)) - 0.015,
            );
            push_wall(
                level,
                selected,
                index,
                decal(
                    center,
                    side,
                    face,
                    "core:detail_vent_stain_01",
                    0.0,
                    parameters.intensity,
                ),
                identity,
                output,
            );
        } else if opening.is_door() && opening.reaches_floor() && parameters.includes(3) {
            let side = size(parameters, &identity, 0.65).min(opening.width * 0.8);
            let inward =
                Vec3::from_array(face.normal()).mul_add(Vec3::splat(side.mul_add(0.5, 0.03)), at);
            push_floor(
                level,
                selected,
                decal(
                    inward,
                    side,
                    DecalSurface::Floor,
                    "core:detail_door_wear_01",
                    floor_rotation(face),
                    parameters.intensity * 0.65,
                ),
                format!("{identity}:floor"),
                output,
            );
            for (suffix, a) in [
                ("left", parameters.scale.mul_add(-0.16, opening.offset)),
                ("right", parameters.scale.mul_add(0.16, opening.end())),
            ] {
                let center = face_point(wall, face, a, wall.y + 0.65);
                push_wall(
                    level,
                    selected,
                    index,
                    decal(
                        center,
                        0.25 * parameters.scale,
                        face,
                        "core:detail_door_wear_01",
                        0.0,
                        parameters.intensity * 0.5,
                    ),
                    format!("{identity}:{suffix}"),
                    output,
                );
            }
        }
    }
    Ok(())
}

struct Vent<'a> {
    id: ObjectId,
    point: Vec3,
    span: f32,
    asset: &'a str,
    ceiling: bool,
    rotation: f32,
}
fn vent_model(id: &str) -> bool {
    id == "core:ceiling_diffuser_01"
        || id.ends_with("_vent")
        || id.contains(":vent_")
        || id.contains(":air_vent")
        || id.contains(":wall_vent")
        || id.contains(":ceiling_vent")
}
fn vent_features<'a>(
    level: &'a LevelDef,
    catalog: &'a crate::engine::assets::AssetCatalog,
) -> impl Iterator<Item = Vent<'a>> {
    level
        .props
        .iter()
        .enumerate()
        .filter(|(_, prop)| {
            vent_model(&prop.model)
                && prop.pitch_degrees.abs() < 0.001
                && prop.roll_degrees.abs() < 0.001
        })
        .map(move |(index, prop)| {
            let base = LevelSurfaces::new(level)
                .floor_y_at(prop.x, prop.z)
                .unwrap_or(0.0);
            let [width, _, depth] = prop
                .size
                .or_else(|| catalog.get(&prop.model).and_then(|entry| entry.size))
                .unwrap_or([0.6, 0.1, 0.6]);
            Vent {
                id: ObjectId::Prop(index),
                point: Vec3::new(prop.x, base + prop.y, prop.z),
                span: width.max(depth) * prop.scale,
                asset: &prop.model,
                ceiling: prop.model.contains("ceiling") || prop.model == "core:ceiling_diffuser_01",
                rotation: prop.rotation_degrees,
            }
        })
        .chain(
            level
                .decals
                .iter()
                .enumerate()
                .filter(|(_, decal)| {
                    decal.material == "core:decal_ceiling_vent_01"
                        && decal.surface != DecalSurface::Floor
                })
                .map(move |(index, decal)| {
                    let ceiling = decal.surface == DecalSurface::Ceiling;
                    let y = if ceiling {
                        LevelSurfaces::new(level).ceiling_y_at(decal.x, decal.z)
                    } else {
                        crate::engine::render::decal_quad_points(decal).map_or(decal.y, |points| {
                            points
                                .iter()
                                .fold(decal.y, |bottom, point| bottom.min(point[1]))
                        })
                    };
                    Vent {
                        id: ObjectId::Decal(index),
                        point: Vec3::new(decal.x, y, decal.z),
                        span: decal.width,
                        asset: &decal.material,
                        ceiling,
                        rotation: -level.ceiling_decal_rotation(decal),
                    }
                }),
        )
}
fn vent_room(
    level: &LevelDef,
    point: Vec3,
    ceiling_sheet: bool,
) -> Option<(usize, &crate::engine::level::RoomDef)> {
    let mut chosen: Option<(usize, &crate::engine::level::RoomDef)> = None;
    for (index, room) in level.rooms.iter().enumerate() {
        if !room.contains(point.x, point.z)
            || point.y < room.floor_y - 0.001
            || point.y > room.ceiling_y_at(point.x, point.z) + 0.001
            || (ceiling_sheet && point.y <= room.floor_y + 0.001)
        {
            continue;
        }
        if chosen.is_none_or(|(_, prior)| room.floor_y > prior.floor_y) {
            chosen = Some((index, room));
        }
    }
    chosen
}
fn vent_objects(
    level: &LevelDef,
    catalog: &crate::engine::assets::AssetCatalog,
    selected: ObjectId,
    parameters: Parameters,
    output: &mut Vec<Proposal>,
) -> Result<(), String> {
    let mut count = 0_usize;
    for feature in vent_features(level, catalog) {
        let Vent {
            id,
            point,
            span,
            asset,
            ceiling,
            rotation,
        } = feature;
        let Some((room_index, room)) =
            vent_room(level, point, matches!(id, ObjectId::Decal(_)) && ceiling)
        else {
            continue;
        };
        let in_scope = selected == id
            || matches!(selected,ObjectId::Room(index) if index==room_index)
            || (matches!(selected, ObjectId::Wall(_)) && !ceiling);
        if !in_scope {
            continue;
        }
        count = count.saturating_add(1);
        if count > super::LIMIT {
            return Err("More than 1024 vent features; choose an individual vent".into());
        }
        let identity = format!("vent:{asset}:{}:{}:{}", point.x, point.y, point.z);
        if parameters.random(&identity) > parameters.density {
            continue;
        }
        let side = size(parameters, &identity, span.max(0.3));
        let receiver = ObjectId::Room(room_index);
        if ceiling {
            let at = Vec3::new(point.x, room.floor_y, point.z);
            let angle = (parameters.random(&format!("{identity}:angle")) - 0.5)
                .mul_add(16.0 * parameters.variation, rotation);
            push_floor(
                level,
                receiver,
                decal(
                    at,
                    side,
                    DecalSurface::Floor,
                    "core:detail_vent_stain_01",
                    angle,
                    parameters.intensity * 0.65,
                ),
                identity,
                output,
            );
        } else {
            for (index, wall) in level.walls.iter().enumerate() {
                if matches!(selected,ObjectId::Wall(i) if i!=index) {
                    continue;
                }
                for face in faces(wall) {
                    let along = if wall.axis() == WallAxis::X {
                        point.x - wall.x
                    } else {
                        point.z - wall.z
                    };
                    let contact = face_point(wall, face, along, point.y);
                    let from_face = Vec3::new(
                        point.x - contact.x,
                        point.y - contact.y,
                        point.z - contact.z,
                    );
                    if from_face.length() > 0.15
                        || from_face.dot(Vec3::from_array(face.normal())) < -0.001
                    {
                        continue;
                    }
                    let at = Vec3::new(contact.x, side.mul_add(-0.5, point.y) - 0.03, contact.z);
                    push_wall(
                        level,
                        receiver,
                        index,
                        decal(
                            at,
                            side,
                            face,
                            "core:detail_vent_stain_01",
                            0.0,
                            parameters.intensity,
                        ),
                        format!("{identity}:{face:?}"),
                        output,
                    );
                }
            }
        }
    }
    Ok(())
}

pub(super) fn generate(
    level: &LevelDef,
    catalog: &crate::engine::assets::AssetCatalog,
    selected: ObjectId,
    parameters: Parameters,
) -> Result<Vec<Proposal>, String> {
    if !matches!(
        selected,
        ObjectId::Room(_)
            | ObjectId::Wall(_)
            | ObjectId::Door(_)
            | ObjectId::Prop(_)
            | ObjectId::Decal(_)
            | ObjectId::FloorRegion(_)
    ) {
        return Err(
            "Select a room floor, wall, door, vent object or an existing generated detail".into(),
        );
    }
    let mut output = Vec::new();
    if parameters.density <= 0.0 {
        return Ok(output);
    }
    let mut attempts = 0_usize;
    for (index, wall) in level.walls.iter().enumerate() {
        // Bound before scanning long or unrelated walls.
        if matches!(selected,ObjectId::Wall(i) if i!=index)
            || matches!(selected, ObjectId::Prop(_) | ObjectId::Decal(_))
        {
            continue;
        }
        for face in faces(wall) {
            if let ObjectId::Room(room_index) = selected
                && !level.rooms.get(room_index).is_some_and(|room| {
                    let (x, z) = wall.min_corner();
                    x <= room.x + room.width
                        && x + wall.width.abs() >= room.x
                        && z <= room.z + room.depth
                        && z + wall.depth.abs() >= room.z
                        && (wall.y - room.floor_y).abs() < 0.01
                })
            {
                continue;
            }
            if parameters.includes(1) && !matches!(selected, ObjectId::Door(_)) {
                floor_edges(
                    level,
                    selected,
                    parameters,
                    index,
                    face,
                    &mut output,
                    &mut attempts,
                )?;
            }
            openings(level, selected, parameters, index, face, &mut output)?;
            if output.len() > super::LIMIT {
                return Err("More than 1024 details; reduce density or select one wall".into());
            }
        }
    }
    if parameters.includes(2) {
        vent_objects(level, catalog, selected, parameters, &mut output)?;
    }
    if output.len() > super::LIMIT {
        return Err("More than 1024 details; reduce density or select one feature".into());
    }
    Ok(output)
}
