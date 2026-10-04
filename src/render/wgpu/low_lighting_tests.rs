//! Real resident-resource checks for lighting independent of texture quality.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    reason = "These isolated preference tests assert exact values and fail immediately on invalid GPU setup or fixture indices."
)]

use super::*;
use crate::settings::Settings;

#[test]
fn lighting_only_change_rebuilds_material_response_without_refitting_textures() {
    let applied = GraphicsConfig::default();
    let requested = GraphicsConfig {
        lighting_quality: QualityLevel::Low,
        ..applied
    };
    let delta = applied.delta_from(requested);
    assert!(delta.lighting);
    assert!(delta.needs_gpu_work());
    assert!(
        !delta.needs_build(),
        "unchanged lightmaps reuse CPU lighting"
    );
    assert!(
        !delta.needs_texture_refit(),
        "lighting is not texture quality"
    );
    assert!(!delta.quality && !delta.lightmaps && !delta.reflections);
}

fn fixture() -> LoadedLevel {
    let level = crate::level::LevelDef::from_json(
        r#"{"format_version":3,"id":"low_lighting_override","name":"Lighting override",
        "spawn":{"x":1.5,"z":1.5},
        "defaults":{"floor":"core:linoleum_polished_01","wall":"core:metal_brushed_01"},
        "rooms":[{"x":0,"z":0,"width":3,"depth":3,"height":3}],
        "walls":[{"x":0,"z":0,"width":3,"depth":0.18,"height":3}],
        "ceiling_lights":[{"fixture":"core:fluorescent_panel_01","x":1.5,"z":1.5,"align":"none"}],
        "props":[{"model":"outdoor:showcase_stump_seat","x":1,"z":1.5}]}
        "#,
    )
    .expect("fixture parses");
    let root = crate::assets::resolve_asset_root().expect("asset root");
    let mut cache = crate::materials::TextureCache::new();
    let materials = crate::materials::resolve_materials(
        &level,
        crate::render::common::api::shipped_asset_catalog(),
        None,
        Some(&root),
        &mut cache,
    );
    assert!(materials.errors().is_empty(), "{:?}", materials.errors());
    LoadedLevel {
        catalog: Arc::new(crate::loader::PropCatalog::load_default()),
        materials,
        light_sheets: Vec::new(),
        entry: crate::loader::LevelEntry {
            id: level.id.clone(),
            name: level.name.clone(),
            author: String::new(),
            source_type: crate::loader::LevelSourceType::Embedded,
            path: std::path::PathBuf::new(),
        },
        level,
    }
}

fn install(renderer: &mut WgpuRenderer, loaded: &LoadedLevel, settings: &Settings, preserve: bool) {
    let spec = settings.graphics_spec();
    renderer.set_quality(spec.quality);
    renderer.set_lighting_quality(spec.lighting);
    renderer.set_lightmap_quality(spec.lightmaps);
    renderer.set_reflection_quality(spec.reflections);
    renderer.set_texture_filtering(spec.filtering);
    renderer.set_bloom_enabled(spec.bloom);
    assert_eq!(
        renderer.graphics_requested(),
        spec,
        "setters record whole request"
    );
    let mut assets = crate::props::PropAssets::load_default();
    let mut options = LightmapBuildOptions::for_lightmaps(spec.lightmaps);
    // This fixture exercises direct receivers. Diffuse bounce transport has
    // separate tests; bounded workers avoid monopolising desktop QA.
    options.solve.bounces = 0;
    options.solve.workers = 2;
    let build = Arc::new(build_level_geometry_timed_with_lightmaps(
        &loaded.level,
        &loaded.catalog,
        &mut assets,
        &loaded.materials,
        options,
        None,
    ));
    assert!(
        build.lightmap_failure.is_none(),
        "{:?}",
        build.lightmap_failure
    );
    renderer.install_prepared(loaded, build, assets, CharacterScene::new(), preserve);
    for _ in 0_i32..100_i32 {
        if renderer.advance_prepared_install() {
            break;
        }
    }
    assert!(
        renderer.prepared_install.is_none(),
        "bounded install finishes"
    );
    assert_eq!(
        renderer.graphics_applied(),
        spec,
        "resident state matches request"
    );
    assert_eq!(renderer.installed_quality, spec.quality);
    assert_eq!(renderer.installed_lightmaps, spec.lightmaps);
    let textures = renderer.world_textures.as_ref().expect("world textures");
    let mut found_surface = false;
    for slot in 0..textures.stats().unique {
        let texture = textures.entry(slot).expect("texture entry");
        if !texture.meta().fallback && texture.meta().class == TextureClass::Surface {
            assert_eq!(
                texture.meta().level,
                spec.quality,
                "real uploads retain texture quality"
            );
            found_surface = true;
        }
    }
    assert!(found_surface, "real surface PNG was uploaded");
    let material = renderer
        .world_materials
        .as_ref()
        .expect("resolved materials")
        .stats();
    if spec.lighting == QualityLevel::Low {
        assert_eq!(
            material.response_materials, 0,
            "Low suppresses actual GPU material response"
        );
        assert_eq!(
            material.normal_maps, 0,
            "Low suppresses normal-map bindings"
        );
    } else {
        assert!(
            material.response_materials > 0,
            "restored quality restores sheen"
        );
        assert!(
            material.normal_maps > 0,
            "restored quality restores real normal maps"
        );
    }
    assert_eq!(renderer.lightmaps_resident, !spec.lightmaps.is_off());
    assert_eq!(
        renderer.reflections_enabled,
        spec.reflections.draws_probes()
    );
    if spec.reflections == ReflectionQuality::Off {
        assert!(
            renderer.reflection_targets.probes().is_empty(),
            "Off retires cubemaps"
        );
    }
}

#[test]
#[ignore = "requires a native GPU adapter; run explicitly on a desktop host"]
fn high_on_medium_off_high_and_repeated_toggles_install_real_resources() {
    let loaded = fixture();
    let mut renderer = WgpuRenderer::new_headless(DrawableSize::new(96, 96)).expect("native GPU");
    let mut settings = Settings::default();
    install(&mut renderer, &loaded, &settings, false);
    let _use_low_quality_lighting_changed = settings.set_use_low_quality_lighting(true);
    install(&mut renderer, &loaded, &settings, true);
    let _quality_changed = settings.set_quality(QualityLevel::Medium);
    install(&mut renderer, &loaded, &settings, true);
    let _use_low_quality_lighting_changed_2 = settings.set_use_low_quality_lighting(false);
    install(&mut renderer, &loaded, &settings, true);
    let _quality_changed_2 = settings.set_quality(QualityLevel::High);
    install(&mut renderer, &loaded, &settings, true);
    for enabled in [true, false, true, false] {
        let _use_low_quality_lighting_changed_3 = settings.set_use_low_quality_lighting(enabled);
        install(&mut renderer, &loaded, &settings, true);
    }
    let _quality_changed_3 = settings.set_quality(QualityLevel::Low);
    install(&mut renderer, &loaded, &settings, true);
    for quality in [QualityLevel::Medium, QualityLevel::High] {
        let _use_low_quality_lighting_changed_4 = settings.set_use_low_quality_lighting(true);
        let _quality_changed_4 = settings.set_quality(quality);
        install(&mut renderer, &loaded, &settings, true);
        let _use_low_quality_lighting_changed_5 = settings.set_use_low_quality_lighting(false);
        install(&mut renderer, &loaded, &settings, true);
    }
}

type LowSnapshot = (Vec<[f32; 4]>, Vec<[f32; 4]>, Vec<[f32; 3]>);

fn low_snapshot(renderer: &mut WgpuRenderer) -> LowSnapshot {
    let build = renderer
        .retained_build
        .as_ref()
        .expect("resident CPU build");
    assert!(build.lightmaps.is_none());
    assert!(build.probes.is_none());
    let static_colors = build
        .batches
        .iter()
        .flat_map(|batch| batch.vertices.iter().map(|vertex| vertex.color))
        .collect::<Vec<_>>();
    assert!(
        !static_colors.is_empty(),
        "static real GLB has lit vertices"
    );
    let world_colors = build
        .mesh
        .ranges
        .iter()
        .flat_map(|range| range.vertices.iter().map(|vertex| vertex.color))
        .collect::<Vec<_>>();
    assert!(renderer.spawn_runtime_model(7, "outdoor:showcase_boulder", [1.5, 0.0, 1.5], 0.0, 1.0));
    let id = renderer.runtime_objects[&7];
    let mut dynamic = Vec::new();
    for position in [[0.7, 0.0, 0.7], [1.5, 0.0, 1.5], [2.2, 0.0, 2.2]] {
        assert!(renderer.set_runtime_transform(7, position, 0.0));
        let _update_stats = renderer.update_dynamic(0.0);
        let lighting = renderer
            .dynamic
            .get(id)
            .expect("dynamic survives")
            .entity_lighting()
            .expect("real dynamic lighting");
        assert_eq!(
            lighting.source,
            crate::render::EntityLightingSource::NoField
        );
        assert!(
            lighting.prepared.is_none(),
            "Low never samples Full irradiance"
        );
        dynamic.push(lighting.display);
    }
    (static_colors, world_colors, dynamic)
}

#[test]
#[ignore = "requires a native GPU adapter; run explicitly on a desktop host"]
fn override_matches_actual_low_static_dynamic_and_material_paths() {
    let loaded = fixture();
    let mut renderer = WgpuRenderer::new_headless(DrawableSize::new(96, 96)).expect("native GPU");
    let mut low = Settings::default();
    let _quality_changed = low.set_quality(QualityLevel::Low);
    install(&mut renderer, &loaded, &low, false);
    let baseline = low_snapshot(&mut renderer);
    for quality in [QualityLevel::Medium, QualityLevel::High] {
        let mut settings = Settings::default();
        let _quality_changed_2 = settings.set_quality(quality);
        let _use_low_quality_lighting_changed = settings.set_use_low_quality_lighting(true);
        install(&mut renderer, &loaded, &settings, true);
        assert_eq!(
            low_snapshot(&mut renderer),
            baseline,
            "actual Low lighting is identical at {quality:?} texture quality"
        );
    }
}
