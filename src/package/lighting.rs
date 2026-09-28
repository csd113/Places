//! Compiled lighting records: the bake result a player samples without baking.
//!
//! The record layout is defined next to the private types it encodes (see
//! `crate::lighting::bake` and `crate::lighting::visibility`); this module owns
//! the archive-facing entry point and its bounds.

use crate::lighting::LevelLighting;

use super::MAX_LIGHTING_BYTES;
use super::binary::Reader;

/// Version of the lighting record layout, owned by the bake module so the two
/// can never drift.
pub const LIGHTING_RECORD_VERSION: u16 = crate::lighting::LIGHTING_RECORD_VERSION;

/// Encodes a baked lighting record.
///
/// # Errors
/// Returns an error when the input is malformed, out of bounds or unsupported.
pub fn write_lighting(lighting: &LevelLighting) -> Result<Vec<u8>, String> {
    let mut writer = super::binary::Writer::new();
    lighting.write_compiled(&mut writer)?;
    let bytes = writer.into_bytes();
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_LIGHTING_BYTES {
        return Err(format!(
            "lighting record is {} bytes (limit {MAX_LIGHTING_BYTES})",
            bytes.len()
        ));
    }
    Ok(bytes)
}

/// Decodes a baked lighting record.
///
/// # Errors
/// Returns an error when the input is malformed, out of bounds or unsupported.
pub fn read_lighting(bytes: &[u8]) -> Result<LevelLighting, String> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_LIGHTING_BYTES {
        return Err(format!(
            "lighting record is {} bytes (limit {MAX_LIGHTING_BYTES})",
            bytes.len()
        ));
    }
    let mut reader = Reader::new(bytes);
    LevelLighting::read_compiled(&mut reader)
}
