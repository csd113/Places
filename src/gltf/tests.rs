//! Unit tests for the GLB reader.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::indexing_slicing
)]

use super::*;
use crate::level::PROP_TRIANGLE_BUDGET;
use crate::materials::MAX_EMISSION_INTENSITY;

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

/// Builder for synthetic GLB fixtures: appends attribute data to the binary
/// chunk and records the matching bufferViews and accessors, so tests can
/// assemble multi-primitive documents without hand-computing byte offsets.
#[derive(Default)]
struct ModelBuilder {
    binary: Vec<u8>,
    views: Vec<String>,
    accessors: Vec<String>,
}

impl ModelBuilder {
    fn add_view(&mut self, payload: &[u8], target: Option<u32>) -> usize {
        while !self.binary.len().is_multiple_of(4) {
            self.binary.push(0);
        }
        let offset = self.binary.len();
        self.binary.extend_from_slice(payload);
        let target = target.map_or(String::new(), |target| format!(r#","target":{target}"#));
        let view = format!(
            r#"{{"buffer":0,"byteOffset":{offset},"byteLength":{length}{target}}}"#,
            length = payload.len()
        );
        self.views.push(view);
        self.views.len() - 1
    }

    fn add_accessor(
        &mut self,
        view: usize,
        component_type: u32,
        count: usize,
        kind: &str,
        normalized: bool,
    ) -> usize {
        let normalized = if normalized {
            r#","normalized":true"#
        } else {
            ""
        };
        self.accessors.push(format!(
            r#"{{"bufferView":{view},"componentType":{component_type},"count":{count},"type":"{kind}"{normalized}}}"#
        ));
        self.accessors.len() - 1
    }

    fn positions(&mut self, positions: &[[f32; 3]]) -> usize {
        let mut bytes = Vec::new();
        for position in positions {
            for value in position {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        let view = self.add_view(&bytes, Some(34962));
        self.add_accessor(view, COMPONENT_FLOAT, positions.len(), "VEC3", false)
    }

    fn uvs(&mut self, uvs: &[[f32; 2]]) -> usize {
        let mut bytes = Vec::new();
        for uv in uvs {
            for value in uv {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        let view = self.add_view(&bytes, Some(34962));
        self.add_accessor(view, COMPONENT_FLOAT, uvs.len(), "VEC2", false)
    }

    fn colors(&mut self, colors: &[[u8; 4]]) -> usize {
        let bytes: Vec<u8> = colors.iter().flatten().copied().collect();
        let view = self.add_view(&bytes, Some(34962));
        self.add_accessor(view, COMPONENT_UBYTE, colors.len(), "VEC4", true)
    }

    fn indices(&mut self, indices: &[u16]) -> usize {
        let bytes: Vec<u8> = indices
            .iter()
            .flat_map(|index| index.to_le_bytes())
            .collect();
        let view = self.add_view(&bytes, Some(34963));
        self.add_accessor(view, COMPONENT_USHORT, indices.len(), "SCALAR", false)
    }

    fn png(&mut self, png: &[u8]) -> usize {
        self.add_view(png, None)
    }

    fn buffer_views(&self) -> String {
        self.views.join(",")
    }

    fn accessors(&self) -> String {
        self.accessors.join(",")
    }

    fn bytes(&self) -> &[u8] {
        &self.binary
    }
}

/// One mesh's JSON with a single primitive over the given accessors.
fn mesh_json(positions: usize, uvs: usize, indices: usize, material: Option<usize>) -> String {
    let material = material.map_or(String::new(), |material| {
        format!(r#","material":{material}"#)
    });
    format!(
        r#"{{"primitives": [{{"attributes": {{"POSITION": {positions}, "TEXCOORD_0": {uvs}}}, "indices": {indices}{material}}}]}}"#
    )
}

/// Wraps one mesh, one node and one material into a GLB with an embedded PNG.
fn triangle_document(material: &str, primitive_material: usize) -> Vec<u8> {
    triangle_document_with_png(material, primitive_material, PIXEL_PNG)
}

/// [`triangle_document`] with a caller-supplied image payload.
fn triangle_document_with_png(material: &str, primitive_material: usize, png: &[u8]) -> Vec<u8> {
    let mut builder = ModelBuilder::default();
    let positions = builder.positions(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
    let uvs = builder.uvs(&[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    let colors = builder.colors(&[[255, 128, 0, 255], [0, 255, 0, 255], [0, 0, 255, 255]]);
    let indices = builder.indices(&[0, 1, 2]);
    let image = builder.png(png);
    let json = format!(
        r#"{{
          "asset": {{"version": "2.0"}},
          "extensionsUsed": ["KHR_materials_emissive_strength"],
          "scene": 0,
          "scenes": [{{"nodes": [0]}}],
          "nodes": [{{"mesh": 0}}],
          "meshes": [{{"primitives": [
            {{"attributes": {{"POSITION": {positions}, "TEXCOORD_0": {uvs}, "COLOR_0": {colors}}}, "indices": {indices}, "material": {primitive_material}}}
          ]}}],
          "accessors": [{accessors}],
          "bufferViews": [{views}],
          "buffers": [{{"byteLength": {length}}}],
          "images": [{{"bufferView": {image}, "mimeType": "image/png"}}],
          "textures": [{{"source": 0}}],
          "materials": [{material}]
        }}"#,
        accessors = builder.accessors(),
        views = builder.buffer_views(),
        length = builder.bytes().len(),
    );
    glb_container(&json, builder.bytes())
}

/// A GLB with `triangles` copies of one triangle, sharing three vertices.
fn repeated_triangle_glb(triangles: usize) -> Vec<u8> {
    let mut builder = ModelBuilder::default();
    let positions = builder.positions(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
    let uvs = builder.uvs(&[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    let repeated: Vec<u16> = (0..triangles).flat_map(|_| [0u16, 1, 2]).collect();
    let indices = builder.indices(&repeated);
    let mesh = mesh_json(positions, uvs, indices, None);
    let json = format!(
        r#"{{
          "asset": {{"version": "2.0"}},
          "scene": 0,
          "scenes": [{{"nodes": [0]}}],
          "nodes": [{{"mesh": 0}}],
          "meshes": [{mesh}],
          "accessors": [{accessors}],
          "bufferViews": [{views}],
          "buffers": [{{"byteLength": {length}}}]
        }}"#,
        accessors = builder.accessors(),
        views = builder.buffer_views(),
        length = builder.bytes().len(),
    );
    glb_container(&json, builder.bytes())
}

#[test]
fn parses_the_shipped_chair_asset() {
    let model = parse_glb(CHAIR_GLB).expect("shipped chair.glb must parse");
    assert!(model.triangles > 0);
    assert_eq!(model.indices.len(), model.triangles * 3);
    let texture = model.textures.first().expect("chair embeds a texture");
    assert!(texture.width.is_power_of_two());
    assert_eq!(texture.width, texture.height);
    assert!(texture.width <= crate::level::MAX_PROP_TEXTURE_SIZE);
    assert_eq!(
        texture.rgba.len(),
        (texture.width as usize) * (texture.height as usize) * 4
    );
    assert_eq!(model.submeshes.len(), 1);
    assert_eq!(model.submeshes[0].texture, Some(0));
    assert_eq!(
        model.submeshes[0].index_count,
        u32::try_from(model.indices.len()).expect("index count fits")
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
    assert_eq!(model.textures.first().expect("texture decodes").width, 1);
    assert_eq!(model.texture_count(), 1);
}

#[test]
fn assigns_materials_and_textures_per_primitive() {
    let mut builder = ModelBuilder::default();
    let positions = builder.positions(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
    let uvs = builder.uvs(&[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    let indices = builder.indices(&[0, 1, 2]);
    let image_a = builder.png(PIXEL_PNG);
    let image_b = builder.png(PIXEL_PNG);
    let primitive = |material: usize| {
        format!(
            r#"{{"attributes": {{"POSITION": {positions}, "TEXCOORD_0": {uvs}}}, "indices": {indices}, "material": {material}}}"#
        )
    };
    let json = format!(
        r#"{{
          "asset": {{"version": "2.0"}},
          "scene": 0,
          "scenes": [{{"nodes": [0]}}],
          "nodes": [{{"mesh": 0}}],
          "meshes": [{{"primitives": [{p0}, {p1}, {p2}]}}],
          "accessors": [{accessors}],
          "bufferViews": [{views}],
          "buffers": [{{"byteLength": {length}}}],
          "images": [
            {{"bufferView": {image_a}, "mimeType": "image/png"}},
            {{"bufferView": {image_b}, "mimeType": "image/png"}}
          ],
          "textures": [{{"source": 0}}, {{"source": 1}}, {{"source": 0}}],
          "materials": [
            {{"pbrMetallicRoughness": {{"baseColorTexture": {{"index": 0}}}}}},
            {{"pbrMetallicRoughness": {{"baseColorTexture": {{"index": 1}}}}}},
            {{"pbrMetallicRoughness": {{"baseColorTexture": {{"index": 2}}}}}}
          ]
        }}"#,
        p0 = primitive(0),
        p1 = primitive(1),
        p2 = primitive(2),
        accessors = builder.accessors(),
        views = builder.buffer_views(),
        length = builder.bytes().len(),
    );
    let model =
        parse_glb(&glb_container(&json, builder.bytes())).expect("multi-primitive model parses");
    assert_eq!(model.triangles, 3);
    assert_eq!(model.materials, 3);
    assert_eq!(model.textures.len(), 2, "each distinct image decodes once");
    assert_eq!(model.submeshes.len(), 3);
    assert_eq!(model.submeshes[0].material, 0);
    assert_eq!(model.submeshes[0].texture, Some(0));
    assert_eq!(model.submeshes[1].material, 1);
    assert_eq!(model.submeshes[1].texture, Some(1));
    assert_eq!(model.submeshes[2].material, 2);
    assert_eq!(
        model.submeshes[2].texture,
        Some(0),
        "a second texture entry over the same image reuses the first decode"
    );

    // Draw ranges tile the index buffer in primitive order.
    let mut next = 0u32;
    for submesh in &model.submeshes {
        assert_eq!(submesh.first_index, next);
        assert_eq!(submesh.index_count, 3);
        next += submesh.index_count;
    }
    assert_eq!(next, u32::try_from(model.indices.len()).expect("fits"));
}

#[test]
fn untextured_materials_multiply_their_base_color_factor() {
    let material = r#"{"pbrMetallicRoughness": {"baseColorFactor": [0.5, 0.25, 1.0, 0.5]}}"#;
    let model = parse_glb(&triangle_document(material, 0)).expect("untextured material parses");
    assert_eq!(model.texture_count(), 0, "unused images are not decoded");
    assert_eq!(model.submeshes[0].texture, None);
    assert!(!model.submeshes[0].emission.is_emissive());
    assert!(
        model.submeshes[0]
            .emission
            .color
            .iter()
            .all(|channel| channel.abs() < 1e-6)
    );
    let color = model.vertices[0].color;
    assert!((color[0] - 0.5).abs() < 1e-6, "{color:?}");
    let expected_green = (128.0 / 255.0) * 0.25;
    assert!((color[1] - expected_green).abs() < 1e-3, "{color:?}");
    assert!(color[2].abs() < 1e-6, "{color:?}");
    assert!((color[3] - 0.5).abs() < 1e-6, "{color:?}");

    // A material with no factor at all keeps the baked COLOR_0 values.
    let model = parse_glb(&triangle_document("{}", 0)).expect("default material parses");
    assert!((model.vertices[0].color[1] - 128.0 / 255.0).abs() < 1e-3);
}

#[test]
fn a_primitive_without_a_material_uses_the_default_slot() {
    let mut builder = ModelBuilder::default();
    let positions = builder.positions(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
    let uvs = builder.uvs(&[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    let indices = builder.indices(&[0, 1, 2]);
    let image = builder.png(PIXEL_PNG);
    let json = format!(
        r#"{{
          "asset": {{"version": "2.0"}},
          "scene": 0,
          "scenes": [{{"nodes": [0]}}],
          "nodes": [{{"mesh": 0}}],
          "meshes": [{{"primitives": [
            {{"attributes": {{"POSITION": {positions}, "TEXCOORD_0": {uvs}}}, "indices": {indices}}}
          ]}}],
          "accessors": [{accessors}],
          "bufferViews": [{views}],
          "buffers": [{{"byteLength": {length}}}],
          "images": [{{"bufferView": {image}, "mimeType": "image/png"}}],
          "textures": [{{"source": 0}}],
          "materials": [{{"pbrMetallicRoughness": {{"baseColorTexture": {{"index": 0}}}}}}]
        }}"#,
        accessors = builder.accessors(),
        views = builder.buffer_views(),
        length = builder.bytes().len(),
    );
    let model = parse_glb(&glb_container(&json, builder.bytes())).expect("default material parses");
    assert_eq!(model.materials, 1);
    assert_eq!(
        model.submeshes[0].material, 1,
        "the implicit glTF default material uses the synthetic slot after the declared list"
    );
    assert_eq!(model.submeshes[0].texture, None);
    assert!(model.textures.is_empty());
    assert!((model.vertices[0].color[0] - 1.0).abs() < 1e-6);
}

#[test]
fn emissive_materials_carry_factor_strength_and_mask() {
    let masked = r#"{
        "emissiveFactor": [0.5, 0.25, 0.0],
        "emissiveTexture": {"index": 0},
        "extensions": {"KHR_materials_emissive_strength": {"emissiveStrength": 2.0}}
    }"#;
    let model = parse_glb(&triangle_document(masked, 0)).expect("emissive material parses");
    assert_eq!(model.textures.len(), 1, "the emissive mask decodes");
    assert_eq!(
        model.submeshes[0].emission,
        MaterialEmission::new([0.5, 0.25, 0.0], 2.0)
            .with_mask(Some(0))
            .sanitized()
    );
    assert!(model.submeshes[0].emission.is_emissive());

    // No strength extension: the glTF default of 1.0 applies.
    let plain = r#"{"emissiveFactor": [1.0, 0.0, 0.0]}"#;
    let model = parse_glb(&triangle_document(plain, 0)).expect("emissive without strength parses");
    assert_eq!(
        model.submeshes[0].emission,
        MaterialEmission::new([1.0, 0.0, 0.0], 1.0)
    );
    assert_eq!(model.texture_count(), 0, "unused images are not decoded");

    // Authored strengths above the engine maximum are clamped, not rejected.
    let hot = r#"{
        "emissiveFactor": [1.0, 1.0, 1.0],
        "extensions": {"KHR_materials_emissive_strength": {"emissiveStrength": 100.0}}
    }"#;
    let model = parse_glb(&triangle_document(hot, 0)).expect("over-strength emission parses");
    assert!(
        (model.submeshes[0].emission.intensity - MAX_EMISSION_INTENSITY).abs() < f32::EPSILON,
        "{:?}",
        model.submeshes[0].emission
    );
}

#[test]
fn node_transforms_move_only_the_meshes_below_them() {
    let mut builder = ModelBuilder::default();
    let positions = builder.positions(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
    let uvs = builder.uvs(&[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    let indices = builder.indices(&[0, 1, 2]);
    let mesh = mesh_json(positions, uvs, indices, None);
    let json = format!(
        r#"{{
          "asset": {{"version": "2.0"}},
          "scene": 0,
          "scenes": [{{"nodes": [0, 1]}}],
          "nodes": [
            {{"mesh": 0}},
            {{"mesh": 1, "translation": [10.0, 0.0, 0.0]}}
          ],
          "meshes": [{mesh}, {mesh}],
          "accessors": [{accessors}],
          "bufferViews": [{views}],
          "buffers": [{{"byteLength": {length}}}]
        }}"#,
        accessors = builder.accessors(),
        views = builder.buffer_views(),
        length = builder.bytes().len(),
    );
    let model = parse_glb(&glb_container(&json, builder.bytes())).expect("two-mesh model parses");
    assert_eq!(model.submeshes.len(), 2);
    let (low, high) = model.bounds().expect("model has vertices");
    assert!(low[0].abs() < 1e-6, "{low:?}");
    assert!((high[0] - 11.0).abs() < 1e-6, "{high:?}");
    assert!(
        model.vertices[..3]
            .iter()
            .all(|vertex| vertex.pos[0] <= 1.0),
        "the untransformed mesh stays at the origin"
    );
    assert!(
        model.vertices[3..]
            .iter()
            .all(|vertex| vertex.pos[0] >= 10.0),
        "the transformed mesh moves by its node translation"
    );
}

#[test]
fn nested_node_transforms_compose() {
    let mut builder = ModelBuilder::default();
    let positions = builder.positions(&[[0.5, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
    let uvs = builder.uvs(&[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    let indices = builder.indices(&[0, 1, 2]);
    let mesh = mesh_json(positions, uvs, indices, None);
    let json = format!(
        r#"{{
          "asset": {{"version": "2.0"}},
          "scene": 0,
          "scenes": [{{"nodes": [0]}}],
          "nodes": [
            {{"children": [1], "translation": [1.0, 0.0, 0.0], "rotation": [0.0, 0.70710678, 0.0, 0.70710678]}},
            {{"children": [2], "scale": [2.0, 2.0, 2.0]}},
            {{"mesh": 0, "translation": [0.0, 0.0, 3.0]}}
          ],
          "meshes": [{mesh}],
          "accessors": [{accessors}],
          "bufferViews": [{views}],
          "buffers": [{{"byteLength": {length}}}]
        }}"#,
        accessors = builder.accessors(),
        views = builder.buffer_views(),
        length = builder.bytes().len(),
    );
    let model = parse_glb(&glb_container(&json, builder.bytes())).expect("nested nodes parse");
    // (0.5,0,0) -> grandchild translate (0.5,0,3) -> child scale (1,0,6)
    // -> root rotate 90 degrees about y (6,0,-1) -> root translate (7,0,-1).
    let position = model.vertices[0].pos;
    assert!((position[0] - 7.0).abs() < 1e-4, "{position:?}");
    assert!(position[1].abs() < 1e-4, "{position:?}");
    assert!((position[2] + 1.0).abs() < 1e-4, "{position:?}");
}

#[test]
fn over_budget_models_still_load() {
    let triangles = PROP_TRIANGLE_BUDGET + 1;
    let model =
        parse_glb(&repeated_triangle_glb(triangles)).expect("over-budget models still load");
    assert_eq!(model.triangles, triangles);
}

#[test]
fn over_ceiling_models_fail() {
    let triangles = MAX_PROP_TRIANGLES + 1;
    let error = parse_glb(&repeated_triangle_glb(triangles)).expect_err("over-ceiling models fail");
    assert!(error.0.contains("engine ceiling"), "{}", error.0);
    assert!(
        error.0.contains(&MAX_PROP_TRIANGLES.to_string()),
        "{}",
        error.0
    );
}

#[test]
fn rejects_models_over_the_primitive_material_and_image_ceilings() {
    let mut builder = ModelBuilder::default();
    let positions = builder.positions(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
    let uvs = builder.uvs(&[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    let indices = builder.indices(&[0, 1, 2]);
    let primitive = format!(
        r#"{{"attributes": {{"POSITION": {positions}, "TEXCOORD_0": {uvs}}}, "indices": {indices}}}"#
    );
    let primitives = std::iter::repeat_n(primitive.as_str(), MAX_PROP_PRIMITIVES + 1)
        .collect::<Vec<&str>>()
        .join(",");
    let json = format!(
        r#"{{
          "asset": {{"version": "2.0"}},
          "scene": 0,
          "scenes": [{{"nodes": [0]}}],
          "nodes": [{{"mesh": 0}}],
          "meshes": [{{"primitives": [{primitives}]}}],
          "accessors": [{accessors}],
          "bufferViews": [{views}],
          "buffers": [{{"byteLength": {length}}}]
        }}"#,
        accessors = builder.accessors(),
        views = builder.buffer_views(),
        length = builder.bytes().len(),
    );
    let error =
        parse_glb(&glb_container(&json, builder.bytes())).expect_err("too many primitives fail");
    assert!(error.0.contains("primitives"), "{}", error.0);

    let materials = std::iter::repeat_n("{}", MAX_PROP_MATERIALS + 1)
        .collect::<Vec<&str>>()
        .join(",");
    let mesh = mesh_json(positions, uvs, indices, Some(0));
    let json = format!(
        r#"{{
          "asset": {{"version": "2.0"}},
          "scene": 0,
          "scenes": [{{"nodes": [0]}}],
          "nodes": [{{"mesh": 0}}],
          "meshes": [{mesh}],
          "accessors": [{accessors}],
          "bufferViews": [{views}],
          "buffers": [{{"byteLength": {length}}}],
          "materials": [{materials}]
        }}"#,
        accessors = builder.accessors(),
        views = builder.buffer_views(),
        length = builder.bytes().len(),
    );
    let error =
        parse_glb(&glb_container(&json, builder.bytes())).expect_err("too many materials fail");
    assert!(error.0.contains("materials"), "{}", error.0);

    let images = std::iter::repeat_n("{}", MAX_PROP_IMAGES + 1)
        .collect::<Vec<&str>>()
        .join(",");
    let json = format!(
        r#"{{
          "asset": {{"version": "2.0"}},
          "scene": 0,
          "scenes": [{{"nodes": [0]}}],
          "nodes": [{{"mesh": 0}}],
          "meshes": [{mesh}],
          "accessors": [{accessors}],
          "bufferViews": [{views}],
          "buffers": [{{"byteLength": {length}}}],
          "images": [{images}]
        }}"#,
        accessors = builder.accessors(),
        views = builder.buffer_views(),
        length = builder.bytes().len(),
    );
    let error =
        parse_glb(&glb_container(&json, builder.bytes())).expect_err("too many images fail");
    assert!(error.0.contains("images"), "{}", error.0);
}

#[test]
fn rejects_node_cycles_and_dangling_references() {
    let cycle = r#"{
      "asset": {"version": "2.0"},
      "scene": 0,
      "scenes": [{"nodes": [0]}],
      "nodes": [{"children": [1]}, {"children": [0]}]
    }"#;
    let error = parse_glb(&glb_container(cycle, b"")).expect_err("cycles must fail");
    assert!(error.0.to_lowercase().contains("cycle"), "{}", error.0);

    let self_cycle = r#"{
      "asset": {"version": "2.0"},
      "scene": 0,
      "scenes": [{"nodes": [0]}],
      "nodes": [{"children": [0]}]
    }"#;
    let error = parse_glb(&glb_container(self_cycle, b"")).expect_err("self references must fail");
    assert!(error.0.to_lowercase().contains("cycle"), "{}", error.0);

    let dangling = r#"{
      "asset": {"version": "2.0"},
      "scene": 0,
      "scenes": [{"nodes": [0]}],
      "nodes": [{"children": [7]}]
    }"#;
    let error = parse_glb(&glb_container(dangling, b"")).expect_err("dangling children must fail");
    assert!(
        error.0.contains('7') && error.0.contains("does not exist"),
        "{}",
        error.0
    );

    let scene_dangling = r#"{
      "asset": {"version": "2.0"},
      "scene": 0,
      "scenes": [{"nodes": [4]}],
      "nodes": [{}]
    }"#;
    let error =
        parse_glb(&glb_container(scene_dangling, b"")).expect_err("dangling roots must fail");
    assert!(error.0.contains('4'), "{}", error.0);

    let parentless = r#"{
      "asset": {"version": "2.0"},
      "nodes": [{"children": [9]}]
    }"#;
    let error =
        parse_glb(&glb_container(parentless, b"")).expect_err("dangling children must fail");
    assert!(error.0.contains('9'), "{}", error.0);
}

#[test]
fn rejects_non_finite_transformed_geometry() {
    let mut builder = ModelBuilder::default();
    let positions = builder.positions(&[[1.0e38, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    let uvs = builder.uvs(&[[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    let indices = builder.indices(&[0, 1, 2]);
    let mesh = mesh_json(positions, uvs, indices, None);
    let json = format!(
        r#"{{
          "asset": {{"version": "2.0"}},
          "scene": 0,
          "scenes": [{{"nodes": [0]}}],
          "nodes": [{{"mesh": 0, "scale": [1.0e38, 1.0, 1.0]}}],
          "meshes": [{mesh}],
          "accessors": [{accessors}],
          "bufferViews": [{views}],
          "buffers": [{{"byteLength": {length}}}]
        }}"#,
        accessors = builder.accessors(),
        views = builder.buffer_views(),
        length = builder.bytes().len(),
    );
    let error = parse_glb(&glb_container(&json, builder.bytes()))
        .expect_err("transforms that overflow to infinity must fail");
    assert!(error.0.contains("non-finite"), "{}", error.0);
}

#[test]
fn rejects_malformed_assets_with_actionable_messages() {
    let json_cases: [(&str, &str, &str); 5] = [
        (
            "empty mesh list",
            r#"{"asset":{"version":"2.0"},"meshes":[]}"#,
            "nodes",
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
        (
            "morph targets",
            r#"{"asset":{"version":"2.0"},"meshes":[{"primitives":[{"targets":[{}]}]}]}"#,
            "morph",
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
    let (json, binary) = minimal_triangle_parts();
    let json = json.replace("\"mode\": 4", "\"mode\": 1");
    let error = parse_glb(&glb_container(&json, &binary)).expect_err("mode 1 is not triangles");
    assert!(error.0.contains("TRIANGLES"), "{}", error.0);

    let (json, binary) = minimal_triangle_parts();
    let json = json.replace("\"TEXCOORD_0\": 1, ", "");
    let error = parse_glb(&glb_container(&json, &binary)).expect_err("a prop needs UVs");
    assert!(
        error.0.to_lowercase().contains("uv"),
        "a prop without UVs must say so: {}",
        error.0
    );
}

#[test]
fn rejects_unsupported_images_extensions_and_sparse_data() {
    let (json, binary) = minimal_triangle_parts();

    let uri_json = json.replace(
        r#"{"bufferView": 4, "mimeType": "image/png"}"#,
        r#"{"uri": "data:image/png;base64,AAAA", "mimeType": "image/png"}"#,
    );
    let error = parse_glb(&glb_container(&uri_json, &binary)).expect_err("data URIs must fail");
    assert!(
        error.0.contains("embedded") || error.0.contains("data-URI"),
        "{}",
        error.0
    );

    let mime_json = json.replace("image/png", "image/jpeg");
    let error = parse_glb(&glb_container(&mime_json, &binary)).expect_err("JPEG must fail");
    assert!(error.0.contains("mime"), "{}", error.0);

    let extension_json = json.replace(
        r#""materials": [{"pbrMetallicRoughness": {"baseColorTexture": {"index": 0}}}]"#,
        r#""materials": [{"pbrMetallicRoughness": {"baseColorTexture": {"index": 0}}, "extensions": {"KHR_materials_clearcoat": {}}}]"#,
    );
    let error =
        parse_glb(&glb_container(&extension_json, &binary)).expect_err("clearcoat must fail");
    assert!(error.0.contains("KHR_materials_clearcoat"), "{}", error.0);

    let sparse_json = json.replace(r#""type": "VEC3""#, r#""type": "VEC3", "sparse": {}"#);
    let error =
        parse_glb(&glb_container(&sparse_json, &binary)).expect_err("sparse accessors must fail");
    assert!(error.0.contains("sparse"), "{}", error.0);
}

/// A PNG header claiming `width x height` pixels.
///
/// The reader checks the `IHDR` dimensions before the decoder sees the
/// payload, so a header-only fixture is enough to exercise the prop texture
/// ceiling without embedding a real multi-megapixel image.
fn huge_png_header(width: u32, height: u32) -> Vec<u8> {
    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    png.extend_from_slice(&13u32.to_be_bytes()); // IHDR chunk length
    png.extend_from_slice(b"IHDR");
    png.extend_from_slice(&width.to_be_bytes());
    png.extend_from_slice(&height.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0]); // depth, colour type, compression, filter, interlace
    png.extend_from_slice(&[0, 0, 0, 0]); // placeholder CRC
    png
}

#[test]
fn rejects_textures_above_the_prop_edge_limit() {
    let material = r#"{"pbrMetallicRoughness": {"baseColorTexture": {"index": 0}}}"#;
    let oversized = huge_png_header(MAX_PROP_TEXTURE_SIZE + 1, 64);
    let error = parse_glb(&triangle_document_with_png(material, 0, &oversized))
        .expect_err("textures above the prop limit must fail");
    assert!(
        error.0.contains(&(MAX_PROP_TEXTURE_SIZE + 1).to_string()),
        "the message must name the offending size: {}",
        error.0
    );
}

#[test]
fn rejects_second_texture_coordinate_sets() {
    let material = r#"{"pbrMetallicRoughness": {"baseColorTexture": {"index": 0, "texCoord": 1}}}"#;
    let error = parse_glb(&triangle_document(material, 0)).expect_err("texCoord 1 must fail");
    assert!(error.0.contains("texCoord"), "{}", error.0);
}

#[test]
fn rejects_models_over_the_vertex_ceiling() {
    // Two primitives share one 32,768-vertex accessor, so assembling both
    // needs 65,536 vertices: one past the 65,535-vertex engine ceiling.
    let vertex_count = MAX_PROP_VERTICES / 2 + 1;
    let mut builder = ModelBuilder::default();
    let positions = builder.positions(&vec![[0.0f32, 0.0, 0.0]; vertex_count]);
    let uvs = builder.uvs(&vec![[0.0f32, 0.0]; vertex_count]);
    let indices = builder.indices(&[0, 1, 2]);
    let primitive = format!(
        r#"{{"attributes": {{"POSITION": {positions}, "TEXCOORD_0": {uvs}}}, "indices": {indices}}}"#
    );
    let json = format!(
        r#"{{
          "asset": {{"version": "2.0"}},
          "scene": 0,
          "scenes": [{{"nodes": [0]}}],
          "nodes": [{{"mesh": 0}}],
          "meshes": [{{"primitives": [{primitive}, {primitive}]}}],
          "accessors": [{accessors}],
          "bufferViews": [{views}],
          "buffers": [{{"byteLength": {length}}}]
        }}"#,
        accessors = builder.accessors(),
        views = builder.buffer_views(),
        length = builder.bytes().len(),
    );
    let error = parse_glb(&glb_container(&json, builder.bytes()))
        .expect_err("models above the vertex ceiling must fail");
    assert!(error.0.contains("vertices"), "{}", error.0);
    assert!(
        error.0.contains(&MAX_PROP_VERTICES.to_string()),
        "{}",
        error.0
    );
}

#[test]
fn rejects_zero_stride_accessors_that_claim_many_elements() {
    // A zero `byteStride` makes the "does the accessor fit its bufferView?"
    // arithmetic collapse to one element, so together with a huge `count` it
    // used to pass the check and then reserve/loop over a buffer it does not
    // have. The reader must reject it instead.
    let (json, binary) = minimal_triangle_parts();
    let json = json.replace(
        r#"{"buffer": 0, "byteOffset": 0, "byteLength": 36, "target": 34962}"#,
        r#"{"buffer": 0, "byteOffset": 0, "byteLength": 36, "byteStride": 0, "target": 34962}"#,
    );
    let json = json.replace(
        r#"{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"}"#,
        r#"{"bufferView": 0, "componentType": 5126, "count": 1000000000, "type": "VEC3"}"#,
    );
    let error = parse_glb(&glb_container(&json, &binary))
        .expect_err("a zero byteStride must not disable the bounds check");
    assert!(error.0.contains("byteStride"), "{}", error.0);
}

#[test]
fn rejects_a_count_larger_than_the_buffer_can_hold() {
    // Even with a plausible stride, a count whose elements cannot physically
    // fit the view is refused before any reservation is made.
    let (json, binary) = minimal_triangle_parts();
    let json = json.replace(
        r#"{"bufferView": 0, "componentType": 5126, "count": 3, "type": "VEC3"}"#,
        r#"{"bufferView": 0, "componentType": 5126, "count": 1000000000, "type": "VEC3"}"#,
    );
    let error = parse_glb(&glb_container(&json, &binary))
        .expect_err("a count past the bufferView must fail");
    assert!(
        error.0.contains("bufferView") || error.0.contains("too small"),
        "{}",
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
