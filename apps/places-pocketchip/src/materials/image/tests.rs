//! Downscaling tests: the runtime quality budget's only image operation.

// Test code: unwrap/expect, indexing, loose casts and permissive arithmetic are idiomatic in tests;
// the production lints stay enforced everywhere else in the crate.
#![allow(
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::panic
)]

use super::*;

/// A `size` x `size` RGBA image whose texel at `(x, y)` encodes its own
/// coordinates, so a downscaled result can be checked exactly.
fn coordinate_image(size: u32) -> RawImage {
    let mut rgba = Vec::new();
    for y in 0..size {
        for x in 0..size {
            rgba.extend_from_slice(&[
                u8::try_from(x % 256).unwrap_or(0),
                u8::try_from(y % 256).unwrap_or(0),
                128,
                255,
            ]);
        }
    }
    RawImage::new(size, size, rgba)
}

/// A solid image: every downscale must reproduce the colour exactly.
fn solid_image(width: u32, height: u32, color: [u8; 4]) -> RawImage {
    let mut rgba = Vec::new();
    for _ in 0..width.saturating_mul(height) {
        rgba.extend_from_slice(&color);
    }
    RawImage::new(width, height, rgba)
}

#[test]
fn an_image_within_budget_is_left_alone() {
    let image = coordinate_image(64);
    assert!(image.downscaled_to(64).is_none());
    assert!(image.downscaled_to(1024).is_none());
    assert!(image.downscaled_to(256).is_none());
}

#[test]
fn a_zero_budget_or_zero_sized_image_is_refused() {
    assert!(coordinate_image(8).downscaled_to(0).is_none());
    assert!(RawImage::new(0, 0, Vec::new()).downscaled_to(256).is_none());
}

#[test]
fn a_power_of_two_sheet_halves_exactly() {
    let image = solid_image(1024, 1024, [10, 20, 30, 255]);
    let half = image.downscaled_to(512).expect("1024 needs scaling to 512");
    assert_eq!((half.width, half.height), (512, 512));
    assert_eq!(half.rgba.len(), 512 * 512 * 4);
    assert!(
        half.rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|texel| *texel == [10, 20, 30, 255])
    );

    let quarter = image.downscaled_to(256).expect("1024 needs scaling to 256");
    assert_eq!((quarter.width, quarter.height), (256, 256));
    assert!(
        quarter
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|texel| *texel == [10, 20, 30, 255])
    );
}

#[test]
fn downscaling_averages_the_covered_texels() {
    // 4x4 with two distinct columns: a 2x2 result must average each 2x2 block.
    let mut image = solid_image(4, 4, [0, 0, 0, 255]);
    for y in 0..4 {
        for x in 0..4 {
            let offset = usize::try_from(y * 4 + x).unwrap_or(0) * 4;
            let value = if x < 2 { 40 } else { 200 };
            image.rgba[offset] = value;
        }
    }
    let half = image.downscaled_to(2).expect("4 needs scaling to 2");
    assert_eq!((half.width, half.height), (2, 2));
    assert_eq!(half.rgba[0], 40);
    assert_eq!(half.rgba[4], 200);
}

#[test]
fn non_power_of_two_edges_keep_their_aspect_and_round_up() {
    let wide = solid_image(96, 64, [1, 2, 3, 255]);
    assert!(wide.downscaled_to(128).is_none(), "96x64 already fits 128");
    let scaled = solid_image(300, 100, [7, 7, 7, 255])
        .downscaled_to(128)
        .expect("300 needs scaling to 128");
    // Smallest integer factor that fits: ceil(300/128) = 3 → 100x34.
    assert_eq!((scaled.width, scaled.height), (100, 34));
    assert!(
        scaled
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .all(|texel| *texel == [7, 7, 7, 255])
    );
}

#[test]
fn downscaling_is_deterministic_and_never_leaves_a_stale_buffer() {
    let image = coordinate_image(256);
    let first = image.downscaled_to(64).expect("256 needs scaling to 64");
    let second = image.downscaled_to(64).expect("256 needs scaling to 64");
    assert_eq!(first, second);
    assert_eq!(first.rgba.len(), 64 * 64 * 4);
    // The top-left output texel averages source 0..3 on both axes: (6+2)/4 = 2.
    assert_eq!(first.rgba.get(..4), Some([2u8, 2, 128, 255].as_slice()));
}

#[test]
fn a_fully_transparent_decal_sheet_keeps_its_alpha_when_scaled() {
    let mut image = solid_image(128, 128, [255, 255, 255, 0]);
    image.rgba[3] = 255; // one opaque texel
    let scaled = image.downscaled_to(32).expect("128 needs scaling to 32");
    assert_eq!((scaled.width, scaled.height), (32, 32));
    let opaque = scaled
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|texel| texel[3] > 0)
        .count();
    assert!(opaque <= 1, "one opaque texel stays at most one");
}
