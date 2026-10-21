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
