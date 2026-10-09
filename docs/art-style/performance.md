# Stage 1 costs and measurement limits

Measured 2026-10-08 UTC on Mac 14,9 MacBook Pro: Apple M 2 Pro, 12 CPU cores
(8 performance + 4 efficiency), 19 GPU cores, 16 GB unified memory, macOS
27.0.1 (26A 434), native Metal. SDL window is 640×360 logical / 1280×720 Retina
drawable. VSync is Off/Immediate. This is one host with normal desktop applications
running, not an isolated laboratory or a Linux/Windows hardware result.

## Valid preparation and resource baseline

Forced compiler measurement:
all three ordinary variants, 12 worker cap, **4.872 s compiler / 4.89 s process
wall time**, 37.83 s accumulated CPU and **659.1 MiB maximum resident set**.
Here “cold” means an explicit forced solve with no prepared package reuse;
OS/file caches were warm. A subsequent reuse check takes
0.254 s compiler / 0.26 s process wall, 46.0 MiB maximum RSS and does not rebuild.
The forced rebuild reproduces the exact package bytes already used by captures.
The measured package is **3,873,651 bytes**.

The fixture has three rooms, 16 distinct static model resources, 5,300 static
model triangles, 14 architectural/fixture draws and 21 static prop draws at
upload, plus the spawned comparison chair. Upload counts are **not measured
visible frame draw counts**. Package preparation produces 5,347 charts, two
pages in both prepared profiles and a 9×5×11 / 495-slot irradiance lattice
(260 air-valid slots). Native Full logs report 167,960 chart texels inside
2,097,152 page texels; occupied chart count does not include every gutter.

| Ordinary quality | Scene / bloom targets | World texture cache receipt | Lightmap array | Hero room process max RSS |
| --- | --- | --- | --- | --- |
| High | 1280×720 / 320×180 | 53,127,840 B | 33,554,432 B | 468,975,616 B (447.3 MiB) |
| Medium | 640×360 / 160×90 | 17,476,256 B | 33,554,432 B | 394,149,888 B (375.9 MiB) |
| Low | 480×270 / 120×67 | 7,514,784 B | Off | 182,566,912 B (174.1 MiB) |

World cache receipts include the committed 1024² white fallback, so their maximum
edge is not a claim that Low surface sheets bypass quality budgets. Props,
normals, sky, framebuffers, reflection targets and CPU copies are separate costs;
these columns must not be summed as complete GPU memory. Static assets report
3,856 KiB decoded texture data. High uses one packaged 64 px reflection probe;
Medium uses its 48 px variant. This scene has no active planar surface. There
are no missing textures or failed static models. Package inspect
preserves exact record/dependency storage.

## Initial timing attempt that cannot be treated as scene performance

Five bounded native samples use 120 warmup + 360 recorded frames: High
room/hall/entities, Medium room and Low room. Raw CSVs/logs/manifests are under
performance; parsed receipts retain
all fields. They confirm the requested world and effective graphics settings.

However, **all frame draw/vertex counters are zero**. In this background desktop
automation session, the Metal window does not submit normal surface scene frames.
`Renderer::render_scene` records counters only after a successful acquisition;
`lib.rs::present_frame` yields for up to 16 ms when presentation is unavailable.
The approximately 17–18 ms loop and 0.15–0.21 ms render-submit values therefore
measure the unavailable-surface loop, not a 55 FPS hero or actual scene GPU cost.
No draw-call reduction, FPS improvement or GPU-time budget is claimed.

The native screenshot readback independently invokes the normal world/post scene
encoding, so the saved PNGs are genuine native scene renders despite this surface
limitation. Capture startup/wall latency is available in each capture manifest
(roughly two seconds per process here); it includes discovery, GPU initialization,
world load, the deliberate half-second wait and readback. It is not steady-frame
performance either. A later successfully submitting normal-build campaign now supplies the baseline
below; the original invalid rows remain preserved and excluded.

An initial timing attempt overlapped the active library test run and is retained
only under `/tmp/places-stage1-performance-shared-load`. It is excluded here.
The first wrongly labeled control attempt omitted `--quality low`; it actually
ran High, was moved to `/tmp/places-stage1-control-request-high`, and was replaced
with the genuine Low receipts. Neither discarded attempt is represented as a
quality/performance comparison.

## Submitted normal-engine baseline after integrated diagnostics

The initial unavailable-surface condition no longer occurred in the expanded
campaign. The preserved **normal release**, with diagnostics omitted, ran six
bounded samples: 120 warmup + 360 recorded frames each, sequentially without
builds/tests/bakes/native workers. **Every sample has 360/360 nonzero scene-draw
frames**. Exact commands,
raw CSV/log/manifest files and
parsed distributions retain provenance.
VSync is requested Off; the initial adapter receipt says Fifo, followed by the
actual settings application log `[wgpu] present mode Immediate (immediate)`.

| Normal quality / fixed view | Median render CPU ms | Median / p95 frame CPU ms | Median / p95 event loop ms | Accounted scene draws / distinct range vertices | Process peak RSS |
| --- | --- | --- | --- | --- | --- |
| High / entities | 7.130 | 7.387 / 13.712 | 7.771 / 14.294 | 17 / 11100 | 431.5 MiB |
| High / hall | 7.140 | 7.358 / 14.645 | 7.758 / 15.009 | 8 / 212 | 432.4 MiB |
| High / room | 7.098 | 7.315 / 14.149 | 7.676 / 14.655 | 31 / 21440 | 433.3 MiB |
| High / window | 6.925 | 7.202 / 14.406 | 7.579 / 14.915 | 18 / 9960 | 431.3 MiB |
| Low / room | 7.354 | 7.586 / 14.003 | 7.963 / 14.473 | 31 / 16568 | 175.9 MiB |
| Medium / room | 7.289 | 7.569 / 13.894 | 7.936 / 14.438 | 31 / 21440 | 365.9 MiB |

These timings measure CPU work/submission and event-loop pacing, **not GPU
execution or physical display refresh**. `frame_ms` adds measured update/render/
swap work; `loop_ms` also includes scheduler/event time. No GPU timestamps,
latency tracing, moving-camera run, long thermal test or other target hardware
was measured. Low/Medium being no faster here is a reason to profile bottlenecks,
not evidence that resolution has no GPU cost. This is an honest local baseline,
not an invented minimum FPS or justification to raise limits.

The original High RSS sample was 447.3 MiB; the new room run is 433.3 MiB. There
was no lighting/render improvement or controlled before/after cost experiment;
host/cache/window conditions differ. Treat this variation as a measurement limit.
The normal build excludes the diagnostic shader branch/selector. Feature-final
image equality verifies composition, but no enabled-feature overhead budget is
claimed from mixed runs.

Capture-time instrumentation also now gives genuine **offscreen base-scene**
counts: room/contact/entities/window/hall are 31/23/17/18/8 accounted draws in
the fixed scene, respectively. Counts exclude uncounted sky/reflections, emission
duplicates, post and UI. Frustum-accepted distinct indexed range/object vertices
are not triangle or occlusion-visible counts. Native state summary
records these counts plus world/prop/dynamic geometry storage, two atlas pages,
reflection faces, actual scene resolution and process RSS for each control.
Static upload remains 5,300 model triangles; a triangle count cannot be inferred
by dividing the submitted distinct vertices by three.

The one provenance-enabled hero bake takes 5.952 s wall versus the earlier
uninstrumented 4.89 s. This includes raw arrays, metadata and diagnostic image
I/O, so it is **not** a physical-solver performance regression measurement.
The package is byte-identical. Raw evidence stays outside `target/`; no caps,
transport settings, formats or map content were changed to improve numbers.

## Existing safety limits and practical budgets

These are current facts, not proposed limits. Decoder/safety maxima and normal
art/performance policies serve different purposes.

- `LightmapConfig` currently permits **eight** pages, 1024² for Medium/Full;
  RGBA16F mean+moment cost 16 B per page texel per contribution group. This hero
  uses 32 MiB; eight pages use 128 MiB before switchable contribution layers.
  The final asset-pass demo already reaches eight Full pages. Package decoding
  separately permits 64 pages, edge ≤4096 and a bounded atlas record; that is
  not an affordable atlas budget or authorization to increase it.
- The irradiance field prefers 1.5 m spacing, at most 64 cells per axis and
  262,144 slots, with 5 cm opaque clearance. These are current implementation
  choices/safety bounds to profile rather than future quality targets.
- Native props normally retain 256² fitted textures (Low ≤128²); surfaces,
  fixtures and decals upload up to 1024/512/256 by quality; sky up to
  2048/1024/512. Ordinary file images have the specification's hard 1024 edge
  ceiling, sky 2048×1024. Embedded prop PNGs and UV/alpha contracts stay valid.
- Prop art review prefers 500 triangles, justifies >800, permits 1,500 for
  static props and 3,000 for skinned characters. The runtime parser's 6,000
  triangle and 65,535 vertex maxima are safety boundaries, not authoring targets.
  The decoded shipped static pack policy remains 64 MiB; Winter's last retained
  inventory is about 41.1 MiB / 52,090 triangles, not a newly measured full-pack
  Stage 1 figure. This fixture reuses its assets.
- Dynamic objects are bounded at 64, unique dynamic meshes at 16; attached
  dynamic lights at eight are unshadowed. Reflections allow at most two probes,
  48/64 px Medium/Full, and one selected half-drawable planar target. Fog regions
  uploaded at Low/Medium/High are bounded at 2/8/16. No AA is implemented.
- Level validation permits 8,000 rooms, 60,000 walls, 100,000 props and 24 million
  generated vertices. Package limits include 1 GiB aggregate uncompressed data,
  512 entries and bounded individual records. These are correctness/resource
  safety caps, not evidence that such a level meets a playable frame budget.
- The geometry checker samples 0.25 m walkability cells, a 0.25 m body radius,
  0.5 m perimeter samples and a 1 m missing-wall run. Package validity,
  catalog validation and geometry success cannot establish microscopic light
  isolation, material appearance, draw ordering or visual chart continuity.

Sources: `src/level.rs`, `src/package/mod.rs`, `src/lighting/lightmap/mod.rs`,
`src/lighting/probes.rs`, `src/quality.rs`, `src/render/common/dynamic.rs`,
`src/render/common/atmosphere.rs`, `src/gltf.rs`, `src/geometry_check.rs` and
`docs/ASSET_SPECIFICATION.md`. No limit was changed in Stage 1.
