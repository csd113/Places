# Arcade doorway performance — 0.10.0 staging

Native measurements on 13 October 2026 reproduced near the reported severity:
the closed bathroom door at close range took **41.99 ms median (23.8 FPS),
47.34 ms worst (21.1 FPS)**. The corrected shader took **11.70 ms median
(85.4 FPS)** at the same resolution and quality, a 72% frame-time reduction.
At the more distant bathroom position, still and lateral runs improved by
69%. Exact sustained 20 FPS was not reproduced; the user's precise camera
position was unspecified.

## Cause and correction

Movable-object direct lighting interpolated eight receiver anchors separately
for each of four emitter taps. Each scalar lookup also explicitly copied a
176-byte anchor containing its entire visibility array into fragment-local
storage. `world.wgsl` now reads just the selected source's four visibility
channels from uniform storage, interpolates the vector once per source, and
uses each independent channel for its original tap. Source selection,
trilinear weights, clamping, falloff, cosine, attenuation and indirect lighting
are preserved. This applies to all objects using this lighting path, without
a map or camera special case. The two changes were measured together; their
individual contributions and actual GPU register spills were not isolated.

The existing GPU meter also added overlapping Metal stage spans. Three bounded,
labelled native samples showed seven overlapping spans: one summed to 98.161 ms
but covered only 16.012 ms. The meter now unions a fixed maximum of sixteen
intervals, counting overlap once and excluding disjoint gaps. The overlay calls
this **GPU elapsed**: stage timeline coverage can include scheduling stalls,
and is neither active execution time nor device utilization. The three-slot
asynchronous readback still skips busy slots instead of waiting. Individual
pass spans cannot establish additive pass costs on this backend.

The closed door still submits geometry behind it under existing frustum culling.
That fact alone did not establish the cause. A same-build depth-prefill pilot
changed still median 14.87→14.70 ms and motion 14.64→14.53 ms; it was discarded.
Splitting the opaque/discard shader path also produced no material gain and was
discarded. No visibility, depth, collision, map, texture, lighting or reflection
quality changes remain. No packages were rebaked.

## Matched measurements

Hardware: Mac14,9, Apple M2 Pro (19 GPU cores), 16 GiB, macOS 27.0.1
(26A434), native wgpu/Metal, Bgra8UnormSrgb/Depth32Float, timestamp period
1 ns. High textures and filtering, Full lightmaps and reflections, bloom on,
FOV 60°. All 22 Arcade fixtures were on; entry and three stall doors were
closed, angle 0°, unobstructed. No additional frame cap was applied.

Windowed runs used logical 1280×720, **2560×1440 drawable, Retina 2×**.
Still eye `(−1.6, 1.6, 2.8)`, yaw/pitch `(90°, 0°)`. Lateral runs retained
X/Y and camera, moving Z approximately 1.65–4.05 at 3 m/s. The tool scripts
right at simulation seconds 2–2.4, then alternating left/right for 0.8 seconds.
Simulation uses 1/60-second steps; reported timings are real wall time.
Each run warms up 120 ready frames. Three alternating before/after pairs
provide 1,800 measured frames per condition. GPU samples are deduplicated by ID
(1,016–1,080 samples); they arrive asynchronously and are not paired with the
current CPU row. Median is the arithmetic middle; p95 uses nearest rank.

Fullscreen still eye `(−0.65, 1.6, 2.8)`, same camera, logical 1512×949,
**3024×1898 drawable, Retina 2×**, 360 measured frames per build. Actual
drawable dimensions differ from the panel's 3024×1964 dimensions. A separate
fullscreen motion pair changed drawable height and is excluded from comparisons.

All values below are milliseconds; CPU loop cadence includes update, render
preparation, drawable/presentation waits and pacing.

| Condition | Before median / p95 / worst | After median / p95 / worst | GPU elapsed median, before → after |
|---|---:|---:|---:|
| Windowed still, Immediate | 14.885 / 17.292 / 27.107 | 4.647 / 6.278 / 15.494 | 15.348 → 5.104 |
| Windowed lateral, Immediate | 14.722 / 23.805 / 29.235 | 4.513 / 6.497 / 18.116 | 14.993 → 4.876 |
| Close fullscreen still, Immediate | 41.988 / 45.807 / 47.341 | 11.704 / 13.512 / 19.935 | 42.701 → 12.400 |
| Close fullscreen still, VSync/Fifo | 41.784 / 42.393 / 46.003 | 11.546 / 12.364 / 14.550 | 42.386 → 12.109 |
| Final publication build, same Fifo repro | — | 11.565 / 12.023 / 12.331 | 12.092 |

Still submissions remained 274 base-scene draws / 73,104 triangles; lateral
median 275 / 73,224, maximum 294 / 76,458. No planar pass was active in this
door view. Baseline fullscreen update median was 0.804 ms. A separate three-second
sample found 162/230 main-thread samples waiting in `CAMetalLayer.nextDrawable`;
the CPU render interval therefore must not be read as active CPU work.
A same-final-build timestamp-off control gave median 11.533 ms versus 11.565 ms
with timing; p95 varied 13.499 versus 12.023 ms. This does not support material
instrumentation overhead, but does show pacing jitter.

Codex, browser and WindowServer remained active; no owned build ran during timing.
Representative load averages were 1.8–2.6, and the final run 2.7→2.4; receipts
retain each run's load. No thermal warning was reported. Three separate device
observations saw 100% whole-device GPU utilization and about 682 MiB player RSS;
these include other apps and do not imply an utilization improvement. Captures,
raw traces, the CPU sample and settings transitions are excluded from headline
timings. This is bounded Mac evidence, not a cross-platform performance guarantee.

## Controls, visuals and remaining limits

The adjacent bathroom view stayed approximately 1.94→1.89 ms; its p95 varied
2.93→3.14 ms. The mirror view retained its planar pass, GPU median 2.889→2.151 ms.
At `(−2,1.6,2.8)`, the entry was verified Open, 90°, unobstructed, with stalls
closed and lights on: loop median 11.231→4.617 ms, p95 23.400→12.970 ms.
An earlier interaction from inside the swing arc reversed and was excluded as
an open-door control.

Arcade party and cabinet views, Home and Office basement were also checked.
Home GPU median changed 1.357→1.155 ms; Office 2.078→1.468 ms while loop median
remained about 1.95–1.97 ms. Party's first p95 worsened 4.071→12.168 ms;
a reversed-order repeat gave 4.069→2.887 ms. Pool's first loop median worsened
2.104→2.674 ms; repeat 1.720→1.385 ms. Pool also contains gravity movement.
These short control runs expose background/presentation variation and do not
establish a uniform FPS benefit or eliminate every dip.

Six unmodified native 2560×1440 captures at ready frame 120 are **byte and pixel
identical within each pair**, including the legitimate open door and reflections:

| View | Baseline | Final |
|---|---|---|
| Closed entry | [before](../images/performance-0.10.0/door-before.png) | [after](../images/performance-0.10.0/door-after.png) |
| Open entry | [before](../images/performance-0.10.0/open-before.png) | [after](../images/performance-0.10.0/open-after.png) |
| Bathroom mirrors | [before](../images/performance-0.10.0/mirrors-before.png) | [after](../images/performance-0.10.0/mirrors-after.png) |

All eight packages passed current dependency verification and clean native
load/presentation checks: Pool, Home, Lantern Hollow, Winter, Beach, Frutiger Aero,
Arcade and Office (180 ready frames each, no skipped ready presentation or device
validation error). Repeated doorway crossings plus texture/filtering/lightmap,
reflection and bloom transitions completed and returned to High/Full settings.
That diagnostic retained an existing **1.805-second settings-transition pause**
(1.800 seconds in update), excluded from steady measurements. It remains open.
The original intermittent gameplay crash remains unexplained; these sessions
do not establish a crash fix.

## Reproduction and verification

Start point: `0.10.0-staging` and origin verified at
`2a3dfd573a7eff6486231fd5e94d07a423500ec2`, origin
`https://github.com/csd113/Places`. Local evidence and retained executables live
under `local/performance-0.10.0/`. Main comparisons use the original shader with
the corrected meter (`before-meter-release`, SHA-256
`aa29cac7298106e3204cbbc666a28a48e53ae9f5a77c31d892936665f0b94b84`)
and the measured candidate (`final-measured-release`,
`f694068c19b9b75057b4891eb9d785a5f69a4daa7227f7bfa3468a9b2428c68c`).
The publication binary adds infrequent door/light state snapshots; its confirmed
SHA-256 is `8b827df137d67e7315ff8b13bbf18d0644b40de3d799d0c66eeee6d50a23a159`.
The untouched original release baseline is retained separately. The Arcade
package SHA-256 remained
`fa9abf3fd0e45842dff2212b6a41787ef56a8529becdb495572561a3f3d373fe`.

```sh
python3 tools/bench/frame_time.py --binary local/performance-0.10.0/bin/final-release \
  --out local/new-door-still --frames 600
# Add --motion for the matched lateral trajectory.
python3 tools/bench/frame_time.py --binary local/performance-0.10.0/bin/final-release \
  --out local/new-door-full --spawn=-0.65,1.6,2.8,90 --fullscreen --vsync --frames 360
```

The tool saves settings, pre-launch binary/package identities (verified unchanged
afterward), CSV, actual drawable/present mode, player bounds and receipt. It
rejects skipped/occluded scenes and retains timeout output. Use a new directory
for each run; no map baking is part of this workflow.

Validation completed with exit 0:

- `cargo fmt --all -- --check` and `git diff --check`.
- `RUSTC_WRAPPER= cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- The same Clippy command with `--release`.
- `RUSTC_WRAPPER= cargo test --workspace`: 2,263 library, two compiler CLI and
  seven integration tests passed; 19 existing GPU tests remain explicitly opted in.
- `RUSTC_WRAPPER= cargo test --workspace --all-features`: 2,269 library plus
  the same CLI/integration checks passed.
- Existing native GPU helper test explicitly run with `--ignored --exact`:
  `render::wgpu::renderer::entity_lighting_tests::normal_direction_helpers_keep_extreme_and_degenerate_vectors_finite_on_gpu`
  passed in 0.22 s. It now protects independent visibility lanes, sources,
  corners, interpolation/clamping and analytic direct-light totals, sharing
  one existing dispatch/readback (448-byte results, 6,240-byte uniform).
- Existing timestamp decoder test now covers overlap/containment/unordered
  intervals and gaps using a 64-byte fixture. No new test count or full-map test.
- Python `test_art_style_hero.py` (six) and `test_bench_metrics.py` (14),
  `py_compile` and benchmark `--help` passed.

Publication SHA and hosted CI outcome are recorded in the delivery message and
local `publication.json`, after normal staging-only publication.
