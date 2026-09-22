//! Unit tests for the bitmap font atlas.

use super::*;
use crate::test_support::assert_exact;

#[test]
fn test_font_atlas_generation() {
    let atlas = generate_font_atlas();
    assert_eq!(atlas.len(), 128 * 64 * 4);
    // Verify white box at pixel (2, 2)
    let idx = (2 * 128 + 2) * 4;
    assert_eq!(atlas[idx + 3], 255);
}

#[test]
fn test_char_uv_bounds() {
    let uv_space = get_char_uv(' ').expect("space uv");
    assert_exact(uv_space[0], 0.0);
    assert_exact(uv_space[1], 0.0);

    let uv_a = get_char_uv('A').expect("A uv");
    assert!(uv_a[0] >= 0.0 && uv_a[2] <= 1.0);
    assert!(uv_a[1] >= 0.0 && uv_a[3] <= 1.0);

    assert!(get_char_uv('\u{1000}').is_none());
}
