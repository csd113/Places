//! Quiet-startup regression for level discovery.
//!
//! `places --list-levels` runs the exact discovery pass the Level Select menu
//! builds, then exits before any window or GPU work, so the three supported
//! launch layouts can be checked from a test:
//!
//! * the repository layout, launched from the repository root;
//! * the same binary launched from another working directory, where the asset
//!   root must still resolve through the executable's ancestors;
//! * a staged distribution layout, where `PLACES_ASSET_ROOT` and
//!   `PLACES_STATE_ROOT` point at the packaged `assets/levels` and the writable
//!   drop-in `levels/` directory.
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
use std::path::Path;
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

/// The binary under test, built by cargo for this integration test target.
const PLACES: &str = env!("CARGO_BIN_EXE_places");

/// Runs `places --list-levels` with the given working directory and optional
/// `PLACES_ASSET_ROOT` / `PLACES_STATE_ROOT` overrides.
fn list_levels(cwd: &Path, asset_root: Option<&Path>, state_root: Option<&Path>) -> Output {
    let mut command = Command::new(PLACES);
    let _configured_command = command.arg("--list-levels").current_dir(cwd);
    if let Some(root) = asset_root {
        let _configured_asset_root = command.env("PLACES_ASSET_ROOT", root);
    }
    if let Some(root) = state_root {
        let _configured_state_root = command.env("PLACES_STATE_ROOT", root);
    }
    command.output().expect("places --list-levels runs")
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

/// The repository layout: the bundled packages are found, the sibling `.json`
/// sources are not rows, and nothing is printed to stderr.
#[test]
fn repository_layout_lists_only_packages() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let rows = playable_rows(&list_levels(repo, None, None));
    let ids: Vec<&str> = rows.iter().map(|row| row.0.as_str()).collect();
    assert!(ids.contains(&"places_demo"), "the demo is listed: {ids:?}");
    assert!(ids.contains(&"model_zoo"), "the zoo is listed: {ids:?}");
    for (id, source, path) in &rows {
        assert!(
            path.ends_with(".placesmap") || source == "embedded",
            "{id} is a package row ({source} {path})"
        );
    }
}

/// Another working directory: discovery resolves the asset root through the
/// executable's ancestors, and startup stays just as quiet.
#[test]
fn another_working_directory_lists_the_same_packages() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let elsewhere =
        std::env::temp_dir().join(format!("places-list-levels-cwd-{}", std::process::id()));
    fs::create_dir_all(&elsewhere).expect("temporary working directory");
    let from_repo = playable_rows(&list_levels(repo, None, None));
    let from_elsewhere = playable_rows(&list_levels(&elsewhere, None, None));
    assert_eq!(
        from_repo, from_elsewhere,
        "the executable's own asset root is independent of the working directory"
    );
}

/// The staged distribution layout: `assets/levels` holds packaged levels only
/// and `levels/` holds dropped-in packages; a stray authoring source in either
/// directory is skipped without a line of output.
#[test]
fn packaged_layout_lists_only_packages() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
    let stage =
        std::env::temp_dir().join(format!("places-list-levels-stage-{}", std::process::id()));
    remove_stage(&stage);
    let assets = stage.join("assets/levels");
    let state_root = stage.join("state");
    let levels = state_root.join("levels");
    let import = state_root.join("import");
    fs::create_dir_all(&assets).expect("staged assets");
    fs::create_dir_all(&levels).expect("staged drop-in levels");
    fs::create_dir_all(&import).expect("staged import");

    let bundled = [
        repo.join("assets/levels/places_demo.placesmap"),
        repo.join("assets/levels/model_zoo.placesmap"),
    ];
    for source in bundled {
        let name = source.file_name().expect("package name");
        let _copied_bytes = fs::copy(&source, assets.join(name)).expect("stage the package");
    }
    // A source beside its package is expected content, not a warning.
    fs::write(assets.join("places_demo.json"), "{}").expect("stage a source");
    // A dropped-in package in the writable directory is a row.
    let drop_in = repo.join("levels/geometry_intentional.placesmap");
    if drop_in.exists() {
        let _copied_bytes = fs::copy(&drop_in, levels.join("geometry_intentional.placesmap"))
            .expect("stage the drop-in package");
    }
    fs::write(levels.join("home_showcase.json"), "{}").expect("stage a drop-in source");

    let rows = playable_rows(&list_levels(
        repo,
        Some(&stage.join("assets")),
        Some(&state_root),
    ));
    let ids: Vec<&str> = rows.iter().map(|row| row.0.as_str()).collect();
    assert!(ids.contains(&"places_demo"), "the demo is listed: {ids:?}");
    assert!(ids.contains(&"model_zoo"), "the zoo is listed: {ids:?}");
    let demo = rows
        .iter()
        .find(|row| row.0 == "places_demo")
        .expect("demo row");
    assert_eq!(
        demo.1, "bundled",
        "the staged package is the bundled source"
    );
    assert!(
        rows.iter().filter(|row| row.0 == "places_demo").count() == 1,
        "the embedded fallback is not duplicated when a package is installed"
    );

    remove_stage(&stage);
}
