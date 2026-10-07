# Final winter toolkit audit — Prompt 9

Certification date: 2026-10-07. Baseline: `a3110c4c79fc3cfefcc0f02c1f8098650231fe88`
on `Winter-expansion`. This pass reviews the implemented toolkit, actual concept
art, native rendering and the preceding snowfall, blizzard and integration
handoffs. It introduces no new environment feature. Publication and cleanup
identity are recorded in the final task handoff and the local evidence archive.

## Completed features and compatibility

The reusable kit includes authored snow/packed-snow/ice PNG materials, explicit
ice traction, snow-covered trees and boulders, modular drifts/caps/railings,
icicles, textured warm string lights, an authored aurora sky, bounded light
snowfall and a configurable sheltered blizzard/whiteout. Winter demonstrates
these assets with paths, pond shores, grove spacing and warm indoor landmarks.
Weather, sky and ground response are selected through generic map/catalogue
configuration; the engine has no Winter-ID switch. Calm remains the shipped
default. The separate `blizzard_review` source uses the same polished content
with the existing severe 5 m visibility configuration.

All five bundled packages and every tracked production asset/Rust source remain
byte-identical to the baseline. Existing Office/painting content, controller and
compiler/lighting changes are preserved. All four non-Winter bundled sources
and decoded packages retain ordinary ground, their original sky and absent
weather. Model Zoo intentionally exhibits winter props without adopting a
winter climate. A new regression test enforces these contracts.

The audit covers schema/default/invalid-value handling, PNG/catalogue lookup,
compiler dependencies and reuse, package decode/currentness, collision and ice
contact, sky/effects reset on level changes, lighting variants, weather shelter,
quality transitions, allocation bounds, logging and device/surface error paths.
Existing Rust/renderer tests cover cancellation, invalid packages/materials,
missing resources, failed preparation and surface recovery. Fatal errors remain
reported; assertions and quality levels were not lowered.

## Defects corrected

- `tools/verify.sh` omitted Winter generation, its asset/string-light tests and
  its build/currentness/decode gates. Winter now participates alongside the
  other four bundled maps. CI archive validation also checks every package.
- `capture_snowfall.py` could summarize a run without rejecting absent/occluded
  presentations, missing world draws, incomplete performance samples, invalid
  clocks/counts, particle-buffer growth or an incomplete live quality cycle.
  It now rejects those cases; regression tests exercise the failure paths.
  The alternate snowfall fixture also exposed a missing-default assumption in
  this new helper; it was corrected to mirror the engine's optional-field
  defaults and f32 budget arithmetic, preserving older valid packages.
- The asset specification's old blanket sub-500 triangle statement omitted the
  reviewed 552/760 triangle string-light models. The documented contract now
  matches the actual kit. Verification documentation lists the complete gates
  and the existing advisory categories accurately.

No production engine, map or asset defect required a change. No unused winter
model, disposable placeholder or dead winter experimental implementation was
found. Intentional diagnostics, concept references and recovery fixtures are
retained. Python/build caches are reproducible cleanup candidates, not art.

## Asset and performance findings

The inventory contains 166 models, 10 animated models/29 clips, 168 embedded PNGs,
182 standalone PNGs, 57 materials and 37 maps: **0 asset audit errors**. All 31
winter models are referenced by Winter or the intentional Model Zoo showcase.
They total 6,650 unique triangles; trees have 1,238/1,442 triangles, string lights
344/552/760, and the other static snow-kit models remain below 500. There are no
identical winter GLBs/geometry, unused primitive vertices/accessors/materials,
duplicate faces or invalid snow topology. The one identical winter PNG is the
intentional shared Home bulb atlas at the winter family's texture location.

Snow and ice are 1024² sources, prop snow 256², string lights 128², the snowflake
256² RGBA with zero edge alpha, and aurora 2048×1024 opaque 2:1. Asset contracts,
UV orientation and tiling are preserved. Snow, ice and aurora seam checks pass.
Normal runtime textures retain the 1024 edge limit; sky uses its separate 2048
limit. Winter imagery loads PNG assets; this audit adds no runtime texture generation.

The polished content remains 304 prop placements/61 trees and 109,412 placed
model triangles. Native High uses 52 cached models, 345 prop draws, 21,447,424
geometry bytes and 9,856 KiB of model textures. Full uses four 16 MiB lightmap
pages (64 MiB), Medium three. Existing 45 subtexel sliver fallbacks remain
reported. Prompt 8's documented 16.1% model-triangle reduction, 19 fewer prop
draws and 16 MiB less Full lightmap residency are retained; this audit makes no
additional art-speedup claim.

Snow evaluates at most 1,400/1,050/700 seeds in High/Medium/Low; the engine cap is
2,048, and the combined steam/snow cap is 10,240 quads within u16 indexing.
Retained snow seeds consume 39,200 bytes, GPU particle capacity 218,400 bytes,
storm uniform 1,568 bytes and CPU shelter intervals 256 bytes. Static inspection
and native diagnostics check retained capacity: no per-frame seed, scratch or
particle-group allocation is needed. This does not measure queue staging or
unrelated allocations. Projected quad coverage is a conservative screen-area
estimate, not hardware fragment/overdraw instrumentation.

Native Metal acceptance: **90 weather/quality diagnostic views**, **36 traversal
routes** (11 existing plus seven adverse routes per mode), both live quality
cycles, six views of the separate snowfall-contrast map and **eight transitions**
from calm/severe Winter to the four existing maps. Diagnostic runs presented
34,774 ready frames with **0 missed ready presentations and 0 buffer growth**.
All configured particle budgets, rejection accounting and monotonic weather
clocks passed. Boundary jumps, stairs, foundations, shores, trees, rocks,
railings, doors, ice coasting/wall contact and ice jumps remained traversable.

Peak submitted flakes were 218 calm / 167 severe; conservative projected quad
coverage peaked at 0.016766 / 0.335721 screen equivalents. Diagnostic CPU sync
peaked at 0.803 / 0.835 ms. These are observed view bounds, not universal maxima.
The alternate map and quality cycles demonstrate reuse independently of the
Winter identifier. Severe outdoor 5 m whiteout, visible nearby lights and clear
sheltered interiors were inspected in actual captures.

The completed-frame ABBA campaign accepted **24 runs × 600 measured frames =
14,400 frames**, plus 2,880 warmup presentations. Every row had positive world
geometry/draw counters and every run had 720 ready presentations with none
missed. Whole-frame means (ms) were:

| Mode | Quality | Snow on | Snow off | On p95 | Off p95 |
| --- | --- | ---: | ---: | ---: | ---: |
| Calm | High | 3.741 | 3.920 | 9.055 | 9.353 |
| Calm | Medium | 2.403 | 3.467 | 4.288 | 8.710 |
| Calm | Low | 4.641 | 3.460 | 13.351 | 8.604 |
| Severe | High | 3.106 | 4.320 | 7.044 | 10.777 |
| Severe | Medium | 5.850 | 2.358 | 13.271 | 8.085 |
| Severe | Low | 2.224 | 3.388 | 3.735 | 8.659 |

The sign and size of these differences vary with host activity/pacing; they
cannot establish isolated weather GPU cost or a speedup. Earlier campaigns'
8–9/16–17 ms pacing jitter is not erased by this fresh run. Raw repetitions and
pooled samples are recorded in `performance.json`.

Two earlier attempts were correctly rejected for unavailable native surfaces
(381 ready/92 missed on a traversal; then 0 ready/87 missed). The console was
unlocked and the temporary power assertion active. The user confirmed the
review window could remain visible; the subsequent complete campaign passed
unchanged presentation guards. Both rejected traces remain preserved. The
separate legacy-package helper failure and its correction are also retained.


## Exact validation

| Gate | Result |
| --- | --- |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Exit 0 |
| `cargo test --workspace` | Exit 0; 2,021 library + 6 binary/integration passed; 23 existing ignored; 0 failed |
| `sh tools/verify.sh` with inherited asset override removed | Exit 0; 1,030.629 s |
| Gate fmt, locked workspace/all-target/all-feature check, strict dev/release Clippy, release build | All exit 0 |
| Gate all-feature Rust tests | 2,021 library + 6 binary/integration passed; 23 existing ignored; 0 failed |
| Gate Python suites | 213 passed, 0 failed, **0 skipped** |
| Explicit GPU atlas and Low lighting resource probes | 1 + 2 passed |
| Five bundled normal compiler builds, currentness and decoding | All pass; current packages reused, no redundant bake |
| Severe review build/currentness/decode | All exit 0; build reused (0.952 s) |
| Calm and severe strict geometry | Both 0 errors, 0 warnings |
| Snow/ice/aurora seams; generator/current texture checks | All exit 0 |
| `git diff --check` | Exit 0 |

After the helper default correction, the 53 tool/benchmark Python tests passed
again (14.294 s), as did fmt, patch whitespace and strict Clippy. The final
Clippy retry disabled the optional cache wrapper after the sandbox denied
`sccache` before compilation; no lint or assertion was suppressed.

The texture check reports **60 existing advisories**: 51 preferred source-size
notices, one intentional non-POT diagnostic fixture and eight absent historical
Painter manifests. Asset/geometry and strict compiler checks have no new
warnings. These advisories were documented rather than hidden or “fixed” by
changing unrelated art. Expected failing-fixture diagnostics in test logs are
asserted test outcomes. Historical Prompt 8's aggregate was originally 187
passes/11 locked-display skips, followed by a successful focused native rerun of
26 passes/0 skips; this audit's complete gate independently has zero skips.

Representative unmodified native captures: [calm square](winter-final-audit-evidence/calm-square.png),
[severe nearby lights](winter-final-audit-evidence/severe-near.png),
[sheltered severe interior](winter-final-audit-evidence/severe-interior.png), and
[Office after severe Winter](winter-final-audit-evidence/non-winter-after-severe.png).

Compact, committed inventories and exact results are in
[`winter-final-audit-evidence/`](winter-final-audit-evidence/). Full logs, raw
native images, frame/player/weather traces, rejected attempts and manifests are
in `debug-maps/winter-audit-20261007/evidence/` (local, Git-ignored).

## Preservation and launch paths

The new frozen archive contains matching sources, all assets/packages, player,
compiler and SDL binaries, and reviewed calm/severe launch settings. Run without
building or using `target`:

```sh
python3 debug-maps/winter-audit-20261007/evidence/launch.py winter
python3 debug-maps/winter-audit-20261007/evidence/launch.py blizzard_review --quality high
```

Existing `debug-maps/{snowfall-20261007,blizzard-20261007,winter-integration-20261007}`
review launchers remain under each `evidence/launch.py`. Compiler and movement
launchers remain at `debug-maps/compiler-audit-20261003/launch.py` and
`debug-maps/movement-audit-20261003/launch.py`; invoke either without a map argument
to list its saved fixtures. Their complete packages, sources, assets and binaries
are retained. The two external Consolidation and Office Refinement evidence
archives are intact. Their recorded SHA manifests, all prior winter manifests,
and the compiler/movement package/source collections were checked.

Before the authorized `cargo clean`, unique historical evidence under `target`
is preserved in `debug-maps/winter-audit-20261007/evidence/recovered-target/`, with
a relocation/hash manifest. Historical embedded path strings still refer to the
original location. Historical snapshot caches are conservatively retained with
their evidence. The final handoff records actual cleanup and post-clean launches.
Only this task's temporary `caffeinate -di` assertion is released at completion;
persistent power, display and lock settings are unchanged.

## Remaining limitations

- Certification uses this Mac/M2 Pro and native Metal; it adds no Linux/Windows
  runtime certification. Automated held controls do not replace exhaustive
  human exploration of every roof or input sequence.
- Snowfall shelter follows authored room/slab coverage. Arbitrary props,
  dynamic roofs and tree canopies do not provide traced shelter. The storm
  shader supports at most 32 shelter regions.
- Snow coverage is authored geometry; there is no dynamic accumulation. Aurora
  is a static PNG sky. Terrain remains the engine's existing planar/rectangular
  authoring system.
- Completed-frame measurements include the whole renderer and host pacing;
  they do not isolate weather GPU cost or prove an art-related speedup.
- The texture advisories and tiny-chart fallbacks above remain intentional or
  pre-existing limitations, not silent clean-pass claims.

Substantially changed files: `tools/verify.sh`, `.github/workflows/rust.yml`,
`tools/bench/capture_snowfall.py`, `tests/test_bench_metrics.py`,
`tests/test_weather.py`, `docs/VERIFICATION.md`, `docs/ASSET_SPECIFICATION.md`, this
report/compact evidence and `debug-maps/winter-audit-20261007/README.md`.
