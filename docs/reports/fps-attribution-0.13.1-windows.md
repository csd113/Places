# Windows FPS attribution — 0.13.1

Measured 2026-10-29 on the Razer Ryzen 9 5900HX / RTX 3070 Laptop GPU,
NVIDIA driver 617.14 (32.0.16.1714), Dx12. This identifies expensive work;
it implements no rendering optimization or quality reduction. Native Mac
validation/performance is **unverified and deliberately deferred by Connor
until requested later**. This is not cross-platform acceptance or a release.

## Ranked demonstrated GPU costs

These are **rendering-removal diagnostics**, not achievable quality-preserving
optimization gains. Positive values mean work removed. Each delta is calculated
within its matched pair, then the median and full between-pair range are reported.
Ranges are not statistical confidence intervals. CPU and GPU times overlap and
must not be added.

| Rank / controlled change | Qualified pairs | GPU elapsed mean removed, ms: median [range] | Whole-loop mean removed, ms: median [range] | Observed FPS added: paired median [range] |
| --- | ---: | ---: | ---: | ---: |
| 1. Hide Hollow `tree_01`, `tree_02`, `tree_03` model draws | 3 | **13.335 [12.746, 13.821]** | **9.861 [9.476, 9.869]** | **67.7 [67.6, 79.0]** |
| 2. Hide Arcade cabinet models in the close cabinet view | 3 | **6.114 [5.628, 6.471]** | **4.706 [4.152, 5.363]** | **84.3 [84.2, 90.8]** |
| 3. Reflections Full → Off in Arcade's wide view | 2 | **1.646 [1.116, 2.175]** | **0.776 [0.588, 0.964]** | **9.5 [6.2, 12.8]** |

The first two rankings are consistent across all three pairs. The reflection
result has lower confidence: two qualified pairs, variable magnitude, and worse
whole-loop tails with the effect disabled. Its third attempted pair failed
foreground verification and is excluded; the campaign was closed instead of
extending small-effect investigation indefinitely.

The tree group is `environments/outdoor/props/tree_*.glb`, specifically the three
catalog assets `outdoor:tree_01`, `outdoor:tree_02`, `outdoor:tree_03` (247 authored
placements, not all visible). Submitted base-scene draws fall **569 → 489** and
triangles **1,393,652 → 153,138**. Other props and source lighting/physics remain.
This is a net full-scene marginal cost, including newly exposed surfaces, not
isolated per-tree time or proof of a particular vertex/overdraw bottleneck.

The cabinet group is the five `cabinet_action`, `cabinet_fighting`,
`cabinet_racing`, `cabinet_scifi`, `cabinet_shooter` GLBs under
`environments/arcade/props/`. There are 36 authored cabinets. In the close view,
base-scene draws fall **116 → 66**, triangles **81,383 → 11,445**. In the separate
wide screening view, hiding cabinets also produced a large change. This
implicates the visible cabinet render workload, not the largest file or polygon
count in the project. It includes material/texture work, visibility, rasterization
and reflected/emissive duplicates; those components are not independently priced.

Arcade's `arcade:lane_wood_01` material requests planar reflection. The wide view
submits one live reflection pass; Reflections Off removes it while base-scene
draws/triangles stay **271 / 123,373**. The control disables the reflection
category, not only one wood texture. Source material/routing and the view identify
the lane capture as the relevant surface context; this is not a causal test of
that PNG's intrinsic sampling cost. The diagnostic wide-view planar span is
2.904 ms, versus a 5.984 ms scene span. These timeline spans can overlap/include
scheduling and are not additive or equal to the isolated A/B saving.

## Frame distributions and bottleneck changes

Values below are medians of run-level statistics. A difference between these
medians need not equal the median paired delta above. FPS uses 1000 / mean
whole-loop milliseconds, not reciprocal median frame time.

| Scene / control | Average FPS | Loop median / p95 / p99, ms | CPU update / render mean, ms | GPU elapsed mean, ms |
| --- | ---: | ---: | ---: | ---: |
| Hollow Full | 57.2 | 17.867 / 22.314 / 24.003 | 4.540 / 2.199 | 16.639 |
| Hollow, tree draws hidden | 124.7 | 7.860 / 9.326 / 10.994 | 4.639 / 2.416 | 3.314 |
| Arcade close Full | 98.2 | 10.139 / 12.184 / 13.066 | 1.375 / 1.782 | 9.557 |
| Arcade close, cabinets hidden | 183.3 | 5.397 / 8.234 / 9.326 | 1.320 / 1.600 | 3.443 |
| Arcade wide Full | 104.1 | 9.636 / 12.192 / 12.549 | 1.341 / 2.535 | 9.039 |
| Arcade wide, reflections off | 113.6 | 8.313 / 14.874 / 15.958 | 1.405 / 1.632 | 7.393 |

Hollow changes bottleneck when tree draws disappear: GPU time drops sharply,
while CPU update remains about 4.6 ms and render preparation/submission remains.
The frame therefore settles near 8 ms, not near the remaining GPU duration.
The reflection control improves mean work/FPS but **does not improve p95/p99**.
Presentation/scheduling remains material; disabling the effect is not an accepted
tail fix or a proposed shipping default.

Four separate, logging-enabled diagnostic runs are excluded from FPS acceptance.
Each retains three explicit CPU stage samples and five GPU pass samples within
its measured CSV window. Hollow character-lighting medians are **3.537 ms Full /
3.436 ms with tree draws hidden**; skin upload is **0.824 / 0.767 ms**. This is
direct stage evidence for a later CPU target, not an independently measured FPS
gain from removing characters. Dynamic lighting is about 0.095 / 0.077 ms.
The tree-hidden diagnostic still encodes 489 base draws; render encode is about
1.060 ms, queue submit 0.186 ms. The full view's encode/submit are 1.183/0.109 ms.
These few-frame stage medians are descriptive and must not be summed into an
authoritative frame budget.

In the last 600 diagnostic physics rows, Hollow has **16 dormant / zero active
bodies**, no substeps, mean physics step **0.0039 ms**, item update **0.0093 ms**.
Arcade has **100 dormant / zero active**, no substeps, step **0.0151–0.0157 ms**,
item update about **0.0355 ms**. Idle rigid bodies do not explain these views.
Active bowling, throwing, swimming and populated moving-body stress are not
measured here. Normal flashlight is off; no flashlight-shadow benchmark or
independent shadow-AO coefficient measurement is claimed. Full lighting uses the
prepared package layers; offline baking is outside every timed interval.

## Lower-priority screening and unresolved splits

The foreground-qualified screening uses 600 warm-up / 1,800 measured frames,
separate from the repeated confirmation cohort. These are single-run signals,
not promoted to reliable gains:

- **Backrooms diagonal:** housing removal changes 631 → 200 base draws, with
  observed GPU median 5.368 → 4.756 ms and FPS 163.9 → 177.6. This is much less
  GPU leverage than the tree/cabinet groups despite many more removed draws.
  The baseline is close to 165 Hz; uncapped throughput improvement for this
  screening pair remains inconclusive. Disabling occlusion changes 631 → 731
  draws but gives no resolved FPS benefit. Keep existing culling on this evidence.
- **Pool basin:** optics fallback changes observed GPU mean about 7.001 →
  6.262 ms and FPS 125.0 → 134.8. Caustics Off gives about 6.491 ms / 130.1 FPS.
  Those small differences are not separated from run variation by repeated
  confirmation. Pool water is not demonstrated to be the leading cost in this
  campaign; this does not prove it is cheap in every camera or active interaction.
- **Hollow grass:** 569 → 516 draws, GPU median 17.260 → 16.915 ms in screening;
  smaller than the tree group and not confirmed. The qualified rock-group retry
  is also a smaller, unreplicated signal. Do not replace grass/rock assets first.
- **Arcade bloom/filtering:** turning bloom off or reducing filtering 16x → 4x
  while keeping texture residency High produces no large resolved improvement.
  Steady diagnostic bloom blur spans are about 0.05 / 0.025 ms; emission is about
  0.03 ms. Large bright artwork is not evidence of a large bloom cost.
- The existing blacklight switch removes the four-source Cosmic circuit in the
  cabinet view, with only a small single-run change. Low-lighting/Static controls
  change multiple rendering/preparation contracts together; their large changes
  cannot be labelled a pure lightmap, circuit, material or shadow cost.
- Hash-labelled, equal-length WGSL ablations in copied executables separately
  omit the eight-source entity-direct loop, UV response, or spatial entity light.
  Qualified probes do not account for the large cabinet/tree draw-removal cost.
  They do not establish an optimization to those shader paths. The Pool direct
  ablation fails foreground qualification and is excluded. Copies live only in
  `DIAGNOSTIC-NOT-FOR-DISTRIBUTION/`; original executable/source/package bytes
  are unchanged. Native shader/pipeline validation passed for the used copies.
- One exact pin-model selector was a no-op because material overrides suffix the
  runtime model identity. It is excluded from attribution; no claim that pins
  are free follows from that attempt. The initial Office camera was below its
  elevated floor; the corrected visual witness is retained, but its timing
  attempt fails foreground qualification and is not a performance result.

A single matched Hollow 1280×720 control preserves 1,393,652 submitted triangles:
GPU mean **16.395 → 11.815 ms**, FPS **58.3 → 78.7**. Pixel reduction helps, but
does not explain the whole tree-group cost. It does not separately quantify
vertex processing, alpha overdraw, texture bandwidth or shader arithmetic.

## Reproduction, identities and limitations

All accepted timing uses the actual native game at **Normal** priority, serially,
with no concurrent build/bake. Balanced power plan and host GPU/iGPU/security
configuration are unchanged. Natural clocks varied; they were not locked.
These are results for this Razer, not hardware-general or peak-device claims.

Confirmation settings: 1920×1080 **physical drawable**, windowed, FOV 60, High
texture residency, High/16x filtering, Full lightmaps, Full reflections, bloom on,
V-sync off and no engine frame limiter. Only the named control differs.
Actual presentation is **Immediate**, swap interval 0, normal DX12 Wait and
queue-depth hint 2. Each confirmation has 1,800 warm-up and 1,200 measured frames,
fixed 1/60 simulation delta and real wall-clock timing. The fixed simulation step
is not a render-rate cap. Model controls exceed the observed 165 Hz display
(cabinet-hidden median average 183.3 FPS); qualified Pool 720p screening exceeds
209 FPS. Requested Immediate alone is not used as proof of uncapped operation.
GPU-heavy full views respond to workload; Backrooms' near-refresh screening is
explicitly inconclusive as above.

The host ownership sampler checks the native PID every 250 ms during CSV growth,
excluding normal post-CSV window shutdown. Accepted periods have at least 99%
foreground ownership (the retained confirmation runs are 100%). Native activation
and an unbound F12 input occur only during warm-up, never as timed gameplay.
Background pilots, partial/failed focus, interrupted controllers and bad cameras
remain in the audit, excluded from the ranking. One duplicated successful
round-0 tree replay is excluded: the **earliest** qualified run is retained,
not the fastest. Cameras and paired conditions are recorded in each `case.json`.

| View | Render eye X,Y,Z | Yaw,pitch degrees |
| --- | --- | --- |
| Hollow street/forest | 18,1.6,21.6 | -35,-2 |
| Arcade close cabinets | 7.4,1.6,17.8 | 306,5 |
| Arcade wide / lanes | 6.8,1.6,28 | 25,-2 |
| Backrooms long diagonal | 65,1.6,62 | 135,0 |
| Pool basin | 2.7,1.6,2.2 | 140,-12 |

The current prepared packages are reused from phase 9; no source map, asset or
shipping default changes and no map is rebaked. Binary SHA-256:
`54937a8c4d9a618c8c790c9c5e1a6a0d60ab4f5573646162fa8d987611d8bbc0`.
`environments.placesmap` SHA-256:
`749c9f3e45120d7c72416891dd5d4f5965739dc88b60b5827f06fb1e33ac28f5`;
`backrooms.placesmap`:
`2a240f79cfcb46a8c79c5cedc9cb5113b36d40f2209b8f529adf81584eb99897`.
Every accepted run verifies package member/variant identity and unchanged hashes,
real scene submissions, successful presentations and zero missed ready presents.
GPU values deduplicate asynchronous sample IDs. Hardware timestamps cover the
union of submitted pass spans, including scheduling stalls; they are not active
utilization, whole-device time, or aligned CPU-row measurements.

`local/verification-0.13.1-attribution/` preserves raw CSVs, logs, settings,
environments, process/foreground receipts, camera PNGs, plans, exact scripts,
executable-ablation provenance and hashes outside `target`. `primary-analysis.json`
contains all pairs/ranges; `diagnostic-analysis.json` contains stage evidence;
`validation-receipt.json` and `custody.json` index final validation/publication.
Its `README.md` describes replay with a new label and the tested same-user hidden
persistent supervisor. Ordinary tests never bake a production map.

This is a report-only repository change. Rust implementation, gameplay sources and assets are byte-identical to
accepted phase 9. Exact-source CI **40817629130** completed successfully, including
debug/release strict Clippy, CPU regressions and production build. The existing
15 benchmark-accounting Python regressions pass. No new project test, rebuild or
rebake is needed for this documentation. New commit/remote/CI observations,
whitespace verification, evidence preservation and final idle cleanup are in the
local custody receipt. Main/v0.13.0 and release publication remain unchanged.

## Authorized follow-on queue

These requests were collected during profiling. None changes this baseline.

1. **Cohesive UI pass:** replace the large NPC text overlay with subtitle-style
   speech; make loading screens minimal with “Loading level” and progress tied
   to real loader work; contain overflowing level titles and give level cards
   adequate, responsive spacing. Bottom-center one/two lines, speaker labels,
   subtle backgrounds and separate dialogue choices are design proposals, not
   independently specified requirements. Unknown loading totals need honest
   indeterminate/stage presentation, not timer-generated percentages.
2. **Game save/resume and checkpoints:** editor-placeable or script-triggered
   checkpoints, with save/resume explicitly enabled/disabled per level. This is
   game state/progression, distinct from editor autosave. Serialization and resume
   semantics remain implementation work.
3. **Automatically execute targeted optimization based on the verified findings**
   after the UI and save/checkpoint work. Use the bounded work item below; preserve
   visual quality and retain only reproducible gains.
4. **Hub wall signs in the editor:** expose the actual existing Hub sign system
   and appearance for placement/authoring. Inspect/reuse that implementation and
   its assets; do not invent a separate sign system.

Native Mac work stays deliberately deferred until Connor requests it. No release
ZIP upload, tag, main merge or public 0.13.1 release is authorized by this report.

### Bounded optimization work item

The measured removals are leverage envelopes, not promises of equal gains without
removing content. Start with these targets, in order of demonstrated opportunity:

| Target | Proposed quality-preserving work | Main correctness / visual risks |
| --- | --- | --- |
| Hollow's three tree GLBs, 13.335 ms paired GPU cost | Inspect visible-instance/mesh-part work, geometry/raster/alpha cost and current conservative visibility/batching. Retain a measured reduction in redundant or provably hidden work. The resolution control shows a mixed cost; determine the relevant component before changing assets. | Missing canopy/silhouette, alpha edges, holes through vegetation, changed normals/lighting, incorrect occlusion. Do not hide trees, reduce texture quality or introduce a lower-quality default as the fix. |
| Arcade cabinet group, 6.114 ms paired GPU cost | Attribute main versus reflected/transparent/material work for the five actual cabinet assets, then remove proven redundant submissions, bindings or equivalent shader/resource work. The tested entity-direct/UV/spatial ablations do not explain the bulk cost; do not assume those are the fix. | CRT/side artwork and UVs, glass layering, emissive masks, material response, correct reflection and lighting. Preserve the existing PNG/alpha/aspect contracts. |
| Hollow character-lighting CPU floor, 3.4–3.5 ms observed stage | Inspect prepared lighting/visibility queries and safe reuse/invalidation for unchanged actors, sources and occluders. This becomes important after tree GPU work falls; it is not a promised full-scene FPS gain while GPU-bound. | Stale illumination/shadows after actor, door, circuit, light or occluder changes; incorrect skin/pose lighting. Preserve visibility and source semantics. |
| Arcade planar capture, 1.646 ms paired GPU mean cost, moderate confidence | Inspect actual reflective-range/capture visibility and duplicate preparation/submission. Establish a repeatable mean and tail improvement before retaining a change. | Missing reflected geometry/actors, stale switched-light/water radiance, incorrect capture selection. Do not silently lower resolution/update rate or disable reflections. |

Keep Backrooms fixture batching as a lower-priority candidate. Its many draw calls
do not establish comparable GPU leverage. Do not start with grass/rocks, bloom,
filtering, idle physics or a broad lighting rewrite based on the inconclusive
screening. Re-open a smaller category only when a concrete hypothesis needs it.

Acceptance for that future implementation:

- Preserve this binary/package/evidence set. After the preceding UI/save tasks,
  freeze a fresh pre-optimization baseline so their changes are not confounded
  with optimization gains. Compare the candidate against that baseline using the
  exact cameras/settings and actual physical drawable,
  actual presentation mode and foreground qualification. Disable V-sync and every
  limiter; a refresh-rate plateau remains inconclusive for throughput.
- Use at least three balanced, warmed pairs per targeted change, with the same
  animation/physics workload. Record average FPS, whole-loop median/p95/p99,
  CPU update/render/stage time, deduplicated GPU elapsed time, submissions and
  between-pair variation. Require a gain larger than observed variation; check
  tail behavior explicitly. Faster average FPS alone is insufficient.
- Validate native final-quality tree/cabinet/lanes views and relevant moving
  actors, alpha/translucency, circuit off/on, reflections and water lighting.
  Do not accept disappeared content, fallback lighting or lower visual settings.
- Follow the canonical map/asset contracts if assets or map inputs change. Run
  required strict Clippy/release gates and proportional existing/narrow regressions.
  Revalidate preparation currency where inputs change; keep any necessary bakes
  outside ordinary tests and separate from timed gameplay.
- Stop speculative branches that fail visual/correctness or repeatability checks.
  No quality-preserving gain for the proposed implementation has yet been measured.

Useful existing entry points for the queued work:

- NPC authors use `projects/environments/scripts/*.luau` → `api.message`
  (`src/engine/scripting/api.rs`) → `ShowMessage` (`src/engine/entities/mod.rs`).
  `src/game/app/mod.rs` receives it near line 1368, handles guidance input near
  2812 and renders it in `render_guidance` near 4273.
  `src/game/ui.rs::append_guidance`, `wrap_guidance`, `guidance_navigation` and
  `src/game/tests/guidance.rs` own the existing panel/navigation behavior.
  It also serves instructions, clears gameplay inputs and blocks item actions;
  subtitle work must deliberately separate NPC speech from generic guidance.
- Loading: `src/game/app/mod.rs::update_loading_progress` near 3657 reads real
  loader phase/decoded bytes and preparation/upload state; menu/status rendering
  is in `src/game/ui/menu.rs`. The same UI module owns level-grid layout/rendering.
- Existing game progress persistence: `src/game/campaign.rs`. It is not a
  complete gameplay checkpoint/resume implementation or editor autosave.
- Runtime cost owners: `src/engine/render/wgpu/renderer.rs` character-lighting
  update stages, `props/`, `world.rs`, `world.wgsl`, and
  `src/engine/render/common/reflections.rs`. Hub sign discovery is intentionally
  left to its later authoring task.
