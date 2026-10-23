//! Essential regressions for the measured prepared-prop duplication: changing
//! baked colour must not duplicate identical position/UV streams. Two synthetic
//! triangles roundtrip the real PMP1 writer. Tiny independent wire fixtures
//! protect v1 compatibility, all accepted strides and hostile plane boundaries.
//! No assets, GPU or map baking. The one real chunk-boundary run crosses the 65,536-vertex tile boundary with 5.1 MiB
//! of original data (under 16 MiB total); other fixtures use a few KiB.

use crate::engine::materials::{MaterialAlpha, MaterialEmission, MaterialResponse};
use crate::engine::package::props;
use crate::engine::render::{PropMeshBatch, PropSubmeshBatch, Vertex};
use crate::engine::spatial::Aabb;

use super::{
    ATTRIBUTE_VERSION, COLOR_BYTES, MAGIC, PREFIX_VERSION, PREVIOUS_SHUFFLE_VERTICES,
    SHARED_PROPS_CHUNK_BYTES, SHARED_PROPS_RECIPE_VERSION, SHUFFLE_VERTICES, SHUFFLED_VERSION,
    Writer, attribute_widths, factor_props, hash, read_recipe, reconstruct_props, u32_from_usize,
    u64_from_usize,
};

fn triangle(color: [f32; 4]) -> PropMeshBatch {
    PropMeshBatch {
        model: "models/attribute-sharing.glb".into(),
        light_control: None,
        casts_static_lighting: true,
        pending_probe_lighting: false,
        textures: Vec::new(),
        source_primitives: Vec::new(),
        submeshes: vec![PropSubmeshBatch {
            response: MaterialResponse::default(),
            texture: None,
            emission: MaterialEmission::default(),
            alpha: MaterialAlpha::OPAQUE,
            first_index: 0,
            index_count: 3,
        }],
        vertices: ([[0.0, -0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
            .map(|pos| Vertex {
                pos,
                color,
                uv: [pos[0], pos[1]],
                ..Vertex::UNLIT
            })
            .to_vec())
        .into(),
        indices: (vec![0, 1, 2]).into(),
        bounds: Aabb {
            min: [0.0; 3],
            max: [1.0, 1.0, 0.0],
        },
    }
}

#[test]
fn changed_baked_color_shares_exact_position_uv_and_preserves_pmp_bytes() -> Result<(), String> {
    let first = props::write_props(&[triangle([0.2, 0.3, 0.4, 1.0])])?;
    let second = props::write_props(&[triangle([0.8, 0.7, 0.6, 1.0])])?;
    let single = factor_props(&[&first])?;
    let both = factor_props(&[&first, &second])?;
    let unique_single: usize = single.segments.iter().map(Vec::len).sum();
    let unique_both: usize = both.segments.iter().map(Vec::len).sum();
    assert_eq!(
        unique_both.saturating_sub(unique_single),
        3 * COLOR_BYTES,
        "only the second colour plane adds bytes; position, UV, tail and raw data are shared"
    );
    for (encoded, original) in both.recipes.iter().zip([&first, &second]) {
        assert_eq!(
            encoded.get(4..6),
            Some(SHARED_PROPS_RECIPE_VERSION.to_le_bytes().as_slice()),
            "the writer publishes the explicit v2 contract"
        );
        let recipe = read_recipe(encoded)?;
        let views = recipe
            .segments()
            .iter()
            .map(|descriptor| {
                both.segments
                    .iter()
                    .find(|bytes| hash::sha256_hex(bytes) == descriptor.sha256)
                    .map(Vec::as_slice)
                    .ok_or_else(|| "missing fixture segment".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(
            reconstruct_props(&recipe, &views)?,
            *original,
            "complete original wire bytes, signed zero and lighting factors survive"
        );
    }
    Ok(())
}

struct WireFixture {
    recipe: Vec<u8>,
    segment: Vec<u8>,
    original: Vec<u8>,
    first_chunk_length: usize,
}

/// Independent wire fixture: one three-vertex part, one segment, no model I/O.
/// The historical record body is opaque to this recipe codec; the owning PMP1
/// decoder still validates actual geometry after reconstruction.
fn wire_fixture(version: u16, stride: usize) -> Result<WireFixture, String> {
    wire_fixture_count(version, stride, 3)
}

fn wire_fixture_count(version: u16, stride: usize, count: usize) -> Result<WireFixture, String> {
    let bytes = count
        .checked_mul(stride)
        .ok_or("fixture length overflows")?;
    let original: Vec<u8> = (0u8..=250).cycle().take(bytes).collect();
    let widths = if version == PREFIX_VERSION {
        [36, 0, 0, stride.saturating_sub(36)]
    } else {
        attribute_widths(stride)
    };
    let mut segment = Vec::new();
    let mut locations = Vec::new();
    let mut references = Vec::new();
    let mut start = 0usize;
    for width in widths {
        let end = start.saturating_add(width);
        if width != 0 {
            let mut plane = Vec::new();
            if version >= SHUFFLED_VERSION {
                let tile_count = if version == SHUFFLED_VERSION {
                    PREVIOUS_SHUFFLE_VERTICES
                } else {
                    SHUFFLE_VERTICES
                };
                for tile in original.chunks(stride.saturating_mul(tile_count)) {
                    for lane in start..end {
                        for vertex in tile.chunks_exact(stride) {
                            plane.push(*vertex.get(lane).ok_or("fixture lane missing")?);
                        }
                    }
                }
            } else {
                for vertex in original.chunks_exact(stride) {
                    plane.extend_from_slice(
                        vertex.get(start..end).ok_or("fixture plane truncated")?,
                    );
                }
            }
            for chunk in plane.chunks(SHARED_PROPS_CHUNK_BYTES) {
                let offset = segment.len();
                references.push(locations.len());
                segment.extend_from_slice(chunk);
                locations.push((offset, chunk.len()));
            }
        }
        start = end;
    }
    let mut writer = Writer::new();
    writer.bytes(&MAGIC);
    writer.u16(version);
    writer.u64(u64_from_usize(original.len())?);
    writer.bytes(hash::sha256_hex(&original).as_bytes());
    writer.u32(1);
    writer.u64(u64_from_usize(segment.len())?);
    writer.bytes(hash::sha256_hex(&segment).as_bytes());
    writer.u32(u32_from_usize(locations.len())?);
    let first_chunk_length = writer.len().saturating_add(6);
    for (offset, length) in &locations {
        writer.u16(0);
        writer.u32(u32_from_usize(*offset)?);
        writer.u32(u32_from_usize(*length)?);
    }
    writer.u32(1);
    writer.u8(1);
    writer.u8(u8::try_from(stride).map_err(|error| error.to_string())?);
    writer.u32(u32_from_usize(count)?);
    for slot in references {
        writer.u32(u32_from_usize(slot)?);
    }
    Ok(WireFixture {
        recipe: writer.into_bytes(),
        segment,
        original,
        first_chunk_length,
    })
}

#[test]
fn shared_recipe_v1_and_v2_preserve_every_supported_vertex_stride() -> Result<(), String> {
    for version in [
        PREFIX_VERSION,
        ATTRIBUTE_VERSION,
        SHUFFLED_VERSION,
        SHARED_PROPS_RECIPE_VERSION,
    ] {
        for stride in [43usize, 45, 55, 57, 69, 81] {
            let fixture = wire_fixture(version, stride)?;
            let recipe = read_recipe(&fixture.recipe)?;
            assert_eq!(recipe.version(), version);
            assert_eq!(
                reconstruct_props(&recipe, &[&fixture.segment])?,
                fixture.original,
                "v{version} preserves all {stride}-byte vertex fields"
            );
        }
    }
    Ok(())
}

#[test]
fn attribute_recipe_refuses_wrong_plane_lengths_truncation_and_payload_hash() -> Result<(), String>
{
    let boundary = wire_fixture_count(SHARED_PROPS_RECIPE_VERSION, 81, 65_537)?;
    let boundary_recipe = read_recipe(&boundary.recipe)?;
    assert_eq!(
        reconstruct_props(&boundary_recipe, &[&boundary.segment])?,
        boundary.original,
        "position, colour and tail cursors retain exact bytes across chunk boundaries"
    );
    let fixture = wire_fixture(SHARED_PROPS_RECIPE_VERSION, 81)?;
    let mut wrong_length = fixture.recipe.clone();
    let end = fixture.first_chunk_length.saturating_add(4);
    wrong_length
        .get_mut(fixture.first_chunk_length..end)
        .ok_or("missing fixture chunk length")?
        .copy_from_slice(&35u32.to_le_bytes());
    let length_error = read_recipe(&wrong_length)
        .err()
        .ok_or("wrong plane length accepted")?;
    assert!(
        length_error.contains("inconsistent chunk length"),
        "{length_error}"
    );
    let mut truncated = fixture.recipe.clone();
    let _removed = truncated.pop().ok_or("empty fixture recipe")?;
    assert!(
        read_recipe(&truncated).is_err(),
        "missing tail reference is refused"
    );
    let recipe = read_recipe(&fixture.recipe)?;
    let mut corrupted = fixture.segment;
    let first = corrupted.first_mut().ok_or("empty fixture segment")?;
    *first ^= 1;
    let payload_error = reconstruct_props(&recipe, &[&corrupted])
        .err()
        .ok_or("corrupted payload accepted")?;
    assert!(
        payload_error.contains("segment length or SHA-256 mismatch"),
        "{payload_error}"
    );
    Ok(())
}
