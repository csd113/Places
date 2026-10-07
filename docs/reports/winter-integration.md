# Winter integration and art polish — Prompt 8

Baseline: `3ceb1fafd6d5e49f02448523aeae14a5b782e673`, branch
`Winter-expansion`. This pass authors Winter's composition and repairs visible
integration defects. No engine, asset catalog, texture, GLB, material, weather,
movement or non-Winter map source was changed. Prompt 9 remains queued.

## Art direction and changes

The actual pixels in
`assets/environment/winter/Winter Expansion Environment Concept Sheet.png`
were inspected before authoring. The reference's snowy evergreen silhouettes,
quiet pond, cold blue negative space, small amber destinations, timber buildings
and supported, selective snow informed the pass.

- Broke up the evergreen grid into smaller groves with wider position/height
  variation and intentional gaps. 61 trees replace 75; the forest resting place
  has clear space and one modest warm navigation beacon. Walking and building
  sightlines remain clear without spreading warm light through the whole grove.
- Replaced the square's broad packed-snow rectangle with a narrow walked spine
  and seating connector. Rotated the three native-scale path strings slightly,
  including their supporting posts and attached baked sources.
- Replaced four identical raised pond corner platforms with tapered dry shore
  shelves. Repositioned and varied supported shoreline drifts while preserving
  the existing west ice crossing, transparent ice and depth backing.
- Removed impossible sheltered accumulation from lodge porch rails, deck edges,
  window sills and the space beneath its sealed awning. Kept exposed roof,
  awning, stair-edge, rock, evergreen, pond rail and ground accumulation. Varied
  cottage base banks and reduced repeated lower rail snow.
- Separated and varied lodge awning icicle groups, preserving a clear doorway.
  Added real timber surrounds/mullions to the six glazed openings.
- Closed the raised lodge's visible sky slot below wall bases with tucked,
  non-solid concrete foundation skirts. Removed buried lodge wall drifts.

The severe review generator now receives the same composition while retaining
its original identity and exact Prompt 7 weather. Calm snowfall stays Winter's
default. Warm entrance/string/forest lights remain nearby landmarks in the
existing 5 m severe visibility; the trees, pond and buildings fade at distance.
Calm sky/aurora, human lighting and severe fog were reviewed together at all
three quality settings. No weather or lighting-engine redesign was introduced.

## Native before/after evidence

These are actual SDL3/wgpu Metal captures on the M2 Pro. Each pair uses the same
camera, timing, player binary and weather; pixels are unchanged.

| View | Before | After |
| --- | --- | --- |
| Square/path and silhouettes | [Before](winter-integration-evidence/before-square.png) | [After](winter-integration-evidence/after-square.png) |
| Lodge approach/sheltered snow | [Before](winter-integration-evidence/before-lodge.png) | [After](winter-integration-evidence/after-lodge.png) |
| Pond/shore composition | [Before](winter-integration-evidence/before-pond.png) | [After](winter-integration-evidence/after-pond.png) |
| Severe entrance landmark | [Before](winter-integration-evidence/before-blizzard-entrance.png) | [After](winter-integration-evidence/after-blizzard-entrance.png) |

Additional native views: [overview](winter-integration-evidence/final-overview.png),
[closed lodge foundation](winter-integration-evidence/final-lodge-foundation.png),
[forest resting place](winter-integration-evidence/final-forest-rest.png).
[Capture provenance](winter-integration-evidence/capture-index.json) and
[native route results](winter-integration-evidence/native-validation.json)
are committed with the images.

Both calm and severe High runs include 22 views and 11 actual held-control
walks: lodge/overhang, stairs, pond crossing, forest, ice coast, snow stop,
ice-wall stop, rock collision/retreat, railing collision/retreat and both
cottage approaches. All assertions passed. Representative square, lodge,
pond, forest resting place, cottage string and interior views also passed in
Medium and Low in both modes: 68 accepted scene/route views, plus two overview
views, ten paired comparison views and a severe live High→Low→Medium→High
cycle. Weather telemetry records no particle-buffer capacity growth.

## Geometry, collision and performance

Strict geometry checks on both final sources report **0 errors, 0 warnings**.
The original outdoor missing-wall/room-leak intent remains unchanged
(159/24 suppressed findings); no waiver was added. Mounting, GLB topology,
normal/winding, snow/rock/tree base preservation, UV budgets and string-light
attachment tests passed. New window/foundation details use separated planes
and tucked ends; snow additions remain non-solid.

Structural rooms, walls, stairs, ramps, guardrails, doors and original solid
voids are unchanged. Original rock/tree collision envelopes are retained;
tree instance positions/scales follow the authored grove changes. Clear
native traversal found no need for an engine collision change. The pond's
dry corner shelves are deliberately lowered to surrounding ground height;
the existing ice-entry step and slippery coast remain tested.

Placed prop count drops **347→304** and placed model triangles
**130,356→109,412** (16.1% less). Unique models drop 53→52. The single extra
static source is the forest beacon (53→54); string sources are transformed
with their cables. The compiler records 205 prepared prop groups:
Off 115 ranges/4,703 vertices/0 atlas pages; Medium and Full
111 ranges/3,655 vertices with 3 and 4 atlas pages respectively. Full previously
used 5 pages. These totals include repeated evergreens, snow caps, string
geometry, ice and aurora through the existing renderer/material paths.
[Composition metrics](winter-integration-evidence/composition.json).

The matched High native square records 364→345 prop draw calls and 312→293
visible total draws. Model texture residency drops 10,112→9,856 KiB; the Full
atlas drops 80→64 MiB resident. Geometry buffers drop 25,414,400→21,447,424
bytes. Existing concrete introduces one additional resolved material (15→16),
while the twelve static translucent draws and single 2048×1024 aurora sky
remain unchanged; no reflection pass is active in this view. These are
renderer counters for the same camera, separate from whole-frame timing.

Diagnostic scene/route runs preserve the 1,400/1,050/700 seed budget. Peak
submitted particles in calm High/Medium/Low are 218/146/101; severe peaks are
167/119/80. Maximum conservative projected quad coverage is
1.696%/.363%/.245% calm and 33.184%/23.659%/19.094% severe. Worst view p95 CPU
sync is 127/108/57 µs calm and 395/102/69 µs severe, including diagnostic
projection and concurrent host verification load. All runs retain zero particle
capacity growth. Coverage is measured before texture alpha/depth rejection,
not hardware fragments. No particle/material engine optimization was needed.

The unlocked completion runs the same square camera through calm and severe
High/Medium/Low sequentially, with VSync off, GPU completion, 120 warmup and
600 measured frames per run in on/off/off/on order. Each side/quality contains
1,200 raw measured frames. All 24 runs have positive geometry/draw counters,
correct Metal world/quality installation, 720 ready presentations each and
zero missed ready presentations. The console was unlocked at every phase
boundary. No other compiler/test campaign ran during timings.

| Weather | Quality | Off mean | On mean | Off median | On median | Mean delta | On p95 |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Calm | High | 8.102 ms | 8.112 ms | 8.043 ms | 8.072 ms | +0.010 ms | 8.665 ms |
| Calm | Medium | 11.636 ms | 9.934 ms | 8.424 ms | 8.216 ms | -1.702 ms | 16.636 ms |
| Calm | Low | 8.090 ms | 8.105 ms | 8.023 ms | 8.018 ms | +0.015 ms | 8.427 ms |
| Severe | High | 8.111 ms | 11.604 ms | 8.056 ms | 8.734 ms | +3.493 ms | 16.843 ms |
| Severe | Medium | 9.877 ms | 11.666 ms | 8.212 ms | 8.683 ms | +1.789 ms | 16.756 ms |
| Severe | Low | 9.848 ms | 9.890 ms | 8.127 ms | 8.243 ms | +0.041 ms | 16.614 ms |

Observed pacing/jitter creates alternating 8–9 and 16–17 ms groups in several
on and off runs. Calm Medium changes distribution across repetitions and
produces a negative mean delta; that is timing noise, not a weather speedup.
The severe mean increments are observed whole-frame differences, not isolated
shader costs. Medians, p95s, raw repetitions and positive presentation checks
are preserved so the distribution is visible.

The earlier before-art calm on means were 8.223/7.364/7.146 ms and medians
6.899/5.261/5.025 ms for High/Medium/Low. Those separate-time samples have
different timing distributions; no before/after art speedup is inferred. The
measured draw/memory reductions above remain direct resource observations.
[Completed-frame results](winter-integration-evidence/completed-frame-results.json),
[console checks](winter-integration-evidence/console-checks.jsonl) and
[performance data](winter-integration-evidence/performance.json).

## Validation and practical limits

- Formatter, locked workspace/all-target/all-feature check, strict Clippy in
  dev/release, release build and the exact required
  `cargo clippy --workspace --all-targets --all-features -- -D warnings`: exit 0.
- `cargo test --workspace`: 2,021 library plus 6 binary/integration passed;
  23 existing library tests ignored. The full gate's all-features suite also
  passes those counts. Explicit atlas planning and the two native Low lighting
  resource tests pass separately; no claim that all ignored tests ran.
- `sh tools/verify.sh`: exit 0. Its 198 Python tests contain 187 passes and
  **11 explicit skipped native presentation tests**, because the macOS console
  was locked. Geometry repair's 14, compiled-build's 10 and all other Python
  groups pass. Native bootstrap runs 26 with 15 passes/11 skips. Fixture
  duplicate-id/worker-failure messages are intentional rejection tests. The
  subsequent unlocked focused bootstrap rerun passes **all 26 tests with zero
  skips** in 150.902 s, resolving those eleven presentation gaps. The aggregate
  gate was not rerun; its original skip history is retained.
- The 18 focused Winter/snow/string/weather Python tests pass. Asset validation,
  166-model inventory, maintained generators, snow topology/budgets, new source
  generator check and patch whitespace checks pass.
- Final Winter and severe compiler builds, decoding, `verify --require-current`
  and strict geometry checks pass. Final Winter: 48,047,278 bytes;
  severe: 48,047,311 bytes, both 19 entries/67 dependencies. Final full build
  times are 143.865/124.151 s under concurrent verification load; the normal
  incremental Winter rebuild is 1.464 s with byte-identical output. These
  are observed build durations, not a controlled baking benchmark.
- Actual native launch, 68 scene/route views, additional comparisons/overviews,
  live quality cycle and both frozen launchers pass. The player binary matches
  the baseline exactly. All four non-Winter shipped packages remain
  byte-identical. The original 400-file Prompt 7 frozen archive and new
  393-file frozen payload pass their complete SHA-256 manifests.

[Verification counts and exact hashes](winter-integration-evidence/verification.json).
The initial locked-console gaps are resolved by the focused unlocked suite and
both accepted timing campaigns. Historical skips remain recorded accurately;
there are no outstanding display-dependent Prompt 8 validation cases.

This is a deliberate low-poly environment; the compiler's planar terrain and
rectangular patch format still produce stepped shore contours and packed-path
edges. This pass improves those contours without rewriting terrain. Existing
45 subtexel sliver diagnostics keep their vertex-lighting fallback; they were
not hidden or treated as failures. Snow/tree/rock variants remain the existing
library assets. Native route automation exercises actual game movement and
collision, but does not constitute a human keyboard playtest of every possible
position or roof. Roof tops are not newly made accessible. Performance numbers
are whole-frame timings including GPU completion with host jitter, not isolated
GPU timestamps. The sky boundary and severe visibility retain their existing design.

Two intermediate compiler runs were intentionally interrupted during geometry
iteration, and a sandboxed initial launch could not access a display. Those
logs are retained as failed/superseded attempts; all accepted builds/captures
use successful final native runs with display access.

## Reproduction and preservation

See [review instructions](../../debug-maps/winter-integration-20261007/README.md)
and `tools/bench/capture_winter_integration.py`. The ignored
`debug-maps/winter-integration-20261007/evidence/` archive preserves raw before
and final captures/telemetry/logs, exact source/packages, matching player,
compiler and SDL binaries, dependency assets and a SHA-256 preservation
manifest outside `target`. Its frozen launcher supports both modes.

Earlier snowfall/blizzard/movement/compiler debug collections and both external
Office/Consolidation evidence directories remain intact. The original Prompt 7
frozen launcher/payload are preserved; only its current review source is synced.
Shared target artifacts remain available for Prompt 9. Publication SHA, remote
confirmation and CI outcome are recorded in the final task handoff after commit.
