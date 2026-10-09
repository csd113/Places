//! Shared offline air-volume validation for probe placement and final labels.

use crate::collision::WallAabb;
use crate::level::LevelSurfaces;
use crate::lighting::LevelLighting;
use crate::lighting::transport::{PROBE_CLEARANCE_M, TransportScene};

#[must_use]
pub fn placement_at(
    [x, y, z]: [f32; 3],
    surfaces: &LevelSurfaces<'_>,
    lighting: &LevelLighting,
    scene: &TransportScene,
    walls: &[WallAabb],
) -> (Option<usize>, &'static str) {
    if ![x, y, z].iter().all(|value| value.is_finite()) {
        return (None, "nonfinite-position");
    }
    let Some(room) = lighting.room_index_at_height(x, y, z) else {
        return (None, "outside-room-footprints");
    };
    let Some(volume) = lighting.rooms().get(room) else {
        return (None, "missing-room-volume");
    };
    let floor = volume.floor_y + surfaces.walkable_offset_at(x, z);
    let ceiling = volume.ceiling_y_at(x, z);
    let clearance = PROBE_CLEARANCE_M;
    if x < volume.x0 || x > volume.x1 || z < volume.z0 || z > volume.z1 {
        return (None, "outside-room-footprint");
    }
    if y < floor + clearance {
        return (None, "below-floor-clearance");
    }
    if y > ceiling - clearance {
        return (None, "above-ceiling-clearance");
    }
    if walls.iter().any(|wall| {
        x >= wall.min_x - clearance
            && x <= wall.max_x + clearance
            && y >= wall.min_y - clearance
            && y <= wall.max_y + clearance
            && z >= wall.min_z - clearance
            && z <= wall.max_z + clearance
    }) {
        return (None, "authored-wall-clearance");
    }
    if !scene.probe_is_clear([x, y, z]) {
        return (None, "opaque-surface-clearance-or-closed-solid");
    }
    (Some(room), "valid-air")
}
