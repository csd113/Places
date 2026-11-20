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
use crate::engine::lighting::entity_receivers::select_baked_direct_lights;
use crate::engine::lighting::lightmap::{Chart, LightmapPatch};
use crate::engine::lighting::transport::{
    TransportAlphaSurface, TransportEmitter, TransportScene, TransportTextureAddress,
    TransportTriangle, TransportWaterBody, receiver_targets,
};
use crate::engine::materials::{AlphaMode, MaterialTable, RawImage};
use crate::engine::render::{
    LevelMesh, MATERIAL_NONE, PropMeshBatch, SurfaceKey, SurfaceKind, Vertex,
};

// Keep the existing renderer-facing receiver vocabulary for geometry and pose callers.
pub use crate::engine::lighting::entity_receivers::{
    EntityDirectLight, EntityLighting, EntitySpatialKey, EntitySpatialLighting,
    EntityStaticRayCache, entity_lighting,
};
pub(super) use crate::engine::lighting::entity_receivers::{
    EntitySamplingOptions, EntitySpatialLightingContext, receiver_surface_supports,
};
#[cfg(any(test, feature = "visual-diagnostics"))]
pub use crate::engine::lighting::entity_receivers::{
    EntityVisibility, entity_spatial_lighting_cached,
};

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
    let blacklights = super::blacklight::BlacklightSet::from_level(&loaded.level, &build.lighting);
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

    let mut emitters: Vec<TransportEmitter> = Vec::new();
    let switchable = switchable_lights(level, lighting);
    let selected = select_baked_direct_lights(lighting, &switchable);
    let mut runtime_direct = Vec::new();
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
        if let Ok(source) = u32::try_from(index)
            && selected.contains(&source)
        {
            runtime_direct.push((emitters.len(), source));
        }
        emitters.push(TransportEmitter::from_baked(light, tag));
        if tag.is_some() {
            stats.switchable_emitters = stats.switchable_emitters.saturating_add(1);
        }
    }
    stats.triangles = triangles.len();
    stats.emitters = emitters.len();
    let scene = TransportScene::new(triangles, emitters)?
        .with_switchable_groups(switchable_groups(level, lighting))
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
        .with_sky(sky_radiance(level));
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

/// The radiance an escaping bounce ray sees, from the level's optional sky.
///
/// Zero without a sky (or with `ambient` 0.0), which preserves the historical
/// "an interior has no sky" solve exactly. The authored scalar is scaled by
/// [`crate::engine::lighting::SKY_AMBIENT_COLOR`].
fn sky_radiance(level: &LevelDef) -> [f32; 3] {
    crate::engine::lighting::sky_radiance(level)
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
        let material = material_albedo(materials, range.key.material);
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
                                    [va, vb, vc],
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
                let albedo = triangle_albedo([1.0; 3], &va, &vb, &vc, image);
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
                                [&va, &vb, &vc],
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

fn alpha_surface(
    alpha: crate::engine::materials::MaterialAlpha,
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
pub fn switchable_lights(
    level: &crate::engine::level::LevelDef,
    lighting: &LevelLighting,
) -> Vec<usize> {
    let mut out = Vec::new();
    for (fixture_index, fixture) in level.ceiling_lights.iter().enumerate() {
        if !fixture.switchable {
            continue;
        }
        for light in lighting.fixture_light_indices(fixture_index) {
            if !out.contains(&light) {
                out.push(light);
            }
        }
    }
    for (index, prop) in level.props.iter().enumerate() {
        if prop
            .light_control()
            .is_some_and(|control| control.switchable)
        {
            out.extend(
                lighting
                    .prop_light_indices(index)
                    .zip(&prop.lights)
                    .filter_map(|(source, light)| light.enabled.then_some(source)),
            );
        }
    }
    out
}

/// Contribution groups in fixture order, retaining each source's live identity.
pub fn switchable_groups(level: &LevelDef, lighting: &LevelLighting) -> Vec<(usize, Vec<usize>)> {
    let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut named = BTreeMap::<&str, usize>::new();
    for (fixture_index, fixture) in level.ceiling_lights.iter().enumerate() {
        if !fixture.switchable {
            continue;
        }
        let sources = lighting.fixture_light_indices(fixture_index);
        let Some(light) = sources.first().copied() else {
            continue;
        };
        if let Some(name) = fixture.switch_group.as_deref()
            && let Some(group) = named.get(name).and_then(|index| groups.get_mut(*index))
        {
            group.1.extend(sources);
        } else {
            if let Some(name) = fixture.switch_group.as_deref() {
                let _previous = named.insert(name, groups.len());
            }
            groups.push((light, sources));
        }
    }
    for (index, prop) in level.props.iter().enumerate() {
        if !prop
            .light_control()
            .is_some_and(|control| control.switchable)
        {
            continue;
        }
        let sources: Vec<usize> = lighting
            .prop_light_indices(index)
            .zip(&prop.lights)
            .filter_map(|(source, light)| light.enabled.then_some(source))
            .collect();
        if let Some(first) = sources.first().copied() {
            if let Some(name) = prop.switch_group.as_deref()
                && let Some(group) = named.get(name).and_then(|ordinal| groups.get_mut(*ordinal))
            {
                group.1.extend(sources);
            } else {
                if let Some(name) = prop.switch_group.as_deref() {
                    let _previous = named.insert(name, groups.len());
                }
                groups.push((first, sources));
            }
        }
    }
    groups
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
            sum[0] += crate::engine::materials::color::decode_byte(r);
            sum[1] += crate::engine::materials::color::decode_byte(g);
            sum[2] += crate::engine::materials::color::decode_byte(b);
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
            crate::engine::materials::color::decode_byte(r),
            crate::engine::materials::color::decode_byte(g),
            crate::engine::materials::color::decode_byte(b),
        ],
        _ => [1.0; 3],
    }
}
