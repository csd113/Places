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
            "component": "nav_agent", "radius": if iteration == 0 { 0.1 } else { 0.2 },
            "height": 0.16, "speed_mps": 0.2, "step_height": 0.2, "max_slope": 2.6667
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
        build(&all_variants).expect("independent clean build of every variant");
        assert_eq!(
            std::fs::read(&out).expect("incremental bytes"),
            std::fs::read(fresh).expect("clean bytes")
        );
        std::fs::copy(&out, source.with_extension("placesmap")).expect("retain playable variant");
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
    build(&request(&source, &out, 1)).expect("initial valid package");
    let good = std::fs::read(&out).expect("valid archive");
    assert!(build(&request(&source, &out, 0)).is_err());
    let invalid = directory.join("invalid-source.expected-failure");
    std::fs::write(&invalid, b"{invalid").expect("retain malformed input");
    assert!(build(&request(&invalid, &out, 12)).is_err());
    assert_eq!(std::fs::read(&out).expect("previous archive"), good);
    assert!(!out.with_extension("placesmap.partial").exists());
    validate(&out).expect("last valid package still decodes");
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
    build(&build_request).expect("valid all-quality cache");
    let good = std::fs::read(&out).expect("valid original bytes");
    std::fs::copy(&source, directory.join("before.json")).expect("retain original authoring input");
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
    std::fs::copy(&out, &bad).expect("preserve corruption reproduction");
    assert!(validate(&out).is_err());
    let rebuilt = build(&build_request).expect("unchanged input safely rebuilds poisoned cache");
    assert!(rebuilt.rebuilt);
    assert_eq!(std::fs::read(&out).expect("repaired bytes"), good);

    std::fs::copy(&bad, &out).expect("restore retained poisoned cache");
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
    build(&build_request).expect("independent clean edited-source build");
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
        std::fs::copy(Path::new("assets").join(&dependency.path), target)
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
    std::fs::copy(&catalogue_path, directory.join("catalogue-before.json"))
        .expect("retain initial catalogue variant");
    std::fs::copy(&out, directory.join("catalogue-before.placesmap"))
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
    build(&build_request).expect("independent changed-catalogue build");
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
    material["emissive"] = serde_json::json!([0.1, 0.1, 0.1]);
    material["emissive_mask"] = "core:tex_white_01".into();
    let catalogue =
        crate::loader::PropCatalog::from_json_str(&raw.to_string()).expect("mask catalogue");
    let dependencies =
        collect_dependencies(&altered, &catalogue, Path::new("assets"), &mut warnings)
            .expect("mask inputs");
    let mask = catalogue
        .assets()
        .texture_path("core:tex_white_01")
        .expect("mask PNG path");
    assert!(
        dependencies
            .iter()
            .any(|dependency| dependency.path == mask)
    );
}
