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

use std::collections::BTreeMap;

use crate::engine::level::{LevelDef, WaterVolumes};
use crate::engine::lighting::LevelLighting;
use crate::engine::lighting::lightmap::{Chart, LightmapPatch};
use crate::engine::lighting::switchable::{Emitters, prepare_emitters};
use crate::engine::lighting::transport::{
    TransportAlphaSurface, TransportScene, TransportTextureAddress, TransportTriangle,
    TransportWaterBody, receiver_targets,
};
use crate::engine::lighting::transport_surfaces::{
    alpha_surface, is_water_surface, material_albedo, triangle_albedo,
};
use crate::engine::materials::{MaterialTable, RawImage};
use crate::engine::render::{LevelMesh, MATERIAL_NONE, PropMeshBatch, SurfaceKey, SurfaceKind};

// Keep the existing renderer-facing receiver vocabulary for geometry and pose callers.
pub use crate::engine::lighting::entity_receivers::{
    EntityLighting, EntitySpatialKey, EntitySpatialLighting, EntityStaticRayCache, entity_lighting,
};
pub(super) use crate::engine::lighting::entity_receivers::{
    EntitySamplingOptions, EntitySpatialLightingContext, receiver_surface_supports,
};
#[cfg(any(test, feature = "visual-diagnostics"))]
pub use crate::engine::lighting::entity_receivers::{
    EntityVisibility, entity_spatial_lighting_cached,
};

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

/// Identity follows the source material/primitive, rather than the texture
/// colour sampled at a triangle centroid. IDs are local to one prepared scene.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum SurfaceMaterial {
    Architecture(SurfaceKey),
    Prop { model: String, primitive: usize },
    UnknownProp { batch: usize, primitive: usize },
}

#[derive(Default)]
struct SurfaceAttributes {
    alpha: Vec<Option<TransportAlphaSurface>>,
    materials: Vec<u32>,
    identities: BTreeMap<SurfaceMaterial, u32>,
    unknown_batches: usize,
}

impl SurfaceAttributes {
    fn material(&mut self, source: SurfaceMaterial) -> u32 {
        let next = u32::try_from(self.identities.len())
            .unwrap_or(u32::MAX)
            .saturating_add(1);
        *self.identities.entry(source).or_insert(next)
    }

    fn push(&mut self, material: u32, alpha: Option<TransportAlphaSurface>) {
        self.materials.push(material);
        self.alpha.push(alpha);
    }

    const fn unknown_batch(&mut self) -> usize {
        let identity = self.unknown_batches;
        self.unknown_batches = self.unknown_batches.saturating_add(1);
        identity
    }
}

/// Immutable receiver visibility used by dynamic lighting after a prepared install.
/// This does not solve lighting or change the compiler's prepared values.
pub fn prepare_runtime_transport(
    loaded: &crate::engine::loader::LoadedLevel,
    build: &crate::engine::render::LevelBuild,
) -> Option<std::sync::Arc<TransportScene>> {
    let blacklights = crate::engine::lighting::ultraviolet::BlacklightSet::from_level(
        &loaded.level,
        &build.lighting,
    );
    if blacklights.is_empty()
        && build
            .probes
            .as_ref()
            .and_then(|field| field.local_direct.as_ref())
            .is_none()
    {
        return None;
    }
    build_transport_scene(
        &loaded.level,
        &build.mesh,
        &build.batches,
        &loaded.materials,
        &build.lighting,
        &[],
    )
    .map(|(scene, _)| std::sync::Arc::new(scene))
}

/// Immutable presentation collision only: no lights, probes, targets or solve.
/// Built lazily for blood when a Static package has no lighting transport index.
/// Claimed character bind poses remain excluded; their live skin owns collision.
#[must_use]
pub fn build_surface_scene(
    level: &LevelDef,
    mesh: &LevelMesh,
    batches: &[PropMeshBatch],
    materials: &MaterialTable,
) -> Option<std::sync::Arc<TransportScene>> {
    let mut triangles = Vec::new();
    let mut surfaces = SurfaceAttributes::default();
    let mut stats = TransportSceneStats::default();
    let mut owners = Vec::new();
    let water = WaterVolumes::from_level(level);
    append_architecture_triangles(
        mesh,
        materials,
        &water,
        &mut triangles,
        &mut surfaces,
        &mut stats,
        &mut owners,
    );
    append_prop_triangles(
        batches,
        &mut triangles,
        &mut surfaces,
        &mut stats,
        &mut owners,
    );
    TransportScene::new(triangles, Vec::new())?
        .with_surface_alpha(surfaces.alpha)
        .map(std::sync::Arc::new)
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
    let mut surfaces = SurfaceAttributes::default();
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
        &mut surfaces,
        &mut stats,
        &mut owners,
    );
    append_prop_triangles(
        batches,
        &mut triangles,
        &mut surfaces,
        &mut stats,
        &mut owners,
    );

    let Emitters {
        sources: emitters,
        runtime_direct,
        groups,
        switchable_count,
    } = prepare_emitters(level, lighting);
    stats.switchable_emitters = switchable_count;
    stats.triangles = triangles.len();
    stats.emitters = emitters.len();
    let scene = TransportScene::new(triangles, emitters)?
        .with_switchable_groups(groups)
        .with_runtime_direct_lights(runtime_direct)
        .with_surface_alpha(surfaces.alpha)?
        .with_surface_materials(surfaces.materials)?
        .with_water(
            water
                .volumes()
                .iter()
                .map(TransportWaterBody::from_volume)
                .collect(),
        )
        .with_receiver_target(receiver_targets(lighting, charts))
        .with_validated_probe_targets(level, lighting, charts)
        .with_sky(crate::engine::lighting::sky_radiance(level));
    let lit_scene = scene.with_global_lights(
        level
            .global_illuminators
            .iter()
            .filter_map(crate::engine::lighting::directional::DirectionalLight::from_definition)
            .collect(),
    );
    if let Err(error) =
        crate::engine::lighting::transport::diagnostics::dump_scene(&lit_scene, &owners)
    {
        crate::engine::logging::warn(format_args!("[lighting-diagnostics] {error}"));
    }
    Some((lit_scene, stats))
}

/// Appends every architecture range's triangles to the transport scene.
///
/// Decals and luminous fixture faces are skipped; opaque fixture housings,
/// walls, floors, ceilings, architecture and prop fallbacks occlude. Water transmits into its attenuating
/// body; other surfaces retain the renderer's alpha coverage at each ray hit.
fn append_architecture_triangles(
    mesh: &LevelMesh,
    materials: &MaterialTable,
    water: &WaterVolumes,
    triangles: &mut Vec<TransportTriangle>,
    surfaces: &mut SurfaceAttributes,
    stats: &mut TransportSceneStats,
    owners: &mut Vec<crate::engine::lighting::transport::diagnostics::CasterRange>,
) {
    for range in &mesh.ranges {
        // Fitted luminous faces are endpoint surfaces of analytic sources.
        // Their ordinary bare housings must still block light: excluding the
        // whole Light family made even a closed blacklight body transparent.
        let fixture_housing = range.key.kind == SurfaceKind::Light && !range.key.has_material();
        if range.key.kind == SurfaceKind::Decal
            || (range.key.kind == SurfaceKind::Light && !fixture_housing)
        {
            continue;
        }
        let first = triangles.len();
        let material = if range.key.material == MATERIAL_NONE {
            [0.8; 3]
        } else {
            materials
                .entry(range.key.material)
                .map_or([0.8; 3], |entry| {
                    material_albedo(entry.tint, entry.image.as_deref())
                })
        };
        let material_id = material_id(materials, range.key.material);
        let identity = surfaces.material(SurfaceMaterial::Architecture(range.key));
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
            // Bare fixture vertices carry their authored body albedo in this
            // deferred-lighting mesh, including the blacklight's dark paint.
            let albedo = if fixture_housing {
                let [red, green, blue, _] = va.color;
                [red, green, blue]
            } else {
                material
            };
            match TransportTriangle::new(va.pos, vb.pos, vc.pos, albedo) {
                Some(triangle) => {
                    let corners = [va.pos, vb.pos, vc.pos];
                    let water_surface = is_water_surface(corners, material_id, water);
                    triangles.push(triangle.with_transmissive(water_surface));
                    surfaces.push(
                        identity,
                        if water_surface {
                            None
                        } else {
                            materials.entry(range.key.material).and_then(|entry| {
                                alpha_surface(
                                    entry.alpha,
                                    entry.image.clone(),
                                    [va.uv, vb.uv, vc.uv],
                                    [va.color[3], vb.color[3], vc.color[3]],
                                    TransportTextureAddress::Repeat,
                                )
                            })
                        },
                    );
                }
                None => stats.skipped_triangles = stats.skipped_triangles.saturating_add(1),
            }
        }
        if crate::engine::lighting::transport::diagnostics::enabled() {
            owners.push(
                crate::engine::lighting::transport::diagnostics::CasterRange {
                    first,
                    end: triangles.len(),
                    owner: format!(
                        "architecture:{:?}:{}",
                        range.key.kind,
                        material_id.unwrap_or("untextured")
                    ),
                },
            );
        }
    }
}

/// Prop sheets clamp their UVs; alpha coverage and material factors match the
/// draw path without turning the whole card into either a blocker or a hole.
fn append_prop_triangles(
    batches: &[PropMeshBatch],
    triangles: &mut Vec<TransportTriangle>,
    surfaces: &mut SurfaceAttributes,
    stats: &mut TransportSceneStats,
    owners: &mut Vec<crate::engine::lighting::transport::diagnostics::CasterRange>,
) {
    for batch in batches {
        if !batch.casts_static_lighting {
            continue;
        }
        let first = triangles.len();
        let unknown_batch = surfaces.unknown_batch();
        for (primitive, submesh) in batch.submeshes.iter().enumerate() {
            // Finished batches omit empty primitives. Only the retained
            // original source slot can be shared safely across those batches.
            // Decoded packages have no offline sidecar: keep their materials
            // scoped to this batch rather than guessing from draw ordinal.
            let source_primitive = if batch.source_primitives.len() == batch.submeshes.len() {
                batch.source_primitives.get(primitive).copied()
            } else {
                None
            };
            let material = source_primitive.map_or(
                SurfaceMaterial::UnknownProp {
                    batch: unknown_batch,
                    primitive,
                },
                |source_slot| SurfaceMaterial::Prop {
                    model: batch.model.clone(),
                    primitive: source_slot,
                },
            );
            let identity = surfaces.material(material);
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
                let albedo = triangle_albedo(
                    [1.0; 3],
                    [va.color, vb.color, vc.color],
                    [va.uv, vb.uv, vc.uv],
                    image,
                );
                match TransportTriangle::new(va.pos, vb.pos, vc.pos, albedo) {
                    Some(triangle) => {
                        triangles
                            .push(triangle.with_shading_normals([va.normal, vb.normal, vc.normal]));
                        surfaces.push(
                            identity,
                            alpha_surface(
                                submesh.alpha,
                                submesh
                                    .texture
                                    .and_then(|texture| batch.textures.get(usize::from(texture)))
                                    .cloned(),
                                [va.uv, vb.uv, vc.uv],
                                [va.color[3], vb.color[3], vc.color[3]],
                                TransportTextureAddress::Clamp,
                            ),
                        );
                    }
                    None => stats.skipped_triangles = stats.skipped_triangles.saturating_add(1),
                }
            }
        }
        if crate::engine::lighting::transport::diagnostics::enabled() {
            owners.push(
                crate::engine::lighting::transport::diagnostics::CasterRange {
                    first,
                    end: triangles.len(),
                    owner: format!("model:{}", batch.model),
                },
            );
        }
    }
}

/// The same triangle/material/coverage preparation, in a movable mesh's local
/// space. The resulting ray resource has no light sources or bake work.
#[must_use]
pub(super) fn dynamic_mesh_visibility_scene(
    mesh: &super::dynamic::DynamicMesh,
) -> Option<TransportScene> {
    let batch = PropMeshBatch {
        model: mesh.model_path.clone(),
        light_control: None,
        casts_static_lighting: true,
        pending_probe_lighting: false,
        // DynamicMesh filters empty source primitives too. Its isolated ray
        // scene can preserve material continuity within each draw submesh
        // without pretending the compressed ordinal is an original source ID.
        source_primitives: Vec::new(),
        textures: mesh.textures.clone(),
        submeshes: mesh
            .submeshes
            .iter()
            .map(|submesh| crate::engine::render::PropSubmeshBatch {
                response: submesh.response,
                texture: submesh.texture,
                emission: submesh.emission,
                alpha: submesh.alpha,
                first_index: submesh.first_index,
                index_count: submesh.index_count,
            })
            .collect(),
        vertices: (mesh.vertices.clone()).into(),
        indices: (mesh.indices.clone()).into(),
        bounds: mesh.bounds,
    };
    let mut triangles = Vec::new();
    let mut surfaces = SurfaceAttributes::default();
    append_prop_triangles(
        &[batch],
        &mut triangles,
        &mut surfaces,
        &mut TransportSceneStats::default(),
        &mut Vec::new(),
    );
    TransportScene::new(triangles, Vec::new())?
        .with_surface_alpha(surfaces.alpha)?
        .with_surface_materials(surfaces.materials)
}

/// Current posed triangles retain the model's shared material/PNG alpha
/// contract and the same per-instance coverage multiplier as its draw.
pub(in crate::engine::render) fn character_visibility_scene(
    character: &super::character::Character,
    textures: &[std::sync::Arc<RawImage>],
) -> Option<TransportScene> {
    let model = &character.asset().model;
    let vertices: Vec<_> = model
        .vertices
        .iter()
        .enumerate()
        .map(|(index, vertex)| {
            let joints = model.joints.get(index).copied().unwrap_or([0; 4]);
            let weights = model.weights.get(index).copied().unwrap_or([0.0; 4]);
            super::character::character_vertex(
                character.albedo().get(index).copied().unwrap_or([1.0; 4]),
                vertex.uv,
                character
                    .animator()
                    .skin_position(joints, weights, vertex.pos),
            )
        })
        .collect();
    let batch = PropMeshBatch {
        model: character.asset().model_path.clone(),
        light_control: None,
        casts_static_lighting: true,
        pending_probe_lighting: false,
        source_primitives: Vec::new(),
        textures: textures.to_vec(),
        submeshes: model
            .submeshes
            .iter()
            .map(|submesh| {
                let mut alpha = submesh.alpha;
                // Coverage is scaled at each actual intersection, preserving
                // MASK thresholds and layered BLEND transmission. OPAQUE
                // primitives retain their existing solid draw contract.
                alpha.opacity *= character.opacity();
                super::props::PropSubmeshBatch {
                    response: submesh.response,
                    texture: submesh.texture,
                    emission: submesh.emission,
                    alpha,
                    first_index: submesh.first_index,
                    index_count: submesh.index_count,
                }
            })
            .collect(),
        vertices: vertices.into(),
        indices: (model.indices.clone()).into(),
        bounds: character.world_bounds(),
    };
    let mut triangles = Vec::new();
    let mut surfaces = SurfaceAttributes::default();
    append_prop_triangles(
        &[batch],
        &mut triangles,
        &mut surfaces,
        &mut TransportSceneStats::default(),
        &mut Vec::new(),
    );
    TransportScene::new(triangles, Vec::new())?
        .with_surface_alpha(surfaces.alpha)?
        .with_surface_materials(surfaces.materials)
}

fn material_id(materials: &MaterialTable, material: u32) -> Option<&str> {
    if material == MATERIAL_NONE {
        return None;
    }
    materials.entry(material).map(|entry| entry.id.as_str())
}
