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
//!   bounds_min  f32 x 3
//!   bounds_max  f32 x 3
//!   submesh_count u32
//!   submeshes   submesh_count x submesh
//!   vertices    u32 count, then count x 69-byte vertex
//!   indices     u32 count, then count x u16
//! ```
//!
//! ```text
//! submesh:
//!   texture      u8 (0 = none, 1 = Some) + u16 slot when present
//!   alpha_mode   u8 (0 = opaque, 1 = cutout, 2 = blend)
//!   alpha_cutoff f32
//!   emission     f32 x 3 colour, f32 intensity
//!   emission_mask u8 (0 = none, 1 = Some) + u16 slot when present
//!   first_index  u32
//!   index_count  u32
//! ```

use std::sync::Arc;

use crate::materials::{AlphaMode, MaterialAlpha, MaterialEmission};
use crate::render::{PropMeshBatch, PropSubmeshBatch};
use crate::spatial::Aabb;

use super::binary::{Reader, Writer, finite3};
use super::mesh::{read_vertex, write_vertex};
use super::{MAX_PROP_BATCH_VERTICES, MAX_PROP_BATCHES, MAX_PROP_SUBMESHES};

/// Version of the props record layout.
///
/// Version 2 added the per-submesh alpha contract (glTF `MASK` foliage draws
/// through the cutout pass); version 3 adds the blended mode (a model
/// authoring `alphaMode: "BLEND"`). An older record is refused and rebuilt.
pub const PROPS_RECORD_VERSION: u16 = 3;

/// Magic identifying a props record.
pub const PROPS_MAGIC: [u8; 4] = *b"PLMP";

/// Largest accepted model path length in a prop batch.
const MAX_MODEL_PATH: u64 = 1024;

/// Encodes prepared prop batches, omitting texture pixels by design.
///
/// # Errors
/// Returns an error when the input is malformed, out of bounds or unsupported.
pub fn write_props(batches: &[PropMeshBatch]) -> Result<Vec<u8>, String> {
    let count =
        u32::try_from(batches.len()).map_err(|error| format!("too many prop batches: {error}"))?;
    let capacity = batches.iter().fold(0_usize, |sum, batch| {
        sum.saturating_add(batch.vertices.len().saturating_mul(80))
    });
    let mut writer = Writer::with_capacity(capacity);
    writer.bytes(&PROPS_MAGIC);
    writer.u16(PROPS_RECORD_VERSION);
    writer.u32(count);
    for batch in batches {
        writer.str(&batch.model)?;
        writer.f32_3(batch.bounds.min);
        writer.f32_3(batch.bounds.max);
        let submeshes = u32::try_from(batch.submeshes.len())
            .map_err(|error| format!("prop batch has too many submeshes: {error}"))?;
        writer.u32(submeshes);
        for submesh in &batch.submeshes {
            write_optional_slot(&mut writer, submesh.texture.map(u32::from))?;
            writer.u8(alpha_mode_code(submesh.alpha));
            writer.f32(submesh.alpha.cutoff);
            writer.f32_3(submesh.emission.color);
            writer.f32(submesh.emission.intensity);
            write_optional_slot(&mut writer, submesh.emission.mask)?;
            writer.u32(submesh.first_index);
            writer.u32(submesh.index_count);
        }
        let vertices = u32::try_from(batch.vertices.len())
            .map_err(|error| format!("prop batch has too many vertices: {error}"))?;
        writer.u32(vertices);
        for vertex in &batch.vertices {
            write_vertex(&mut writer, vertex);
        }
        writer.u16s(&batch.indices)?;
    }
    Ok(writer.into_bytes())
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
    let mut batches = Vec::with_capacity(count);
    for _ in 0..count {
        batches.push(read_batch(&mut reader)?);
    }
    if !reader.is_empty() {
        return Err(format!(
            "props record has {} trailing bytes",
            reader.remaining()
        ));
    }
    Ok(batches)
}

fn read_batch(reader: &mut Reader<'_>) -> Result<PropMeshBatch, String> {
    let model = reader.str(MAX_MODEL_PATH)?;
    if model.is_empty() {
        return Err("prop batch has an empty model path".to_string());
    }
    if super::normalize_entry_name(&model).as_deref() != Some(model.as_str()) {
        return Err(format!(
            "prop model path '{model}' is not a safe relative path"
        ));
    }
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
    let vertex_count = u64::from(reader.u32()?);
    if vertex_count == 0 || vertex_count > MAX_PROP_BATCH_VERTICES {
        return Err(format!(
            "prop batch declares {vertex_count} vertices (limit {MAX_PROP_BATCH_VERTICES})"
        ));
    }
    let batch_vertex_count = usize::try_from(vertex_count)
        .map_err(|error| format!("prop batch vertex count is too large: {error}"))?;
    let mut vertices = Vec::with_capacity(batch_vertex_count.min(4096));
    for _ in 0..batch_vertex_count {
        vertices.push(read_vertex(reader)?);
    }
    let indices = reader.u16s(MAX_PROP_BATCH_VERTICES)?;
    if indices
        .iter()
        .any(|index| usize::from(*index) >= batch_vertex_count)
    {
        return Err("prop batch has an index outside its vertex list".to_string());
    }
    if indices.len() % 3 != 0 {
        return Err("prop batch index count is not a multiple of three".to_string());
    }
    for submesh in &submeshes {
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
    Ok(PropMeshBatch {
        model,
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
    Ok(PropSubmeshBatch {
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
