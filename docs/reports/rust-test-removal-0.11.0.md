# Rust automated-test removal — 0.11.0

## Scope and production preservation

Starting commit: `502b37c84839d8815a2706234b42b58e8b22d7a7`, on
`0.11.0-staging`. The approved stage removes all existing project-owned Rust
unit/integration/executable doctests and test-only audit/support infrastructure,
including tests introduced by the renderer and shader stages. No old suite was
rerun before deletion; no map was baked.

The 2,377 `#[test]` functions and every `cfg(test)` item are gone. A source fence
inventory finds only JSON/text documentation examples, with no executable Rust
doctests. No dependency existed solely under `[dev-dependencies]`, and no explicit
`[[test]]` or custom harness declaration existed. Cargo's ordinary discovery
remains available for future justified tests; no blanket `test = false` or
`doctest = false` hides new tests.

Thirteen root audit/support modules, 65 owner test files and three integration
files contain no production implementation. All mixed production files retain
their non-test items; extraction and moves were unnecessary. Removed helpers are
previously test-gated CPU shader mirrors, assertions, small fixture constructors,
renderer controls and test-only accessors/reexports. Three test-serial worker
branches are removed without changing production worker budgets; cross-platform
performance helpers retain their OS-specific compilation guards.

Runtime/source validation, package hashes and record-size/path bounds, production
assertions, loading/cancellation/error paths, assets, compiler binaries, geometry
checker/repair planner, native renderer probes, benchmark binaries and production
`engine::lighting::transport::probe_audit` remain. The latter stays byte-identical,
including safe diagnostic file publication. Python validation and developer tools,
third-party dependency code, and shared `tests/fixtures/levels` / `native` inputs
remain; only the ten Rust-exclusive unit JSON fixtures are removed.

The shared Rust gate/CI replace the obsolete test stage with an all-feature
production executable build. The former test-only optimization profile and
ignored Rust GPU invocation in the full desktop script are removed. Active docs
now distinguish retained tooling/runtime checks from retired Rust-only assertions;
a build is not artwork or native-render acceptance. Historical reports retain
their dated test evidence.

`AGENTS.md` is now tracked so the updated future-test policy is durable, including
owning-folder discovery and no ordinary-CI full-map bakes. It and verification
guidance require FPS comparisons to disable V-sync/frame caps, record actual
presentation/drawable settings, and label refresh-rate plateaus inconclusive.
This cleanup makes no new performance comparison.

## Accounting

No source moves, merges or production extractions occur in this stage.
The changelog enumerates every genuinely deleted file. Git text-line accounting
below separates whole-file deletion from edits within surviving Rust files.

| Path | Status | Added lines | Removed lines |
| --- | --- | ---: | ---: |
| `src/architecture_audit.rs` | deleted | 0 | 3,244 |
| `src/engine/ai/mod.rs` | test items removed | 0 | 3 |
| `src/engine/ai/tests.rs` | deleted | 0 | 1,376 |
| `src/engine/assets.rs` | test items removed | 0 | 122 |
| `src/engine/assets/tests.rs` | deleted | 0 | 1,016 |
| `src/engine/collision.rs` | test items removed | 0 | 3 |
| `src/engine/collision/movement.rs` | test items removed | 0 | 65 |
| `src/engine/collision/tests.rs` | deleted | 0 | 883 |
| `src/engine/collision_index.rs` | test items removed | 0 | 200 |
| `src/engine/diagnostics.rs` | test items removed | 0 | 58 |
| `src/engine/door.rs` | test items removed | 0 | 148 |
| `src/engine/entities/balloon.rs` | test items removed | 0 | 77 |
| `src/engine/entities/components.rs` | test items removed | 0 | 382 |
| `src/engine/entities/events.rs` | test items removed | 0 | 403 |
| `src/engine/entities/id.rs` | test items removed | 0 | 73 |
| `src/engine/entities/mod.rs` | test items removed | 0 | 2,043 |
| `src/engine/entities/sequences.rs` | test items removed | 0 | 99 |
| `src/engine/entities/spawn.rs` | test items removed | 0 | 85 |
| `src/engine/entities/timers.rs` | test items removed | 0 | 261 |
| `src/engine/entity.rs` | test items removed | 0 | 285 |
| `src/engine/font.rs` | test items removed | 0 | 3 |
| `src/engine/font/tests.rs` | deleted | 0 | 64 |
| `src/engine/gltf.rs` | test items removed | 0 | 3 |
| `src/engine/gltf/tests.rs` | deleted | 0 | 2,563 |
| `src/engine/interact.rs` | test items removed | 0 | 440 |
| `src/engine/level.rs` | test items removed | 0 | 7 |
| `src/engine/level/tests.rs` | deleted | 0 | 3,461 |
| `src/engine/level/tests/wall_ceiling.rs` | deleted | 0 | 656 |
| `src/engine/lighting.rs` | test items removed | 0 | 3 |
| `src/engine/lighting/bake.rs` | test items removed | 0 | 355 |
| `src/engine/lighting/directional.rs` | test items removed | 0 | 3 |
| `src/engine/lighting/directional/tests.rs` | deleted | 0 | 123 |
| `src/engine/lighting/light.rs` | test items removed | 0 | 3 |
| `src/engine/lighting/light/tests.rs` | deleted | 0 | 681 |
| `src/engine/lighting/lightmap/atlas.rs` | test items removed | 0 | 57 |
| `src/engine/lighting/lightmap/mod.rs` | test items removed | 0 | 3 |
| `src/engine/lighting/lightmap/tests.rs` | deleted | 0 | 2,143 |
| `src/engine/lighting/occlusion.rs` | test items removed | 0 | 1,116 |
| `src/engine/lighting/probes.rs` | test items removed | 0 | 502 |
| `src/engine/lighting/tests.rs` | deleted | 0 | 2,718 |
| `src/engine/lighting/transport.rs` | test items removed | 0 | 18 |
| `src/engine/lighting/transport/coverage.rs` | test items removed | 0 | 4 |
| `src/engine/lighting/transport/diagnostics.rs` | test items removed | 0 | 222 |
| `src/engine/lighting/transport/tests.rs` | deleted | 0 | 1,805 |
| `src/engine/lighting/transport/tests/continuity.rs` | deleted | 0 | 572 |
| `src/engine/lighting/transport/tests/fill.rs` | deleted | 0 | 269 |
| `src/engine/lighting/transport/tests/model_lighting.rs` | deleted | 0 | 392 |
| `src/engine/lighting/transport/tests/performance.rs` | deleted | 0 | 880 |
| `src/engine/lighting/transport/tests/probes.rs` | deleted | 0 | 805 |
| `src/engine/lighting/transport/tests/quality.rs` | deleted | 0 | 1,264 |
| `src/engine/lighting/visibility.rs` | test items removed | 0 | 1,140 |
| `src/engine/loader.rs` | test items removed | 0 | 28 |
| `src/engine/loader/tests.rs` | deleted | 0 | 5,914 |
| `src/engine/logging.rs` | test items removed | 0 | 49 |
| `src/engine/materials.rs` | test items removed | 0 | 3 |
| `src/engine/materials/builtin.rs` | test items removed | 0 | 48 |
| `src/engine/materials/color.rs` | test items removed | 0 | 23 |
| `src/engine/materials/emission.rs` | test items removed | 0 | 3 |
| `src/engine/materials/emission/tests.rs` | deleted | 0 | 47 |
| `src/engine/materials/fluorescence.rs` | test items removed | 0 | 32 |
| `src/engine/materials/image.rs` | test items removed | 1 | 95 |
| `src/engine/materials/image/tests.rs` | deleted | 0 | 274 |
| `src/engine/materials/reflection.rs` | test items removed | 0 | 66 |
| `src/engine/materials/response.rs` | test items removed | 0 | 3 |
| `src/engine/materials/response/tests.rs` | deleted | 0 | 134 |
| `src/engine/materials/tests.rs` | deleted | 0 | 1,676 |
| `src/engine/nav/mod.rs` | test items removed | 0 | 3 |
| `src/engine/nav/query.rs` | test items removed | 0 | 37 |
| `src/engine/nav/tests.rs` | deleted | 0 | 1,210 |
| `src/engine/package/collision.rs` | test items removed | 0 | 255 |
| `src/engine/package/compact_lightmaps.rs` | test items removed | 0 | 187 |
| `src/engine/package/lightmaps.rs` | test items removed | 0 | 115 |
| `src/engine/package/manifest.rs` | test items removed | 0 | 44 |
| `src/engine/package/mod.rs` | test items removed | 0 | 3 |
| `src/engine/package/navigation.rs` | test items removed | 0 | 99 |
| `src/engine/package/props.rs` | test items removed | 0 | 1,117 |
| `src/engine/package/shared_props.rs` | test items removed | 0 | 278 |
| `src/engine/package/tests.rs` | deleted | 0 | 1,718 |
| `src/engine/package/world.rs` | test items removed | 0 | 210 |
| `src/engine/package/world/dependency_tests.rs` | deleted | 0 | 396 |
| `src/engine/package/world/irradiance_tests.rs` | deleted | 0 | 159 |
| `src/engine/perf.rs` | test items removed | 0 | 3 |
| `src/engine/perf/platform.rs` | test items removed | 2 | 29 |
| `src/engine/perf/tests.rs` | deleted | 0 | 170 |
| `src/engine/physics/floor_tests.rs` | deleted | 0 | 133 |
| `src/engine/physics/hull_tests.rs` | deleted | 0 | 296 |
| `src/engine/physics/mod.rs` | test items removed | 0 | 12 |
| `src/engine/physics/recovery.rs` | test items removed | 0 | 4 |
| `src/engine/physics/recovery_tests.rs` | deleted | 0 | 500 |
| `src/engine/physics/tests.rs` | deleted | 0 | 1,885 |
| `src/engine/physics/water_tests.rs` | deleted | 0 | 478 |
| `src/engine/physics/world.rs` | test items removed | 0 | 23 |
| `src/engine/props.rs` | test items removed | 0 | 3 |
| `src/engine/props/tests.rs` | deleted | 0 | 662 |
| `src/engine/quality.rs` | test items removed | 0 | 3 |
| `src/engine/quality/tests.rs` | deleted | 0 | 817 |
| `src/engine/render.rs` | test items removed | 0 | 19 |
| `src/engine/render/boundary_tests.rs` | deleted | 0 | 261 |
| `src/engine/render/common/animation.rs` | test items removed | 0 | 146 |
| `src/engine/render/common/api.rs` | test items removed | 4 | 599 |
| `src/engine/render/common/architecture.rs` | test items removed | 0 | 45 |
| `src/engine/render/common/atmosphere.rs` | test items removed | 0 | 380 |
| `src/engine/render/common/blacklight.rs` | test items removed | 0 | 141 |
| `src/engine/render/common/character.rs` | test items removed | 0 | 1,905 |
| `src/engine/render/common/doors.rs` | test items removed | 0 | 450 |
| `src/engine/render/common/dynamic.rs` | test items removed | 0 | 1,426 |
| `src/engine/render/common/dynamic_lights.rs` | test items removed | 0 | 251 |
| `src/engine/render/common/dynamic_visibility.rs` | test items removed | 0 | 376 |
| `src/engine/render/common/dynamic_visibility/tests/receiver_cache.rs` | deleted | 0 | 83 |
| `src/engine/render/common/effects.rs` | test items removed | 0 | 852 |
| `src/engine/render/common/fixtures.rs` | test items removed | 0 | 314 |
| `src/engine/render/common/framebuffer.rs` | test items removed | 0 | 97 |
| `src/engine/render/common/light_transport.rs` | test items removed | 0 | 1,502 |
| `src/engine/render/common/mesh.rs` | test items removed | 0 | 41 |
| `src/engine/render/common/mod.rs` | test items removed | 0 | 56 |
| `src/engine/render/common/postprocess.rs` | test items removed | 0 | 102 |
| `src/engine/render/common/probe_filter.rs` | test items removed | 0 | 206 |
| `src/engine/render/common/prop_chart_tests.rs` | deleted | 0 | 723 |
| `src/engine/render/common/props.rs` | test items removed | 0 | 449 |
| `src/engine/render/common/reflections.rs` | test items removed | 0 | 300 |
| `src/engine/render/common/snow.rs` | test items removed | 0 | 602 |
| `src/engine/render/common/storm.rs` | test items removed | 0 | 89 |
| `src/engine/render/common/tests/character_normals.rs` | deleted | 0 | 62 |
| `src/engine/render/common/view.rs` | test items removed | 0 | 67 |
| `src/engine/render/common/water.rs` | test items removed | 0 | 100 |
| `src/engine/render/tests.rs` | deleted | 0 | 10,325 |
| `src/engine/render/wgpu/caustics.rs` | test items removed | 0 | 160 |
| `src/engine/render/wgpu/character.rs` | test items removed | 0 | 504 |
| `src/engine/render/wgpu/decals.rs` | test items removed | 0 | 363 |
| `src/engine/render/wgpu/diagnostics.rs` | test items removed | 0 | 183 |
| `src/engine/render/wgpu/dynamic.rs` | test items removed | 0 | 17 |
| `src/engine/render/wgpu/effects.rs` | test items removed | 0 | 80 |
| `src/engine/render/wgpu/entity_lighting_tests.rs` | deleted | 0 | 1,517 |
| `src/engine/render/wgpu/environment.rs` | test items removed | 0 | 48 |
| `src/engine/render/wgpu/gpu_timing.rs` | test items removed | 0 | 118 |
| `src/engine/render/wgpu/lightmap.rs` | test items removed | 0 | 491 |
| `src/engine/render/wgpu/low_lighting_tests.rs` | deleted | 0 | 315 |
| `src/engine/render/wgpu/material.rs` | test items removed | 0 | 447 |
| `src/engine/render/wgpu/postprocess.rs` | test items removed | 0 | 386 |
| `src/engine/render/wgpu/props.rs` | test items removed | 0 | 176 |
| `src/engine/render/wgpu/reflections.rs` | test items removed | 0 | 609 |
| `src/engine/render/wgpu/renderer.rs` | test items removed | 0 | 630 |
| `src/engine/render/wgpu/sky.rs` | test items removed | 0 | 19 |
| `src/engine/render/wgpu/surface.rs` | test items removed | 0 | 197 |
| `src/engine/render/wgpu/tests/postprocess.rs` | deleted | 0 | 27 |
| `src/engine/render/wgpu/tests/props.rs` | deleted | 0 | 46 |
| `src/engine/render/wgpu/tests/surface_lifecycle.rs` | deleted | 0 | 60 |
| `src/engine/render/wgpu/tests/world_transforms.rs` | deleted | 0 | 57 |
| `src/engine/render/wgpu/texture.rs` | test items removed | 0 | 954 |
| `src/engine/render/wgpu/ui.rs` | test items removed | 0 | 221 |
| `src/engine/render/wgpu/uv_receivers.rs` | test items removed | 0 | 189 |
| `src/engine/render/wgpu/world.rs` | test items removed | 0 | 2,775 |
| `src/engine/runtime_work.rs` | test items removed | 1 | 24 |
| `src/engine/spatial.rs` | test items removed | 0 | 3 |
| `src/engine/spatial/tests.rs` | deleted | 0 | 587 |
| `src/engine/weather.rs` | test items removed | 0 | 316 |
| `src/fog_void_audit.rs` | deleted | 0 | 208 |
| `src/game/app/actions.rs` | test items removed | 0 | 36 |
| `src/game/app/flashlight.rs` | test items removed | 0 | 25 |
| `src/game/app/item_input.rs` | test items removed | 0 | 73 |
| `src/game/app/item_runtime.rs` | test items removed | 0 | 43 |
| `src/game/app/loading.rs` | test items removed | 0 | 450 |
| `src/game/app/loading/cache_tests.rs` | deleted | 0 | 404 |
| `src/game/app/mod.rs` | test items removed | 0 | 35 |
| `src/game/bench.rs` | test items removed | 0 | 3 |
| `src/game/bench/lighting_sequence.rs` | test items removed | 0 | 93 |
| `src/game/bench/tests.rs` | deleted | 0 | 311 |
| `src/game/console/mod.rs` | test items removed | 0 | 3 |
| `src/game/console/tests.rs` | deleted | 0 | 209 |
| `src/game/display.rs` | test items removed | 0 | 122 |
| `src/game/input.rs` | test items removed | 0 | 14 |
| `src/game/input/tests.rs` | deleted | 0 | 593 |
| `src/game/mod.rs` | test items removed | 0 | 7 |
| `src/game/perf_overlay.rs` | test items removed | 0 | 105 |
| `src/game/settings.rs` | test items removed | 0 | 7 |
| `src/game/settings/low_lighting_tests.rs` | deleted | 0 | 156 |
| `src/game/settings/tests.rs` | deleted | 0 | 1,429 |
| `src/game/tests.rs` | deleted | 0 | 10,004 |
| `src/game/tests/art_style.rs` | deleted | 0 | 200 |
| `src/game/tests/ice.rs` | deleted | 0 | 260 |
| `src/game/tests/movement_diagnostics.rs` | deleted | 0 | 383 |
| `src/game/tests/movement_performance.rs` | deleted | 0 | 83 |
| `src/game/tests/movement_regression.rs` | deleted | 0 | 726 |
| `src/game/tests/recovery.rs` | deleted | 0 | 149 |
| `src/game/tests/water_posture.rs` | deleted | 0 | 253 |
| `src/game/tests/winter.rs` | deleted | 0 | 130 |
| `src/game/ui.rs` | test items removed | 0 | 963 |
| `src/game/ui/menu.rs` | test items removed | 0 | 234 |
| `src/game/ui/pointer.rs` | test items removed | 0 | 75 |
| `src/lib.rs` | test items removed | 0 | 25 |
| `src/lighting_audit.rs` | deleted | 0 | 1,242 |
| `src/lighting_audit_cases.rs` | deleted | 0 | 2,102 |
| `src/lighting_isolation.rs` | deleted | 0 | 494 |
| `src/lighting_partition_audit.rs` | deleted | 0 | 624 |
| `src/lighting_repair_regression.rs` | deleted | 0 | 784 |
| `src/lighting_vertical_audit.rs` | deleted | 0 | 577 |
| `src/map_compiler/application.rs` | test items removed | 0 | 50 |
| `src/map_compiler/diagnostics.rs` | test items removed | 0 | 199 |
| `src/map_compiler/geometry_check.rs` | test items removed | 0 | 1,251 |
| `src/map_compiler/gui.rs` | test items removed | 0 | 45 |
| `src/map_compiler/mod.rs` | test items removed | 0 | 169 |
| `src/map_compiler/probes.rs` | test items removed | 0 | 309 |
| `src/map_compiler/progress.rs` | test items removed | 0 | 138 |
| `src/map_compiler/storage.rs` | test items removed | 0 | 43 |
| `src/map_compiler/tests/geometry_overlap.rs` | deleted | 0 | 53 |
| `src/map_compiler/tests/material_transport.rs` | deleted | 0 | 43 |
| `src/map_compiler/tests/performance.rs` | deleted | 0 | 652 |
| `src/showcase_audit.rs` | deleted | 0 | 118 |
| `src/static_prop_lighting_tests.rs` | deleted | 0 | 1,382 |
| `src/surface_audit.rs` | deleted | 0 | 1,260 |
| `src/test_support.rs` | deleted | 0 | 433 |
| `src/zoo_audit.rs` | deleted | 0 | 195 |
| `tests/command_line.rs` | deleted | 0 | 48 |
| `tests/fixtures/unit/architecture_contacts.json` | deleted | 0 | 232 |
| `tests/fixtures/unit/controller_contacts.json` | deleted | 0 | 3,319 |
| `tests/fixtures/unit/encounter_lifecycle.json` | deleted | 0 | 267 |
| `tests/fixtures/unit/map_independent_preparation.json` | deleted | 0 | 23 |
| `tests/fixtures/unit/navigation_stairs.json` | deleted | 0 | 259 |
| `tests/fixtures/unit/navigation_steps.json` | deleted | 0 | 560 |
| `tests/fixtures/unit/renderer_material_contract.json` | deleted | 0 | 29 |
| `tests/fixtures/unit/renderer_weather_contract.json` | deleted | 0 | 24 |
| `tests/fixtures/unit/shore_contacts.json` | deleted | 0 | 67 |
| `tests/fixtures/unit/winter_contacts.json` | deleted | 0 | 76 |
| `tests/list_levels.rs` | deleted | 0 | 364 |
| `tests/macos_cpu_port.rs` | deleted | 0 | 70 |

## Acceptance

- Rust 1.99.1 and Cargo 1.99.1 match the repository pin. The sandbox cannot use
  the configured sccache wrapper; validation sets `RUSTC_WRAPPER=` without
  changing repository settings or compiler/lint policy.
- Formatting, locked all-target/all-feature check, and strict debug/release
  all-target/all-feature Clippy pass. All-feature production release build passes;
  the default-feature release build also passes.
- Python source contracts: 372 run, 17 optional/native skips, exit 0. No Python
  validation infrastructure was removed. Asset validator: 593 assets, 370
  placeables, eight themes, zero warnings, exit 0.
- Independent reviewer normalized every one of the 134 mixed Rust files against
  the starting commit, stripping test-gated items, comments/whitespace, and the
  documented test-only branches. Zero unexpected non-test code differences;
  production probe-audit diagnostics remain byte-identical.
- Optional strict rustdoc generation still fails on 57 pre-existing broken,
  private or redundant intra-doc link locations. Every reported source line is
  present unchanged at the starting commit. Links made stale by this removal
  were fixed. No lint is suppressed; rustdoc is outside the production gate.

Machine receipts, logs, source inventories, per-file accounting and independent
review are retained under `local/rust-test-removal-0.11.0/`, outside `target`.
The direct native Metal smoke loads the existing Arcade package, realizes 76
models with zero failures, draws its world/props/characters and captures the
expected door/materials/interaction HUD. The accepted run exits 0, records 180
ready presentations and zero ready-state misses at 2560×1440, and requests then
reports Immediate presentation with V-sync disabled. This is functional smoke
coverage, with no throughput/performance claim. An initial 30-frame pilot also
exited 0 and rendered correctly but its wrapper rejected three startup
presentation misses (27 ready presentations); that failed pilot is retained and
not counted as the accepted run. Neither run shows a fatal, panic, GPU validation
error or failed model. No `.app` wrapper, security setting change or map bake was
used. Native coverage is M2 Pro/Metal only.

The production compiler validates the installed Arcade archive (`off`/`full`,
19 entries, 95 dependencies); the player CLI discovers all eight installed maps.
No matching open cleanup issue was found; unrelated queued issues remain open.

Exact-commit CI, remote-ref verification, preserved executable hashes and final
`cargo clean` receipts are retained with the commit identity in the local handoff
and delivery response. This stage stops before queued flashlight work.
