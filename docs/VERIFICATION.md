# Desktop verification

This is the authoritative gate for the current Places desktop project: the
wgpu renderer (Metal on macOS, Vulkan on Linux, Direct3D 12 on Windows). The
removed OpenGL/GLES2 reference renderer and the complete Stage 10 dual-renderer
state are preserved at the `renderer-gles2-reference` tag; see
[RENDERER_REFERENCE.md](RENDERER_REFERENCE.md) and
[WGPU_STAGE10.md](WGPU_STAGE10.md). Run from the repository root on a desktop
session with Rust 1.91 or newer, SDL2 and pkg-config, Python 3, and Node.js
available. macOS setup is
`brew install sdl2 pkg-config`. Python tooling uses the standard library. No npm
install is required. No CI configuration is currently tracked.

```sh
sh tools/verify.sh
```

The script stops at the first failure and runs these commands in order:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo
cargo test --workspace --all-features
python3 tools/assets/validate.py
python3 tools/textures/build.py --check
python3 tools/props/build.py --check
python3 -m unittest tests.test_package
(cd level-editor && npm test)
cargo build --release
python3 -m unittest tests.test_compiled_build
python3 -m unittest tests.test_wgpu_bootstrap
git diff --check
```

All commands must exit zero. Compiled-build tests open real SDL windows and
GPU surfaces; a skipped suite is not a completed desktop gate. Rust's three intentionally
ignored diagnostics are opt-in reports, not required tests. Texture checking
currently emits 35 soft-budget warnings for artwork above 256 pixels; these
are accepted shipped source sizes within the 1024-pixel hard limit, documented
in ASSET_SPECIFICATION.md. The package suite intentionally exercises an invalid catalog and prints
`FAIL core:couch: duplicate logical asset id` / `1 error(s), 0 warning(s)`
after its successful unittest summary. This is the negative fixture in
`test_a_broken_catalog_surfaces_in_the_validators_exit_code`, not a shipped
asset failure. Editor tests also print `WebGL is unavailable` while testing
the fallback with a mocked browser. The wgpu bootstrap suite opens a real SDL
window on the native backend (Metal on macOS, Vulkan on Linux, Direct3D 12 on
Windows) and fails if the adapter reports any other backend. Require the final
test summaries to show zero failures. Other warnings require investigation.
`clippy.toml` permits only the four unavoidable transitive duplicate crates:
SDL2/PNG require different bitflags versions, PNG/flate2 require different
miniz_oxide versions, the wgpu proc-macro tree uses syn 3 while serde/thiserror
use syn 2, and wgpu-hal's hashbrown 0.17 cannot be unified with the 0.16 the
rest of the tree selects. No general lint group is suppressed.

The catalog validator checks shipped and fixture levels. Rust and package tests
cover model budgets, texture seams, schema, geometry, collision and lighting;
prop `--check` alone only verifies GLB parsing and existence.

## Generation checks

When changing generators, first read ASSET_SPECIFICATION.md and, for levels,
MAP_AUTHORING_GUIDE.md. Run the existing non-forced generation paths:

```sh
python3 tools/textures/build.py
python3 tools/props/build.py
python3 tools/levels/build_fixture_levels.py
```

Run generators to completion before tests or runtime captures: they rewrite
files in place. Review generated changes; a second run must leave those outputs
identical.
The texture generator skips shipped images whose dimensions differ from its
placeholder painter. Never use `--force` as a validation step. Use
`python3 tools/props/generate_spooner_man.py` for the documented entity-only
workflow, and `python3 tools/props/build.py --thumbs` when intentionally updating
editor thumbnails. PNG artwork is loaded from committed assets at runtime.

## Runtime and visual gate

Launch the demo with `PLACES_LEVEL=places_demo cargo run --release`. The wgpu
renderer draws the whole reference feature set — baked lightmaps, reflections,
props, fixtures, emission, decals, fog, post-processing and the HUD; see
WGPU_STAGE9.md and WGPU_STAGE10.md. For a repeatable visual check, capture the
canonical 25 views in both quality profiles without overwriting the frozen
images:

```sh
PLACES_CAPTURE_DIR="$PWD/target/verification-captures" \
    sh tools/bench/capture_baseline_views.sh
python3 tools/bench/check_holes.py target/verification-captures/high/*.png
```

The canonical setup is 640x360 logical pixels and requires the same 2x display
backing scale as the tracked 1280x720 images. `docs/renderer-baseline/` is the
frozen **OpenGL** reference set: compare a wgpu capture against it per view with
`compare_captures.py`, where the numbers to expect are the bounded Stage 10
residuals (WGPU_STAGE10.md §3/§4/§12), not byte equality. Byte equality applies
only when comparing two wgpu runs (for example a pre-change and post-change
build).

The historical OpenGL reference can be captured from the preservation worktree
(`git worktree add <dir> renderer-gles2-reference`); against that checkout's own
committed assets, `compare_baseline.py` must still report 50/50 byte-identical
images. `tools/bench/capture_expanded_views.sh` adds the Stage 10 supplementary
view set (geometry junctions, materials, lightmaps, reflections, props, decals,
fog, UI).

The wgpu gate also includes `python3 -m unittest tests.test_wgpu_bootstrap`,
which asserts the native backend (Metal on macOS, Vulkan on Linux, Direct3D 12
on Windows), a boot level whose world uploads non-empty geometry, its texture
resolution, its material resolution, the Full/Low response gate, a real
in-process level replacement through `PLACES_LEVEL`, fallback and diagnostic
levels, both quality profiles, a bounded frame loop and a clean exit.

Live window lifecycle can be scripted with the benchmark-only switch
`PLACES_BENCH_WINDOW_CYCLE=<frame>:resize:<w>x<h>|minimize|restore[,...]`
together with `PLACES_BENCH_QUALITY_CYCLE`; see WGPU_STAGE10.md §6/§9.

`tools/bench/README.md` documents additional performance, lightmap and visual
comparison diagnostics. Generated captures and reports belong under `target/`.
