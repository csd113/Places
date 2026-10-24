//! Bounded prop upload units. No partially assembled prop set is published.
use super::{
    CacheOutcome, ImageIdentity, PropChunk, PropMeshBatch, PropUpload, TextureCache, TextureClass,
    TextureOrigin, WORLD_VERTEX_STRIDE, WorldVertex, pack_prop_vertices, prop_draw_geometry,
};
use crate::engine::spatial::Aabb;
use std::sync::Arc;

const VERTEX_SLICE: usize = 4096;
const VISIBILITY_SLICE: usize = 256;
const INDEX_SLICE: usize = 4096;

pub(super) struct Context<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub cache: &'a mut TextureCache,
    pub material_layout: &'a wgpu::BindGroupLayout,
    pub visibility: Option<crate::engine::render::common::blacklight::BlacklightVisibility<'a>>,
    pub staged: bool,
}

#[derive(Default)]
pub(super) struct BatchCursor {
    pub texture_slots: Vec<usize>,
    texture: usize,
    mask: bool,
    uploaded: bool,
    waiting: bool,
    job: Option<super::super::texture::TextureJob>,
    pub chunk: Option<usize>,
    pub instances: std::ops::Range<u32>,
    color_offset: u32,
    vertex: usize,
    source_vertex: Option<usize>,
    source_index: Option<usize>,
    receiver_part: usize,
    receiver_index: usize,
    receivers: Option<Vec<bool>>,
    pub part: usize,
    scan_part: usize,
    prepared_bounds: Vec<(Aabb, u32)>,
    index: usize,
    seen: Vec<bool>,
    bounds: Option<Aabb>,
    vertex_count: u32,
}

impl PropUpload {
    pub(crate) fn advance_batch_with_blacklight(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &mut TextureCache,
        material_layout: &wgpu::BindGroupLayout,
        batch: &PropMeshBatch,
        visibility: Option<crate::engine::render::common::blacklight::BlacklightVisibility<'_>>,
    ) -> Result<bool, String> {
        self.advance_batch(
            &mut Context {
                device,
                queue,
                cache,
                material_layout,
                visibility,
                staged: true,
            },
            batch,
        )
    }

    pub(super) fn advance_batch(
        &mut self,
        ctx: &mut Context<'_>,
        batch: &PropMeshBatch,
    ) -> Result<bool, String> {
        if batch.indices.is_empty()
            || self.skip_models.iter().any(|model| model == &batch.model)
            || super::super::diagnostic_model_hidden(&batch.model)
        {
            return Ok(true);
        }
        let mut cursor = self.cursor.take().unwrap_or_default();
        if self.props.textures.is_empty() {
            self.props.textures.push(ctx.cache.fallback());
        }
        cursor.waiting = false;
        let began = std::time::Instant::now();
        let deadline = self.deadline.unwrap_or_else(|| {
            began
                .checked_add(std::time::Duration::from_millis(4))
                .unwrap_or(began)
        });
        loop {
            if self.advance_unit(ctx, batch, &mut cursor)? {
                return Ok(true);
            }
            if cursor.waiting || std::time::Instant::now() >= deadline {
                self.cursor = Some(cursor);
                return Ok(false);
            }
        }
    }

    fn advance_unit(
        &mut self,
        ctx: &mut Context<'_>,
        batch: &PropMeshBatch,
        cursor: &mut BatchCursor,
    ) -> Result<bool, String> {
        if cursor.texture < batch.textures.len() {
            self.advance_texture(ctx, batch, cursor)?;
            // An installed sheet flushes alone; pending CPU mips can overlap
            // independent bounded geometry/bounds work without another owner.
            if cursor.job.is_none() {
                return Ok(false);
            }
        }
        if self.advance_independent(ctx, batch, cursor) {
            cursor.waiting = false;
        } else if cursor.texture < batch.textures.len() {
            cursor.waiting = true;
        } else if let Some((bounds, vertices)) = cursor.prepared_bounds.get(cursor.part).copied() {
            self.append_part(ctx, batch, cursor, bounds, vertices);
            cursor.part = cursor.part.saturating_add(1);
            self.wrote = true;
        } else {
            return Ok(true);
        }
        Ok(false)
    }

    fn advance_independent(
        &mut self,
        ctx: &Context<'_>,
        batch: &PropMeshBatch,
        cursor: &mut BatchCursor,
    ) -> bool {
        if cursor.receiver_part < batch.submeshes.len() {
            advance_receivers(batch, cursor);
        } else if cursor.chunk.is_none() {
            self.begin_geometry(ctx, batch, cursor);
        } else if let Some(start) = cursor.source_index {
            let indices = if self
                .props
                .chunks
                .get(cursor.chunk.unwrap_or(usize::MAX))
                .is_some_and(|c| c.shared.is_some())
            {
                batch
                    .vertices
                    .shared()
                    .map_or(batch.indices.as_slice(), |shared| {
                        shared.asset.model.indices.as_slice()
                    })
            } else {
                batch.indices.as_slice()
            };
            let end = start.saturating_add(INDEX_SLICE).min(indices.len());
            if let Some(chunk) = cursor.chunk.and_then(|slot| self.props.chunks.get(slot)) {
                super::super::world::write_index_slice(
                    ctx.queue,
                    &chunk.index_buffer,
                    indices,
                    start,
                    end,
                );
            }
            cursor.source_index = (end < indices.len()).then_some(end);
            self.wrote = true;
        } else if cursor.source_vertex.is_some() {
            self.upload_source(ctx, batch, cursor);
        } else if cursor.vertex < batch.vertices.len() {
            self.upload_vertices(ctx, batch, cursor);
        } else if let Some(part) = batch.submeshes.get(cursor.scan_part) {
            let prepared = if part.index_count == 0 {
                Some((batch.bounds, 0))
            } else {
                advance_bounds(batch, cursor)
            };
            if let Some(bounds) = prepared {
                cursor.prepared_bounds.push(bounds);
                cursor.scan_part = cursor.scan_part.saturating_add(1);
                cursor.index = 0;
                cursor.bounds = None;
                cursor.vertex_count = 0;
                cursor.seen.clear();
            }
        } else {
            return false;
        }
        true
    }

    pub(crate) fn begin_step(&mut self, began: std::time::Instant) {
        self.deadline = began.checked_add(std::time::Duration::from_millis(4));
    }

    fn advance_texture(
        &mut self,
        ctx: &mut Context<'_>,
        batch: &PropMeshBatch,
        cursor: &mut BatchCursor,
    ) -> Result<(), String> {
        use super::super::texture::{TextureKey, TextureSemantic, TextureSource, TextureWrap};
        let Some(image) = batch.textures.get(cursor.texture) else {
            return Ok(());
        };
        let uv_only = super::super::material::ultraviolet_only_texture(
            cursor.texture,
            batch
                .submeshes
                .iter()
                .map(|p| (p.texture, p.emission.mask, p.response.fluorescence.mask)),
        );
        if !cursor.mask && uv_only {
            cursor.texture_slots.push(0);
            cursor.mask = true;
            return Ok(());
        }
        let reactive = batch.submeshes.iter().any(|p| {
            p.response
                .fluorescence
                .mask
                .and_then(|i| usize::try_from(i).ok())
                == Some(cursor.texture)
        });
        if cursor.mask && !reactive {
            cursor.texture = cursor.texture.saturating_add(1);
            cursor.mask = false;
            return Ok(());
        }
        let identity = self
            .image_keys
            .entry((batch.model.clone(), cursor.texture))
            .or_insert_with(|| ImageIdentity {
                image: Arc::clone(image),
                key: source_image_key(ctx.cache, image, ctx.staged),
            });
        if !Arc::ptr_eq(&identity.image, image) || identity.key.is_empty() {
            identity.image = Arc::clone(image);
            // The bounded mip worker supplies an uncached content key. Keep
            // this cursor on cheap identity lookups while geometry advances.
            identity.key = source_image_key(ctx.cache, image, ctx.staged);
        }
        let key = TextureKey {
            logical: identity.key.clone(),
            semantic: if cursor.mask {
                TextureSemantic::DataLinear
            } else {
                TextureSemantic::BaseColorDisplay
            },
            class: if cursor.mask {
                TextureClass::EmissionMask
            } else {
                TextureClass::Prop
            },
            level: self.level,
            wrap: TextureWrap::Clamp,
        };
        if let Some(job) = &cursor.job {
            if let Some(images) = job.poll()? {
                let mut uploaded = false;
                for prepared in images {
                    uploaded |= matches!(
                        ctx.cache
                            .install_prepared_image(ctx.device, ctx.queue, prepared),
                        CacheOutcome::Uploaded
                    );
                }
                self.wrote |= uploaded;
                cursor.job = None;
                cursor.uploaded = !cursor.mask && uploaded;
            }
            cursor.waiting = true;
            return Ok(());
        }
        if ctx.staged && !ctx.cache.upload_is_cached(&key, TextureOrigin::Catalog) {
            cursor.job = super::super::texture::TextureJob::start(vec![TextureSource {
                key,
                image: Arc::clone(image),
                origin: TextureOrigin::Catalog,
            }])?;
            cursor.waiting = true;
            return Ok(());
        }
        let (outcome, texture) = ctx.cache.get_or_upload_with_key(
            ctx.device,
            ctx.queue,
            key,
            image,
            TextureOrigin::Catalog,
        );
        if cursor.mask {
            cursor.texture = cursor.texture.saturating_add(1);
            cursor.mask = false;
        } else {
            self.record_texture(outcome, texture, cursor);
            cursor.mask = true;
        }
        Ok(())
    }

    fn record_texture(
        &mut self,
        outcome: CacheOutcome,
        texture: Arc<super::super::texture::GpuTexture>,
        cursor: &mut BatchCursor,
    ) {
        if cursor.uploaded || matches!(outcome, CacheOutcome::Uploaded) {
            self.props.stats.texture_uploads = self.props.stats.texture_uploads.saturating_add(1);
            self.wrote = true;
        } else {
            self.props.stats.texture_cache_hits =
                self.props.stats.texture_cache_hits.saturating_add(1);
        }
        cursor.uploaded = false;
        self.props.stats.resident_bytes =
            self.props.stats.resident_bytes.saturating_add(
                usize::try_from(texture.meta().resident_bytes).unwrap_or(usize::MAX),
            );
        let slot = self
            .props
            .textures
            .iter()
            .position(|old| Arc::ptr_eq(old, &texture))
            .unwrap_or_else(|| {
                let slot = self.props.textures.len();
                self.props.textures.push(texture);
                slot
            });
        cursor.texture_slots.push(slot);
    }

    fn begin_geometry(
        &mut self,
        ctx: &Context<'_>,
        batch: &PropMeshBatch,
        cursor: &mut BatchCursor,
    ) {
        let props = &mut self.props;
        if let Some((shared, plan)) = batch.vertices.shared().and_then(|shared| {
            self.shared_plans
                .get_mut(&super::super::prop_instances::identity(shared))
                .filter(|plan| plan.admitted(ctx.device))
                .map(|plan| (shared, plan))
        }) {
            let index = if let Some(index) = plan.chunk {
                index
            } else {
                let index = props.chunks.len();
                props.chunks.push(plan.upload(ctx.device, shared));
                plan.chunk = Some(index);
                cursor.source_vertex = Some(0);
                cursor.source_index = Some(0);
                index
            };
            if let Some(storage) = props
                .chunks
                .get_mut(index)
                .and_then(|chunk| chunk.shared.as_mut())
            {
                cursor.color_offset = storage.next_vertex;
                cursor.instances = storage.begin_append(
                    ctx.queue,
                    shared,
                    u32::try_from(batch.vertices.len()).unwrap_or(u32::MAX),
                );
            }
            cursor.chunk = Some(index);
        } else {
            let vertex_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("places-wgpu-prop-vertices"),
                size: u64::try_from(batch.vertices.len())
                    .unwrap_or(u64::MAX)
                    .saturating_mul(WORLD_VERTEX_STRIDE)
                    .max(4),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let index_buffer = super::super::world::staged_index_buffer(
                ctx.device,
                batch.indices.len(),
                "places-wgpu-prop-indices",
            );
            cursor.source_index = Some(0);
            cursor.chunk = Some(props.chunks.len());
            cursor.instances = 0..1;
            props.chunks.push(PropChunk {
                vertex_buffer,
                index_buffer,
                vertex_count: u32::try_from(batch.vertices.len()).unwrap_or(u32::MAX),
                shared: None,
            });
        }
        props.stats.batches = props.stats.batches.saturating_add(1);
        props.stats.vertices = props.stats.vertices.saturating_add(batch.vertices.len());
        props.stats.indices = props.stats.indices.saturating_add(batch.indices.len());
        self.wrote = true;
    }

    fn upload_vertices(
        &mut self,
        ctx: &Context<'_>,
        batch: &PropMeshBatch,
        cursor: &mut BatchCursor,
    ) {
        let reactive = cursor.receivers.is_some();
        let end = cursor
            .vertex
            .saturating_add(if reactive {
                VISIBILITY_SLICE
            } else {
                VERTEX_SLICE
            })
            .min(batch.vertices.len());
        let Some(chunk) = cursor.chunk.and_then(|slot| self.props.chunks.get(slot)) else {
            return;
        };
        if let Some(storage) = &chunk.shared {
            if let Some(shared) = batch.vertices.shared() {
                let colors: Vec<_> = shared.colors_range(cursor.vertex..end).collect();
                let offset = u64::from(cursor.color_offset)
                    .saturating_add(u64::try_from(cursor.vertex).unwrap_or(u64::MAX))
                    .saturating_mul(16);
                let visibility = reactive.then(|| {
                    (cursor.vertex..end)
                        .map(|index| {
                            batch
                                .vertices
                                .get(index)
                                .filter(|_| {
                                    cursor
                                        .receivers
                                        .as_ref()
                                        .and_then(|flags| flags.get(index))
                                        .copied()
                                        .unwrap_or(false)
                                })
                                .map_or([0.0; 4], |vertex| {
                                    ctx.visibility.map_or([0.0; 4], |v| v.vertex(&vertex))
                                })
                        })
                        .collect::<Vec<_>>()
                });
                storage.write_slice(ctx.queue, offset, &colors, visibility.as_deref());
            }
        } else {
            let vertices: Vec<_> = if cursor.vertex == 0 && end == batch.vertices.len() {
                pack_prop_vertices(batch, |vertex| {
                    ctx.visibility.map_or([0.0; 4], |v| v.vertex(vertex))
                })
            } else {
                (cursor.vertex..end)
                    .filter_map(|index| batch.vertices.get(index).map(|v| (index, v)))
                    .map(|(index, vertex)| {
                        let mut gpu = WorldVertex::from(vertex);
                        if cursor
                            .receivers
                            .as_ref()
                            .and_then(|flags| flags.get(index))
                            .copied()
                            .unwrap_or(false)
                        {
                            gpu.uv_visibility =
                                ctx.visibility.map_or([0.0; 4], |v| v.vertex(&vertex));
                        }
                        gpu
                    })
                    .collect()
            };
            ctx.queue.write_buffer(
                &chunk.vertex_buffer,
                u64::try_from(cursor.vertex)
                    .unwrap_or(u64::MAX)
                    .saturating_mul(WORLD_VERTEX_STRIDE),
                bytemuck::cast_slice(&vertices),
            );
        }
        cursor.vertex = end;
        self.wrote = true;
    }

    fn upload_source(
        &mut self,
        ctx: &Context<'_>,
        batch: &PropMeshBatch,
        cursor: &mut BatchCursor,
    ) {
        let Some(shared) = batch.vertices.shared() else {
            return;
        };
        let Some(chunk) = cursor.chunk.and_then(|slot| self.props.chunks.get(slot)) else {
            return;
        };
        let start = cursor.source_vertex.unwrap_or(0);
        let end = start
            .saturating_add(VERTEX_SLICE)
            .min(shared.asset.model.vertices.len());
        let vertices: Vec<_> = shared
            .asset
            .model
            .vertices
            .get(start..end)
            .unwrap_or_default()
            .iter()
            .map(|v| {
                WorldVertex::from(crate::engine::render::Vertex {
                    pos: v.pos,
                    normal: v.normal.unwrap_or([0.0; 3]),
                    uv: v.uv,
                    caustics_albedo: [v.color[0], v.color[1], v.color[2]],
                    ..crate::engine::render::Vertex::UNLIT
                })
            })
            .collect();
        ctx.queue.write_buffer(
            &chunk.vertex_buffer,
            u64::try_from(start)
                .unwrap_or(u64::MAX)
                .saturating_mul(WORLD_VERTEX_STRIDE),
            bytemuck::cast_slice(&vertices),
        );
        cursor.source_vertex = (end < shared.asset.model.vertices.len()).then_some(end);
        self.wrote = true;
    }

    pub(crate) fn take_upload_work(&mut self) -> bool {
        std::mem::take(&mut self.wrote)
    }
}

fn source_image_key(
    cache: &mut TextureCache,
    image: &Arc<crate::engine::loader::RawImage>,
    staged: bool,
) -> String {
    if staged {
        cache.cached_model_image_key(image).unwrap_or_default()
    } else {
        cache.model_image_key(image)
    }
}

fn advance_receivers(batch: &PropMeshBatch, cursor: &mut BatchCursor) {
    let Some(part) = batch.submeshes.get(cursor.receiver_part) else {
        return;
    };
    if part.response.fluorescence.strength <= 0.0 {
        cursor.receiver_part = cursor.receiver_part.saturating_add(1);
        return;
    }
    let receivers = cursor
        .receivers
        .get_or_insert_with(|| vec![false; batch.vertices.len()]);
    let start = usize::try_from(part.first_index).unwrap_or(usize::MAX);
    let count = usize::try_from(part.index_count).unwrap_or(usize::MAX);
    let end = cursor.receiver_index.saturating_add(INDEX_SLICE).min(count);
    if let Some(indices) = batch
        .indices
        .get(start.saturating_add(cursor.receiver_index)..start.saturating_add(end))
    {
        for index in indices {
            if let Some(flag) = receivers.get_mut(usize::from(*index)) {
                *flag = true;
            } else {
                receivers.fill(true);
            }
        }
    } else {
        receivers.fill(true);
    }
    cursor.receiver_index = end;
    if end == count {
        cursor.receiver_part = cursor.receiver_part.saturating_add(1);
        cursor.receiver_index = 0;
    }
}

fn advance_bounds(batch: &PropMeshBatch, cursor: &mut BatchCursor) -> Option<(Aabb, u32)> {
    let part = batch.submeshes.get(cursor.scan_part)?;
    if part.index_count <= u32::try_from(INDEX_SLICE).unwrap_or(u32::MAX) {
        return Some(
            prop_draw_geometry(batch, part.first_index, part.index_count).unwrap_or_else(|| {
                (
                    batch.bounds,
                    u32::try_from(batch.vertices.len()).unwrap_or(u32::MAX),
                )
            }),
        );
    }
    let start = usize::try_from(part.first_index).ok()?;
    let count = usize::try_from(part.index_count).ok()?;
    let end = cursor.index.saturating_add(INDEX_SLICE).min(count);
    if cursor.index == 0 {
        cursor.seen.resize(batch.vertices.len(), false);
        cursor.bounds = Some(Aabb::EMPTY);
    }
    let Some(indices) = batch
        .indices
        .get(start.saturating_add(cursor.index)..start.saturating_add(end))
    else {
        return Some((
            batch.bounds,
            u32::try_from(batch.vertices.len()).unwrap_or(u32::MAX),
        ));
    };
    for index in indices {
        let slot = usize::from(*index);
        let (Some(vertex), Some(seen)) = (batch.vertices.get(slot), cursor.seen.get_mut(slot))
        else {
            return Some((
                batch.bounds,
                u32::try_from(batch.vertices.len()).unwrap_or(u32::MAX),
            ));
        };
        if !vertex.pos.iter().all(|value| value.is_finite()) {
            return Some((
                batch.bounds,
                u32::try_from(batch.vertices.len()).unwrap_or(u32::MAX),
            ));
        }
        if !*seen {
            *seen = true;
            cursor.vertex_count = cursor.vertex_count.saturating_add(1);
            if let Some(bounds) = &mut cursor.bounds {
                bounds.expand(vertex.pos);
            }
        }
    }
    cursor.index = end;
    if end < count {
        return None;
    }
    let mut bounds = cursor.bounds.unwrap_or(batch.bounds);
    for (min, max) in bounds.min.iter_mut().zip(bounds.max.iter_mut()) {
        let margin = min.abs().max(max.abs()).mul_add(8.0 * f32::EPSILON, 1.0e-4);
        *min -= margin;
        *max += margin;
    }
    Some((bounds, cursor.vertex_count))
}

#[cfg(test)]
#[path = "../../../tests/prop_upload_bounds.rs"]
mod tests;
