//! Borrowed-plane packing must preserve exact codec bytes and switch order.
//! A 17-square/two-group fixture crosses tile boundaries in under 20 KiB;
//! this catches a plane swap/truncation without a solve, assets or a GPU.
use crate::engine::package::{compact_lightmaps, ktx2::f32_to_f16_bits};

#[test]
fn borrowed_planes_preserve_compact_bytes_and_reject_incomplete_groups() -> Result<(), String> {
    let mut layers = Vec::new();
    for group in 0_u16..2 {
        let mut mean = Vec::new();
        let mut moment = Vec::new();
        for texel in 0_u16..289 {
            let value = f32::from(texel % 17).mul_add(0.125_f32, f32::from(group));
            for channel in [value, 0.25_f32, 0.5_f32, 0.5_f32] {
                mean.extend_from_slice(&f32_to_f16_bits(channel).to_le_bytes());
            }
            for channel in [-value * 0.25_f32, 0.0_f32, 0.0_f32, 0.5_f32] {
                moment.extend_from_slice(&f32_to_f16_bits(channel).to_le_bytes());
            }
        }
        layers.push(mean);
        layers.push(moment);
    }
    let flat: Vec<u8> = layers.iter().flatten().copied().collect();
    let expected = compact_lightmaps::encode_with_stats(17, 1, 2, &flat)?;
    assert_eq!(
        compact_lightmaps::encode_layers_with_stats(17, 1, 2, &layers)?,
        expected,
        "borrowing separate state planes preserves every encoded byte and tile mode"
    );
    let _removed = layers.pop();
    assert!(
        compact_lightmaps::encode_layers_with_stats(17, 1, 2, &layers).is_err(),
        "missing switch plane must be rejected before encoding"
    );
    layers.push(vec![0; 8]);
    assert!(
        compact_lightmaps::encode_layers_with_stats(17, 1, 2, &layers).is_err(),
        "truncated plane must be rejected before encoding"
    );
    Ok(())
}
