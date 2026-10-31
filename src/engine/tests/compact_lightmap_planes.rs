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

// Protects the measured repeated-switch-page owner: exact runtime planes and
// group order, one allocation per unique encoded pair, copy-on-edit isolation,
// and cancellation/malformed framing. Six 17x17 pages; no bake, asset or GPU.
#[test]
fn runtime_shared_pairs_preserve_payload_order_and_edit_isolation() -> Result<(), String> {
    use crate::engine::lighting::lightmap::{LevelLightmaps, LightmapStats, SwitchableLightmaps};
    use crate::engine::package::lightmaps::{read_lightmaps_owned, write_lightmaps};
    let first = shared_pair_fixture_page(0.5);
    let second = shared_pair_fixture_page(4.0);
    let maps = LevelLightmaps {
        pages: vec![first.clone(), second.clone(), first.clone()],
        charts: Vec::new(),
        stats: LightmapStats::default(),
        cache_key: "shared-pair-regression".into(),
        padding: 1,
        switchable: vec![SwitchableLightmaps {
            light_index: 3,
            pages: vec![second, first.clone(), first],
        }],
    };
    let (meta, encoded) = write_lightmaps(&maps)?;
    let expected = compact_lightmaps::decode(&encoded)?;
    let mut actual = read_lightmaps_owned(&meta, encoded.clone())?;
    let pages: Vec<_> = actual
        .pages
        .iter()
        .chain(actual.switchable.iter().flat_map(|group| &group.pages))
        .collect();
    let pair_bytes = 289_usize.saturating_mul(16);
    for (page, expected_pair) in pages
        .iter()
        .zip(expected.rgba16f_planes.chunks_exact(pair_bytes))
    {
        assert_eq!(
            page.rgba16f_planes(),
            Some(expected_pair),
            "all decoded HDR/moment/alpha bits and group indices remain exact"
        );
        assert!(
            !page.texels.is_materialized(),
            "runtime must retain packed storage"
        );
    }
    assert_eq!(
        actual.retained_pixel_bytes(),
        pair_bytes.saturating_mul(2),
        "six logical pages have exactly two pixel owners"
    );
    check_gpu_pair_plan(&actual, &maps, pair_bytes)?;
    let first_bytes = actual
        .pages
        .first()
        .and_then(LightmapPage::rgba16f_planes)
        .ok_or("missing first plane")?
        .to_vec();
    actual
        .pages
        .get_mut(2)
        .ok_or("missing edit alias")?
        .set_texel(0, 0, LightmapTexel::ZERO);
    assert_eq!(
        actual.pages.first().and_then(LightmapPage::rgba16f_planes),
        Some(first_bytes.as_slice()),
        "editing an alias cannot mutate another circuit/page"
    );
    let edited_plan = crate::engine::render::pair_upload_plan(&actual);
    assert_eq!(edited_plan.sources, vec![0, 1, 2]);
    assert_eq!(
        edited_plan.layers.get(..6),
        Some([0, 2, 4, 2, 0, 0].as_slice())
    );
    let edited_bytes = actual
        .pages
        .get(2)
        .ok_or("missing edited page")?
        .retained_pixel_bytes();
    assert_eq!(
        actual.retained_pixel_bytes(),
        pair_bytes.saturating_mul(2).saturating_add(edited_bytes),
        "only the edited page adds its actual float allocation, including spare capacity"
    );
    assert!(
        read_lightmaps_owned(
            &meta,
            encoded
                .get(..encoded.len().saturating_sub(1))
                .ok_or("truncated fixture")?
                .to_vec()
        )
        .is_err(),
        "whole framing fails before partial publication"
    );
    let _scope =
        crate::engine::resources::enter(Some(crate::engine::resources::DecodeBudget::new(
            Arc::new(std::sync::atomic::AtomicBool::new(true)),
        )));
    assert!(
        read_lightmaps_owned(&meta, encoded).is_err(),
        "cancelled pair decoding cannot publish an atlas"
    );
    Ok(())
}

fn shared_pair_fixture_page(energy: f32) -> LightmapPage {
    LightmapPage {
        width: 17,
        height: 17,
        texels: (0_u16..289)
            .map(|i| LightmapTexel {
                irradiance: [f32::from(i % 17).mul_add(0.03125_f32, energy), 0.25, 2.0],
                direction: [-0.125, 0.0, -0.0],
                axis: [0.5, 0.5],
            })
            .collect::<Vec<_>>()
            .into(),
    }
}

fn check_gpu_pair_plan(
    actual: &crate::engine::lighting::lightmap::LevelLightmaps,
    original: &crate::engine::lighting::lightmap::LevelLightmaps,
    pair_bytes: usize,
) -> Result<(), String> {
    let pages: Vec<_> = actual
        .pages
        .iter()
        .chain(actual.switchable.iter().flat_map(|group| &group.pages))
        .collect();
    // Essential GPU regression: shared nonblank pairs across pages/circuits
    // must resolve exact HDR/moment/alpha bits, while edits regain unique storage.
    // Extends this existing six-page 17-square fixture; no new bake or GPU work.
    let plan = crate::engine::render::pair_upload_plan(actual);
    assert_eq!(plan.sources, vec![0, 1]);
    assert_eq!(plan.layers.get(..6), Some([0, 2, 0, 2, 0, 0].as_slice()));
    let stats = crate::engine::render::lightmap_upload_stats(Some(actual));
    assert_eq!(stats.logical_pairs, 6);
    assert_eq!(stats.physical_pairs, 2);
    assert_eq!(stats.resident_bytes, pair_bytes.saturating_mul(2));
    assert_eq!(stats.lookup_bytes, 800);
    for (logical, layer) in plan.layers.iter().take(6).enumerate() {
        let physical = usize::try_from(layer / 2).map_err(|error| error.to_string())?;
        let source = *plan
            .sources
            .get(physical)
            .ok_or("missing physical source")?;
        assert_eq!(
            pages.get(logical).and_then(|page| page.rgba16f_planes()),
            pages.get(source).and_then(|page| page.rgba16f_planes())
        );
    }
    let editable_plan = crate::engine::render::pair_upload_plan(original);
    assert_eq!(
        editable_plan.sources.len(),
        6,
        "editable pages cannot alias mutable storage"
    );
    Ok(())
}
