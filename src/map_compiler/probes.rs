//! Compiler-only air-volume validation for the serialized probe lattice.

use crate::engine::collision::WallAabb;
use crate::engine::level::{LevelDef, LevelSurfaces};
use crate::engine::lighting::LevelLighting;
use crate::engine::lighting::probe_placement::placement_at;
use crate::engine::lighting::probes::ProbeField;
use crate::engine::lighting::transport::TransportScene;

/// Label only positions in actual air. Runtime's forgiving room lookup is
/// useful for shading boundary vertices, but must not place probes in solids.
pub(super) fn label(
    field: &mut ProbeField,
    level: &LevelDef,
    lighting: &LevelLighting,
    scene: &TransportScene,
    walls: &[WallAabb],
    quality: &str,
) {
    let surfaces = LevelSurfaces::new(level);
    field.assign_rooms(|position| room_at(position, &surfaces, lighting, scene, walls));
    if crate::engine::lighting::transport::probe_audit::enabled() {
        let placements = placements(field, &surfaces, lighting, scene, walls);
        if let Err(error) =
            crate::engine::lighting::transport::probe_audit::dump_validity(quality, &placements)
        {
            crate::engine::logging::warn(format_args!("[probe-diagnostics] {error}"));
        }
    }
}

#[derive(serde::Serialize)]
struct Placement {
    id: usize,
    position: [f32; 3],
    room: Option<usize>,
    status: &'static str,
}

fn placements(
    field: &ProbeField,
    surfaces: &LevelSurfaces<'_>,
    lighting: &LevelLighting,
    scene: &TransportScene,
    walls: &[WallAabb],
) -> Vec<Placement> {
    field
        .probes
        .iter()
        .enumerate()
        .filter_map(|(id, _)| {
            let position = field.probe_position(id)?;
            let (room, status) = placement_at(position, surfaces, lighting, scene, walls);
            Some(Placement {
                id,
                position,
                room,
                status,
            })
        })
        .collect()
}

fn room_at(
    [x, y, z]: [f32; 3],
    surfaces: &LevelSurfaces<'_>,
    lighting: &LevelLighting,
    scene: &TransportScene,
    walls: &[WallAabb],
) -> Option<usize> {
    placement_at([x, y, z], surfaces, lighting, scene, walls).0
}
