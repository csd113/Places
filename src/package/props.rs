//! Binary codec for the prepared static prop batches.
//!
//! Prop vertices are pre-transformed and pre-lit by the compiler; model
//! textures are *not* embedded here — the batch records the model path and the
//! per-submesh texture slots, and the player reattaches the decoded texture
//! images from the same catalog model it verifies by hash. This keeps one copy
//! of model artwork on disk and makes the package's dependency list exact.
//!
//! ```text
//! magic         4 bytes  "PLMP"
//! version       u16      [`PROPS_RECORD_VERSION`]
//! batch_count   u32
//! batches       batch_count x batch
//! ```
//!
//! ```text
//! batch:
//!   model       u32 length + UTF-8 bytes
//!   casts_static_lighting u8 (0 = fully claimed character model, 1 = static caster)
//!   bounds_min  f32 x 3
//!   bounds_max  f32 x 3
//!   submesh_count u32
//!   submeshes   submesh_count x submesh
//!   vertices    u32 count, then vertex_mode u8 and vertex storage
//!   indices     u32 count, then count x u16
//! ```
//!
//! Vertex mode 0 stores literal 69-byte neutral vertices. Modes 1 and 2 store
//! a `u32` frame count, then 28-byte normal/tangent/handedness frames. Each
//! vertex retains its 36-byte position/colour/UV prefix and 5-byte lightmap
//! tail, followed by a `u16` (mode 1) or `u32` (mode 2) frame index. The palette
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
//!   first_index  u32
//!   index_count  u32
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::materials::{AlphaMode, MaterialAlpha, MaterialEmission};
use crate::render::{PropMeshBatch, PropSubmeshBatch, Vertex};
use crate::spatial::Aabb;

use super::binary::{Reader, Writer, finite3};
use super::mesh::{read_vertex, write_vertex};
use super::{MAX_BINARY_BYTES, MAX_PROP_BATCH_VERTICES, MAX_PROP_BATCHES, MAX_PROP_SUBMESHES};

/// Version of the props record layout.
///
/// Version 2 added the per-submesh alpha contract (glTF `MASK` foliage draws
/// through the cutout pass); version 3 adds the blended mode (a model
/// authoring `alphaMode: "BLEND"`); version 4 adds scalar material response;
/// version 5 preserves whether a model's retained bind pose is a static caster.
/// Version 6 adds lossless per-batch normal/tangent/handedness frame palettes.
/// Versions 3 and 4 retain the conservative static-caster default, and version 3
/// also retains the explicit matte material default. Older records are refused.
pub const PROPS_RECORD_VERSION: u16 = 6;

/// Magic identifying a props record.
pub const PROPS_MAGIC: [u8; 4] = *b"PLMP";

/// Largest accepted model path length in a prop batch.
const MAX_MODEL_PATH: u64 = 1024;

/// Width of the neutral vertex written by the existing mesh codec.
const SERIALIZED_VERTEX_BYTES: usize = 69;

const VERTEX_PREFIX_BYTES: usize = 36;
const FRAME_BYTES: usize = 28;
const FRAME_END: usize = 64;
const VERTEX_LITERAL_BYTES: usize = 41;
const VERTEX_MODE_LITERAL: u8 = 0;

/// Retain the historical literal record's allocation ceiling even when the
/// encoded frame palette is smaller. All charges use the actual legacy-width
/// headers plus 69 bytes per stored vertex and two bytes per ordered index.
struct LogicalPropsBudget {
    bytes: u64,
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
/// Only vertices whose complete 69-byte records match are shared. First
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
        budget.add_slots(interned.vertices.len(), 69, 0)?;
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
    writer.str(&batch.model)?;
    writer.u8(u8::from(batch.casts_static_lighting));
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
        return Err("prop vertex codec no longer writes 69-byte records".to_string());
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
    if count == 0 || u64::from(count) > MAX_PROP_BATCH_VERTICES {
        return Err(format!(
            "prop batch declares {count} vertices (limit {MAX_PROP_BATCH_VERTICES})"
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
        return Err("prop vertex codec no longer writes 69-byte records".to_string());
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
    if !matches!(version, 3..=5 | PROPS_RECORD_VERSION) {
        return Err(format!(
            "props record version {version} is not supported (this build reads {PROPS_RECORD_VERSION})"
        ));
    }
    let count = reader.count(
        u64::try_from(MAX_PROP_BATCHES).unwrap_or(u64::MAX),
        "prop batch count",
    )?;
    let mut budget = LogicalPropsBudget::new();
    let mut batches = Vec::with_capacity(count);
    for _ in 0..count {
        batches.push(read_batch(&mut reader, version, &mut budget)?);
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
    version: u16,
    budget: &mut LogicalPropsBudget,
) -> Result<PropMeshBatch, String> {
    let header_start = reader.remaining();
    let model = reader.str(MAX_MODEL_PATH)?;
    if model.is_empty() {
        return Err("prop batch has an empty model path".to_string());
    }
    if super::normalize_entry_name(&model).as_deref() != Some(model.as_str()) {
        return Err(format!(
            "prop model path '{model}' is not a safe relative path"
        ));
    }
    let casts_static_lighting = read_static_caster(reader, version)?;
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
        submeshes.push(read_submesh(reader, version)?);
    }
    let vertex_count = u64::from(reader.u32()?);
    if vertex_count == 0 || vertex_count > MAX_PROP_BATCH_VERTICES {
        return Err(format!(
            "prop batch declares {vertex_count} vertices (limit {MAX_PROP_BATCH_VERTICES})"
        ));
    }
    let batch_vertex_count = usize::try_from(vertex_count)
        .map_err(|error| format!("prop batch vertex count is too large: {error}"))?;
    budget.add_slots(header_start.saturating_sub(reader.remaining()), 1, 0)?;
    budget.add_slots(batch_vertex_count, 69, 0)?;
    let vertices = read_vertex_storage(reader, version, batch_vertex_count)?;
    let indices = read_prop_indices(reader, budget)?;
    validate_draw_indices(batch_vertex_count, &indices, &submeshes)?;
    Ok(PropMeshBatch {
        model,
        source_primitives: Vec::new(),
        casts_static_lighting,
        textures: Vec::<Arc<crate::loader::RawImage>>::new(),
        submeshes,
        vertices,
        indices,
        bounds: Aabb {
            min: bounds_min,
            max: bounds_max,
        },
    })
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
    version: u16,
    vertex_count: usize,
) -> Result<Vec<Vertex>, String> {
    if version < 6 {
        return read_literal_vertices(reader, vertex_count);
    }
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
    let byte_count = checked_storage_size(vertex_count, SERIALIZED_VERTEX_BYTES, 0)?;
    let bytes = reader.bytes(byte_count)?;
    let (keys, _) = bytes.as_chunks::<SERIALIZED_VERTEX_BYTES>();
    let mut vertices = Vec::with_capacity(vertex_count.min(4096));
    for key in keys {
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
        VERTEX_LITERAL_BYTES.saturating_add(width.bytes()),
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
        let literal = reader.bytes(VERTEX_LITERAL_BYTES)?;
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
        .get_mut(FRAME_END..)
        .ok_or_else(|| "prop neutral vertex omitted its lightmap tail".to_string())?
        .copy_from_slice(
            literal
                .get(VERTEX_PREFIX_BYTES..)
                .ok_or_else(|| "prop vertex literal omitted its lightmap tail".to_string())?,
        );
    read_vertex(&mut Reader::new(&neutral))
}

fn read_static_caster(reader: &mut Reader<'_>, version: u16) -> Result<bool, String> {
    if version < 5 {
        return Ok(true);
    }
    match reader.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(format!(
            "prop batch has invalid static-caster marker {other}"
        )),
    }
}

fn read_submesh(reader: &mut Reader<'_>, version: u16) -> Result<PropSubmeshBatch, String> {
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
    let (specular, roughness) = if version >= 4 {
        (reader.f32_3()?, reader.f32()?)
    } else {
        ([0.0; 3], 1.0)
    };
    if !finite3(specular)
        || !roughness.is_finite()
        || specular.iter().any(|v| !(0.0..=1.0).contains(v))
        || !(0.0..=1.0).contains(&roughness)
    {
        return Err("prop submesh has invalid scalar response".to_string());
    }
    Ok(PropSubmeshBatch {
        response: crate::materials::MaterialResponse {
            specular,
            roughness,
            ..crate::materials::MaterialResponse::NONE
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
/// [`crate::level::MAX_PROP_IMAGES`] (16); a slot that does not fit the record
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
mod interning_tests {
    use super::*;

    pub(super) fn batch(vertices: Vec<Vertex>, indices: Vec<u16>) -> Result<PropMeshBatch, String> {
        let index_count = u32::try_from(indices.len())
            .map_err(|error| format!("test index count is too large: {error}"))?;
        Ok(PropMeshBatch {
            model: "models/interning.glb".to_string(),
            source_primitives: Vec::new(),
            casts_static_lighting: false,
            textures: Vec::new(),
            submeshes: vec![PropSubmeshBatch {
                texture: Some(2),
                alpha: MaterialAlpha::blend(1.0),
                emission: MaterialEmission {
                    color: [0.5, 0.25, 0.125],
                    intensity: 2.0,
                    mask: Some(1),
                },
                response: crate::materials::MaterialResponse::with_sheen(0.25, 0.5),
                first_index: 0,
                index_count,
            }],
            vertices,
            indices,
            bounds: Aabb {
                min: [-2.0; 3],
                max: [2.0; 3],
            },
        })
    }

    fn vertex_bytes(vertex: &Vertex) -> Vec<u8> {
        let mut writer = Writer::new();
        write_vertex(&mut writer, vertex);
        writer.into_bytes()
    }

    fn expanded_vertex_bytes(batch: &PropMeshBatch) -> Result<Vec<u8>, String> {
        let mut writer = Writer::new();
        for index in &batch.indices {
            let vertex = batch
                .vertices
                .get(usize::from(*index))
                .ok_or_else(|| "test triangle has an invalid vertex index".to_string())?;
            write_vertex(&mut writer, vertex);
        }
        Ok(writer.into_bytes())
    }

    fn roundtrip(
        batch: &PropMeshBatch,
    ) -> Result<(Vec<u8>, PropMeshBatch, PropsEncodingStats), String> {
        let (bytes, stats) = write_props_with_stats(std::slice::from_ref(batch))?;
        let mut decoded = read_props(&bytes)?;
        assert_eq!(decoded.len(), 1, "the codec must retain its one batch");
        let decoded_batch = decoded
            .pop()
            .ok_or_else(|| "prop roundtrip omitted its batch".to_string())?;
        Ok((bytes, decoded_batch, stats))
    }

    fn triangle_indices(vertex_count: usize) -> Result<Vec<u16>, String> {
        let mut indices = Vec::with_capacity(vertex_count.saturating_mul(3));
        for slot in 0..vertex_count {
            let index = u16::try_from(slot)
                .map_err(|error| format!("test vertex index is too large: {error}"))?;
            indices.extend_from_slice(&[0, index, 0]);
        }
        Ok(indices)
    }

    fn assert_batch_metadata(original: &PropMeshBatch, decoded: &PropMeshBatch) {
        assert_eq!(decoded.model, original.model, "model identity must survive");
        assert_eq!(
            decoded.casts_static_lighting, original.casts_static_lighting,
            "static caster metadata must survive"
        );
        assert_eq!(
            decoded.submeshes, original.submeshes,
            "material contracts and draw ranges must survive"
        );
        assert_eq!(
            decoded.bounds.min.map(f32::to_bits),
            original.bounds.min.map(f32::to_bits),
            "minimum bounds must retain their bits"
        );
        assert_eq!(
            decoded.bounds.max.map(f32::to_bits),
            original.bounds.max.map(f32::to_bits),
            "maximum bounds must retain their bits"
        );
    }

    #[test]
    fn exact_vertex_interning_preserves_ordered_triangles_and_metadata() -> Result<(), String> {
        let lower_left = Vertex::new([0.0, 0.0, 0.0], [1.0; 4], [0.0, 0.0]);
        let lower_right = Vertex::new([1.0, 0.0, 0.0], [1.0; 4], [1.0, 0.0]);
        let upper_right = Vertex::new([1.0, 1.0, 0.0], [1.0; 4], [1.0, 1.0]);
        let upper_left = Vertex::new([0.0, 1.0, 0.0], [1.0; 4], [0.0, 1.0]);
        let unreferenced_apex = Vertex::new([0.5, 0.5, 1.0], [0.5; 4], [0.5, 0.5]);
        let mut original = batch(
            vec![
                lower_left,
                lower_right,
                upper_right,
                lower_left,
                upper_right,
                upper_left,
                unreferenced_apex,
                lower_right,
            ],
            vec![0, 1, 2, 3, 4, 5],
        )?;
        let first_submesh = original
            .submeshes
            .first_mut()
            .ok_or_else(|| "test batch omitted its first submesh".to_string())?;
        first_submesh.index_count = 3;
        original.submeshes.push(PropSubmeshBatch {
            texture: None,
            alpha: MaterialAlpha::OPAQUE,
            emission: MaterialEmission::NONE,
            response: crate::materials::MaterialResponse::NONE,
            first_index: 3,
            index_count: 3,
        });
        let (bytes, decoded, stats) = roundtrip(&original)?;
        assert_eq!(
            decoded.vertices,
            [
                lower_left,
                lower_right,
                upper_right,
                upper_left,
                unreferenced_apex
            ],
            "keep first occurrence order, including the unreferenced vertex"
        );
        assert_eq!(
            decoded.indices,
            [0, 1, 2, 0, 2, 3],
            "keep triangle and corner order"
        );
        assert_eq!(
            expanded_vertex_bytes(&original)?,
            expanded_vertex_bytes(&decoded)?,
            "every drawn corner must retain all 69 bytes"
        );
        assert_batch_metadata(&original, &decoded);
        assert_eq!(
            stats,
            PropsEncodingStats {
                input_vertex_slots: 8,
                stored_vertex_slots: 5,
                duplicate_slots: 3,
                batch_count: 1,
                index_count: 6
            },
            "report exact slot counts"
        );
        assert_eq!(
            write_props(std::slice::from_ref(&original))?,
            bytes,
            "encoding must be deterministic"
        );
        assert_eq!(
            write_props(std::slice::from_ref(&decoded))?,
            bytes,
            "the compact representation must be idempotent"
        );
        assert_eq!(
            original.vertices.len(),
            8,
            "encoding must not mutate its input"
        );
        Ok(())
    }

    #[test]
    fn identical_vertices_are_interned_only_within_their_own_batch() -> Result<(), String> {
        let first = batch(vec![Vertex::UNLIT; 3], vec![0, 1, 2])?;
        let mut second = first.clone();
        second.model = "models/other_interning.glb".to_string();
        second.casts_static_lighting = true;
        let originals = [first, second];
        let (bytes, stats) = write_props_with_stats(&originals)?;
        let decoded = read_props(&bytes)?;
        assert_eq!(decoded.len(), 2, "both independent batches must remain");
        for (original, decoded_batch) in originals.iter().zip(&decoded) {
            assert_eq!(
                decoded_batch.vertices.len(),
                1,
                "each batch must own its shared vertex"
            );
            assert_eq!(
                decoded_batch.indices, [0; 3],
                "indices must remain local to their batch"
            );
            assert_batch_metadata(original, decoded_batch);
            assert_eq!(
                expanded_vertex_bytes(original)?,
                expanded_vertex_bytes(decoded_batch)?,
                "each batch's drawn corners must be byte exact"
            );
        }
        assert_eq!(
            stats,
            PropsEncodingStats {
                input_vertex_slots: 6,
                stored_vertex_slots: 2,
                duplicate_slots: 4,
                batch_count: 2,
                index_count: 6
            },
            "counts must aggregate independently compacted batches"
        );
        Ok(())
    }

    #[test]
    fn every_serialized_vertex_attribute_remains_a_distinct_seam() -> Result<(), String> {
        let base = Vertex {
            pos: [1.0; 3],
            color: [0.5; 4],
            uv: [0.125; 2],
            normal: [0.0, 0.0, 1.0],
            tangent: [1.0, 0.0, 0.0],
            handedness: 1.0,
            lightmap: [17, 41],
            lightmap_page: 9,
        };
        let base_bytes = vertex_bytes(&base);
        assert_eq!(
            base_bytes.len(),
            SERIALIZED_VERTEX_BYTES,
            "the key must cover the entire record"
        );
        let mut vertices = vec![base, base];
        let mut expected_keys = vec![base_bytes.clone()];
        for offset in 0..SERIALIZED_VERTEX_BYTES {
            let mut changed = base_bytes.clone();
            *changed
                .get_mut(offset)
                .ok_or_else(|| "test omitted an attribute byte".to_string())? ^= 1;
            vertices.push(read_vertex(&mut Reader::new(&changed))?);
            expected_keys.push(changed);
        }
        let indices = triangle_indices(vertices.len())?;
        let original = batch(vertices, indices)?;
        let (_, decoded, stats) = roundtrip(&original)?;
        let actual_keys: Vec<_> = decoded.vertices.iter().map(vertex_bytes).collect();
        assert_eq!(
            actual_keys, expected_keys,
            "a change in any serialized byte must prevent sharing"
        );
        assert_eq!(
            stats.duplicate_slots, 1,
            "only the exact duplicate may merge"
        );
        assert_eq!(
            expanded_vertex_bytes(&original)?,
            expanded_vertex_bytes(&decoded)?,
            "all seam attributes must survive in draw order"
        );
        Ok(())
    }

    #[test]
    fn signed_zero_vertex_components_do_not_merge() -> Result<(), String> {
        let base = Vertex {
            pos: [0.0; 3],
            color: [0.0; 4],
            uv: [0.0; 2],
            normal: [0.0; 3],
            tangent: [0.0; 3],
            handedness: 0.0,
            ..Vertex::UNLIT
        };
        let base_bytes = vertex_bytes(&base);
        let mut vertices = vec![base, base];
        let mut expected_keys = vec![base_bytes.clone()];
        for component in 0_usize..16 {
            let sign_offset = component.saturating_mul(4).saturating_add(3);
            let mut changed = base_bytes.clone();
            *changed
                .get_mut(sign_offset)
                .ok_or_else(|| "test omitted a float sign byte".to_string())? ^= 0x80;
            vertices.push(read_vertex(&mut Reader::new(&changed))?);
            expected_keys.push(changed);
        }
        let indices = triangle_indices(vertices.len())?;
        let original = batch(vertices, indices)?;
        let (_, decoded, stats) = roundtrip(&original)?;
        let actual_keys: Vec<_> = decoded.vertices.iter().map(vertex_bytes).collect();
        assert_eq!(
            actual_keys, expected_keys,
            "each signed zero must retain its original component bits"
        );
        assert_eq!(
            stats.duplicate_slots, 1,
            "positive and negative zero are distinct keys"
        );
        assert_eq!(
            expanded_vertex_bytes(&original)?,
            expanded_vertex_bytes(&decoded)?,
            "signed zeros must survive in draw order"
        );
        Ok(())
    }

    #[test]
    fn invalid_source_indices_are_rejected_before_remapping() -> Result<(), String> {
        let original = batch(vec![Vertex::UNLIT; 3], vec![0, 1, 3])?;
        let Err(error) = write_props(std::slice::from_ref(&original)) else {
            return Err("the codec accepted an invalid original index".to_string());
        };
        assert!(
            error.contains("index outside its vertex list"),
            "the error must identify the invalid original index"
        );
        Ok(())
    }

    #[test]
    fn unreferenced_unique_vertices_beyond_u16_index_space_remain_valid() -> Result<(), String> {
        let unique_slots = usize::from(u16::MAX).saturating_add(2);
        let mut vertices = Vec::with_capacity(unique_slots.saturating_add(1));
        for slot in 0..unique_slots {
            let offset = u32::try_from(slot)
                .map_err(|error| format!("test float offset is too large: {error}"))?;
            let x = f32::from_bits(1.0_f32.to_bits().saturating_add(offset));
            vertices.push(Vertex::new([x, 0.0, 0.0], [1.0; 4], [0.0; 2]));
        }
        let duplicate = *vertices
            .first()
            .ok_or_else(|| "test omitted its initial vertex".to_string())?;
        vertices.push(duplicate);
        let original = batch(vertices, vec![0, 1, u16::MAX])?;
        let (_, decoded, stats) = roundtrip(&original)?;
        assert_eq!(
            decoded.vertices.len(),
            unique_slots,
            "retain every unique unreferenced vertex above the u16 index range"
        );
        assert_eq!(
            decoded.indices, original.indices,
            "valid original u16 indices must remain valid"
        );
        assert_eq!(
            stats.input_vertex_slots,
            unique_slots.saturating_add(1),
            "count unreferenced input slots"
        );
        assert_eq!(
            stats.stored_vertex_slots, unique_slots,
            "count unreferenced retained slots"
        );
        assert_eq!(stats.duplicate_slots, 1, "remove only the exact duplicate");
        let original_keys: Vec<_> = original
            .vertices
            .iter()
            .take(unique_slots)
            .map(vertex_bytes)
            .collect();
        let decoded_keys: Vec<_> = decoded.vertices.iter().map(vertex_bytes).collect();
        assert_eq!(
            decoded_keys, original_keys,
            "retain first occurrence order beyond the index range"
        );
        assert_eq!(
            expanded_vertex_bytes(&original)?,
            expanded_vertex_bytes(&decoded)?,
            "all referenced corners must remain byte exact"
        );
        Ok(())
    }
}

#[cfg(test)]
mod palette_tests {
    use super::*;

    fn vertex_stream(vertices: &[Vertex]) -> Vec<u8> {
        let mut writer = Writer::new();
        for vertex in vertices {
            write_vertex(&mut writer, vertex);
        }
        writer.into_bytes()
    }

    fn vertices_with_distinct_frames(frame_count: usize) -> Result<Vec<Vertex>, String> {
        let mut vertices = Vec::with_capacity(frame_count.saturating_mul(2));
        for slot in 0..frame_count {
            let offset = u32::try_from(slot)
                .map_err(|error| format!("test frame offset is too large: {error}"))?;
            let frame_component = f32::from_bits(1.0_f32.to_bits().saturating_add(offset));
            for x in [0.0, 1.0] {
                vertices.push(Vertex {
                    pos: [x, 0.0, 0.0],
                    normal: [frame_component, 0.0, 0.0],
                    ..Vertex::UNLIT
                });
            }
        }
        Ok(vertices)
    }

    fn roundtrip_mode(vertices: &[Vertex]) -> Result<u8, String> {
        let references: Vec<_> = vertices.iter().collect();
        let mut writer = Writer::new();
        write_vertex_storage(&mut writer, &references)?;
        let bytes = writer.into_bytes();
        let mode = bytes
            .get(4)
            .copied()
            .ok_or_else(|| "test storage omitted its mode".to_string())?;
        let mut reader = Reader::new(&bytes);
        let count = usize::try_from(reader.u32()?)
            .map_err(|error| format!("test vertex count is too large: {error}"))?;
        let decoded = read_vertex_storage(&mut reader, 6, count)?;
        assert!(
            reader.is_empty(),
            "vertex storage must consume its exact run"
        );
        assert_eq!(
            vertex_stream(&decoded),
            vertex_stream(vertices),
            "every reconstructed vertex must retain all 69 bytes in order"
        );
        Ok(mode)
    }

    // A hand-written palette run keeps malformed-reader controls independent
    // of the encoder's mode selection and payload sizing.
    fn palette_run(frames: &[Vertex], vertices: &[(Vertex, u16)]) -> Result<Vec<u8>, String> {
        let mut writer = Writer::new();
        writer.u8(1);
        writer.u32(
            u32::try_from(frames.len())
                .map_err(|error| format!("test palette is too large: {error}"))?,
        );
        for frame in frames {
            writer.f32_3(frame.normal);
            writer.f32_3(frame.tangent);
            writer.f32(frame.handedness);
        }
        for (vertex, frame_id) in vertices {
            writer.f32_3(vertex.pos);
            writer.f32_4(vertex.color);
            for component in vertex.uv {
                writer.f32(component);
            }
            for coordinate in vertex.lightmap {
                writer.u16(coordinate);
            }
            writer.u8(vertex.lightmap_page);
            writer.u16(*frame_id);
        }
        Ok(writer.into_bytes())
    }

    fn assert_rejected(bytes: &[u8], vertex_count: usize, expected: &str) -> Result<(), String> {
        let Err(error) = read_vertex_storage(&mut Reader::new(bytes), 6, vertex_count) else {
            return Err(format!("invalid palette was accepted: {expected}"));
        };
        assert!(
            error.contains(expected),
            "expected '{expected}', received '{error}'"
        );
        Ok(())
    }

    #[test]
    fn literal_mode_is_retained_for_unique_frames_and_exact_size_ties() -> Result<(), String> {
        let all_pairs = vertices_with_distinct_frames(11)?;
        let mut unique_frames: Vec<_> = all_pairs.iter().step_by(2).copied().collect();
        assert_eq!(
            roundtrip_mode(&unique_frames)?,
            VERTEX_MODE_LITERAL,
            "unique frames cost more as a palette"
        );
        let additional = *all_pairs
            .get(1)
            .ok_or_else(|| "test omitted its extra vertex".to_string())?;
        unique_frames.push(additional);
        assert_eq!(
            unique_frames.len(),
            12,
            "the tie control requires 12 vertices and 11 frames"
        );
        assert_eq!(
            roundtrip_mode(&unique_frames)?,
            VERTEX_MODE_LITERAL,
            "69*12 equals 4+28*11+43*12; retain literal mode on the tie"
        );
        Ok(())
    }

    #[test]
    fn u16_palette_retains_first_occurrence_frame_order_and_exact_bits() -> Result<(), String> {
        let vertices = vertices_with_distinct_frames(2)?;
        assert_eq!(
            roundtrip_mode(&vertices)?,
            1,
            "a smaller palette uses u16 indices"
        );
        let references: Vec<_> = vertices.iter().collect();
        let mut writer = Writer::new();
        write_vertex_storage(&mut writer, &references)?;
        let bytes = writer.into_bytes();
        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.u32()?, 4, "all four unique vertices remain");
        assert_eq!(reader.u8()?, 1, "this control uses mode 1");
        assert_eq!(reader.u32()?, 2, "only two distinct frames are stored");
        let stored_frames = reader.bytes(2_usize.saturating_mul(FRAME_BYTES))?;
        let mut expected_frames = Writer::new();
        for vertex in vertices.iter().step_by(2) {
            expected_frames.f32_3(vertex.normal);
            expected_frames.f32_3(vertex.tangent);
            expected_frames.f32(vertex.handedness);
        }
        assert_eq!(
            stored_frames,
            expected_frames.as_slice(),
            "frame order follows the first vertex carrying each exact frame"
        );
        for expected_id in [0_u16, 0, 1, 1] {
            let _literal = reader.bytes(VERTEX_LITERAL_BYTES)?;
            assert_eq!(
                reader.u16()?,
                expected_id,
                "each vertex retains its correct frame reference"
            );
        }
        assert!(
            reader.is_empty(),
            "the planned palette length must be exact"
        );
        Ok(())
    }

    #[test]
    fn frame_index_width_boundary_preserves_large_unreferenced_vertex_lists() -> Result<(), String>
    {
        for (frame_count, expected_mode) in [
            (usize::from(u16::MAX).saturating_add(1), 1),
            (usize::from(u16::MAX).saturating_add(2), 2),
        ] {
            let vertices = vertices_with_distinct_frames(frame_count)?;
            assert_eq!(
                roundtrip_mode(&vertices)?,
                expected_mode,
                "65,536 frames fit u16; 65,537 require u32"
            );
            let original = interning_tests::batch(vertices, vec![0, 1, 2])?;
            let (bytes, stats) = write_props_with_stats(std::slice::from_ref(&original))?;
            let decoded = read_props(&bytes)?;
            let decoded_batch = decoded
                .first()
                .ok_or_else(|| "large palette record omitted its batch".to_string())?;
            assert_eq!(
                vertex_stream(&decoded_batch.vertices),
                vertex_stream(&original.vertices),
                "all unique unreferenced vertices and frame bits must remain in order"
            );
            assert_eq!(
                decoded_batch.indices, original.indices,
                "the triangle indices stay u16 and unchanged"
            );
            assert_eq!(
                decoded_batch.submeshes, original.submeshes,
                "draw ranges and materials stay unchanged"
            );
            assert_eq!(
                stats.stored_vertex_slots,
                original.vertices.len(),
                "both distinct vertices per frame must remain"
            );
            assert_eq!(
                stats.duplicate_slots, 0,
                "frame sharing must not merge distinct vertices"
            );
        }
        Ok(())
    }

    #[test]
    fn unknown_modes_and_invalid_frame_counts_are_rejected_before_allocation() -> Result<(), String>
    {
        assert_rejected(&[3], 1, "unknown vertex mode 3")?;
        for (count, vertices, expected) in [
            (0_u32, 1_usize, "0 frames"),
            (2, 1, "2 frames for 1 vertices"),
            (65_537, 65_537, "more than 65,536 frames"),
            (16_777_217, 16_777_217, "prop frame count declares"),
        ] {
            let mut writer = Writer::new();
            writer.u8(1);
            writer.u32(count);
            assert_rejected(writer.as_slice(), vertices, expected)?;
        }
        Ok(())
    }

    #[test]
    fn out_of_range_frame_indices_and_truncated_runs_are_rejected() -> Result<(), String> {
        let invalid_index = palette_run(&[Vertex::UNLIT], &[(Vertex::UNLIT, 1)])?;
        assert_rejected(&invalid_index, 1, "frame index outside its palette")?;
        let valid = palette_run(&[Vertex::UNLIT], &[(Vertex::UNLIT, 0)])?;
        for length in 0..valid.len() {
            let truncated = valid
                .get(..length)
                .ok_or_else(|| "test truncation length is invalid".to_string())?;
            let Err(_error) = read_vertex_storage(&mut Reader::new(truncated), 6, 1) else {
                return Err(format!("truncated palette was accepted at {length} bytes"));
            };
        }
        Ok(())
    }

    #[test]
    fn non_finite_frames_are_rejected_even_when_unused() -> Result<(), String> {
        for invalid_frame in [
            Vertex {
                normal: [f32::NAN, 0.0, 1.0],
                ..Vertex::UNLIT
            },
            Vertex {
                tangent: [1.0, f32::INFINITY, 0.0],
                ..Vertex::UNLIT
            },
            Vertex {
                handedness: f32::NEG_INFINITY,
                ..Vertex::UNLIT
            },
        ] {
            let bytes = palette_run(
                &[Vertex::UNLIT, invalid_frame],
                &[(Vertex::UNLIT, 0), (Vertex::UNLIT, 0)],
            )?;
            assert_rejected(&bytes, 2, "frame palette has a non-finite component")?;
        }
        Ok(())
    }

    #[test]
    fn non_finite_literal_attributes_retain_the_neutral_vertex_guard() -> Result<(), String> {
        let invalid_vertex = Vertex {
            uv: [f32::INFINITY, 0.0],
            ..Vertex::UNLIT
        };
        let bytes = palette_run(&[Vertex::UNLIT], &[(invalid_vertex, 0)])?;
        assert_rejected(&bytes, 1, "vertex has a non-finite component")?;
        Ok(())
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;

    #[test]
    fn logical_literal_budget_accepts_exact_limit_and_rejects_one_byte_over() -> Result<(), String>
    {
        let mut budget = LogicalPropsBudget::new();
        budget.add_bytes(MAX_BINARY_BYTES.saturating_sub(10))?;
        assert_eq!(
            budget.bytes, MAX_BINARY_BYTES,
            "the existing exact 512MiB ceiling remains valid"
        );
        let Err(error) = budget.add_bytes(1) else {
            return Err("one byte above the logical literal budget was accepted".to_string());
        };
        assert!(
            error.contains("logical literal storage"),
            "the rejection must identify expanded allocation safety"
        );
        assert_eq!(
            budget.bytes, MAX_BINARY_BYTES,
            "a failed charge must not change the budget"
        );
        Ok(())
    }

    #[test]
    fn logical_literal_budget_is_cumulative_and_checks_declared_run_arithmetic()
    -> Result<(), String> {
        let mut budget = LogicalPropsBudget::new();
        budget.add_slots(3_000_000, 69, 0)?;
        budget.add_slots(3_000_000, 69, 0)?;
        let Err(error) = budget.add_slots(2_000_000, 69, 0) else {
            return Err("separate batches bypassed the cumulative budget".to_string());
        };
        assert!(
            error.contains("logical literal storage"),
            "individual runs cannot reset the record budget"
        );
        let mut overflow_budget = LogicalPropsBudget::new();
        let Err(overflow_error) = overflow_budget.add_slots(usize::MAX, u64::MAX, 4) else {
            return Err("declared logical byte multiplication overflow was accepted".to_string());
        };
        assert!(
            overflow_error.contains("overflows"),
            "declared byte arithmetic must be checked"
        );
        Ok(())
    }

    #[test]
    fn large_declared_vertex_count_fails_before_storage_or_vertex_allocation() -> Result<(), String>
    {
        let mut writer = Writer::new();
        writer.bytes(&PROPS_MAGIC);
        writer.u16(PROPS_RECORD_VERSION);
        writer.u32(1);
        writer.str("models/budget.glb")?;
        writer.u8(1);
        writer.f32_3([0.0; 3]);
        writer.f32_3([1.0; 3]);
        writer.u32(0);
        writer.u32(
            u32::try_from(MAX_PROP_BATCH_VERTICES)
                .map_err(|error| format!("test vertex count is too large: {error}"))?,
        );
        // Deliberately omit even the mode byte: the logical bound must fail
        // from the valid declared count before storage parsing or allocation.
        let Err(error) = read_props(writer.as_slice()) else {
            return Err("a header-only oversized logical record was accepted".to_string());
        };
        assert!(
            error.contains("logical literal storage"),
            "reject the declared expansion before reading missing storage"
        );
        Ok(())
    }

    #[test]
    fn declared_index_run_fails_logical_budget_before_reading_or_allocating() -> Result<(), String>
    {
        let mut writer = Writer::new();
        writer.u32(3);
        let mut budget = LogicalPropsBudget {
            bytes: MAX_BINARY_BYTES.saturating_sub(4),
        };
        let Err(error) = read_prop_indices(&mut Reader::new(writer.as_slice()), &mut budget) else {
            return Err("an index declaration bypassed the logical budget".to_string());
        };
        assert!(
            error.contains("logical literal storage"),
            "reject before reading the deliberately absent index payload"
        );
        Ok(())
    }

    #[test]
    fn encoded_record_budget_has_the_same_exact_boundary() -> Result<(), String> {
        let limit = usize::try_from(MAX_BINARY_BYTES)
            .map_err(|error| format!("test record limit is too large: {error}"))?;
        check_encoded_size(limit)?;
        let Err(error) = check_encoded_size(limit.saturating_add(1)) else {
            return Err("an encoded prop record above 512MiB was accepted".to_string());
        };
        assert!(
            error.contains("encoded record"),
            "the encoded typed record bound also remains explicit"
        );
        Ok(())
    }

    #[test]
    fn logical_budget_equals_the_independent_v5_literal_record_size() -> Result<(), String> {
        let original = interning_tests::batch(vec![Vertex::UNLIT; 4], vec![0, 1, 2])?;
        let bytes = write_props(std::slice::from_ref(&original))?;
        let mut reader = Reader::new(&bytes);
        let _header = reader.bytes(10)?;
        let mut budget = LogicalPropsBudget::new();
        let decoded = read_batch(&mut reader, PROPS_RECORD_VERSION, &mut budget)?;
        let literal_bytes = legacy_tests::literal_record(&decoded, 5)?;
        assert!(
            reader.is_empty(),
            "the entire one-batch record must be accounted for"
        );
        assert_eq!(
            budget.bytes,
            u64::try_from(literal_bytes.len())
                .map_err(|error| format!("test literal size is too large: {error}"))?,
            "charge the exact v5 model, optional-slot, material, vertex and index headers"
        );
        assert_eq!(
            decoded.vertices.len(),
            1,
            "the logical charge follows complete-vertex interning, before frame compression"
        );
        Ok(())
    }
}

#[cfg(test)]
mod legacy_tests {
    use super::*;

    // Write the historical literal layouts directly. Their fixtures must not
    // be made by deleting fields from the current palette-capable record.
    pub(super) fn literal_record(batch: &PropMeshBatch, version: u16) -> Result<Vec<u8>, String> {
        let mut writer = Writer::new();
        writer.bytes(&PROPS_MAGIC);
        writer.u16(version);
        writer.u32(1);
        writer.str(&batch.model)?;
        if version >= 5 {
            writer.u8(u8::from(batch.casts_static_lighting));
        }
        writer.f32_3(batch.bounds.min);
        writer.f32_3(batch.bounds.max);
        writer.u32(
            u32::try_from(batch.submeshes.len())
                .map_err(|error| format!("legacy test has too many submeshes: {error}"))?,
        );
        for submesh in &batch.submeshes {
            write_optional_slot(&mut writer, submesh.texture.map(u32::from))?;
            writer.u8(alpha_mode_code(submesh.alpha));
            writer.f32(submesh.alpha.cutoff);
            writer.f32_3(submesh.emission.color);
            writer.f32(submesh.emission.intensity);
            write_optional_slot(&mut writer, submesh.emission.mask)?;
            if version >= 4 {
                writer.f32_3(submesh.response.specular);
                writer.f32(submesh.response.roughness);
            }
            writer.u32(submesh.first_index);
            writer.u32(submesh.index_count);
        }
        writer.u32(
            u32::try_from(batch.vertices.len())
                .map_err(|error| format!("legacy test has too many vertices: {error}"))?,
        );
        for vertex in &batch.vertices {
            write_vertex(&mut writer, vertex);
        }
        writer.u16s(&batch.indices)?;
        Ok(writer.into_bytes())
    }

    fn caster_batch(casts_static_lighting: bool) -> PropMeshBatch {
        PropMeshBatch {
            model: "models/caster.glb".to_string(),
            source_primitives: Vec::new(),
            casts_static_lighting,
            textures: Vec::new(),
            submeshes: vec![PropSubmeshBatch {
                texture: None,
                alpha: MaterialAlpha::OPAQUE,
                emission: MaterialEmission::NONE,
                response: crate::materials::MaterialResponse::NONE,
                first_index: 0,
                index_count: 3,
            }],
            vertices: vec![
                crate::render::Vertex::new([0.0, 0.0, 0.0], [1.0; 4], [0.0, 0.0]),
                crate::render::Vertex::new([1.0, 0.0, 0.0], [1.0; 4], [1.0, 0.0]),
                crate::render::Vertex::new([0.0, 1.0, 0.0], [1.0; 4], [0.0, 1.0]),
            ],
            indices: vec![0, 1, 2],
            bounds: Aabb {
                min: [0.0; 3],
                max: [1.0, 1.0, 0.0],
            },
        }
    }

    #[test]
    fn static_caster_metadata_round_trips_and_rejects_invalid_markers() -> Result<(), String> {
        for casts_static_lighting in [false, true] {
            let batch = caster_batch(casts_static_lighting);
            let mut bytes = write_props(std::slice::from_ref(&batch))?;
            let decoded = read_props(&bytes)?;
            assert_eq!(decoded.len(), 1);
            let decoded_batch = decoded
                .first()
                .ok_or_else(|| "caster roundtrip omitted its batch".to_string())?;
            assert_eq!(decoded_batch.casts_static_lighting, casts_static_lighting);
            assert_eq!(decoded_batch.vertices, batch.vertices);
            assert_eq!(decoded_batch.indices, batch.indices);
            assert_eq!(write_props(&decoded)?, bytes);
            let marker_offset = 10 + 4 + batch.model.len();
            *bytes
                .get_mut(marker_offset)
                .ok_or_else(|| "caster roundtrip omitted its marker".to_string())? = 2;
            let Err(error) = read_props(&bytes) else {
                return Err("invalid caster marker was accepted".to_string());
            };
            assert!(error.contains("invalid static-caster marker 2"));
        }
        Ok(())
    }

    #[test]
    fn legacy_prop_records_default_to_static_casters() -> Result<(), String> {
        let batch = caster_batch(false);
        let legacy_v4 = literal_record(&batch, 4)?;
        let decoded_v4 = read_props(&legacy_v4)?;
        let decoded_v4_batch = decoded_v4
            .first()
            .ok_or_else(|| "legacy v4 record omitted its batch".to_string())?;
        assert!(decoded_v4_batch.casts_static_lighting);
        assert_eq!(decoded_v4_batch.vertices, batch.vertices);
        assert_eq!(decoded_v4_batch.indices, batch.indices);

        let legacy_v3 = literal_record(&batch, 3)?;
        let decoded_v3 = read_props(&legacy_v3)?;
        let decoded_v3_batch = decoded_v3
            .first()
            .ok_or_else(|| "legacy v3 record omitted its batch".to_string())?;
        assert!(decoded_v3_batch.casts_static_lighting);
        assert_eq!(decoded_v3_batch.vertices, batch.vertices);
        assert_eq!(decoded_v3_batch.indices, batch.indices);
        let submesh = decoded_v3_batch
            .submeshes
            .first()
            .ok_or_else(|| "legacy v3 record omitted its submesh".to_string())?;
        assert_eq!(submesh.response.specular, [0.0; 3]);
        assert_eq!(submesh.response.roughness.to_bits(), 1.0_f32.to_bits());
        Ok(())
    }

    #[test]
    fn legacy_v5_literal_vertices_and_caster_flags_remain_exact() -> Result<(), String> {
        for casts_static_lighting in [false, true] {
            let mut batch = caster_batch(casts_static_lighting);
            batch.vertices.extend_from_within(..3);
            batch.indices = vec![3, 4, 5];
            let bytes = literal_record(&batch, 5)?;
            let decoded = read_props(&bytes)?;
            let decoded_batch = decoded
                .first()
                .ok_or_else(|| "legacy v5 record omitted its batch".to_string())?;
            assert_eq!(
                decoded_batch.casts_static_lighting, casts_static_lighting,
                "v5 stores its explicit caster flag"
            );
            assert_eq!(
                decoded_batch.vertices, batch.vertices,
                "legacy decoding retains every literal vertex slot"
            );
            assert_eq!(
                decoded_batch.indices, batch.indices,
                "legacy decoding retains its original indices"
            );
            assert_eq!(
                decoded_batch.submeshes, batch.submeshes,
                "v5 material and draw metadata remain exact"
            );
        }
        Ok(())
    }

    #[test]
    fn legacy_v3_submesh_keeps_alpha_emission_and_explicit_matte_response() -> Result<(), String> {
        let mut writer = Writer::new();
        write_optional_slot(&mut writer, Some(2))?;
        writer.u8(alpha_mode_code(MaterialAlpha::blend(1.0)));
        writer.f32(0.35);
        writer.f32_3([0.5, 0.25, 0.125]);
        writer.f32(2.0);
        write_optional_slot(&mut writer, Some(1))?;
        writer.u32(6);
        writer.u32(3);
        let bytes = writer.into_bytes();
        let mut reader = Reader::new(&bytes);
        let decoded = read_submesh(&mut reader, 3)?;
        assert!(reader.is_empty());
        assert_eq!(decoded.texture, Some(2));
        assert_eq!(decoded.alpha.mode, crate::materials::AlphaMode::Blend);
        assert_eq!(decoded.emission.color, [0.5, 0.25, 0.125]);
        assert_eq!(decoded.emission.intensity.to_bits(), 2.0_f32.to_bits());
        assert_eq!(decoded.emission.mask, Some(1));
        assert_eq!(decoded.response.specular, [0.0; 3]);
        assert_eq!(decoded.response.roughness.to_bits(), 1.0_f32.to_bits());
        assert_eq!((decoded.first_index, decoded.index_count), (6, 3));
        Ok(())
    }
}
