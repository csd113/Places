//! Deterministic JSON for content identities and packaged semantic records.
//!
//! `serde_json::to_vec` serializes a struct in field order but a `HashMap`
//! field in *iteration* order, so two runs over identical content can produce
//! different bytes. Every identity that must be stable — the lightmap cache
//! key and the package's `semantics.json` — goes through this helper, which
//! converts to a `serde_json::Value` first; that representation stores object
//! keys in sorted order, so the bytes are canonical.

/// Serializes `value` as compact JSON with object keys in sorted order.
///
/// # Errors
///
/// Returns an error when the value cannot be represented as JSON (for example
/// a non-finite float).
pub fn canonical_json_bytes<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let value = serde_json::to_value(value).map_err(|error| error.to_string())?;
    serde_json::to_vec(&value).map_err(|error| error.to_string())
}
