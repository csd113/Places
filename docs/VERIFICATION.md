# Desktop verification

This is the authoritative gate for Places: the wgpu renderer (Metal on macOS,
Vulkan on Linux, Direct3D 12 on Windows) behind the SDL3 platform layer. Run
from the repository root on a desktop session. The release's platform status is
recorded in the matrix in §7.

## 1. Prerequisites

- Rust 1.91 or newer (the project is verified on 1.98.x).
- SDL3 3.2 or newer and `pkg-config`.
- Python 3 (standard library only; no `pip install`).
- Node.js for the level-editor tests (no `npm install` needed; the editor's
  tests run from their committed lockfile).
- A working native GPU driver for the platform's backend.

Per platform:

| Platform | Setup |
|---|---|
| macOS | `brew install sdl3 pkg-config` (the verified host) |
| Linux | install SDL3 development headers and `pkg-config` (Debian/Ubuntu: `libsdl3-dev`; Fedora: `SDL3-devel`; Arch: `sdl3` + `pkgconf`) plus the Vulkan loader and a driver (`libvulkan1`/`vulkan-icd-loader`, `mesa-vulkan-drivers`) |
| Windows | install SDL3 (for example `vcpkg install sdl3`, or a prebuilt SDL3 development package) and a C/C++ toolchain (MSVC Build Tools with the C++ workload, or MinGW-w64); the backend is Direct3D 12 |

No CI configuration is currently tracked; the gate is run manually.

## 2. The gate

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

All commands must exit zero. Compiled-build tests open real SDL3 windows and
GPU surfaces; a skipped suite is not a completed desktop gate. Require the final
test summaries to show zero failures; other warnings require investigation.

Alongside the gate, the GPU diagnostics run explicitly (they are ignored by
default because they need an adapter or write measurement files):

```sh
cargo test --all-features --bin places -- --ignored
```

The five intentionally ignored diagnostics are: the reflection cube round-trip
orientation test and the sRGB sample round-trip measurement (both need a GPU
adapter), the lighting parity-vector regeneration, the stair-trace CSV
developer diagnostic, and the lightmap chart-statistics measurement. They are
opt-in reports, not required tests; a developer running them should expect
possible file writes under `target/`.

Expected, understood output noise:

- Texture checking currently emits 39 soft-budget warnings for artwork above
  256 pixels; these are accepted shipped source sizes within the 1024-pixel
  hard limit, documented in [ASSET_SPECIFICATION.md](ASSET_SPECIFICATION.md).
- The package suite intentionally exercises an invalid catalog and prints
  `FAIL core:couch: duplicate logical asset id` / `1 error(s), 0 warning(s)`
  after its successful unittest summary. This is the negative fixture in
  `test_a_broken_catalog_surfaces_in_the_validators_exit_code`, not a shipped
  asset failure.
- Editor tests print `WebGL is unavailable` while testing the fallback with a
  mocked browser.
- The wgpu bootstrap suite opens a real SDL3 window on the native backend
  (Metal on macOS, Vulkan on Linux, Direct3D 12 on Windows) and fails if the
  adapter reports any other backend.
- `clippy.toml` permits only the three unavoidable transitive duplicate
  crates: PNG/flate2 require different miniz_oxide versions, zerocopy-derive
  (via half and naga) uses syn 2 while bytemuck_derive, serde_derive and
  thiserror-impl use syn 3, and wgpu-hal's hashbrown 0.17 cannot be unified
  with the 0.16 that gpu-allocator selects on the Vulkan/D3D12 targets. No
  general lint group is suppressed.

The catalog validator checks shipped and fixture levels. Rust and package tests
cover model budgets, texture seams, schema, geometry, collision and lighting;
prop `--check` alone only verifies GLB parsing and existence.

## 3. Generation checks

When changing generators, first read
[ASSET_SPECIFICATION.md](ASSET_SPECIFICATION.md) and, for levels,
[MAP_AUTHORING_GUIDE.md](MAP_AUTHORING_GUIDE.md). Run the existing non-forced
generation paths:

```sh
python3 tools/textures/build.py
python3 tools/props/build.py
python3 tools/levels/build_fixture_levels.py
```

Run generators to completion before tests or runtime captures: they rewrite
files in place. Review generated changes; a second run must leave those outputs
identical. The texture generator skips shipped images whose dimensions differ
from its placeholder painter. Never use `--force` as a validation step. Use
`python3 tools/props/generate_spooner_man.py` for the documented entity-only
workflow, and `python3 tools/props/build.py --thumbs` when intentionally
updating editor thumbnails. PNG artwork is loaded from committed assets at
runtime.

## 4. Runtime and visual gate

Launch the demo with `PLACES_LEVEL=places_demo cargo run --release`. The
renderer draws the complete feature set — baked lightmaps, reflections, props,
fixtures, emission, decals, fog, post-processing and the HUD; see
[RENDERER.md](RENDERER.md).

For a repeatable visual check, capture the canonical 25 views in both quality
profiles:

```sh
PLACES_CAPTURE_DIR="$PWD/target/verification/canonical" \
    sh tools/bench/capture_baseline_views.sh
python3 tools/bench/check_holes.py target/verification/canonical/high/*.png
```

**Always set `PLACES_CAPTURE_DIR`.** `capture_baseline_views.sh` defaults to
writing `docs/renderer-baseline/{high,low}`, and those PNGs and their
`manifest.txt` files are frozen historical evidence: they must not be
regenerated or overwritten. The same rule applies to
`capture_expanded_views.sh` (its default target is a `target/` directory, but
set the variable anyway).

The expanded campaign covers the feature-targeted views the canonical set
exercises only incidentally (geometry junctions, materials, lightmap regions,
reflections, props, decals, fog, height):

```sh
PLACES_CAPTURE_DIR="$PWD/target/verification/expanded" \
PLACES_BASELINE_STATE="$PWD/target/verification/state" \
    sh tools/bench/capture_expanded_views.sh
```

That is 25 views × 2 profiles canonically and 40 views × 2 profiles expanded.
The UI is exercised by the fixed validation set, which includes the pause-menu
views:

```sh
PLACES_CAPTURE_DIR="$PWD/target/verification/views" \
    sh tools/bench/capture_views.sh
```

The canonical setup is 640×360 logical pixels and the pinned session settings;
the drawable is 1280×720 at the same 2.0x backing scale the tracked images use.

### Comparing captures

- **Two current builds** (a pre-change and post-change pair, or a same-binary
  determinism check): capture both with the same script and require byte
  equality or use `tools/bench/compare_captures.py` for per-view numbers.
- **Against the frozen set** (`docs/renderer-baseline/`): that set is the
  historical reference captured from the former renderer. Compare per view with
  `compare_captures.py`; the expected numbers are the bounded residuals in
  [RENDERER.md](RENDERER.md) §14-§15, never byte equality. `compare_baseline.py`
  is the byte-equality gate for the frozen set against the preserved
  implementation's own checkout.
- **The preserved implementation** is reproducible from the tag worktree; see
  [RENDERER_REFERENCE.md](RENDERER_REFERENCE.md).

`tools/bench/README.md` documents the additional performance, lightmap and
visual comparison diagnostics. Generated captures and reports belong under
`target/`.

## 5. Window-lifecycle scripting

Live window lifetime and live quality switches can be scripted with
benchmark-only switches. `PLACES_BENCH_WINDOW_CYCLE` drives real window events
through the SDL window — `resize:<w>x<h>`, `minimize`, `restore` — and
`PLACES_BENCH_QUALITY_CYCLE` switches the quality level through the normal
rebuild path (`low` / `medium` / `high`; the legacy `full` name is High):

```sh
PLACES_BENCH=1 PLACES_BENCH_FRAMES=34 \
PLACES_BENCH_WINDOW_CYCLE=3:resize:800x450,6:resize:500x300,8:resize:800x450,12:minimize,16:restore,18:resize:640x360,24:resize:900x500,26:resize:640x360 \
PLACES_BENCH_QUALITY_CYCLE=20:low,22:high \
PLACES_LEVEL=places_demo target/release/places
```

A run is clean when it exits 0 with no validation, surface or device errors and
no panic, and the post-lifecycle capture matches the pre-lifecycle one.

## 6. Recorded platform evidence

The evidence below was recorded on the development host (macOS, Apple M2 Pro,
Metal backend) during the platform and renderer validation campaigns. It is
recorded evidence, not a claim about platforms that have not been run.

- Adapter and surface: Apple M2 Pro / Metal / surface format
  `Bgra8UnormSrgb` / present mode `Fifo` (and `Immediate` with VSync off) /
  alpha `Opaque` / depth `Depth32Float`.
- Platform-layer equivalence: the SDL3 platform layer was captured against the
  immediately preceding build of the same renderer and produced 50/50 canonical
  and 80/80 expanded byte-identical captures, plus byte-identical UI/menu,
  after-restore and fullscreen captures; the fixed size/adapter lines matched.
- Input: W/S/A/D displacement, arrow look at 90°/s horizontal and 60°/s
  vertical (measured 89.6/59.9), Escape pause/resume, W+S cancellation and no
  stuck key after release — all through real OS events, recorded during the
  platform-layer campaign on this host.
- Lifecycle: resize larger/smaller/rapid/repeated, minimize and restore,
  repeated level rebuilds, capture and shutdown; no validation, surface or
  device errors. The scripted High↔Low cycles across the quality-cycle and
  lifecycle-loop runs rebuild eight times with texture residency alternating
  exactly between High 188,743,640 B and Low 15,728,600 B with no growth, and
  a separate 900-frame bounded run completes clean.
- Release validation (final cleanup): the canonical 50-view, expanded 80-view
  and fixed-set 76-view campaigns (UI/menu, walkthrough, High and Low) were
  captured before and after the final cleanup and are byte-identical
  (`compare_captures.py` largest mean difference 0.000). The eight-run lifecycle
  campaign (normal and rapid resize, minimize/restore, fullscreen boot,
  repeated quality cycles, bounded 900-frame run, repeated lifecycle loop,
  quit-while-minimized) exits 0 in every run with no validation, surface or
  device errors; a level replacement clears the previous level's dynamic
  scene, which the bootstrap suite asserts.
- Test gate: `cargo test --workspace --all-features` reports 996 passed /
  0 failed / 5 ignored (the five diagnostics in §2 pass when run explicitly),
  and `sh tools/verify.sh` is green including the real-window bootstrap and
  compiled-build suites.
- Input: the runtime input matrix requires a frontmost window because macOS
  delivers synthetic key events only to the active application and does not let
  a background process activate one. In a session where the test process cannot
  bring the window forward, the in-process input tests (the SDL3 event path)
  and the recorded OS-event matrix above are the available evidence; do not
  report a runtime input run that did not deliver events.

## 7. Cross-platform status

The rows below are filled in by the release's platform campaign. A row may be
marked `VERIFIED` only from an executed run on that platform; compilation
alone is never verification. Do not mark a platform verified from a
cross-build, and do not accept a silently chosen non-native backend: the
bootstrap suite's adapter assertion is the check.

| Platform / campaign | Backend | Status | Evidence |
|---|---|---|---|
| macOS arm64 (development host) | Metal | **VERIFIED** | §6 recorded evidence |
| Linux ARM64 Docker (native container) | Vulkan (software) | BUILD/TEST **PASS**; GPU **SOFTWARE GPU ONLY** | display-independent gate green in `debian trixie` + SDL3 3.2.10; the two real-window suites need a GPU and are not run in the container; lavapipe/llvmpipe adapter, Xvfb window, capture written |
| Linux AMD64 Docker (emulated) | Vulkan (software) | BUILD/TEST **PASS**; GPU **SOFTWARE GPU ONLY** | same display-independent gate under `linux/amd64` emulation; timings are not performance evidence |
| Linux software Vulkan | Vulkan (Mesa lavapipe) | **SOFTWARE GPU ONLY** | wgpu adapter `llvmpipe`, backend Vulkan, real SDL3 X11 window, capture written; never described as hardware |
| Windows x86_64 cross-build | Direct3D 12 (cross-compiled) | **CROSS-BUILD ONLY** | `x86_64-pc-windows-gnu` PE links SDL3.dll and the D3D12 backend; not executed |
| Native Linux | Vulkan | NOT EXECUTED — ENVIRONMENT BLOCKED | no native Linux host available |
| Native Windows | Direct3D 12 | NOT EXECUTED — ENVIRONMENT BLOCKED | no Windows host available |

Legend: **VERIFIED** = executed and recorded on that platform; **CROSS-BUILD
ONLY** = compiled but not executed; **SOFTWARE GPU ONLY** = executed against a
software Vulkan driver (never presented as native GPU validation); **NOT
RECORDED** = the campaign has not produced evidence yet.

### Cross-platform execution procedure

On each remaining platform, from a graphical session:

```sh
# 1. Build and confirm the native backend is the required one.
cargo build --release
python3 -m unittest tests.test_wgpu_bootstrap
PLACES_LEVEL=places_demo PLACES_VERBOSE=1 target/release/places
#    -> "[renderer] wgpu | adapter: ... | backend: Vulkan|Dx12 | ..."

# 2. Full gate (fmt, clippy, tests, asset/texture/prop/package/editor/compiled-build).
sh tools/verify.sh

# 3. Canonical and expanded captures (never into docs/renderer-baseline/).
PLACES_CAPTURE_DIR=$PWD/target/verification/canonical \
    sh tools/bench/capture_baseline_views.sh
PLACES_CAPTURE_DIR=$PWD/target/verification/expanded \
    sh tools/bench/capture_expanded_views.sh

# 4. Lifecycle (resize/minimize/restore/quality cycles) as in §5,
#    plus the UI captures with tools/bench/capture_views.sh.
```

There is no renderer selector: the build always initializes wgpu on the
platform's native backend and fails fast if that adapter is absent.
