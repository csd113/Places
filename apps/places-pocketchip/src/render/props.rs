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
//!
//! The prop albedo atlas
//! ---------------------
//! One model per level still costs one texture upload and one material change
//! per draw, which is the dominant CPU cost on the target device. When
//! [`super::prop_atlas`] is enabled (the default; see
//! `LIMINAL_PROP_ATLAS`), every model that samples exactly one unmasked albedo
//! is instead baked into one shared 1024-texel atlas and its instance UVs are
//! remapped into that model's cell *here*, as the instance vertices are
//! appended. Models that cannot be atlased (an emissive mask, several albedos,
//! no albedo at all, an unusable image, or a full atlas) keep the historical
//! per-model texture path, and their UVs are left exactly as authored.

use std::rc::Rc;

use super::prop_atlas::{AtlasPlacement, PropAtlas};
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
    /// The model material's `doubleSided` declaration. A double-sided primitive
    /// draws with back-face culling disabled; a single-sided one is wound so
    /// its front faces out of the solid.
    pub double_sided: bool,
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
    /// The level's shared albedo atlas, when this model was baked into it.
    ///
    /// When `Some`, every UV in `vertices` already addresses this model's
    /// atlas cell and the renderer must neither upload `textures` for the model
    /// nor draw its ranges with any other texture. `None` keeps the historical
    /// per-model path untouched. Every atlased batch of one level shares the
    /// same [`Rc`], so the renderer can upload the sheet exactly once.
    pub atlas: Option<Rc<PropAtlas>>,
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
    /// This model's atlas cell, when it was atlased. Every appended vertex's UV
    /// is remapped through it.
    atlas: Option<AtlasPlacement>,
}

struct PrimitiveBuilder {
    texture: Option<u16>,
    emission: MaterialEmission,
    double_sided: bool,
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
                    double_sided: submesh.double_sided,
                    indices: Vec::new(),
                })
                .collect(),
            vertices: Vec::with_capacity(model.vertices.len()),
            bounds: crate::spatial::Aabb::EMPTY,
            index_count: 0,
            atlas: None,
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
        append_instance_vertices(
            &mut self.vertices,
            transform,
            &model.vertices,
            lighting,
            self.atlas,
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

    fn finish(self, atlas: Option<&Rc<PropAtlas>>) -> PropMeshBatch {
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
                double_sided: primitive.double_sided,
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
            // A model only carries a placement when the level's atlas exists,
            // so the shared handle is present exactly when the UVs were
            // remapped: the two can never disagree.
            atlas: self.atlas.and_then(|_| atlas.map(Rc::clone)),
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
///
/// The level's albedo atlas is planned and filled in the same pass: each model
/// that can share one cell gets one, and the instance UVs appended for it are
/// remapped into that cell. See [`super::prop_atlas`].
pub(super) fn resolve_prop_instances<'a>(
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
    // Model path -> its atlas cell, or `None` once it has been tried and
    // rejected: a model is never retried, so a full atlas or a bad image
    // degrades to the per-model path exactly once per model.
    let mut atlas_cells: HashMap<String, Option<AtlasPlacement>> = HashMap::new();
    let mut fallbacks: Vec<&'a PropDef> = Vec::new();
    let mut busy_vertices = 0usize;

    // Read the startup override once for the whole build, as the other
    // `LIMINAL_*` switches are. The atlas is built here, not in the renderer,
    // because the UV remap has to be baked as instances are appended.
    let atlas_enabled = super::prop_atlas::enabled();
    // Only the fits and copies are timed, not the whole prop pass: the reported
    // figure has to be what the atlas itself costs the level build.
    let mut atlas_millis = 0.0f64;
    let mut atlas = PropAtlas::new();

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
                let placement = *atlas_cells.entry(model_path.clone()).or_insert_with(|| {
                    plan_atlas_cell(
                        &mut atlas,
                        atlas_enabled,
                        &asset.model,
                        &textures,
                        &mut atlas_millis,
                    )
                });
                let mut builder = BatchBuilder::new(&model_path, &asset.model, textures);
                builder.atlas = placement;
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

    atlas.record_build_millis(atlas_millis);
    // The atlas is shared by every atlased batch of this level; it is dropped
    // with them once the renderer has uploaded its pixels.
    let shared = (atlas_enabled && atlas.cells_used() > 0).then(|| Rc::new(atlas));
    let batches = builders
        .into_iter()
        .map(|builder| builder.finish(shared.as_ref()))
        .collect();
    (batches, fallbacks)
}

/// Plans one model's atlas cell, timing the fit and copy that fills it.
///
/// Returns `None` — leaving the model on its own texture — when the atlas is
/// disabled, when the model cannot share a single cell, or when the sheet is
/// full. The timing accumulates into `millis` so the developer line reports what
/// the atlas itself costs the level build, not the whole prop pass.
fn plan_atlas_cell(
    atlas: &mut PropAtlas,
    enabled: bool,
    model: &crate::gltf::PropModel,
    textures: &[Rc<crate::loader::RawImage>],
    millis: &mut f64,
) -> Option<AtlasPlacement> {
    if !enabled {
        return None;
    }
    // `place` checks the cell budget and the image itself; any refusal leaves
    // the model on its own texture.
    let slot = super::prop_atlas::albedo_slot(model)?;
    let image = textures.get(usize::from(slot))?;
    let started = std::time::Instant::now();
    let placement = atlas.place(image);
    *millis = started.elapsed().as_secs_f64().mul_add(1000.0, *millis);
    placement
}

/// Transforms and lights one instance's model vertices.
///
/// Every distinct model vertex is transformed and lit exactly once per
/// placement, and the environment is baked into the instance's colour: the same
/// model in a dark corner and under a fixture still shares one batch, but is no
/// longer uniformly lit.
///
/// `atlas` remaps the source UVs into the model's atlas cell at append time, so
/// the fragment shader still samples `v_uv` directly and never does atlas
/// arithmetic. A model without a placement keeps its authored UVs exactly.
fn append_instance_vertices(
    batch: &mut Vec<Vertex>,
    model: &glam::Mat4,
    source: &[crate::gltf::PropVertex],
    lighting: &LevelLighting,
    atlas: Option<AtlasPlacement>,
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
            uv: atlas.map_or(vertex.uv, |placement| placement.remap_uv(vertex.uv)),
            ..Vertex::UNLIT
        });
    }
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
    let translation =
        glam::Mat4::from_translation(glam::Vec3::new(prop.x, base_y + prop.y, prop.z));
    // `glam` matrix multiplication is per-element `f32` arithmetic with no
    // overflow or panic path; clippy cannot see that through the operator impl.
    #[allow(clippy::arithmetic_side_effects)]
    let transform = translation * rotation * scale;
    transform
}
