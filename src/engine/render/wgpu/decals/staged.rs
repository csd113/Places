//! Worker-packed decal geometry, bounded uploads and one-sheet CPU preparation.
use super::{
    DecalChunk, DecalGpuStats, DecalSheet, QualityLevel, TextureCache, WORLD_VERTEX_STRIDE,
    WgpuDecals, WorldVertex, decal_blend_sheets, decal_sheet_is_blend, pack_decal_ranges,
};
use std::sync::Arc;

pub struct DecalUpload {
    packer: crate::engine::render::common::MeshPacker,
    value: WgpuDecals,
    chunk: Option<DecalChunk>,
    chunk_cursor: usize,
    vertex_cursor: usize,
    index_cursor: usize,
    sheet_ids: Vec<String>,
    catalog: Arc<crate::engine::loader::PropCatalog>,
    root: Option<std::path::PathBuf>,
    job: Option<super::super::texture::TextureJob>,
    prepared: Option<super::super::texture::PreparedImage>,
    diagnostics: usize,
    wrote: bool,
}

impl DecalUpload {
    /// CPU-only packing, performed by the existing preparation worker.
    pub fn prepare(
        loaded: &crate::engine::loader::LoadedLevel,
        mesh: &crate::engine::render::common::mesh::LevelMesh,
    ) -> Self {
        let (packer, mut draws) = pack_decal_ranges(mesh);
        let blends = decal_blend_sheets(&loaded.level, loaded.catalog.assets());
        for draw in &mut draws {
            draw.blend = decal_sheet_is_blend(draw.sheet, &blends);
        }
        Self {
            packer,
            value: WgpuDecals {
                chunks: Vec::new(),
                draws,
                sheets: Vec::new(),
                external_sheets: 0,
                stats: DecalGpuStats::default(),
            },
            chunk: None,
            chunk_cursor: 0,
            vertex_cursor: 0,
            index_cursor: 0,
            sheet_ids: super::decal_external_sheet_ids(&loaded.level, loaded.catalog.assets()),
            catalog: Arc::clone(&loaded.catalog),
            root: crate::engine::assets::resolve_asset_root(),
            job: None,
            prepared: None,
            diagnostics: 0,
            wrote: false,
        }
    }

    pub fn advance(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &mut TextureCache,
        quality: QualityLevel,
    ) -> Result<bool, String> {
        self.wrote = false;
        let began = std::time::Instant::now();
        loop {
            if self.chunk_cursor < self.packer.chunks.len() {
                self.advance_geometry(device, queue);
            } else {
                return self.advance_sheet(device, queue, cache, quality);
            }
            if began.elapsed() >= std::time::Duration::from_millis(4) {
                return Ok(false);
            }
        }
    }

    fn advance_geometry(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        let Some(source) = self.packer.chunks.get_mut(self.chunk_cursor) else {
            return;
        };
        if self.chunk.is_none() {
            let vertex_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("places-wgpu-decal-vertices"),
                size: u64::try_from(source.vertices.len())
                    .unwrap_or(u64::MAX)
                    .saturating_mul(WORLD_VERTEX_STRIDE)
                    .max(4),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let index_buffer = super::super::world::staged_index_buffer(
                device,
                source.indices.len(),
                "places-wgpu-decal-indices",
            );
            self.chunk = Some(DecalChunk {
                vertex_buffer,
                index_buffer,
                vertex_count: u32::try_from(source.vertices.len()).unwrap_or(u32::MAX),
                index_count: u32::try_from(source.indices.len()).unwrap_or(u32::MAX),
            });
            self.wrote = true;
            return;
        }
        if self.index_cursor < source.indices.len() {
            let end = self
                .index_cursor
                .saturating_add(4096)
                .min(source.indices.len());
            if let Some(chunk) = &self.chunk {
                super::super::world::write_index_slice(
                    queue,
                    &chunk.index_buffer,
                    &source.indices,
                    self.index_cursor,
                    end,
                );
            }
            self.index_cursor = end;
            self.wrote = true;
            return;
        }
        let end = self
            .vertex_cursor
            .saturating_add(4096)
            .min(source.vertices.len());
        if let Some(chunk) = &self.chunk {
            let vertices: Vec<_> = source
                .vertices
                .get(self.vertex_cursor..end)
                .unwrap_or_default()
                .iter()
                .map(WorldVertex::from)
                .collect();
            if !vertices.is_empty() {
                queue.write_buffer(
                    &chunk.vertex_buffer,
                    u64::try_from(self.vertex_cursor)
                        .unwrap_or(u64::MAX)
                        .saturating_mul(WORLD_VERTEX_STRIDE),
                    bytemuck::cast_slice(&vertices),
                );
                self.wrote = true;
            }
        }
        self.vertex_cursor = end;
        if end == source.vertices.len() {
            if let Some(chunk) = self.chunk.take() {
                self.value.chunks.push(chunk);
            }
            *source = crate::engine::render::common::MeshChunk::default();
            self.chunk_cursor = self.chunk_cursor.saturating_add(1);
            self.vertex_cursor = 0;
            self.index_cursor = 0;
        }
    }

    fn advance_sheet(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        cache: &mut TextureCache,
        quality: QualityLevel,
    ) -> Result<bool, String> {
        if self.value.sheets.len() > self.sheet_ids.len() {
            return Ok(true);
        }
        if let Some(image) = self.prepared.take() {
            let key = image.key().clone();
            self.diagnostics = self
                .diagnostics
                .saturating_add(usize::from(image.is_diagnostic()));
            self.wrote |= matches!(
                cache.install_prepared_image(device, queue, image),
                super::super::texture::CacheOutcome::Uploaded
            );
            let texture = cache
                .prepared_texture(&key)
                .ok_or("Prepared decal sheet was not installed")?;
            self.value.sheets.push(DecalSheet::Cached(texture));
        } else if let Some(job) = &self.job {
            if let Some(mut images) = job.poll()? {
                self.prepared = images.pop();
                self.job = None;
            }
        } else {
            let id = self
                .value
                .sheets
                .len()
                .checked_sub(1)
                .and_then(|slot| self.sheet_ids.get(slot))
                .cloned();
            self.job = super::super::texture::TextureJob::start_decal(
                Arc::clone(&self.catalog),
                self.root.clone(),
                id,
                quality,
            )?;
        }
        Ok(false)
    }

    /// Both banks belong to this exact transaction and use the same sheet order.
    pub fn reuse_sheets(&mut self, other: &WgpuDecals) {
        if !self.value.sheets.is_empty() || other.external_sheets != self.sheet_ids.len() {
            return;
        }
        let shared: Option<Vec<_>> = other
            .sheets
            .iter()
            .map(|sheet| match sheet {
                DecalSheet::Cached(texture) => Some(DecalSheet::Cached(Arc::clone(texture))),
                DecalSheet::Owned {
                    _texture: _,
                    _view: _,
                    low: _,
                    medium: _,
                    high: _,
                    resident_bytes: _,
                } => None,
            })
            .collect();
        if let Some(sheets) = shared {
            self.value.sheets = sheets;
            self.diagnostics = other.stats.diagnostic_sheets;
        }
    }

    pub const fn wrote_upload(&self) -> bool {
        self.wrote
    }

    pub fn finish(mut self) -> WgpuDecals {
        let vertices = self
            .value
            .chunks
            .iter()
            .map(|chunk| usize::try_from(chunk.vertex_count).unwrap_or(usize::MAX))
            .fold(0_usize, usize::saturating_add);
        let indices = self
            .value
            .chunks
            .iter()
            .map(|chunk| usize::try_from(chunk.index_count).unwrap_or(usize::MAX))
            .fold(0_usize, usize::saturating_add);
        self.value.external_sheets = self.sheet_ids.len();
        self.value.stats = DecalGpuStats {
            draws: self.value.draws.len(),
            chunks: self.value.chunks.len(),
            vertices,
            indices,
            vertex_bytes: vertices
                .saturating_mul(usize::try_from(WORLD_VERTEX_STRIDE).unwrap_or(usize::MAX)),
            index_bytes: indices.saturating_mul(std::mem::size_of::<u16>()),
            sheets: self.value.sheets.len(),
            external_sheets: self.sheet_ids.len(),
            diagnostic_sheets: self.diagnostics,
            texture_bytes: self
                .value
                .sheets
                .iter()
                .map(DecalSheet::resident_bytes)
                .fold(0_usize, usize::saturating_add),
        };
        self.value
    }
}
