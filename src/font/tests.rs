//! Unit tests for the bitmap font atlas.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
)]

use super::*;
use crate::test_support::assert_exact;

#[test]
fn test_font_atlas_generation() {
    let atlas = generate_font_atlas();
    assert_eq!(atlas.len(), 256 * 128 * 4);
    // Original glyph pixels and the white UI cell retain their pixel positions
    // and orientation. The UV test and native comparisons cover their sampling.
    for (index, rows) in FONT_DATA.iter().enumerate() {
        for (y, row) in rows.iter().enumerate() {
            for x in 0..8 {
                let pixel_x = (index % 16) * 8 + x;
                let pixel_y = (index / 16) * 8 + y;
                let offset = (pixel_y * 256 + pixel_x) * 4;
                let alpha = if index == 0 || row & (0x80 >> x) != 0 {
                    255
                } else {
                    0
                };
                assert_eq!(&atlas[offset..offset + 4], &[255, 255, 255, alpha]);
            }
        }
    }
    // No opaque palm, badge, outer margin or spill into the unused cell.
    // This tiny deterministic texture fixture needs no map or GPU.
    for (x, y) in [(32, 106), (0, 64), (63, 127), (96, 96)] {
        assert_eq!(atlas[(y * 256 + x) * 4 + 3], 0);
    }
    assert!(
        atlas[64 * 256 * 4..]
            .as_chunks::<4>()
            .0
            .iter()
            .any(|pixel| pixel[3] > 0)
    );
}

#[test]
fn test_char_uv_bounds() {
    let uv_space = get_char_uv(' ').expect("space uv");
    assert_exact(uv_space[0], 0.0);
    assert_exact(uv_space[1], 0.0);

    let uv_a = get_char_uv('A').expect("A uv");
    crate::test_support::assert_exact_array(
        uv_a,
        [8.0 / 256.0, 16.0 / 128.0, 16.0 / 256.0, 24.0 / 128.0],
    );
    assert!(uv_a[0] >= 0.0 && uv_a[2] <= 1.0);
    assert!(uv_a[1] >= 0.0 && uv_a[3] <= 1.0);

    assert!(get_char_uv('\u{1000}').is_none());
}
