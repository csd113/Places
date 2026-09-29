//! Props: the neutral per-model batches as wgpu buffers and materials.
//!
//! The renderer-neutral build (`render::common::props`) has already resolved
//! every placed prop into world-space, per-vertex-lit geometry, one batch per
//! `(model, spatial cell)` with one submesh per glTF primitive. This module
//! uploads exactly that:
//!
//! * one 16-bit-indexable vertex/index buffer pair per batch;
//! * one clamped GPU sheet per model texture, cached through the texture cache
//!   under the bounded `Catalog` retention policy;
//! * one plain-opaque GPU material per distinct `(sheet, emission)` submesh,
//!   because a prop's emission is per primitive and the model parser reads no
//!   normal map, alpha mode or reflection contract;
//! * one draw per submesh, culled by the batch bounds.
//!
//! Props are opaque by construction (`common::materials::batch_pass_for` maps
//! `PropFallback` and every non-architectural family to the opaque pass), and
//! their light is already in the vertex colour, so the frame path draws them
//! with the ordinary pipeline between the static opaque pass and the cut-out
//! pass. GPU resources are prepared at installation: the
//! buffers, textures and materials are level resources; only the draws' light
//! scale and reflection gates change, and both are the materials' business.

use std::sync::Arc;

use super::material::{EmissionRecord, GpuMaterial};
use super::texture::{CacheOutcome, GpuTexture, TextureCache};
use super::world::{WORLD_VERTEX_STRIDE, WorldVertex};
use crate::materials::TextureOrigin;
use crate::quality::{QualityLevel, TextureClass};
use crate::render::common::materials::BatchPass;
use crate::render::common::props::PropMeshBatch;
use crate::spatial::Aabb;

/// One 16-bit-indexable prop buffer pair.
pub struct PropChunk {
    /// Interleaved [`WorldVertex`] data.
    pub vertex_buffer: wgpu::Buffer,
    /// `u16` triangle indices.
    pub index_buffer: wgpu::Buffer,
    /// Distinct vertices the chunk holds.
    pub vertex_count: u32,
    /// Indices the chunk holds.
    pub index_count: u32,
}

/// One prop submesh draw: one primitive of one model batch.
#[derive(Clone, Copy, Debug)]
pub struct PropDraw {
    /// Which [`PropChunk`].
    pub chunk: usize,
    /// First index in the chunk's index buffer.
    pub index_start: u32,
    /// Indices the draw covers.
    pub index_count: u32,
    /// Distinct vertices the draw indexes.
    pub vertex_count: u32,
    /// World-space bounds of every instance in the batch.
    pub bounds: Aabb,
    /// Slot into [`WgpuProps::textures`].
    pub texture: usize,
    /// Slot into [`WgpuProps::materials`].
    pub material: usize,
    /// Which world draw pass the submesh belongs to: opaque, or the
    /// alpha-tested cutout pass for a glTF `MASK` material (foliage cards).
    pub pass: BatchPass,
    /// True when the submesh's material emits; the emissive pass draws exactly
    /// these.
    pub emissive: bool,
}

/// What one level's prop upload produced, as plain counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PropGpuStats {
    /// Batched model cells uploaded.
    pub batches: usize,
    /// GPU buffer pairs.
    pub chunks: usize,
    /// Submesh draws.
    pub draws: usize,
    /// Distinct vertices uploaded.
    pub vertices: usize,
    /// Indices uploaded.
    pub indices: usize,
    /// Model sheets uploaded this level load.
    pub texture_uploads: usize,
    /// Model sheets the cache already held.
    pub texture_cache_hits: usize,
    /// Texel storage the sampled sheets occupy.
    pub resident_bytes: usize,
}

/// Every prop the loaded level draws.
pub struct WgpuProps {
    chunks: Vec<PropChunk>,
    draws: Vec<PropDraw>,
    materials: Vec<GpuMaterial>,
    textures: Vec<Arc<GpuTexture>>,
    stats: PropGpuStats,
}

/// Accumulates one world's prop resources in draw order across frame budgets.
///
/// The caller owns the batch cursor and retains this builder until every batch
/// has been visited. Material identities persist across calls, so splitting the
/// upload cannot change deduplication, ordering, or accounting.
pub struct PropUpload {
    props: WgpuProps,
    identities: Vec<(usize, [f32; 3], Option<usize>, u32)>,
    skip_models: Vec<String>,
    image_keys: std::collections::HashMap<(String, usize), ImageIdentity>,
    level: QualityLevel,
}

/// Kept only while an upload is assembled; cloned batches share the same image.
struct ImageIdentity {
    image: Arc<crate::loader::RawImage>,
    key: String,
}

impl PropUpload {
    pub(crate) fn new(skip_models: &[String], level: QualityLevel) -> Self {
        Self {
            props: WgpuProps {
                chunks: Vec::new(),
                draws: Vec::new(),
                materials: Vec::new(),
                textures: Vec::new(),
                stats: PropGpuStats::default(),
            },
            identities: Vec::new(),
            skip_models: skip_models.to_vec(),
            image_keys: std::collections::HashMap::new(),
            level,
        }
    }

    /// Visits one batch, including skipped and empty batches. The caller may
    /// stop between calls without exposing a partially installed world.
    #[allow(clippy::too_many_lines)] // one batch shares sheet slots, material identities and counters
    pub(crate) fn push_batch(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &mut TextureCache,
        material_layout: &wgpu::BindGroupLayout,
        batch: &PropMeshBatch,
    ) {
        if batch.indices.is_empty() || self.skip_models.iter().any(|model| model == &batch.model) {
            return;
        }
        let WgpuProps {
            stats,
            chunks,
            draws,
            materials,
            textures,
        } = &mut self.props;
        let identities = &mut self.identities;
        let level = self.level;
        stats.batches = stats.batches.saturating_add(1);
        // The model's sheets, once per texture list entry.
        let texture_base = textures.len();
        for (index, image) in batch.textures.iter().enumerate() {
            let identity = self
                .image_keys
                .entry((batch.model.clone(), index))
                .or_insert_with(|| ImageIdentity {
                    image: Arc::clone(image),
                    key: super::texture::image_content_key(
                        &format!("prop:{}:{index}", batch.model),
                        image,
                    ),
                });
            if !Arc::ptr_eq(&identity.image, image) {
                identity.image = Arc::clone(image);
                identity.key = super::texture::image_content_key(
                    &format!("prop:{}:{index}", batch.model),
                    image,
                );
            }
            let logical = &identity.key;
            let (outcome, texture) = cache.get_or_upload_fitted(
                device,
                queue,
                logical,
                image.as_ref(),
                TextureClass::Prop,
                TextureOrigin::Catalog,
                level,
            );
            match outcome {
                CacheOutcome::Uploaded => {
                    stats.texture_uploads = stats.texture_uploads.saturating_add(1);
                }
                CacheOutcome::Reused => {
                    stats.texture_cache_hits = stats.texture_cache_hits.saturating_add(1);
                }
            }
            stats.resident_bytes = stats.resident_bytes.saturating_add(
                usize::try_from(texture.meta().resident_bytes).unwrap_or(usize::MAX),
            );
            textures.push(texture);
        }
        let chunk = WgpuProps::upload_batch(device, queue, batch);
        let chunk_index = chunks.len();
        stats.vertices = stats
            .vertices
            .saturating_add(usize::try_from(chunk.vertex_count).unwrap_or(usize::MAX));
        stats.indices = stats
            .indices
            .saturating_add(usize::try_from(chunk.index_count).unwrap_or(usize::MAX));
        chunks.push(chunk);
        for submesh in &batch.submeshes {
            if submesh.index_count == 0 {
                continue;
            }
            let texture = texture_base
                .saturating_add(usize::from(submesh.texture.unwrap_or(0)))
                .min(textures.len().saturating_sub(1));
            let mask = submesh
                .emission
                .mask
                .and_then(|index| usize::from(index).checked_add(texture_base))
                .filter(|index| *index < textures.len());
            let record = EmissionRecord::material(submesh.emission, mask.is_some());
            // The cutoff is part of the material identity: two primitives that
            // share a sheet and emission but discard at different alpha levels
            // need different uniforms.
            let cutoff = submesh.alpha.cutoff.to_bits();
            let identity = (texture, record.color, mask, cutoff);
            let material = identities
                .iter()
                .position(|existing| *existing == identity)
                .unwrap_or_else(|| {
                    let mask_texture = mask
                        .and_then(|index| textures.get(index).cloned())
                        .unwrap_or_else(|| cache.fallback());
                    let gpu = GpuMaterial::plain_with_alpha(
                        device,
                        queue,
                        material_layout,
                        cache,
                        &mask_texture,
                        record,
                        submesh.alpha,
                    );
                    materials.push(gpu);
                    identities.push(identity);
                    materials.len().saturating_sub(1)
                });
            draws.push(PropDraw {
                chunk: chunk_index,
                index_start: submesh.first_index,
                index_count: submesh.index_count,
                vertex_count: chunks
                    .get(chunk_index)
                    .map_or(0, |chunk| chunk.vertex_count),
                bounds: batch.bounds,
                texture,
                material,
                pass: if submesh.alpha.is_cutout() {
                    BatchPass::Cutout
                } else {
                    BatchPass::Opaque
                },
                emissive: record.is_emissive(),
            });
        }
    }

    /// Consumes the completed accumulator; no GPU submission or wait occurs here.
    pub(crate) fn finish(mut self) -> WgpuProps {
        self.props.stats.chunks = self.props.chunks.len();
        self.props.stats.draws = self.props.draws.len();
        self.props
    }
}

impl WgpuProps {
    /// Uploads every batch of one neutral build, uploading the model sheets it
    /// uses through the shared texture cache.
    ///
    /// Called once per level load. A batch with no indices produces no chunk
    /// and no draws; a batch whose model path is in `skip_models` is skipped
    /// entirely (the character path draws that model's animated instances, and
    /// the static bind-pose batch must not draw underneath them); a texture
    /// that cannot be resolved still yields an entry (the shared fallback), so
    /// a broken model draws white rather than disappearing.
    #[must_use]
    pub fn upload(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &mut TextureCache,
        material_layout: &wgpu::BindGroupLayout,
        batches: &[PropMeshBatch],
        skip_models: &[String],
        level: QualityLevel,
    ) -> Self {
        let mut upload = PropUpload::new(skip_models, level);
        for batch in batches {
            upload.push_batch(device, queue, cache, material_layout, batch);
        }
        upload.finish()
    }

    /// Uploads one batch's vertices and indices.
    fn upload_batch(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batch: &PropMeshBatch,
    ) -> PropChunk {
        let vertices: Vec<WorldVertex> = batch
            .vertices
            .iter()
            .map(|vertex| WorldVertex::from(*vertex))
            .collect();
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-wgpu-prop-vertices"),
            size: (vertices.len() as u64)
                .saturating_mul(WORLD_VERTEX_STRIDE)
                .max(4),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        if !vertices.is_empty() {
            queue.write_buffer(&vertex_buffer, 0, bytemuck::cast_slice(&vertices));
        }
        let index_buffer =
            super::world::upload_index_buffer(device, &batch.indices, "places-wgpu-prop-indices");
        PropChunk {
            vertex_buffer,
            index_buffer,
            vertex_count: u32::try_from(batch.vertices.len()).unwrap_or(u32::MAX),
            index_count: u32::try_from(batch.indices.len()).unwrap_or(u32::MAX),
        }
    }

    /// The uploaded draws, in draw order.
    #[must_use]
    pub fn draws(&self) -> &[PropDraw] {
        &self.draws
    }

    /// The upload counters.
    #[must_use]
    pub const fn stats(&self) -> PropGpuStats {
        self.stats
    }

    /// The material of one draw.
    #[must_use]
    pub fn material(&self, slot: usize) -> Option<&GpuMaterial> {
        self.materials.get(slot)
    }

    /// The texture of one draw.
    #[must_use]
    pub fn texture(&self, slot: usize) -> Option<&GpuTexture> {
        self.textures.get(slot).map(Arc::as_ref)
    }

    /// The vertex/index buffers of one chunk.
    #[must_use]
    pub fn chunk(&self, slot: usize) -> Option<&PropChunk> {
        self.chunks.get(slot)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]

    use super::*;
    use crate::render::common::mesh::Vertex;

    #[test]
    fn an_empty_batch_list_uploads_nothing() {
        let stats = PropGpuStats::default();
        assert_eq!(stats.batches, 0);
        assert_eq!(stats.draws, 0);
    }

    #[test]
    fn the_world_vertex_carries_the_prop_vertex_field_for_field() {
        // The prop path reuses the world vertex layout; the lightmap fields are
        // the neutral builder's `UNLIT` sentinel, so the shader keeps the
        // per-vertex lit colour.
        let vertex = Vertex {
            pos: [1.0, 2.0, 3.0],
            color: [0.5, 0.25, 0.75, 1.0],
            uv: [0.2, 0.4],
            ..Vertex::UNLIT
        };
        let world = WorldVertex::from(&vertex);
        assert_eq!(world.position, [1.0, 2.0, 3.0]);
        assert_eq!(world.uv, [0.2, 0.4]);
        assert_eq!(
            world.lightmap_page,
            f32::from(crate::render::common::mesh::LIGHTMAP_NONE)
        );
        assert_eq!(world.lightmap_uv, [0, 0]);
        assert_eq!(world.color[3], 255);
    }
}
