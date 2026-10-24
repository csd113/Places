//! Uniform tile storage must preserve the old decoder's half bits, including
//! negative zero, across short boundary tiles and independent circuit groups.
//! This <10 KiB fixture adds no bake/GPU/assets and runs in milliseconds.

use crate::engine::package::compact_lightmaps;

fn legacy_constants() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"PLC1");
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&16_u16.to_le_bytes());
    for field in [17_u32, 1, 2, 0] {
        bytes.extend_from_slice(&field.to_le_bytes());
    }
    for group in 0_u16..2 {
        for y in [0_u16, 16] {
            for x in [0_u16, 16] {
                bytes.extend_from_slice(&[1, 0, 0, 0]);
                for minimum in [
                    f32::from(group),
                    0.25_f32,
                    0.5_f32,
                    -0.125_f32,
                    -0.0_f32,
                    0.0_f32,
                ] {
                    bytes.extend_from_slice(&minimum.to_le_bytes());
                    bytes.extend_from_slice(&0.0_f32.to_le_bytes());
                }
                let count = usize::from(17_u16.saturating_sub(x).min(16))
                    .saturating_mul(usize::from(17_u16.saturating_sub(y).min(16)));
                bytes.extend(std::iter::repeat_n(0_u8, count.saturating_mul(8)));
            }
        }
    }
    bytes
}

#[test]
fn constant_tiles_preserve_legacy_half_bits_and_repack_idempotently() -> Result<(), String> {
    let legacy = legacy_constants();
    let expected = compact_lightmaps::decode(&legacy)?;
    let migrated = compact_lightmaps::repack_constants(legacy)?;
    assert_eq!(
        migrated.len(),
        152,
        "eight uniform tiles store twelve coefficient bytes each"
    );
    let actual = compact_lightmaps::decode(&migrated)?;
    assert_eq!(
        actual.rgba16f_planes, expected.rgba16f_planes,
        "every old decoded coefficient/alpha bit is retained"
    );
    assert_eq!(
        actual.stats.constant_blocks, 8,
        "partial tiles and both circuit groups are compacted"
    );
    assert_eq!(
        compact_lightmaps::repack_constants(migrated.clone())?,
        migrated,
        "migration is idempotent"
    );
    let encoded = compact_lightmaps::encode(17, 1, 2, &expected.rgba16f_planes)?;
    assert_eq!(
        compact_lightmaps::decode(&encoded)?.rgba16f_planes,
        expected.rgba16f_planes,
        "new uniform-tile encoder retains historical signed-zero behavior"
    );
    Ok(())
}

#[test]
fn constant_tile_framing_and_coefficients_fail_closed() -> Result<(), String> {
    let migrated = compact_lightmaps::repack_constants(legacy_constants())?;
    for length in 0..migrated.len() {
        let truncated = migrated.get(..length).ok_or("truncation range")?;
        assert!(
            compact_lightmaps::decode(truncated).is_err(),
            "truncated constant tile framing must fail at {length}"
        );
    }
    let mut wrong_revision = migrated.clone();
    wrong_revision
        .get_mut(4..6)
        .ok_or("revision field missing")?
        .copy_from_slice(&1_u16.to_le_bytes());
    assert!(
        compact_lightmaps::decode(&wrong_revision).is_err(),
        "legacy revision cannot silently accept the new mode"
    );
    let mut nonfinite = migrated;
    nonfinite
        .get_mut(28..30)
        .ok_or("constant coefficient missing")?
        .copy_from_slice(&0x7c00_u16.to_le_bytes());
    assert!(
        compact_lightmaps::decode(&nonfinite).is_err(),
        "nonfinite constant irradiance is refused"
    );
    assert!(
        compact_lightmaps::repack_constants(nonfinite).is_err(),
        "migration must validate constant coefficients too"
    );
    Ok(())
}
