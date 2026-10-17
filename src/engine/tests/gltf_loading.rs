//! Tiny malformed-input regressions; no shipped assets, map baking or GPU.

use super::{
    CHUNK_BIN, CHUNK_JSON, GLB_MAGIC, MAX_PROP_VERTICES, parse_container, validate_primitive_counts,
};

fn glb(chunks: &[(u32, &[u8])]) -> Result<Vec<u8>, String> {
    let mut bytes = GLB_MAGIC.to_le_bytes().to_vec();
    bytes.extend_from_slice(&2_u32.to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    for (kind, payload) in chunks {
        let padded = payload.len().div_ceil(4).saturating_mul(4);
        bytes.extend_from_slice(
            &u32::try_from(padded)
                .map_err(|error| error.to_string())?
                .to_le_bytes(),
        );
        bytes.extend_from_slice(&kind.to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes.resize(
            bytes
                .len()
                .saturating_add(padded.saturating_sub(payload.len())),
            b' ',
        );
    }
    let length = u32::try_from(bytes.len()).map_err(|error| error.to_string())?;
    bytes
        .get_mut(8..12)
        .ok_or("missing GLB header length")?
        .copy_from_slice(&length.to_le_bytes());
    Ok(bytes)
}

#[test]
fn glb_container_rejects_ambiguous_and_foreign_payloads() -> Result<(), String> {
    let json = br#"{"buffers":[{"byteLength":1}],"bufferViews":[{"buffer":0,"byteLength":1}]}"#;
    let valid = glb(&[(CHUNK_JSON, json), (CHUNK_BIN, &[7])])?;
    let (_, binary) = parse_container(&valid).map_err(|error| error.to_string())?;
    assert_eq!(
        binary,
        &[7],
        "the declared buffer excludes GLB alignment padding"
    );
    assert!(
        valid.as_ptr_range().contains(&binary.as_ptr()),
        "the binary chunk must borrow the input instead of copying it"
    );

    let mut trailing = valid.clone();
    trailing.push(0);
    let mut incomplete_header = trailing.clone();
    let incomplete_length =
        u32::try_from(incomplete_header.len()).map_err(|error| error.to_string())?;
    incomplete_header
        .get_mut(8..12)
        .ok_or("missing header")?
        .copy_from_slice(&incomplete_length.to_le_bytes());
    let cases = [
        ("trailing file bytes", trailing),
        ("truncated chunk header", incomplete_header),
        ("duplicate JSON", glb(&[(CHUNK_JSON, json), (CHUNK_JSON, json), (CHUNK_BIN, &[7])])?),
        ("duplicate BIN", glb(&[(CHUNK_JSON, json), (CHUNK_BIN, &[7]), (CHUNK_BIN, &[7])])?),
        ("BIN before JSON", glb(&[(CHUNK_BIN, &[7]), (CHUNK_JSON, json)])?),
        ("external buffer", glb(&[(CHUNK_JSON, br#"{"buffers":[{"byteLength":1,"uri":"elsewhere.bin"}]}"#), (CHUNK_BIN, &[7])])?),
        ("foreign buffer index", glb(&[(CHUNK_JSON, br#"{"buffers":[{"byteLength":1}],"bufferViews":[{"buffer":1,"byteLength":1}]}"#), (CHUNK_BIN, &[7])])?),
        ("buffer length mismatch", glb(&[(CHUNK_JSON, br#"{"buffers":[{"byteLength":5}]}"#), (CHUNK_BIN, &[7])])?),
    ];
    for (name, bytes) in cases {
        assert!(
            parse_container(&bytes).is_err(),
            "malformed GLB accepted: {name}"
        );
    }
    Ok(())
}

#[test]
fn primitive_counts_apply_existing_ceilings_before_reading_data() -> Result<(), String> {
    let json = serde_json::json!({"accessors":[
        {"count":MAX_PROP_VERTICES.saturating_add(1)}, {"count":4_usize}
    ]});
    let oversized = serde_json::json!({"attributes":{"POSITION":0_usize}});
    let vertex_error = validate_primitive_counts(&json, &oversized, 3)
        .err()
        .ok_or("oversized POSITION unexpectedly accepted")?;
    assert!(
        vertex_error.0.contains("POSITION"),
        "the vertex ceiling must precede any binary read"
    );
    let indices = serde_json::json!({"indices":1_usize});
    assert!(
        validate_primitive_counts(&json, &indices, 3).is_err(),
        "indices cannot exceed the remaining triangle budget"
    );
    assert!(
        validate_primitive_counts(&json, &indices, 4).is_ok(),
        "an accessor exactly at the remaining budget is accepted"
    );
    Ok(())
}
