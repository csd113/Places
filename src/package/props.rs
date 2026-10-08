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
//!   vertices    u32 count, then count x 69-byte neutral vertex
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
//!   specular     f32 x 3 (linear)
//!   roughness    f32
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
/// authoring `alphaMode: "BLEND"`); version 4 adds scalar material response;
/// version 5 preserves whether a model's retained bind pose is a static caster.
/// Versions 3 and 4 retain the conservative static-caster default, and version 3
/// also retains the explicit matte material default. Older records are refused.
pub const PROPS_RECORD_VERSION: u16 = 5;

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
        writer.u8(u8::from(batch.casts_static_lighting));
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
            writer.f32_3(submesh.response.specular);
            writer.f32(submesh.response.roughness);
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
    if !matches!(version, 3 | 4 | PROPS_RECORD_VERSION) {
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
        batches.push(read_batch(&mut reader, version)?);
    }
    if !reader.is_empty() {
        return Err(format!(
            "props record has {} trailing bytes",
            reader.remaining()
        ));
    }
    Ok(batches)
}

fn read_batch(reader: &mut Reader<'_>, version: u16) -> Result<PropMeshBatch, String> {
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
mod legacy_tests {
    use super::*;

    fn caster_batch(casts_static_lighting: bool) -> PropMeshBatch {
        PropMeshBatch {
            model: "models/caster.glb".to_string(),
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
        let bytes = write_props(std::slice::from_ref(&batch))?;
        let marker_offset = 10 + 4 + batch.model.len();
        let mut legacy_v4 = bytes;
        let removed = legacy_v4.remove(marker_offset);
        assert_eq!(removed, 0);
        legacy_v4
            .get_mut(4..6)
            .ok_or_else(|| "legacy v4 record omitted its version".to_string())?
            .copy_from_slice(&4_u16.to_le_bytes());
        let decoded_v4 = read_props(&legacy_v4)?;
        let decoded_v4_batch = decoded_v4
            .first()
            .ok_or_else(|| "legacy v4 record omitted its batch".to_string())?;
        assert!(decoded_v4_batch.casts_static_lighting);
        assert_eq!(decoded_v4_batch.vertices, batch.vertices);
        assert_eq!(decoded_v4_batch.indices, batch.indices);

        // The v3 submesh omits the 16-byte scalar response after emission.
        let response_offset = marker_offset + 24 + 4 + 1 + 1 + 4 + 16 + 1;
        let mut legacy_v3 = legacy_v4;
        drop(legacy_v3.drain(response_offset..response_offset + 16));
        legacy_v3
            .get_mut(4..6)
            .ok_or_else(|| "legacy v3 record omitted its version".to_string())?
            .copy_from_slice(&3_u16.to_le_bytes());
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
