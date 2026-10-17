//! Picking follows the same region precedence and signed ramp surface as play.

use super::{Aabb, LevelDef, LevelSurfaces, ObjectId, PickObject, Vec3};

pub(super) fn terrain_objects(level: &LevelDef) -> Vec<PickObject> {
    let surfaces = LevelSurfaces::new(level);
    let mut objects: Vec<PickObject> = Vec::new();
    for (room_index, room) in level.rooms.iter().enumerate() {
        let grid = surfaces.floor_grid(room);
        let mut xs = grid.xs;
        let mut zs = grid.zs;
        let (rx0, rx1, rz0, rz1) = room.bounds();
        for ramp in surfaces.ramps_for_room(room) {
            let (x0, x1, z0, z1) = ramp.bounds();
            xs.extend([x0.clamp(rx0, rx1), x1.clamp(rx0, rx1)]);
            zs.extend([z0.clamp(rz0, rz1), z1.clamp(rz0, rz1)]);
        }
        xs.sort_by(f32::total_cmp);
        xs.dedup();
        zs.sort_by(f32::total_cmp);
        zs.dedup();
        for x_span in xs.windows(2) {
            let &[x0, x1] = x_span else {
                continue;
            };
            for z_span in zs.windows(2) {
                let &[z0, z1] = z_span else {
                    continue;
                };
                let x = f32::midpoint(x0, x1);
                let z = f32::midpoint(z0, z1);
                let slope = level
                    .ramps
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, ramp)| ramp.contains(x, z));
                let region_match = level
                    .floor_regions
                    .iter()
                    .enumerate()
                    .rev()
                    .find(|(_, region)| region.contains(x, z));
                let id = slope.map_or_else(
                    || {
                        region_match.map_or(ObjectId::Room(room_index), |(index, _)| {
                            ObjectId::FloorRegion(index)
                        })
                    },
                    |(index, _)| ObjectId::Ramp(index),
                );
                let height = |px, pz| {
                    room.floor_y
                        + slope.map_or_else(
                            || region_match.map_or(0.0, |(_, region)| region.offset_y),
                            |(_, ramp)| ramp.offset_at(px, pz),
                        )
                };
                let points = [
                    Vec3::new(x0, height(x0, z0), z0),
                    Vec3::new(x1, height(x1, z0), z0),
                    Vec3::new(x1, height(x1, z1), z1),
                    Vec3::new(x0, height(x0, z1), z1),
                ];
                let index = objects
                    .iter()
                    .position(|object| object.id == id)
                    .unwrap_or_else(|| {
                        objects.push(PickObject::boxed(id, Aabb::EMPTY, Vec::new()));
                        objects.len().saturating_sub(1)
                    });
                if let Some(object) = objects.get_mut(index) {
                    for point in points {
                        object.bounds.expand(point.to_array());
                    }
                    let [northwest, northeast, southeast, southwest] = points;
                    object.triangles.extend([
                        [northwest, northeast, southeast],
                        [northwest, southeast, southwest],
                    ]);
                    if matches!(id, ObjectId::Room(_)) {
                        object.bounds.expand([x0, room.eave_y(), z0]);
                    } else {
                        object.bounds.expand([x0, room.floor_y, z0]);
                    }
                }
            }
        }
    }
    objects
}
