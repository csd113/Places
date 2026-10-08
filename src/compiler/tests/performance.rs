//! Compiler input identity, bounded workers and safe incremental publication.
use super::*;

fn evidence_directory(tag: &str) -> PathBuf {
    let base = std::env::var_os("PLACES_COMPILER_TEST_EVIDENCE").map_or_else(
        || PathBuf::from("target/agent-work/compiler-regressions"),
        PathBuf::from,
    );
    let path = base.join(format!("{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&path).expect("create retained regression evidence");
    path
}

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../../tests/fixtures/levels/test_room.json"
    ))
    .expect("checked-in test room")
}

fn reuse_fixture() -> serde_json::Value {
    serde_json::json!({
        "format_version":3, "id":"compiler_reuse", "name":"Compiler reuse",
        "spawn":{"x":1.5,"z":1.5},
        "rooms":[{"x":0.0,"z":0.0,"width":3.0,"depth":3.0,"height":2.5}],
        "walls":[{"x":0.0,"z":0.0,"width":3.0,"depth":0.1,"height":2.5}],
        "ceiling_lights":[{"fixture":"core:fluorescent_panel_01","x":1.5,"z":1.5}],
        "props":[{"model":"core:crate","x":0.7,"z":0.7,"scale":0.5,"solid":true}]
    })
}

fn request(source: &Path, out: &Path, workers: usize) -> BuildRequest {
    BuildRequest {
        source: source.to_path_buf(),
        out: out.to_path_buf(),
        asset_root: PathBuf::from("assets"),
        variants: vec![LightmapQuality::Off],
        workers,
        force: false,
        capture_probes: false,
    }
}

fn write_source(path: &Path, value: &serde_json::Value) {
    std::fs::write(path, serde_json::to_vec_pretty(value).expect("source JSON"))
        .expect("retain source");
}

#[test]
fn metadata_navigation_reuse_has_no_orphans_and_matches_clean_at_one_and_many_workers() {
    let directory = evidence_directory("reuse");
    let out = directory.join("working.placesmap");
    let mut value = reuse_fixture();
    for (iteration, workers) in [1, 12, 64].into_iter().enumerate() {
        value["name"] = format!("Compiler reuse regression {iteration}").into();
        value["props"][0]["components"] = serde_json::json!([{
            "component": "nav_agent", "radius": if iteration == 0 { 0.1_f64 } else { 0.2_f64 },
            "height": 0.16_f64, "speed_mps": 0.2_f64, "step_height": 0.2_f64, "max_slope": 2.666_7_f64
        }]);
        let source = directory.join(format!("iteration-{iteration}.json"));
        write_source(&source, &value);
        let mut all_variants = request(&source, &out, workers);
        all_variants.variants = vec![
            LightmapQuality::Off,
            LightmapQuality::Medium,
            LightmapQuality::Full,
        ];
        let report = build(&all_variants).expect("incremental build");
        assert_eq!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("reused prepared geometry")),
            iteration > 0
        );
        let fresh = out.with_extension("clean.placesmap");
        all_variants.out = fresh.clone();
        all_variants.force = true;
        drop(build(&all_variants).expect("independent clean build of every variant"));
        assert_eq!(
            std::fs::read(&out).expect("incremental bytes"),
            std::fs::read(fresh).expect("clean bytes")
        );
        let _with_extension_status = std::fs::copy(&out, source.with_extension("placesmap"))
            .expect("retain playable variant");
        let manifest = inspect(&out).expect("inspect current package");
        let navigation: Vec<_> = manifest
            .entries
            .iter()
            .filter(|entry| entry.role == "navigation")
            .collect();
        assert_eq!(
            navigation.len(),
            1,
            "old navigation records must never accumulate"
        );
        assert_eq!(navigation[0].name, manifest.variants[0].entries.navigation);
    }
}

#[test]
fn failed_source_and_invalid_worker_budget_preserve_last_valid_package() {
    let directory = evidence_directory("failure");
    let source = directory.join("valid.json");
    let out = directory.join("valid.placesmap");
    write_source(&source, &fixture());
    drop(build(&request(&source, &out, 1)).expect("initial valid package"));
    let good = std::fs::read(&out).expect("valid archive");
    assert!(build(&request(&source, &out, 0)).is_err());
    let invalid = directory.join("invalid-source.expected-failure");
    std::fs::write(&invalid, b"{invalid").expect("retain malformed input");
    assert!(build(&request(&invalid, &out, 12)).is_err());
    assert_eq!(std::fs::read(&out).expect("previous archive"), good);
    assert!(!out.with_extension("placesmap.partial").exists());
    drop(validate(&out).expect("last valid package still decodes"));
}

#[test]
fn corrupt_cached_lighting_rebuilds_both_unchanged_and_metadata_only_inputs() {
    let directory = evidence_directory("corrupt-cache");
    let source = directory.join("valid.json");
    let out = directory.join("cache.placesmap");
    let mut value = reuse_fixture();
    write_source(&source, &value);
    let mut build_request = request(&source, &out, 12);
    build_request.variants = vec![
        LightmapQuality::Off,
        LightmapQuality::Medium,
        LightmapQuality::Full,
    ];
    drop(build(&build_request).expect("valid all-quality cache"));
    let good = std::fs::read(&out).expect("valid original bytes");
    let _join_status = std::fs::copy(&source, directory.join("before.json"))
        .expect("retain original authoring input");
    std::fs::write(directory.join("before.placesmap"), &good)
        .expect("retain original playable package");
    let manifest = inspect(&out).expect("valid original manifest");
    let poisoned = &manifest.variants[1].entries.lighting;
    let mut reader = open_package(&out).expect("read original archive");
    let mut entries = Vec::new();
    for name in reader.names().to_vec() {
        let mut bytes = reader
            .read_entry(&name, crate::package::MAX_BINARY_BYTES)
            .expect("bounded fixture record");
        if name == *poisoned {
            *bytes.last_mut().expect("nonempty lighting record") ^= 1;
        }
        entries.push(PendingEntry { name, bytes });
    }
    drop(reader);
    write_archive(&out, entries).expect("retain readable ZIP with invalid lighting hash");
    let bad = directory.join("corrupt-cache.expected-failure");
    let _expect_status = std::fs::copy(&out, &bad).expect("preserve corruption reproduction");
    assert!(validate(&out).is_err());
    let rebuilt = build(&build_request).expect("unchanged input safely rebuilds poisoned cache");
    assert!(rebuilt.rebuilt);
    assert_eq!(std::fs::read(&out).expect("repaired bytes"), good);

    let _expect_status_2 = std::fs::copy(&bad, &out).expect("restore retained poisoned cache");
    value["name"] = "Corrupt cache metadata regression".into();
    write_source(&source, &value);
    let edited = build(&build_request).expect("metadata edit safely rejects poisoned stage");
    assert!(
        !edited
            .warnings
            .iter()
            .any(|warning| warning.contains("reused prepared geometry"))
    );
    let fresh = out.with_extension("clean.placesmap");
    build_request.out = fresh.clone();
    build_request.force = true;
    drop(build(&build_request).expect("independent clean edited-source build"));
    assert_eq!(
        std::fs::read(&out).expect("repaired metadata archive"),
        std::fs::read(fresh).expect("independent clean bytes")
    );
    assert!(!out.with_extension("placesmap.partial").exists());
}

#[test]
fn changed_catalogue_invalidates_both_package_and_prepared_stage() {
    let directory = evidence_directory("catalogue");
    let root = directory.join("asset-root");
    std::fs::create_dir_all(&root).expect("retained custom asset root");
    let source = directory.join("catalogue-input.json");
    let out = directory.join("catalogue-input.placesmap");
    write_source(&source, &fixture());
    let (catalogue, _) =
        load_catalog_identity(Path::new("assets/catalog.json")).expect("original catalogue");
    let level = LevelDef::from_json(&fixture().to_string()).expect("fixture");
    for dependency in collect_dependencies(&level, &catalogue, Path::new("assets"), &mut Vec::new())
        .expect("complete dependencies")
    {
        let target = root.join(&dependency.path);
        std::fs::create_dir_all(target.parent().expect("dependency parent"))
            .expect("dependency directories");
        let _join_status = std::fs::copy(Path::new("assets").join(&dependency.path), target)
            .expect("preserve exact original input bytes");
    }
    let mut raw: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string("assets/catalog.json").expect("catalogue source"),
    )
    .expect("catalogue JSON");
    let catalogue_path = root.join("catalog.json");
    write_source(&catalogue_path, &raw);
    let mut build_request = request(&source, &out, 1);
    build_request.asset_root = root;
    let original = build(&build_request).expect("original custom-root package");
    let _join_status_2 = std::fs::copy(&catalogue_path, directory.join("catalogue-before.json"))
        .expect("retain initial catalogue variant");
    let _join_status_3 = std::fs::copy(&out, directory.join("catalogue-before.placesmap"))
        .expect("retain initial package variant");
    raw["assets"][0]["display_name"] = "Compiler catalogue identity regression".into();
    write_source(&catalogue_path, &raw);
    let changed = build(&build_request).expect("changed custom-root package");
    assert!(changed.rebuilt);
    assert_ne!(changed.fingerprint, original.fingerprint);
    assert!(
        !changed
            .warnings
            .iter()
            .any(|warning| warning.contains("reused prepared geometry"))
    );
    let fresh = out.with_extension("clean.placesmap");
    build_request.out = fresh.clone();
    build_request.force = true;
    drop(build(&build_request).expect("independent changed-catalogue build"));
    assert_eq!(
        std::fs::read(out).expect("incremental bytes"),
        std::fs::read(fresh).expect("clean bytes")
    );
}

#[test]
fn catalogue_bytes_and_each_referenced_image_class_are_build_inputs() {
    let (catalog, original_hash) =
        load_catalog_identity(Path::new("assets/catalog.json")).expect("shipped catalogue");
    let level = LevelDef::from_json(include_str!(
        "../../../tests/fixtures/levels/test_room.json"
    ))
    .expect("test room");
    let input = lighting_fingerprint(&level, &[LightmapQuality::Off], &[]).expect("stage key");
    let original = fingerprint_with_catalog(&input, &original_hash);
    assert_eq!(original, fingerprint_with_catalog(&input, &original_hash));
    assert_ne!(
        original,
        sha256_hex(format!("asset_inputs_v1\ninput {input}\ncatalog {original_hash}\n").as_bytes()),
        "the finalized compiler must invalidate experimental pipeline packages"
    );
    assert_ne!(
        original,
        fingerprint_with_catalog(&input, &sha256_hex(b"changed catalogue"))
    );
    let mut altered = level;
    altered.defaults.wall = "core:plastic_panel_01".to_string();
    let mut warnings = Vec::new();
    let dependencies = collect_dependencies(&altered, &catalog, Path::new("assets"), &mut warnings)
        .expect("collect complete inputs");
    let normal = catalog
        .assets()
        .material("core:plastic_panel_01")
        .and_then(|entry| entry.normal_texture.as_deref())
        .expect("normal map");
    let normal_path = catalog
        .assets()
        .texture_path(normal)
        .expect("normal PNG path");
    assert!(
        dependencies
            .iter()
            .any(|dependency| dependency.path == normal_path)
    );
    assert_eq!(
        dependencies
            .iter()
            .filter(|dependency| dependency.path == normal_path)
            .count(),
        1
    );
    // A constant existing white sheet preserves any UV frame. No test artwork
    // or production catalogue is written; this exercises the mask dependency.
    let mut raw: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string("assets/catalog.json").expect("catalogue text"),
    )
    .expect("catalogue JSON");
    let material = raw["assets"]
        .as_array_mut()
        .expect("assets")
        .iter_mut()
        .find(|entry| entry["id"] == "core:plastic_panel_01")
        .expect("panel material");
    material["emissive"] = serde_json::json!([0.1_f64, 0.1_f64, 0.1_f64]);
    material["emissive_mask"] = "core:tex_white_01".into();
    let catalogue =
        crate::loader::PropCatalog::from_json_str(&raw.to_string()).expect("mask catalogue");
    let mask_dependencies =
        collect_dependencies(&altered, &catalogue, Path::new("assets"), &mut warnings)
            .expect("mask inputs");
    let mask = catalogue
        .assets()
        .texture_path("core:tex_white_01")
        .expect("mask PNG path");
    assert!(
        mask_dependencies
            .iter()
            .any(|dependency| dependency.path == mask)
    );
}

#[test]
fn runtime_template_closure_covers_embedded_images_and_unused_overrides_once() {
    let (catalog, _) =
        load_catalog_identity(Path::new("assets/catalog.json")).expect("hero catalogue");
    let mut level = LevelDef::from_json(include_str!(
        "../../../tests/fixtures/levels/art_style_hero_transparency.json"
    ))
    .expect("transparency hero");
    let mut alternate = level.spawn_templates[1].clone();
    alternate.id = "unused_override".into();
    alternate.model = "sheet-ghost-cat".into();
    level.spawn_templates.push(alternate);
    let dependencies = collect_dependencies(&level, &catalog, Path::new("assets"), &mut Vec::new())
        .expect("complete runtime closure");
    for id in ["home:dining_chair", "sheet-ghost", "sheet-ghost-cat"] {
        let path = catalog.get(id).model.expect("real runtime model");
        assert_eq!(
            dependencies
                .iter()
                .filter(|dependency| dependency.path == path)
                .count(),
            1,
            "placed/template duplicates and unused overrides still have one dependency"
        );
        let bytes = std::fs::read(Path::new("assets").join(path)).expect("real GLB bytes");
        let model = crate::gltf::parse_glb(&bytes).expect("self-contained runtime GLB");
        assert!(
            !model.textures.is_empty(),
            "GLB identity contains its real PNG textures"
        );
    }
}

#[test]
fn runtime_only_model_bytes_and_new_template_paths_make_currency_stale() {
    let directory = evidence_directory("runtime-dependencies");
    let root = directory.join("assets");
    std::fs::create_dir_all(&root).expect("isolated runtime dependency root");
    let source = directory.join("runtime.json");
    let out = directory.join("runtime.placesmap");
    let mut value = reuse_fixture();
    value["spawn_templates"] = serde_json::json!([{"id":"ghost", "model":"sheet-ghost"}]);
    write_source(&source, &value);
    let (catalog, _) =
        load_catalog_identity(Path::new("assets/catalog.json")).expect("runtime catalogue");
    let mut level = LevelDef::from_json(&value.to_string()).expect("runtime source");
    crate::loader::prepare_level(&mut level, catalog.assets(), None);
    let mut dependencies =
        collect_dependencies(&level, &catalog, Path::new("assets"), &mut Vec::new())
            .expect("runtime inputs");
    // Preserve an unreferenced alternate model too; currency must discover it
    // from a new template, rather than only rehash the old manifest paths.
    let alternate_path = catalog
        .get("sheet-ghost-cat")
        .model
        .expect("alternate model");
    dependencies.push(PackageDependency {
        kind: DependencyKind::Model,
        path: alternate_path.clone(),
        sha256: String::new(),
        bytes: 0,
    });
    for dependency in dependencies {
        let destination = root.join(&dependency.path);
        std::fs::create_dir_all(destination.parent().expect("dependency parent"))
            .expect("dependency directories");
        let _copied = std::fs::copy(Path::new("assets").join(dependency.path), destination)
            .expect("preserve input bytes");
    }
    let _copied_catalog = std::fs::copy("assets/catalog.json", root.join("catalog.json"))
        .expect("preserve catalogue");
    let mut build_request = request(&source, &out, 1);
    build_request.asset_root = root.clone();
    build_request.capture_probes = true;
    let original = build(&build_request).expect("runtime-only dependency package");
    assert!(
        original
            .variant_stats
            .iter()
            .all(|variant| variant.probe_points == 0),
        "this currency regression does not require a GPU capture"
    );
    assert!(
        verify(&source, &out, &root)
            .expect("current source")
            .current,
        "ordinary currency agrees with the complete closure"
    );
    let model_path = root.join(catalog.get("sheet-ghost").model.expect("ghost GLB"));
    let original_model = std::fs::read(&model_path).expect("preserved GLB");
    let mut altered = original_model.clone();
    // A same-size byte change is solely a dependency-identity oracle. The
    // altered asset is never compiled or rendered, and is restored afterward.
    *altered.last_mut().expect("nonempty model") ^= 1;
    std::fs::write(&model_path, altered).expect("retain changed-byte oracle");
    let stale = verify(&source, &out, &root).expect("changed-byte currency");
    assert!(
        !stale.current,
        "same-size runtime-only model edits must invalidate currency"
    );
    assert!(
        stale
            .differences
            .iter()
            .any(|reason| reason.contains("sheet-ghost.glb") && reason.ends_with("changed")),
        "the exact changed runtime dependency is reported"
    );
    std::fs::write(model_path, original_model).expect("restore valid runtime model");
    value["spawn_templates"]
        .as_array_mut()
        .expect("templates")
        .push(serde_json::json!({"id":"alternate", "model":"sheet-ghost-cat"}));
    write_source(&source, &value);
    let added = verify(&source, &out, &root).expect("new-template currency");
    assert!(!added.current, "new runtime model paths must be discovered");
    assert!(
        added
            .differences
            .iter()
            .any(|reason| reason.contains(&alternate_path) && reason.ends_with("added")),
        "currency reports a dependency absent from the old manifest"
    );
}

#[test]
fn presentation_reuse_matches_independent_full_archives_for_every_variant() {
    let directory = evidence_directory("presentation");
    let source = directory.join("presentation.json");
    let out = directory.join("presentation.placesmap");
    let mut value = reuse_fixture();
    write_source(&source, &value);
    let mut build_request = request(&source, &out, 1);
    build_request.variants = LightmapQuality::ALL.to_vec();
    drop(build(&build_request).expect("base prepared package"));
    let before = inspect(&out).expect("base manifest");
    value["environment"] = serde_json::json!({"presentation":{
        "exposure":1.25_f64, "tone_knee":0.8_f64, "saturation":0.97_f64, "contrast":1.0_f64
    }});
    write_source(&source, &value);
    let incremental = build(&build_request).expect("presentation increment");
    assert!(
        incremental
            .cache_decisions
            .iter()
            .any(|decision| decision.stage == "prepared_world" && decision.status == "hit"),
        "presentation skips validated prepared work"
    );
    let after = inspect(&out).expect("incremental manifest");
    assert_eq!(
        before.lighting_fingerprint, after.lighting_fingerprint,
        "presentation is excluded from stored linear HDR"
    );
    for (old, new) in before.variants.iter().zip(&after.variants) {
        assert_eq!(old.entries.mesh, new.entries.mesh, "geometry reused");
        assert_eq!(old.entries.props, new.entries.props, "prop records reused");
        assert_eq!(
            old.entries.lighting, new.entries.lighting,
            "lighting reused"
        );
        assert_eq!(
            old.entries.lightmaps, new.entries.lightmaps,
            "all atlas texels reused"
        );
        assert_eq!(
            old.entries.irradiance, new.entries.irradiance,
            "probe field reused"
        );
        assert_eq!(
            old.entries.collision, new.entries.collision,
            "collision reused"
        );
    }
    build_request.out = directory.join("presentation-clean.placesmap");
    build_request.force = true;
    drop(build(&build_request).expect("independent full presentation build"));
    assert_eq!(
        std::fs::read(&out).expect("incremental archive"),
        std::fs::read(&build_request.out).expect("clean archive"),
        "incremental provenance, atlas metadata and all records match full bytes"
    );
    let level = LevelDef::from_json(&value.to_string()).expect("presentation source");
    let mut fog = level.clone();
    fog.environment.as_mut().expect("environment").fog.density = 0.02;
    assert_ne!(
        lighting_fingerprint(&level, &build_request.variants, &[]).expect("presentation key"),
        lighting_fingerprint(&fog, &build_request.variants, &[]).expect("fog key"),
        "fog remains an input because reflection capture consumes it"
    );
}

#[test]
fn compiler_tool_and_capture_mode_identity_are_automatic_and_path_independent() {
    let executable = std::env::current_exe().expect("test tool path");
    assert_eq!(
        compiler_tool_sha256().expect("cached tool identity"),
        crate::package::hash::sha256_file(&executable).expect("independent executable hash"),
        "the tool identity hashes the actual installed bytes"
    );
    let inputs = BuildInputs {
        revision: 1,
        source_sha256: sha256_hex(b"source"),
        catalog_sha256: sha256_hex(b"catalogue"),
        tool_sha256: sha256_hex(b"tool"),
        capture_probes: false,
    };
    let key = fingerprint_with_build_inputs("stage", &inputs);
    let mut tool_edit = inputs.clone();
    tool_edit.tool_sha256 = sha256_hex(b"changed compiled code or WGSL");
    assert_ne!(
        key,
        fingerprint_with_build_inputs("stage", &tool_edit),
        "tool edits invalidate prepared work"
    );
    let mut capture_edit = inputs.clone();
    capture_edit.capture_probes = true;
    assert_ne!(
        key,
        fingerprint_with_build_inputs("stage", &capture_edit),
        "CPU-only work cannot substitute for captured cubes"
    );
    let mut display_edit = inputs;
    display_edit.source_sha256 = sha256_hex(b"changed presentation bytes");
    assert_eq!(
        key,
        fingerprint_with_build_inputs("stage", &display_edit),
        "the stage wrapper does not reintroduce raw source bytes"
    );
}

#[test]
fn compiler_input_provenance_corruption_and_absence_rebuild_safely() {
    let directory = evidence_directory("input-integrity");
    let source = directory.join("source.json");
    let out = directory.join("integrity.placesmap");
    write_source(&source, &reuse_fixture());
    let build_request = request(&source, &out, 1);
    drop(build(&build_request).expect("valid provenance package"));
    let good = std::fs::read(&out).expect("valid archive bytes");
    let current = build(&build_request).expect("unchanged current archive");
    assert!(
        !current.rebuilt && current.cache_decisions[0].status == "hit",
        "unchanged inputs reuse verified package"
    );
    for case in ["corrupt", "absent", "role", "revision"] {
        let manifest = inspect(&out).expect("original manifest");
        let mut reader = open_package(&out).expect("original entries");
        let mut entries = Vec::new();
        for name in reader.names().to_vec() {
            if case == "absent" && name == BUILD_INPUTS_ENTRY {
                continue;
            }
            let mut bytes = reader
                .read_entry(&name, crate::package::MAX_BINARY_BYTES)
                .expect("fixture record");
            if name == BUILD_INPUTS_ENTRY {
                if case == "corrupt" {
                    *bytes.last_mut().expect("input bytes") ^= 1;
                } else if case == "revision" {
                    let mut inputs: BuildInputs =
                        serde_json::from_slice(&bytes).expect("input contract");
                    inputs.revision = 2;
                    bytes = crate::canonical_json::canonical_json_bytes(&inputs)
                        .expect("new input revision");
                }
            } else if name == "manifest.json" {
                let mut changed = manifest.clone();
                if case == "absent" {
                    changed
                        .entries
                        .retain(|entry| entry.name != BUILD_INPUTS_ENTRY);
                } else if case == "role" {
                    changed
                        .entries
                        .iter_mut()
                        .find(|entry| entry.name == BUILD_INPUTS_ENTRY)
                        .expect("declared inputs")
                        .role = "invalid-input-role".to_owned();
                } else if case == "revision" {
                    let mut inputs: BuildInputs = serde_json::from_slice(
                        &reader
                            .read_entry(BUILD_INPUTS_ENTRY, crate::package::MAX_MANIFEST_BYTES)
                            .expect("input entry"),
                    )
                    .expect("input contract");
                    inputs.revision = 2;
                    let revised = crate::canonical_json::canonical_json_bytes(&inputs)
                        .expect("new input revision");
                    let entry = changed
                        .entries
                        .iter_mut()
                        .find(|entry| entry.name == BUILD_INPUTS_ENTRY)
                        .expect("declared inputs");
                    entry.sha256 = sha256_hex(&revised);
                    entry.bytes = u64::try_from(revised.len()).expect("bounded inputs");
                }
                bytes = changed.to_json().expect("changed contract manifest");
            }
            entries.push(PendingEntry { name, bytes });
        }
        drop(reader);
        write_archive(&out, entries).expect("retain invalid cache oracle");
        let rebuilt = build(&build_request).expect("provenance failure rebuild");
        assert!(
            rebuilt.rebuilt,
            "invalid provenance never hits package cache"
        );
        assert!(
            rebuilt
                .cache_decisions
                .iter()
                .all(|decision| decision.status == "miss"),
            "both complete and prepared caches reject invalid provenance"
        );
        assert_eq!(
            std::fs::read(&out).expect("repaired archive"),
            good,
            "safe full rebuilding restores exact valid bytes"
        );
    }
}
