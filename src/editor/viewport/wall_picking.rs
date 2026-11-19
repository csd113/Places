//! Door and doorway targeting against the accepted editing scene.

use std::sync::Arc;

use glam::Vec3;

use super::{PickObject, play_picking};
use crate::editor::document::{ObjectId, wall_edit::DoorwayTarget};
use crate::engine::{
    collision::DoorCollider,
    level::{LevelDef, LevelSurfaces, WallAxis, WallDef},
    render::{LevelMesh, SurfaceKind},
};

pub(in crate::editor) struct WallPicking {
    pub(in crate::editor::viewport) mesh: Arc<LevelMesh>,
    pub(in crate::editor::viewport) objects: Arc<[PickObject]>,
    pub(in crate::editor::viewport) doors: Arc<[DoorCollider]>,
}

impl WallPicking {
    pub(in crate::editor) fn target(
        &self,
        level: &LevelDef,
        (origin, direction): (Vec3, Vec3),
    ) -> Result<(usize, DoorwayTarget), &'static str> {
        let surfaces = LevelSurfaces::new(level);
        let (index, target, distance) = self
            .objects
            .iter()
            .filter_map(|object| {
                let ObjectId::Wall(index) = object.id else {
                    return None;
                };
                let wall = level.walls.get(index)?;
                let (target, distance) = wall_hit(object, wall, &surfaces, origin, direction)?;
                Some((index, target, distance))
            })
            .min_by(|first, second| first.2.total_cmp(&second.2))
            .ok_or("Point at a visible wall or doorway")?;

        // Drawn architecture includes floor/ceiling faces and unselectable trim.
        // Door leaves are oriented solids, unlike their selection bounds.
        let limit = distance - 0.001;
        if play_picking::front_face_hit(&self.mesh, origin, direction, limit, |kind| {
            kind != SurfaceKind::Decal
        })
        .is_some()
            || play_picking::foreground_before(&self.objects, &self.doors, origin, direction, limit)
        {
            return Err("Foreground geometry blocks the wall or doorway");
        }
        Ok((index, target))
    }
}

fn wall_hit(
    object: &PickObject,
    wall: &WallDef,
    surfaces: &LevelSurfaces<'_>,
    origin: Vec3,
    direction: Vec3,
) -> Option<(DoorwayTarget, f32)> {
    // A face hit already has a depth and may project past the wall's end at
    // the centre plane. Only search that plane when the ray misses the solid.
    let solid = object.distance(origin, direction);
    let (x, z) = wall.min_corner();
    let distance = solid.unwrap_or_else(|| {
        let (cross, start, delta) = match wall.axis() {
            WallAxis::X => (z, origin.z, direction.z),
            WallAxis::Z => (x, origin.x, direction.x),
        };
        (wall.thickness().mul_add(0.5, cross) - start) / delta
    });
    if !(0.0..=500.0).contains(&distance) {
        return None;
    }
    let hit = direction.mul_add(Vec3::splat(distance), origin);
    let along = match wall.axis() {
        WallAxis::X => hit.x - x,
        WallAxis::Z => hit.z - z,
    };
    if solid.is_none() && !(0.0..=wall.length()).contains(&along) {
        return None;
    }
    let offset = along.clamp(0.0, wall.length());
    let height = hit.y - wall.y;
    let clear = wall
        .height
        .unwrap_or_else(|| surfaces.clear_ceiling_height_along(wall, offset));
    if !(0.0..=clear).contains(&height) {
        return None;
    }
    if solid.is_some() {
        return Some((DoorwayTarget::WallFace(offset), distance));
    }

    // A clear aperture carries its authored index into the edit. Rounding
    // the pointer offset could move it out of the opening or into another one.
    let index = wall.openings.iter().position(|opening| {
        (opening.offset..=opening.end()).contains(&offset)
            && (opening.sill..=opening.sill + opening.height).contains(&height)
    })?;
    Some((DoorwayTarget::Aperture { index, offset }, distance))
}
