//! Binary codec for the prepared static prop batches.
//!
//! Prop vertices are pre-transformed and pre-lit by the compiler; model
//! textures are *not* embedded here — the batch records the model path and the
//! per-submesh texture slots, and the player reattaches the decoded texture
//! images from the same catalog model it verifies by hash. This keeps one copy
//! of model artwork on disk and makes the package's dependency list exact.
//!
//! ```text
//! magic         4 bytes  "PMP1"
//! version       u16      [`PROPS_RECORD_VERSION`]
//! batch_count   u32
//! batches       batch_count x batch
//! ```
//!
//! ```text
//! batch:
//!   model       u32 length + UTF-8 bytes
//!   casts_static_lighting u8 (0 = fully claimed character model, 1 = static caster)
//!   light_control u8 presence + u32 controller identity when present
//!   bounds_min  f32 x 3
//!   bounds_max  f32 x 3
//!   submesh_count u32
//!   submeshes   submesh_count x submesh
//!   vertices    u32 count, then vertex_mode u8 and vertex storage
//!   indices     u32 count, then count x u16
//! ```
//!
//! Vertex mode 0 stores literal 81-byte neutral vertices. Modes 1 and 2 store
//! a `u32` frame count, then 28-byte normal/tangent/handedness frames. Each
//! vertex retains its 36-byte position/colour/UV prefix and 5-byte lightmap
//! tail and 12-byte neutral caustics factor, followed by a `u16` (mode 1) or
//! `u32` (mode 2) frame index. The palette
//! is used only when it is strictly smaller, with exact first-occurrence keys.
//!
//! ```text
//! submesh:
//!   texture      u8 (0 = none, 1 = Some) + u16 slot when present
//!   alpha_mode   u8 (0 = opaque, 1 = cutout, 2 = blend)
//!   alpha_cutoff f32
//!   emission     f32 x 3 colour, f32 intensity
//!   emission_mask u8 (0 = none, 1 = Some) + u16 slot when present
//!   specular     f32 x 3 (linear)
//!   roughness    f32
//!   uv_response  f32
//!   uv_mask      u8 (0 = none, 1 = Some) + u16 slot when present
//!   uv_tint      f32 x 3 (linear authored tint, independent of baked light)
//!   first_index  u32
//!   index_count  u32
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::engine::materials::{AlphaMode, MaterialAlpha, MaterialEmission};
use crate::engine::render::{PropMeshBatch, PropSubmeshBatch, Vertex};
use crate::engine::spatial::Aabb;

use super::binary::{Reader, Writer, finite3};
use super::mesh::{read_vertex, write_vertex};
use super::{MAX_BINARY_BYTES, MAX_PROP_BATCH_VERTICES, MAX_PROP_BATCHES, MAX_PROP_SUBMESHES};

/// Current pre-release props record version; superseded layouts require recompilation.
pub const PROPS_RECORD_VERSION: u16 = 1;

/// Magic identifying a props record.
pub const PROPS_MAGIC: [u8; 4] = *b"PMP1";

/// Largest accepted model path length in a prop batch.
const MAX_MODEL_PATH: u64 = 1024;

/// Width of the neutral vertex written by the existing mesh codec.
const SERIALIZED_VERTEX_BYTES: usize = 81;

const VERTEX_PREFIX_BYTES: usize = 36;
const FRAME_BYTES: usize = 28;
const FRAME_END: usize = 64;
const VERTEX_LITERAL_BYTES: usize = 53;
const VERTEX_MODE_LITERAL: u8 = 0;

/// Retain the historical literal record's allocation ceiling even when the
/// encoded frame palette is smaller. All charges use the actual legacy-width
/// headers plus the version's literal width per vertex and two bytes per index.
struct LogicalPropsBudget {
    bytes: u64,
}

const fn serialized_vertex_bytes() -> usize {
    SERIALIZED_VERTEX_BYTES
}

const fn vertex_literal_bytes() -> usize {
    VERTEX_LITERAL_BYTES
}

impl LogicalPropsBudget {
    const fn new() -> Self {
        Self { bytes: 10 }
    }

    fn add_bytes(&mut self, bytes: u64) -> Result<(), String> {
        let next = self
            .bytes
            .checked_add(bytes)
            .ok_or_else(|| "prop logical literal storage size overflows".to_string())?;
        if next > MAX_BINARY_BYTES {
            return Err(format!(
                "prop logical literal storage would hold {next} bytes (limit {MAX_BINARY_BYTES})"
            ));
        }
        self.bytes = next;
        Ok(())
    }

    fn add_slots(&mut self, count: usize, width: u64, overhead: u64) -> Result<(), String> {
        let slot_count = u64::try_from(count)
            .map_err(|error| format!("prop logical slot count is too large: {error}"))?;
        let bytes = slot_count
            .checked_mul(width)
            .and_then(|run| run.checked_add(overhead))
            .ok_or_else(|| "prop logical literal storage size overflows".to_string())?;
        self.add_bytes(bytes)
    }
}

fn check_encoded_size(bytes: usize) -> Result<(), String> {
    let encoded_bytes = u64::try_from(bytes)
        .map_err(|error| format!("prop encoded record size is too large: {error}"))?;
    if encoded_bytes > MAX_BINARY_BYTES {
        return Err(format!(
            "prop encoded record would hold {encoded_bytes} bytes (limit {MAX_BINARY_BYTES})"
        ));
    }
    Ok(())
}

/// Counts from lossless, within-batch prop vertex interning.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropsEncodingStats {
    /// Number of input vertex slots across all batches, including unreferenced slots.
    pub input_vertex_slots: usize,
    /// Number of distinct serialized vertex slots retained across all batches.
    pub stored_vertex_slots: usize,
    /// Number of byte-identical vertex slots removed within their own batch.
    pub duplicate_slots: usize,
    /// Number of encoded batches.
    pub batch_count: usize,
    /// Number of ordered indices retained across all batches.
    pub index_count: usize,
}

/// Encodes prepared prop batches, omitting texture pixels by design.
///
/// # Errors
/// Returns an error when the input is malformed, out of bounds or unsupported.
pub fn write_props(batches: &[PropMeshBatch]) -> Result<Vec<u8>, String> {
    write_props_with_stats(batches).map(|(bytes, _)| bytes)
}

/// Encodes prop batches and reports exact vertex slot savings.
///
/// Only vertices whose complete 81-byte records match are shared. First
/// occurrence order, all ordered indices and all batch metadata are preserved;
/// distinct shading, material UVs and lightmap coordinates retain their seams.
///
/// # Errors
/// Returns an error when the input is malformed, out of bounds or unsupported.
pub fn write_props_with_stats(
    batches: &[PropMeshBatch],
) -> Result<(Vec<u8>, PropsEncodingStats), String> {
    let count =
        u32::try_from(batches.len()).map_err(|error| format!("too many prop batches: {error}"))?;
    let mut writer = Writer::new();
    writer.bytes(&PROPS_MAGIC);
    writer.u16(PROPS_RECORD_VERSION);
    writer.u32(count);
    let mut budget = LogicalPropsBudget::new();
    let mut stats = PropsEncodingStats {
        batch_count: batches.len(),
        ..PropsEncodingStats::default()
    };
    for batch in batches {
        let interned = intern_vertices(batch)?;
        let header_start = writer.len();
        write_batch_header(&mut writer, batch)?;
        // Palette bytes and mode tags do not reduce the expanded allocation
        // charge. Account for the as-yet-unwritten vertex_count header here.
        let header_bytes = writer.len().saturating_sub(header_start);
        budget.add_slots(header_bytes, 1, 4)?;
        budget.add_slots(interned.vertices.len(), 81, 0)?;
        budget.add_slots(interned.indices.len(), 2, 4)?;
        write_vertex_storage(&mut writer, &interned.vertices)?;
        writer.u16s(&interned.indices)?;
        check_encoded_size(writer.len())?;
        stats.input_vertex_slots = stats
            .input_vertex_slots
            .saturating_add(batch.vertices.len());
        stats.stored_vertex_slots = stats
            .stored_vertex_slots
            .saturating_add(interned.vertices.len());
        stats.duplicate_slots = stats
            .duplicate_slots
            .saturating_add(batch.vertices.len().saturating_sub(interned.vertices.len()));
        stats.index_count = stats.index_count.saturating_add(batch.indices.len());
    }
    // The compiler retains several quality records at once. Do not retain an
    // input-sized reservation after compaction, or spare growth capacity.
    let mut bytes = writer.into_bytes();
    bytes.shrink_to_fit();
    Ok((bytes, stats))
}

fn write_batch_header(writer: &mut Writer, batch: &PropMeshBatch) -> Result<(), String> {
    if batch.pending_probe_lighting {
        return Err("prop batch still needs prepared probe lighting".to_string());
    }
    writer.str(&batch.model)?;
    writer.u8(u8::from(batch.casts_static_lighting));
    writer.u8(u8::from(batch.light_control.is_some()));
    if let Some(control) = batch.light_control {
        writer.u32(control);
    }
    writer.f32_3(batch.bounds.min);
    writer.f32_3(batch.bounds.max);
    let submeshes = u32::try_from(batch.submeshes.len())
        .map_err(|error| format!("prop batch has too many submeshes: {error}"))?;
    writer.u32(submeshes);
    for submesh in &batch.submeshes {
        write_optional_slot(writer, submesh.texture.map(u32::from))?;
        writer.u8(alpha_mode_code(submesh.alpha));
        writer.f32(submesh.alpha.cutoff);
        writer.f32_3(submesh.emission.color);
        writer.f32(submesh.emission.intensity);
        write_optional_slot(writer, submesh.emission.mask)?;
        writer.f32_3(submesh.response.specular);
        writer.f32(submesh.response.roughness);
        writer.f32(submesh.response.fluorescence.strength);
        write_optional_slot(writer, submesh.response.fluorescence.mask)?;
        writer.f32_3(submesh.response.fluorescence.tint);
        writer.u32(submesh.first_index);
        writer.u32(submesh.index_count);
    }
    Ok(())
}

struct InternedVertices<'a> {
    vertices: Vec<&'a Vertex>,
    indices: Vec<u16>,
}

fn intern_vertices(batch: &PropMeshBatch) -> Result<InternedVertices<'_>, String> {
    // Use the codec itself as the key: this includes signed zero and every
    // lighting/frame field, without relying on Rust struct padding or float Eq.
    let mut encoded =
        Writer::with_capacity(batch.vertices.len().saturating_mul(SERIALIZED_VERTEX_BYTES));
    for vertex in &batch.vertices {
        write_vertex(&mut encoded, vertex);
    }
    let (keys, remainder) = encoded.as_slice().as_chunks::<SERIALIZED_VERTEX_BYTES>();
    if !remainder.is_empty() || keys.len() != batch.vertices.len() {
        return Err("prop vertex codec no longer writes 81-byte records".to_string());
    }
    let mut by_bytes = HashMap::<[u8; SERIALIZED_VERTEX_BYTES], usize>::new();
    let mut vertices = Vec::with_capacity(batch.vertices.len());
    let mut remap = Vec::with_capacity(batch.vertices.len());
    for (vertex, key) in batch.vertices.iter().zip(keys) {
        let slot = *by_bytes.entry(*key).or_insert_with(|| {
            let new_slot = vertices.len();
            vertices.push(vertex);
            new_slot
        });
        remap.push(slot);
    }
    let mut indices = Vec::with_capacity(batch.indices.len());
    for index in &batch.indices {
        let slot = remap
            .get(usize::from(*index))
            .ok_or_else(|| "prop batch has an index outside its vertex list".to_string())?;
        // An input u16 index can only reference an original slot <= 65,535,
        // and first-occurrence interning never increases that slot. Unique
        // unreferenced vertices beyond that range remain in the record.
        indices.push(u16::try_from(*slot).map_err(|error| {
            format!("interned prop index does not fit the record's u16 slot: {error}")
        })?);
    }
    Ok(InternedVertices { vertices, indices })
}

#[derive(Clone, Copy)]
enum FrameIndexWidth {
    U16,
    U32,
}

impl FrameIndexWidth {
    const fn mode(self) -> u8 {
        match self {
            Self::U16 => 1,
            Self::U32 => 2,
        }
    }

    const fn bytes(self) -> usize {
        match self {
            Self::U16 => 2,
            Self::U32 => 4,
        }
    }

    fn write(self, writer: &mut Writer, index: usize) -> Result<(), String> {
        match self {
            Self::U16 => writer.u16(
                u16::try_from(index)
                    .map_err(|error| format!("prop frame index does not fit a u16: {error}"))?,
            ),
            Self::U32 => writer.u32(
                u32::try_from(index)
                    .map_err(|error| format!("prop frame index does not fit a u32: {error}"))?,
            ),
        }
        Ok(())
    }

    fn read(self, reader: &mut Reader<'_>) -> Result<usize, String> {
        match self {
            Self::U16 => Ok(usize::from(reader.u16()?)),
            Self::U32 => usize::try_from(reader.u32()?)
                .map_err(|error| format!("prop frame index is too large: {error}")),
        }
    }
}

struct FramePalette {
    frames: Vec<[u8; FRAME_BYTES]>,
    vertex_frames: Vec<usize>,
    width: FrameIndexWidth,
}

fn choose_frame_palette(
    keys: &[[u8; SERIALIZED_VERTEX_BYTES]],
) -> Result<Option<FramePalette>, String> {
    let mut frames = Vec::new();
    let mut vertex_frames = Vec::with_capacity(keys.len());
    let mut by_bytes = HashMap::<[u8; FRAME_BYTES], usize>::new();
    for key in keys {
        let frame: [u8; FRAME_BYTES] = key
            .get(VERTEX_PREFIX_BYTES..FRAME_END)
            .ok_or_else(|| "prop neutral vertex omitted its frame".to_string())?
            .try_into()
            .map_err(|error| format!("prop neutral frame has the wrong width: {error}"))?;
        let slot = *by_bytes.entry(frame).or_insert_with(|| {
            let new_slot = frames.len();
            frames.push(frame);
            new_slot
        });
        vertex_frames.push(slot);
    }
    let width = if frames.len() <= usize::from(u16::MAX).saturating_add(1) {
        FrameIndexWidth::U16
    } else {
        FrameIndexWidth::U32
    };
    // Include the mode byte in both alternatives and the palette count in
    // the palette alternative. Literal mode wins exact size ties.
    let literal_size = checked_storage_size(keys.len(), SERIALIZED_VERTEX_BYTES, 1)?;
    let frame_size = checked_storage_size(frames.len(), FRAME_BYTES, 5)?;
    let palette_size = checked_storage_size(
        keys.len(),
        VERTEX_LITERAL_BYTES.saturating_add(width.bytes()),
        frame_size,
    )?;
    Ok((palette_size < literal_size).then_some(FramePalette {
        frames,
        vertex_frames,
        width,
    }))
}

fn checked_storage_size(count: usize, width: usize, overhead: usize) -> Result<usize, String> {
    count
        .checked_mul(width)
        .and_then(|bytes| bytes.checked_add(overhead))
        .ok_or_else(|| "prop vertex storage size overflows".to_string())
}

fn write_vertex_storage(writer: &mut Writer, vertices: &[&Vertex]) -> Result<(), String> {
    let count = u32::try_from(vertices.len())
        .map_err(|error| format!("prop batch has too many vertices: {error}"))?;
    if count == 0
        || usize::try_from(count).unwrap_or(usize::MAX) > crate::engine::spatial::MAX_INDEX_VERTICES
    {
        return Err(format!(
            "prop batch declares {count} vertices (16-bit address limit {})",
            crate::engine::spatial::MAX_INDEX_VERTICES
        ));
    }
    let mut encoded = Writer::with_capacity(checked_storage_size(
        vertices.len(),
        SERIALIZED_VERTEX_BYTES,
        0,
    )?);
    for vertex in vertices {
        write_vertex(&mut encoded, vertex);
    }
    let (keys, remainder) = encoded.as_slice().as_chunks::<SERIALIZED_VERTEX_BYTES>();
    if !remainder.is_empty() || keys.len() != vertices.len() {
        return Err("prop vertex codec no longer writes 81-byte records".to_string());
    }
    writer.u32(count);
    if let Some(palette) = choose_frame_palette(keys)? {
        write_palette_vertices(writer, keys, &palette)?;
    } else {
        writer.u8(VERTEX_MODE_LITERAL);
        writer.bytes(encoded.as_slice());
    }
    Ok(())
}

fn write_palette_vertices(
    writer: &mut Writer,
    keys: &[[u8; SERIALIZED_VERTEX_BYTES]],
    palette: &FramePalette,
) -> Result<(), String> {
    writer.u8(palette.width.mode());
    let count = u32::try_from(palette.frames.len())
        .map_err(|error| format!("prop frame count is too large: {error}"))?;
    writer.u32(count);
    for frame in &palette.frames {
        writer.bytes(frame);
    }
    for (key, index) in keys.iter().zip(&palette.vertex_frames) {
        writer.bytes(
            key.get(..VERTEX_PREFIX_BYTES)
                .ok_or_else(|| "prop neutral vertex omitted its prefix".to_string())?,
        );
        writer.bytes(
            key.get(FRAME_END..)
                .ok_or_else(|| "prop neutral vertex omitted its lightmap tail".to_string())?,
        );
        palette.width.write(writer, *index)?;
    }
    Ok(())
}

/// Decodes prepared prop batches.
///
/// Textures are intentionally absent: the returned batches carry empty
/// `textures` lists and the caller attaches the images decoded from the same
/// verified model before upload.
///
/// # Errors
/// Returns an error when the input is malformed, out of bounds or unsupported.
pub fn read_props(bytes: &[u8]) -> Result<Vec<PropMeshBatch>, String> {
    check_encoded_size(bytes.len())?;
    let mut reader = Reader::new(bytes);
    if reader.bytes(4)? != PROPS_MAGIC {
        return Err("props record has the wrong magic".to_string());
    }
    let version = reader.u16()?;
    if version != PROPS_RECORD_VERSION {
        return Err(format!(
            "props record version {version} is not supported (this build reads {PROPS_RECORD_VERSION})"
        ));
    }
    let count = reader.count(
        u64::try_from(MAX_PROP_BATCHES).unwrap_or(u64::MAX),
        "prop batch count",
    )?;
    // Even the oldest accepted batch needs its fixed header, nonempty model
    // path, one literal vertex and the index count. Reject impossible counts
    // before allocating their much larger in-memory batch records.
    reader.require_items(count, 110, "prop batch count")?;
    let mut budget = LogicalPropsBudget::new();
    let mut batches = Vec::with_capacity(count);
    for _ in 0..count {
        batches.push(read_batch(&mut reader, &mut budget)?);
    }
    if !reader.is_empty() {
        return Err(format!(
            "props record has {} trailing bytes",
            reader.remaining()
        ));
    }
    Ok(batches)
}

fn read_batch(
    reader: &mut Reader<'_>,
    budget: &mut LogicalPropsBudget,
) -> Result<PropMeshBatch, String> {
    let header_start = reader.remaining();
    let header = read_batch_header(reader)?;
    let vertex_count = read_vertex_count(reader)?;
    budget.add_slots(header_start.saturating_sub(reader.remaining()), 1, 0)?;
    budget.add_slots(
        vertex_count,
        u64::try_from(serialized_vertex_bytes()).unwrap_or(u64::MAX),
        0,
    )?;
    let vertices = read_vertex_storage(reader, vertex_count)?;
    let indices = read_prop_indices(reader, budget)?;
    validate_draw_indices(vertex_count, &indices, &header.submeshes)?;
    Ok(PropMeshBatch {
        model: header.model,
        light_control: header.light_control,
        source_primitives: Vec::new(),
        casts_static_lighting: header.casts_static_lighting,
        pending_probe_lighting: false,
        textures: Vec::<Arc<crate::engine::loader::RawImage>>::new(),
        submeshes: header.submeshes,
        vertices,
        indices,
        bounds: header.bounds,
    })
}

struct BatchHeader {
    model: String,
    light_control: Option<u32>,
    casts_static_lighting: bool,
    submeshes: Vec<PropSubmeshBatch>,
    bounds: Aabb,
}

fn read_batch_header(reader: &mut Reader<'_>) -> Result<BatchHeader, String> {
    let model = reader.str(MAX_MODEL_PATH)?;
    if model.is_empty() {
        return Err("prop batch has an empty model path".to_string());
    }
    if super::normalize_entry_name(&model).as_deref() != Some(model.as_str()) {
        return Err(format!(
            "prop model path '{model}' is not a safe relative path"
        ));
    }
    let casts_static_lighting = read_static_caster(reader)?;
    let light_control = {
        match reader.u8()? {
            0 => None,
            1 => {
                let control = reader.u32()?;
                if u64::from(control)
                    >= crate::engine::level::MAX_LEVEL_CEILING_LIGHTS
                        .saturating_add(crate::engine::level::MAX_LEVEL_PROPS)
                {
                    return Err("prop light controller identity exceeds the level bound".into());
                }
                Some(control)
            }
            _ => return Err("prop light controller presence is invalid".into()),
        }
    };
    let bounds_min = reader.f32_3()?;
    let bounds_max = reader.f32_3()?;
    if !finite3(bounds_min) || !finite3(bounds_max) {
        return Err("prop batch has a non-finite bound".to_string());
    }
    if bounds_min
        .iter()
        .zip(bounds_max.iter())
        .any(|(low, high)| low > high)
    {
        return Err("prop batch bounds are inverted".to_string());
    }
    let submesh_count = reader.count(
        u64::try_from(MAX_PROP_SUBMESHES).unwrap_or(u64::MAX),
        "prop submesh count",
    )?;
    let mut submeshes = Vec::with_capacity(submesh_count);
    for _ in 0..submesh_count {
        submeshes.push(read_submesh(reader)?);
    }
    Ok(BatchHeader {
        model,
        light_control,
        casts_static_lighting,
        submeshes,
        bounds: Aabb {
            min: bounds_min,
            max: bounds_max,
        },
    })
}

fn read_vertex_count(reader: &mut Reader<'_>) -> Result<usize, String> {
    let vertex_count = u64::from(reader.u32()?);
    if vertex_count == 0
        || vertex_count
            > u64::try_from(crate::engine::spatial::MAX_INDEX_VERTICES).unwrap_or(u64::MAX)
    {
        return Err(format!(
            "prop batch declares {vertex_count} vertices (16-bit address limit {})",
            crate::engine::spatial::MAX_INDEX_VERTICES
        ));
    }
    usize::try_from(vertex_count)
        .map_err(|error| format!("prop batch vertex count is too large: {error}"))
}

/// One validated serialized vertex run; offsets refer to the original PMP1 bytes.
pub(crate) struct VertexRegion {
    pub start: usize,
    pub end: usize,
    pub stride: usize,
}

/// Validate a record and locate its vertex runs without allocating decoded meshes.
pub(crate) fn vertex_regions(bytes: &[u8]) -> Result<Vec<VertexRegion>, String> {
    check_encoded_size(bytes.len())?;
    let mut reader = Reader::new(bytes);
    if reader.bytes(4)? != PROPS_MAGIC {
        return Err("props record has the wrong magic".to_string());
    }
    let version = reader.u16()?;
    if version != PROPS_RECORD_VERSION {
        return Err(format!("props record version {version} is not supported"));
    }
    let count = reader.count(
        u64::try_from(MAX_PROP_BATCHES).unwrap_or(u64::MAX),
        "prop batch count",
    )?;
    let mut regions = Vec::with_capacity(count.min(4096));
    let mut budget = LogicalPropsBudget::new();
    for _ in 0..count {
        let header_start = reader.remaining();
        let header = read_batch_header(&mut reader)?;
        let vertices = read_vertex_count(&mut reader)?;
        budget.add_slots(header_start.saturating_sub(reader.remaining()), 1, 0)?;
        budget.add_slots(
            vertices,
            u64::try_from(serialized_vertex_bytes()).unwrap_or(u64::MAX),
            0,
        )?;
        let stride = validate_vertex_run(&mut reader, vertices)?;
        let end = bytes.len().saturating_sub(reader.remaining());
        let run_bytes = checked_storage_size(vertices, stride, 0)?;
        regions.push(VertexRegion {
            start: end.saturating_sub(run_bytes),
            end,
            stride,
        });
        validate_index_run(&mut reader, &mut budget, vertices, &header.submeshes)?;
    }
    if !reader.is_empty() {
        return Err("props record has trailing bytes".to_string());
    }
    Ok(regions)
}

fn validate_vertex_run(reader: &mut Reader<'_>, vertices: usize) -> Result<usize, String> {
    let mode = reader.u8()?;
    if mode == 0 {
        let stride = serialized_vertex_bytes();
        let length = checked_storage_size(vertices, stride, 0)?;
        for key in reader.bytes(length)?.chunks_exact(stride) {
            let _vertex = read_vertex(&mut Reader::new(key))?;
        }
        return Ok(stride);
    }
    let width = match mode {
        1 => FrameIndexWidth::U16,
        2 => FrameIndexWidth::U32,
        other => return Err(format!("prop batch has unknown vertex mode {other}")),
    };
    let frames = reader.count(MAX_PROP_BATCH_VERTICES, "prop frame count")?;
    if frames == 0
        || frames > vertices
        || (mode == 1 && frames > usize::from(u16::MAX).saturating_add(1))
    {
        return Err("prop batch has an invalid frame palette count".to_string());
    }
    let frame_bytes = checked_storage_size(frames, FRAME_BYTES, 0)?;
    let literal_bytes = vertex_literal_bytes();
    let stride = literal_bytes.saturating_add(width.bytes());
    let required = checked_storage_size(vertices, stride, frame_bytes)?;
    if required > reader.remaining() {
        return Err("prop frame palette or vertex storage is truncated".to_string());
    }
    let (palette, _) = reader.bytes(frame_bytes)?.as_chunks::<FRAME_BYTES>();
    for frame in palette {
        validate_frame(frame)?;
    }
    for _ in 0..vertices {
        let literal = reader.bytes(literal_bytes)?;
        let frame = palette
            .get(width.read(reader)?)
            .ok_or_else(|| "prop vertex has a frame index outside its palette".to_string())?;
        let _vertex = read_palette_vertex(literal, frame)?;
    }
    Ok(stride)
}

fn validate_index_run(
    reader: &mut Reader<'_>,
    budget: &mut LogicalPropsBudget,
    vertices: usize,
    submeshes: &[PropSubmeshBatch],
) -> Result<(), String> {
    let count = reader.count(MAX_PROP_BATCH_VERTICES, "index run")?;
    budget.add_slots(count, 2, 4)?;
    if !count.is_multiple_of(3) {
        return Err("prop batch index count is not a multiple of three".to_string());
    }
    let length = checked_storage_size(count, 2, 0)?;
    let (pairs, _) = reader.bytes(length)?.as_chunks::<2>();
    if pairs
        .iter()
        .any(|pair| usize::from(u16::from_le_bytes(*pair)) >= vertices)
    {
        return Err("prop batch has an index outside its vertex list".to_string());
    }
    for submesh in submeshes {
        let end = u64::from(submesh.first_index)
            .checked_add(u64::from(submesh.index_count))
            .ok_or_else(|| "prop submesh range overflows".to_string())?;
        if end > u64::try_from(count).unwrap_or(u64::MAX) {
            return Err("prop submesh range is outside the index list".to_string());
        }
    }
    Ok(())
}

fn validate_draw_indices(
    vertex_count: usize,
    indices: &[u16],
    submeshes: &[PropSubmeshBatch],
) -> Result<(), String> {
    if indices
        .iter()
        .any(|index| usize::from(*index) >= vertex_count)
    {
        return Err("prop batch has an index outside its vertex list".to_string());
    }
    if !indices.len().is_multiple_of(3) {
        return Err("prop batch index count is not a multiple of three".to_string());
    }
    for submesh in submeshes {
        let start = usize::try_from(submesh.first_index)
            .map_err(|error| format!("prop submesh range is too large: {error}"))?;
        let length = usize::try_from(submesh.index_count)
            .map_err(|error| format!("prop submesh range is too large: {error}"))?;
        let end = start
            .checked_add(length)
            .ok_or_else(|| "prop submesh range overflows".to_string())?;
        if end > indices.len() {
            return Err("prop submesh range is outside the index list".to_string());
        }
    }
    Ok(())
}

fn read_prop_indices(
    reader: &mut Reader<'_>,
    budget: &mut LogicalPropsBudget,
) -> Result<Vec<u16>, String> {
    let index_count = reader.count(MAX_PROP_BATCH_VERTICES, "index run")?;
    budget.add_slots(index_count, 2, 4)?;
    let byte_count = checked_storage_size(index_count, 2, 0)?;
    let (pairs, _) = reader.bytes(byte_count)?.as_chunks::<2>();
    Ok(pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect())
}

fn read_vertex_storage(
    reader: &mut Reader<'_>,
    vertex_count: usize,
) -> Result<Vec<Vertex>, String> {
    match reader.u8()? {
        VERTEX_MODE_LITERAL => read_literal_vertices(reader, vertex_count),
        1 => read_palette_vertices(reader, vertex_count, FrameIndexWidth::U16),
        2 => read_palette_vertices(reader, vertex_count, FrameIndexWidth::U32),
        other => Err(format!("prop batch has unknown vertex mode {other}")),
    }
}

fn read_literal_vertices(
    reader: &mut Reader<'_>,
    vertex_count: usize,
) -> Result<Vec<Vertex>, String> {
    let stride = serialized_vertex_bytes();
    let byte_count = checked_storage_size(vertex_count, stride, 0)?;
    let bytes = reader.bytes(byte_count)?;
    let mut vertices = Vec::with_capacity(vertex_count.min(4096));
    for key in bytes.chunks_exact(stride) {
        vertices.push(read_vertex(&mut Reader::new(key))?);
    }
    Ok(vertices)
}

fn read_palette_vertices(
    reader: &mut Reader<'_>,
    vertex_count: usize,
    width: FrameIndexWidth,
) -> Result<Vec<Vertex>, String> {
    let frame_count = reader.count(MAX_PROP_BATCH_VERTICES, "prop frame count")?;
    if frame_count == 0 || frame_count > vertex_count {
        return Err(format!(
            "prop batch declares {frame_count} frames for {vertex_count} vertices"
        ));
    }
    if matches!(width, FrameIndexWidth::U16)
        && frame_count > usize::from(u16::MAX).saturating_add(1)
    {
        return Err("prop u16 frame palette declares more than 65,536 frames".to_string());
    }
    let frame_bytes = checked_storage_size(frame_count, FRAME_BYTES, 0)?;
    let required_bytes = checked_storage_size(
        vertex_count,
        vertex_literal_bytes().saturating_add(width.bytes()),
        frame_bytes,
    )?;
    if required_bytes > reader.remaining() {
        return Err("prop frame palette or vertex storage is truncated".to_string());
    }
    // Borrow the bounded palette run directly; no palette allocation is
    // needed. Validate even unused frames before allocating decoded vertices.
    let (frames, _) = reader.bytes(frame_bytes)?.as_chunks::<FRAME_BYTES>();
    for frame in frames {
        validate_frame(frame)?;
    }
    let mut vertices = Vec::with_capacity(vertex_count.min(4096));
    for _ in 0..vertex_count {
        let literal = reader.bytes(vertex_literal_bytes())?;
        let frame_index = width.read(reader)?;
        let frame = frames
            .get(frame_index)
            .ok_or_else(|| "prop vertex has a frame index outside its palette".to_string())?;
        vertices.push(read_palette_vertex(literal, frame)?);
    }
    Ok(vertices)
}

fn validate_frame(frame: &[u8; FRAME_BYTES]) -> Result<(), String> {
    let mut reader = Reader::new(frame);
    let normal = reader.f32_3()?;
    let tangent = reader.f32_3()?;
    let handedness = reader.f32()?;
    if !finite3(normal) || !finite3(tangent) || !handedness.is_finite() {
        return Err("prop frame palette has a non-finite component".to_string());
    }
    Ok(())
}

fn read_palette_vertex(literal: &[u8], frame: &[u8; FRAME_BYTES]) -> Result<Vertex, String> {
    // Reconstruct the neutral record on the stack and retain the mesh codec's
    // complete finite-field validation without a per-vertex allocation.
    let mut neutral = [0_u8; SERIALIZED_VERTEX_BYTES];
    neutral
        .get_mut(..VERTEX_PREFIX_BYTES)
        .ok_or_else(|| "prop neutral vertex omitted its prefix".to_string())?
        .copy_from_slice(
            literal
                .get(..VERTEX_PREFIX_BYTES)
                .ok_or_else(|| "prop vertex literal omitted its prefix".to_string())?,
        );
    neutral
        .get_mut(VERTEX_PREFIX_BYTES..FRAME_END)
        .ok_or_else(|| "prop neutral vertex omitted its frame".to_string())?
        .copy_from_slice(frame);
    neutral
        .get_mut(FRAME_END..serialized_vertex_bytes())
        .ok_or_else(|| "prop neutral vertex omitted its lightmap tail".to_string())?
        .copy_from_slice(
            literal
                .get(VERTEX_PREFIX_BYTES..)
                .ok_or_else(|| "prop vertex literal omitted its lightmap tail".to_string())?,
        );
    read_vertex(&mut Reader::new(&neutral))
}

fn read_static_caster(reader: &mut Reader<'_>) -> Result<bool, String> {
    match reader.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(format!(
            "prop batch has invalid static-caster marker {other}"
        )),
    }
}

fn read_submesh(reader: &mut Reader<'_>) -> Result<PropSubmeshBatch, String> {
    let texture = read_optional_slot(reader)?
        .map(u16::try_from)
        .transpose()
        .map_err(|error| format!("prop submesh texture slot does not fit a u16: {error}"))?;
    let alpha_mode = reader.u8()?;
    let mode = match alpha_mode {
        0 => AlphaMode::Opaque,
        1 => AlphaMode::Cutout,
        // A blended primitive's opacity is not carried: an imported GLB's
        // `baseColorFactor` alpha is already folded into its vertex colours,
        // so the contract itself is opacity 1.0 and per-instance fade is a
        // runtime component.
        2 => AlphaMode::Blend,
        other => return Err(format!("prop submesh has unknown alpha mode {other}")),
    };
    let alpha_cutoff = reader.f32()?;
    let color = reader.f32_3()?;
    let intensity = reader.f32()?;
    let mask = read_optional_slot(reader)?;
    if !finite3(color) || !intensity.is_finite() || !alpha_cutoff.is_finite() {
        return Err("prop submesh has a non-finite emission or alpha".to_string());
    }
    let (specular, roughness) = (reader.f32_3()?, reader.f32()?);
    if !finite3(specular)
        || !roughness.is_finite()
        || specular.iter().any(|v| !(0.0..=1.0).contains(v))
        || !(0.0..=1.0).contains(&roughness)
    {
        return Err("prop submesh has invalid scalar response".to_string());
    }
    let fluorescence = {
        let strength = reader.f32()?;
        let uv_mask = read_optional_slot(reader)?;
        let tint = reader.f32_3()?;
        if !strength.is_finite()
            || !(0.0..=crate::engine::materials::MAX_UV_RESPONSE).contains(&strength)
            || (uv_mask.is_some() && strength <= 0.0)
            || !finite3(tint)
            || tint.iter().any(|channel| !(0.0..=1.0).contains(channel))
        {
            return Err("prop submesh has invalid UV response".to_string());
        }
        crate::engine::materials::MaterialFluorescence {
            strength,
            mask: uv_mask,
            tint,
        }
    };
    Ok(PropSubmeshBatch {
        response: crate::engine::materials::MaterialResponse {
            specular,
            roughness,
            fluorescence,
            ..crate::engine::materials::MaterialResponse::NONE
        },
        texture,
        emission: MaterialEmission {
            color,
            intensity,
            mask,
        },
        alpha: MaterialAlpha {
            mode,
            opacity: 1.0,
            cutoff: alpha_cutoff,
        }
        .sanitized(),
        first_index: reader.u32()?,
        index_count: reader.u32()?,
    })
}

/// One submesh's alpha mode as the record encodes it.
const fn alpha_mode_code(alpha: MaterialAlpha) -> u8 {
    match alpha.mode {
        AlphaMode::Opaque => 0,
        AlphaMode::Cutout => 1,
        AlphaMode::Blend => 2,
    }
}

/// Writes one optional texture slot in the record's `u16` slot width.
///
/// The runtime material/emission index is 32 bits, but a prop record's slots
/// index the model's own texture list, which the model loader caps at
/// [`crate::engine::level::MAX_PROP_IMAGES`] (16); a slot that does not fit the record
/// is an explicit error rather than a truncation.
///
/// # Errors
/// Returns a message when the slot does not fit the record's `u16` field.
fn write_optional_slot(writer: &mut Writer, slot: Option<u32>) -> Result<(), String> {
    match slot {
        None => {
            writer.u8(0);
        }
        Some(value) => {
            let texture_slot = u16::try_from(value).map_err(|error| {
                format!("prop texture slot {value} does not fit the record's u16 slot: {error}")
            })?;
            writer.u8(1);
            writer.u16(texture_slot);
        }
    }
    Ok(())
}

fn read_optional_slot(reader: &mut Reader<'_>) -> Result<Option<u32>, String> {
    match reader.u8()? {
        0 => Ok(None),
        1 => Ok(Some(u32::from(reader.u16()?))),
        other => Err(format!("invalid optional slot marker {other}")),
    }
}

#[cfg(test)]
#[path = "../tests/prop_batch_boundary.rs"]
mod boundary_tests;
