//! Prop instancing and lighting.
//!
//! Every placed instance of one model is transformed and lit on the CPU at
//! level load time and appended to a per-model batch, so the renderer binds one
//! buffer and one texture per model and issues one draw call for all of its
//! instances. A prop with no usable asset falls back to a placeholder box.

use super::*;

/// Instanced prop geometry for one distinct prop model in a level.
///
/// Every placed instance of the same model is transformed on the CPU at level
/// load time and appended here, so the renderer binds one buffer and one
/// texture per model and issues one draw call for all of its instances. The
/// decoded model itself is parsed once and shared through
/// [`crate::props::PropAssets`].
#[derive(Clone, Debug)]
pub struct PropMeshBatch {
    /// Catalogue model path, e.g. `models/chair.glb`.
    pub model: String,
    /// Diffuse texture shared by every instance in this batch.
    pub texture: crate::loader::RawImage,
    /// Pre-transformed vertices, referenced by `indices`.
    ///
    /// The GLB already stores its mesh indexed, so an instance is a vertex
    /// offset and the model's own index list; nothing is expanded. That keeps
    /// the GPU shading ~30% fewer vertices per instance than the flat triangle
    /// list this used to build.
    pub vertices: Vec<Vertex>,
    /// `GL_UNSIGNED_SHORT` indices into `vertices`, offset per instance.
    pub indices: Vec<u16>,
    /// World-space bounds of every instance in this batch, used for frustum
    /// culling. One batch covers one model inside one spatial cell, so a prop
    /// field spread over a level becomes several cullable ranges of the same
    /// model instead of one range spanning the whole level.
    pub bounds: crate::spatial::Aabb,
}

/// Resolves every placed prop into either a batched real mesh or a fallback box,
/// sharing one decoded model (and one texture) per distinct model path.
///
/// Baked lighting is sampled per transformed vertex in world space, so a prop
/// standing on a crate or lying on a bed is lit at its real height and still
/// contributes to the same shared per-model batch (one draw call per model).
pub(super) fn resolve_prop_instances<'a>(
    level: &'a LevelDef,
    catalog: &crate::loader::PropCatalog,
    assets: &mut crate::props::PropAssets,
    lighting: &LevelLighting,
    surfaces: &LevelSurfaces<'_>,
) -> (Vec<PropMeshBatch>, Vec<&'a PropDef>) {
    use std::collections::{HashMap, HashSet};

    let grid = spatial_cell_grid(level);
    let mut batches: Vec<PropMeshBatch> = Vec::new();
    // Keyed by (model, cell): one drawable range per model per spatial cell.
    let mut index_by_batch: HashMap<(String, crate::spatial::CellKey), usize> = HashMap::new();
    let mut models_seen: HashSet<String> = HashSet::new();
    let mut fallbacks: Vec<&'a PropDef> = Vec::new();
    let mut busy_vertices = 0usize;

    for prop in &level.props {
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
        let needs_new_batch = index_by_batch.get(&key).is_none_or(|index| {
            batches[*index].vertices.len() + asset.model.vertices.len()
                > crate::spatial::MAX_INDEX_VERTICES
        });
        if needs_new_batch {
            models_seen.insert(model_path.clone());
            batches.push(PropMeshBatch {
                model: model_path.clone(),
                texture: asset.model.texture.clone(),
                vertices: Vec::with_capacity(asset.model.vertices.len()),
                indices: Vec::with_capacity(asset.model.indices.len()),
                bounds: crate::spatial::Aabb::EMPTY,
            });
            index_by_batch.insert(key, batches.len() - 1);
        }
        let batch_index = index_by_batch[&(model_path.clone(), cell)];
        let batch = &mut batches[batch_index];
        batch.bounds = batch.bounds.union(&instance_bounds);
        // One instance is the model's own index list shifted by the vertex
        // offset this instance was appended at. Nothing is expanded into a flat
        // triangle list, and each distinct model vertex is transformed and
        // lit exactly once per placement.
        let base = batch.vertices.len();
        for vertex in &asset.model.vertices {
            let position = model.transform_point3(glam::Vec3::new(
                vertex.pos[0],
                vertex.pos[1],
                vertex.pos[2],
            ));
            // Bake the environment into the instance's colour: the same model in
            // a dark corner and under a fixture still shares one batch, but is
            // no longer uniformly lit.
            let light = lighting.sample(position.x, position.y, position.z);
            batch.vertices.push(Vertex {
                pos: [position.x, position.y, position.z],
                color: [
                    vertex.color[0] * light.r,
                    vertex.color[1] * light.g,
                    vertex.color[2] * light.b,
                    vertex.color[3],
                ],
                uv: vertex.uv,
            });
        }
        for index in &asset.model.indices {
            batch
                .indices
                .push(u16::try_from(base).unwrap_or(u16::MAX) + *index);
        }
        busy_vertices += asset.model.vertices.len();
    }

    (batches, fallbacks)
}

/// World-space bounds of a local-space box placed by `transform`.
///
/// Only the eight corners are transformed: the result is the AABB of the
/// rotated box, which is conservative (never smaller than the real geometry),
/// which is exactly what a culling test needs.
pub(super) fn transform_bounds(
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
    glam::Mat4::from_translation(glam::Vec3::new(prop.x, base_y + prop.y, prop.z))
        * rotation
        * scale
}
