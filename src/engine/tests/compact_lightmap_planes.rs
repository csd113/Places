//! Borrowed-plane packing must preserve exact codec bytes and switch order.
//! A 17-square/two-group fixture crosses tile boundaries in under 20 KiB;
//! this catches a plane swap/truncation without a solve, assets or a GPU.
use crate::engine::lighting::lightmap::{LightmapPage, LightmapTexel};
use crate::engine::package::{compact_lightmaps, ktx2::f32_to_f16_bits};
use std::sync::Arc;

#[test]
fn borrowed_planes_preserve_compact_bytes_and_reject_incomplete_groups() -> Result<(), String> {
    let mut layers = Vec::new();
    let mut editable = Vec::new();
    let mut packed = Vec::new();
    for group in 0_u16..2 {
        let mut mean = Vec::new();
        let mut moment = Vec::new();
        let mut pixels = Vec::new();
        for texel in 0_u16..289 {
            let value = f32::from(texel % 17).mul_add(0.1253_f32, f32::from(group) + 0.0001_f32);
            pixels.push(LightmapTexel {
                irradiance: [value, 0.25, 0.5],
                direction: [-value * 0.25, 0.0, 0.0],
                axis: [0.5, 0.5],
            });
            for channel in [value, 0.25_f32, 0.5_f32, 0.5_f32] {
                mean.extend_from_slice(&f32_to_f16_bits(channel).to_le_bytes());
            }
            for channel in [-value * 0.25_f32, 0.0_f32, 0.0_f32, 0.5_f32] {
                moment.extend_from_slice(&f32_to_f16_bits(channel).to_le_bytes());
            }
        }
        editable.push(LightmapPage {
            width: 17,
            height: 17,
            texels: pixels.into(),
        });
        let backing: Vec<u8> = mean.iter().chain(&moment).copied().collect();
        let length = backing.len();
        packed.push(LightmapPage::from_packed_planes(
            17,
            17,
            Arc::new(backing),
            0..length,
        )?);
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
    for pages in [&editable, &packed] {
        let references: Vec<_> = pages.iter().collect();
        assert_eq!(
            compact_lightmaps::encode_pages_with_stats(17, 1, 2, &references)?,
            expected,
            "page encoding must preserve half rounding, tile decisions and circuit order"
        );
        assert!(
            compact_lightmaps::encode_pages_with_stats(
                17,
                1,
                2,
                references.get(..1).ok_or("missing fixture page")?
            )
            .is_err()
        );
    }
    assert!(
        packed.iter().all(|page| !page.texels.is_materialized()),
        "export must borrow half planes without materializing another full float atlas"
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
