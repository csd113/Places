//! A 16-square imported atlas can fit its byte budget yet exceed array layers.
//! This 520 KiB fixture checks physical identities, without GPU work or a bake.
use super::{upload_stats, validate_limits};
use crate::engine::lighting::lightmap::{
    LevelLightmaps, LightmapPage, LightmapStats, SwitchableLightmaps,
};
use std::sync::Arc;

#[test]
fn imported_atlas_checks_physical_layers_after_exact_aliasing() -> Result<(), String> {
    let pair_bytes = 16_usize * 16 * 16;
    let backing = Arc::new([0_u8, 0, 0, 0, 0, 0, 0, 0x38].repeat(pair_bytes * 130 / 8));
    let page = |index: usize| {
        LightmapPage::from_packed_planes(
            16,
            16,
            Arc::clone(&backing),
            index.saturating_mul(pair_bytes)..index.saturating_add(1).saturating_mul(pair_bytes),
        )
    };
    let pages = |group: usize| {
        (0..26)
            .map(|index| page(group.saturating_mul(26).saturating_add(index)))
            .collect::<Result<Vec<_>, _>>()
    };
    let mut maps = LevelLightmaps {
        pages: pages(0)?,
        charts: Vec::new(),
        stats: LightmapStats::default(),
        cache_key: "physical-layer-limit".into(),
        padding: 1,
        switchable: (1..5)
            .map(|group| {
                Ok(SwitchableLightmaps {
                    light_index: group,
                    pages: pages(group)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    };
    assert_eq!(upload_stats(Some(&maps)).physical_pairs, 130);
    assert!(
        validate_limits(Some(&maps), 8192, 256).is_err(),
        "260 layers must fail before texture allocation"
    );
    for group in &mut maps.switchable {
        group.pages.clone_from(&maps.pages);
    }
    assert_eq!(upload_stats(Some(&maps)).physical_pairs, 26);
    assert!(
        validate_limits(Some(&maps), 8192, 256).is_ok(),
        "logical copies that alias exact immutable planes fit"
    );
    assert!(
        validate_limits(Some(&maps), 8, 256).is_err(),
        "edge limits also apply to imported atlases"
    );
    Ok(())
}
