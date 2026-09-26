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

use std::rc::Rc;

use super::{LevelDef, LevelLighting, LevelSurfaces, PropDef, Vertex, spatial_cell_grid};
use crate::materials::MaterialEmission;

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
/// [`crate::props::PropAssets`], and its images are shared through [`Rc`], so a
/// model placed in twenty cells still has one decoded copy per texture.
#[derive(Clone, Debug)]
pub struct PropMeshBatch {
    /// Catalogue model path, e.g. `models/chair.glb`.
    pub model: String,
    /// Every texture the model uses, indexed by [`PropSubmeshBatch::texture`]
    /// and by [`MaterialEmission::mask`]. Shared with every other batch of the
    /// same model.
    pub textures: Vec<Rc<crate::loader::RawImage>>,
    /// One entry per primitive of the model, in index-buffer order. Primitives
    /// that draw nothing are absent, so a batch with an empty `indices` has no
    /// submeshes either.
    pub submeshes: Vec<PropSubmeshBatch>,
    /// Pre-transformed vertices, referenced by `indices`.
    ///
    /// The GLB already stores its mesh indexed, so an instance is a vertex
    /// offset and the model's own index list; nothing is expanded. That keeps
    /// the GPU shading ~30% fewer vertices per instance than the flat triangle
    /// list this used to build.
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
    textures: Vec<Rc<crate::loader::RawImage>>,
    primitives: Vec<PrimitiveBuilder>,
    vertices: Vec<Vertex>,
    bounds: crate::spatial::Aabb,
    /// Total indices accumulated, so a new instance can be rejected before it
    /// writes anything it cannot finish.
    index_count: usize,
}

struct PrimitiveBuilder {
    texture: Option<u16>,
    emission: MaterialEmission,
    indices: Vec<u16>,
}

impl BatchBuilder {
    fn new(
        model_path: &str,
        model: &crate::gltf::PropModel,
        textures: Vec<Rc<crate::loader::RawImage>>,
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
                    indices: Vec::new(),
                })
                .collect(),
            vertices: Vec::with_capacity(model.vertices.len()),
            bounds: crate::spatial::Aabb::EMPTY,
            index_count: 0,
        }
    }

    /// Whether one more instance of `model` fits this batch's 16-bit offsets.
    const fn has_room_for(&self, model: &crate::gltf::PropModel) -> bool {
        self.vertices.len().saturating_add(model.vertices.len())
            <= crate::spatial::MAX_INDEX_VERTICES
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
        append_instance_vertices(&mut self.vertices, transform, &model.vertices, lighting);
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
    use std::collections::{HashMap, HashSet};

    let grid = spatial_cell_grid(level);
    let mut builders: Vec<BatchBuilder> = Vec::new();
    // Keyed by (model, cell): one drawable range per model per spatial cell.
    let mut index_by_batch: HashMap<(String, crate::spatial::CellKey), usize> = HashMap::new();
    let mut models_seen: HashSet<String> = HashSet::new();
    let mut textures_by_model: HashMap<String, Vec<Rc<crate::loader::RawImage>>> = HashMap::new();
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
                    .is_some_and(|b| b.has_room_for(&asset.model)) =>
            {
                index
            }
            _ => {
                models_seen.insert(model_path.clone());
                let textures = textures_by_model
                    .entry(model_path.clone())
                    .or_insert_with(|| asset.model.textures.iter().cloned().map(Rc::new).collect())
                    .clone();
                let builder = BatchBuilder::new(&model_path, &asset.model, textures);
                if !builder.has_room_for(&asset.model) {
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
        builder.push_instance(&model, &asset, lighting);
        busy_vertices = busy_vertices.saturating_add(asset.model.vertices.len());
    }

    let batches = builders.into_iter().map(BatchBuilder::finish).collect();
    (batches, fallbacks)
}

/// Transforms and lights one instance's model vertices.
///
/// Every distinct model vertex is transformed and lit exactly once per
/// placement, and the environment is baked into the instance's colour: the same
/// model in a dark corner and under a fixture still shares one batch, but is no
/// longer uniformly lit.
fn append_instance_vertices(
    batch: &mut Vec<Vertex>,
    model: &glam::Mat4,
    source: &[crate::gltf::PropVertex],
    lighting: &LevelLighting,
) {
    for vertex in source {
        let position =
            model.transform_point3(glam::Vec3::new(vertex.pos[0], vertex.pos[1], vertex.pos[2]));
        let light = lighting.sample(position.x, position.y, position.z);
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
