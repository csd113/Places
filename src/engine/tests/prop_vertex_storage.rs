//! Two bounded regressions: exact bits/copy-on-write (512 vertices), and the
//! 65,536-entry palette boundary (about 0.3 MiB input). No assets or map bakes.
//! Existing codec tests cover file validation; these protect the runtime form.
use super::{Attribute, PropVertices, Vertex};

fn bits(vertex: Vertex) -> Vec<u32> {
    let mut values = Vec::with_capacity(22);
    for channels in [
        vertex.pos.as_slice(),
        vertex.color.as_slice(),
        vertex.caustics_albedo.as_slice(),
        vertex.uv.as_slice(),
        vertex.normal.as_slice(),
        vertex.tangent.as_slice(),
    ] {
        values.extend(channels.iter().copied().map(f32::to_bits));
    }
    values.push(vertex.handedness.to_bits());
    values.extend(vertex.lightmap.map(u32::from));
    values.push(u32::from(vertex.lightmap_page));
    values
}

#[test]
fn immutable_vertices_keep_every_bit_without_materializing_and_edit_independently()
-> Result<(), String> {
    let source: Vec<_> = (0_u16..512)
        .map(|index| Vertex {
            pos: [f32::from(index), -0.0, 1.0],
            color: [32.25, f32::from_bits(1), 0.0, 0.375],
            caustics_albedo: [0.125, 0.25, 0.5],
            uv: [f32::from(index % 4), -0.0],
            normal: [0.0, 1.0, 0.0],
            tangent: [1.0, 0.0, -0.0],
            handedness: -1.0,
            lightmap: [index, u16::MAX],
            lightmap_page: u8::MAX,
        })
        .collect();
    let expected: Vec<_> = source.iter().copied().map(bits).collect();
    let raw_bytes = source
        .capacity()
        .saturating_mul(std::mem::size_of::<Vertex>());
    let packed = PropVertices::packed(source)?;
    assert!(
        packed.retained_bytes() < raw_bytes / 2,
        "fixture must exercise compact immutable ownership"
    );
    assert_eq!(
        packed.iter().map(bits).collect::<Vec<_>>(),
        expected,
        "all float bits, signed zero, HDR, alpha, frame and atlas fields must survive"
    );
    let retained = packed.retained_bytes();
    assert!(
        packed.get(packed.len()).is_none(),
        "out-of-range reads must fail"
    );
    assert_eq!(
        packed.retained_bytes(),
        retained,
        "reads must not cache an expanded float vector"
    );
    let mut edited = packed.clone();
    if let Some(vertex) = edited.make_mut().first_mut() {
        vertex.color = [1.0; 4];
    }
    assert_ne!(
        edited.identity(),
        packed.identity(),
        "editing a clone must detach its immutable ownership"
    );
    assert_eq!(
        packed.iter().map(bits).collect::<Vec<_>>(),
        expected,
        "editing must preserve the original shared vertices"
    );
    Ok(())
}

#[test]
fn unique_attributes_beyond_u16_palette_capacity_preserve_last_value() {
    let source: Vec<_> = (0_u32..65_537).map(|index| [index]).collect();
    let attributes = Attribute::new(source);
    assert_eq!(
        attributes.get(65_536),
        Some([65_536]),
        "palette capacity must never truncate or wrap a unique attribute"
    );
    assert!(
        attributes.get(65_537).is_none(),
        "direct storage must retain checked bounds"
    );
}
