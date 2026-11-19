//! Exact visible architecture triangles, without grid snapping or a world plane.
use super::{ObjectId, PickObject, ray_box, ray_triangle};
use crate::engine::{
    level::WalkableFloor,
    render::{LevelMesh, SurfaceKind},
};
use glam::Vec3;
pub(in crate::editor::viewport) fn play_floor_hit(
    mesh: &LevelMesh,
    objects: &[PickObject],
    floor: &WalkableFloor,
    doors: &[crate::engine::collision::DoorCollider],
    (origin, direction): (Vec3, Vec3),
) -> Option<Vec3> {
    let (distance, up) = front_face_hit(mesh, origin, direction, f32::INFINITY, |_| true)?;
    if !up || foreground_before(objects, doors, origin, direction, distance - 0.001) {
        return None;
    }
    let hit = direction.mul_add(Vec3::splat(distance), origin);
    floor
        .height_below(hit.x, hit.z, hit.y + 0.001)
        .filter(|height| (*height - hit.y).abs() <= 0.001)
        .map(|_| hit)
}

pub(super) fn front_face_hit(
    mesh: &LevelMesh,
    origin: Vec3,
    direction: Vec3,
    limit: f32,
    include: impl Fn(SurfaceKind) -> bool,
) -> Option<(f32, bool)> {
    let mut nearest: Option<(f32, bool)> = None;
    for range in &mesh.ranges {
        if !include(range.key.kind)
            || ray_box(origin, direction, range.bounds).is_none_or(|distance| distance >= limit)
        {
            continue;
        }
        for indices in range.indices.as_chunks::<3>().0 {
            let [a, b, c] = *indices;
            let (Some(first), Some(second), Some(third)) = (
                range.vertices.get(usize::from(a)),
                range.vertices.get(usize::from(b)),
                range.vertices.get(usize::from(c)),
            ) else {
                continue;
            };
            let normal = Vec3::from_array(first.normal);
            // Architecture is single-sided; the ceiling above a room is culled.
            if normal.dot(direction) >= 0.0 {
                continue;
            }
            if let Some(distance) = ray_triangle(
                origin,
                direction,
                [
                    Vec3::from_array(first.pos),
                    Vec3::from_array(second.pos),
                    Vec3::from_array(third.pos),
                ],
            ) && distance < limit
                && nearest.is_none_or(|hit| distance < hit.0)
            {
                nearest = Some((distance, normal.y > 0.5));
            }
        }
    }
    nearest
}

pub(super) fn foreground_before(
    objects: &[PickObject],
    doors: &[crate::engine::collision::DoorCollider],
    origin: Vec3,
    direction: Vec3,
    limit: f32,
) -> bool {
    doors
        .iter()
        .filter_map(|door| door.ray_entry(origin, direction, limit))
        .chain(
            objects
                .iter()
                .filter(|object| matches!(object.id, ObjectId::Prop(_) | ObjectId::Light(_)))
                .filter_map(|object| object.distance(origin, direction)),
        )
        .any(|blocker| blocker < limit)
}
