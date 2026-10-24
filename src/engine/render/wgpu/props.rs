//! Prepared props as shared source buffers or independent fallback batches.
//!
//! The neutral interface supplies world-space geometry and independent baked
//! lighting. Exact canonical admission exposes a shared source model and its
//! placements; this module uploads that shape once, separate RGBA32F lighting,
//! transforms and required static UV visibility. Compatible material ranges
//! merge automatically while each placement keeps its own culling bounds.
//! Unsupported topology and sorted blends retain individual batch buffers.
//!
//! Model sheets stay in the existing bounded texture cache. Resolved texture,
//! material and emission-controller identity governs draw compatibility;
//! controls update existing material uniforms. Shared geometry and draw merging
//! are independent mechanisms, and reflection/emission/shadow passes use the
//! same source shape without sharing another placement's baked colours.

use std::sync::Arc;

mod staged;

use super::material::{EmissionRecord, GpuMaterial};
use super::texture::{CacheOutcome, GpuTexture, TextureCache};
use super::world::{WORLD_VERTEX_STRIDE, WorldVertex};
use crate::engine::materials::TextureOrigin;
use crate::engine::quality::{QualityLevel, TextureClass};
use crate::engine::render::common::materials::BatchPass;
use crate::engine::render::common::props::PropMeshBatch;
use crate::engine::spatial::Aabb;

/// One 16-bit-indexable source or fallback buffer pair.
pub struct PropChunk {
    /// Interleaved [`WorldVertex`] data.
    pub vertex_buffer: wgpu::Buffer,
    /// `u16` triangle indices.
    pub index_buffer: wgpu::Buffer,
    /// Distinct vertices the chunk holds.
    pub vertex_count: u32,
    pub shared: Option<super::prop_instances::SharedChunk>,
}

/// One prop submesh draw: one primitive of one model batch.
#[derive(Clone, Debug)]
pub struct PropDraw {
    /// Which [`PropChunk`].
    pub chunk: usize,
    pub instances: std::ops::Range<u32>,
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct PropGpuStats {
    pub vertex_bytes: usize,
    pub index_bytes: usize,
    pub lighting_instance_bytes: u64,
    pub cullable_draws: usize,
    pub canonical_meshes: usize,
    pub canonical_vertices: usize,
    pub canonical_instances: usize,
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
        texture_slots: &[usize],
        texture_count: usize,
        light_control: Option<u32>,
    ) -> Self {
        let mask_slot = |slot: Option<u32>| {
            slot.and_then(|index| usize::try_from(index).ok())
                .filter(|index| *index < texture_count)
                .and_then(|index| texture_slots.get(index).copied())
        };
        let emission_mask = mask_slot(submesh.emission.mask);
        let emission = EmissionRecord::material(submesh.emission, emission_mask.is_some());
        Self {
            texture: submesh
                .texture
                .and_then(|index| texture_slots.get(usize::from(index)).copied())
                .unwrap_or(0),
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
    cursor: Option<staged::BatchCursor>,
    wrote: bool,
    deadline: Option<std::time::Instant>,
    props: WgpuProps,
    identities: Vec<PropMaterialIdentity>,
    skip_models: Vec<String>,
    image_keys: std::collections::HashMap<(String, usize), ImageIdentity>,
    level: QualityLevel,
    lighting_level: QualityLevel,
    shared_plans: std::collections::HashMap<
        *const crate::engine::props::LoadedPropAsset,
        super::prop_instances::Plan,
    >,
}

/// Kept only while an upload is assembled; cloned batches share the same image.
struct ImageIdentity {
    image: Arc<crate::engine::loader::RawImage>,
    key: String,
}

/// Visibility is consumed only by fluorescent primitives. OR their referenced
/// vertices so shared indices retain coverage, independently of lighting quality.
pub(super) fn pack_prop_vertices(
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
        batches: &[PropMeshBatch],
    ) -> Self {
        let mut shared_plans = std::collections::HashMap::<_, super::prop_instances::Plan>::new();
        for batch in batches {
            if let Some(shared) = batch.vertices.shared() {
                let plan = shared_plans
                    .entry(super::prop_instances::identity(shared))
                    .or_default();
                plan.vertices = plan
                    .vertices
                    .saturating_add(u64::try_from(batch.vertices.len()).unwrap_or(u64::MAX));
                plan.instances = plan
                    .instances
                    .saturating_add(u64::try_from(shared.placements.len()).unwrap_or(u64::MAX));
                plan.reactive |= batch
                    .submeshes
                    .iter()
                    .any(|s| s.response.fluorescence.strength > 0.0);
            }
        }
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
            shared_plans,
            cursor: None,
            wrote: false,
            deadline: None,
        }
    }

    /// Immediate compiler/tool seam; runtime installation uses bounded advances.
    pub(crate) fn push_batch_with_blacklight(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &mut TextureCache,
        material_layout: &wgpu::BindGroupLayout,
        batch: &PropMeshBatch,
        visibility: Option<crate::engine::render::common::blacklight::BlacklightVisibility<'_>>,
    ) {
        let mut ctx = staged::Context {
            device,
            queue,
            cache,
            material_layout,
            visibility,
            staged: false,
        };
        loop {
            match self.advance_batch(&mut ctx, batch) {
                Ok(true) => break,
                Ok(false) => {}
                Err(error) => {
                    crate::engine::logging::warn(error);
                    break;
                }
            }
        }
    }

    fn append_part(
        &mut self,
        ctx: &mut staged::Context<'_>,
        batch: &PropMeshBatch,
        cursor: &staged::BatchCursor,
        primitive_bounds: Aabb,
        vertex_count: u32,
    ) {
        let part_slot = cursor.part;
        let Some(submesh) = batch.submeshes.get(part_slot) else {
            return;
        };
        let texture_slots = &cursor.texture_slots;
        let chunk_index = cursor.chunk.unwrap_or(usize::MAX);
        let instances = cursor.instances.clone();
        let is_shared = self
            .props
            .chunks
            .get(chunk_index)
            .is_some_and(|chunk| chunk.shared.is_some());
        let source_parts = is_shared
            .then(|| batch.vertices.shared())
            .flatten()
            .map(|shared| &shared.asset.model.submeshes);
        if submesh.index_count == 0 {
            return;
        }
        let (texture, material, emissive) = self.part_material(ctx, batch, submesh, texture_slots);
        let pass = BatchPass::of(submesh.alpha);
        // Preserve the existing batch-centre transparent sorting contract.
        let bounds = if pass == BatchPass::Translucent {
            batch.bounds
        } else {
            primitive_bounds
        };
        let (index_start, index_count, draw_vertex_count) =
            source_parts.and_then(|parts| parts.get(part_slot)).map_or(
                (submesh.first_index, submesh.index_count, vertex_count),
                |part| {
                    (
                        part.first_index,
                        part.index_count,
                        vertex_count
                            .checked_div(instances.end.saturating_sub(instances.start))
                            .unwrap_or(vertex_count),
                    )
                },
            );
        self.props.draws.push(PropDraw {
            chunk: chunk_index,
            instances,
            index_start,
            index_count,
            vertex_count: draw_vertex_count,
            bounds,
            texture,
            material,
            pass,
            emissive,
        });
    }

    fn part_material(
        &mut self,
        ctx: &mut staged::Context<'_>,
        batch: &PropMeshBatch,
        submesh: &crate::engine::render::common::props::PropSubmeshBatch,
        texture_slots: &[usize],
    ) -> (usize, usize, bool) {
        let device = ctx.device;
        let queue = ctx.queue;
        let cache = &mut *ctx.cache;
        let material_layout = ctx.material_layout;
        let level = self.level;
        let lighting_level = self.lighting_level;
        let WgpuProps {
            stats: _,
            chunks: _,
            draws: _,
            materials,
            textures,
            emission_controls,
            animation_seconds: _,
        } = &mut self.props;
        let identities = &mut self.identities;
        let identity = PropMaterialIdentity::new(
            submesh,
            texture_slots,
            batch.textures.len(),
            batch.light_control,
        );
        let texture = identity.texture;
        let uv_mask = if let Some(image) = submesh.response.fluorescence.mask.and_then(|index| {
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
        (texture, material, record.is_emissive())
    }

    /// Consumes the completed accumulator; no GPU submission or wait occurs here.
    pub(crate) fn finish(mut self) -> WgpuProps {
        // Shared opaque/cutout draws can be grouped across cells. Individual
        // instance bounds still decide every surviving contiguous run at encode.
        let mut shared = Vec::new();
        self.props.draws.retain(|draw| {
            if self
                .props
                .chunks
                .get(draw.chunk)
                .is_some_and(|chunk| chunk.shared.is_some())
            {
                shared.push(draw.clone());
                false
            } else {
                true
            }
        });
        shared.sort_by_key(|draw| {
            (
                draw.chunk,
                draw.material,
                draw.texture,
                draw.index_start,
                draw.index_count,
                draw.instances.start,
            )
        });
        let mut merged: Vec<PropDraw> = Vec::new();
        for draw in shared {
            let previous_end = merged.last().map(|previous| previous.instances.end);
            let consecutive = previous_end == Some(draw.instances.start);
            if let Some(last) = merged.last_mut()
                && last.chunk == draw.chunk
                && last.material == draw.material
                && last.texture == draw.texture
                && last.index_start == draw.index_start
                && last.index_count == draw.index_count
                && last.pass == draw.pass
                && last.emissive == draw.emissive
                && consecutive
            {
                last.instances.end = draw.instances.end;
                last.bounds = last.bounds.union(&draw.bounds);
            } else {
                merged.push(draw);
            }
        }
        if std::env::var("PLACES_BENCH").as_deref() == Ok("1")
            && std::env::var("PLACES_BENCH_PROP_MERGE").as_deref() == Ok("0")
        {
            // Mechanism control: keep shared buffers/lighting unchanged, but
            // submit each placement independently. No map or quality changes.
            let mut separate = Vec::new();
            for draw in merged {
                for instance in draw.instances.clone() {
                    let mut single = draw.clone();
                    single.instances = instance..instance.saturating_add(1);
                    separate.push(single);
                }
            }
            self.props.draws.extend(separate);
        } else {
            self.props.draws.extend(merged);
        }
        self.props.update_storage_stats();
        self.props
    }
}

impl WgpuProps {
    fn update_storage_stats(&mut self) {
        for chunk in &self.chunks {
            self.stats.vertex_bytes = self
                .stats
                .vertex_bytes
                .saturating_add(usize::try_from(chunk.vertex_buffer.size()).unwrap_or(usize::MAX));
            self.stats.index_bytes = self
                .stats
                .index_bytes
                .saturating_add(usize::try_from(chunk.index_buffer.size()).unwrap_or(usize::MAX));
            if let Some(canonical) = &chunk.shared {
                self.stats.canonical_vertices = self
                    .stats
                    .canonical_vertices
                    .saturating_add(usize::try_from(chunk.vertex_count).unwrap_or(usize::MAX));
                self.stats.canonical_meshes = self.stats.canonical_meshes.saturating_add(1);
                self.stats.canonical_instances = self
                    .stats
                    .canonical_instances
                    .saturating_add(canonical.bounds.len());
                self.stats.lighting_instance_bytes = self
                    .stats
                    .lighting_instance_bytes
                    .saturating_add(canonical.bytes());
            }
        }
        self.stats.chunks = self.chunks.len();
        self.stats.draws = self.draws.len();
        self.stats.cullable_draws = self.draws.iter().fold(0_usize, |count, draw| {
            count.saturating_add(
                usize::try_from(draw.instances.end.saturating_sub(draw.instances.start))
                    .unwrap_or(usize::MAX),
            )
        });
        crate::engine::logging::info(format_args!(
            "[prop-storage] {}",
            serde_json::to_string(&self.stats).unwrap_or_default()
        ));
    }

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
        let mut upload = PropUpload::new(skip_models, level, lighting_level, batches);
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

    #[cfg(feature = "visual-diagnostics")]
    pub(crate) fn geometry_buffer_bytes(&self) -> u64 {
        self.chunks.iter().fold(0_u64, |bytes, chunk| {
            bytes
                .saturating_add(chunk.vertex_buffer.size())
                .saturating_add(chunk.index_buffer.size())
                .saturating_add(
                    chunk
                        .shared
                        .as_ref()
                        .map_or(0, super::prop_instances::SharedChunk::bytes),
                )
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
