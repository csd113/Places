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
//! * one draw per submesh, culled by its indexed primitive bounds for opaque
//!   and cutout; blended draws retain batch bounds and sorting centres.
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

/// Accumulates one world's prop resources in draw order across frame budgets.
///
/// The caller owns the batch cursor and retains this builder until every batch
/// has been visited. Material identities persist across calls, so splitting the
/// upload cannot change deduplication, ordering, or accounting.
type PropMaterialIdentity = (
    usize,
    [f32; 3],
    Option<usize>,
    crate::engine::materials::MaterialAlpha,
    crate::engine::materials::MaterialResponse,
    Option<u32>,
);

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
                gpu.uv_visibility = visibility(vertex);
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
        if batch.indices.is_empty() || self.skip_models.iter().any(|model| model == &batch.model) {
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
            let texture = prop_texture_slot(submesh.texture, texture_base, batch.textures.len(), 0);
            let mask = submesh
                .emission
                .mask
                .and_then(|index| {
                    usize::try_from(index)
                        .ok()
                        .and_then(|local_slot| local_slot.checked_add(texture_base))
                })
                .filter(|index| *index < textures.len());
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
            let record = EmissionRecord::material(submesh.emission, mask.is_some());
            // The cutoff is part of the material identity: two primitives that
            // share a sheet and emission but discard at different alpha levels
            // need different uniforms.
            let controller = record
                .is_emissive()
                .then_some(batch.light_control)
                .flatten();
            let identity = (
                texture,
                record.color,
                mask,
                submesh.alpha,
                submesh.response,
                controller,
            );
            let material = identities
                .iter()
                .position(|existing| *existing == identity)
                .unwrap_or_else(|| {
                    let mask_texture = mask
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
    #![allow(
        clippy::float_cmp,
        reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
    )]

    use super::*;
    use crate::engine::render::common::mesh::Vertex;

    #[test]
    fn primitive_bounds_exclude_other_materials_and_count_shared_indices() {
        let batch = PropMeshBatch {
            model: String::new(),
            light_control: None,
            casts_static_lighting: true,
            pending_probe_lighting: false,
            textures: Vec::new(),
            submeshes: Vec::new(),
            source_primitives: Vec::new(),
            vertices: vec![
                Vertex::new([-1.0, 0.0, -0.02], [1.0; 4], [0.0; 2]),
                Vertex::new([1.0, 0.0, -1.0], [1.0; 4], [0.0; 2]),
                Vertex::new([0.0, 1.0, -1.0], [1.0; 4], [0.0; 2]),
                Vertex::new([100.0, 0.0, 0.0], [1.0; 4], [0.0; 2]),
            ],
            indices: vec![0, 1, 2, 0, 2, 1],
            bounds: Aabb {
                min: [-1.0, 0.0, -1.0],
                max: [100.0, 1.0, 0.0],
            },
        };
        let result = prop_draw_geometry(&batch, 0, 6);
        let (bounds, vertices) = result.unwrap_or((Aabb::EMPTY, 0));
        assert_eq!(vertices, 3);
        assert!(
            bounds.min.first().is_some_and(|v| *v < -1.0)
                && bounds.max.first().is_some_and(|v| *v < 1.001),
            "a numeric guard must not include the unrelated distant material"
        );
        assert!(
            bounds.max.get(2).is_some_and(|v| *v > -0.02),
            "near-plane rounding stays conservative"
        );
        let (_, frustum) =
            crate::engine::render::RenderCamera::new(glam::Vec3::ZERO, 0.0, 0.0, 60.0)
                .view_projection(crate::engine::render::common::view::DrawableSize::new(
                    640, 360,
                ));
        assert!(
            result.is_some_and(|(visible_bounds, _)| frustum.intersects_aabb(&visible_bounds)),
            "near-plane-crossing triangles stay visible"
        );
        assert_eq!(prop_draw_geometry(&batch, u32::MAX, 3), None);
        assert_eq!(prop_draw_geometry(&batch, 0, 0), None);
    }

    #[test]
    fn an_empty_batch_list_uploads_nothing() {
        let stats = PropGpuStats::default();
        assert_eq!(stats.batches, 0);
        assert_eq!(stats.draws, 0);
    }

    /// Three vertices protect skipped ray work and mixed-material UV
    /// coverage without a device, texture decode, transport build or map bake.
    #[test]
    fn ordinary_props_skip_uv_queries_and_mixed_props_preserve_shared_vertices()
    -> Result<(), String> {
        let part = crate::engine::render::common::props::PropSubmeshBatch {
            response: crate::engine::materials::MaterialResponse::default(),
            texture: None,
            emission: crate::engine::materials::MaterialEmission::default(),
            alpha: crate::engine::materials::MaterialAlpha::default(),
            first_index: 0,
            index_count: 3,
        };
        let mut batch = PropMeshBatch {
            model: String::new(),
            light_control: None,
            casts_static_lighting: true,
            pending_probe_lighting: false,
            textures: Vec::new(),
            submeshes: vec![part.clone(), part],
            source_primitives: Vec::new(),
            vertices: vec![Vertex::UNLIT; 3],
            indices: vec![0, 1, 2, 0, 1, 0],
            bounds: Aabb::EMPTY,
        };
        let mut queries = 0_usize;
        let ordinary = pack_prop_vertices(&batch, |_| {
            queries = queries.saturating_add(1);
            [0.25; 4]
        });
        assert_eq!(queries, 0, "nonreactive vertices never trace UV rays");
        assert!(
            ordinary
                .iter()
                .all(|vertex| vertex.uv_visibility == [0.0; 4])
        );
        let reactive = batch.submeshes.get_mut(1).ok_or("mixed primitive")?;
        reactive.first_index = 3;
        reactive.response.fluorescence.strength = 1.0;
        let mixed = pack_prop_vertices(&batch, |_| {
            queries = queries.saturating_add(1);
            [0.25; 4]
        });
        assert_eq!(queries, 2, "each shared vertex keeps its UV visibility");
        for (index, ((before, after), source)) in
            ordinary.iter().zip(&mixed).zip(&batch.vertices).enumerate()
        {
            assert_eq!(before.position, source.pos);
            assert_eq!(after.position, before.position);
            assert_eq!(after.color, before.color);
            assert_eq!(
                after.uv_visibility,
                if index < 2 { [0.25; 4] } else { [0.0; 4] }
            );
        }
        batch
            .submeshes
            .get_mut(1)
            .ok_or("mixed primitive")?
            .first_index = u32::MAX;
        let fallback = pack_prop_vertices(&batch, |_| [0.25; 4]);
        assert!(
            fallback
                .iter()
                .all(|vertex| vertex.uv_visibility == [0.25; 4]),
            "invalid draw metadata keeps conservative coverage"
        );
        Ok(())
    }

    #[test]
    fn untextured_snow_does_not_sample_the_canonical_models_atlas() {
        // Slots 5 and 6 contain the canonical model's images; slot 7 is white.
        assert_eq!(prop_texture_slot(None, 5, 2, 7), 7);
        assert_eq!(prop_texture_slot(Some(0), 5, 2, 7), 5);
        assert_eq!(prop_texture_slot(Some(1), 5, 2, 7), 6);
        assert_eq!(prop_texture_slot(None, 5, 0, 5), 5);
    }

    #[test]
    fn invalid_prop_texture_indices_cannot_sample_another_models_sheet() {
        assert_eq!(prop_texture_slot(Some(2), 5, 2, 7), 7);
        assert_eq!(prop_texture_slot(Some(1), usize::MAX, 2, 7), 7);
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
            f32::from(crate::engine::render::common::mesh::LIGHTMAP_NONE)
        );
        assert_eq!(world.lightmap_uv, [0, 0]);
        assert_eq!(world.color[3], 1.0);
    }
}
