//! Prop instancing and lighting.
//!
//! Every placed instance of one model is transformed and lit on the CPU at
//! level load time and appended to a per-model batch, so the renderer binds one
//! buffer per model and issues one draw call per *primitive* of all of its
//! instances. A prop with no usable asset falls back to a placeholder box.
//!
//! Materials
//! ---------
//! A production GLB may split one model into several primitives, each with its
//! own material, texture and emission. Instances are accumulated per
//! `(model, spatial cell)` and their index lists are grouped by primitive, so a
//! model with three primitives costs three draws per cell no matter how many
//! times it is placed — a field of vending machines still batches. A single
//! primitive model behaves exactly as it always has: one range, one draw.

use std::sync::Arc;

use super::{LevelDef, LevelLighting, LevelSurfaces, PropDef, Vertex, spatial_cell_grid};
use crate::materials::{MaterialAlpha, MaterialEmission};

/// One primitive's slice of a [`PropMeshBatch`]: its texture, its emission and
/// the range of the batch's index buffer it draws.
#[derive(Clone, Debug, PartialEq)]
pub struct PropSubmeshBatch {
    /// Index into [`PropMeshBatch::textures`], or `None` for an untextured
    /// material (the renderer draws it through the shared white sheet with the
    /// material's `baseColorFactor` already baked into the vertex colours).
    pub texture: Option<u16>,
    /// The material's visual emission. Never a light source: an emissive prop
    /// only glows, and any environmental illumination it contributes comes from
    /// the generic lights its level entry attaches to it.
    pub emission: MaterialEmission,
    /// The material's alpha contract. A glTF `MASK` primitive is
    /// [`crate::materials::AlphaMode::Cutout`] and draws through the cutout
    /// pass; a `BLEND` primitive is [`crate::materials::AlphaMode::Blend`] and
    /// is meaningful only on a route with a translucent pass (a character or a
    /// dynamic object); every other prop primitive is opaque.
    pub alpha: MaterialAlpha,
    /// First index into [`PropMeshBatch::indices`].
    pub first_index: u32,
    /// Number of indices in this submesh (a multiple of three).
    pub index_count: u32,
}

/// Instanced prop geometry for one distinct prop model inside one spatial cell.
///
/// Every placed instance of the same model is transformed on the CPU at level
/// load time and appended here, so the renderer binds one buffer per model and
/// draws each of its primitives once for every instance in the batch. The
/// decoded model itself is parsed once and shared through
/// [`crate::props::PropAssets`], and its images are shared through [`Arc`], so a
/// model placed in twenty cells still has one decoded copy per texture.
#[derive(Clone, Debug)]
pub struct PropMeshBatch {
    /// Catalogue model path, e.g. `models/chair.glb`.
    pub model: String,
    /// Every texture the model uses, indexed by [`PropSubmeshBatch::texture`]
    /// and by [`MaterialEmission::mask`]. Shared with every other batch of the
    /// same model.
    pub textures: Vec<Arc<crate::loader::RawImage>>,
    /// One entry per primitive of the model, in index-buffer order. Primitives
    /// that draw nothing are absent, so a batch with an empty `indices` has no
    /// submeshes either.
    pub submeshes: Vec<PropSubmeshBatch>,
    /// Pre-transformed vertices, referenced by `indices`.
    ///
    /// The historical Low path retains source indexing. Surface-lightmapped
    /// static models split triangle corners so each face owns its atlas UVs
    /// and normal frame without borrowing light across hard edges.
    pub vertices: Vec<Vertex>,
    /// `GL_UNSIGNED_SHORT` indices into `vertices`, offset per instance and
    /// grouped so each submesh's range is contiguous.
    pub indices: Vec<u16>,
    /// World-space bounds of every instance in this batch, used for frustum
    /// culling. One batch covers one model inside one spatial cell, so a prop
    /// field spread over a level becomes several cullable ranges of the same
    /// model instead of one range spanning the whole level.
    pub bounds: crate::spatial::Aabb,
}

/// Index lists accumulated per primitive while instances are appended.
///
/// Grouping the indices here (rather than keeping each instance's runs inline)
/// is what lets the finished batch draw one material across every instance in
/// one call: instance order never leaks into the draw calls.
struct BatchBuilder {
    model: String,
    textures: Vec<Arc<crate::loader::RawImage>>,
    primitives: Vec<PrimitiveBuilder>,
    vertices: Vec<Vertex>,
    bounds: crate::spatial::Aabb,
    /// Total indices accumulated, so a new instance can be rejected before it
    /// writes anything it cannot finish.
    index_count: usize,
    lighting_sources: Vec<usize>,
    sampled_light: Vec<crate::lighting::LightColor>,
}

struct PrimitiveBuilder {
    texture: Option<u16>,
    emission: MaterialEmission,
    alpha: MaterialAlpha,
    indices: Vec<u16>,
}

impl BatchBuilder {
    fn new(
        model_path: &str,
        model: &crate::gltf::PropModel,
        textures: Vec<Arc<crate::loader::RawImage>>,
    ) -> Self {
        Self {
            model: model_path.to_string(),
            textures,
            primitives: model
                .submeshes
                .iter()
                .map(|submesh| PrimitiveBuilder {
                    texture: submesh.texture,
                    emission: submesh.emission,
                    alpha: submesh.alpha,
                    indices: Vec::new(),
                })
                .collect(),
            vertices: Vec::with_capacity(model.vertices.len()),
            bounds: crate::spatial::Aabb::EMPTY,
            index_count: 0,
            lighting_sources: lighting_sources(&model.vertices),
            sampled_light: Vec::with_capacity(model.vertices.len()),
        }
    }

    /// Whether one more instance of `model` fits this batch's 16-bit offsets.
    const fn has_room_for(&self, model: &crate::gltf::PropModel, lightmapped: bool) -> bool {
        let count = if lightmapped {
            model.indices.len()
        } else {
            model.vertices.len()
        };
        self.vertices.len().saturating_add(count) <= crate::spatial::MAX_INDEX_VERTICES
    }

    /// Transforms and appends one instance. The caller must have checked
    /// [`Self::has_room_for`].
    fn push_instance(
        &mut self,
        transform: &glam::Mat4,
        asset: &crate::props::LoadedPropAsset,
        lighting: &LevelLighting,
    ) {
        let model = &asset.model;
        append_instance_vertices(
            &mut self.vertices,
            transform,
            &model.vertices,
            lighting,
            &self.lighting_sources,
            &mut self.sampled_light,
        );
        let base =
            u16::try_from(self.vertices.len().saturating_sub(model.vertices.len())).unwrap_or(0);
        for (slot, submesh) in model.submeshes.iter().enumerate() {
            let Some(primitive) = self.primitives.get_mut(slot) else {
                continue;
            };
            let start = usize::try_from(submesh.first_index).unwrap_or(0);
            let count = usize::try_from(submesh.index_count).unwrap_or(0);
            let Some(range) = model.indices.get(start..start.saturating_add(count)) else {
                continue;
            };
            primitive
                .indices
                .extend(range.iter().map(|index| base.saturating_add(*index)));
            self.index_count = self.index_count.saturating_add(count);
        }
    }

    /// Static models keep their source albedo and receive the same physically
    /// solved HDR atlas as architecture. Animated models retain the probe path.
    fn push_lightmapped_instance(
        &mut self,
        transform: &glam::Mat4,
        asset: &crate::props::LoadedPropAsset,
        plan: &mut crate::lighting::lightmap::LightmapPlan,
        bounds: &crate::spatial::Aabb,
    ) {
        let source = &asset.model;
        let normal_matrix = transform.inverse().transpose();
        let large = bounds
            .max
            .iter()
            .zip(bounds.min)
            .any(|(high, low)| high - low > 3.0);
        for (slot, submesh) in source.submeshes.iter().enumerate() {
            let Some(primitive) = self.primitives.get_mut(slot) else {
                continue;
            };
            let start = usize::try_from(submesh.first_index).unwrap_or(0);
            let count = usize::try_from(submesh.index_count).unwrap_or(0);
            let Some(indices) = source.indices.get(start..start.saturating_add(count)) else {
                continue;
            };
            let density = if large || submesh.alpha.mode == crate::materials::AlphaMode::Cutout {
                4.0
            } else {
                16.0
            };
            for triangle in indices.as_chunks::<3>().0 {
                let [Some(a), Some(b), Some(c)] =
                    triangle.map(|index| source.vertices.get(usize::from(index)))
                else {
                    continue;
                };
                let Some(mut vertices) =
                    model_triangle_vertices([a, b, c], transform, &normal_matrix)
                else {
                    continue;
                };
                if !plan.stamp_prop_triangle(&mut vertices, density) && !plan.failed() {
                    continue;
                }
                let Ok(base) = u16::try_from(self.vertices.len()) else {
                    continue;
                };
                self.vertices.extend(vertices);
                primitive
                    .indices
                    .extend([base, base.saturating_add(1), base.saturating_add(2)]);
                self.index_count = self.index_count.saturating_add(3);
            }
        }
    }

    fn finish(self) -> PropMeshBatch {
        let mut indices: Vec<u16> = Vec::with_capacity(self.index_count);
        let mut submeshes: Vec<PropSubmeshBatch> = Vec::with_capacity(self.primitives.len());
        for primitive in self.primitives {
            if primitive.indices.is_empty() {
                continue;
            }
            let first_index = u32::try_from(indices.len()).unwrap_or(0);
            let index_count = u32::try_from(primitive.indices.len()).unwrap_or(0);
            indices.extend_from_slice(&primitive.indices);
            submeshes.push(PropSubmeshBatch {
                texture: primitive.texture,
                emission: primitive.emission,
                alpha: primitive.alpha,
                first_index,
                index_count,
            });
        }
        PropMeshBatch {
            model: self.model,
            textures: self.textures,
            submeshes,
            vertices: self.vertices,
            indices,
            bounds: self.bounds,
        }
    }
}

/// Resolves every placed prop into either a batched real mesh or a fallback box,
/// sharing one decoded model (and its textures) per distinct model path.
///
/// Baked lighting is sampled per transformed vertex in world space, so a prop
/// standing on a crate or lying on a bed is lit at its real height and still
/// contributes to the same shared per-model batch (one draw call per primitive
/// per model and cell).
pub fn resolve_prop_instances<'a>(
    level: &'a LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
    lighting: &LevelLighting,
    surfaces: &LevelSurfaces<'_>,
) -> (Vec<PropMeshBatch>, Vec<&'a PropDef>) {
    resolve_prop_instances_inner(level, catalog, assets, lighting, surfaces, None)
}

/// Resolves static surface receivers after architecture has reserved its atlas
/// space. Moving/animated models continue to use their independent probe path.
pub fn resolve_prop_instances_lightmapped<'a>(
    level: &'a LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
    lighting: &LevelLighting,
    surfaces: &LevelSurfaces<'_>,
    plan: &mut crate::lighting::lightmap::LightmapPlan,
) -> (Vec<PropMeshBatch>, Vec<&'a PropDef>) {
    resolve_prop_instances_inner(level, catalog, assets, lighting, surfaces, Some(plan))
}

#[allow(clippy::too_many_lines)] // one bounded deterministic model/cell batching pass
fn resolve_prop_instances_inner<'a>(
    level: &'a LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
    lighting: &LevelLighting,
    surfaces: &LevelSurfaces<'_>,
    mut plan: Option<&mut crate::lighting::lightmap::LightmapPlan>,
) -> (Vec<PropMeshBatch>, Vec<&'a PropDef>) {
    use std::collections::{HashMap, HashSet};

    let grid = spatial_cell_grid(level);
    let mut builders: Vec<BatchBuilder> = Vec::new();
    // Keyed by (model, cell): one drawable range per model per spatial cell.
    let mut index_by_batch: HashMap<(String, crate::spatial::CellKey), usize> = HashMap::new();
    let mut models_seen: HashSet<String> = HashSet::new();
    let mut textures_by_model: HashMap<String, Vec<Arc<crate::loader::RawImage>>> = HashMap::new();
    let mut fallbacks: Vec<&'a PropDef> = Vec::new();
    let mut busy_vertices = 0usize;

    for prop in &level.props {
        // A floating prop is drawn by the dynamic path at the water surface,
        // not by the static batch at its dry authored position, and it is not
        // a placeholder box either: the dynamic spawn reports its own asset
        // failure.
        if prop.float.is_some() {
            continue;
        }
        let entry = catalog.get(&prop.model);
        let Some(model_path) = entry.model.clone() else {
            fallbacks.push(prop);
            continue;
        };
        if busy_vertices >= crate::level::MAX_LEVEL_PROP_VERTICES {
            fallbacks.push(prop);
            continue;
        }
        let asset = match assets.resolve(&model_path) {
            Ok(asset) => asset,
            Err(error) => {
                assets.report_failure(&model_path, &error);
                fallbacks.push(prop);
                continue;
            }
        };
        if !models_seen.contains(&model_path)
            && models_seen.len() >= crate::level::MAX_LEVEL_PROP_MODELS
        {
            fallbacks.push(prop);
            continue;
        }

        let lightmapped = plan.is_some() && !asset.model.is_animatable();

        // A prop's authored `y` is an offset above the local walkable floor, so
        // a chair in an elevated room or a recessed region lands on the surface
        // it was placed against.
        let base_y = surfaces.floor_y_at(prop.x, prop.z).unwrap_or(0.0);
        let model = prop_instance_matrix(prop, base_y);
        // Cull by the instance's real world-space extent, not by the cell it
        // happens to be centred in: a chair on a cell boundary must not be
        // culled while a sliver of it is still on screen.
        let instance_bounds = match asset.model.bounds() {
            Some((low, high)) => transform_bounds(
                &crate::spatial::Aabb {
                    min: low,
                    max: high,
                },
                &model,
            ),
            None => crate::spatial::Aabb::from_point([prop.x, base_y + prop.y, prop.z]),
        };
        let cell = grid.cell_of(instance_bounds.centre());

        // One batch holds every instance of a model inside one spatial cell, but
        // never more than a 16-bit index can address: `PropMeshBatch::indices`
        // are `GL_UNSIGNED_SHORT` offsets into the batch's own vertex list, so a
        // cell holding hundreds of instances has to become several batches.
        let key = (model_path.clone(), cell);
        let batch_index = match index_by_batch.get(&key).copied() {
            Some(index)
                if builders
                    .get(index)
                    .is_some_and(|b| b.has_room_for(&asset.model, lightmapped)) =>
            {
                index
            }
            _ => {
                models_seen.insert(model_path.clone());
                let textures = textures_by_model
                    .entry(model_path.clone())
                    .or_insert_with(|| asset.model.textures.iter().cloned().map(Arc::new).collect())
                    .clone();
                let builder = BatchBuilder::new(&model_path, &asset.model, textures);
                if !builder.has_room_for(&asset.model, lightmapped) {
                    fallbacks.push(prop);
                    continue;
                }
                builders.push(builder);
                let index = builders.len().saturating_sub(1);
                index_by_batch.insert(key.clone(), index);
                index
            }
        };
        let Some(builder) = builders.get_mut(batch_index) else {
            fallbacks.push(prop);
            continue;
        };
        builder.bounds = builder.bounds.union(&instance_bounds);
        if lightmapped && let Some(plan) = plan.as_deref_mut() {
            builder.push_lightmapped_instance(&model, &asset, plan, &instance_bounds);
        } else {
            builder.push_instance(&model, &asset, lighting);
        }
        busy_vertices = busy_vertices.saturating_add(if lightmapped {
            asset.model.indices.len()
        } else {
            asset.model.vertices.len()
        });
    }

    let batches = builders.into_iter().map(BatchBuilder::finish).collect();
    (batches, fallbacks)
}

/// Derives a stable UV tangent frame for each actual triangle while retaining
/// valid smooth normals. Lighting coordinates never replace the source UVs.
#[allow(clippy::arithmetic_side_effects)] // bounded float frame math; degenerate triangles are rejected
fn model_triangle_vertices(
    source: [&crate::gltf::PropVertex; 3],
    transform: &glam::Mat4,
    normal_matrix: &glam::Mat4,
) -> Option<[Vertex; 3]> {
    let [a, b, c] =
        source.map(|vertex| transform.transform_point3(glam::Vec3::from_array(vertex.pos)));
    let edge_u = b - a;
    let edge_v = c - a;
    let face = edge_u.cross(edge_v).normalize_or_zero();
    // Zero-area source triangles draw no pixels and cannot be receivers.
    if face == glam::Vec3::ZERO {
        return None;
    }
    let [uv_a, uv_b, uv_c] = source.map(|vertex| glam::Vec2::from_array(vertex.uv));
    let du = uv_b - uv_a;
    let dv = uv_c - uv_a;
    let determinant = du.x.mul_add(dv.y, -(du.y * dv.x));
    let (tangent, bitangent) = if determinant.abs() > 1.0e-8 {
        (
            (edge_u * dv.y - edge_v * du.y) / determinant,
            (edge_v * du.x - edge_u * dv.x) / determinant,
        )
    } else {
        (edge_u, edge_v)
    };
    Some(source.map(|vertex| {
        let normal = vertex.normal.map_or(face, |normal| {
            normal_matrix
                .transform_vector3(glam::Vec3::from_array(normal))
                .normalize_or_zero()
        });
        let projected = (tangent - normal * tangent.dot(normal)).normalize_or_zero();
        let frame_u = if projected == glam::Vec3::ZERO {
            normal.any_orthonormal_vector()
        } else {
            projected
        };
        Vertex {
            pos: transform
                .transform_point3(glam::Vec3::from_array(vertex.pos))
                .to_array(),
            color: vertex.color,
            uv: vertex.uv,
            normal: normal.to_array(),
            tangent: frame_u.to_array(),
            handedness: if normal.cross(frame_u).dot(bitangent) < 0.0 {
                -1.0
            } else {
                1.0
            },
            ..Vertex::UNLIT
        }
    }))
}

/// UV seams and hard edges duplicate positions. Share only their light sample;
/// colours, UVs, vertex order and per-placement transforms remain independent.
fn lighting_sources(vertices: &[crate::gltf::PropVertex]) -> Vec<usize> {
    let mut first = std::collections::HashMap::with_capacity(vertices.len());
    vertices
        .iter()
        .enumerate()
        .map(|(index, vertex)| *first.entry(vertex.pos.map(f32::to_bits)).or_insert(index))
        .collect()
}

/// Transforms and lights one instance's model vertices.
///
/// Every model vertex is transformed, and equal positions share lighting per
/// placement, and the environment is baked into the instance's colour: the same
/// model in a dark corner and under a fixture still shares one batch, but is no
/// longer uniformly lit.
fn append_instance_vertices(
    batch: &mut Vec<Vertex>,
    model: &glam::Mat4,
    source: &[crate::gltf::PropVertex],
    lighting: &LevelLighting,
    representatives: &[usize],
    samples: &mut Vec<crate::lighting::LightColor>,
) {
    samples.clear();
    batch.reserve(source.len());
    for (index, vertex) in source.iter().enumerate() {
        let position =
            model.transform_point3(glam::Vec3::new(vertex.pos[0], vertex.pos[1], vertex.pos[2]));
        let light = representatives
            .get(index)
            .and_then(|first| samples.get(*first))
            .copied()
            .unwrap_or_else(|| lighting.sample(position.x, position.y, position.z));
        samples.push(light);
        batch.push(Vertex {
            pos: [position.x, position.y, position.z],
            color: [
                vertex.color[0] * light.r,
                vertex.color[1] * light.g,
                vertex.color[2] * light.b,
                vertex.color[3],
            ],
            uv: vertex.uv,
            ..Vertex::UNLIT
        });
    }
}

/// World-space bounds of a local-space box placed by `transform`.
///
/// Only the eight corners are transformed: the result is the AABB of the
/// rotated box, which is conservative (never smaller than the real geometry),
/// which is exactly what a culling test needs.
pub fn transform_bounds(
    local: &crate::spatial::Aabb,
    transform: &glam::Mat4,
) -> crate::spatial::Aabb {
    let mut bounds = crate::spatial::Aabb::EMPTY;
    for x in [local.min[0], local.max[0]] {
        for y in [local.min[1], local.max[1]] {
            for z in [local.min[2], local.max[2]] {
                let point = transform.transform_point3(glam::Vec3::new(x, y, z));
                bounds.expand([point.x, point.y, point.z]);
            }
        }
    }
    bounds
}

/// Instance transform for a placed prop: translate, rotate about Y and scale.
///
/// `base_y` is the world Y of the walkable floor at the prop's `(x, z)`; the
/// authored `prop.y` is an offset above it. This is exactly the transform the
/// placeholder boxes use (see [`add_prop_box`]), so a prop keeps its position,
/// orientation and vertical offset when its real model replaces the box. Model
/// space is metres with the origin at the floor-contact centre (see
/// `assets/README.md`).
#[must_use]
pub fn prop_instance_matrix(prop: &PropDef, base_y: f32) -> glam::Mat4 {
    let rotation = glam::Mat4::from_rotation_y(prop.rotation_degrees.to_radians());
    let scale = glam::Mat4::from_scale(glam::Vec3::splat(prop.scale));
    let translation =
        glam::Mat4::from_translation(glam::Vec3::new(prop.x, base_y + prop.y, prop.z));
    // `glam` matrix multiplication is per-element `f32` arithmetic with no
    // overflow or panic path; clippy cannot see that through the operator impl.
    #[allow(clippy::arithmetic_side_effects)]
    let transform = translation * rotation * scale;
    transform
}

#[cfg(test)]
mod lighting_reuse_tests {
    use super::{LevelLighting, append_instance_vertices, lighting_sources};

    #[test]
    fn seam_vertices_keep_exact_colours_and_uvs_across_instances() -> Result<(), String> {
        let level = crate::level::LevelDef::from_json(include_str!(
            "../../../tests/fixtures/levels/test_room.json"
        ))
        .map_err(|error| error.to_string())?;
        let lighting = LevelLighting::bake(&level);
        let mut assets = crate::props::PropAssets::load_default();
        let asset = assets.resolve("environment/office/props/models/chair.glb")?;
        let source = &asset.model.vertices;
        let shared = lighting_sources(source);
        let independent: Vec<usize> = (0..source.len()).collect();
        assert_ne!(
            shared, independent,
            "fixture must exercise duplicated positions"
        );
        let mut reused_samples = Vec::new();
        for position in [glam::Vec3::ZERO, glam::Vec3::new(2.0, 0.3, 1.0)] {
            let transform = glam::Mat4::from_scale_rotation_translation(
                glam::Vec3::splat(1.25),
                glam::Quat::from_rotation_y(0.73),
                position,
            );
            let mut actual = Vec::new();
            let mut reference = Vec::new();
            append_instance_vertices(
                &mut actual,
                &transform,
                source,
                &lighting,
                &shared,
                &mut reused_samples,
            );
            append_instance_vertices(
                &mut reference,
                &transform,
                source,
                &lighting,
                &independent,
                &mut Vec::new(),
            );
            assert_eq!(actual, reference);
        }
        Ok(())
    }
}
