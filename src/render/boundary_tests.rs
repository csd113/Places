//! Renderer boundary guards.
//!
//! The checks are deliberately simple string scans over the repository
//! sources: they fail if a wgpu type leaks out of its backend module, if the
//! renderer-neutral layer grows a dependency on the backend, or if an engine
//! module reaches into the backend. They are the repository-level statement of
//! the ownership rules in `docs/ARCHITECTURE.md`, section 3.
//!
//! The facade (`src/render/facade.rs`) is the one place outside the backend
//! module that names it.

// Test code: unwrap/expect, indexing and permissive arithmetic are idiomatic.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used,
    reason = "Regression fixtures assert exact reference results and fail on invalid setup; these exceptions are confined to tests"
)]

use std::path::{Path, PathBuf};

/// Repository root, from the compile-time manifest directory.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every `.rs` file under `dir`, recursively.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// The source of every `.rs` file in the repository, with its relative path.
fn all_sources() -> Vec<(String, String)> {
    let root = repo_root();
    let mut files = Vec::new();
    rust_files(&root.join("src"), &mut files);
    files
        .into_iter()
        .map(|file| {
            let rel = file
                .strip_prefix(&root)
                .unwrap_or(&file)
                .to_string_lossy()
                .replace('\\', "/");
            let text = std::fs::read_to_string(&file).unwrap_or_default();
            (rel, text)
        })
        .collect()
}

/// True for files allowed to name the wgpu API: the backend itself, the
/// facade that owns it, the module root and the in-crate test suites.
fn may_name_wgpu(rel: &str) -> bool {
    rel.starts_with("src/render/wgpu/")
        || rel == "src/render/facade.rs"
        || rel == "src/render.rs"
        || rel == "src/render/tests.rs"
        || rel == "src/render/boundary_tests.rs"
}

#[test]
fn the_engine_bootstrap_owns_no_platform_gpu_calls() {
    // The window/context calls the engine must never make (`gl_swap_window`,
    // `gl_attr`, `gl_create_context`, `.opengl()`, …) and backend-specific
    // window flags (`metal_view`) stay behind `render`; main builds a plain
    // window and the facade owns the surface.
    const GPU_WINDOW_CALLS: [&str; 8] = [
        "gl_swap_window(",
        "gl_create_context(",
        ".gl_attr(",
        "gl_set_swap_interval(",
        "gl_get_swap_interval(",
        "gl_make_current(",
        ".opengl()",
        ".metal_view()",
    ];
    let mut offenders = Vec::new();
    for (rel, text) in all_sources() {
        if rel.starts_with("src/render/") {
            continue;
        }
        if GPU_WINDOW_CALLS.iter().any(|call| text.contains(call)) {
            offenders.push(rel);
        }
    }
    assert!(
        offenders.is_empty(),
        "GPU window calls must stay inside render; found in: {offenders:?}"
    );
}

#[test]
fn the_neutral_layer_does_not_depend_on_the_backend() {
    let mut offenders = Vec::new();
    for (rel, text) in all_sources() {
        if !rel.starts_with("src/render/common/") {
            continue;
        }
        if text.contains("wgpu::") {
            offenders.push(format!("{rel} (backend type)"));
        }
        for line in text.lines() {
            let trimmed_line = line.trim_start();
            if trimmed_line.starts_with("use ")
                && trimmed_line.contains("wgpu")
                && !trimmed_line.contains("//")
            {
                offenders.push(format!("{rel}: {trimmed_line}"));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "renderer-neutral code must not depend on a backend: {offenders:?}"
    );
}

#[test]
fn only_the_wgpu_backend_and_facade_name_wgpu() {
    let mut offenders = Vec::new();
    for (rel, text) in all_sources() {
        if may_name_wgpu(&rel) {
            continue;
        }
        if text.contains("wgpu::")
            || text.contains("use wgpu")
            || text.contains("extern crate wgpu")
        {
            offenders.push(rel);
        }
    }
    assert!(
        offenders.is_empty(),
        "wgpu types must stay inside render::wgpu and its facade; found in: {offenders:?}"
    );
}

#[test]
fn engine_modules_do_not_import_a_renderer_backend() {
    let mut offenders = Vec::new();
    for (rel, text) in all_sources() {
        if may_name_wgpu(&rel) {
            continue;
        }
        // Doc comments may *describe* a backend; code may not reach into one.
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if trimmed.contains("crate::render::wgpu") || trimmed.contains("render::wgpu::") {
                offenders.push(format!("{rel}: {trimmed}"));
                break;
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "backends are reached only through render::Renderer; found in: {offenders:?}"
    );
}

/// The player never performs static preparation.
///
/// The compiler (`src/compiler.rs`, the `places-compile` binary) owns geometry
/// emission, the lighting bake, chart planning, atlas fill, probe capture and
/// collision derivation. The player's files must not name any of those
/// operations, and must not reach into the compiler module at all: a player
/// process cannot compile a map, even by accident or through a future
/// refactor. Tests and audits may still exercise the preparation code directly.
#[test]
fn the_player_never_invokes_static_preparation() {
    const PLAYER_FILES: [&str; 5] = [
        "src/lib.rs",
        "src/loading.rs",
        "src/loader.rs",
        "src/package/world.rs",
        "src/main.rs",
    ];
    const FORBIDDEN: [&str; 14] = [
        "bake_reflection_probes",
        "capture_reflection_probes",
        "reprepare_reflection_probes",
        "fill_lightmaps",
        "prepare_level_geometry_with_lightmaps",
        "build_level_geometry",
        "LevelLighting::bake",
        "crate::compiler",
        "compiler::build",
        "prepare_level",
        "rebuild_vertex_lit_level",
        "set_fill_workers",
        "dump_lightmaps_for_level",
        "bake_with",
    ];
    let mut offenders = Vec::new();
    for (rel, text) in all_sources() {
        if !PLAYER_FILES.contains(&rel.as_str()) {
            continue;
        }
        for line in text.lines() {
            let trimmed = line.trim_start();
            // Comments and the shared helper's own definition (the compiler
            // calls it in-crate) are not player calls.
            if trimmed.starts_with("//") || is_fn_definition(trimmed) {
                continue;
            }
            for needle in FORBIDDEN {
                if line.contains(needle) {
                    offenders.push(format!("{rel}: {trimmed}"));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "the player must decode compiled packages, never prepare them; found: {offenders:?}"
    );
}

/// True for a line that declares a function rather than calling one.
fn is_fn_definition(trimmed: &str) -> bool {
    [
        "fn ",
        "pub fn ",
        "pub(crate) fn ",
        "pub(super) fn ",
        "const fn ",
    ]
    .iter()
    .any(|prefix| trimmed.starts_with(prefix))
}
