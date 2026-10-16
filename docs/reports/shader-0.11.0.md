# Shader correctness and maintenance — 0.11.0 staging

Shader-stage baseline is `6011d92ab1cf6d7710ef631672109d3f55e90e6f`.
The completed renderer stage is not included in this stage's performance gains.
Implementation and acceptance below are separate from test removal, flashlight
close-up issue #23, console #22, sound #12 and caustics #19.

## Implemented corrections

- **Normal transforms:** a small absolute determinant was treated as singular.
  A valid rotated model or animated joint at scale `1e-5` consequently received
  identity or zero normals. One shared inverse-transpose helper preserves the
  exact ordinary path and rescales the finite linear matrix before exceptional
  inversion. Placement, skinning and diagnostic snapshots now agree; each keeps
  its existing singular fallback. Tiny retained skin-weight totals also work.
- **Fluorescence bindings:** untextured static models with equal local mask
  index zero could reuse another model's material/mask. A named material identity
  includes the validated resolved fluorescence resource slot. Missing or invalid
  local indices cannot borrow the next model's image. Identical masked models in
  separate spatial batches may retain separate material bind groups, consistent
  with the existing base/emission slot policy; cached image allocations still share.
- **Raw presentation:** non-sRGB window fallback decoded already encoded display
  RGB without a hardware re-encode. The final pipeline selects the existing raw
  copy for raw surfaces. An explicit feature-only native probe exercises actual
  production copies on the player's device: both raw RGBA/BGRA controls change
  `[55,13,133,255]` to `[128,64,191,255]`; both sRGB controls retain the latter
  exactly. This reproduces the former entry selection, not an old executable's
  physical raw window. Normal Metal windows use sRGB.
- **Fragment maintenance:** prepare authored/geometric normals once and reuse
  them for material lighting and caustic selection. Derivatives remain outside
  conditional normal preparation; visible-side material normals and original
  geometric receiver orientation stay distinct. Exact uniform zero-strength
  gates skip sheen powers/reflection samples; every nonzero response is retained.
- Correct stale float vertex-colour, HDR target, uniform-size, alpha, reflection
  readback and static/runtime lighting contracts. Direct benchmark execution now
  records the actual child PID/RSS and has no app packaging or automatic retry.

No textures, map/package contents, resolution, sample counts, material response,
light/fog intensity, depth/alpha policy or enabled effects were reduced.

## Inventory and profile

All ten production WGSL files were inspected: six vertex and 25 fragment entry
points, with no authored compute entry point. World concatenates storm, attached
lights, world and caustics helpers; decals shares storm/attached-light definitions.
Sky, effects, post and UI are independent modules; diagnostics is feature-only
world source injection. Existing backend query operations create internal compute
pipelines; those are not omitted authored lighting shaders.

World has five binding groups: camera; base sheet; material/normal/emission/UV;
environment/lightmaps/reflections/caustics; attached lights/flashlight visibility.
The 88-byte vertex stride, 80-byte camera, 112-byte material, 6,240-byte environment,
400-byte attached-light record and storage/node strides agree with host layouts.
Lightmaps, scene, reflection, emission and blur remain linear HDR; colour textures
sample sRGB, masks/normals stay numeric, HUD/presented values stay encoded display
RGB. Review covered Low/Full gates, spatial entity lighting/UV response, shadow
coverage, reflected transforms, fog, sampling/LOD, storage bounds and invalid data.
Detailed entry/binding/layout inventories and source hashes are preserved locally.

A Metal System Trace attached to the directly launched baseline identifies scene
fragment work as the dominant measured GPU pass cost in the close-door view: a stable five-second window contains
600 scene encoders, approximately 3,948.74 ms of fragment intervals and 168.70 ms
of vertex intervals. Scene fragment work is about 6.58 ms per encoder versus
0.28 ms vertex work. Planar fragment work is about 0.36 ms, emission 0.16 ms,
resolve 0.21 ms and the final copy 0.13 ms. Channels overlap: these are not additive
frame costs. The trace separates geometry from fragment and reflection/post work;
it does not isolate overdraw, texture stalls and arithmetic within fragments.
No claim assigns all pixel cost to lighting arithmetic. The normal consolidation
is a bounded removal of demonstrably repeated work, not a shader rewrite claim.

## Native acceptance

An immutable before bundle contains the accepted default player, source archive,
all ancillary assets and real copied Arcade/Office/Pool packages. Its 963-file
SHA-256 manifest was reverified; there are no symlinks to mutable assets. The
accepted repaired Office package is the shader baseline. Default and diagnostic
candidate binaries are retained separately outside `target`.

Every native player was executed directly with explicit repository working
directory, assets and isolated state. No `.app` or LaunchServices wrapper, TCC
reset, security workaround or identity change was used. The sandbox exposed no displays; normal
escalation enabled native Metal. Instruments' initial direct launch of the baseline
(PID 37985) requested Documents access at 08:31 local time; its 15-second trace
ended with the target still awaiting permission and contains no target GPU work.
The user's Allow response is logged at 08:58, after that target had exited; no
duplicate or resumed old launch is claimed. That trace is excluded. Profiler
attachment to an already directly launched player succeeded.

Arcade door, Office counter/normal view, Pool transparency/caustics basin, independent
Low lighting and fixed-simulation Office flashlight captures are PNG-byte and
pixel identical before/after at 1512×838. Package, settings, camera, simulation,
drawable and geometry match. A wall-clock flashlight pilot reached different
mannequin poses and was excluded; fixed input timing resolves that confound.
Rare normal-scale/mask identity fixes have targeted CPU evidence rather than a
claim that these unchanged shipped views demonstrate their boundary cases.

A genuine four-second native Pool video records 120 source frames at 1280×720,
30 FPS (ready frames 1–239, stride two). Sampled frames and free/held/thrown/moving
captures show the intact duck and water. All 294 physics records are finite,
30 are held, and recovery count stays zero. Reset leaves two dormant bodies and
reactivation resumes them; this is not body-removal proof or every-frame QA.

The 63.40-second diagnostic transition campaign covers texture quality, lower
lighting, Arcade↔Pool, same-ID reload, Office and resize. Several rapid Full and
Low-lighting requests were superseded before their own install; those demonstrate
cancellation, not completed state transitions. There were 61 ready-state
nonpresentations in a roughly 1.1-second transient gap and one loading miss;
rendering recovered, with no logged fatal/validation failure. The gap is not
attributed to a confirmed shader defect or described as zero skipped frames.
Across 390 resource records there is no fatal state. Late periodic frame samples 6000–6600
retain 142 textures, 1,311 bind groups, 56 render pipelines, 22 samplers and 116
persistent cache entries; buffers vary 677–682 and views 158–160, staging clears.
This is bounded non-accumulation evidence, not a long-term leak guarantee. RSS
samples varied during concurrent build/test work and are not a savings or settled
memory claim; Metal HAL allocation-byte fields remain unavailable. A subsequent
23.15-second isolated control captures fully installed independent Low lighting,
Full lighting, Static lightmaps, Full lightmaps and explicit same-map reload.
All five actual capture states match their intended settings and have zero
ready-state misses; this does not erase the longer campaign's transient gap.

## Matched throughput

Both default executables ran directly, High textures/filtering, Full lightmaps
and reflections, bloom, FOV60, VSync off/Immediate, swap interval zero and no
engine FPS cap. Three interleaved 600-frame runs per side/view follow identical
120-ready-frame warmups and fixed 1/60 simulation. Pair order reverses in trial
two; two seconds between process exits/launches allows desktop state to settle.
All 24 accepted runs exit zero with 720 ready presentations and no ready misses.
Binary/package hashes, settings, camera/drawable, movement and per-frame geometry
match. CSV player samples use global-frame cadence, so their extrema differ by
sampling phase after variable load lengths. Reconstructing each sampled position
against the same ready-world movement schedule gives maximum Z error 1.24e-6 m
for all twelve lateral runs; this is recorded separately from sampled extrema.

| View / physical drawable | Before average FPS | After average FPS | Change |
| --- | ---: | ---: | ---: |
| Close closed door, 3024×1898 | 119.99 | 120.00 | +0.00% |
| Arcade cabinets, 3024×1898 | 119.98 | 119.99 | +0.01% |
| Bathroom lateral, 3024×1676 | 120.02 | 120.03 | +0.01% |
| Bathroom lateral, 1512×838 | 120.02 | 120.07 | +0.05% |

Average FPS is total measured frames divided by total loop wall time, pooling
1,800 frames per side/view. **No measurable FPS improvement is established.**
Despite uncapped Immediate settings, this direct-launch environment is paced near
120 FPS across substantially different GPU workloads; its cause is not isolated.
Do not compare this ceiling with historical wrapped renderer-stage throughput or
attribute those earlier gains to shaders. Internal GPU spans vary with scheduling
and are not substituted for a throughput gain. A separate single candidate Metal
trace has scene-fragment mean about 6.40 ms versus baseline 6.58 ms; that bounded
instrumented observation is insufficient for a general GPU speedup claim.

An initial candidate fullscreen attempt returned 3024×1676 instead of 3024×1898
and was rejected, including its unmatched pair. No smaller drawable is counted
as a shader improvement. The accepted rerun enforces each requested drawable.
Pre-campaign thermal/performance queries were unavailable; the post-campaign
query reports no recorded warnings. Temperatures were not controlled.
Permission waits, trace runs, builds and other native controls are excluded from
the accepted timing campaign.

## Verification and limits

Strict workspace/all-target/all-feature Clippy passed in debug and release.
Default workspace tests pass: 2,344 library plus seven integration, 19 existing
ignores. All-feature tests pass: 2,351 library plus seven integration, same ignores.
Python source contracts: 372 run, four optional skips. Four new owning-subsystem
CPU tests protect distinct transform, skinning, mask and raw-format boundaries;
they require no GPU, asset decode or map bake. Existing tests remain intact.
Independent source, actual native visual/probe/moving-prop, timing, motion-witness
and resource review found no unresolved correctness issue. The reviewer
independently recomputed the timing averages, geometry matches and sampled motion.

Native platform coverage is Apple M2 Pro/Metal only. Raw fallback was exercised as
an offscreen format control, not a physical non-sRGB display. No forced physical
device loss or other-platform native execution is claimed. Flashlight/contact
shadow equations and quality remain unchanged; issue #23 is not resolved here.
Evidence and queue notes are under `local/shader-0.11.0/`.
