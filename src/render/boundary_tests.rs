//! Stage 3/4 boundary guards.
//!
//! Three cheap kinds of check:
//!
//! * **Source scans** that fail if a GL or wgpu type leaks out of its backend
//!   module, if the renderer-neutral layer grows a dependency on a backend, or
//!   if an engine module reaches into a backend. These are deliberately simple
//!   string scans over the repository sources; they are the repository-level
//!   statement of the ownership rules in `docs/RENDERER_BOUNDARY.md`.
//! * **Preparation tests** for the renderer-neutral `PreparedFrame`: the
//!   frame-level decisions (target size, nearest probe, planar plane) are
//!   exercised without a GL context.
//!
//! The facade (`src/render/facade.rs`) is the one place outside a backend
//! module that dispatches to both; the selector (`src/render/backend.rs`) names
//! neither API.

// Test code: unwrap/expect, indexing and permissive arithmetic are idiomatic.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use std::path::{Path, PathBuf};

use super::common::camera::RenderCamera;
use super::common::frame::{FrameState, PreparedFrame};
use super::common::reflections::{ReflectionPlane, Reflections};
use super::common::view::DrawableSize;
use crate::quality::QualityProfile;
use crate::spatial::Aabb;

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

/// True for files allowed to name the OpenGL API: the backend itself, the
/// in-crate test suite and these guards.
fn may_name_gl(rel: &str) -> bool {
    rel.starts_with("src/render/opengl/")
        || rel == "src/render/tests.rs"
        || rel == "src/render/boundary_tests.rs"
}

/// True for files allowed to name the wgpu API: the backend itself, the facade
/// that dispatches to both backends, the selector's module root and the
/// in-crate test suites.
fn may_name_wgpu(rel: &str) -> bool {
    rel.starts_with("src/render/wgpu/")
        || rel == "src/render/facade.rs"
        || rel == "src/render.rs"
        || rel == "src/render/tests.rs"
        || rel == "src/render/boundary_tests.rs"
}

#[test]
fn only_the_opengl_backend_names_glow() {
    let mut offenders = Vec::new();
    for (rel, text) in all_sources() {
        if may_name_gl(&rel) {
            continue;
        }
        if text.contains("glow::")
            || text.contains("use glow")
            || text.contains("extern crate glow")
        {
            offenders.push(rel);
        }
    }
    assert!(
        offenders.is_empty(),
        "GL types must stay inside render::opengl; found in: {offenders:?}"
    );
}

#[test]
fn the_engine_bootstrap_owns_no_gl_calls() {
    // The window/context calls main used to make (`gl_swap_window`,
    // `gl_attr`, `gl_create_context`, `gl_set_swap_interval`, …) are behind
    // `render::opengl::context` and reached through the `render` facade.
    const GL_CALLS: [&str; 7] = [
        "gl_swap_window(",
        "gl_create_context(",
        ".gl_attr(",
        "gl_set_swap_interval(",
        "gl_get_swap_interval(",
        "gl_make_current(",
        ".opengl()",
    ];
    let mut offenders = Vec::new();
    for (rel, text) in all_sources() {
        if may_name_gl(&rel) {
            continue;
        }
        if GL_CALLS.iter().any(|call| text.contains(call)) {
            offenders.push(rel);
        }
    }
    assert!(
        offenders.is_empty(),
        "GL calls must stay inside the OpenGL backend; found in: {offenders:?}"
    );
}

#[test]
fn engine_modules_do_not_import_the_opengl_backend() {
    let mut offenders = Vec::new();
    for (rel, text) in all_sources() {
        if may_name_gl(&rel) || rel == "src/render.rs" {
            continue;
        }
        // Doc comments may *describe* the backend; code may not reach into it.
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if trimmed.contains("crate::render::opengl") || trimmed.contains("render::opengl::") {
                offenders.push(format!("{rel}: {trimmed}"));
                break;
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "the OpenGL backend is reached only through render::Renderer; found in: {offenders:?}"
    );
}

#[test]
fn the_neutral_layer_does_not_depend_on_the_backend() {
    let mut offenders = Vec::new();
    for (rel, text) in all_sources() {
        if !rel.starts_with("src/render/common/") {
            continue;
        }
        if text.contains("glow::") || text.contains("wgpu::") {
            offenders.push(format!("{rel} (backend type)"));
        }
        for line in text.lines() {
            let line = line.trim_start();
            if line.starts_with("use ")
                && (line.contains("opengl") || line.contains("wgpu") || line.contains("glow"))
                && !line.contains("//")
            {
                offenders.push(format!("{rel}: {line}"));
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
        if may_name_gl(&rel) || may_name_wgpu(&rel) {
            continue;
        }
        // Doc comments may *describe* a backend; code may not reach into one.
        for line in text.lines() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            if trimmed.contains("crate::render::opengl")
                || trimmed.contains("render::opengl::")
                || trimmed.contains("crate::render::wgpu")
                || trimmed.contains("render::wgpu::")
            {
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

/// A neutral level state with one mirror plane in front of the camera.
fn plane_in_front() -> Reflections {
    let mut reflections = Reflections::default();
    reflections.routing.planes.push(ReflectionPlane {
        normal: [0.0, 1.0, 0.0],
        offset: 1.5,
        bounds: Aabb {
            min: [-4.0, -1.5, -4.0],
            max: [4.0, -1.5, 4.0],
        },
    });
    reflections
}

#[test]
fn prepared_frame_is_none_for_an_empty_drawable() {
    let camera = RenderCamera::new(glam::Vec3::ZERO, 0.0, 0.0, 60.0);
    let reflections = Reflections::default();
    let frame = PreparedFrame::plan(&FrameState {
        camera,
        drawable: DrawableSize::new(0, 0),
        quality: QualityProfile::Full,
        offscreen_enabled: true,
        offscreen_failed: false,
        culling: true,
        reflections: &reflections,
        probe_positions: &[],
    });
    assert!(frame.is_none(), "a zero drawable has nothing to render");
}

#[test]
fn prepared_frame_picks_the_nearest_probe_and_the_low_cap() {
    let camera = RenderCamera::new(glam::Vec3::ZERO, 0.0, 0.0, 60.0);
    let reflections = Reflections::default();
    let probes = [[40.0, 1.0, 0.0], [3.0, 1.0, 0.0]];
    let mut frame = PreparedFrame::plan(&FrameState {
        camera,
        drawable: DrawableSize::new(1920, 1080),
        quality: QualityProfile::Low,
        offscreen_enabled: true,
        offscreen_failed: false,
        culling: true,
        reflections: &reflections,
        probe_positions: &probes,
    })
    .expect("a drawable plans a frame");

    // The nearest probe wins; the second is nearer the origin.
    assert_eq!(frame.probe, Some(1));
    assert_eq!(frame.target_size, Some(DrawableSize::new(480, 270)));
    frame.begin_scene(true, &reflections.routing);
    assert_eq!(frame.render_size, DrawableSize::new(480, 270));
    assert!(frame.offscreen);
}

#[test]
fn prepared_frame_selects_the_visible_mirror_plane() {
    let camera = RenderCamera::new(glam::Vec3::ZERO, 0.0, 0.0, 60.0);
    let reflections = plane_in_front();
    let mut frame = PreparedFrame::plan(&FrameState {
        camera,
        drawable: DrawableSize::new(1280, 720),
        quality: QualityProfile::Full,
        offscreen_enabled: true,
        offscreen_failed: false,
        culling: true,
        reflections: &reflections,
        probe_positions: &[],
    })
    .expect("a drawable plans a frame");
    assert!(
        frame.planar_plane.is_none(),
        "selection happens in begin_scene"
    );
    frame.begin_scene(true, &reflections.routing);
    assert_eq!(
        frame.planar_plane,
        Some(0),
        "the only plane is in front of the camera and must be reflected"
    );
}

#[test]
fn prepared_frame_never_reflects_with_reflections_switched_off() {
    let camera = RenderCamera::new(glam::Vec3::ZERO, 0.0, 0.0, 60.0);
    let mut reflections = plane_in_front();
    reflections.set_enabled(false);
    let mut frame = PreparedFrame::plan(&FrameState {
        camera,
        drawable: DrawableSize::new(1280, 720),
        quality: QualityProfile::Full,
        offscreen_enabled: true,
        offscreen_failed: false,
        culling: true,
        reflections: &reflections,
        probe_positions: &[[1.0, 1.0, 1.0]],
    })
    .expect("a drawable plans a frame");
    frame.begin_scene(true, &reflections.routing);
    assert!(
        frame.probe.is_none(),
        "disabled reflections sample no probe"
    );
    assert!(
        frame.planar_plane.is_none(),
        "disabled reflections draw no plane"
    );
}
