//! Building the offline transport scene from a prepared level build.
//!
//! The transport solver needs plain world-space triangles with a diffuse
//! albedo, plus the resolved light sources. This module assembles them from the
//! same data the renderer draws: the prepared [`LevelMesh`] ranges (skipping the
//! decal pass, which is a visual overlay and not an occluder), the placed prop
//! batches, and the level's material table.
//!
//! Diffuse albedo per surface is the material's authored tint multiplied by the
//! texture's sampled colour at the triangle's centre, in the renderer's working
//! display space. A prop vertex already carries its material's `baseColorFactor`
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
    TransportEmitter, TransportScene, TransportTriangle, TransportWaterBody, probe_targets,
    receiver_targets,
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
        &mut stats,
        &mut owners,
    );
    append_prop_triangles(batches, &mut triangles, &mut stats, &mut owners);

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
    let scene = scene.with_global_lights(
        level
            .global_illuminators
            .iter()
            .filter_map(crate::lighting::directional::DirectionalLight::from_definition)
            .collect(),
    );
    if let Err(error) = crate::lighting::transport::diagnostics::dump_scene(&scene, &owners) {
        crate::logging::warn(format_args!("[lighting-diagnostics] {error}"));
    }
    Some((scene, stats))
}

/// The runtime environment sample. Prepared values stay linear HDR until
/// the material shader reconstructs diffuse lighting at its world normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EntityLighting {
    pub prepared: Option<crate::lighting::lightmap::LightmapTexel>,
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
/// A failed lookup uses the existing authored environment model, bounded to
/// its display range. No brightness floor is applied to valid baked energy.
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
                    v.clamp(0.0, 1.0)
                } else {
                    crate::lighting::AMBIENT_LEVEL
                }
            })
        },
        |t| crate::lighting::transport::soft_clip(t.irradiance),
    );
    EntityLighting {
        prepared,
        display,
        source,
    }
}

/// Isotropic display value used by compatibility tests.
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
/// architecture and prop fallbacks occlude. A water volume's drawn surface and
/// a blend/cutout architecture pane are marked transmissive; everything else,
/// including a solid glass material, keeps blocking.
fn append_architecture_triangles(
    mesh: &LevelMesh,
    materials: &MaterialTable,
    water: &WaterVolumes,
    triangles: &mut Vec<TransportTriangle>,
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
        // Architecture panes the renderer draws as blend/cutout (glass,
        // grilles) transmit like the shipped vertex-lit bake's openings; a
        // solid glass or any opaque architecture range keeps blocking.
        let transmits = matches!(
            range.key.kind,
            SurfaceKind::Floor | SurfaceKind::Ceiling | SurfaceKind::Wall
        ) && material_transmits(materials, range.key.material);
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
            let albedo = triangle_albedo(material, va, vb, vc, None);
            match TransportTriangle::new(va.pos, vb.pos, vc.pos, albedo) {
                Some(triangle) => {
                    let corners = [va.pos, vb.pos, vc.pos];
                    let transmissive = transmits || is_water_surface(corners, material_id, water);
                    triangles.push(triangle.with_transmissive(transmissive));
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

/// Appends placed prop geometry with the cutout transmission convention used
/// by architecture. Treating a cutout card as opaque blocks the entire card,
/// including its transparent pixels, and can black out the ground below grass.
fn append_prop_triangles(
    batches: &[PropMeshBatch],
    triangles: &mut Vec<TransportTriangle>,
    stats: &mut TransportSceneStats,
    owners: &mut Vec<crate::lighting::transport::diagnostics::CasterRange>,
) {
    for batch in batches {
        let first = triangles.len();
        for submesh in &batch.submeshes {
            // Static batches support MASK; BLEND is an opaque fallback on
            // this draw route, so it must retain its existing solid behavior.
            let transmissive = submesh.alpha.mode == AlphaMode::Cutout;
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
                    Some(triangle) => triangles.push(
                        triangle
                            .with_shading_normals([va.normal, vb.normal, vc.normal])
                            .with_transmissive(transmissive),
                    ),
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

/// True when a range's resolved material is drawn as a translucent or
/// alpha-tested opening rather than a solid occluder.
///
/// This is the same `alpha_mode: blend|cutout` classification the renderer
/// uses to route a surface into the translucent or alpha-tested pass; the
/// vertex-lit bake transmits light through both, so the transport solve must
/// too. A material that does not resolve stays solid.
fn material_transmits(materials: &MaterialTable, material: u32) -> bool {
    if material == MATERIAL_NONE {
        return false;
    }
    materials
        .entry(material)
        .is_some_and(|entry| matches!(entry.alpha.mode, AlphaMode::Cutout | AlphaMode::Blend))
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
    let Some(material_id) = material_id else {
        return false;
    };
    water.volumes().iter().any(|volume| {
        volume.material_id() == material_id
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
        material_albedo[0] * a.color[0],
        material_albedo[1] * a.color[1],
        material_albedo[2] * a.color[2],
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
            sum[0] += f32::from(r) / 255.0;
            sum[1] += f32::from(g) / 255.0;
            sum[2] += f32::from(b) / 255.0;
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

/// Nearest-texel sample of one texture, wrapping on both axes.
#[must_use]
pub fn sample_texture(image: &RawImage, uv: [f32; 2]) -> [f32; 3] {
    if image.width == 0 || image.height == 0 {
        return [1.0; 3];
    }
    let width = usize::try_from(image.width).unwrap_or(1).max(1);
    let height = usize::try_from(image.height).unwrap_or(1).max(1);
    let u_coordinate = if uv[0].is_finite() {
        uv[0].rem_euclid(1.0)
    } else {
        0.0
    };
    let v_coordinate = if uv[1].is_finite() {
        uv[1].rem_euclid(1.0)
    } else {
        0.0
    };
    let width_f =
        f32::from(u16::try_from(width.min(usize::from(u16::MAX))).unwrap_or(u16::MAX)).max(1.0);
    let height_f =
        f32::from(u16::try_from(height.min(usize::from(u16::MAX))).unwrap_or(u16::MAX)).max(1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // Both coordinates are finite and in `0..width` after the modulo and the
    // multiply, so the truncating cast is exact for the range it receives.
    let x_texel = ((u_coordinate * width_f).floor() as u32)
        .min(u32::try_from(width.saturating_sub(1)).unwrap_or(0));
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
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
            f32::from(r) / 255.0,
            f32::from(g) / 255.0,
            f32::from(b) / 255.0,
        ],
        _ => [1.0; 3],
    }
}

#[cfg(test)]
mod tests {
    // Test code: unwrap/expect and permissive float comparison are idiomatic
    // here; the production lints stay enforced above.
    #![allow(clippy::expect_used, clippy::float_cmp)]

    use super::*;

    #[test]
    fn cutout_prop_cards_transmit_but_opaque_batch_fallbacks_remain_solid() {
        for mode in [AlphaMode::Opaque, AlphaMode::Cutout, AlphaMode::Blend] {
            let batch = PropMeshBatch {
                model: "alpha-contract".to_owned(),
                textures: Vec::new(),
                submeshes: vec![crate::render::PropSubmeshBatch {
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
            append_prop_triangles(
                &[batch],
                &mut triangles,
                &mut TransportSceneStats::default(),
                &mut Vec::new(),
            );
            assert_eq!(triangles.len(), 2);
            let scene = TransportScene::new(triangles, Vec::new()).expect("scene");
            assert_eq!(
                scene.occluded([0.0, 1.0, -1.0], [0.0, 1.0, 1.0]),
                mode != AlphaMode::Cutout
            );
            assert_eq!(
                scene.probe_is_clear([0.0, 1.0, 0.0]),
                mode == AlphaMode::Cutout
            );
        }
    }

    #[test]
    fn a_solid_texture_reads_its_colour() {
        let image = RawImage::new(2, 2, [0, 128, 255, 255].repeat(4));
        let mean = mean_texture_color(&image);
        assert!((mean[0] - 0.0).abs() < 1e-6);
        assert!((mean[1] - (128.0 / 255.0)).abs() < 1e-6);
        assert!((mean[2] - 1.0).abs() < 1e-6);
        let sample = sample_texture(&image, [0.75, 0.25]);
        for (actual, expected) in sample.iter().zip(mean.iter()) {
            assert!((actual - expected).abs() < 1.0e-6, "{sample:?} vs {mean:?}");
        }
    }

    #[test]
    fn a_wrapping_sample_stays_inside_the_texture() {
        let image = RawImage::new(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
            ],
        );
        let sample = sample_texture(&image, [1.25, -0.25]);
        assert!(sample.iter().all(|value| (0.0..=1.0).contains(value)));
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

    /// The renderer's alpha contract reaches the transport solve: a blend or
    /// cutout architecture pane transmits a shadow ray, an opaque wall at the
    /// same place blocks it.
    #[test]
    fn blend_and_cutout_architecture_panes_transmit_while_an_opaque_wall_blocks() {
        let from = [0.0, 1.0, -1.0];
        let to = [0.0, 1.0, 1.0];
        for (material, transmits) in [
            ("core:glass_window_clear_01", true),
            ("core:grille_vent_01", true),
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
