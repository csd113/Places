# Lighting energy repair and maintained-level rebake

Evidence root: `target/lighting-repair/remaining/`. This continues the earlier
`target/lighting-repair/report.md` repair; its intersection, wall ownership,
ceiling clipping, stair, roof and per-triangle bounce-cache fixes are preserved.
No authoring intensity, global brightness, exposure, tone-map or albedo changes
were made. No dependencies were added. Publication was subsequently requested by the user.

## Confirmed causes → fixes → independent evidence

| First incorrect stage | Repair | Evidence / why previous checks missed it |
|---|---|---|
| Direct/indirect angular compression treated a first moment as the cosine integral. Opposing grazing lights could illuminate a floor; a uniform hemisphere reconstructed 1.5 instead of 1.0. | Carry the actual per-channel `sum(weight * max(dot(direction, normal), 0))` beside the moments. Interpolate this energy in the bounce cache, gather it for bounces/probes, and calibrate final directional encoding at the geometric normal. | New independent hemisphere quadrature and tangent-light zero tests. Previous tests covered single lobes, finiteness, visibility, relative contrast and CPU/shader agreement; those can all pass with the same incorrect multi-direction energy. |
| Second bounce gathered cumulative `D + KD` and added `K(D + KD)` to it, counting `KD` twice. | Gather only the preceding bounce order: cumulative output is `D + KD + K²D`. | A reflective floor beneath a black ceiling has nonzero first bounce and exactly zero second bounce. The regression failed against the old recurrence and passes now. Previous tests checked that bounce added light, not that each reflection order appeared once. |
| Installed package selection could use stale content. | Bump solver revision 6 → 7, force rebuild all variants, verify dependencies/fingerprints, log runtime package identity, and reject stale staged capture packages. | Before verification found stale Model Zoo, The Pit and Geometry Intentional. The old capture set exercised the demo only and did not establish every installed package's freshness. |

The compiler now rejects a mismatch between integrated and encoded surface
energy, reporting room, patch kind/origin, texel coordinates and both RGB
values. This is an arithmetic invariant, not a brightness ceiling: legitimate
HDR remains valid. A new package round-trip test independently expects
`[2, 1, 0.5]` after the runtime half-float decoder and explicitly requires values
above one to survive. Existing contrast/occlusion thresholds are unchanged.

The all-level pass also exposed a compiler integrity-check mismatch:
`capacity_dense` emits a 308,295,329-byte prop record. The runtime and typed
validator already allow 512 MiB binary records, but generic validation and cache
reuse read every entry with a 256 MiB limit. Those reads now share the runtime's
existing mesh/prop budget while retaining the archive total cap, smaller limits
for other roles, hashes and typed decoding. The measured capacity record is a
regression witness. Small-package tests never crossed the inconsistent cap.
The frozen final compiler was refreshed after this discovery, and the final
forced bake queue was restarted; intermediate `pass1/` evidence is retained.

The first attempted baseline runtime capture of `test_room` also exposed an
index-upload crash: 363 `u16` indices occupy 726 bytes, which violates wgpu's
four-byte queue-copy alignment. Shared world, prop, character, decal and dynamic
index uploads now use wgpu's existing padded mapped initialization, retaining
the original draw count. The GPU regression independently checks 0, 3, 6 and
363 indices against 4, 8, 12 and 728 allocated bytes, and was explicitly run on
Metal. Quad-only coverage had hidden the odd-triangle case. The original player
cannot provide a `test_room` before image; its panic log is retained. Its
original baked package is also captured with the final upload fix, providing
a lighting baseline without altering the old baked energy
(`before-test-room-upload-fix/`). This baseline explicitly records the newer
player hash and original package hash.

## Stage measurements

`stages-before.log`, `stages-after.log` and `stage-measurements.json` retain the
same deterministic Full-quality stage audit. Values below are area-weighted
RGB-mean light, before display tone mapping. The audit assembles its geometry
through the existing regression path; it is representative solver evidence,
not a claim that these statistics are extracted from the final package.

| Region / stage | Before | After |
|---|---:|---:|
| Demo home (room 6), direct ceiling mean | 0.072351 | 0.014159 |
| Demo home, final ceiling mean / peak | 0.607548 / 0.810113 | 0.319927 / 0.470075 |
| Demo home, final floor mean / peak | 0.480874 / 0.963233 | 0.344828 / 0.568973 |
| Demo home, final wall mean / peak | 0.321005 / 1.080530 | 0.232138 / 0.527432 |
| Demo room 0, final ceiling mean / peak | 1.423202 / 2.029367 | 0.534123 / 0.690286 |
| Home Showcase room 1, final floor mean / peak | 1.164634 / 1.766816 | 0.681959 / 0.893850 |
| Home Showcase room 2, final ceiling mean / peak | 1.277339 / 1.502930 | 0.520894 / 0.563111 |

The old demo-home ceiling already averaged 0.570020 **before** fill, versus
0.607548 after fill. Baseline fill was not the first incorrect stage and was
not used to compensate for the repair. The diagnostic fraction above 1.4
(where the existing display shoulder retains less than 5% slope) fell from
50.11% to zero for demo room 0 ceiling and from 23.72% to zero for Home Showcase
room 1 floor. This diagnostic is not a universal pass/fail brightness threshold.

## Pipeline checks and hypotheses

- Authoring → geometry/emitters: existing fixture sampling, calibrated intensity
  units/falloff and material albedo remain unchanged. Multiple directions, not
  a duplicated authored fixture, explain the independent failing witnesses.
  The earlier geometry/ray regressions remain in the full passing suite.
- Direct → bounce → fill: stage audits isolate the first error before fill;
  per-order and exact-integral regressions cover the two proven energy errors.
- Filtering/atlas → package: chart-local filtering averages the exact energy
  with the moments; its output must preserve that integral. HDR lightmaps use
  half-float storage, not sRGB texture decoding or an 8-bit early clamp.
- Package → runtime: each final capture checks the loaded manifest hash,
  renderer level, graphics profile and lightmap variant, and rejects a runtime
  transport/bake log. It also requires positive preparation-trace evidence of
  zero lighting and atlas work; missing or nonzero timing fails the check.
  Whole-archive hashes are recorded separately. The
  embedded loader identifies its package by whole-archive hash; its empty
  source path plus an empty installed-level directory confirms fallback use.
- Runtime shader/tonemapping: `surface_light` reconstructs base and enabled
  switchable layers, then applies the existing soft shoulder once. It has no
  runtime fixture-light loop that could duplicate baked illumination. Emission
  remains a separate material/bloom term. Static textured surfaces and emissive
  diffusers are assessed separately in actual Metal runtime captures.
- Linear/display conventions: the engine uses its documented calibrated
  display-light scale. No speculative gamma conversion or inverse-square
  migration was introduced. Dynamic probes still use the established neutral
  directional representation; this repair removes incorrect surface energy
  feeding their gathers, not the approximation inherent in that representation.

## Coverage and reproducibility

`tools/bench/capture_lighting_regions.py` inventories every top-level maintained
source and adjacent package in `assets/levels`, `tests/fixtures/levels` and
`levels`. Duplicate source copies are hashed. `geometry_broken` and nested
negative/repair inputs are inventoried separately from playable acceptance.
There are 19 maintained playable level IDs. The only embedded level is the
compiled demo in `src/loader.rs`; it is refreshed by rebuilding the player
following the demo package update.

Room views use explicit floor-relative eye height, four directions per grid
site at no more than 12 m spacing, plus tall-room ceiling views and water/floor
region views. Each room also has an explicit floor view and ordinary-height
ceiling view: a small room's cardinal cameras can otherwise see only walls.
Curved partitions get views toward and away from both faces,
because a center camera can land inside a screen. High covers every direction; Medium and Low cover every site,
ceiling and region with one cardinal direction. Each profile pins all advanced
settings. The 25 canonical demo cameras were also refreshed with the frozen
final player in `canonical-final/`, including opposite roof, balcony and
pool/window close-ups. They supplement the full room-grid acceptance set.

Before views preserve the original executables and installed packages, then
compile missing fixture packages with the original compiler. The first
abandoned `remaining/before/` attempt had incorrect eye height and is excluded;
use `before-installed/` and `before-fixtures/`. Camera coordinates, settings,
logs, committed runtime state and package hashes accompany the PNGs.
`before-curves/` and `after-curves/` contain the additional curved-partition
views discovered during visual review; view IDs deduplicate overlapping captures.
`before-surfaces/` adds matched floor/ceiling baselines without rebaking or
repeating existing camera investigations. `baseline-coverage-final.json` verifies
1,087 unique High before views with exactly matching final camera coordinates
and angles, covering every bakeable room/region. `final-runtime-captures.json`
selects the final captures, including the two authoring-repair replacements.
`water_partitions.py` supplies explicit wet/dry ceiling and overview views for
Water Transmission, whose generic centre camera lies inside its divider.

Final bakes use the single frozen `final-bin/places-compile` executable and
`build <source> --workers 12 --force --json --out <package>`. An exclusive lock
shares the worker budget with any remaining baseline bake. Builds are serial;
no two compiler processes compute concurrently. Every successful build contains
Off, Medium and Full variants. Existing installed/shipped copies are updated;
fixture-only packages remain under `final/packages/`.

## Retired level and authoring repairs

`capacity_sparse` fails before lighting: the current dense navigation format
requires 404,854,416 cells across its four islands at ±2 km, against a 2,097,152
cell cap. The original compiler reproduces this failure. Changing room scale,
coarsening navigation or relaxing the allocation cap would change unrelated
behavior; no fake/partial package is emitted. This level could not receive package
or runtime acceptance in this repair. At the user's subsequent request, its JSON
source was deleted and removed from generation and benchmark defaults. The
large-coordinate CPU regression remains in memory, preserving collision, water,
trigger and route assertions. The historical 19-level results below retain the
failed attempt; all 18 remaining maintained playable sources were upgraded.

The all-profile visual pass exposed a second kind of defect in Water Transmission:
Low showed jagged depth fighting where the divider extended from -1.2 m to 3.2 m
over faces already supplied by the basin skirts. This was first incorrect in
**authoring/geometry**, before lighting or tone mapping. Starting the divider at
deck height (0 m), with its top unchanged, removed all 24 duplicate-surface errors
and the visible overlap. Basin dimensions, water, fixtures and the opaque control
remain unchanged. `water-overlap-comparison.png` records the matched Low result.

The Rendering Diagnostic's two stained wall panels overlaid a continuous backing
wall and duplicated its generated baseboards. Splitting the backing wall around
the existing panels removed its 14 duplicate-surface errors while retaining the
same wall boundary and materials. Both levels were force-rebaked with the same
frozen compiler, revalidated and recaptured at all three qualities; the water
wet/dry supplements were repeated too. Source fingerprints changed automatically.

New geometry regressions check the corrected fixtures and recreate each original
overlap as a negative witness. The panel witness runs the full authoring preparation
step because that is where the duplicate baseboards were introduced. Existing
geometry diagnostics already reported these conditions; previous maintained-clean
fixture assertions and demo-only visual cameras did not cover these two sources.
No diagnostic thresholds or intent suppressions were changed.

The full suite caught a test that required the diagnostic fixture's former
overlapping representation. That original input remains an explicit coalescing
regression; the repaired playable source is additionally checked for both stained
panels' exact position and full height. The test's material-partition assertions
remain intact. This test-only update does not change the frozen binaries or bakes.

All 19 maintained geometry checks now exit successfully with zero errors. Warning
findings remain explicit: Water Transmission has 32 opposite-facing contacts at
the opaque control, and Rendering Diagnostic has six small trim-joint slivers
(0.00081 m² each) plus three doorway/room-footprint warnings. The other 17 levels'
findings are unchanged. `geometry-final-acceptance.json` retains before/final
summaries and links to the two corrected reports.

## Final results

18 of 19 maintained playable levels were force-rebuilt and accepted at the
package/runtime lighting stages: 54 quality variants, 90 rooms. All 18 passed
package `validate`, source `verify`, dependency integrity and current-fingerprint
checks without warnings. The five existing shipped/installed copies match the
rebuilt archives byte for byte. `capacity_sparse` remains blocked before lighting.

The final visual pass inspected 2,185 frozen-player runtime images: 2,145 room/
region/profile views, 12 wet/dry basin views, 25 canonical demo close-ups and
3 embedded-fallback views. Every external capture identifies the exact rebuilt
package, renderer level, profile and lightmap variant; preparation traces confirm
zero lighting and atlas work. The three embedded captures are pixel-identical
to the corresponding external-package captures.

Counts below are the main High / Medium / Low room-region set. “Pass” in the
bake column means all three variants, package validation and current source/
dependency verification passed. All geometry checks now have zero errors;
remaining warnings are detailed above.

| Level | Rooms | High / Medium / Low | Bake / verify | Visual finding | Geometry errors |
|---|---:|---:|---|---|---:|
| `asset_polish_showcase` | 1 | 14 / 11 / 11 | Pass | Curved walls, trim and material detail preserved | 0 |
| `capacity_dense` | 1 | 202 / 97 / 97 | Pass | 120-fixture hall, dense props and pool checked; authored dark edge retained | 0 |
| `capacity_sparse` | 4 | — | Navigation limit | No package/runtime acceptance | 0 |
| `entity_showcase` | 1 | 18 / 6 / 6 | Pass | Fixture pools, ceiling, carpet and actors coherent; open void confirmed | 0 |
| `geometry_intentional` | 3 | 22 / 13 / 13 | Pass | Curves, tile detail and central light pool preserved | 0 |
| `home_showcase` | 4 | 25 / 13 / 13 | Pass | Ceiling/wood/tile detail retained; excess energy removed | 0 |
| `level0_pit` | 25 | 294 / 129 / 129 | Pass | All storeys checked; authored deep-gallery darkness retained | 0 |
| `lighting_diagnostic` | 9 | 54 / 27 / 27 | Pass | Unlit, colored, mixed, bright and tall-room controls remain distinct | 0 |
| `lighting_isolation` | 13 | 78 / 39 / 39 | Pass | Sealed, door/window, stub and RGB isolation controls preserved | 0 |
| `lighting_repair_cases` | 5 | 30 / 15 / 15 | Pass | Cluster remains brighter with detail; doorway darkness and controls preserved | 0 |
| `model_zoo` | 1 | 111 / 51 / 51 | Pass | Hall, prop clusters, curves and water coherent | 0 |
| `places_demo` | 12 | 114 / 63 / 63 | Pass | Home, roof, balcony, stairs, pool and steam checked | 0 |
| `pool_showcase` | 3 | 24 / 12 / 12 | Pass | Pale basin tiles, grout, steps and fixture gradients preserved | 0 |
| `prop_showcase` | 2 | 12 / 6 / 6 | Pass | Furniture, carpet, wallpaper and localized illumination retained | 0 |
| `prop_stress` | 1 | 18 / 6 / 6 | Pass | Dense furniture and lamp detail retained; sparse fixture pools coherent | 0 |
| `rendering_diagnostic` | 3 | 18 / 9 / 9 | Pass | Warm/cool/dark controls preserved; duplicate panel walls repaired | 0 |
| `test_room` | 1 | 10 / 4 / 4 | Pass | All profiles render after upload fix; matched old-package lighting baseline checked | 0 |
| `vertical_diagnostic` | 4 | 32 / 20 / 20 | Pass | Raised floors, stairs, ceiling heights and obstruction shadows preserved | 0 |
| `water_transmission` | 1 | 11 / 8 / 8 | Pass | Wet/dry transmission retained; Low skirt/divider overlap repaired | 0 |

High retains texture detail, controlled fixture pools and occlusion. Medium
preserves the same illumination structure with its existing coarser indirect
sampling; some soft sampling variation remains visible. Low intentionally uses
vertex lighting without lightmaps, with flatter/brighter fill and reduced shadow
detail. No global profile settings or acceptance thresholds were changed.

Intentional dark areas were checked against authoring: the deep Pit galleries
have disabled/zero-brightness fixtures; the dense stress hall contains exactly
120 fixtures and stops after three lights in its final southern row. Bright
diffuser faces and the demo sauna steam were not mistaken for diffuse blowout.
Water Transmission has four extra wet/dry views per profile because its generic
centre camera lies in a divider. The final canonical home roof/ceiling and
balcony close-ups retain surface grain and intentional shadows.

The final compiler SHA-256 is
`07f8406d5212eaaa97b7247cfc2c923e5c147ae20e61d46ed0f3392d7e17e3e6`;
player SHA-256 is
`5017aabd93960b22bbdad7705546844c946a2159249ae0aeeeded449a791ce15`.
A second forced 12-worker Home Showcase bake using that same compiler was
byte-identical (`d921378b7275dfab7e9e28eeb77ac128e2b6abc2c7c6d53ed099103b93c5f4fa`).
All task-owned bake, capture and verification jobs were awaited.

Evidence indexes under the evidence root:

- `final/results.json`: per-level source/package paths, variant statistics, validation,
  source/dependency freshness, archive hashes and updated copies.
- `final-audit.json`: exact package/player/manifest identities and complete
  room/profile view-set assertions; `baseline-coverage-final.json`: 1,087
  independently recorded matched High baselines.
- `supplement-final-audit.json`: water, canonical and embedded identity/
  zero-bake checks, including embedded/external pixel equality.
- `visual-final-reviewed.json` and `sheets/after-final/`: per-level visual
  findings and reviewed contact sheets; full-resolution PNGs/logs/traces live
  under `after-final/`, `canonical-final/`, `after-water-authoring/`,
  `after-rendering-authoring/` and `water-partitions-after-authoring/`.
  `final-runtime-captures.json` selects the latest 2,145 main captures, excluding
  the superseded pre-authoring water/rendering images.
- `matched-energy-comparison.png`: matched home and ceiling before/final views.
- `geometry-final-acceptance.json`: all 19 geometry results; the two repaired
  levels retain their previous summaries alongside the final zero-error results.
- `final/failures.json`: the original and final navigation blocker is documented
  in the report above; this file retains the final failed command/error.

## Validation commands

- `git diff --check`: passed.
- `cargo fmt --all --check`: passed.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo`: passed.
- `cargo test --workspace --all-features -- --test-threads=1`: 1,660 passed,
  zero failed, 14 ignored, including the final authoring regressions. This
  workspace declares no optional Cargo features.
- `cargo test odd_triangle_indices_upload_without_changing_draw_counts -- --ignored`: passed on Metal.
- `python3 -m unittest discover -s tests -p test_lighting_regions.py`: six passed.
- `python3 -m unittest discover -s tests -p test_package.py`: 47 passed;
  the printed duplicate-ID error belongs to an intentional negative test.
- `python3 tools/assets/validate.py`: passed, no warnings.
- `python3 tools/textures/build.py --check`: passed, 42 existing preferred-size
  warnings across the 51 textures checked; no texture assets changed.
- `python3 tools/props/build.py --check`: passed.
- `places --check-geometry --level <source> --json <report>` for all 19
  maintained IDs: all 19 have zero errors after the two focused authoring repairs;
  warnings and before/final comparisons are retained.

## Changed files

- Solver/encoding: `src/lighting/transport.rs`,
  `src/lighting/lightmap/mod.rs`.
- Regressions: `src/lighting/transport/tests.rs`,
  `src/lighting/transport/tests/fill.rs`, `src/lighting_repair_regression.rs`,
  `src/geometry_check.rs` (two fixture regressions), `src/render/tests.rs`
  (preserved legacy coalescing coverage and repaired-panel dimensions).
- Package validation and runtime identity: `src/compiler.rs`, `src/loading.rs`.
- Padded GPU index uploads: `src/render/wgpu/world.rs`,
  `src/render/wgpu/props.rs`, `src/render/wgpu/dynamic.rs`,
  `src/render/wgpu/character.rs`, `src/render/wgpu/decals.rs`.
- Capture tooling: `tools/bench/capture_lighting_regions.py`,
  `tests/test_lighting_regions.py`.
- Documentation: `docs/MAP_AUTHORING_GUIDE.md`, `docs/RENDERER.md`, this report.
- Corrected level authoring: `tests/fixtures/levels/water_transmission.json`,
  `tests/fixtures/levels/rendering_diagnostic.json`.
- Tracked shipped packages: `assets/levels/places_demo.placesmap`,
  `assets/levels/model_zoo.placesmap`.
- Existing local installed copies: `levels/home_showcase.placesmap`,
  `levels/geometry_intentional.placesmap`, `levels/level0_pit.placesmap`.
  Fixture-only rebuilt packages and all evidence remain under the evidence root.

Only the two proven overlapping-geometry source defects were changed. No visual
assets, dependencies, exposure settings or authored light intensities changed.
Geometry/texture warnings were not suppressed.

## Requested retirement and publication

Deleted `tests/fixtures/levels/capacity_sparse.json`, the only maintained source
that failed the upgrade. Updated `src/zoo_audit.rs`,
`tools/levels/build_capacity_fixtures.py`, `tools/bench/loading.py` and
`tools/bench/measure_capacity.sh` to retain CPU regression coverage and remove
references that would regenerate or try to load the retired playable source.
Historical reports and intentional negative-test inputs remain unchanged.

Post-retirement validation passed: `cargo fmt --all --check`, the full strict
Clippy command above, and `cargo test --workspace --all-features -- --test-threads=2`
(1,660 passed, zero failed, 14 ignored). The dense generator `--check`, six
capture-harness tests, Python compilation, shell syntax and `git diff --check`
also passed. All 18 remaining maintained IDs match successful final rebakes.
