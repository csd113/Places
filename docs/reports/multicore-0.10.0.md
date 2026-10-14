# Multicore runtime and Arcade performance — 0.10.0

The normal player now uses measured, bounded CPU parallelism. On this M2 Pro,
the final player's same-build cabinet comparison reduces CPU update **28%**;
heavy water work improves **30–45%**. Arcade's Full closed-door GPU interval
falls **39%**, and final loop p95/p99 fall **36%**. These results address the
reported fluctuating drops toward roughly 60 FPS. Remaining tails and smaller
regressions are recorded below; an unconditional 120 FPS lock is not claimed.

Eight published packages, textures, shadows, lighting quality, content, physics
parameters and fixed steps are preserved. No map is rebaked, no asset is
redesigned, and sound remains deferred.

## Runtime execution

One persistent `runtime_work` Rayon pool serves dirty receiver lighting,
Full-lighting character skinning/environment preparation, and water immersion
quadrature. Automatic capacity is at most four workers, reserving one available
logical CPU outside that budget; developer overrides are bounded to eight and
the hardware budget. One-core hosts, wasm32, thread-creation failure and small
jobs use the same work inline. There is no player setting or per-frame thread
creation. Loading retains its existing bounded preparation/cancellation path.

Measured gates are eight dirty lighting receivers, 8,192 changed vertices
across at least two characters in Full lighting, and 64 awake enabled dynamic
water bodies. Jobs borrow disjoint output slots, then join. GPU queue writes,
SDL, ordered physics impulses and fixed solver steps stay on the caller.
Vertex/water staging is reused; no results remain queued across map changes or
shutdown. Arcade character staging adds about 2.2 MiB. Future real-time audio
callbacks must not allocate jobs for, lock, or wait on this synchronous pool.

Exact lighting revision/placement/bounds/caster keys avoid unchanged sampling
and emitter scans. A moving balloon center inside its stable swept envelope
updates its center sample without rebuilding spatial anchors. Legacy APIs still
resample mutable fields. Resource installation and fixture switches advance the
renderer revision. Vertex order, inverse-transpose normals, UV response and
physics application order are preserved.

Static character preparation remains interleaved with uploads. A same-binary
control showed batching its shorter GPU workload raised frame-time tails;
the installed environment selects the corrected policy during live switches.
Water and receiver lighting retain their independent work gates.

Rapier 0.36.1's own solver parallelism is measured separately through optional
`parallel-physics`. Its
[determinism contract](https://rapier.rs/docs/user_guides/rust/determinism/)
covers worker-count independence subject to platform/compiler conditions.
Rapier's `None` pool selects Rayon's global pool, so its reference uses an
explicit bounded one-worker pool. Heavy solver work shares the runtime pool;
entry is 128 awake enabled dynamic bodies, exit fewer than 112 for 0.25 seconds.
Unchanged bindings are retained; explicit developer comparisons bypass policy.
The feature stays **off in production**: its overhead outweighs the total
benefit at current scene sizes. Actual parallel lighting, skinning and water
ship independently of that feature.

## Arcade bottleneck and fixes

The Full spatial entity shader reconstructed the same eight trilinear weights
for indirect light, attenuation and direct-source visibility. A large nearby
receiver covers much of the Retina door view, magnifying that fragment cost.
It now computes two vectors once and reuses them, preserving corner order and
each `(x * y) * z` product. Fully occluded direct sources lose only their
strength when all eight anchors and four visibility lanes are exactly zero;
partial blending, water throughput and finite-emitter taps remain intact.
That source pruning alone did not materially improve the pilot.

The shared shader skips fog only when global density and regional count are
both zero. Completely overwritten fullscreen postprocess targets clear instead
of loading prior pixels; their quad has no blending/depth and single-sample
output. Controlled preceding-candidate repeats improved GPU intervals about 3%
at the door and 5% at cabinets for this attachment change. There is no map-name
check or quality concession.

The user clarified that FPS fluctuated toward roughly 60 rather than holding
an exact cap. Published door loop tails around 15–20 ms are consistent with
those drops, while representative views are below the 8.33 ms 120 Hz budget.
This is an inference from matched timings, not a replay of the user's monitor
telemetry. Reducing CPU work alone would not fix this GPU-heavy view.

## Matched native measurements

Apple M2 Pro/Metal, **3024 × 1898 physical pixels**, 1512 × 949 logical,
Retina scale 2, fullscreen, High textures/filtering, Full reflections, bloom,
FOV 60, identical packages/assets. Main runs use uncapped Immediate presentation;
there is no engine frame-cap setting. Each warms 120 ready frames and measures
600 at a fixed 1/60-second simulation step. Runs alternate order with no owned
build/bake/native contention. The harness rejects mismatched drawable, settings,
package, player bounds, reflection count or median submitted geometry.

GPU numbers are unioned pass-stage elapsed intervals, including scheduling
stalls. Overlapping Metal timestamps cannot be added or interpreted as exclusive
pass cost/active utilization. Hardware bandwidth/utilization and pixel-overdraw
counters were not collected. CPU update/render/submission, swap, geometry,
physics state and RSS receipts are retained.

The initial eight-scene campaign has 23 accepted pairs across 48 runs, all with
zero missed ready surfaces. One lateral pair differs by one 5 cm input step and
is excluded. Median of per-run statistics, milliseconds:

| View | CPU update before → after | GPU before → after | Loop before → after | Loop p95 before → after | Loop p99 before → after |
| --- | ---: | ---: | ---: | ---: | ---: |
| Arcade closed door | 2.205 → 1.192 | 11.582 → 7.092 | 10.847 → 6.584 | 14.518 → 8.637 | 16.857 → 9.033 |
| Arcade lateral movement | 1.785 → 1.046 | 6.819 → 4.431 | 6.261 → 3.900 | 9.168 → 6.029 | 10.677 → 7.260 |
| Arcade cabinets, one reflection | 1.550 → 1.009 | 4.204 → 4.130 | 3.470 → 3.651 | 5.997 → 5.917 | 6.800 → 6.325 |
| Arcade party room | 1.574 → 0.914 | 3.339 → 3.252 | 2.415 → 2.365 | 4.016 → 4.565 | 4.690 → 5.119 |
| Arcade bowling lanes | 1.582 → 0.909 | 3.767 → 3.728 | 2.928 → 2.972 | 4.566 → 5.071 | 5.383 → 5.563 |
| Home | 0.020 → 0.021 | 2.289 → 2.412 | 1.702 → 1.727 | 3.226 → 3.246 | 3.585 → 3.752 |
| Pool, dormant startup | 0.058 → 0.026 | 2.948 → 3.205 | 1.744 → 1.727 | 3.711 → 3.939 | 3.986 → 4.172 |
| Office | 1.471 → 1.380 | 1.990 → 2.083 | 2.182 → 2.128 | 3.257 → 3.526 | 3.815 → 3.818 |

All accepted pairs submit identical geometry: door 233 draws/117,902 triangles;
cabinets 98/76,238 plus one reflection. Several GPU/loop tails rise in lighter
views despite better CPU update; this is not a universal FPS gain. Peak RSS is
1320 → 1316 MiB at the door and 1313 → 1308 at cabinets, with noisier lateral
+83 MiB. No atlas expansion occurred. Earlier 41.99 → 11.70 ms lighting results
and small-window model-adoption measurements are separate baselines.

That campaign uses candidate `d6d4d757789c…`. Subsequent changes gate water-pool
initialization/64-body dispatch and keep Static uploads interleaved; the Full
shader/preparation path is unchanged. Final player `f7932d448807…` controls are
separate: two pairs/view for Full, three for Static.

| Final mode/view | CPU update before → after | GPU before → after | Loop before → after | Loop p95 before → after | Loop p99 before → after |
| --- | ---: | ---: | ---: | ---: | ---: |
| Full door | 2.234 → 1.259 | 11.606 → 7.052 | 9.814 → 7.185 | 16.513 → 10.583 | 19.708 → 12.569 |
| Full cabinets | 1.540 → 1.030 | 4.194 → 4.127 | 3.616 → 3.661 | 5.913 → 5.963 | 6.746 → 6.970 |
| Static door | 1.268 → 1.197 | 1.926 → 1.843 | 2.315 → 2.203 | 4.265 → 4.312 | 5.170 → 4.789 |
| Static cabinets | 1.289 → 1.260 | 2.680 → 2.834 | 2.087 → 2.015 | 3.526 → 3.635 | 4.879 → 4.771 |

Final door GPU improves 39% again, loop median 27% and p95/p99 36%. Its residual
tails remain above 8.33 ms and vary across campaigns. Render/submission tails
dominate; uncapped door swap p99 is about 0.11 ms. Cabinets remain roughly
throughput-neutral despite lower CPU update, with small upward tail differences.

Static's earlier batching regression was repeatable: identical-binary inline
versus automatic cabinet GPU 2.843–2.864 versus 3.235–3.239 ms, loop p95
3.574–3.606 versus 4.119–4.240 ms. Interleaved uploads correct that large cost.
Final Static cabinets retain a repeatable covered-GPU increase **0.11–0.21 ms
(4–8%)** and aggregate loop p95 **+0.109 ms (3%)**. Individual p99 pairs vary,
including one worse final run; aggregate p99 improves. The remaining driver/
shader/cadence contribution is unresolved. This residual is not hidden by Full's
gain, and Static is not claimed as a GPU win. Its sampled tails stay below
8.33 ms. Static is the existing UI label for internal off/vertex lighting;
its existing policy suspends reflections, so it is an independent control.

The final player's own two-pair inline/automatic Full cabinet comparison shows
CPU **1.402 → 1.005 ms (28% lower)**, p95 1.745 → 1.380 and p99 1.815 → 1.527.
GPU is 4.124 → 4.129; loop 3.678 → 3.772, p95 5.844 → 5.912 and p99
6.819 → 6.998 ms. Parallel work frees CPU headroom while that loop remains GPU/
submission limited. Earlier three-pair low-load controls start no runtime
workers in Home/Pool: update stays 0.019 ms in Home and 0.023 → 0.024 in Pool;
loop 1.676 → 1.686 and 1.739 → 1.715 ms, with noisier Pool p99.

A separate Retina Fifo door pair gives loop 10.693 → 8.342 ms, GPU
11.284 → 6.580, update 2.227 → 1.414, candidate p95/p99 8.655/9.815.
Three Home Fifo pairs show roughly 17 ms p95 in one published and one candidate
run, about 10.6 ms otherwise: no consistent paced regression/gain is established.
Diagnostic stage samples support the bottleneck analysis but are excluded from
acceptance timings.

## Physics and correctness

Five alternating repetitions of bounded 12/32/64/128-body contact, immersion
and dormant fixtures use 240 fixed steps/24 warmup. Only `ItemWorld::step` is
timed; velocity refreshes and per-frame state/contact/impulse hashes are outside
the timer. All **120** final one/four-worker and cross-feature comparisons are
exactly equal.

| Immersed bodies | Normal build inline → four workers, median ms | p99 ms | Policy |
| --- | ---: | ---: | --- |
| 12 | 0.024959 → 0.024958 | 0.032667 → 0.037542 | Both inline; timing noise |
| 32 | 0.063208 → 0.063208 | 0.077875 → 0.079167 | Both inline |
| 64 | 0.125292 → 0.087979 | 0.146125 → 0.117416 | Parallel, 30% gain |
| 128 | 0.252062 → 0.139145 | 0.287083 → 0.175375 | Parallel, 45% gain |

A 32-body pilot saved only 3.5% median while worsening p99 by 0.015 ms, motivating
the 64 threshold. Dry/dormant/sleeping entries cannot start unused water workers.
At 128 contacts, optional Rapier parallel changes its own reference
0.25310 → 0.21933 ms versus ordinary Rapier 0.22304: little total benefit.
Optional parallel-solver water at 128 costs about 0.162 ms versus normal plus
parallel water 0.139 ms. Smaller solver scenes cost more with the feature.

Final source passes fmt, strict workspace/all-target/all-feature Clippy in debug
and release, default Rust tests **2,325 passed**, all-feature **2,332 passed**,
and normal/optional release builds. Python source contracts: **356 run, four
existing skips**. Gates use empty `RUSTC_WRAPPER` because inherited sccache's
socket is unavailable in the sandbox; no gate bakes or requires shipped maps.
The 19 existing GPU-dependent Rust cases remain explicit desktop checks. The
existing 16 × 16 native GPU analytic normal/direction/visibility/direct-light
check passed separately. New small regressions protect joined slot reuse,
lighting/cache invalidation, normal/vertex/bounds parity, zero-source boundaries,
solver hysteresis and exact one-step 64-body water state (~9,216 simple rays).

Native acceptance preserves existing workflows:

- Bowling: **944 checks**, real pickup/drop/throw, contacts/gutters, blocked and
  held-member resets, two complete rounds; peak 11 active bodies, final dormant.
- Water: **243 checks**, six cases covering dormancy, settling, ownership,
  live quality, posture/entry/crouch/surface/shelf/deck and contacts. Five cases
  use Retina; the player case passes its original 1280 × 720 workflow.
- Moving receiver: **468 checks** at 1280 × 720 using the preserved small ball
  fixture, covering held/thrown motion, dim-room boundaries and fixture switches.
- Eleven matched capture pairs cover all eight packages and relevant Arcade
  views: nine identical, door seven pixels and Lantern one pixel different,
  maximum 1/255. Four final Full/Static door/cabinet pairs repeat that result:
  three identical, Full door seven pixels at 1/255, all at Retina resolution.
- Two 36-second lifecycle sessions each pass eight world commits across Arcade,
  Pool, Home and Office, repeated loads, UV/Full/Static/Medium/High switches,
  six captures, zero native validation errors and clean shutdown. Transient RSS
  peaks 1.23–1.52 GiB settle to 884–904 MiB near the final Arcade replacement.
  This checks resource return, not long-duration leaks or all driver caches.

Rejected/limited attempts remain separate: a wrong-drawable settling run passed
when retried at the required size; fullscreen water-player short shelf/deck
targets fail in both published and candidate while the original windowed case
passes. The fullscreen bowling plan exceeds its existing 60-second action budget
before launch; its supported windowed workflow passes. No fullscreen shelf/deck
claim or gameplay fault is inferred. Main performance/captures are Retina.
No other hardware, browser execution or physical single-core host is claimed;
the inline override and fallback code were checked.

## Deliverables and repetition

`local/multicore-0.10.0/runtime-deliverables.json` and `handoff.json` retain exact
publication, binary/package identities and custody. All four binaries and the
playable `play/Places-multicore.command` are outside `target`:
`bin/places-f7932d448807`, `bin/places-compile-5b0a868bc651`,
`bin/places-physics-bench-3cf470a75be3`,
`bin/places-physics-bench-parallel-0bd04d556491`. Eight packages retain their
published hashes under `assets/levels`.

Receipt groups: `final-full/accepted-summary.json`, `final-{static,full,inline}-policy/`,
`shipping-inline/`, `shipping-{vsync,vsync-home-repeat,static,static-repeat,static-inline}/`,
`physics-shipping-*.json`, `shipping-captures/`, `final-policy-captures/`,
`shipping-bowling-windowed/`, six accepted water case receipts,
`shipping-lighting-ball/`, `{native-lifecycle,final-policy-lifecycle}/`, and
`static-policy-gates/`. Large trace logs are gzip-compressed with an archive
manifest; decompress before re-auditing. Rejected receipts are labelled separately.

```sh
python3 tools/bench/multicore.py \
  --binary local/multicore-0.10.0/bin/places-f7932d448807 \
  --out local/new-inline-check --scenes cabinets home pool
python3 tools/bench/multicore.py \
  --binary local/multicore-0.10.0/bin/places-f7932d448807 \
  --baseline-binary local/model-integration-0.10.0/bin/places \
  --out local/new-published-check --scenes door lateral cabinets party lanes home pool office
local/multicore-0.10.0/bin/places-physics-bench-3cf470a75be3 \
  --workers 4 --frames 240 --repeats 5
```

Build first; measure in a fresh output directory with one native session and no
owned build/bake contention. Add `--lightmaps static` or `--vsync` for separate
controls. The tools never bake maps or run automatically in CI. No next queue
stage is started.
