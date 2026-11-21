//! Compiler identities for reflected sky and air-volume checks for irradiance probes.

use crate::engine::assets::AssetCatalog;
use crate::engine::collision::WallAabb;
use crate::engine::level::{LevelDef, LevelSurfaces};
use crate::engine::lighting::LevelLighting;
use crate::engine::lighting::probe_placement::placement_at;
use crate::engine::lighting::probes::ProbeField;
use crate::engine::lighting::transport::TransportScene;
use crate::engine::package::{DependencyKind, PackageDependency, hash::sha256_hex};

/// Match the sky inputs submitted during reflection capture. File identities
/// come from the same observed dependencies as the encoded texture readers.
pub(super) fn sky_fingerprint(
    level: &LevelDef,
    catalog: &AssetCatalog,
    dependencies: &[PackageDependency],
) -> Result<String, String> {
    let image = |id| {
        catalog.texture_path(id).map(|path| {
            let identity = dependencies.iter().find(|dependency| {
                dependency.kind == DependencyKind::Texture && dependency.path == path
            });
            (path, identity)
        })
    };
    let background = level
        .sky_background_texture()
        .map(|id| (image(id), level.sky.as_ref().map(|sky| sky.brightness)));
    let discs: Vec<_> = level
        .global_illuminators
        .iter()
        .take(crate::engine::level::MAX_GLOBAL_ILLUMINATORS)
        .filter_map(|light| {
            let visual = light.active_sky_visual()?;
            let id = light.visual_texture()?;
            Some((light.kind, light.source_direction(), visual, image(id)))
        })
        .collect();
    let bytes = crate::engine::canonical_json::canonical_json_bytes(&(
        "reflection-sky-v1",
        background,
        discs,
    ))?;
    Ok(sha256_hex(&bytes))
}

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

pub(super) fn append_coverage_warnings(
    field: &ProbeField,
    room_count: usize,
    quality: &str,
    warnings: &mut Vec<String>,
) {
    let mut covered = vec![false; room_count];
    for probe in &field.probes {
        if let Ok(room) = usize::try_from(probe.room)
            && let Some(room_coverage) = covered.get_mut(room)
        {
            *room_coverage = true;
        }
    }
    for (room, is_covered) in covered.iter().enumerate() {
        if !is_covered {
            warnings.push(format!(
                "{quality} room {room} has no valid irradiance probe at {:.3} m spacing",
                field.cell_m
            ));
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
