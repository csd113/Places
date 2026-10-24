//! Regression value: one large primitive must yield during its bounds scan and
//! preserve exact culling bounds, distinct counts and invalid-index fallback.
//! Three vertices / 24 KiB indices; no assets, device or map bake, under 1 ms.
use super::{BatchCursor, INDEX_SLICE, advance_bounds, prop_draw_geometry};
use crate::engine::materials::{MaterialAlpha, MaterialEmission, MaterialResponse};
use crate::engine::render::{PropMeshBatch, PropSubmeshBatch, Vertex};
use crate::engine::spatial::Aabb;
use std::sync::Arc;

#[test]
fn primitive_bounds_yield_without_changing_culling_or_invalid_index_fallback() -> Result<(), String>
{
    let indices: Vec<u16> = [0_u16, 1, 2]
        .into_iter()
        .cycle()
        .take(INDEX_SLICE.saturating_mul(3))
        .collect();
    let count = u32::try_from(indices.len()).map_err(|error| error.to_string())?;
    let batch = PropMeshBatch {
        model: "bounded-fixture".into(),
        light_control: None,
        casts_static_lighting: true,
        pending_probe_lighting: false,
        textures: Vec::new(),
        source_primitives: vec![0],
        submeshes: vec![PropSubmeshBatch {
            response: MaterialResponse::default(),
            texture: None,
            emission: MaterialEmission::default(),
            alpha: MaterialAlpha::OPAQUE,
            first_index: 0,
            index_count: count,
        }],
        vertices: vec![
            Vertex {
                pos: [-2.0, -0.0, 3.0],
                ..Vertex::UNLIT
            },
            Vertex {
                pos: [5.0, 1.0, -4.0],
                ..Vertex::UNLIT
            },
            Vertex {
                pos: [0.0, 4.0, 2.0],
                ..Vertex::UNLIT
            },
        ]
        .into(),
        indices: Arc::new(indices),
        bounds: Aabb {
            min: [-10.0; 3],
            max: [10.0; 3],
        },
    };
    let expected =
        prop_draw_geometry(&batch, 0, count).ok_or_else(|| "valid fixture refused".to_string())?;
    let mut cursor = BatchCursor::default();
    assert!(
        advance_bounds(&batch, &mut cursor).is_none(),
        "a large primitive must yield before visiting all indices"
    );
    assert_eq!(
        cursor.index, INDEX_SLICE,
        "one unit must visit at most the bounded index slice"
    );
    assert!(
        advance_bounds(&batch, &mut cursor).is_none(),
        "the second slice must also yield"
    );
    let actual =
        advance_bounds(&batch, &mut cursor).ok_or_else(|| "final bounds missing".to_string())?;
    assert_eq!(
        (
            actual.0.min.map(f32::to_bits),
            actual.0.max.map(f32::to_bits),
            actual.1
        ),
        (
            expected.0.min.map(f32::to_bits),
            expected.0.max.map(f32::to_bits),
            expected.1
        ),
        "slicing must preserve exact bounds and distinct vertex count"
    );
    let mut invalid = batch.clone();
    if let Some(index) = Arc::make_mut(&mut invalid.indices).last_mut() {
        *index = u16::MAX;
    }
    let mut invalid_cursor = BatchCursor::default();
    let _first = advance_bounds(&invalid, &mut invalid_cursor);
    let _second = advance_bounds(&invalid, &mut invalid_cursor);
    let fallback = advance_bounds(&invalid, &mut invalid_cursor)
        .ok_or_else(|| "invalid primitive hid its geometry".to_string())?;
    assert_eq!(
        (
            fallback.0.min.map(f32::to_bits),
            fallback.0.max.map(f32::to_bits),
            fallback.1
        ),
        (
            batch.bounds.min.map(f32::to_bits),
            batch.bounds.max.map(f32::to_bits),
            3
        ),
        "invalid metadata must retain the conservative whole-batch fallback"
    );
    Ok(())
}
