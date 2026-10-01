//! End-to-end contracts for real static model surface lighting.
//!
//! The fixtures use shipped stump, rock and furniture GLBs and the normal
//! compiler geometry entry point. They inspect the same atlas UVs, HDR texels,
//! normals and package records the runtime shader receives, rather than a
//! separate approximation of the lighting equation.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::expect_used,
    clippy::float_cmp,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

use glam::Vec3;

use crate::level::LevelDef;
use crate::lighting::lightmap::{LevelLightmaps, LightmapMode, LightmapTexel};
use crate::loader::PropCatalog;
use crate::quality::QualityLevel;
use crate::render::{LightmapBuildOptions, PropMeshBatch, Vertex};

const STUMP: &str = "outdoor:showcase_stump_seat";
const ROCK: &str = "outdoor:showcase_boulder";

fn scene(model: &str, blocker: &str, brightness: f32) -> LevelDef {
    LevelDef::from_json(&format!(
        r#"{{"format_version":3,"id":"static_model_receiver","name":"Static model receiver",
            "spawn":{{"x":6,"z":5}},
            "rooms":[{{"x":0,"z":0,"width":7,"depth":6,"height":3}}],
            "walls":[{blocker}],
            "ceiling_lights":[{{"fixture":"core:fluorescent_panel_01","x":1.8,"z":3,
                "align":"none","brightness":{brightness},"range":8}}],
            "props":[{{"id":"receiver","model":"{model}","x":4,"z":3,"solid":true}}]
        }}"#
    ))
    .expect("synthetic receiver room parses")
}

fn build(level: &LevelDef, quality: QualityLevel, mode: LightmapMode) -> crate::render::LevelBuild {
    let root = crate::assets::resolve_asset_root().expect("shipped assets are available");
    let catalog = PropCatalog::load_from_path(&root.join("catalog.json"))
        .expect("the shipped catalog parses");
    let materials = crate::render::logical_materials(level);
    let mut assets = crate::props::PropAssets::with_root(root);
    let mut options = LightmapBuildOptions::for_level(quality, mode);
    // Direct illumination is sufficient to establish shadow receiving and
    // sidedness. Bounce gathers have their own solver tests and would blur
    // these controlled comparisons as well as increase their cost.
    options.solve.bounces = 0;
    options.solve.workers = 2;
    let result = crate::render::build_level_geometry_timed_with_lightmaps(
        level,
        &catalog,
        &mut assets,
        &materials,
        options,
        None,
    );
    assert!(
        result.lightmap_failure.is_none(),
        "{:?}",
        result.lightmap_failure
    );
    assert!(
        !result.batches.is_empty(),
        "a real GLB must draw, not a fallback box"
    );
    result
}

fn receiver_batch<'a>(build: &'a crate::render::LevelBuild, suffix: &str) -> &'a PropMeshBatch {
    build
        .batches
        .iter()
        .find(|batch| batch.model.ends_with(suffix))
        .expect("the requested model is in a real static batch")
}

/// Bilinear sampling of the stored moment planes, followed by the same
/// directional reconstruction the fragment shader applies. UV interpolation
/// precedes normal reconstruction; colours are deliberately absent here.
fn sample(atlas: &LevelLightmaps, vertices: [Vertex; 3]) -> [f32; 3] {
    let page = vertices[0].lightmap_page;
    assert!(vertices.iter().all(Vertex::is_lightmapped));
    assert!(vertices.iter().all(|vertex| vertex.lightmap_page == page));
    let image = &atlas.pages[usize::from(page)];
    let uv = [0, 1].map(|axis| {
        vertices
            .iter()
            .map(|vertex| f32::from(vertex.lightmap[axis]) / 65_535.0)
            .sum::<f32>()
            / 3.0
    });
    let x = uv[0].mul_add(image.width as f32, -0.5).max(0.0);
    let y = uv[1].mul_add(image.height as f32, -0.5).max(0.0);
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let dx = x.fract();
    let dy = y.fract();
    let mut texel = LightmapTexel::ZERO;
    for (ox, oy, weight) in [
        (0, 0, (1.0 - dx) * (1.0 - dy)),
        (1, 0, dx * (1.0 - dy)),
        (0, 1, (1.0 - dx) * dy),
        (1, 1, dx * dy),
    ] {
        let contribution = image
            .texel(
                (x0 + ox).min(image.width - 1),
                (y0 + oy).min(image.height - 1),
            )
            .expect("UV samples the atlas page");
        for channel in 0..3 {
            texel.irradiance[channel] =
                contribution.irradiance[channel].mul_add(weight, texel.irradiance[channel]);
            texel.direction[channel] =
                contribution.direction[channel].mul_add(weight, texel.direction[channel]);
        }
    }
    let normal = vertices
        .iter()
        .map(|vertex| Vec3::from_array(vertex.normal))
        .sum::<Vec3>()
        .normalize();
    texel.light_at(normal.to_array())
}

fn samples(build: &crate::render::LevelBuild, suffix: &str, normal: Vec3) -> Vec<f32> {
    let atlas = build
        .lightmaps
        .as_ref()
        .expect("Medium/High must have a real atlas");
    let batch = receiver_batch(build, suffix);
    batch
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|indices| {
            let vertices = indices.map_indices(&batch.vertices);
            let facing = vertices
                .iter()
                .map(|vertex| Vec3::from_array(vertex.normal))
                .sum::<Vec3>()
                .normalize();
            (facing.dot(normal) > 0.65).then(|| sample(atlas, vertices).iter().sum::<f32>() / 3.0)
        })
        .collect()
}

trait TriangleIndices {
    fn map_indices(&self, vertices: &[Vertex]) -> [Vertex; 3];
}

impl TriangleIndices for [u16; 3] {
    fn map_indices(&self, vertices: &[Vertex]) -> [Vertex; 3] {
        self.map(|index| vertices[usize::from(index)])
    }
}

fn mean(values: &[f32]) -> f32 {
    assert!(
        !values.is_empty(),
        "the receiver must have samples in the requested orientation"
    );
    values.iter().sum::<f32>() / values.len() as f32
}

#[test]
fn static_stumps_and_pond_rocks_receive_shadows_from_architecture() {
    for (model, suffix) in [
        (STUMP, "showcase_stump_seat.glb"),
        (ROCK, "showcase_boulder.glb"),
    ] {
        let lit = build(
            &scene(model, "", 2.0),
            QualityLevel::Medium,
            LightmapMode::On,
        );
        let shadow = build(
            &scene(
                model,
                r#"{"x":2.9,"z":1.9,"width":0.25,"depth":2.2,"height":2.9}"#,
                2.0,
            ),
            QualityLevel::Medium,
            LightmapMode::On,
        );
        let lit_top = mean(&samples(&lit, suffix, Vec3::Y));
        let shadow_top = mean(&samples(&shadow, suffix, Vec3::Y));
        assert!(
            lit_top > 0.02,
            "unblocked {model} must receive actual fixture light: {lit_top}"
        );
        assert!(
            shadow_top < lit_top * 0.7,
            "{model} must receive the wall's cast shadow: lit={lit_top}, shadow={shadow_top}"
        );
    }
}

#[test]
fn static_stump_faces_receive_directional_light_and_self_occlude() {
    let result = build(&scene(STUMP, "", 2.0), QualityLevel::High, LightmapMode::On);
    let lit_side = mean(&samples(&result, "showcase_stump_seat.glb", -Vec3::X));
    let away_side = mean(&samples(&result, "showcase_stump_seat.glb", Vec3::X));
    assert!(
        lit_side > away_side * 1.5 + 0.01,
        "the lamp-facing bark must differ physically from the self-occluded back: {lit_side} vs {away_side}"
    );
}

#[test]
fn static_prop_albedo_stays_neutral_while_hdr_illumination_changes() {
    let bright = build(
        &scene(STUMP, "", 8.0),
        QualityLevel::Medium,
        LightmapMode::On,
    );
    let dim = build(
        &scene(STUMP, "", 0.05),
        QualityLevel::Medium,
        LightmapMode::On,
    );
    let bright_batch = receiver_batch(&bright, "showcase_stump_seat.glb");
    let dim_batch = receiver_batch(&dim, "showcase_stump_seat.glb");
    assert_eq!(bright_batch.indices, dim_batch.indices);
    assert_eq!(
        bright_batch
            .vertices
            .iter()
            .map(|vertex| vertex.color)
            .collect::<Vec<_>>(),
        dim_batch
            .vertices
            .iter()
            .map(|vertex| vertex.color)
            .collect::<Vec<_>>(),
        "fixture brightness belongs in HDR lightmaps, never baked into model albedo"
    );
    assert!(
        mean(&samples(&bright, "showcase_stump_seat.glb", Vec3::Y))
            > mean(&samples(&dim, "showcase_stump_seat.glb", Vec3::Y)) * 2.0
    );
    assert!(
        bright
            .lightmaps
            .as_ref()
            .unwrap()
            .pages
            .iter()
            .flat_map(|page| &page.texels)
            .flat_map(|texel| texel.irradiance)
            .any(|channel| channel > 1.0),
        "HDR illumination must remain unclamped"
    );
}

#[test]
fn model_lightmap_coordinates_normals_and_shading_survive_package_roundtrip() {
    let built = build(
        &scene(ROCK, "", 2.0),
        QualityLevel::Medium,
        LightmapMode::On,
    );
    let atlas = built.lightmaps.as_ref().unwrap();
    let encoded = crate::package::props::write_props(&built.batches).unwrap();
    let decoded = crate::package::props::read_props(&encoded).unwrap();
    for (before, after) in built.batches.iter().zip(&decoded) {
        assert_eq!(
            before.vertices, after.vertices,
            "normals, albedo and atlas coordinates are exact"
        );
        assert_eq!(before.indices, after.indices);
        assert_eq!(before.submeshes, after.submeshes);
    }
    let (meta, pixels) = crate::package::lightmaps::write_lightmaps(atlas).unwrap();
    let decoded_atlas = crate::package::lightmaps::read_lightmaps(&meta, &pixels).unwrap();
    assert_eq!(atlas.charts, decoded_atlas.charts);
    let batch = receiver_batch(&built, "showcase_boulder.glb");
    for indices in batch.indices.as_chunks::<3>().0 {
        let triangle = indices.map_indices(&batch.vertices);
        for (before, after) in sample(atlas, triangle)
            .into_iter()
            .zip(sample(&decoded_atlas, triangle))
        {
            assert!(
                (before - after).abs() <= before.abs().mul_add(0.002, 0.0001),
                "half-float serialization must preserve shader illumination: {before} vs {after}"
            );
        }
    }
}

#[test]
fn rotated_scaled_static_models_have_valid_normals_and_independent_atlas_seams() {
    let mut level = scene(STUMP, "", 2.0);
    level.props[0].scale = 1.7;
    level.props[0].rotation_degrees = 90.0;
    let built = build(&level, QualityLevel::High, LightmapMode::On);
    let batch = receiver_batch(&built, "showcase_stump_seat.glb");
    for vertex in &batch.vertices {
        let normal = Vec3::from_array(vertex.normal);
        assert!(normal.is_finite() && (normal.length() - 1.0).abs() < 0.001);
        assert!(
            vertex.is_lightmapped(),
            "every visible static stump surface needs atlas coordinates"
        );
        assert!(usize::from(vertex.lightmap_page) < built.lightmaps.as_ref().unwrap().pages.len());
    }
    let mut distinct_seam = false;
    for (index, a) in batch.vertices.iter().enumerate() {
        for b in &batch.vertices[index + 1..] {
            if Vec3::from_array(a.pos).distance(Vec3::from_array(b.pos)) < 0.0001
                && Vec3::from_array(a.normal).dot(Vec3::from_array(b.normal)) < 0.6
            {
                distinct_seam |= a.lightmap_page != b.lightmap_page || a.lightmap != b.lightmap;
            }
        }
    }
    assert!(
        distinct_seam,
        "hard edges must split their atlas coordinates, rather than leak between faces"
    );
    assert!(
        mean(&samples(&built, "showcase_stump_seat.glb", -Vec3::X))
            > mean(&samples(&built, "showcase_stump_seat.glb", Vec3::X)) + 0.01,
        "world-space normal orientation must remain correct after scale and yaw"
    );
}

#[test]
fn legacy_low_and_repeated_medium_high_builds_keep_their_distinct_lighting_contracts() {
    let level = scene("core:crate", "", 2.0);
    let medium = build(&level, QualityLevel::Medium, LightmapMode::On);
    let low = build(&level, QualityLevel::Low, LightmapMode::Off);
    assert!(low.lightmaps.is_none());
    assert!(
        low.batches
            .iter()
            .flat_map(|batch| &batch.vertices)
            .all(|vertex| !vertex.is_lightmapped())
    );
    for quality in [QualityLevel::High, QualityLevel::Medium, QualityLevel::High] {
        let restored = build(&level, quality, LightmapMode::On);
        assert!(
            restored
                .batches
                .iter()
                .flat_map(|batch| &batch.vertices)
                .all(Vertex::is_lightmapped)
        );
        if quality == QualityLevel::Medium {
            assert_eq!(
                medium.lightmaps.as_ref().unwrap().pages,
                restored.lightmaps.as_ref().unwrap().pages
            );
            assert_eq!(medium.batches[0].vertices, restored.batches[0].vertices);
        }
    }
}

#[test]
fn animated_sheet_ghosts_keep_probe_lighting_and_do_not_receive_static_atlas_coordinates() {
    let mut level = scene(STUMP, "", 2.0);
    let ghost: crate::level::PropDef = serde_json::from_value(serde_json::json!({
        "id":"animated_ghost", "model":"sheet-ghost-cat", "x":5.5, "z":3.0
    }))
    .unwrap();
    level.props.push(ghost);
    let root = crate::assets::resolve_asset_root().unwrap();
    let catalog = PropCatalog::load_from_path(&root.join("catalog.json")).unwrap();
    let path = catalog.get("sheet-ghost-cat").model.unwrap();
    let mut assets = crate::props::PropAssets::with_root(root);
    let asset = assets.resolve(&path).unwrap();
    assert!(
        asset.model.skin.is_some() && !asset.model.animations.is_empty(),
        "the fixture must be an actual animated skinned ghost"
    );
    // Air-region labels belong to the compiler, not the intermediate inline
    // renderer build. Exercise the packaged production records so this test
    // checks the real dynamic-model install path without inventing labels.
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/static-prop-fixtures")
        .join(format!("animated-ghost-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let source = scratch.join("receiver.json");
    let out = scratch.join("receiver.placesmap");
    std::fs::write(&source, serde_json::to_vec(&level).unwrap()).unwrap();
    let request = crate::compiler::BuildRequest {
        source,
        out: out.clone(),
        asset_root: assets.root().unwrap().to_path_buf(),
        variants: vec![crate::quality::LightmapQuality::Medium],
        workers: 2,
        force: true,
        capture_probes: false,
    };
    let report = crate::compiler::build(&request).unwrap();
    assert!(
        report.rebuilt && report.warnings.is_empty(),
        "{:?}",
        report.warnings
    );
    let opened = crate::package::world::open(&out).unwrap();
    let compiled = crate::package::world::load_variant(
        &out,
        &opened.manifest,
        crate::quality::LightmapQuality::Medium,
        &mut assets,
    )
    .unwrap();
    let dynamic = crate::render::entity_lighting(
        &compiled.lighting,
        compiled.irradiance.as_deref(),
        [5.5, 1.6, 4.0],
    );
    assert_eq!(
        dynamic.source,
        crate::render::EntityLightingSource::Prepared
    );
    assert!(
        dynamic.prepared.is_some(),
        "animated models retain the prepared irradiance field"
    );
    for batch in compiled.props.iter().filter(|batch| batch.model == path) {
        assert!(
            batch.vertices.iter().all(|vertex| !vertex.is_lightmapped()),
            "an animated ghost must not freeze its bind pose into static lightmap charts"
        );
    }
}

#[test]
fn nearby_real_prop_geometry_casts_a_shadow_onto_a_static_stump() {
    let open = scene(STUMP, "", 2.0);
    let mut occluded = open.clone();
    occluded.props.push(
        serde_json::from_value(serde_json::json!({
            "id":"occluding_fridge", "model":"core:fridge", "x":2.9, "z":3.0,
            "scale":1.2,"solid":true
        }))
        .unwrap(),
    );
    let open_build = build(&open, QualityLevel::Medium, LightmapMode::On);
    let occluded_build = build(&occluded, QualityLevel::Medium, LightmapMode::On);
    let open_top = mean(&samples(&open_build, "showcase_stump_seat.glb", Vec3::Y));
    let occluded_top = mean(&samples(
        &occluded_build,
        "showcase_stump_seat.glb",
        Vec3::Y,
    ));
    assert!(
        occluded_top < open_top * 0.7,
        "actual fridge triangles must cast onto the stump's surface: open={open_top}, occluded={occluded_top}"
    );
}

#[test]
fn existing_house_furniture_receives_surface_shadows_with_neutral_albedo() {
    let lit_level = scene("core:cabinet", "", 2.0);
    let mut shadow_level = lit_level.clone();
    shadow_level.walls = scene(
        "core:cabinet",
        r#"{"x":2.9,"z":1.9,"width":0.25,"depth":2.2,"height":2.9}"#,
        2.0,
    )
    .walls;
    let lit = build(&lit_level, QualityLevel::Medium, LightmapMode::On);
    let shadow = build(&shadow_level, QualityLevel::Medium, LightmapMode::On);
    let lit_top = mean(&samples(&lit, "cabinet.glb", Vec3::Y));
    let shadow_top = mean(&samples(&shadow, "cabinet.glb", Vec3::Y));
    assert!(
        shadow_top < lit_top * 0.7,
        "the shared model pipeline must shadow furniture too: lit={lit_top}, shadow={shadow_top}"
    );
    assert_eq!(
        receiver_batch(&lit, "cabinet.glb")
            .vertices
            .iter()
            .map(|v| v.color)
            .collect::<Vec<_>>(),
        receiver_batch(&shadow, "cabinet.glb")
            .vertices
            .iter()
            .map(|v| v.color)
            .collect::<Vec<_>>(),
        "cast shadows must never be painted into the cabinet's material colours"
    );
}

/// A triangle with a deliberately slanted valid smooth normal, transformed
/// non-uniformly by its GLB node. This exercises authored shading data independently
/// from the showcase GLBs, most of which legitimately omit NORMAL.
fn authored_normal_triangle() -> Vec<u8> {
    let mut binary = Vec::new();
    for position in [[0.0_f32, 0.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]] {
        for channel in position {
            binary.extend_from_slice(&channel.to_le_bytes());
        }
    }
    for index in [0_u16, 1, 2] {
        binary.extend_from_slice(&index.to_le_bytes());
    }
    binary.extend_from_slice(&[0, 0]);
    let n = std::f32::consts::FRAC_1_SQRT_2;
    for _ in 0..3 {
        for channel in [n, n, 0.0] {
            binary.extend_from_slice(&channel.to_le_bytes());
        }
    }
    for uv in [[0.0_f32, 0.0], [0.0, 1.0], [1.0, 0.0]] {
        for channel in uv {
            binary.extend_from_slice(&channel.to_le_bytes());
        }
    }
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,
        "scenes":[{{"nodes":[0]}}],
        "nodes":[{{"mesh":0,"scale":[2,1,0.5],"rotation":[0,{n},0,{n}]}}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":2,"TEXCOORD_0":3}},"indices":1}}]}}],
        "buffers":[{{"byteLength":104}}],
        "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},
            {{"buffer":0,"byteOffset":36,"byteLength":6}},
            {{"buffer":0,"byteOffset":44,"byteLength":36}},
            {{"buffer":0,"byteOffset":80,"byteLength":24}}],
        "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},
            {{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}},
            {{"bufferView":2,"componentType":5126,"count":3,"type":"VEC3"}},
            {{"bufferView":3,"componentType":5126,"count":3,"type":"VEC2"}}]}}"#
    );
    crate::test_support::glb_container(&json, &binary)
}

#[test]
fn valid_authored_model_normals_survive_nonuniform_node_transforms() {
    let model = crate::gltf::parse_glb(&authored_normal_triangle()).unwrap();
    let expected = Vec3::new(0.0, 1.0, -0.5).normalize();
    assert_eq!(model.vertices.len(), 3);
    for vertex in &model.vertices {
        let normal = Vec3::from_array(vertex.normal.expect("valid NORMAL is retained"));
        assert!(
            normal.distance(expected) < 0.0001,
            "authored normals need inverse-transpose and normalization, not position transform: {normal:?} vs {expected:?}"
        );
    }
}

/// Explicit developer preflight: build each bundled map's real geometry,
/// charts and transport scene, but never run the expensive atlas fill. This
/// diagnostic is ignored in the normal suite because bundled source content
/// changes during authoring; invoke it after generators settle before rebakes.
#[test]
#[ignore = "explicit bundled-map atlas planning preflight; no lightmap fill"]
#[allow(clippy::print_stderr)] // Explicit developer diagnostic, outside normal tests and runtime.
fn bundled_static_models_fit_medium_and_full_atlas_plans() {
    let root = crate::assets::resolve_asset_root().unwrap();
    let catalog = PropCatalog::load_from_path(&root.join("catalog.json")).unwrap();
    for id in ["lantern_hollow", "model_zoo", "places_demo"] {
        let json = std::fs::read_to_string(root.join("levels").join(format!("{id}.json"))).unwrap();
        let mut level = LevelDef::from_json(&json).unwrap();
        crate::loader::prepare_level(&mut level, catalog.assets(), None);
        let materials = crate::render::logical_materials(&level);
        let mut assets = crate::props::PropAssets::with_root(&root);
        for quality in [
            crate::quality::LightmapQuality::Medium,
            crate::quality::LightmapQuality::Full,
        ] {
            let options = LightmapBuildOptions::for_lightmaps(quality);
            let started = std::time::Instant::now();
            let prepared = crate::render::prepare_level_geometry_with_lightmaps(
                &level,
                &catalog,
                &mut assets,
                &materials,
                options,
                None,
            );
            let (pages, charts) = prepared
                .fill
                .as_ref()
                .map_or((0, 0), |request| (request.page_count, request.charts.len()));
            eprintln!(
                "ATLAS_PREFLIGHT map={id} quality={} pages={pages} charts={charts} planning_seconds={:.3} failure={:?}",
                quality.name(),
                started.elapsed().as_secs_f64(),
                prepared.build.lightmap_failure,
            );
            assert!(
                prepared.build.lightmap_failure.is_none(),
                "{id} {quality:?} must fit its real static-model chart budget: {:?}",
                prepared.build.lightmap_failure
            );
            let request = prepared
                .fill
                .expect("an uncached preflight plans the atlas fill");
            assert!(!request.charts.is_empty() && request.page_count > 0);
            assert!(request.page_count <= request.config.max_pages);
            assert!(
                prepared
                    .build
                    .batches
                    .iter()
                    .flat_map(|batch| &batch.vertices)
                    .any(Vertex::is_lightmapped),
                "{id} must plan real static model receivers"
            );
        }
    }
}

#[test]
fn thousands_of_model_charts_pack_deterministically_without_gutter_overlap_or_extra_page_budgets() {
    use crate::lighting::lightmap::{ChartAllocator, LightmapConfig, LightmapPatch, PatchKind};

    let config = LightmapConfig {
        texels_per_metre: 8.0,
        page_edge: 512,
        max_pages: 8,
        padding: 2,
        bytes_per_texel: 16,
    };
    let patch = |kind, width, height| LightmapPatch {
        origin: [0.0; 3],
        u_axis: [width, 0.0, 0.0],
        v_axis: [0.0, 0.0, height],
        room: None,
        kind,
    };
    let mut inputs = Vec::new();
    for index in 0_u32..4_000 {
        if index.is_multiple_of(113) {
            inputs.push(patch(PatchKind::Floor, 4.0, 3.0));
        }
        // Varied triangle charts encounter real packing fragmentation, rather
        // than proving only that equal rectangles tile an empty page.
        let width = (2 + index * 7 % 11) as f32 / 8.0;
        let height = (2 + index * 5 % 13) as f32 / 8.0;
        inputs.push(patch(PatchKind::Prop, width, height));
    }
    let allocate = || {
        let mut allocator = ChartAllocator::new(config);
        let charts = inputs
            .iter()
            .map(|input| {
                allocator
                    .allocate(input)
                    .expect("thousands of small model charts must fit the shared budget")
            })
            .collect::<Vec<_>>();
        assert!(!allocator.failed());
        assert!(allocator.page_count() <= config.max_pages);
        (charts, allocator.page_count())
    };
    let (charts, pages) = allocate();
    let (repeated_charts, repeated_pages) = allocate();
    assert_eq!(
        charts, repeated_charts,
        "chart coordinates must be deterministic"
    );
    assert_eq!(
        pages, repeated_pages,
        "model-page assignment must be deterministic"
    );
    let architecture = inputs
        .iter()
        .zip(&charts)
        .filter_map(|(input, chart)| (input.kind != PatchKind::Prop).then_some(chart.page))
        .collect::<std::collections::BTreeSet<_>>();
    let models = inputs
        .iter()
        .zip(&charts)
        .filter_map(|(input, chart)| (input.kind == PatchKind::Prop).then_some(chart.page))
        .collect::<std::collections::BTreeSet<_>>();
    assert!(!architecture.is_empty() && !models.is_empty());
    assert!(
        architecture.is_disjoint(&models),
        "interleaved architecture and model allocation must use disjoint shared-budget pages"
    );

    let placements = inputs
        .iter()
        .copied()
        .zip(charts.iter().copied())
        .collect::<Vec<_>>();
    assert_chart_gutters_disjoint(&config, pages, &placements);

    // Architecture and model pools share one cap: one full-page chart from
    // each family exhausts a two-page budget, and failure stays sticky.
    let mut capped = ChartAllocator::new(LightmapConfig {
        page_edge: 32,
        max_pages: 2,
        ..config
    });
    let floor = capped.allocate(&patch(PatchKind::Floor, 4.0, 4.0)).unwrap();
    let model = capped.allocate(&patch(PatchKind::Prop, 4.0, 4.0)).unwrap();
    assert_ne!(floor.page, model.page);
    assert_eq!(capped.page_count(), 2);
    assert!(capped.allocate(&patch(PatchKind::Prop, 4.0, 4.0)).is_none());
    assert!(capped.failed());
    assert!(
        capped
            .allocate(&patch(PatchKind::Floor, 0.125, 0.125))
            .is_none()
    );
    assert_eq!(
        capped.page_count(),
        2,
        "model pages must never bypass the shared page cap"
    );
}

fn assert_chart_gutters_disjoint(
    config: &crate::lighting::lightmap::LightmapConfig,
    pages: usize,
    charts: &[(
        crate::lighting::lightmap::LightmapPatch,
        crate::lighting::lightmap::Chart,
    )],
) {
    let edge = usize::try_from(config.page_edge).unwrap();
    let mut occupancy = vec![vec![false; edge * edge]; pages];
    for (patch, chart) in charts {
        let padding = config.padding_for(patch.kind);
        assert!(chart.x >= padding && chart.y >= padding);
        let left = chart.x - padding;
        let top = chart.y - padding;
        let right = chart.x + chart.width + padding;
        let bottom = chart.y + chart.height + padding;
        assert!(right <= config.page_edge && bottom <= config.page_edge);
        let page = &mut occupancy[usize::from(chart.page)];
        for y in top..bottom {
            for x in left..right {
                let slot = usize::try_from(y).unwrap() * edge + usize::try_from(x).unwrap();
                assert!(
                    !page[slot],
                    "chart or gutter overlap at page={}, x={x}, y={y}",
                    chart.page
                );
                page[slot] = true;
            }
        }
    }
}

#[test]
fn adjacent_model_charts_assemble_with_their_own_single_texel_gutters_and_preserve_neighbor_colors()
{
    use crate::lighting::lightmap::{
        ChartAllocator, LightmapAtlas, LightmapConfig, LightmapPatch, PatchKind,
    };
    let config = LightmapConfig {
        texels_per_metre: 8.0,
        page_edge: 16,
        max_pages: 2,
        padding: 2,
        bytes_per_texel: 16,
    };
    assert_eq!(config.padding_for(PatchKind::Prop), 1);
    assert_eq!(config.padding_for(PatchKind::Floor), 2);
    let inputs = [
        (PatchKind::Floor, 0.25, 0.25),
        (PatchKind::Prop, 0.25, 0.375),
        (PatchKind::Prop, 0.375, 0.25),
        (PatchKind::Prop, 0.25, 0.25),
    ];
    let mut allocator = ChartAllocator::new(config);
    let charts = inputs
        .into_iter()
        .map(|(kind, width, height)| {
            let patch = LightmapPatch {
                origin: [0.0; 3],
                u_axis: [width, 0.0, 0.0],
                v_axis: [0.0, 0.0, height],
                room: None,
                kind,
            };
            let chart = allocator.allocate(&patch).unwrap();
            (patch, chart)
        })
        .collect::<Vec<_>>();
    assert_eq!(allocator.page_count(), 2);
    assert_chart_gutters_disjoint(&config, allocator.page_count(), &charts);
    let values = charts
        .iter()
        .enumerate()
        .map(|(index, (_, chart))| {
            let seed = index as f32 + 1.0;
            (0..chart.width * chart.height)
                .map(|texel| {
                    let x = (texel % chart.width) as f32;
                    let y = (texel / chart.width) as f32;
                    LightmapTexel {
                        irradiance: [
                            x.mul_add(0.07, seed),
                            y.mul_add(0.11, seed * 0.5),
                            seed * 1.3,
                        ],
                        direction: [seed * 0.25, -seed * 0.125, (x + y) * 0.025],
                        axis: [0.5; 2],
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    let atlas = LightmapAtlas::assemble(&config, allocator.page_count(), &charts, &values).unwrap();
    let mut written = vec![vec![false; 256]; atlas.page_count()];
    for ((patch, chart), data) in charts.iter().zip(&values) {
        assert_assembled_chart_gutter(
            &config,
            patch,
            chart,
            data,
            &atlas.pages()[usize::from(chart.page)],
            &mut written[usize::from(chart.page)],
        );
    }
    for (page, touched) in atlas.pages().iter().zip(&written) {
        for (texel, touched) in page.texels.iter().zip(touched) {
            if !touched {
                assert_eq!(
                    *texel,
                    LightmapTexel::ZERO,
                    "assembly must not write outside any chart's real reserved gutter"
                );
            }
        }
    }
    assert_adjacent_model_reservations(&charts);
}

fn assert_assembled_chart_gutter(
    config: &crate::lighting::lightmap::LightmapConfig,
    patch: &crate::lighting::lightmap::LightmapPatch,
    chart: &crate::lighting::lightmap::Chart,
    data: &[LightmapTexel],
    page: &crate::lighting::lightmap::LightmapPage,
    touched: &mut [bool],
) {
    let padding = config.padding_for(patch.kind);
    let width = usize::try_from(chart.width).unwrap();
    let edge = usize::try_from(config.page_edge).unwrap();
    for y in chart.y - padding..chart.y + chart.height + padding {
        for x in chart.x - padding..chart.x + chart.width + padding {
            let source_x = x.clamp(chart.x, chart.x + chart.width - 1) - chart.x;
            let source_y = y.clamp(chart.y, chart.y + chart.height - 1) - chart.y;
            let offset =
                usize::try_from(source_y).unwrap() * width + usize::try_from(source_x).unwrap();
            assert_eq!(
                page.texel(x, y).unwrap(),
                data[offset],
                "model/architecture data and gutters must keep their own exact HDR color and moment at({x},{y})"
            );
            touched[usize::try_from(y).unwrap() * edge + usize::try_from(x).unwrap()] = true;
        }
    }
}

fn assert_adjacent_model_reservations(
    charts: &[(
        crate::lighting::lightmap::LightmapPatch,
        crate::lighting::lightmap::Chart,
    )],
) {
    use crate::lighting::lightmap::PatchKind;
    // Require neighboring reservations to touch: a widely separated layout
    // would not detect accidentally dilating Prop charts with two texels.
    let props = charts
        .iter()
        .filter(|(patch, _)| patch.kind == PatchKind::Prop)
        .collect::<Vec<_>>();
    let mut adjacent = false;
    for (index, (_, a)) in props.iter().enumerate() {
        for (_, b) in &props[index + 1..] {
            if a.page == b.page {
                let y_overlap = a.y - 1 < b.y + b.height + 1 && b.y - 1 < a.y + a.height + 1;
                let x_overlap = a.x - 1 < b.x + b.width + 1 && b.x - 1 < a.x + a.width + 1;
                adjacent |= (y_overlap
                    && (a.x + a.width + 1 == b.x - 1 || b.x + b.width + 1 == a.x - 1))
                    || (x_overlap
                        && (a.y + a.height + 1 == b.y - 1 || b.y + b.height + 1 == a.y - 1));
            }
        }
    }
    assert!(
        adjacent,
        "real adjacent model reservations are required to prove color isolation"
    );
}

#[test]
fn real_dense_showcase_metadata_exceeds_material_budget_and_loads_under_its_own_limit() {
    let root = crate::assets::resolve_asset_root().unwrap();
    let package = root.join("levels/lantern_hollow.placesmap");
    let opened = crate::package::world::open(&package).unwrap();
    let full = opened.manifest.variant("full").unwrap();
    assert!(full.lightmap_failure.is_none());
    let metadata = full.entries.lightmaps_meta.as_ref().unwrap();
    let entry = opened.manifest.entry(metadata).unwrap();
    assert!(
        entry.bytes > crate::package::MAX_MATERIALS_BYTES,
        "the real dense-model chart record must reproduce the previous materials-cap rejection"
    );
    assert!(entry.bytes <= crate::package::MAX_LIGHTMAP_METADATA_BYTES);
    assert_eq!(
        crate::package::MAX_LIGHTMAP_METADATA_BYTES,
        64 * 1024 * 1024
    );
    let mut reader =
        crate::package::PackageReader::new(std::fs::File::open(&package).unwrap()).unwrap();
    assert!(
        reader
            .read_entry(metadata, crate::package::MAX_MATERIALS_BYTES)
            .is_err(),
        "this fixture must fail the old unrelated materials-record budget"
    );
    drop(reader);

    let mut assets = crate::props::PropAssets::with_root(root);
    let loaded = crate::package::world::load_variant(
        &package,
        &opened.manifest,
        crate::quality::LightmapQuality::Full,
        &mut assets,
    )
    .expect(
        "the actual runtime record reader must accept dense model charts using their dedicated cap",
    );
    let atlas = loaded.lightmaps.as_ref().unwrap();
    assert!(
        atlas.charts.len() >= 230_000,
        "this must exercise the real 230k-chart bake, never padded synthetic JSON"
    );
    assert_eq!(atlas.pages.len(), 7);
    assert!(
        loaded
            .props
            .iter()
            .flat_map(|batch| &batch.vertices)
            .any(Vertex::is_lightmapped)
    );
    for suffix in ["showcase_stump_seat.glb", "showcase_boulder.glb"] {
        let batches = loaded
            .props
            .iter()
            .filter(|batch| batch.model.ends_with(suffix))
            .collect::<Vec<_>>();
        assert!(
            !batches.is_empty(),
            "{suffix} must be a real compiled receiver"
        );
        assert!(
            batches
                .iter()
                .flat_map(|batch| &batch.vertices)
                .all(Vertex::is_lightmapped),
            "{suffix} must retain per-surface atlas coordinates through real package decoding"
        );
    }
}
