# Desktop verification

This is the authoritative gate for Places: the wgpu renderer (Metal on macOS,
Vulkan on Linux, Direct3D 12 on Windows) behind the SDL3 platform layer. Run
from the repository root on a desktop session. The release's platform status is
recorded in the matrix in §7.

## 1. Prerequisites

- Rust 1.99.0, selected by `rust-toolchain.toml` with Clippy and rustfmt.
  Cargo's minimum compiler version is 1.99; no machine-wide default is changed.
- SDL3 3.2 or newer and `pkg-config`.
- Python 3 (standard library only; no `pip install`).
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

The script stops at the first failure and runs these commands in order. It builds
the release executables before Python/native checks, so an older binary cannot
stand in for the current source and pinned toolchain:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo
cargo test --workspace --all-features
cargo build --release
python3 tools/assets/validate.py
python3 tools/props/build.py --check
python3 tools/assets/audit.py --workers 12 --out target/verification/asset-inventory.json
python3 tools/entities/author_halloween_assets.py --check
python3 tools/entities/check_clip_boundaries.py
python3 tools/levels/build_outdoor_fixture.py --check
python3 tools/levels/build_outdoor_route.py --check
python3 tools/levels/build_lantern_hollow.py --check
python3 tools/levels/build_lighting_quality.py --check
python3 -m unittest tests.test_showcase_assets tests.test_ghost_surface tests.test_lantern_hollow
cargo test --lib showcase_audit
cargo test --lib static_prop_lighting_tests
cargo test --lib bundled_static_models_fit_medium_and_full_atlas_plans -- --ignored
cargo run --quiet --release --bin places-compile -- build assets/levels/places_demo.json --workers 12
cargo run --quiet --release --bin places-compile -- build assets/levels/model_zoo.json --workers 12
cargo run --quiet --release --bin places-compile -- build assets/levels/lantern_hollow.json --workers 12
cargo run --quiet --release --bin places-compile -- build assets/levels/movement_test.json --workers 12
cargo run --quiet --release --bin places-compile -- verify assets/levels/places_demo.json --package assets/levels/places_demo.placesmap --require-current
cargo run --quiet --release --bin places-compile -- verify assets/levels/model_zoo.json --package assets/levels/model_zoo.placesmap --require-current
cargo run --quiet --release --bin places-compile -- verify assets/levels/lantern_hollow.json --package assets/levels/lantern_hollow.placesmap --require-current
cargo run --quiet --release --bin places-compile -- verify assets/levels/movement_test.json --package assets/levels/movement_test.placesmap --require-current
cargo run --quiet --release --bin places-compile -- validate assets/levels/lantern_hollow.placesmap
cargo run --quiet --release --bin places-compile -- validate assets/levels/places_demo.placesmap
cargo run --quiet --release --bin places-compile -- validate assets/levels/model_zoo.placesmap
cargo run --quiet --release --bin places-compile -- validate assets/levels/movement_test.placesmap
python3 -m unittest tests.test_package
python3 -m unittest tests.test_packaging tests.test_glb_accessors tests.test_asset_audit
python3 -m unittest tests.test_tool_execution tests.test_zoo_generator tests.test_bench_metrics tests.test_lightmap_harness
python3 -m unittest tests.test_geometry_repair
cargo test --lib render::wgpu::renderer::low_lighting_tests -- --ignored --test-threads=1
python3 -m unittest tests.test_compiled_build
python3 -m unittest tests.test_wgpu_bootstrap
git diff --check
```

The four `places-compile build` steps are the incremental gate: a current
package is reused (`rebuilt: false`, bytes untouched), and a source or
fingerprint change publishes an atomically replaced package before `validate`
checks it. Snapshot the four `.placesmap` hashes when a change could affect the
compiler, so a silent rewrite is visible in the evidence. The
`verify … --require-current` steps are the staleness gate: a package whose
source, asset identities or variants changed after its build fails the run
instead of shipping, which is exactly how a stale bundled package is caught.
`validate` remains the decode gate: it re-reads every record and re-hashes every
entry.

The package suite runs the texture CLI `--check`; the gate does not invoke it twice.

All commands must exit zero. Compiled-build tests open real SDL3 windows and
GPU surfaces; a skipped suite is not a completed desktop gate. Require the final
test summaries to show zero failures; other warnings require investigation.

`tests/platform_support.py` owns the desktop-session capability table the two
window suites share: macOS and native Windows attempt the suite, while X11 and
Wayland hosts require `DISPLAY`/`WAYLAND_DISPLAY`. A native Windows session
without a usable desktop fails with the binary's window error instead of being
silently skipped. `tests/test_packaging.py` exercises the `tools/package.sh`
destination guard against disposable fixtures, including a real packaging run
whose unrelated siblings must survive; it needs the release binary and can be
skipped with `PLACES_SKIP_PACKAGING=1`. `tests/test_glb_accessors.py` mirrors
the Rust GLB accessor fixtures in Python. `tests/test_bench_metrics.py` pins
the loading-harness metric names and the settings-transition window rule.

The map geometry checker is a manual, read-only gate over one level (its
behaviour is also pinned by the `geometry_check::tests` fixture suite):

```sh
cargo run --release -- --check-geometry --level places_demo
cargo run --release -- --check-geometry --level levels/level0_pit.json \
    --json target/pit-geometry.json --markers-obj target/pit-markers.obj
```

It exits `0` when the level has no confirmed defects and `1` when it does
(`--strict` also fails on warnings); `2` means the level could not be read or
parsed. See `docs/MAP_AUTHORING_GUIDE.md` §31 for every check, the intent
annotations and the honest limitations.

A confirmed wall/doorway plane shift (`wall-joint-step`) is repaired offline
with the planner and the order-preserving applier; the Rust suite additionally
pins the five maintained sources against the `wall-joint-*` rules:

```sh
cargo run --release -- --repair-geometry --level assets/levels/places_demo.json \
    --plan target/demo-plan.json
python3 tools/levels/repair_alignment.py --plan target/demo-plan.json --check
python3 tools/levels/repair_alignment.py --plan target/demo-plan.json --apply
```

The applier refuses a source that changed since planning, validates the result
with the checker, and re-plans to prove a second pass makes no edits; the
player never repairs or snaps geometry at load time. Any repaired map must be
recompiled and rebaked (`places-compile build --force`) before shipping, because
the prepared mesh, lighting, collision, navigation and probes all depend on the
repaired surfaces.

Alongside the gate, the GPU diagnostics run explicitly (they are ignored by
default because they need an adapter or write measurement files):

```sh
cargo test --all-features --bin places -- --ignored
```

The Low-lighting override resource tests run explicitly in the desktop gate:
these inspect real GPU installations and selected-quality recovery. Other
intentionally ignored diagnostics include: the reflection cube round-trip
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
identical.

Generated authoring sources are not the playable content. After regenerating a
source, recompile its package and verify it:

```sh
./target/release/places-compile build assets/levels/model_zoo.json
./target/release/places-compile validate assets/levels/model_zoo.placesmap
./target/release/places-compile verify assets/levels/model_zoo.json \
    --package assets/levels/model_zoo.placesmap
```

`validate` decodes every record and re-hashes every entry; `verify` compares the
package's developer fingerprint with the source and asset identities as they are
now. `tests/test_package.py` independently checks the bundled packages with the
Python standard library (manifest shape, content addressing, variant payloads).

A player-side no-preparation check runs the built binary with a fresh, isolated
state root and no source tree access:

```sh
PLACES_STATE_ROOT="$PWD/target/verification/fresh-state" \
PLACES_ASSET_ROOT="$PWD/assets" \
PLACES_LOAD_TRACE=1 \
PLACES_LEVEL=places_demo ./target/release/places
```

The trace must show the compiled level committed with no `[lightmaps] fill`
step, and the state root must contain only writable player state (`levels/`,
`import/`, `settings.json`, `cache/`). The texture generator skips shipped images whose dimensions differ
from its placeholder painter. Never use `--force` as a validation step. Use
`python3 tools/props/animate_spooner_man.py` for the documented entity-only
workflow. PNG artwork is loaded from committed assets at runtime.

## 4. Runtime and visual gate

Compile the bundled sources, then launch the demo with
`PLACES_LEVEL=places_demo cargo run --release` (or the packaged binary). The
player decodes the compiled package — packaged lightmaps, packaged reflection
probe captures and compiled collision — and the renderer draws the complete
feature set: baked lightmaps, reflections, props, fixtures, emission, decals,
fog, post-processing and the HUD; see [RENDERER.md](RENDERER.md).

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

Settings screens can be captured without a keyboard: `PLACES_SCREEN=settings`
opens the Settings root, `PLACES_SCREEN=graphics` the Graphics page (Advanced
collapsed) and `PLACES_SCREEN=advanced` the Graphics page with the Advanced
group expanded; combine with `PLACES_CAPTURE` as above.

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
through the SDL window — `resize:<w>x<h>`, `minimize`, `restore` —
`PLACES_BENCH_QUALITY_CYCLE` switches the overall quality level through the
normal rebuild path (`low` / `medium` / `high`),
and `PLACES_BENCH_GRAPHICS_CYCLE` changes one Advanced graphics setting at a
time through the same setters the menu uses (`filtering=low|medium|high`,
`lightmaps=off|medium|full`, `reflections=off|medium|full`, `bloom=on|off`).
`PLACES_MOVE_SCRIPT` drives the *player* through real held controls for a
capture (`forward@0-6.5,jump@0.2-0.6`, ready-world simulation seconds), and
`PLACES_CAPTURE_TIME=seconds` waits for the matching ready-world second, which
is how `tools/bench/capture_movement.sh` records jumps, landings and pool
crossings at the same in-world moments at any frame rate:

```sh
PLACES_BENCH=1 PLACES_BENCH_FRAMES=34 \
PLACES_BENCH_WINDOW_CYCLE=3:resize:800x450,6:resize:500x300,8:resize:800x450,12:minimize,16:restore,18:resize:640x360,24:resize:900x500,26:resize:640x360 \
PLACES_BENCH_QUALITY_CYCLE=20:low,22:high \
PLACES_BENCH_GRAPHICS_CYCLE=10:reflections=off,14:lightmaps=medium,28:bloom=off \
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

# 2. Full gate (fmt, clippy, tests, asset/texture/prop/package/compiled-build).
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

## Loading lifecycle measurements

`python3 tools/bench/loading.py --binary target/release/places --out /tmp/places-loading-check`
runs the already-built game serially with isolated cold/warm application state,
pinned resolution/quality/camera, phase logs and monotonic JSONL traces. The output
directory must be new. This does not purge OS, driver or user caches. Use the same
command with `target/debug/places` to measure debug runtime separately from builds.

`PLACES_LOAD_TRACE=/absolute/path/trace.jsonl` opts into request, preparation,
upload, world-commit, event-pump and present records. The harness includes initial
setup in whole-process gaps, reports loading intervals separately, and leaves
unavailable baseline samples null. Input latency is enqueue-to-handling latency,
not physical input-device latency.

Native smoke tests exercise the real SDL loop with `PLACES_BENCH_ACTIONS` JSON and
`PLACES_PREPARE_DELAY_MS` (only honored with `PLACES_BENCH=1`). Bounded action scripts
anchor to `start`, `request:<id>`, `upload:<id>`, `ready:<id>` or `failed:<id>`.
Each anchor denotes its first occurrence; `upload` follows accepted CPU preparation
and precedes staged GPU completion. Tests cover cancellation before and after that
boundary, retry, supersession, failure recovery, quality changes and repeated visits.
Their timing observations are saved as evidence rather than tight unit-test limits.
Optional RSS sampling in action tests is coarse process memory, excludes GPU/child
allocations and may miss peaks; it is separate from the startup harness's whole-run
`/usr/bin/time` peak on macOS.
