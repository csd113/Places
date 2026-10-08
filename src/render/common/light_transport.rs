//! Building the offline transport scene from a prepared level build.
//!
//! The transport solver needs plain world-space triangles with a diffuse
//! albedo, plus the resolved light sources. This module assembles them from the
//! same data the renderer draws: the prepared [`LevelMesh`] ranges (skipping the
//! decal pass, which is a visual overlay and not an occluder), the placed prop
//! batches, and the level's material table.
//!
//! Diffuse albedo per surface is the material's authored tint multiplied by the
//! texture's sampled colour at the triangle's centre, in linear light. A prop vertex already carries its material's `baseColorFactor`
//! in its vertex colour, so a prop triangle's albedo is that colour times its
//! submesh texture. This is the receiving side of the transport equation: the
//! albedo modulates what a bounce *emits*, and it is never baked into the stored
//! light the shader multiplies.
//!
//! Emitter classification is the compiled one: each [`BakedLight`] becomes a
//! transport emitter, and a ceiling fixture the level marks `switchable` is
//! tagged with its light index so the solver can solve its contribution into
//! its own prepared layer set.

use crate::level::{LevelDef, WaterVolumes};
use crate::lighting::LevelLighting;
use crate::lighting::lightmap::{Chart, LightmapPatch};
use crate::lighting::transport::{
    TransportAlphaSurface, TransportEmitter, TransportScene, TransportTextureAddress,
    TransportTriangle, TransportWaterBody, probe_targets, receiver_targets,
};
use crate::materials::{AlphaMode, MaterialTable, RawImage};
use crate::render::{LevelMesh, MATERIAL_NONE, PropMeshBatch, SurfaceKind, Vertex};

/// World-space tolerance, in metres, for matching a triangle's corners to a
/// water volume's surface plane and footprint.
const WATER_SURFACE_EPS_M: f32 = 1.0e-3;

/// What one transport-scene build produced, for the developer report.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransportSceneStats {
    /// Triangles in the scene.
    pub triangles: usize,
    /// Triangles skipped as degenerate or non-finite.
    pub skipped_triangles: usize,
    /// Resolved emitters.
    pub emitters: usize,
    /// Emitters tagged switchable.
    pub switchable_emitters: usize,
}

/// Builds the transport scene for one prepared build.
///
/// `charts` is the finished lightmap plan's chart list. It is needed here so
/// the authored baseline target of every receiver and probe can be resolved
/// once, from the same zone-grid lookup the vertex bake used, and stored with
/// the scene the solve later runs against.
///
/// Returns `None` when the scene exceeds the transport solver's triangle budget;
/// the caller then fails the variant over to the vertex-lit build by name,
/// exactly like a lightmap plan failure.
#[must_use]
pub fn build_transport_scene(
    level: &LevelDef,
    mesh: &LevelMesh,
    batches: &[PropMeshBatch],
    materials: &MaterialTable,
    lighting: &LevelLighting,
    charts: &[(LightmapPatch, Chart)],
) -> Option<(TransportScene, TransportSceneStats)> {
    let mut stats = TransportSceneStats::default();
    let mut triangles: Vec<TransportTriangle> = Vec::new();
    let mut surface_alpha = Vec::new();
    let mut owners = Vec::new();
    // Water bodies transmit and attenuate; the drawn surface triangles are
    // matched against the same resolved volumes the player swims in. A dry
    // level must not pay for the floor resolution the water emitter also skips.
    let water = if level.water.is_empty() {
        WaterVolumes::new()
    } else {
        WaterVolumes::from_level(level)
    };
    append_architecture_triangles(
        mesh,
        materials,
        &water,
        &mut triangles,
        &mut surface_alpha,
        &mut stats,
        &mut owners,
    );
    append_prop_triangles(
        batches,
        &mut triangles,
        &mut surface_alpha,
        &mut stats,
        &mut owners,
    );

    let mut emitters: Vec<TransportEmitter> = Vec::new();
    let switchable = switchable_lights(level, lighting);
    for (index, light) in lighting.lights().iter().enumerate() {
        let tag = switchable
            .iter()
            .copied()
            .find(|candidate| *candidate == index);
        // A switchable fixture is prepared even when it is authored off, so a
        // runtime toggle can turn its contribution on; a plainly inactive
        // light contributes nothing to any solve.
        if !light.is_active() && tag.is_none() {
            continue;
        }
        emitters.push(TransportEmitter::from_baked(light, tag));
        if tag.is_some() {
            stats.switchable_emitters = stats.switchable_emitters.saturating_add(1);
        }
    }
    stats.triangles = triangles.len();
    stats.emitters = emitters.len();
    let scene = TransportScene::new(triangles, emitters)?
        .with_surface_alpha(surface_alpha)?
        .with_water(
            water
                .volumes()
                .iter()
                .map(TransportWaterBody::from_volume)
                .collect(),
        )
        .with_receiver_target(receiver_targets(lighting, charts))
        .with_probe_target(probe_targets(lighting, charts))
        .with_sky(sky_radiance(level));
    let lit_scene = scene.with_global_lights(
        level
            .global_illuminators
            .iter()
            .filter_map(crate::lighting::directional::DirectionalLight::from_definition)
            .collect(),
    );
    if let Err(error) = crate::lighting::transport::diagnostics::dump_scene(&lit_scene, &owners) {
        crate::logging::warn(format_args!("[lighting-diagnostics] {error}"));
    }
    Some((lit_scene, stats))
}

/// The runtime environment sample. Prepared values stay linear HDR until
/// the material shader reconstructs diffuse lighting at its world normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityLighting {
    pub prepared: Option<crate::lighting::lightmap::LightmapTexel>,
    /// Legacy field name; values are linear HDR, never encoded display RGB.
    pub display: [f32; 3],
    pub source: EntityLightingSource,
}

/// Detectable fallback reasons; darkness in a valid probe is intentional.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EntityLightingSource {
    Prepared,
    NoField,
    Roomless,
    Unresolved,
    InvalidPosition,
}

/// Sample at the transformed model-bounds centre, in metres, Y up.
/// A failed lookup uses the existing linear authored environment model. No brightness floor is applied to valid baked energy.
#[must_use]
pub fn entity_lighting(
    lighting: &LevelLighting,
    irradiance: Option<&crate::lighting::probes::ProbeField>,
    position: [f32; 3],
) -> EntityLighting {
    let finite = position.iter().all(|v| v.is_finite());
    let room = finite
        .then(|| lighting.room_index_at_height(position[0], position[1], position[2]))
        .flatten()
        .filter(|r| lighting.probe_region_at(*r, position).is_some());
    let prepared = room.and_then(|_| {
        irradiance.and_then(|field| {
            field.sample_filtered_with_rooms(position, None, |probe, label| {
                lighting.labelled_probe_visible_from(position, probe, label)
            })
        })
    });
    let source = if !finite {
        EntityLightingSource::InvalidPosition
    } else if irradiance.is_none() {
        EntityLightingSource::NoField
    } else if room.is_none() {
        EntityLightingSource::Roomless
    } else if prepared.is_none() {
        EntityLightingSource::Unresolved
    } else {
        EntityLightingSource::Prepared
    };
    let display = prepared.map_or_else(
        || {
            // Invalid transforms never enter the spatial lookup. The environment
            // at the map origin supplies a deterministic diagnostic fallback.
            let p = if finite { position } else { [0.0; 3] };
            let light = lighting.sample(p[0], p[1], p[2]);
            [light.r, light.g, light.b].map(|v| {
                if v.is_finite() {
                    v.max(0.0)
                } else {
                    crate::lighting::AMBIENT_LEVEL
                }
            })
        },
        |t| t.irradiance,
    );
    EntityLighting {
        prepared,
        display,
        source,
    }
}

/// Isotropic linear light used by compatibility tests.
#[cfg(test)]
#[must_use]
pub fn moving_object_light(
    lighting: &LevelLighting,
    irradiance: Option<&crate::lighting::probes::ProbeField>,
    position: [f32; 3],
) -> [f32; 3] {
    entity_lighting(lighting, irradiance, position).display
}

/// The radiance an escaping bounce ray sees, from the level's optional sky.
///
/// Zero without a sky (or with `ambient` 0.0), which preserves the historical
/// "an interior has no sky" solve exactly. The authored scalar is scaled by
/// [`crate::lighting::SKY_AMBIENT_COLOR`].
fn sky_radiance(level: &LevelDef) -> [f32; 3] {
    let Some(sky) = level.sky.as_ref() else {
        return [0.0; 3];
    };
    let ambient = if sky.ambient.is_finite() {
        sky.ambient.clamp(0.0, crate::level::MAX_SKY_AMBIENT)
    } else {
        0.0
    };
    crate::lighting::SKY_AMBIENT_COLOR.map(|channel| channel * ambient)
}

/// Appends every architecture range's triangles to the transport scene.
///
/// Decals and fixture geometry are skipped; walls, floors, ceilings,
/// architecture and prop fallbacks occlude. Water transmits into its attenuating
/// body; other surfaces retain the renderer's alpha coverage at each ray hit.
fn append_architecture_triangles(
    mesh: &LevelMesh,
    materials: &MaterialTable,
    water: &WaterVolumes,
    triangles: &mut Vec<TransportTriangle>,
    surface_alpha: &mut Vec<Option<TransportAlphaSurface>>,
    stats: &mut TransportSceneStats,
    owners: &mut Vec<crate::lighting::transport::diagnostics::CasterRange>,
) {
    for range in &mesh.ranges {
        // Decals are a visual overlay, and fixture geometry is the analytic
        // emitters' own housing: keeping it would let a fixture shadow itself
        // (its authored luminous face sits between the room and the emitter
        // point), which the historical occluder classification never did.
        // Walls, floors, ceilings, architecture and prop fallbacks occlude.
        if matches!(range.key.kind, SurfaceKind::Decal | SurfaceKind::Light) {
            continue;
        }
        let first = triangles.len();
        let material = material_albedo(materials, range.key.material);
        let material_id = material_id(materials, range.key.material);
        for chunk in range.indices.as_chunks::<3>().0 {
            let (Some(a), Some(b), Some(c)) = (chunk.first(), chunk.get(1), chunk.get(2)) else {
                continue;
            };
            let (Some(va), Some(vb), Some(vc)) = (
                range.vertices.get(usize::from(*a)),
                range.vertices.get(usize::from(*b)),
                range.vertices.get(usize::from(*c)),
            ) else {
                continue;
            };
            let albedo = material;
            match TransportTriangle::new(va.pos, vb.pos, vc.pos, albedo) {
                Some(triangle) => {
                    let corners = [va.pos, vb.pos, vc.pos];
                    let water_surface = is_water_surface(corners, material_id, water);
                    triangles.push(triangle.with_transmissive(water_surface));
                    surface_alpha.push(if water_surface {
                        None
                    } else {
                        materials.entry(range.key.material).and_then(|entry| {
                            alpha_surface(
                                entry.alpha,
                                entry.image.clone(),
                                [va, vb, vc],
                                TransportTextureAddress::Repeat,
                            )
                        })
                    });
                }
                None => stats.skipped_triangles = stats.skipped_triangles.saturating_add(1),
            }
        }
        if crate::lighting::transport::diagnostics::enabled() {
            owners.push(crate::lighting::transport::diagnostics::CasterRange {
                first,
                end: triangles.len(),
                owner: format!(
                    "architecture:{:?}:{}",
                    range.key.kind,
                    material_id.unwrap_or("untextured")
                ),
            });
        }
    }
}

/// Prop sheets clamp their UVs; alpha coverage and material factors match the
/// draw path without turning the whole card into either a blocker or a hole.
fn append_prop_triangles(
    batches: &[PropMeshBatch],
    triangles: &mut Vec<TransportTriangle>,
    surface_alpha: &mut Vec<Option<TransportAlphaSurface>>,
    stats: &mut TransportSceneStats,
    owners: &mut Vec<crate::lighting::transport::diagnostics::CasterRange>,
) {
    for batch in batches {
        let first = triangles.len();
        for submesh in &batch.submeshes {
            let start = usize::try_from(submesh.first_index).unwrap_or(usize::MAX);
            let count = usize::try_from(submesh.index_count).unwrap_or(usize::MAX);
            let end = start.saturating_add(count);
            let image = submesh
                .texture
                .and_then(|index| batch.textures.get(usize::from(index)))
                .map(AsRef::as_ref);
            let mut index = start;
            while index < end {
                let Some(chunk) = batch.indices.get(index..index.saturating_add(3)) else {
                    break;
                };
                index = index.saturating_add(3);
                let (Some(a), Some(b), Some(c)) = (chunk.first(), chunk.get(1), chunk.get(2))
                else {
                    continue;
                };
                let (Some(va), Some(vb), Some(vc)) = (
                    batch.vertices.get(usize::from(*a)),
                    batch.vertices.get(usize::from(*b)),
                    batch.vertices.get(usize::from(*c)),
                ) else {
                    continue;
                };
                let albedo = triangle_albedo([1.0; 3], va, vb, vc, image);
                match TransportTriangle::new(va.pos, vb.pos, vc.pos, albedo) {
                    Some(triangle) => {
                        triangles
                            .push(triangle.with_shading_normals([va.normal, vb.normal, vc.normal]));
                        surface_alpha.push(alpha_surface(
                            submesh.alpha,
                            submesh
                                .texture
                                .and_then(|texture| batch.textures.get(usize::from(texture)))
                                .cloned(),
                            [va, vb, vc],
                            TransportTextureAddress::Clamp,
                        ));
                    }
                    None => stats.skipped_triangles = stats.skipped_triangles.saturating_add(1),
                }
            }
        }
        if crate::lighting::transport::diagnostics::enabled() {
            owners.push(crate::lighting::transport::diagnostics::CasterRange {
                first,
                end: triangles.len(),
                owner: format!("model:{}", batch.model),
            });
        }
    }
}

/// The level material id one surface key resolves to, or `None` when the key
/// binds no level material (a fixture housing or a prop fallback).
fn material_id(materials: &MaterialTable, material: u32) -> Option<&str> {
    if material == MATERIAL_NONE {
        return None;
    }
    materials.entry(material).map(|entry| entry.id.as_str())
}

fn alpha_surface(
    alpha: crate::materials::MaterialAlpha,
    image: Option<std::sync::Arc<RawImage>>,
    vertices: [&Vertex; 3],
    address: TransportTextureAddress,
) -> Option<TransportAlphaSurface> {
    (alpha.mode != AlphaMode::Opaque).then(|| TransportAlphaSurface {
        alpha,
        image,
        uv: vertices.map(|vertex| vertex.uv),
        vertex_alpha: vertices.map(|vertex| vertex.color[3]),
        address,
    })
}

/// True when a triangle is one water volume's drawn surface: all three corners
/// lie on one resolved footprint at its `surface_y` (so the triangle is
/// horizontal) and the range resolves to that volume's material id.
///
/// The surface is drawn only for [`WaterVolumes`] entries, so matching the
/// authored geometry back to the same resolution is what keeps the transmitted
/// surface and the attenuating body one thing.
fn is_water_surface(
    corners: [[f32; 3]; 3],
    material_id: Option<&str>,
    water: &WaterVolumes,
) -> bool {
    let Some(surface_material) = material_id else {
        return false;
    };
    water.volumes().iter().any(|volume| {
        volume.material_id() == surface_material
            && corners.iter().all(|corner| {
                (corner[1] - volume.surface_y).abs() <= WATER_SURFACE_EPS_M
                    && corner[0] >= volume.x0 - WATER_SURFACE_EPS_M
                    && corner[0] <= volume.x1 + WATER_SURFACE_EPS_M
                    && corner[2] >= volume.z0 - WATER_SURFACE_EPS_M
                    && corner[2] <= volume.z1 + WATER_SURFACE_EPS_M
            })
    })
}

/// The light indices of every switchable ceiling fixture, in fixture order.
///
/// The compiled lighting record already knows the fixture-to-light mapping, so
/// this is the same classification the runtime switch uses: a fixture whose
/// `switchable` flag is set owns one light, and that light's contribution must
/// be prepared as its own selectable layer.
#[must_use]
pub fn switchable_lights(level: &crate::level::LevelDef, lighting: &LevelLighting) -> Vec<usize> {
    let mut out = Vec::new();
    for (fixture_index, fixture) in level.ceiling_lights.iter().enumerate() {
        if !fixture.switchable {
            continue;
        }
        if let Some(light) = lighting.fixture_light_index(fixture_index)
            && !out.contains(&light)
        {
            out.push(light);
        }
    }
    out
}

/// The albedo of one triangle: the material tint (or the prop vertex's own
/// base colour) multiplied by the submesh texture's colour at the triangle
/// centre.
fn triangle_albedo(
    material_albedo: [f32; 3],
    a: &Vertex,
    b: &Vertex,
    c: &Vertex,
    texture: Option<&RawImage>,
) -> [f32; 3] {
    let mut base = [
        material_albedo[0] * ((a.color[0] + b.color[0] + c.color[0]) / 3.0),
        material_albedo[1] * ((a.color[1] + b.color[1] + c.color[1]) / 3.0),
        material_albedo[2] * ((a.color[2] + b.color[2] + c.color[2]) / 3.0),
    ];
    if let Some(image) = texture {
        let centroid = [
            (a.uv[0] + b.uv[0] + c.uv[0]) / 3.0,
            (a.uv[1] + b.uv[1] + c.uv[1]) / 3.0,
        ];
        let sample = sample_texture(image, centroid);
        base = [
            base[0] * sample[0],
            base[1] * sample[1],
            base[2] * sample[2],
        ];
    }
    [
        base[0].clamp(0.0, 1.0),
        base[1].clamp(0.0, 1.0),
        base[2].clamp(0.0, 1.0),
    ]
}

/// The averaged albedo of one level material, or a neutral white for a slot
/// with no level material.
fn material_albedo(materials: &MaterialTable, material: u32) -> [f32; 3] {
    if material == MATERIAL_NONE {
        return [0.8; 3];
    }
    let Some(entry) = materials.entry(material) else {
        return [0.8; 3];
    };
    let mut tint = entry.tint;
    if let Some(image) = entry.image.as_deref() {
        let mean = mean_texture_color(image);
        tint = [tint[0] * mean[0], tint[1] * mean[1], tint[2] * mean[2]];
    }
    [
        tint[0].clamp(0.0, 1.0),
        tint[1].clamp(0.0, 1.0),
        tint[2].clamp(0.0, 1.0),
    ]
}

/// Mean colour of one texture, sampled on a fixed stride so a large image is
/// bounded work.
#[must_use]
pub fn mean_texture_color(image: &RawImage) -> [f32; 3] {
    if image.width == 0 || image.height == 0 {
        return [1.0; 3];
    }
    let texels = (usize::try_from(image.width).unwrap_or(0))
        .saturating_mul(usize::try_from(image.height).unwrap_or(0));
    if texels == 0 {
        return [1.0; 3];
    }
    let stride = (texels / 1024).max(1);
    let mut sum = [0.0_f32; 3];
    let mut count = 0_usize;
    let mut index = 0usize;
    while index < texels {
        let offset = index.saturating_mul(4);
        if let Some([r, g, b, a]) = image
            .rgba
            .get(offset..offset.saturating_add(4))
            .and_then(|slice| <[u8; 4]>::try_from(slice).ok())
            && a > 0
        {
            sum[0] += crate::materials::color::decode_byte(r);
            sum[1] += crate::materials::color::decode_byte(g);
            sum[2] += crate::materials::color::decode_byte(b);
            count = count.saturating_add(1);
        }
        index = index.saturating_add(stride);
    }
    if count == 0 {
        return [1.0; 3];
    }
    // The stride caps the sample count near 1024, so the divisor is bounded.
    let divisor = f32::from(u16::try_from(count).unwrap_or(u16::MAX)).max(1.0);
    [sum[0] / divisor, sum[1] / divisor, sum[2] / divisor]
}

/// Nearest-texel sample of a GLB sheet, clamped like the runtime sampler.
#[must_use]
pub fn sample_texture(image: &RawImage, uv: [f32; 2]) -> [f32; 3] {
    if image.width == 0 || image.height == 0 {
        return [1.0; 3];
    }
    let width = usize::try_from(image.width).unwrap_or(1).max(1);
    let height = usize::try_from(image.height).unwrap_or(1).max(1);
    let u_coordinate = if uv[0].is_finite() {
        uv[0].clamp(0.0, 1.0)
    } else {
        0.0
    };
    let v_coordinate = if uv[1].is_finite() {
        uv[1].clamp(0.0, 1.0)
    } else {
        0.0
    };
    let width_f =
        f32::from(u16::try_from(width.min(usize::from(u16::MAX))).unwrap_or(u16::MAX)).max(1.0);
    let height_f =
        f32::from(u16::try_from(height.min(usize::from(u16::MAX))).unwrap_or(u16::MAX)).max(1.0);
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Finite wrapped UVs become floored texel indices bounded by u16 texture dimensions and clamped to the last texel."
    )]
    // Both coordinates are finite and in `0..width` after the modulo and the
    // multiply, so the truncating cast is exact for the range it receives.
    let x_texel = ((u_coordinate * width_f).floor() as u32)
        .min(u32::try_from(width.saturating_sub(1)).unwrap_or(0));
    #[expect(
        clippy::as_conversions,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "Finite wrapped UVs become floored texel indices bounded by u16 texture dimensions and clamped to the last texel."
    )]
    let y_texel = ((v_coordinate * height_f).floor() as u32)
        .min(u32::try_from(height.saturating_sub(1)).unwrap_or(0));
    let offset = usize::try_from(y_texel)
        .unwrap_or(0)
        .saturating_mul(width)
        .saturating_add(usize::try_from(x_texel).unwrap_or(0))
        .saturating_mul(4);
    match image
        .rgba
        .get(offset..offset.saturating_add(4))
        .and_then(|slice| <[u8; 4]>::try_from(slice).ok())
    {
        Some([r, g, b, _]) => [
            crate::materials::color::decode_byte(r),
            crate::materials::color::decode_byte(g),
            crate::materials::color::decode_byte(b),
        ],
        _ => [1.0; 3],
    }
}

#[cfg(test)]
mod tests {
    // Test code: unwrap/expect and permissive float comparison are idiomatic
    // here; the production lints stay enforced above.
    #![allow(
        clippy::expect_used,
        clippy::float_cmp,
        reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
    )]

    use super::*;

    #[test]
    fn alpha_prop_cards_preserve_covered_pixels_and_opaque_fallbacks() {
        for mode in [AlphaMode::Opaque, AlphaMode::Cutout, AlphaMode::Blend] {
            let batch = PropMeshBatch {
                model: "alpha-contract".to_owned(),
                textures: Vec::new(),
                submeshes: vec![crate::render::PropSubmeshBatch {
                    response: crate::materials::MaterialResponse::NONE,
                    texture: None,
                    emission: crate::materials::MaterialEmission::NONE,
                    alpha: crate::materials::MaterialAlpha {
                        mode,
                        ..crate::materials::MaterialAlpha::OPAQUE
                    },
                    first_index: 0,
                    index_count: 6,
                }],
                vertices: vec![
                    Vertex::new([-1.0, 0.0, 0.0], [1.0; 4], [0.0, 0.0]),
                    Vertex::new([1.0, 0.0, 0.0], [1.0; 4], [1.0, 0.0]),
                    Vertex::new([1.0, 2.0, 0.0], [1.0; 4], [1.0, 1.0]),
                    Vertex::new([-1.0, 2.0, 0.0], [1.0; 4], [0.0, 1.0]),
                ],
                indices: vec![0, 1, 2, 0, 2, 3],
                bounds: crate::spatial::Aabb::EMPTY,
            };
            let mut triangles = Vec::new();
            let mut alpha = Vec::new();
            append_prop_triangles(
                &[batch],
                &mut triangles,
                &mut alpha,
                &mut TransportSceneStats::default(),
                &mut Vec::new(),
            );
            assert_eq!(triangles.len(), 2);
            let scene = TransportScene::new(triangles, Vec::new())
                .expect("scene")
                .with_surface_alpha(alpha)
                .expect("aligned alpha");
            assert_eq!(
                scene.occluded([0.0, 1.0, -1.0], [0.0, 1.0, 1.0]),
                mode != AlphaMode::Blend
            );
            assert_eq!(
                scene.probe_is_clear([0.0, 1.0, 0.0]),
                mode == AlphaMode::Blend
            );
        }
    }

    #[test]
    fn architecture_bounce_albedo_excludes_vertex_tint_and_lighting() {
        let level = pane_level("core:wallpaper_yellow_01");
        let materials = crate::render::logical_materials(&level);
        let material = materials
            .index_of("core:wallpaper_yellow_01")
            .expect("material");
        let mut mesh = pane_mesh(material);
        let expected = material_albedo(&materials, material);
        for vertex in &mut mesh.ranges.first_mut().expect("one pane").vertices {
            vertex.color = [0.01, 0.3, 4.0, 1.0];
        }
        let mut triangles = Vec::new();
        append_architecture_triangles(
            &mesh,
            &materials,
            &WaterVolumes::new(),
            &mut triangles,
            &mut Vec::new(),
            &mut TransportSceneStats::default(),
            &mut Vec::new(),
        );
        assert_eq!(triangles.len(), 2);
        assert!(triangles.iter().all(|triangle| triangle.albedo == expected));
    }

    #[test]
    fn a_solid_texture_reads_its_colour() {
        let image = RawImage::new(2, 2, [0, 128, 255, 255].repeat(4));
        let mean = mean_texture_color(&image);
        assert!((mean[0] - 0.0).abs() < 1e-6);
        assert!((mean[1] - crate::materials::color::decode_byte(128)).abs() < 1e-6);
        assert!((mean[2] - 1.0).abs() < 1e-6);
        let sample = sample_texture(&image, [0.75, 0.25]);
        for (actual, expected) in sample.iter().zip(mean.iter()) {
            assert!((actual - expected).abs() < 1.0e-6, "{sample:?} vs {mean:?}");
        }
    }

    #[test]
    fn a_prop_sample_clamps_to_the_runtime_sheet_edges() {
        let image = RawImage::new(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        );
        let sample = sample_texture(&image, [1.25, -0.25]);
        assert_eq!(sample, [0.0, 1.0, 0.0]);
    }

    use crate::level::LevelDef;
    use crate::render::{LevelMeshBatches, LevelMeshRange, SurfaceKey};
    use crate::spatial::Aabb;

    /// A one-room level whose floor references `material`, so the logical
    /// material table resolves it.
    fn pane_level(material: &str) -> LevelDef {
        LevelDef::from_json(&format!(
            r#"{{
                "format_version": 3,
                "id": "transport_pane",
                "name": "Transport Pane",
                "spawn": {{ "x": 0.0, "z": 0.0 }},
                "rooms": [{{ "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0,
                            "material": "{material}" }}]
            }}"#
        ))
        .expect("pane level parses")
    }

    /// A hand-built vertical wall pane at `z = 0`, one quad, one material.
    fn pane_mesh(material: u32) -> LevelMesh {
        let vertices = vec![
            Vertex::new([-1.0, 0.0, 0.0], [1.0; 4], [0.0, 0.0]),
            Vertex::new([1.0, 0.0, 0.0], [1.0; 4], [1.0, 0.0]),
            Vertex::new([1.0, 2.0, 0.0], [1.0; 4], [1.0, 1.0]),
            Vertex::new([-1.0, 2.0, 0.0], [1.0; 4], [0.0, 1.0]),
        ];
        let indices = vec![0, 1, 2, 0, 2, 3];
        LevelMesh {
            ranges: vec![LevelMeshRange {
                key: SurfaceKey::new(SurfaceKind::Wall, material),
                vertices,
                indices,
                bounds: Aabb::EMPTY,
            }],
            batches: LevelMeshBatches::default(),
            vertex_count: 4,
            index_count: 6,
        }
    }

    /// The grille's authored central slat blocks, while clear BLEND glazing
    /// retains a straight transmitting path and an opaque wall blocks.
    #[test]
    fn architecture_panes_keep_glass_transmission_and_grille_slats() {
        let from = [0.0, 1.0, -1.0];
        let to = [0.0, 1.0, 1.0];
        for (material, transmits) in [
            ("core:glass_window_clear_01", true),
            ("core:grille_vent_01", false),
            ("core:painting_dull_01", false),
        ] {
            let level = pane_level(material);
            let materials = crate::render::logical_materials(&level);
            let material_index = materials.index_of(material).expect("material resolves");
            let mesh = pane_mesh(material_index);
            let lighting = LevelLighting::bake(&level);
            let (scene, _) = build_transport_scene(&level, &mesh, &[], &materials, &lighting, &[])
                .expect("scene builds");
            assert_eq!(
                scene.occluded(from, to),
                !transmits,
                "{material}: the pane must {} the ray",
                if transmits { "transmit" } else { "block" }
            );
        }
    }

    /// A void wall's drawn faces are real transport geometry: the prepared
    /// solve blocks on them exactly as it blocks on a solid prop's drawn
    /// triangles, independent of the fast-path `occludes` flag. Transparency
    /// is the only transmission, exactly like every other drawn surface.
    #[test]
    fn void_wall_faces_block_the_prepared_transport_solve() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 3,
                "id": "transport_void",
                "name": "Transport Void",
                "spawn": { "x": 0.0, "z": 0.0 },
                "rooms": [ { "x": -4.0, "z": -4.0, "width": 8.0, "depth": 8.0, "height": 3.0 } ],
                "void_walls": [
                    { "id": "slab", "min": [-0.15, 0.0, -2.0], "max": [0.15, 3.0, 2.0],
                      "material": "core:wallpaper_yellow_01", "occludes": false }
                ]
            }"#,
        )
        .expect("transport void level parses");
        let materials = crate::render::logical_materials(&level);
        let mesh = crate::render::build_level_geometry(&level);
        let lighting = LevelLighting::bake(&level);
        let (scene, _) = build_transport_scene(&level, &mesh, &[], &materials, &lighting, &[])
            .expect("scene builds");
        assert!(
            scene.occluded([-2.0, 1.0, 0.0], [2.0, 1.0, 0.0]),
            "the drawn faces block the solve even with occludes: false"
        );
        assert!(
            !scene.occluded([-2.0, 4.0, 0.0], [2.0, 4.0, 0.0]),
            "above the box the ray escapes"
        );
    }

    /// The authored water volume reaches the solver as a transmissive surface
    /// and an attenuating body: a ray crosses the surface, a point below it is
    /// attenuated by its submerged depth, and a point above is not.
    #[test]
    fn a_water_volume_transmits_and_attenuates_by_submerged_depth() {
        let level = LevelDef::from_json(
            r#"{
                "format_version": 3,
                "id": "transport_water",
                "name": "Transport Water",
                "spawn": { "x": 0.0, "z": 0.0 },
                "rooms": [ { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0, "height": 3.0 } ],
                "water": [
                    { "x": 0.0, "z": 0.0, "width": 4.0, "depth": 4.0,
                      "surface_y": 1.0, "bottom_y": -1.0 }
                ]
            }"#,
        )
        .expect("water level parses");
        let materials = crate::render::logical_materials(&level);
        let water_index = materials
            .index_of(crate::level::DEFAULT_WATER_MATERIAL)
            .expect("the water material resolves");
        // The same quad the water emitter draws: `p0, p1, p2` then `p0, p2, p3`.
        let vertices = vec![
            Vertex::new([0.0, 1.0, 4.0], [1.0; 4], [0.0, 0.0]),
            Vertex::new([4.0, 1.0, 4.0], [1.0; 4], [1.0, 0.0]),
            Vertex::new([4.0, 1.0, 0.0], [1.0; 4], [1.0, 1.0]),
            Vertex::new([0.0, 1.0, 0.0], [1.0; 4], [0.0, 1.0]),
        ];
        let mesh = LevelMesh {
            ranges: vec![LevelMeshRange {
                key: SurfaceKey::new(SurfaceKind::Floor, water_index),
                vertices,
                indices: vec![0, 1, 2, 0, 2, 3],
                bounds: Aabb::EMPTY,
            }],
            batches: LevelMeshBatches::default(),
            vertex_count: 4,
            index_count: 6,
        };
        let lighting = LevelLighting::bake(&level);
        let (scene, _) = build_transport_scene(&level, &mesh, &[], &materials, &lighting, &[])
            .expect("scene builds");
        assert!(
            !scene.occluded([2.0, 0.0, 2.0], [2.0, 2.0, 2.0]),
            "the water surface must transmit"
        );
        let below = scene.attenuation_at([2.0, 0.5, 2.0]);
        assert!(
            below.iter().all(|value| *value < 1.0),
            "a submerged point must be attenuated: {below:?}"
        );
        assert!(
            below[0] < below[1] && below[1] < below[2],
            "red is absorbed most: {below:?}"
        );
        assert_eq!(
            scene.attenuation_at([2.0, 1.5, 2.0]),
            [1.0; 3],
            "above the surface there is no attenuation"
        );
    }
}
