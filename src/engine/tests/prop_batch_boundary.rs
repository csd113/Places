//! Four-byte headers protect a real producer/reader mismatch before allocation.
//! This needs no meshes, map preparation, assets or GPU resources.
use super::{Reader, read_vertex_count};

#[test]
fn prop_vertex_admission_matches_the_compilers_sixteen_bit_batches() -> Result<(), String> {
    let accepted = 65_536_u32.to_le_bytes();
    assert_eq!(
        read_vertex_count(&mut Reader::new(&accepted))?,
        65_536,
        "every addressable vertex remains valid"
    );
    let refused = 65_537_u32.to_le_bytes();
    assert!(
        read_vertex_count(&mut Reader::new(&refused)).is_err(),
        "unreferenced vertices cannot request an oversized GPU buffer"
    );
    Ok(())
}

/// A 4,097-vertex synthetic run protects the loading-memory regression: the
/// framed count used to grow from 4,096 to 8,192 retained slots. Exercise both
/// literal and palette decoders with under 1 MiB, no map/asset/GPU dependency.
#[test]
fn decoded_prop_vertices_do_not_retain_geometric_growth_slack() -> Result<(), String> {
    use crate::engine::package::binary::Writer;
    use crate::engine::render::Vertex;
    let count = 4_097;
    let vertices = vec![Vertex::UNLIT; count];
    let mut literal = Writer::new();
    for vertex in &vertices {
        super::write_vertex(&mut literal, vertex);
    }
    let decoded = super::read_literal_vertices(&mut Reader::new(literal.as_slice()), count)?;
    assert_eq!(decoded.len(), count, "literal framing retains every vertex");
    assert_eq!(
        decoded.capacity(),
        count,
        "literal storage has no unused growth slots"
    );
    let mut palette = Writer::new();
    super::write_vertex_storage(&mut palette, &vertices.iter().collect::<Vec<_>>())?;
    let mut palette_reader = Reader::new(palette.as_slice());
    let framed_count = super::read_vertex_count(&mut palette_reader)?;
    let palette_vertices = super::read_vertex_storage(&mut palette_reader, framed_count)?;
    assert_eq!(
        palette_vertices.len(),
        count,
        "palette framing retains every vertex"
    );
    assert_eq!(
        palette_vertices.capacity(),
        count,
        "palette storage has no unused growth slots"
    );
    Ok(())
}
