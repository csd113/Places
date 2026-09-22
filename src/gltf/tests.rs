//! Unit tests for the GLB reader.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::indexing_slicing
)]

use super::*;

/// A real, shipped prop asset: the parser must accept what the toolkit writes.
const CHAIR_GLB: &[u8] = include_bytes!("../../assets/environment/office/props/models/chair.glb");
/// 1x1 opaque PNG used by the synthetic fixtures below.
const PIXEL_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 218, 99, 56, 81, 17, 245, 31, 0, 6,
    64, 2, 154, 192, 122, 5, 31, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

fn glb_container(json: &str, binary: &[u8]) -> Vec<u8> {
    let mut json_chunk = json.as_bytes().to_vec();
    while !json_chunk.len().is_multiple_of(4) {
        json_chunk.push(b' ');
    }
    let mut bin_chunk = binary.to_vec();
    while !bin_chunk.len().is_multiple_of(4) {
        bin_chunk.push(0);
    }
    let total = 12 + 8 + json_chunk.len() + 8 + bin_chunk.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(&GLB_MAGIC.to_le_bytes());
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&u32::try_from(total).unwrap_or(u32::MAX).to_le_bytes());
    out.extend_from_slice(
        &u32::try_from(json_chunk.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    out.extend_from_slice(&CHUNK_JSON.to_le_bytes());
    out.extend_from_slice(&json_chunk);
    out.extend_from_slice(
        &u32::try_from(bin_chunk.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    out.extend_from_slice(&CHUNK_BIN.to_le_bytes());
    out.extend_from_slice(&bin_chunk);
    out
}

/// A one-triangle GLB with 16-bit indices, normalised byte colours and an
/// embedded PNG, i.e. the narrow profile the toolkit emits.
fn minimal_triangle_glb() -> Vec<u8> {
    let (json, binary) = minimal_triangle_parts();
    glb_container(&json, &binary)
}

/// The JSON and binary chunks used by [`minimal_triangle_glb`], exposed so
/// tests can mutate one part of the document.
fn minimal_triangle_parts() -> (String, Vec<u8>) {
    let mut binary = Vec::new();
    let positions: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    for position in positions {
        for value in position {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    while binary.len() % 4 != 0 {
        binary.push(0);
    }
    let uv_offset = binary.len();
    for uv in [[0.0f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
        for value in uv {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let color_offset = binary.len();
    for color in [[255u8, 128, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]] {
        binary.extend_from_slice(&color);
    }
    while binary.len() % 2 != 0 {
        binary.push(0);
    }
    let index_offset = binary.len();
    for index in [0u16, 1, 2] {
        binary.extend_from_slice(&index.to_le_bytes());
    }
    let image_offset = binary.len();
    binary.extend_from_slice(PIXEL_PNG);

    let json = format!(
        r#"{{
          "asset": {{"version": "2.0"}},
          "scene": 0,
          "scenes": [{{"nodes": [0]}}],
          "nodes": [{{"mesh": 0}}],
          "meshes": [{{"primitives": [{{"attributes": {{"POSITION": 0, "TEXCOORD_0": 1, "COLOR_0": 2}}, "indices": 3, "material": 0, "mode": 4}}]}}],
          "accessors": [
            {{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"}},
            {{"bufferView": 1, "componentType": 5126, "count": 3, "type": "VEC2"}},
            {{"bufferView": 2, "componentType": 5121, "normalized": true, "count": 3, "type": "VEC4"}},
            {{"bufferView": 3, "componentType": 5123, "count": 3, "type": "SCALAR"}}
          ],
          "bufferViews": [
            {{"buffer": 0, "byteOffset": 0, "byteLength": 36, "target": 34962}},
            {{"buffer": 0, "byteOffset": {uv_offset}, "byteLength": 24, "target": 34962}},
            {{"buffer": 0, "byteOffset": {color_offset}, "byteLength": 12, "target": 34962}},
            {{"buffer": 0, "byteOffset": {index_offset}, "byteLength": 6, "target": 34963}},
            {{"buffer": 0, "byteOffset": {image_offset}, "byteLength": {png_length}}}
          ],
          "buffers": [{{"byteLength": {buffer_length}}}],
          "images": [{{"bufferView": 4, "mimeType": "image/png"}}],
          "samplers": [{{"magFilter": 9729, "minFilter": 9987, "wrapS": 33071, "wrapT": 33071}}],
          "textures": [{{"sampler": 0, "source": 0}}],
          "materials": [{{"pbrMetallicRoughness": {{"baseColorTexture": {{"index": 0}}}}}}]
        }}"#,
        png_length = PIXEL_PNG.len(),
        buffer_length = binary.len()
    );
    (json, binary)
}

#[test]
fn parses_the_shipped_chair_asset() {
    let model = parse_glb(CHAIR_GLB).expect("shipped chair.glb must parse");
    assert!(model.triangles > 0);
    assert_eq!(model.indices.len(), model.triangles * 3);
    assert!(model.texture.width.is_power_of_two());
    assert_eq!(model.texture.width, model.texture.height);
    assert!(model.texture.width <= crate::level::MAX_PROP_TEXTURE_SIZE);
    assert_eq!(
        model.texture.rgba.len(),
        (model.texture.width as usize) * (model.texture.height as usize) * 4
    );

    // Origin convention: base on y = 0, horizontally centred, metres.
    let (low, high) = model.bounds().expect("chair has vertices");
    assert!(low[1].abs() < 0.012, "chair base sits at {}", low[1]);
    assert!(f32::midpoint(low[0], high[0]).abs() < 0.02);
    assert!(f32::midpoint(low[2], high[2]).abs() < 0.02);
    assert!((high[0] - low[0] - 0.5).abs() < 0.05);
    assert!((high[1] - low[1] - 0.9).abs() < 0.05);
}

#[test]
fn parses_normalised_colours_and_16_bit_indices() {
    let model = parse_glb(&minimal_triangle_glb()).expect("synthetic triangle parses");
    assert_eq!(model.triangles, 1);
    assert_eq!(model.vertices.len(), 3);
    assert!((model.vertices[0].color[0] - 1.0).abs() < 1e-6);
    assert!((model.vertices[0].color[1] - 128.0 / 255.0).abs() < 1e-3);
    assert!((model.vertices[1].color[1] - 1.0).abs() < 1e-6);
    assert_eq!(model.texture.width, 1);
}

#[test]
fn rejects_malformed_assets_with_actionable_messages() {
    let json_cases: [(&str, &str, &str); 4] = [
        (
            "empty mesh list",
            r#"{"asset":{"version":"2.0"},"meshes":[]}"#,
            "mesh",
        ),
        (
            "unsupported extension",
            r#"{"asset":{"version":"2.0"},"extensionsUsed":["KHR_materials_clearcoat"],"meshes":[{"primitives":[]}]}"#,
            "extensions",
        ),
        (
            "skinned mesh",
            r#"{"asset":{"version":"2.0"},"skins":[{}],"meshes":[{"primitives":[]}]}"#,
            "skinned",
        ),
        (
            "animated asset",
            r#"{"asset":{"version":"2.0"},"animations":[{}],"meshes":[{"primitives":[]}]}"#,
            "animated",
        ),
    ];
    for (label, json, expected) in json_cases {
        let bytes = glb_container(json, b"\0\0\0\0");
        let error = parse_glb(&bytes).expect_err(&format!("{label} must not parse"));
        assert!(
            error.0.to_lowercase().contains(expected),
            "{label}: message {:?} should mention {expected:?}",
            error.0
        );
    }

    let raw_cases: [(&str, &[u8]); 2] = [
        ("not a GLB at all", b"hello world, definitely not a model"),
        ("empty file", &[]),
    ];
    for (label, payload) in raw_cases {
        let error = parse_glb(payload).expect_err(&format!("{label} must not parse"));
        assert!(
            error.0.contains("GLB"),
            "{label}: message {:?} should mention GLB",
            error.0
        );
    }
}

#[test]
fn rejects_unsupported_primitive_shapes() {
    let json =
        r#"{"asset":{"version":"2.0"},"meshes":[{"primitives":[{"attributes":{},"mode":1}]}]}"#;
    let error = parse_glb(&glb_container(json, b"\0\0\0\0")).expect_err("mode 1 is not triangles");
    assert!(error.0.contains("TRIANGLES"), "{}", error.0);

    let (mut json, binary) = minimal_triangle_parts();
    json = json.replace("\"TEXCOORD_0\": 1, ", "");
    let error = parse_glb(&glb_container(&json, &binary)).expect_err("a prop needs UVs");
    assert!(
        error.0.to_lowercase().contains("uv"),
        "a prop without UVs must say so: {}",
        error.0
    );
}

#[test]
fn rejects_truncated_and_oversized_containers() {
    let mut truncated = minimal_triangle_glb();
    truncated.truncate(truncated.len() - 40);
    assert!(parse_glb(&truncated).is_err());

    let mut declared_too_long = minimal_triangle_glb();
    declared_too_long[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    let error = parse_glb(&declared_too_long).expect_err("oversized header");
    assert!(error.0.contains("exceeds"), "{}", error.0);
}
