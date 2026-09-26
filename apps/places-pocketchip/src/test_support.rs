//! Exact-value assertion helpers shared by the unit tests.
//!
//! These tests check the numbers the code produces, not tolerances: a baked
//! shade, a level's dimensions or a camera setting must come out exactly as
//! documented. IEEE equality is therefore written out as a `partial_cmp`
//! (`-0.0` equals `0.0`, NaN equals nothing) instead of being loosened to an
//! epsilon, and determinism checks can compare bit patterns so even a one-ULP
//! drift between two runs fails the test.

// Test code: untypical-but-clear helpers; the production lints stay enforced
// everywhere else in the crate.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::unwrap_used
)]

use std::cmp::Ordering;
use std::fmt::Display;

/// Asserts `actual` is exactly `expected` under IEEE equality.
#[track_caller]
pub fn assert_exact(actual: f32, expected: f32) {
    assert!(
        actual.partial_cmp(&expected) == Some(Ordering::Equal),
        "expected {expected}, got {actual}"
    );
}

/// Asserts `actual` is exactly `expected`, naming the case if it is not.
#[track_caller]
pub fn assert_exact_named(actual: f32, expected: f32, context: impl Display) {
    assert!(
        actual.partial_cmp(&expected) == Some(Ordering::Equal),
        "{context}: expected {expected}, got {actual}"
    );
}

/// Asserts every component of `actual` is exactly `expected`.
#[track_caller]
pub fn assert_exact_array<const N: usize>(actual: [f32; N], expected: [f32; N]) {
    for (index, (actual, expected)) in actual.iter().zip(expected.iter()).enumerate() {
        assert!(
            actual.partial_cmp(expected) == Some(Ordering::Equal),
            "component {index}: expected {expected}, got {actual}"
        );
    }
}

/// Every `step` from `start` through `end`, both ends included.
///
/// The samples are produced by repeated addition, so a scan visits exactly the
/// values the accumulator-based loop it replaces did. The generator is used by
/// the continuity tests, which walk a surface in fixed increments and compare
/// each sample with the previous one.
pub fn scan(start: f32, step: f32, end: f32) -> impl Iterator<Item = f32> {
    scan_while(start, step, move |value| value <= end)
}

/// Every `step` from `start` up to, but not including, `end`.
pub fn scan_below(start: f32, step: f32, end: f32) -> impl Iterator<Item = f32> {
    scan_while(start, step, move |value| value < end)
}

fn scan_while(start: f32, step: f32, keep: impl Fn(f32) -> bool) -> impl Iterator<Item = f32> {
    let mut next = start;
    std::iter::from_fn(move || {
        let value = next;
        next += step;
        keep(value).then_some(value)
    })
}

// ---------------------------------------------------------------------------
// Test-only GLB construction
//
// The prop pipeline reads GLBs written by `tools/props`; these helpers build the
// same container shape by hand so the engine's multi-material and emissive paths
// can be exercised end-to-end without shipping a fixture asset. Files a test
// writes go under `target/agent-work/` (the repository's scratch area), never
// into `assets/`.
// ---------------------------------------------------------------------------

/// One material of a test GLB: its texture pixels and optional emission.
#[derive(Clone, Copy, Debug)]
pub struct TestGlbMaterial {
    /// Base colour of the embedded 2x2 texture.
    pub base: [u8; 4],
    /// `emissiveFactor`, or `None` for a non-emissive material.
    pub emissive: Option<[f32; 3]>,
    /// Whether the emissive mask uses the material's own texture.
    pub emissive_mask: bool,
}

impl TestGlbMaterial {
    /// A plain textured material with no emission.
    #[must_use]
    pub const fn plain(base: [u8; 4]) -> Self {
        Self {
            base,
            emissive: None,
            emissive_mask: false,
        }
    }

    /// A textured material that also emits, optionally through its own texture.
    #[must_use]
    pub const fn emissive(base: [u8; 4], factor: [f32; 3], masked: bool) -> Self {
        Self {
            base,
            emissive: Some(factor),
            emissive_mask: masked,
        }
    }
}

/// Builds a self-contained two-primitive, two-material GLB.
///
/// Each primitive is an axis-aligned 1x1 quad in the X/Z plane, one after the
/// other along +Z, so a test can tell the two materials apart by index range
/// alone. The second material is the one that carries `second`.
#[must_use]
pub fn two_material_glb(first: TestGlbMaterial, second: TestGlbMaterial) -> Vec<u8> {
    const FLOAT: u32 = 5126;
    const UNSIGNED_SHORT: u32 = 5123;

    let mut binary: Vec<u8> = Vec::new();
    let mut views: Vec<String> = Vec::new();
    let mut accessors: Vec<String> = Vec::new();

    let mut add_view = |binary: &mut Vec<u8>, payload: &[u8], target: Option<u32>| -> usize {
        while !binary.len().is_multiple_of(4) {
            binary.push(0);
        }
        let offset = binary.len();
        binary.extend_from_slice(payload);
        let target = target.map_or(String::new(), |target| format!(r#","target":{target}"#));
        views.push(format!(
            r#"{{"buffer":0,"byteOffset":{offset},"byteLength":{length}{target}}}"#,
            length = payload.len()
        ));
        views.len().saturating_sub(1)
    };
    let add_accessor = |accessors: &mut Vec<String>,
                        view: usize,
                        component_type: u32,
                        count: usize,
                        kind: &str| {
        accessors.push(format!(
            r#"{{"bufferView":{view},"componentType":{component_type},"count":{count},"type":"{kind}"}}"#
        ));
        accessors.len().saturating_sub(1)
    };

    // Quad 0 occupies z in 0..1, quad 1 z in 1..2; both span x in 0..1.
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u16> = Vec::new();
    for quad in 0..2 {
        let z0 = f32::from(u8::try_from(quad).unwrap_or(0));
        let z1 = z0 + 1.0;
        let base = u16::try_from(positions.len()).unwrap_or(0);
        positions.extend_from_slice(&[
            [0.0, 0.0, z0],
            [1.0, 0.0, z0],
            [1.0, 0.0, z1],
            [0.0, 0.0, z1],
        ]);
        uvs.extend_from_slice(&[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let mut position_bytes = Vec::new();
    for position in &positions {
        for value in position {
            position_bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    let position_view = add_view(&mut binary, &position_bytes, Some(34962));
    let position_accessor = add_accessor(
        &mut accessors,
        position_view,
        FLOAT,
        positions.len(),
        "VEC3",
    );

    let mut uv_bytes = Vec::new();
    for uv in &uvs {
        for value in uv {
            uv_bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    let uv_view = add_view(&mut binary, &uv_bytes, Some(34962));
    let uv_accessor = add_accessor(&mut accessors, uv_view, FLOAT, uvs.len(), "VEC2");

    let mut index_bytes = Vec::new();
    for index in &indices {
        index_bytes.extend_from_slice(&index.to_le_bytes());
    }
    let index_view = add_view(&mut binary, &index_bytes, Some(34963));
    let index_accessor = add_accessor(
        &mut accessors,
        index_view,
        UNSIGNED_SHORT,
        indices.len(),
        "SCALAR",
    );

    // One 2x2 PNG per material, encoded through the engine's own encoder.
    let mut images: Vec<String> = Vec::new();
    let mut textures: Vec<String> = Vec::new();
    for material in [first, second] {
        let base: [u8; 4] = material.base;
        let pixels: Vec<u8> = base.iter().copied().cycle().take(2 * 2 * 4).collect();
        let image = crate::materials::RawImage::new(2, 2, pixels);
        let png = crate::materials::encode_png(&image).unwrap_or_default();
        let view = add_view(&mut binary, &png, None);
        images.push(format!(r#"{{"bufferView":{view},"mimeType":"image/png"}}"#));
        textures.push(format!(
            r#"{{"source":{}}}"#,
            images.len().saturating_sub(1)
        ));
    }

    let material_json = |spec: TestGlbMaterial, texture: usize| -> String {
        let mut material = format!(
            r#"{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":{texture}}},"metallicFactor":0.0,"roughnessFactor":1.0}}"#
        );
        if let Some([red, green, blue]) = spec.emissive {
            use std::fmt::Write as _;
            let _ = write!(material, r#","emissiveFactor":[{red},{green},{blue}]"#);
            if spec.emissive_mask {
                let _ = write!(material, r#","emissiveTexture":{{"index":{texture}}}"#);
            }
        }
        material.push('}');
        material
    };

    let json = format!(
        r#"{{
          "asset": {{"version": "2.0"}},
          "scene": 0,
          "scenes": [{{"nodes": [0]}}],
          "nodes": [{{"mesh": 0}}],
          "meshes": [{{"primitives": [
            {{"attributes": {{"POSITION": {position_accessor}, "TEXCOORD_0": {uv_accessor}}}, "indices": {index_accessor}, "material": 0}},
            {{"attributes": {{"POSITION": {position_accessor}, "TEXCOORD_0": {uv_accessor}}}, "indices": {index_accessor}, "material": 1}}
          ]}}],
          "accessors": [{accessors}],
          "bufferViews": [{views}],
          "buffers": [{{"byteLength": {length}}}],
          "images": [{images}],
          "textures": [{textures}],
          "materials": [{first}, {second}]
        }}"#,
        accessors = accessors.join(","),
        views = views.join(","),
        images = images.join(","),
        textures = textures.join(","),
        first = material_json(first, 0),
        second = material_json(second, 1),
        length = binary.len(),
    );
    glb_container(&json, &binary)
}

/// Wraps a JSON chunk and a binary chunk in a glTF 2.0 GLB container.
#[must_use]
pub fn glb_container(json: &str, binary: &[u8]) -> Vec<u8> {
    let mut json_bytes = json.as_bytes().to_vec();
    while !json_bytes.len().is_multiple_of(4) {
        json_bytes.push(b' ');
    }
    let mut binary_bytes = binary.to_vec();
    while !binary_bytes.len().is_multiple_of(4) {
        binary_bytes.push(0);
    }
    let total = 12_u32
        .saturating_add(8)
        .saturating_add(u32::try_from(json_bytes.len()).unwrap_or(u32::MAX))
        .saturating_add(8)
        .saturating_add(u32::try_from(binary_bytes.len()).unwrap_or(u32::MAX));
    let mut out: Vec<u8> = Vec::with_capacity(usize::try_from(total).unwrap_or(0));
    out.extend_from_slice(&0x4654_6C67_u32.to_le_bytes());
    out.extend_from_slice(&2_u32.to_le_bytes());
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(
        &u32::try_from(json_bytes.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    out.extend_from_slice(&0x4E4F_534A_u32.to_le_bytes());
    out.extend_from_slice(&json_bytes);
    out.extend_from_slice(
        &u32::try_from(binary_bytes.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    out.extend_from_slice(&0x004E_4942_u32.to_le_bytes());
    out.extend_from_slice(&binary_bytes);
    out
}
