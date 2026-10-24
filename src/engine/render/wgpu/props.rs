//! Props: the neutral per-model batches as wgpu buffers and materials.
//!
//! The renderer-neutral build (`render::common::props`) has already resolved
//! every placed prop into world-space geometry with prepared lightmap
//! coordinates or vertex lighting, one batch per `(model, spatial cell)` with
//! one submesh per glTF primitive. This module uploads exactly that:
//!
//! * one 16-bit-indexable vertex/index buffer pair per batch;
//! * one clamped GPU sheet per model texture, cached through the texture cache
//!   under the bounded `Catalog` retention policy;
//! * GPU materials carrying each primitive's alpha, emission and scalar
//!   surface/fluorescence response;
//! * one draw per submesh, culled by its indexed primitive bounds for opaque
//!   and cutout; blended draws retain batch bounds and sorting centres.
//!
//! Each primitive's authored alpha mode selects the opaque, cut-out or sorted
//! translucent pass through `BatchPass::of`. The shared world shader consumes
//! the prepared lighting and material response. Buffers, textures and materials
//! are prepared at installation; light controls and emission animations update
//! existing material uniforms.

use std::sync::Arc;

use super::material::{EmissionRecord, GpuMaterial};
use super::texture::{CacheOutcome, GpuTexture, TextureCache};
use super::world::{WORLD_VERTEX_STRIDE, WorldVertex};
use crate::engine::materials::TextureOrigin;
use crate::engine::quality::{QualityLevel, TextureClass};
use crate::engine::render::common::materials::BatchPass;
use crate::engine::render::common::props::PropMeshBatch;
use crate::engine::spatial::Aabb;

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
    /// Bounds of this indexed primitive across placed instances.
    /// Blended draws retain the whole batch bounds for stable sorting.
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
    emission_controls: Vec<PropEmissionControl>,
    animation_seconds: f32,
}

struct PropEmissionControl {
    controller: u32,
    material: usize,
    enabled: bool,
    scale: f32,
    animation: Option<crate::engine::render::common::animation::EmissionAnimation>,
    applied: f32,
}

/// Resolved resource slots and uniform values shared by one prop material.
#[derive(Debug, PartialEq)]
struct PropMaterialIdentity {
    texture: usize,
    emission: EmissionRecord,
    emission_mask: Option<usize>,
    fluorescence_mask: Option<usize>,
    alpha: crate::engine::materials::MaterialAlpha,
    response: crate::engine::materials::MaterialResponse,
    controller: Option<u32>,
}

impl PropMaterialIdentity {
    fn new(
        submesh: &crate::engine::render::common::props::PropSubmeshBatch,
        texture_base: usize,
        texture_count: usize,
        light_control: Option<u32>,
    ) -> Self {
        let mask_slot = |slot: Option<u32>| {
            slot.and_then(|index| usize::try_from(index).ok())
                .filter(|index| *index < texture_count)
                .and_then(|index| texture_base.checked_add(index))
        };
        let emission_mask = mask_slot(submesh.emission.mask);
        let emission = EmissionRecord::material(submesh.emission, emission_mask.is_some());
        Self {
            texture: prop_texture_slot(submesh.texture, texture_base, texture_count, 0),
            emission,
            emission_mask,
            // Local model mask indices are not identities across batches. Even
            // an untextured primitive must keep its own resolved mask binding.
            fluorescence_mask: mask_slot(submesh.response.fluorescence.mask),
            alpha: submesh.alpha,
            response: submesh.response,
            controller: emission.is_emissive().then_some(light_control).flatten(),
        }
    }
}

/// Accumulates one world's prop resources in draw order across frame budgets.
///
/// The caller owns the batch cursor and retains this builder until every batch
/// has been visited. Material identities persist across calls, so splitting the
/// upload cannot change deduplication, ordering, or accounting.
pub struct PropUpload {
    props: WgpuProps,
    identities: Vec<PropMaterialIdentity>,
    skip_models: Vec<String>,
    image_keys: std::collections::HashMap<(String, usize), ImageIdentity>,
    level: QualityLevel,
    lighting_level: QualityLevel,
}

/// Kept only while an upload is assembled; cloned batches share the same image.
struct ImageIdentity {
    image: Arc<crate::engine::loader::RawImage>,
    key: String,
}

fn prop_texture_slot(
    texture: Option<u16>,
    texture_base: usize,
    texture_count: usize,
    fallback: usize,
) -> usize {
    texture
        .map(usize::from)
        .filter(|index| *index < texture_count)
        .and_then(|index| texture_base.checked_add(index))
        .unwrap_or(fallback)
}

/// Visibility is consumed only by fluorescent primitives. OR their referenced
/// vertices so shared indices retain coverage, independently of lighting quality.
fn pack_prop_vertices(
    batch: &PropMeshBatch,
    mut visibility: impl FnMut(&crate::engine::render::Vertex) -> [f32; 4],
) -> Vec<WorldVertex> {
    let mut receivers = Vec::new();
    for part in batch
        .submeshes
        .iter()
        .filter(|part| part.response.fluorescence.strength > 0.0)
    {
        if receivers.is_empty() {
            receivers.resize(batch.vertices.len(), false);
        }
        let referenced = usize::try_from(part.first_index).ok().and_then(|start| {
            let end = start.checked_add(usize::try_from(part.index_count).ok()?)?;
            batch.indices.get(start..end)
        });
        let Some(indices) = referenced else {
            // Invalid draw metadata retains the previous conservative coverage.
            receivers.fill(true);
            break;
        };
        for index in indices {
            let Some(receiver) = receivers.get_mut(usize::from(*index)) else {
                receivers.fill(true);
                break;
            };
            *receiver = true;
        }
    }
    batch
        .vertices
        .iter()
        .enumerate()
        .map(|(index, vertex)| {
            let mut gpu = WorldVertex::from(vertex);
            if receivers.get(index).copied().unwrap_or(false) {
                gpu.uv_visibility = visibility(&vertex);
            }
            gpu
        })
        .collect()
}

/// Bounds and distinct referenced vertices of one immutable primitive range.
/// Invalid metadata falls back to the whole batch rather than hiding geometry.
fn prop_draw_geometry(batch: &PropMeshBatch, first: u32, count: u32) -> Option<(Aabb, u32)> {
    let start = usize::try_from(first).ok()?;
    let end = start.checked_add(usize::try_from(count).ok()?)?;
    let indices = batch.indices.get(start..end)?;
    let mut bounds = Aabb::EMPTY;
    let mut seen = vec![false; batch.vertices.len()];
    let mut vertices = 0u32;
    for index in indices {
        let slot = usize::from(*index);
        let vertex = batch.vertices.get(slot)?;
        if !vertex.pos.iter().all(|value| value.is_finite()) {
            return None;
        }
        bounds.expand(vertex.pos);
        let visited = seen.get_mut(slot)?;
        if !*visited {
            vertices = vertices.saturating_add(1);
            *visited = true;
        }
    }
    if bounds.is_empty() {
        return None;
    }
    // Match the conservative guard used for CPU/GPU camera matrix rounding.
    for (min, max) in bounds.min.iter_mut().zip(bounds.max.iter_mut()) {
        let margin = min.abs().max(max.abs()).mul_add(8.0 * f32::EPSILON, 1.0e-4);
        *min -= margin;
        *max += margin;
    }
    Some((bounds, vertices))
}

impl PropUpload {
    pub(crate) fn new(
        skip_models: &[String],
        level: QualityLevel,
        lighting_level: QualityLevel,
    ) -> Self {
        Self {
            props: WgpuProps {
                chunks: Vec::new(),
                draws: Vec::new(),
                materials: Vec::new(),
                textures: Vec::new(),
                stats: PropGpuStats::default(),
                emission_controls: Vec::new(),
                animation_seconds: 0.0,
            },
            identities: Vec::new(),
            skip_models: skip_models.to_vec(),
            image_keys: std::collections::HashMap::new(),
            level,
            lighting_level,
        }
    }

    /// Visits one batch, including skipped and empty batches. The caller may
    /// stop between calls without exposing a partially installed world.
    #[expect(
        clippy::too_many_lines,
        reason = "one batch shares sheet slots, material identities and counters"
    )] // one batch shares sheet slots, material identities and counters
    pub(crate) fn push_batch_with_blacklight(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &mut TextureCache,
        material_layout: &wgpu::BindGroupLayout,
        batch: &PropMeshBatch,
        visibility: Option<crate::engine::render::common::blacklight::BlacklightVisibility<'_>>,
    ) {
        if batch.indices.is_empty()
            || self.skip_models.iter().any(|model| model == &batch.model)
            || super::diagnostic_model_hidden(&batch.model)
        {
            return;
        }
        let WgpuProps {
            stats,
            chunks,
            draws,
            materials,
            textures,
            emission_controls,
            animation_seconds: _,
        } = &mut self.props;
        let identities = &mut self.identities;
        let level = self.level;
        let lighting_level = self.lighting_level;
        stats.batches = stats.batches.saturating_add(1);
        // Slot zero is shared white for untextured primitives. A snow cap
        // must not sample another primitive's foliage, bark or wood atlas.
        if textures.is_empty() {
            textures.push(cache.fallback());
        }
        // The model's sheets, once per texture list entry.
        let texture_base = textures.len();
        for (index, image) in batch.textures.iter().enumerate() {
            if super::material::ultraviolet_only_texture(
                index,
                batch.submeshes.iter().map(|part| {
                    (
                        part.texture,
                        part.emission.mask,
                        part.response.fluorescence.mask,
                    )
                }),
            ) {
                textures.push(cache.fallback());
                continue;
            }
            let identity = self
                .image_keys
                .entry((batch.model.clone(), index))
                .or_insert_with(|| ImageIdentity {
                    image: Arc::clone(image),
                    key: super::texture::model_image_key(image),
                });
            if !Arc::ptr_eq(&identity.image, image) {
                identity.image = Arc::clone(image);
                identity.key = super::texture::model_image_key(image);
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
        let chunk = WgpuProps::upload_batch(device, queue, batch, visibility);
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
            let identity = PropMaterialIdentity::new(
                submesh,
                texture_base,
                batch.textures.len(),
                batch.light_control,
            );
            let texture = identity.texture;
            let uv_mask = if let Some(image) =
                submesh.response.fluorescence.mask.and_then(|index| {
                    batch
                        .textures
                        .get(usize::try_from(index).unwrap_or(usize::MAX))
                }) {
                super::material::upload_uv_mask(
                    device,
                    queue,
                    cache,
                    &format!("prop:{}:uv", batch.model),
                    image,
                    level,
                )
            } else {
                cache.fallback()
            };
            let record = identity.emission;
            // The cutoff is part of the material identity: two primitives that
            // share a sheet and emission but discard at different alpha levels
            // need different uniforms.
            let controller = identity.controller;
            let material = identities
                .iter()
                .position(|existing| *existing == identity)
                .unwrap_or_else(|| {
                    let mask_texture = identity
                        .emission_mask
                        .and_then(|index| textures.get(index).cloned())
                        .unwrap_or_else(|| cache.fallback());
                    let gpu = GpuMaterial::model_with_fluorescence(
                        device,
                        queue,
                        material_layout,
                        cache,
                        &mask_texture,
                        &uv_mask,
                        super::material::ModelMaterial {
                            albedo_tint: [1.0; 3],
                            emission: record,
                            alpha: submesh.alpha,
                            response: submesh.response,
                            response_enabled: lighting_level.draws_surface_response(),
                        },
                    );
                    materials.push(gpu);
                    if let Some(owner) = controller {
                        emission_controls.push(PropEmissionControl {
                            controller: owner,
                            material: materials.len().saturating_sub(1),
                            enabled: true,
                            scale: 1.0,
                            animation: None,
                            applied: 1.0,
                        });
                    }
                    identities.push(identity);
                    materials.len().saturating_sub(1)
                });
            let pass = BatchPass::of(submesh.alpha);
            let (primitive_bounds, vertex_count) =
                prop_draw_geometry(batch, submesh.first_index, submesh.index_count).unwrap_or_else(
                    || {
                        (
                            batch.bounds,
                            u32::try_from(batch.vertices.len()).unwrap_or(u32::MAX),
                        )
                    },
                );
            // Preserve the existing batch-centre transparent sorting contract.
            let bounds = if pass == BatchPass::Translucent {
                batch.bounds
            } else {
                primitive_bounds
            };
            draws.push(PropDraw {
                chunk: chunk_index,
                index_start: submesh.first_index,
                index_count: submesh.index_count,
                vertex_count,
                bounds,
                texture,
                material,
                pass,
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
    /// Configure a prepared owner's already-emissive slots without changing artwork.
    pub fn configure_light_control(
        &mut self,
        queue: &wgpu::Queue,
        controller: u32,
        enabled: bool,
        scale: f32,
        animation: Option<crate::engine::render::common::animation::EmissionAnimation>,
    ) {
        for control in &mut self.emission_controls {
            if control.controller == controller {
                control.enabled = enabled;
                control.scale = scale;
                control.animation = animation;
            }
        }
        self.update_animations(queue, self.animation_seconds);
    }

    /// Switch only an owner's luminous slots; opaque hardware never gains emission.
    pub fn set_light_enabled(&mut self, queue: &wgpu::Queue, controller: u32, enabled: bool) {
        for control in &mut self.emission_controls {
            if control.controller == controller {
                control.enabled = enabled;
            }
        }
        self.update_animations(queue, self.animation_seconds);
    }

    /// Animate existing emission uniforms; unchanged materials incur no writes.
    pub fn update_animations(&mut self, queue: &wgpu::Queue, seconds: f32) {
        self.animation_seconds = seconds;
        for control in &mut self.emission_controls {
            let scale = if control.enabled {
                control.scale
                    * control
                        .animation
                        .map_or(1.0, |animation| animation.factor(seconds))
            } else {
                0.0
            };
            if scale.to_bits() != control.applied.to_bits() {
                if let Some(material) = self.materials.get(control.material) {
                    material.write_emission_scale(queue, scale);
                }
                control.applied = scale;
            }
        }
    }

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
    #[expect(
        clippy::too_many_arguments,
        reason = "upload resources plus one shared immutable visibility context"
    )]
    pub fn upload_with_blacklight(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &mut TextureCache,
        material_layout: &wgpu::BindGroupLayout,
        batches: &[PropMeshBatch],
        skip_models: &[String],
        level: QualityLevel,
        lighting_level: QualityLevel,
        visibility: Option<crate::engine::render::common::blacklight::BlacklightVisibility<'_>>,
    ) -> Self {
        let mut upload = PropUpload::new(skip_models, level, lighting_level);
        for batch in batches {
            upload.push_batch_with_blacklight(
                device,
                queue,
                cache,
                material_layout,
                batch,
                visibility,
            );
        }
        upload.finish()
    }

    /// Uploads one batch's vertices and indices.
    fn upload_batch(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        batch: &PropMeshBatch,
        visibility: Option<crate::engine::render::common::blacklight::BlacklightVisibility<'_>>,
    ) -> PropChunk {
        let vertices = pack_prop_vertices(batch, |vertex| {
            visibility.map_or([0.0; 4], |context| context.vertex(vertex))
        });
        let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("places-wgpu-prop-vertices"),
            size: u64::try_from(vertices.len())
                .unwrap_or(u64::MAX)
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

    #[cfg(feature = "visual-diagnostics")]
    pub(crate) fn geometry_buffer_bytes(&self) -> u64 {
        self.chunks.iter().fold(0_u64, |bytes, chunk| {
            bytes
                .saturating_add(chunk.vertex_buffer.size())
                .saturating_add(chunk.index_buffer.size())
        })
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
