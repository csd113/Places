//! Deterministic entity-lighting checks through actual renderer resources.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::unwrap_used
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
    for _ in 0..100 {
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
                    pos,
                    color: [1.0; 4],
                    uv: [0.0; 2],
                })
                .to_vec(),
            indices: vec![0, 1, 2],
            submeshes: vec![PropSubmesh {
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
    renderer.update_characters(0.0, LocomotionSnapshot::default(), &[]);
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
    renderer.update_dynamic(0.0);
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
        let image = capture(&mut renderer);
        if quality == QualityLevel::High {
            assert_eq!(image.rgba, baseline, "restored render matches direct High");
        }
    }
    for _ in 0..3 {
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
    renderer
        .dynamic
        .spawn(&asset, [0.0; 3], 0.0, 1.0, 0.0)
        .expect("triangle");
    renderer.update_dynamic(0.0);
    renderer.upload_dynamic();
    let bright = capture(&mut renderer);
    // The geometric normal is (0,1,1.4) normalized. Its +Y moment gives
    // ~0.216 diffuse; using the default +Z normal would incorrectly give 0.1.
    let texel = renderer.dynamic_field.as_ref().unwrap().probes[0].texel();
    let normal = Vec3::new(0.0, 1.0, 1.4).normalize().to_array();
    let expected = texel.light_at(normal)[0];
    assert!(expected > 0.21 && expected < 0.22);
    let mut field = renderer.dynamic_field.as_ref().unwrap().as_ref().clone();
    field.probes[0].direction = [0.0, -0.3, 0.0];
    renderer.dynamic_field = Some(Arc::new(field));
    renderer.update_dynamic(0.0);
    let dark = capture(&mut renderer);
    if let Ok(directory) = std::env::var("PLACES_ENTITY_TEST_CAPTURES") {
        let directory = std::path::Path::new(&directory);
        std::fs::create_dir_all(directory).expect("capture dir");
        std::fs::write(
            directory.join("entity-facing-light.png"),
            crate::materials::encode_png(&bright).expect("encode bright"),
        )
        .expect("bright PNG");
        std::fs::write(
            directory.join("entity-away-from-light.png"),
            crate::materials::encode_png(&dark).expect("encode dark"),
        )
        .expect("dark PNG");
    }
    let centre = (64 * 128 + 64) * 4;
    assert!(
        (f32::from(bright.rgba[centre]) - expected * 255.0).abs() < 2.0,
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
    archive
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
    let mut source_changes = [0; 3];
    for axis in 0..3 {
        let mut previous: Option<crate::render::common::light_transport::EntityLighting> = None;
        for step in 0..=100_u16 {
            let mut centre = position;
            centre[axis] += f32::from(step).mul_add(0.01, -0.5);
            let offset = std::array::from_fn(|i| centre[i] - local[i]);
            assert!(renderer.dynamic.set_transform(id, offset, 0.0, 1.0));
            renderer.update_dynamic(0.0);
            let sample = renderer.dynamic.get(id).unwrap().entity_lighting().unwrap();
            assert!(sample.display.iter().all(|v| v.is_finite()));
            if let Some(last) = previous {
                if last.source != sample.source {
                    source_changes[axis] += 1;
                }
                let jump = sample
                    .display
                    .iter()
                    .zip(last.display)
                    .map(|(a, b)| (a - b).abs())
                    .fold(0.0_f32, f32::max);
                jumps[axis] = jumps[axis].max(jump);
            }
            renderer.update_dynamic(0.0);
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
    renderer.update_dynamic(0.0);
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
    renderer
        .dynamic
        .spawn(&asset, offset, 0.0, 1.0, 0.0)
        .expect("neutral entity");
    renderer.update_dynamic(0.0);
    renderer.upload_dynamic();
    let lighting = renderer.dynamic_lighting.as_ref().unwrap();
    let field = renderer.dynamic_field.as_ref().unwrap();
    let room = lighting
        .room_index_at_height(position[0], position[1], position[2])
        .filter(|r| lighting.probe_region_at(*r, position).is_some());
    let candidates = field.sample_diagnostics_filtered(position, room, |p| {
        room.is_some_and(|r| lighting.same_probe_region(r, position, p))
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
            ("doorway", [9.0, 1.0, 3.0]),
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
