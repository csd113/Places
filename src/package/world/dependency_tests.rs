//! Package-open dependency identity contracts, with isolated installed assets.
#![allow(
    clippy::expect_used,
    reason = "Regression fixture construction fails immediately on invalid setup; this exception is confined to tests"
)]

use super::*;
use crate::package::{DependencyKind, PackageDependency, PackageEntry, VariantEntries};
use std::io::Write as _;
use std::path::PathBuf;

const MODEL: &str = "environment/home/props/models/dining_chair.glb";
const TEXTURE: &str = "environment/home/textures/walls/wall_paint_warm_01.png";

type FixtureRecord = (&'static str, &'static str, Vec<u8>);

fn evidence_directory(tag: &str) -> PathBuf {
    let base = std::env::var_os("PLACES_COMPILER_TEST_EVIDENCE").map_or_else(
        || PathBuf::from("target/agent-work/package-dependencies"),
        PathBuf::from,
    );
    let directory = base.join(format!("package-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("dependency evidence directory");
    directory
}

fn installed_dependencies(root: &Path) -> Vec<PackageDependency> {
    [
        (DependencyKind::Model, MODEL),
        (DependencyKind::Texture, TEXTURE),
    ]
    .into_iter()
    .map(|(kind, path)| {
        let bytes = std::fs::read(Path::new("assets").join(path)).expect("real hero asset");
        let destination = root.join(path);
        std::fs::create_dir_all(destination.parent().expect("asset parent"))
            .expect("installed asset directory");
        std::fs::write(destination, &bytes).expect("isolated installed asset");
        PackageDependency {
            kind,
            path: path.to_owned(),
            bytes: u64::try_from(bytes.len()).expect("fixture length"),
            sha256: crate::package::hash::sha256_hex(&bytes),
        }
    })
    .collect()
}

fn dependency_records(level: &LevelDef) -> [FixtureRecord; 5] {
    let mesh = LevelMesh {
        ranges: Vec::new(),
        batches: crate::render::LevelMeshBatches::default(),
        vertex_count: 0,
        index_count: 0,
    };
    let collision = CompiledCollision {
        walls: Vec::new(),
        floor: crate::level::WalkableFloor::from_level(level),
        ceiling: crate::level::WalkableCeiling::from_level(level),
        water: crate::level::WaterVolumes::new(),
        ladders: crate::level::Ladders::new(),
    };
    let navigation = crate::package::navigation::NavGrid {
        cell_m: 1.0,
        cells_x: 1,
        cells_z: 1,
        classes: vec![
            crate::package::navigation::NavClass::new(0.3, 1.8, 0.3, 1.0)
                .expect("navigation class"),
        ],
        cell_y: vec![0.0],
        cell_flags: vec![crate::package::navigation::CELL_SURFACE],
        cell_headroom_cm: vec![250],
        cell_portal: vec![crate::package::navigation::NO_PORTAL],
        walkable: vec![vec![1]],
        region: vec![vec![0]],
        ..crate::package::navigation::NavGrid::default()
    };
    [
        (
            "mesh",
            ".mesh",
            crate::package::mesh::write_mesh(&mesh).expect("mesh"),
        ),
        (
            "props",
            ".props",
            crate::package::props::write_props(&[]).expect("props"),
        ),
        (
            "lighting",
            ".lighting",
            crate::package::lighting::write_lighting(&LevelLighting::bake(level))
                .expect("lighting"),
        ),
        (
            "collision",
            ".collision",
            crate::package::collision::write_collision(&collision).expect("collision"),
        ),
        (
            "navigation",
            ".navigation",
            crate::package::navigation::write_navigation(&navigation).expect("navigation"),
        ),
    ]
}

/// A small, fully decodable package isolates the open/install boundary without
/// running the compiler or a transport/reflection solve.
fn dependency_archive(dependencies: Vec<PackageDependency>) -> Vec<u8> {
    dependency_archive_with_inputs(dependencies, None)
}

fn dependency_archive_with_inputs(
    dependencies: Vec<PackageDependency>,
    inputs: Option<(&str, Vec<u8>)>,
) -> Vec<u8> {
    let level = LevelDef::from_json(
        r#"{"format_version":3,"id":"dependency_contract","name":"Dependency contract",
            "spawn":{"x":0.5,"z":0.5},
            "rooms":[{"x":0.0,"z":0.0,"width":1.0,"depth":1.0,"height":2.5}]}"#,
    )
    .expect("minimal package semantics");
    let records = dependency_records(&level);
    let [mesh, props, lighting, collision, navigation] = records
        .each_ref()
        .map(|(_, suffix, bytes)| crate::package::hash::blob_name(bytes, suffix));
    let semantics = crate::canonical_json::canonical_json_bytes(&level).expect("semantics");
    let mut entries: Vec<_> = records
        .iter()
        .map(|(role, suffix, bytes)| PackageEntry {
            name: crate::package::hash::blob_name(bytes, suffix),
            role: (*role).to_owned(),
            bytes: u64::try_from(bytes.len()).expect("record length"),
            sha256: crate::package::hash::sha256_hex(bytes),
        })
        .collect();
    entries.push(PackageEntry {
        name: "semantics.json".to_owned(),
        role: "semantics".to_owned(),
        bytes: u64::try_from(semantics.len()).expect("semantics length"),
        sha256: crate::package::hash::sha256_hex(&semantics),
    });
    if let Some((role, bytes)) = &inputs {
        entries.push(PackageEntry {
            name: "build-inputs.json".to_owned(),
            role: (*role).to_owned(),
            bytes: u64::try_from(bytes.len()).expect("provenance length"),
            sha256: crate::package::hash::sha256_hex(bytes),
        });
    }
    let manifest = Manifest {
        package_format: crate::package::FORMAT_VERSION,
        id: level.id.clone(),
        name: level.name.clone(),
        author: level.author.clone(),
        created_by: "package dependency regression".to_owned(),
        compiler_fingerprint: "0".repeat(64),
        lighting_fingerprint: None,
        required_capabilities: vec!["geometry".to_owned()],
        dependencies,
        entries,
        variants: vec![Variant {
            lightmap_quality: "off".to_owned(),
            quality_profile: "low".to_owned(),
            lightmap_failure: None,
            entries: VariantEntries {
                mesh,
                props,
                lighting,
                collision,
                navigation,
                lightmaps: None,
                lightmaps_meta: None,
                irradiance: None,
                probes: Vec::new(),
            },
        }],
    };
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    if let Some((_, bytes)) = inputs {
        writer
            .start_file("build-inputs.json", options)
            .expect("provenance entry");
        writer.write_all(&bytes).expect("provenance payload");
    }
    for (_, suffix, bytes) in records {
        writer
            .start_file(crate::package::hash::blob_name(&bytes, suffix), options)
            .expect("record entry");
        writer.write_all(&bytes).expect("record payload");
    }
    for (name, bytes) in [
        ("semantics.json", semantics),
        ("manifest.json", manifest.to_json().expect("manifest")),
    ] {
        writer.start_file(name, options).expect("named entry");
        writer.write_all(&bytes).expect("named payload");
    }
    writer.finish().expect("package fixture").into_inner()
}

fn open_at(bytes: &[u8], root: Option<&Path>) -> Result<OpenedPackage, String> {
    open_reader_with_asset_root(package_reader(bytes)?, root)
}

#[test]
fn package_open_rejects_same_size_png_and_glb_substitution() {
    let directory = evidence_directory("same-size");
    let root = directory.join("assets");
    let bytes = dependency_archive(installed_dependencies(&root));
    std::fs::write(directory.join("valid.placesmap"), &bytes).expect("retained package");
    let opened = open_at(&bytes, Some(&root)).expect("original dependencies match");
    let mut assets = crate::props::PropAssets::with_root(&root);
    drop(
        load_variant_bytes(&bytes, &opened.manifest, LightmapQuality::Off, &mut assets)
            .expect("fixture is fully decodable"),
    );

    for path in [TEXTURE, MODEL] {
        let destination = root.join(path);
        let original = std::fs::read(&destination).expect("original dependency");
        let mut substituted = original.clone();
        // A same-length identity oracle, rejected before image/model decoding.
        *substituted.last_mut().expect("nonempty real asset") ^= 1_u8;
        std::fs::write(&destination, &substituted).expect("same-size substitution");
        let error = open_at(&bytes, Some(&root)).expect_err("changed bytes must be rejected");
        assert!(error.contains(path), "dependency path: {error}");
        assert!(error.contains("SHA-256 changed"), "identity cause: {error}");
        assert!(error.contains("places-compile"), "rebuild action: {error}");
        std::fs::write(destination, original).expect("restore immutable dependency");
        drop(open_at(&bytes, Some(&root)).expect("restored bytes open unchanged package"));
    }
}

#[test]
fn package_open_keeps_named_missing_and_resized_dependency_failures() {
    let directory = evidence_directory("missing-size");
    let root = directory.join("assets");
    let bytes = dependency_archive(installed_dependencies(&root));
    let path = root.join(MODEL);
    let original = std::fs::read(&path).expect("original GLB");
    std::fs::write(&path, b"short").expect("resized dependency");
    let error = open_at(&bytes, Some(&root)).expect_err("resized resource rejected");
    assert!(
        error.contains(MODEL) && error.contains("recorded"),
        "{error}"
    );
    std::fs::remove_file(&path).expect("missing dependency");
    let missing_error = open_at(&bytes, Some(&root)).expect_err("missing resource rejected");
    assert!(
        missing_error.contains(MODEL) && missing_error.contains("missing"),
        "{missing_error}"
    );
    std::fs::write(path, original).expect("restore GLB");
    drop(open_at(&bytes, Some(&root)).expect("restored package remains valid"));
}

#[test]
fn the_original_embedded_demo_still_opens_without_an_asset_root() {
    let opened = open_at(crate::loader::embedded_demo_package(), None)
        .expect("original embedded fallback in a truly asset-less install");
    assert_eq!(opened.level.id, crate::loader::DEMO_LEVEL_ID);
    let missing_root = evidence_directory("empty-assets");
    let error = open_at(crate::loader::embedded_demo_package(), Some(&missing_root))
        .expect_err("an installed root cannot silently skip missing dependencies");
    assert!(
        error.contains("package dependency") && error.contains("missing"),
        "{error}"
    );
}

#[test]
fn new_package_catalog_identity_rejects_same_size_edits_and_legacy_remains_supported() {
    let directory = evidence_directory("catalog");
    let root = directory.join("assets");
    let dependencies = installed_dependencies(&root);
    let original = br#"{"assets":[],"materials":[]}"#;
    std::fs::write(root.join("catalog.json"), original).expect("installed catalog fixture");
    let inputs = serde_json::to_vec(&serde_json::json!({
        "revision": 1_u32,
        "catalog_sha256": crate::package::hash::sha256_hex(original),
        "source_sha256": "runtime ignores source",
        "tool_sha256": "runtime ignores tool",
        "capture_probes": false
    }))
    .expect("revision-one provenance");
    let new =
        dependency_archive_with_inputs(dependencies.clone(), Some(("compiler-inputs", inputs)));
    let legacy = dependency_archive(dependencies);
    drop(open_at(&new, Some(&root)).expect("matching catalog opens new package"));
    std::fs::write(
        root.join("catalog.json"),
        br#"{"assets":[],"materials":{}}"#,
    )
    .expect("same-size catalog substitution");
    let error = open_at(&new, Some(&root)).expect_err("new package rejects changed catalog");
    assert!(
        error.contains("catalog.json") && error.contains("SHA-256 changed"),
        "{error}"
    );
    assert!(error.contains("places-compile"), "{error}");
    std::fs::write(
        root.join("catalog.json"),
        br#"{"assets":[{"id":"additive"}],"materials":[]}"#,
    )
    .expect("additive legacy catalog");
    drop(
        open_at(&legacy, Some(&root))
            .expect("absent optional provenance retains legacy compatibility"),
    );
    drop(open_at(&new, None).expect("true asset-less root skips installed catalog verification"));
}

#[test]
fn declared_optional_provenance_rejects_malformed_revision_role_and_digest() {
    for (tag, role, inputs, reason) in [
        (
            "revision",
            "compiler-inputs",
            serde_json::json!({"revision":2_u32,"catalog_sha256":"0".repeat(64)}),
            "revision 2",
        ),
        (
            "role",
            "texture",
            serde_json::json!({"revision":1_u32,"catalog_sha256":"0".repeat(64)}),
            "invalid role",
        ),
        (
            "digest",
            "compiler-inputs",
            serde_json::json!({"revision":1_u32,"catalog_sha256":"bad"}),
            "invalid catalog SHA-256",
        ),
        (
            "malformed",
            "compiler-inputs",
            serde_json::json!({"catalog_sha256":"0".repeat(64)}),
            "malformed",
        ),
    ] {
        let directory = evidence_directory(tag);
        let bytes = dependency_archive_with_inputs(
            Vec::new(),
            Some((
                role,
                serde_json::to_vec(&inputs).expect("invalid input fixture"),
            )),
        );
        std::fs::write(directory.join("rejected.placesmap"), &bytes)
            .expect("retained invalid provenance");
        let error =
            open_at(&bytes, None).expect_err("declared malformed record cannot bypass guard");
        assert!(error.contains(reason), "{tag}: {error}");
    }
}
