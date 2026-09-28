//! SHA-256 helpers for package integrity and content-addressed blob names.

use sha2::{Digest, Sha256};

/// Lowercase hex SHA-256 of `bytes`.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut text = String::with_capacity(64);
    for byte in digest {
        text.push(char::from_digit(u32::from(byte >> 4), 16).unwrap_or('0'));
        text.push(char::from_digit(u32::from(byte & 0x0f), 16).unwrap_or('0'));
    }
    text
}

/// The content-addressed blob name for `bytes` with the given `suffix`
/// (including its leading dot, e.g. `.mesh`).
#[must_use]
pub fn blob_name(bytes: &[u8], suffix: &str) -> String {
    format!("blobs/{}{suffix}", sha256_hex(bytes))
}

/// The SHA-256 hex prefix embedded in a `blobs/<hex><suffix>` name.
#[must_use]
pub fn sha256_from_blob_name(name: &str) -> Option<String> {
    let rest = name.strip_prefix("blobs/")?;
    let (hex, _suffix) = rest.split_once('.')?;
    if hex.len() != 64 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some(hex.to_ascii_lowercase())
}

/// SHA-256 of a file's bytes, as lowercase hex.
///
/// # Errors
/// Returns an error when the input is malformed, out of bounds or unsupported.
pub fn sha256_file(path: &std::path::Path) -> Result<String, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("could not read {}: {error}", path.display()))?;
    Ok(sha256_hex(&bytes))
}
