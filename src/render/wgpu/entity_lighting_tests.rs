//! Deterministic entity-lighting checks through actual renderer resources.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "These isolated native GPU tests assert exact shader reference values and fail immediately on invalid setup or fixture indices."
)]

use super::*;
use crate::gltf::{PropModel, PropSubmesh, PropVertex};
use crate::lighting::probes::{ProbeField, ProbeSample};
use crate::props::{LoadedPropAsset, PropAssets};
use glam::Vec3;

fn fixture() -> LoadedLevel {
    let level = crate::level::LevelDef::from_json(
        r#"{"format_version":3,"id":"entity_gpu","name":"Entity GPU",
        "spawn":{"x":0,"z":0},"rooms":[{"x":-5,"z":-5,"width":10,"depth":10,"height":3}]}"#,
    )
    .expect("level");
    LoadedLevel {
        catalog: Arc::new(crate::loader::PropCatalog::load_default()),
        materials: crate::render::logical_materials(&level),
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

fn prepared(loaded: &LoadedLevel, quality: QualityLevel) -> Arc<LevelBuild> {
    let mut assets = PropAssets::load_default();
    let (mut mesh, batches, lighting, timings) =
        crate::render::common::api::build_level_geometry_timed(
            &loaded.level,
            &loaded.catalog,
            &mut assets,
            &loaded.materials,
        );
    // Isolate neutral entities from unrelated fixture geometry in the readback.
    mesh.ranges.clear();
    mesh.vertex_count = 0;
    mesh.index_count = 0;
    let probes = (quality != QualityLevel::Low).then(|| {
        Arc::new(ProbeField {
            local_direct: None,
            min: [-1.0, 0.0, -1.0],
            cell_m: 2.0,
            dims: [1; 3],
            probes: vec![ProbeSample {
                irradiance: [0.2; 3],
                direction: [0.0, 0.0, 0.3],
                axis: [0.5; 2],
                room: 0,
            }],
        })
    });
    Arc::new(LevelBuild {
        mesh,
        batches,
        lighting,
        timings,
        probes,
        lightmaps: None,
        lightmap_failure: None,
        lightmap_millis: 0.0,
    })
}

fn install(
    renderer: &mut WgpuRenderer,
    loaded: &LoadedLevel,
    quality: QualityLevel,
    preserve: bool,
) {
    renderer.set_quality(quality);
    renderer.set_lightmap_quality(if quality == QualityLevel::Low {
        LightmapQuality::Off
    } else {
        LightmapQuality::Full
    });
    renderer.set_reflection_quality(ReflectionQuality::Off);
    renderer.install_prepared(
        loaded,
        prepared(loaded, quality),
        PropAssets::load_default(),
        CharacterScene::new(),
        preserve,
    );
    for _ in 0_i32..100_i32 {
        if renderer.advance_prepared_install() {
            return;
        }
    }
    assert!(renderer.prepared_install.is_none(), "install completed");
}

fn triangle() -> Arc<LoadedPropAsset> {
    Arc::new(LoadedPropAsset {
        model_path: "neutral-test-triangle".to_string(),
        model: PropModel {
            vertices: [[-0.7, 0.3, 0.0], [0.7, 0.3, 0.0], [0.0, 1.7, -0.01]]
                .map(|pos| PropVertex {
                    normal: None,
                    pos,
                    color: [1.0; 4],
                    uv: [0.0; 2],
                })
                .to_vec(),
            indices: vec![0, 1, 2],
            submeshes: vec![PropSubmesh {
                response: crate::materials::MaterialResponse::NONE,
                material: 0,
                texture: None,
                emission: crate::materials::MaterialEmission::NONE,
                alpha: crate::materials::MaterialAlpha::OPAQUE,
                first_index: 0,
                index_count: 3,
            }],
            triangles: 1,
            materials: 1,
            ..PropModel::default()
        },
    })
}

fn actor(renderer: &mut WgpuRenderer) {
    renderer
        .spawn_runtime_character("neutral-rat", "rat", [1.0, 1.0, 0.0], 0.0, 1.0)
        .expect("character");
    drop(renderer.update_characters(0.0, LocomotionSnapshot::default(), &[]));
}

fn capture(renderer: &mut WgpuRenderer) -> crate::loader::RawImage {
    capture_at(renderer, [0.0, 1.0, 0.0])
}

fn capture_at(renderer: &mut WgpuRenderer, position: [f32; 3]) -> crate::loader::RawImage {
    let eye = Vec3::from_array(position) + Vec3::new(0.0, 0.0, 3.0);
    renderer.render_scene(RenderCamera::new(eye, 0.0, 0.0, 60.0));
    // Force the direct capture pass: a headless device has no presented image.
    renderer.post = None;
    renderer
        .capture_default_framebuffer()
        .expect("frame readback")
}

#[test]
#[ignore = "requires a native GPU adapter; run explicitly on a desktop host"]
fn entity_resources_restore_after_quality_cycles_and_map_reloads() {
    let loaded = fixture();
    let mut renderer = WgpuRenderer::new_headless(DrawableSize::new(128, 128)).expect("GPU");
    install(&mut renderer, &loaded, QualityLevel::High, false);
    let id = renderer
        .dynamic
        .spawn(&triangle(), [0.0; 3], 0.0, 1.0, 0.0)
        .expect("dynamic");
    let _update_stats = renderer.update_dynamic(0.0);
    renderer.upload_dynamic();
    actor(&mut renderer);
    let initial = renderer
        .world_dynamic
        .as_ref()
        .unwrap()
        .diagnostic_uniform(0)
        .unwrap()
        .entity_irradiance;
    assert_eq!(initial, [0.2, 0.2, 0.2, 1.0]);
    let image = capture(&mut renderer);
    let baseline = image.rgba;
    for quality in [
        QualityLevel::Medium,
        QualityLevel::Low,
        QualityLevel::Medium,
        QualityLevel::High,
    ] {
        install(&mut renderer, &loaded, quality, true);
        assert!(renderer.dynamic.get(id).is_some());
        assert!(
            renderer
                .characters
                .runtime_character("neutral-rat")
                .is_some(),
            "runtime actors survive graphics rebuilds"
        );
        let gpu = renderer
            .world_dynamic
            .as_ref()
            .unwrap()
            .diagnostic_uniform(0)
            .unwrap();
        let character_gpu = renderer
            .world_characters
            .as_ref()
            .unwrap()
            .diagnostic_uniform("neutral-rat", None)
            .unwrap();
        assert_eq!(gpu.entity_irradiance, character_gpu.entity_irradiance);
        assert_eq!(gpu.entity_moment, character_gpu.entity_moment);
        if quality == QualityLevel::Low {
            assert_eq!(gpu.entity_irradiance, [0.0, 0.0, 0.0, -1.0]);
        } else {
            assert_eq!(gpu.entity_irradiance, initial);
        }
        let quality_image = capture(&mut renderer);
        if quality == QualityLevel::High {
            assert_eq!(
                quality_image.rgba, baseline,
                "restored render matches direct High"
            );
        }
    }
    for _ in 0_i32..3_i32 {
        let mut other = loaded.clone();
        other.level.id = "empty-level".to_string();
        install(&mut renderer, &other, QualityLevel::Low, false);
        assert!(renderer.dynamic.objects().is_empty());
        assert!(renderer.characters.is_empty());
        assert!(renderer.dynamic_field.is_none());
        install(&mut renderer, &loaded, QualityLevel::High, false);
        assert!(renderer.dynamic_field.is_some());
    }
}

#[test]
#[ignore = "requires a native GPU adapter; run explicitly on a desktop host"]
fn directional_entity_irradiance_reaches_real_rendered_pixels() {
    let loaded = fixture();
    let mut renderer = WgpuRenderer::new_headless(DrawableSize::new(128, 128)).expect("GPU");
    install(&mut renderer, &loaded, QualityLevel::High, false);
    // A sloped face differs from the import's default +Z vertex normal.
    let mut asset = triangle();
    Arc::get_mut(&mut asset).unwrap().model.vertices[2].pos[2] = -1.0;
    let mut field = renderer.dynamic_field.as_ref().unwrap().as_ref().clone();
    field.probes[0].direction = [0.0, 0.3, 0.0];
    renderer.dynamic_field = Some(Arc::new(field));
    let _spawn_status = renderer
        .dynamic
        .spawn(&asset, [0.0; 3], 0.0, 1.0, 0.0)
        .expect("triangle");
    let _update_stats = renderer.update_dynamic(0.0);
    renderer.upload_dynamic();
    let bright = capture(&mut renderer);
    // The geometric normal is (0,1,1.4) normalized. Its +Y moment gives
    // ~0.216 diffuse; using the default +Z normal would incorrectly give 0.1.
    let texel = renderer.dynamic_field.as_ref().unwrap().probes[0].texel();
    let normal = Vec3::new(0.0, 1.0, 1.4).normalize().to_array();
    let expected = texel.light_at(normal)[0];
    assert!(expected > 0.21 && expected < 0.22);
    let mut reversed_field = renderer.dynamic_field.as_ref().unwrap().as_ref().clone();
    reversed_field.probes[0].direction = [0.0, -0.3, 0.0];
    renderer.dynamic_field = Some(Arc::new(reversed_field));
    let _update_stats_2 = renderer.update_dynamic(0.0);
    let dark = capture(&mut renderer);
    if let Ok(directory) = std::env::var("PLACES_ENTITY_TEST_CAPTURES") {
        let capture_path = std::path::Path::new(&directory);
        std::fs::create_dir_all(capture_path).expect("capture dir");
        std::fs::write(
            capture_path.join("entity-facing-light.png"),
            crate::materials::encode_png(&bright).expect("encode bright"),
        )
        .expect("bright PNG");
        std::fs::write(
            capture_path.join("entity-away-from-light.png"),
            crate::materials::encode_png(&dark).expect("encode dark"),
        )
        .expect("dark PNG");
    }
    let centre = (64 * 128 + 64) * 4;
    assert!(
        expected
            .mul_add(-255.0, f32::from(bright.rgba[centre]))
            .abs()
            < 2.0,
        "posed face normal reaches pixels"
    );
    assert!(
        bright.rgba[centre] > dark.rgba[centre] + 20,
        "moment sign changes diffuse pixels: {} vs {}",
        bright.rgba[centre],
        dark.rgba[centre]
    );
}

fn archive_bytes(archive: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>, name: &str) -> Vec<u8> {
    use std::io::Read;
    let mut bytes = Vec::new();
    let _by_name_status = archive
        .by_name(name)
        .expect("entry")
        .read_to_end(&mut bytes)
        .expect("bytes");
    bytes
}

fn movement_trace(renderer: &mut WgpuRenderer, position: [f32; 3]) -> serde_json::Value {
    let id = renderer.dynamic.objects()[0].id();
    let local = renderer.dynamic.objects()[0].mesh().centre;
    let mut jumps = [0.0_f32; 3];
    let mut source_changes = [0_i32; 3];
    for axis in 0..3 {
        let mut previous: Option<crate::render::common::light_transport::EntityLighting> = None;
        for step in 0..=100_u16 {
            let mut centre = position;
            centre[axis] += f32::from(step).mul_add(0.01, -0.5);
            let offset = std::array::from_fn(|i| centre[i] - local[i]);
            assert!(renderer.dynamic.set_transform(id, offset, 0.0, 1.0));
            let _update_stats = renderer.update_dynamic(0.0);
            let sample = renderer.dynamic.get(id).unwrap().entity_lighting().unwrap();
            assert!(sample.display.iter().all(|v| v.is_finite()));
            if let Some(last) = previous {
                if last.source != sample.source {
                    source_changes[axis] += 1_i32;
                }
                let jump = sample
                    .display
                    .iter()
                    .zip(last.display)
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0_f32, f32::max);
                jumps[axis] = jumps[axis].max(jump);
            }
            let _update_stats_2 = renderer.update_dynamic(0.0);
            assert_eq!(
                renderer.dynamic.get(id).unwrap().entity_lighting(),
                Some(sample)
            );
            previous = Some(sample);
        }
    }
    // Restore the captured pose after tracing 1 cm movements on all axes.
    let offset = std::array::from_fn(|i| position[i] - local[i]);
    assert!(renderer.dynamic.set_transform(id, offset, 0.0, 1.0));
    let _update_stats_3 = renderer.update_dynamic(0.0);
    serde_json::json!({"max_display_step_xyz":jumps,"source_changes_xyz":source_changes})
}

fn location_trace(
    renderer: &mut WgpuRenderer,
    name: &str,
    position: [f32; 3],
) -> serde_json::Value {
    let asset = triangle();
    let local = crate::render::common::dynamic::DynamicMesh::from_asset(&asset)
        .expect("mesh")
        .centre;
    let offset = std::array::from_fn(|axis| position[axis] - local[axis]);
    renderer.dynamic.clear();
    let _spawn_status = renderer
        .dynamic
        .spawn(&asset, offset, 0.0, 1.0, 0.0)
        .expect("neutral entity");
    let _update_stats = renderer.update_dynamic(0.0);
    renderer.upload_dynamic();
    let lighting = renderer.dynamic_lighting.as_ref().unwrap();
    let field = renderer.dynamic_field.as_ref().unwrap();
    let room = lighting
        .room_index_at_height(position[0], position[1], position[2])
        .filter(|r| lighting.probe_region_at(*r, position).is_some());
    let candidates = field.sample_diagnostics_with_rooms(position, None, |p, label| {
        room.is_some() && lighting.labelled_probe_visible_from(position, p, label)
    });
    let sample = renderer.dynamic.objects()[0]
        .entity_lighting()
        .expect("sample");
    let gpu = renderer
        .world_dynamic
        .as_ref()
        .unwrap()
        .diagnostic_uniform(0)
        .unwrap();
    if let Some(texel) = sample.prepared {
        assert_eq!(gpu.entity_irradiance[..3], texel.irradiance);
        assert_eq!(gpu.entity_moment[..3], texel.direction);
    }
    let gpu_energy = gpu.entity_irradiance;
    let gpu_moment = gpu.entity_moment;
    let directional = sample.prepared.map(|t| t.light_at([0.0, 0.0, 1.0]));
    let baseline = lighting.baseline_in_room(room.unwrap_or(0), position[0], position[2]);
    let old_floor = sample
        .display
        .iter()
        .zip([baseline.r, baseline.g, baseline.b])
        .map(|(v, floor)| v.max(floor))
        .collect::<Vec<_>>();
    let movement = movement_trace(renderer, position);
    let image = capture_at(renderer, position);
    if let Ok(directory) = std::env::var("PLACES_ENTITY_TEST_CAPTURES") {
        std::fs::write(
            std::path::Path::new(&directory).join(format!("demo-{name}.png")),
            crate::materials::encode_png(&image).expect("encode"),
        )
        .expect("capture");
    }
    serde_json::json!({"name": name, "position": position, "room": room,
        "source": format!("{:?}", sample.source), "display_isotropic": sample.display,
        "old_floor_applied_to_new_sample": old_floor,
        "authored_baseline": [baseline.r,baseline.g,baseline.b],
        "interpolated_energy": sample.prepared.map(|t| t.irradiance),
        "interpolated_moment": sample.prepared.map(|t| t.direction),
        "gpu_energy": gpu_energy, "gpu_moment": gpu_moment,
        "pre_tonemap_facing_z": directional,
        "movement_1cm": movement,
        "centre_pixel_rgba": &image.rgba[(64 * 128 + 64) * 4..(64 * 128 + 64) * 4 + 4],
        "candidates": candidates.iter().map(|c| serde_json::json!({"id":c.id,
            "position":c.world_position,"distance_m":c.distance_m,"weight":c.weight,
            "raw_decoded_energy":c.probe.irradiance,"raw_decoded_moment":c.probe.direction,
            "room":c.probe.room})).collect::<Vec<_>>()})
}

#[test]
#[ignore = "requires a native GPU adapter; explicit compiled-demo diagnostic"]
fn compiled_demo_probe_to_pixel_comparison() {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(
        include_bytes!("../../../assets/levels/places_demo.placesmap").as_slice(),
    ))
    .expect("archive");
    let manifest: serde_json::Value =
        serde_json::from_slice(&archive_bytes(&mut archive, "manifest.json")).expect("manifest");
    let mut renderer = WgpuRenderer::new_headless(DrawableSize::new(128, 128)).expect("GPU");
    install(&mut renderer, &fixture(), QualityLevel::High, false);
    let mut reports = Vec::new();
    for variant in manifest["variants"].as_array().expect("variants") {
        let Some(entry) = variant["entries"]["irradiance"].as_str() else {
            continue;
        };
        let raw = archive_bytes(&mut archive, entry);
        let field = ProbeField::read(&raw).expect("decode");
        assert_eq!(
            field.write().expect("reencode"),
            raw,
            "all record bytes preserved"
        );
        let lighting = archive_bytes(
            &mut archive,
            variant["entries"]["lighting"]
                .as_str()
                .expect("lighting entry"),
        );
        renderer.dynamic_lighting =
            Some(crate::package::lighting::read_lighting(&lighting).expect("lighting"));
        renderer.dynamic_field = Some(Arc::new(field));
        let locations = [
            ("bright-room", [3.0, 1.5, 3.0]),
            ("moderate-room", [14.0, 1.5, 3.5]),
            ("dim-room", [45.0, 1.0, 13.0]),
            ("dark-hall", [61.0, 0.5, -5.0]),
            ("home", [59.0, 1.0, 8.0]),
            ("pool", [14.0, 0.0, 13.0]),
            ("stairs", [21.0, 0.5, 3.0]),
            ("outdoor", [12.0, 1.0, -30.0]),
            ("doorway", [9.0, 1.0, 3.6]),
            ("door-jamb-boundary", [9.0, 1.0, 3.0]),
            ("pumpkin", [13.5, 0.3, -83.0]),
            ("skeleton", [9.6, 0.9, -52.0]),
        ];
        let traces = locations
            .into_iter()
            .map(|(name, p)| {
                location_trace(
                    &mut renderer,
                    &format!(
                        "{}-{name}",
                        variant["lightmap_quality"].as_str().expect("quality")
                    ),
                    p,
                )
            })
            .collect::<Vec<_>>();
        reports.push(serde_json::json!({"variant":variant,"traces":traces}));
    }
    if let Ok(path) = std::env::var("PLACES_ENTITY_PROBE_REPORT") {
        std::fs::write(path, serde_json::to_vec_pretty(&reports).expect("json")).expect("report");
    }
}

/// Install the real compiled geometry and atlas, so visual checks retain the
/// surroundings against which an entity's illumination must make sense.
fn install_compiled_scene(
    renderer: &mut WgpuRenderer,
    level_id: &str,
    quality: QualityLevel,
    preserve: bool,
) {
    let manager = crate::loader::LevelManager::new();
    let entry = manager
        .entries()
        .iter()
        .find(|entry| entry.id == level_id)
        .expect("level");
    let loaded = manager.load_level(entry).expect("loaded level");
    let manifest = crate::package::world::inspect(&entry.path).expect("manifest");
    let lightmaps = match quality {
        QualityLevel::Low => LightmapQuality::Off,
        QualityLevel::Medium => LightmapQuality::Medium,
        QualityLevel::High => LightmapQuality::Full,
    };
    let mut assets = PropAssets::load_default();
    let mut variant =
        crate::package::world::load_variant(&entry.path, &manifest, lightmaps, &mut assets)
            .expect("compiled variant");
    // Use the production claim list to remove authored moving actors' static
    // bind-pose batches from this controlled fixture. Otherwise those bodies
    // occlude a neutral test model at the exact same anchor. Native full-scene
    // captures separately retain the authored actors and normal gameplay path.
    let authored = CharacterScene::spawn_characters_with_field(
        &loaded.level,
        &loaded.catalog,
        &mut assets,
        &variant.lighting,
        variant.irradiance.as_deref(),
    );
    variant
        .props
        .retain(|batch| !authored.claimed_models().contains(&batch.model));
    let build = Arc::new(LevelBuild {
        mesh: variant.mesh,
        batches: variant.props,
        lighting: variant.lighting,
        timings: crate::render::BuildTimings::default(),
        probes: variant.irradiance,
        lightmaps: variant.lightmaps,
        lightmap_failure: None,
        lightmap_millis: 0.0,
    });
    renderer.set_quality(quality);
    renderer.set_lightmap_quality(lightmaps);
    renderer.set_reflection_quality(ReflectionQuality::Off);
    renderer.install_prepared(&loaded, build, assets, CharacterScene::new(), preserve);
    for _ in 0_i32..100_i32 {
        if renderer.advance_prepared_install() {
            return;
        }
    }
    assert!(
        renderer.prepared_install.is_none(),
        "compiled install completed"
    );
}

fn scene_capture(renderer: &mut WgpuRenderer, position: [f32; 3]) -> crate::loader::RawImage {
    renderer.post = None;
    renderer.render_scene(RenderCamera::new(
        Vec3::from_array(position) + Vec3::new(0.0, 0.0, 1.5),
        0.0,
        0.0,
        60.0,
    ));
    renderer
        .capture_default_framebuffer()
        .expect("scene readback")
}

fn neutral_at(renderer: &mut WgpuRenderer, position: [f32; 3], shifted_pivot: bool) -> DynamicId {
    let mut asset = triangle();
    if shifted_pivot {
        Arc::get_mut(&mut asset).unwrap().model_path = "neutral-shifted-test-triangle".to_string();
        for vertex in &mut Arc::get_mut(&mut asset).unwrap().model.vertices {
            vertex.pos = (Vec3::from_array(vertex.pos) + Vec3::new(8.0, -5.0, 12.0)).to_array();
        }
    }
    let centre = crate::render::common::dynamic::DynamicMesh::from_asset(&asset)
        .unwrap()
        .centre;
    let offset = (Vec3::from_array(position) - Vec3::from_array(centre) * 0.4).to_array();
    let id = renderer
        .dynamic
        .spawn(&asset, offset, 0.0, 0.4, 0.0)
        .expect("neutral model");
    let _update_stats = renderer.update_dynamic(0.0);
    renderer.upload_dynamic();
    assert!(
        (renderer.dynamic.get(id).unwrap().centre() - Vec3::from_array(position)).length() < 1.0e-5
    );
    id
}

fn compiled_scene_location(
    renderer: &mut WgpuRenderer,
    directory: &str,
    quality: QualityLevel,
    name: &str,
    position: [f32; 3],
) -> serde_json::Value {
    renderer.dynamic.clear();
    renderer.upload_dynamic();
    let baseline = scene_capture(renderer, position);
    let id = neutral_at(renderer, position, false);
    let sample = renderer.dynamic.get(id).unwrap().entity_lighting().unwrap();
    let normal_image = scene_capture(renderer, position);
    renderer.dynamic.clear();
    let shifted = neutral_at(renderer, position, true);
    let shifted_sample = renderer
        .dynamic
        .get(shifted)
        .unwrap()
        .entity_lighting()
        .unwrap();
    assert_eq!(sample.source, shifted_sample.source);
    for (value, other) in sample.display.into_iter().zip(shifted_sample.display) {
        assert!(
            (value - other).abs() < 1.0e-5,
            "asset pivot must not change lighting"
        );
    }
    let shifted_image = scene_capture(renderer, position);
    for (suffix, image) in [
        ("environment", baseline),
        ("neutral", normal_image),
        ("pivot", shifted_image),
    ] {
        let path = std::path::Path::new(directory).join(format!("{quality:?}-{name}-{suffix}.png"));
        std::fs::write(path, crate::materials::encode_png(&image).expect("PNG")).expect("capture");
    }
    serde_json::json!({"quality": format!("{quality:?}"), "location": name,
                "position": position, "source": format!("{:?}", sample.source), "display": sample.display,
                "energy": sample.prepared.map(|texel| texel.irradiance),
                "moment": sample.prepared.map(|texel| texel.direction)})
}

#[test]
#[ignore = "requires native GPU; actual demo environment and compiled quality transitions"]
fn compiled_environment_neutral_entities_and_quality_restoration() {
    let directory = std::env::var("PLACES_ENTITY_SCENE_CAPTURES").expect("capture directory");
    std::fs::create_dir_all(&directory).expect("directory");
    let mut renderer = WgpuRenderer::new_headless(DrawableSize::new(320, 240)).expect("GPU");
    let locations = [
        ("bright", [4.5, 1.0, 3.5]),
        ("moderate", [14.0, 1.0, 3.5]),
        ("dim", [43.0, 0.5, 13.0]),
        ("dark-hall", [61.0, 0.5, -8.0]),
        ("home", [62.5, 1.0, 8.0]),
        ("pool", [5.6, 0.0, 8.4]),
        ("stairs", [21.5, 0.0, 3.5]),
        ("home-stairs", [56.6, 1.0, 13.6]),
        ("door-bright", [58.0, 0.5, 13.0]),
        ("door-dim", [52.0, 0.5, 13.0]),
        ("outdoor", [1.4, 1.0, -18.0]),
        ("night-guard", [13.5, 1.0, -95.7]),
    ];
    let mut report = Vec::new();
    for quality in [QualityLevel::Medium, QualityLevel::High] {
        install_compiled_scene(&mut renderer, "places_demo", quality, false);
        for (name, position) in locations {
            report.push(compiled_scene_location(
                &mut renderer,
                &directory,
                quality,
                name,
                position,
            ));
        }
    }
    let movement = compiled_movement_and_lookup_cost(&mut renderer, &directory);
    std::fs::write(
        std::path::Path::new(&directory).join("movement-performance.json"),
        serde_json::to_vec_pretty(&movement).expect("movement JSON"),
    )
    .expect("movement report");
    let real_models = [
        ("bright", [4.5, 1.0, 3.5]),
        ("dim", [43.0, 0.5, 13.0]),
        ("night", [13.5, 1.0, -95.7]),
    ]
    .into_iter()
    .map(|(name, position)| real_models_at_same_anchor(&mut renderer, &directory, name, position))
    .collect::<Vec<_>>();
    std::fs::write(
        std::path::Path::new(&directory).join("real-models.json"),
        serde_json::to_vec_pretty(&real_models).expect("models JSON"),
    )
    .expect("model report");
    let position = [4.5, 1.0, 3.5];
    renderer.dynamic.clear();
    let id = neutral_at(&mut renderer, position, false);
    let direct_high = scene_capture(&mut renderer, position);
    for quality in [
        QualityLevel::Medium,
        QualityLevel::Low,
        QualityLevel::Medium,
        QualityLevel::High,
    ] {
        install_compiled_scene(&mut renderer, "places_demo", quality, true);
        let sample = renderer
            .dynamic
            .get(id)
            .expect("preserved entity")
            .entity_lighting()
            .unwrap();
        assert!(sample.display.iter().all(|value| value.is_finite()));
        if quality != QualityLevel::Low {
            assert!(sample.prepared.is_some());
        }
        let image = scene_capture(&mut renderer, position);
        if quality == QualityLevel::High {
            assert_eq!(image.rgba, direct_high.rgba);
        }
    }
    std::fs::write(
        std::path::Path::new(&directory).join("report.json"),
        serde_json::to_vec_pretty(&report).expect("report"),
    )
    .expect("report file");
}

fn real_models_at_same_anchor(
    renderer: &mut WgpuRenderer,
    directory: &str,
    name: &str,
    position: [f32; 3],
) -> serde_json::Value {
    use crate::render::common::light_transport::entity_lighting;
    renderer.dynamic.clear();
    renderer.upload_dynamic();
    let expected = entity_lighting(
        renderer.dynamic_lighting.as_ref().unwrap(),
        renderer.dynamic_field.as_deref(),
        position,
    );
    let mut models = Vec::new();
    for model in ["rat", "sheet-ghost-cat", "skeleton"] {
        renderer
            .spawn_runtime_character("same-anchor", model, [0.0; 3], 0.0, 1.0)
            .expect("real model");
        let local = renderer
            .characters
            .runtime_character("same-anchor")
            .unwrap()
            .lighting_sample_position();
        let offset = (Vec3::from_array(position) - Vec3::from_array(local)).to_array();
        assert!(renderer.set_runtime_character_transform("same-anchor", offset, 0.0));
        drop(renderer.update_characters(0.0, LocomotionSnapshot::default(), &[]));
        let actor = renderer
            .characters
            .runtime_character("same-anchor")
            .unwrap();
        let actual_position = actor.lighting_sample_position();
        assert!((Vec3::from_array(actual_position) - Vec3::from_array(position)).length() < 1.0e-5);
        let actual = actor.entity_lighting();
        assert_eq!(actual.source, expected.source);
        for (value, reference) in actual.display.into_iter().zip(expected.display) {
            assert!(
                (value - reference).abs() < 1.0e-5,
                "real model anchors must agree"
            );
        }
        let gpu = renderer
            .world_characters
            .as_ref()
            .unwrap()
            .diagnostic_uniform("same-anchor", None)
            .unwrap();
        let energy = gpu.entity_irradiance;
        let moment = gpu.entity_moment;
        if let Some(texel) = actual.prepared {
            assert_eq!(energy[..3], texel.irradiance);
            assert_eq!(moment[..3], texel.direction);
        }
        let image = scene_capture(renderer, position);
        std::fs::write(
            std::path::Path::new(directory).join(format!("model-{name}-{model}.png")),
            crate::materials::encode_png(&image).expect("PNG"),
        )
        .expect("model capture");
        models.push(serde_json::json!({"model": model, "anchor":actual_position,
            "energy": energy, "moment":moment, "display":actual.display}));
        assert!(renderer.despawn_runtime_character("same-anchor"));
        drop(renderer.update_characters(0.0, LocomotionSnapshot::default(), &[]));
    }
    serde_json::json!({"location":name, "models":models})
}

fn compiled_movement_and_lookup_cost(
    renderer: &mut WgpuRenderer,
    directory: &str,
) -> serde_json::Value {
    renderer.dynamic.clear();
    let id = neutral_at(renderer, [4.5, 1.0, 3.6], false);
    let local = Vec3::from_array(renderer.dynamic.get(id).unwrap().mesh().centre) * 0.4;
    let paths = [
        (
            "office-door-moderate",
            Vec3::new(4.5, 1.0, 3.6),
            Vec3::new(17.0, 1.0, 3.6),
        ),
        (
            "vertical-stairs",
            Vec3::new(21.0, 0.0, 3.0),
            Vec3::new(21.0, 1.4, 3.0),
        ),
        (
            "night",
            Vec3::new(13.5, 1.0, -95.7),
            Vec3::new(13.5, 1.0, -93.7),
        ),
    ];
    serde_json::json!(
        paths
            .into_iter()
            .map(|(name, from, to)| compiled_movement_path(
                renderer, id, local, directory, name, from, to
            ))
            .collect::<Vec<_>>()
    )
}
fn compiled_movement_path(
    renderer: &mut WgpuRenderer,
    id: DynamicId,
    local: Vec3,
    directory: &str,
    name: &str,
    from: Vec3,
    to: Vec3,
) -> serde_json::Value {
    let positions = (0..=500_u16)
        .map(|step| from.lerp(to, f32::from(step) / 500.0))
        .collect::<Vec<_>>();
    let mut samples: Vec<(crate::render::EntityLightingSource, [f32; 3])> = Vec::new();
    let mut maximum_step = 0.0_f32;
    for (index, position) in positions.iter().enumerate() {
        assert!(
            renderer
                .dynamic
                .set_transform(id, (*position - local).to_array(), 0.0, 0.4)
        );
        let _update_stats = renderer.update_dynamic(0.0);
        let sample = renderer.dynamic.get(id).unwrap().entity_lighting().unwrap();
        assert_eq!(
            sample.source,
            crate::render::EntityLightingSource::Prepared,
            "valid path {name}"
        );
        assert!(sample.display.iter().all(|value| value.is_finite()));
        if let Some(previous) = samples.last() {
            let previous_source: &crate::render::EntityLightingSource = &previous.0;
            assert_eq!(sample.source, *previous_source);
        }
        if let Some((_, previous)) = samples.last() {
            for (value, old) in sample.display.into_iter().zip(*previous) {
                maximum_step = maximum_step.max((value - old).abs());
            }
        }
        let _update_stats_2 = renderer.update_dynamic(0.0);
        assert_eq!(
            renderer.dynamic.get(id).unwrap().entity_lighting(),
            Some(sample),
            "stationary stability"
        );
        let gpu = renderer
            .world_dynamic
            .as_ref()
            .unwrap()
            .diagnostic_uniform(0)
            .unwrap();
        assert_eq!(
            gpu.entity_irradiance[..3],
            sample.prepared.unwrap().irradiance
        );
        if index % 50 == 0 {
            let image = scene_capture(renderer, position.to_array());
            std::fs::write(
                std::path::Path::new(directory).join(format!("move-{name}-{index:03}.png")),
                crate::materials::encode_png(&image).expect("movement PNG"),
            )
            .expect("movement capture");
        }
        samples.push((sample.source, sample.display));
    }
    for (position, expected) in positions.iter().zip(&samples).rev() {
        assert!(
            renderer
                .dynamic
                .set_transform(id, (*position - local).to_array(), 0.0, 0.4)
        );
        let _update_stats_3 = renderer.update_dynamic(0.0);
        let sample = renderer.dynamic.get(id).unwrap().entity_lighting().unwrap();
        assert_eq!(
            (sample.source, sample.display),
            *expected,
            "reverse path stability"
        );
    }
    serde_json::json!({"path":name,"samples":positions.len(),
        "maximum_display_step":maximum_step,
        "lookup_us":probe_lookup_microseconds(renderer, &positions)})
}
fn probe_lookup_microseconds(renderer: &WgpuRenderer, positions: &[Vec3]) -> f64 {
    use crate::render::common::light_transport::entity_lighting;
    let started = std::time::Instant::now();
    let mut count = 0_u32;
    for _ in 0_i32..64_i32 {
        for position in positions {
            let _sample = std::hint::black_box(entity_lighting(
                renderer.dynamic_lighting.as_ref().unwrap(),
                renderer.dynamic_field.as_deref(),
                position.to_array(),
            ));
            count += 1;
        }
    }
    started.elapsed().as_secs_f64() * 1_000_000.0 / f64::from(count)
}
