# Model lighting: root causes and correction

**Status: lighting correction verified — production defects reproduced and
corrected; native visual, package, automated and runtime gates passed. Publication
identity and exact-head CI are recorded separately in the final delivery record.**

This separate correction follows the completed seven-stage Art-style overhaul.
The original production baseline is `a1e7122bba5df549c2e82c16371bcc57db99e976`.
Production visual comparisons use the actual native SDL3/wgpu renderer on Apple
M2 Pro / Metal, 1280×720, FOV 60°, unchanged exposure, source geometry, materials
and textures. Focused GPU fixtures also exercise other resolutions and FOVs.

## Original defects and confirmed causes

### Hallows skeleton lighting changed with camera distance

The original seated `campfire_skeleton_1` in Lantern Hollow is at
`[-16.7009,0,-31.7478]`, yaw −150.0229°, playing its constant held
`pose_sit_chair`. Its GLB has no authored normals. The shader reconstructs facet
normals from screen derivatives of posed world positions, but the old absolute
squared-length threshold (`1e-12`) rejects valid derivatives as the camera gets
closer. It substitutes world-up normals, creating incorrectly bright regions.
The triggering variable is render-eye depth/resolution/FOV; player distance,
probe payload and animation pose are unchanged in the controlled reproduction.

The corrected shader rescales each derivative by its largest absolute component
before crossing, then safely normalizes the direction. Authored normals use the
same normalization. Only exact zero or collinear frames use the fallback. No
light, brightness floor, exposure, facing rule or material exception is changed.

The original native GPU twin fails at 512×288, FOV 45°, depth 0.125 m:
missing-normal RGB `[148,148,149]`, authored-normal `[88,88,89]`, with identical
incident lighting. Corrected twins pass across four transforms, two resolutions,
three FOVs and two depths; extreme, zero and collinear vectors remain finite.
The original 128×128 fixture could not reach the faulty threshold above its near
plane. CPU probe tests never exercised this fragment-normal branch.

### Corridor recovery rays falsely hit their own boundary

The Demo Home corridor's ceiling/wall teeth originate in recovery fill.
Ceiling chart 1006's maximum adjacent edge RGB-mean jump increases from
0.00004969 direct to 0.01390652 filtered to 0.15931628 filled. The fill traced
visibility from the physical point on the adjoining wall, where floating-point
cancellation creates a tiny positive hit on the boundary the ray is leaving.

Recovery now evaluates falloff at the same physical point and traces from the
receiver's existing safe ray origin, as direct transport already does. Air-probe
semantics and real blockers are preserved. Exact production-coordinate controls
fail before and pass after; a sealed blocker still receives zero. Solver
revision 17 invalidates stale bakes. Generic room tests missed the exact boundary
coordinates and the separate recovery stage.

### Kitchen lightmaps smeared real small shadows across panels

The actual cupboards, sink and refrigerator contain proud rails, pulls and
recesses; their directional contact shadows are legitimate. Three interacting
bake/reconstruction errors spread or invent broad dark patches:

- Centre plus four-corner agreement did not prove uniform footprint visibility.
  The original refrigerator returned zero even though an interior sample
  received RGB `[0.325395,0.296110,0.250554]`. Medium now integrates a bounded
  2×2 physical grid and High a 4×4 grid, retaining per-tap visibility and cosine.
- Triangle-centroid texture colour was mistaken for material identity. Two
  coplanar triangles of one textured refrigerator primitive were separated at
  their geometric diagonal. Scene-local source-primitive identity now preserves
  continuity within that material, while real material, gap, plane, shading-
  normal and transmission boundaries remain guarded.
- Eight High intervals/metre gave 125 mm footprints around 12 mm rails and
  30 mm strips. Small opaque props now use High 32 / Medium 24 intervals/metre;
  large architectural and cutout contracts retain their original sampling.
  A 4 mm physical oracle on 284 visible samples per panel shows decreasing
  reconstruction error at 8/16/32 density: cabinet 0.036505/0.025410/0.014363,
  sink 0.037496/0.027042/0.014212, refrigerator 0.018581/0.010987/0.005147.
  The cabinet/sink oracle itself is bit-identical before and after.

Compatible coplanar source triangles share illumination charts while preserving
the original source diagonal and all six corners. Unsupported shapes retain the
triangle path. Independent native preparation checks preserve 1,242,369 ordered
corners per quality profile, including source attributes, bounds, draw ranges and
casters; architectural frames, dimensions and sampling pitches are unchanged.
Charts decrease from 126,624 to 78,818 in Demo and 245,495 to 191,610 in Hallows.

The planner applies the exact typed byte budget to the actual contribution-group
count before emitting charts. Full permits up to eleven pages; Demo's two groups
fit ten pages under the unchanged 320 MiB +64 KiB bound. Hallows uses eleven base
pages (176 MiB, previously 160 MiB); Medium uses eight. No codec guard, gutter or
sampling quality is weakened. Geometry revision 8 and cache format 14 invalidate
old preparations; legacy folded-triangle readers and explicit quad topology are
covered by regressions. Earlier simplified fixtures lacked these real rails,
textured diagonals and narrow interior lighting openings.

## Native visual verification

An independent specialist inspected 210 integrated native captures: 42 static,
102 skeleton, 30 supplemental and 36 quality. All original target cases pass:

- Camera-only 12 → 0.4 m approach and retreat, fixed-eye player movement,
  orientation-only controls and ordinary crouch/controller movement preserve
  directional skeleton shading. The collision-limited approach reaches an
  actual 0.600017 m. Its selected skull region's erroneous world-up fallback
  count decreases from 12,908 to zero. Eight retreat pairs and all seventeen
  fixed-eye player normal images match exactly; the incident target payload is
  constant throughout each controlled sequence.
- Demo and Hallows cupboard/sink/refrigerator close-ups and both Demo corridor
  legs pass at Medium and High, with Low as a reference. Actual Home Showcase
  cabinet/sink and unoccluded refrigerator doors also pass. Recesses, handles,
  rails, grounding and directional shadows remain visible.
- Actual animated/skinned cat, movable washer/door, indirect/dark rooms and
  nighttime controls pass. All 268 non-level assets and 27 supplemental target
  transforms match the originals.
- Low → Medium → High → Low → High restoration passes at the Demo kitchen and
  original close skeleton. All sixteen same-frame/profile High-only and
  Low-only image comparisons are byte-exact, with matching capture state.

The final normal release player repeats eleven static and seventeen held-skeleton
captures byte-exact against accepted diagnostic-player final composition. All 48
supported packages compile, verify currentness, decode and load natively; all 49
positive/named-negative geometry controls retain their expected results. A second
specialist independently inspects all 48 current native views and paired originals.
Forty-three broad pairs share the original source/view/settings/catalogue; three
intentional camera changes and two earlier source repairs are qualified. Default
spawn animation phases are not pinned, so broad pixel differences are not claimed
as deterministic target comparisons. No new fallback appears; the existing
`capacity_dense` Medium/Full page overflow remains explicit.

Original and corrected baked-light diagnostics use the same encoding; an earlier
receipt's inaccurate description was corrected after inspecting both frozen
binaries. Both final-composition and baked-light PNGs are comparable. The images
below are untouched native captures copied byte for byte into this curated set.

| Original production case | Before | After |
| --- | --- | --- |
| Hallows skeleton, 12 m | [PNG](images/model-lighting/hallows-far-before.png) | [PNG](images/model-lighting/hallows-far-after.png) |
| Hallows skeleton, 0.6 m | [PNG](images/model-lighting/hallows-close-before.png) | [PNG](images/model-lighting/hallows-close-after.png) |
| Demo cabinet/sink close-up | [PNG](images/model-lighting/demo-cabinet-sink-before.png) | [PNG](images/model-lighting/demo-cabinet-sink-after.png) |
| Demo kitchen | [PNG](images/model-lighting/demo-kitchen-before.png) | [PNG](images/model-lighting/demo-kitchen-after.png) |
| Hallows cabinet/sink | [PNG](images/model-lighting/hallows-cabinet-sink-before.png) | [PNG](images/model-lighting/hallows-cabinet-sink-after.png) |
| Hallows refrigerator | [PNG](images/model-lighting/hallows-refrigerator-before.png) | [PNG](images/model-lighting/hallows-refrigerator-after.png) |
| Demo corridor, north | [PNG](images/model-lighting/demo-corridor-north-before.png) | [PNG](images/model-lighting/demo-corridor-north-after.png) |
| Demo corridor, south | [PNG](images/model-lighting/demo-corridor-south-before.png) | [PNG](images/model-lighting/demo-corridor-south-after.png) |
| Home Showcase cabinet/sink | [PNG](images/model-lighting/home-showcase-cabinet-sink-before.png) | [PNG](images/model-lighting/home-showcase-cabinet-sink-after.png) |
| Home Showcase refrigerator | [PNG](images/model-lighting/home-showcase-refrigerator-before.png) | [PNG](images/model-lighting/home-showcase-refrigerator-after.png) |

## Repeatable controls and regression protection

[Native replay instructions](../tests/fixtures/native/model-lighting/README.md)
retain the accepted production cameras, frame sequences, player/camera controls,
quality loop and commands. The opt-in benchmark pins delta and records actual
render eye, player eye, angles, FOV, ready frame and simulation time. These
controls are inert during ordinary gameplay. Capture output must be new and
lives outside published docs; original baseline images are never overwritten.

Focused regressions cover near/far GPU twins, finite normal math, exact corridor
boundary/support visibility, sealed blockers, real refrigerator material and
narrow-slot coverage, source-prop oracle grids, source-attribute preservation,
quad topology/legacy decoding, typed page limits and cache-key consistency.
Actual compiled-environment GPU tests also trace probe → CPU payload → GPU
uniform → directional pixel and exercise quality/resource restoration.

| Final local gate | Result |
| --- | --- |
| `cargo fmt --all --check` | Pass |
| `cargo check --locked --workspace --all-targets --all-features` | Pass |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Pass |
| Strict release workspace/all-target/all-feature Clippy | Pass |
| `cargo test --workspace` | Pass, 2,197 library tests plus all integration/compiler targets; 32 ignored |
| Six ignored native entity-lighting GPU regressions | Pass, 6/6 |
| Complete Python discovery, including native smoke and packaging | Pass, 292/292 |
| Native Low/Medium/High resource override regressions | Pass, 2/2 |
| Real bundled static-model atlas planning | Pass |
| Normal and diagnostic release compiler/player builds | Pass |
| Demo, Hallows and actual Home Showcase currentness/full record validation | Pass |
| Complete supported package and native campaigns | Pass, 48/48 supported maps and 49 geometry controls |
| Independent final source/diff review | Pass |
| Quiet runtime comparison | Pass, four paired native views with presentation verified |

Earlier complete workspace attempts exposed stale page/topology/chart expectations.
Their updated checks preserve out-of-range rejection, legacy topology and the real
dense record's old-cap rejection, adding explicit quad and exact current Hallows
inventory checks. The final full rerun passes; earlier failures are not
claimed as passes. The Python dump inspector now accepts page 10 and rejects 11,
with edge/gutter/overlap tests on the last valid page.

## Costs and limits

Full cold production bakes take 562.666 s (Demo), 506.804 s (Hallows), 37.714 s
(Home Showcase) on this host. Demo peaks at 3.98 GiB measured RSS. The fixed High
coverage grid costs at most sixteen transport evaluations versus the former
uniform five; Medium uses four. These are offline compiler costs. This mission
has no valid matched original cold-bake timing and makes no compiler-speed claim.
Four paired High views use matching source cameras, assets and settings, with
GPU drain and a presentation trace, with VSync off and a 640×360 logical window
confirmed as a 1280×720 Retina drawable. All corrected intervals and three
complete original intervals follow 120 actually presented warmup frames and
contain 900 continuously presented frames. The original kitchen retains 891
frames, excluding nine trace-proven unavailable frames; no timing outliers are
discarded.

| View | CPU update mean, before → after (ms) | Renderer mean (ms) | Renderer median (ms) | Peak RSS, before → after (MiB) |
| --- | --- | --- | --- | --- |
| Demo kitchen | 8.818 → 8.866 | 3.227 → 3.348 | 3.165 → 3.248 | 2661.6 → 2663.6 |
| Demo corridor | 8.872 → 8.849 | 2.830 → 2.841 | 2.787 → 2.752 | 2290.9 → 1915.5 |
| Hallows far skeleton | 4.135 → 4.080 | 4.819 → 4.571 | 3.755 → 4.026 | 1269.1 → 1357.1 |
| Hallows campfire | 4.121 → 4.075 | 4.623 → 4.384 | 3.821 → 3.998 | 1270.9 → 1332.6 |

Draw calls remain 258/195/103/63 for these four views, and index storage is
unchanged. Demo VBO storage decreases 29,932,144→28,368,824 B and Hallows
56,547,800→55,610,568 B. The largest renderer median increase is 0.271 ms
(7.2%) in Hallows; CPU updates remain comparable. Hallows adds one 16 MiB GPU
atlas page. Its measured process peak rises by up to 88.0 MiB; peak RSS does not
isolate allocation causes or demonstrate a sustained plateau. All allocations
remain bounded by fixed atlas, chart and quality contracts. These single runs
include tracing and measure GPU-drained completion wall time, not GPU timestamps
or gameplay FPS. Timing variability prevents a speedup claim.

Earlier occluded runs were rejected. Nonzero draw counters alone do not prove
presentation because they can remain stale after skipped acquisition. With the
desktop available, normal accessibility focus/raise of the verified owned game
window restored continuous presentation for the corridor and both Hallows views.
Existing accessibility authorization was used; no security, permission or
persistent OS settings were changed. No rejected interval contributes to this table.

Restored Low disables atlas/probe/reflection lighting but retains an inactive
CPU visibility cache: Demo 16 meshes/5,178 triangles, Hallows 8/2,592. That is not
an active lighting leak and does not prove a sustained memory plateau. The
original nighttime missing-decal checkerboards remain unrelated; optional door
footage is partly occluded. Physical Linux/Windows GPU results, gameplay FPS and
sustained memory plateau are unclaimed. Publication uses the existing Art-style branch. Remote SHA and exact-head CI
are checked after commit and recorded with the final delivery. The matching
normal/diagnostic binaries, compiler, SDL library, all 48 packages, assets,
settings, curated PNG hashes, replay inputs and accepted verification summaries
are preserved outside `target` in the local `integrated-runtime-v3` recovery
bundle. Historical and superseded corrective evidence is recoverable in genuine
macOS Trash as described in the [docs index](README.md). Reproducible Rust build
artifacts are cleaned only after publication, CI and recovery acceptance.
