//! Quiet-startup regression for level discovery.
//!
//! `places --list-levels` runs the exact discovery pass the Level Select menu
//! builds, then exits before any window or GPU work, so the three supported
//! launch layouts can be checked from a test:
//!
//! * the repository layout, launched from the repository root;
//! * the same binary launched from another working directory, where the asset
//!   root must still resolve through the executable's ancestors;
//! * a staged distribution layout, where `PLACES_ASSET_ROOT` selects the root
//!   owning `assets/levels` and `PLACES_STATE_ROOT` owns the writable drop-in
//!   `levels/` directory.
//!
//! In every layout only compiled `.placesmap` packages are rows, authoring
//! sources (the `.json` files kept beside them) are skipped silently, and the
//! process prints nothing to stderr.

// Test code: unwrap/expect, indexing and permissive arithmetic are idiomatic in
// tests; the production lints stay enforced everywhere else.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    reason = "These isolated CLI fixtures fail immediately on invalid setup, process execution or discovery output."
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Removes only this test's staging directory; absence is the sole expected error.
fn remove_stage(path: &Path) {
    if let Err(error) = fs::remove_dir_all(path) {
        assert_eq!(
            error.kind(),
            std::io::ErrorKind::NotFound,
            "cannot clean CLI fixture {}: {error}",
            path.display()
        );
    }
}

/// Stages the exact installed dependency closure the package declares.
///
/// A package-only directory is not a complete portable install.
/// Include the real catalogue and external assets so discovery exercises the
/// ordinary package currency guards in the staged distribution layout.
fn stage_package(repo: &Path, source: &Path, destination: &Path, asset_root: &Path) {
    let manifest = places::package::world::inspect(source).expect("package manifest");
    for dependency in &manifest.dependencies {
        if dependency.kind == places::package::DependencyKind::Embedded {
            continue;
        }
        let installed = repo.join("assets").join(&dependency.path);
        let staged = asset_root.join(&dependency.path);
        fs::create_dir_all(staged.parent().expect("dependency parent"))
            .expect("staged dependency directory");
        let copied = fs::copy(&installed, &staged).expect("stage the declared dependency");
        assert_eq!(copied, dependency.bytes, "{} size", dependency.path);
        assert_eq!(
            places::package::hash::sha256_file(&staged).expect("staged dependency hash"),
            dependency.sha256,
            "{} identity",
            dependency.path
        );
    }
    let _copied_bytes = fs::copy(source, destination).expect("stage the package");
}

/// The binary under test, built by Cargo for this integration test target.
const PLACES: &str = env!("CARGO_BIN_EXE_places");

/// Runs discovery with clean developer overrides and optional package/state roots.
fn list_levels(
    binary: &Path,
    cwd: &Path,
    asset_root: Option<&Path>,
    state_root: Option<&Path>,
) -> Output {
    let mut command = Command::new(binary);
    let _configured_command = command.arg("--list-levels").current_dir(cwd);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("PLACES_") {
            let _removed_override = command.env_remove(key);
        }
    }
    if let Some(root) = asset_root {
        let _configured_asset_root = command.env("PLACES_ASSET_ROOT", root);
    }
    if let Some(root) = state_root {
        let _configured_state_root = command.env("PLACES_STATE_ROOT", root);
    }
    command.output().expect("places --list-levels runs")
}

/// Constructs the real install boundary without any generated repository maps.
fn stage_base(tag: &str) -> PathBuf {
    let stage =
        std::env::temp_dir().join(format!("places-list-levels-{tag}-{}", std::process::id()));
    remove_stage(&stage);
    for directory in ["assets/levels", "levels", "import", "bin"] {
        fs::create_dir_all(stage.join(directory)).expect("isolated layout directory");
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let _catalog_bytes = fs::copy(
        repo.join("assets/catalog.json"),
        stage.join("assets/catalog.json"),
    )
    .expect("stage the exact compiler catalog");
    let _binary_bytes = fs::copy(PLACES, stage.join("bin/places"))
        .expect("stage the Cargo-built player beside its payload");
    stage
}

/// Builds a small known-id package through the ordinary CPU compiler path.
fn compile_tiny_package(directory: &Path, id: &str, name: &str) -> PathBuf {
    fs::create_dir_all(directory).expect("fixture authoring directory");
    let source = directory.join(format!("{id}.json"));
    let json = serde_json::json!({
        "format_version": 3_u32,
        "id": id,
        "name": name,
        "spawn": {"x": 1.0_f32, "z": 1.0_f32},
        "defaults": {"floor": "core:carpet_beige_01", "ceiling": "core:ceiling_panel_01"},
        "rooms": [{"x": 0.0_f32, "z": 0.0_f32, "width": 2.0_f32,
                   "depth": 2.0_f32, "height": 2.5_f32}],
        "ceiling_lights": [{"fixture": "core:fluorescent_panel_01",
                           "x": 1.0_f32, "z": 1.0_f32}]
    });
    fs::write(
        &source,
        serde_json::to_vec(&json).expect("fixture source JSON"),
    )
    .expect("fixture source");
    let out = source.with_extension("placesmap");
    let request = places::compiler::BuildRequest {
        source,
        out: out.clone(),
        asset_root: Path::new(env!("CARGO_MANIFEST_DIR")).join("assets"),
        variants: vec![places::quality::LightmapQuality::Off],
        workers: 1,
        force: true,
        capture_probes: false,
    };
    drop(places::compiler::build(&request).expect("isolated package compiles without GPU work"));
    out
}

fn stage_fixture_install(tag: &str) -> PathBuf {
    let stage = stage_base(tag);
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let assets = stage.join("assets");
    for (id, name, directory) in [
        ("places_demo", "Places Demo", "assets/levels"),
        ("model_zoo", "Model Zoo", "assets/levels"),
        ("community_room", "Community Room", "levels"),
    ] {
        let package = compile_tiny_package(&stage.join("authoring"), id, name);
        stage_package(
            repo,
            &package,
            &stage.join(directory).join(format!("{id}.placesmap")),
            &assets,
        );
    }
    fs::write(stage.join("assets/levels/places_demo.json"), "{}")
        .expect("source beside a bundled package");
    fs::write(stage.join("levels/home_showcase.json"), "{}").expect("uncompiled installed source");
    stage
}

fn assert_fixture_rows(rows: &[(String, String, String)]) {
    let mut identities: Vec<_> = rows
        .iter()
        .map(|(id, source, _)| (id.as_str(), source.as_str()))
        .collect();
    identities.sort_unstable();
    assert_eq!(
        identities,
        vec![
            ("community_room", "installed"),
            ("model_zoo", "bundled"),
            ("places_demo", "bundled")
        ],
        "only the three generated package fixtures are playable rows"
    );
}

/// Asserts a successful, quiet run whose stdout holds only playable rows.
fn playable_rows(output: &Output) -> Vec<(String, String, String)> {
    assert!(
        output.status.success(),
        "discovery exits 0: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.is_empty(),
        "discovery is silent on the normal layout: {stderr}"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains(".json"),
        "authoring sources are never rows: {stdout}"
    );
    stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.split('\t');
            let id = fields.next().unwrap_or_default().to_string();
            let source = fields.next().unwrap_or_default().to_string();
            let path = fields.next().unwrap_or_default().to_string();
            assert!(
                fields.next().is_none(),
                "a row has exactly three tab-separated fields: {line}"
            );
            (id, source, path)
        })
        .collect()
}

/// Compares package identity across launch layouts without requiring the CLI
/// to print relative and absolute paths with the same spelling.
fn canonical_package_rows(
    rows: Vec<(String, String, String)>,
    cwd: &Path,
) -> Vec<(String, String, PathBuf)> {
    rows.into_iter()
        .map(|(id, source, path)| {
            assert!(
                path.ends_with(".placesmap"),
                "{id} is a compiled package row ({source} {path})"
            );
            let package = cwd.join(&path).canonicalize().unwrap_or_else(|error| {
                panic!(
                    "cannot resolve {id} package {path} from {}: {error}",
                    cwd.display()
                )
            });
            (id, source, package)
        })
        .collect()
}

#[test]
fn package_path_normalization_preserves_exact_file_identity() {
    let stage =
        std::env::temp_dir().join(format!("places-list-levels-path-{}", std::process::id()));
    remove_stage(&stage);
    let first = stage.join("first");
    let second = stage.join("second");
    for directory in [&first, &second] {
        fs::create_dir_all(directory).expect("package path fixture directory");
        fs::write(directory.join("same.placesmap"), []).expect("package path fixture");
    }
    let rows = |path: &str| {
        vec![(
            "same".to_string(),
            "installed".to_string(),
            path.to_string(),
        )]
    };
    let relative = canonical_package_rows(rows("same.placesmap"), &first);
    let absolute = canonical_package_rows(
        rows(first.join("same.placesmap").to_str().expect("fixture path")),
        &second,
    );
    assert_eq!(relative, absolute, "absolute paths ignore the launch cwd");
    assert_ne!(
        relative,
        canonical_package_rows(rows("same.placesmap"), &second),
        "equal filenames in different directories are different packages"
    );
    remove_stage(&stage);
}

/// Repository launch discovery is isolated from optional generated content.
#[test]
fn repository_layout_lists_only_packages() {
    let stage = stage_fixture_install("repository");
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let rows = playable_rows(&list_levels(
        Path::new(PLACES),
        repo,
        Some(&stage),
        Some(&stage),
    ));
    assert_fixture_rows(&rows);
    let canonical_stage = stage.canonicalize().expect("isolated repository root");
    for (_, _, package) in canonical_package_rows(rows, repo) {
        assert!(
            package.starts_with(&canonical_stage),
            "repository discovery must use the isolated package root: {}",
            package.display()
        );
    }
    remove_stage(&stage);
}

/// A copied executable resolves its own ancestor payload from any launch cwd.
#[test]
fn another_working_directory_lists_the_same_packages() {
    let stage = stage_fixture_install("cwd");
    let binary = stage.join("bin/places");
    let elsewhere = stage.join("unrelated");
    fs::create_dir_all(&elsewhere).expect("unrelated launch directory");
    let from_install = playable_rows(&list_levels(&binary, &stage, None, None));
    assert_fixture_rows(&from_install);
    let from_elsewhere = playable_rows(&list_levels(&binary, &elsewhere, None, None));
    assert_fixture_rows(&from_elsewhere);
    assert_eq!(
        canonical_package_rows(from_install, &stage),
        canonical_package_rows(from_elsewhere, &elsewhere),
        "the executable's own payload resolves to the same exact package files"
    );
    remove_stage(&stage);
}

/// Explicit payload and state overrides keep bundled and installed rows distinct.
#[test]
fn packaged_layout_lists_only_packages() {
    let stage = stage_fixture_install("explicit");
    let user_state_root = stage.join("state");
    fs::create_dir_all(user_state_root.join("levels")).expect("explicit installed directory");
    let _copied_package = fs::copy(
        stage.join("levels/community_room.placesmap"),
        user_state_root.join("levels/community_room.placesmap"),
    )
    .expect("install a package into explicit state");
    fs::write(user_state_root.join("levels/home_showcase.json"), "{}")
        .expect("uncompiled source in explicit state");
    let rows = playable_rows(&list_levels(
        &stage.join("bin/places"),
        Path::new(env!("CARGO_MANIFEST_DIR")),
        Some(&stage),
        Some(&user_state_root),
    ));
    assert_fixture_rows(&rows);
    assert!(
        rows.iter().filter(|row| row.0 == "places_demo").count() == 1,
        "the installed Demo is offered exactly once"
    );
    remove_stage(&stage);
}

/// Raw sources alone are quiet and cannot become phantom playable rows.
#[test]
fn source_only_layout_lists_no_packages() {
    let stage = stage_base("sources-only");
    fs::write(stage.join("assets/levels/places_demo.json"), "{}")
        .expect("uncompiled bundled source");
    fs::write(stage.join("levels/community_room.json"), "{}").expect("uncompiled installed source");
    let rows = playable_rows(&list_levels(&stage.join("bin/places"), &stage, None, None));
    assert!(
        rows.is_empty(),
        "uncompiled sources are never playable menu entries"
    );
    remove_stage(&stage);
}
