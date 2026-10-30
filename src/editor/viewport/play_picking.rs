//! Exact visible architecture triangles, without grid snapping or a world plane.
use super::{PickObject, ray_triangle};
use crate::engine::{level::WalkableFloor, render::LevelMesh};
use glam::Vec3;
pub(in crate::editor::viewport) fn play_floor_hit(
    mesh: &LevelMesh,
    objects: &[PickObject],
    floor: &WalkableFloor,
    doors: &[crate::engine::collision::DoorCollider],
    (origin, direction): (Vec3, Vec3),
) -> Option<Vec3> {
    let mut nearest: Option<(f32, bool)> = None;
    for range in &mesh.ranges {
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
            // The viewport renders single-sided architecture: ceilings seen from above are culled.
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
            ) && nearest.is_none_or(|hit| distance < hit.0)
            {
                nearest = Some((distance, normal.y > 0.5));
            }
        }
    }
    let (distance, up) = nearest?;
    if !up
        || doors
            .iter()
            .filter_map(|door| door.ray_entry(origin, direction, distance))
            .any(|blocker| blocker < distance - 0.001)
        || objects
            .iter()
            .filter(|object| {
                matches!(
                    object.id,
                    crate::editor::document::ObjectId::Prop(_)
                        | crate::editor::document::ObjectId::Light(_)
                )
            })
            .filter_map(|object| object.distance(origin, direction))
            .any(|blocker| blocker < distance - 0.001)
    {
        return None;
    }
    let hit = direction.mul_add(Vec3::splat(distance), origin);
    floor
        .height_below(hit.x, hit.z, hit.y + 0.001)
        .filter(|height| (*height - hit.y).abs() <= 0.001)
        .map(|_| hit)
}
