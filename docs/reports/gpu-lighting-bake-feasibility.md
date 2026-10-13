# GPU lighting bake feasibility

13 October 2026. Audited `arcade-env` at
`6c5f43f4d3d2a82989fa58bd83bbfccb6c20a8d5`; Apple M2 Pro, 12 CPU cores
(8 performance / 4 efficiency), 19 GPU cores, 16 GiB unified memory; wgpu 30.0.1,
native Metal, optimized Rust 1.99 release build.

**GPU acceleration could meaningfully shorten large fresh Full bakes. Recommend
a bounded correctness prototype as future work, keeping the CPU baker as the
default. A production GPU rewrite is not justified yet.** The expensive work is
parallel transport, but the current watertight intersection deliberately uses
double precision unavailable on this Metal adapter. There is no acceleration
switch to enable. This audit implements no lighting backend and changes no map,
asset, package, solver, quality setting or cache contract.

## What the compiler actually does

The [compiler](../../src/compiler.rs) validates dependencies and package hashes,
reuses a valid complete package or prepared lighting when permitted, builds
collision/navigation inputs, resolves material images, then prepares each variant.
Full runs [scene preparation](../../src/render/common/light_transport.rs), chart
planning and [CPU transport](../../src/lighting/transport.rs), assembles atlases,
encodes prepared records/PLLC, optionally captures reflections, shares prop records,
hashes records and publishes a Deflate-6 archive. Static uses the cheaper vertex
bake; accelerating Full rays cannot remove its geometry/visibility preparation.

The transport scene has a 16-bin SAH BVH, four-triangle target leaves, depth cap
40 and contiguous 48-byte ray triangles. It also retains canonical bounds and
the original tree/order for point queries and equal-distance hit identity.
Any-hit visibility exits early; nearest-hit bounce traversal prunes by distance.
Receiver jobs already use dynamic contiguous batches of 32–2,048, at most 12 scoped
CPU workers, ordered outputs and cancellation checks.

Full uses three emitter taps per axis, two bounce orders and 64 deterministic
hemisphere samples per receiver/order. [Direct integration](../../src/lighting/transport/coverage.rs)
keeps all 4×4 receiver-footprint positions, adding an 8×8 stencil at visibility
edges: 80 positions there. Bounce hits read previous-order radiance through eight
cache cells, selecting compatible visible surface representatives and sometimes
searching farther cells. These connectivity checks trace additional rays. The
base and single switched layer share geometry/stencils but keep independent
radiance; other circuit layouts use the general path. Porting only intersections
would leave cache lookup, accumulation and much ray-dependent control on CPU.

[Filtering](../../src/lighting/transport/filter.rs) is a serial, physical-surface
four-neighbour stencil with chart joins and visibility checks. It filters indirect
energy, preserving direct shadows, material/normal boundaries and directional
moments. Recovery fill has its own visibility support. Irradiance probes use
CPU direct/gather transport before filtering; reflection cubemaps already use
Metal, followed by readback and deterministic CPU mip/record encoding.

## Measurements and their limits

Used the existing [benchmark harness](../../tools/bench/compiler_bench.py), with
explicit 4×4 packing/switch control and 8×6 material-routing fixture; three fresh
processes per worker count, `--force --variants off,full`, no owned compilation,
native probe or tests during accepted timing intervals. Filesystem/driver caches
were not flushed. Medians below are milliseconds; child phases are nested inside
Full preparation and must not be added to that parent. The harness polls exit at
100 ms intervals, so process wall is deliberately separate from compiler wall.

| Measured stage | 4×4 / 1 worker | 4×4 / 8 workers | 8×6 / 1 worker | 8×6 / 8 workers |
|---|---:|---:|---:|---:|
| Complete process wall | 3,222.8 | 933.1 | 4,056.6 | 1,143.3 |
| Compiler wall | 3,136.2 | 836.7 | 3,982.1 | 1,067.6 |
| Full prepare + encode, parent | 2,949.9 | 675.3 | 3,533.1 | 697.2 |
| Scene/BVH + targets | 0.657 | 0.601 | 0.868 | 0.748 |
| Receiver construction | 7.9 | 7.8 | 8.2 | 8.4 |
| Direct visibility, all layers | 1,118.1 | 156.3 | 1,281.2 | 190.5 |
| Indirect cache + gather + accumulation | 1,540.7 | 225.6 | 1,979.2 | 287.3 |
| Irradiance probes | 0.87 | 0.93 | 2.42 | 1.92 |
| Indirect filter + compression to coefficients | 77.2 | 78.6 | 88.2 | 86.8 |
| Recovery fill / final validation | 0.64 | 0.89 | 62.8 | 9.8 |
| Atlas assembly | 5.9 | 5.9 | 3.3 | 3.6 |
| Reflection capture, including device/upload/readback/encoding | — | — | 179.0 | 177.2 |
| Record hashes + manifest | 47.1 | 47.0 | 25.0 | 24.6 |
| Archive compression + publication | 17.6 | 17.7 | 20.6 | 20.0 |

Eight workers deliver measured **3.45× / 3.55× end-to-end CPU speedups** versus
one. Each fixture has one identical complete archive hash across all six builds.
The 4×4 solve has 442 triangles, 20,831 receivers, 3,686,832 direct-ray counter
increments and 2,666,368 logical gather samples; 8×6 has 106 triangles, 46,409
receivers, 7,336,656 / 5,940,352 respectively. Gather counters include horizon
rejection and can count shared light layers; they are not triangle-test counts.

At eight workers, direct + indirect account for **40.9% / 41.8% of process wall**
(45.6% / 44.8% of compiler wall). Approximately 199 / 108 ms of Full preparation
is outside the listed child timers, covering other preparation, assembly of solve
outputs and prepared-record/atlas encoding; it is not measured ray time or an
isolated codec measurement. Material resolution costs 38 / 72 ms; despite its
`capture_device_materials` label, this phase does not initialize Metal. The
reflection device is initialized later inside `capture_full`.

These small scenes establish costs and scaling, not current production proportions.
The [October 3 pipeline audit](compiler-pipeline-audit-20261003.md) measured final
Hollow at 246.247 s: indirect 173.763 s (70.6%), direct 20.017 s, prop preparation
30.430 s. Its earlier native sample located traversal/intersection/cache selection
as the hot path. Those sources/settings predate later quality/content changes.
The [storage audit](map-storage-lighting.md) measured Full prepare/encode at
535.20 s for Hollow and 354.55 s for Arcade, but did not isolate rays. The more
recent [efficiency audit](lightmap-efficiency-and-ceiling-halo.md) measured Arcade
643.721 s / 2,633 MiB peak RSS and Home 35.978 s; those totals cannot be combined
with older ray fractions to claim a current GPU bake estimate. Archived Demo/Zoo
observations were read only; no archived or production map was baked here.

## GPU setup, movement and capacity

A small isolated wgpu probe measured adapter/device creation, an empty WGSL compute
pipeline and byte-exact buffer roundtrips. **It traces no rays and measures no
lighting kernel speed.** First observed process setup was 18.19 + 10.55 + 253.63 =
282.37 ms; another fresh process with warm driver cache was 19.02 + 6.33 + 0.13 =
25.48 ms. These are two observations, not a cold-cache guarantee or the compilation
cost of a real BVH shader. No-op pipeline execution was not timed.

Warm repetitions within the first device, medians of three, measured:

| Buffer payload | CPU staging copy | Submit + upload + two GPU copies + map/wait | Copy mapped result to CPU Vec | Total |
|---|---:|---:|---:|---:|
| 4 MiB | 0.54 ms | 0.66 ms | 0.11 ms | 1.30 ms |
| 16 MiB | 2.06 ms | 1.43 ms | 0.33 ms | 4.02 ms |
| 64 MiB | 7.27 ms | 3.74 ms | 1.42 ms | 12.60 ms |

The 64 MiB first roundtrip cost 27.64 ms before warm reuse; constructing its CPU
input cost 5.16 ms separately. Allocation calls were below 0.1 ms but exclude
deferred commitment. The second process's warm 64 MiB total was 13.48 ms.
These synchronized wall costs include scheduling; they are not GPU timestamps or
PCIe bandwidth. Unified memory still incurs wgpu staging, ownership transitions,
completion waits and CPU result copies. [Queue documentation](https://docs.rs/wgpu/30.0.1/wgpu/struct.Queue.html#method.write_buffer)
confirms immediate staging and deferred transfer at submission. Real scene packing,
alpha uploads, cache flattening and kernel setup remain unmeasured.

The adapter advertises approximately 4 GiB buffer/binding limits, 29 storage
bindings and 1,024 workgroup invocations. The probe requested default device limits;
advertised limits must be explicitly negotiated, not assumed available. Portable
[wgpu defaults](https://docs.rs/wgpu/30.0.1/wgpu/struct.Limits.html) are 128 MiB per
storage binding, 256 MiB per buffer, eight storage bindings and 256 invocations.
Design chunked dispatch/resources around those, querying actual capabilities.

Arcade's measured 3.405 million receivers would occupy about 260 MiB in a proposed
80-byte GPU receiver layout. One packed 60-byte accumulator array alone is about
195 MiB per contribution, before GPU alignment; direct, total and previous/next
bounce buffers quickly exceed 1 GiB with independent circuits. Add both tree
orders, triangles, flattened cache representatives, alpha data, CPU copies and
atlases. Runtime Arcade lightmaps' 240 MiB is not the bake working set. Receiver
tiling must retain access to the global previous-order radiance/cache; independently
baking tiles would change light transport. Never allocate every emitted ray at
once. Chunk buffers and preserve cross-chunk indices; fail over on capacity or
device failure without publishing a partial package.

## Practical backend choices and correctness

**Portable wgpu compute/software BVH is the best first experiment.** Keep CPU
geometry validation, chart packing, tree/cache construction and serialization.
Flatten nested vectors/maps into explicit WGSL storage layouts; batch many
receivers, keep scene and bounce buffers resident, synchronize between orders and
read final coefficients back in batches. Start with one invocation per receiver
and ordered sample accumulation; splitting rays into parallel reductions changes
rounding. Grouping spatially can improve locality, but divergent BVH paths,
variable direct footprints, cache fallback and Blend crossings limit occupancy.
Large traversal stacks increase register pressure/spills. Measure these kernels
against eight CPU workers, not the serial reference.

The native probe reports `SHADER_F64=false` and `EXPERIMENTAL_RAY_QUERY=true`.
wgpu's [30.0.1 Metal implementation](https://github.com/gfx-rs/wgpu/blob/v30.0.1/wgpu-hal/src/metal/adapter.rs)
corroborates ray-query support; the API documentation's Vulkan-only platform list
is incomplete here. A flag is not a measured successful ray-query bake. Native
Metal acceleration structures or wgpu experimental queries are an alternative,
but use different intersection/tie rules, require acceleration-structure build
costs and give up portable WebGPU. The [experimental API contract](https://docs.rs/wgpu/30.0.1/wgpu/struct.ExperimentalFeatures.html)
also requires an explicit unsafe opt-in. M2 lacks dedicated hardware ray tracing;
[Apple introduced that on M3](https://www.apple.com/newsroom/2023/10/apple-unveils-m3-m3-pro-and-m3-max-the-most-advanced-chips-for-a-personal-computer/).
Do not predict RTX/M3 performance from this host.

The current ray kernel evaluates watertight edge functions/divisions in `f64`,
rounds accepted distances to `f32`, and replays canonical traversal for ties.
Thin-face alpha barycentrics also use `f64`. [WGSL types](https://www.w3.org/TR/WGSL/#floating-point-types)
provide runtime `f32` and optional `f16`, not runtime double precision.
[Its evaluation rules](https://www.w3.org/TR/WGSL/#floating-point-evaluation)
permit reassociation/fusion and some subnormal flushing. Even an ordered GPU
reduction cannot promise CPU/GPU or cross-device bit identity. The SplitMix64
sample seed and visibility signatures additionally need exact two-u32 arithmetic
or CPU-generated sequences for browser portability; native u64 is insufficient
as a portable contract. Current Cargo enables desktop backends only; browser
baking is not required for a future WebGPU player to read these same packages.

Retain linear HDR, signed per-channel moments, actual shading/geometric normals,
source shapes/range, 80-position shadow integration, switch separation, sky and
the recovery/energy guards. Preserve opaque/MASK coverage with numeric bilinear
alpha and clamp/repeat addressing; Blend attenuates straight rays, merges shared
crossings and keeps cache-connectivity semantics distinct. Glass passes through;
water applies depth attenuation. A GPU must not replace these with binary shadows,
refraction, display RGB, half-precision accumulation or premature clamping.

**No GPU error tolerance is established by this audit.** Existing CPU changes
are accepted by exact record comparisons. A future approximate backend needs
CPU-reference sweeps of boundary/tie/thin-opening/long-ray cases, alpha thresholds,
water, dark channels, HDR peaks, normals, multi-bounce and circuits, followed by
coefficient and reconstructed-light error distributions and native shadow views.
Visibility, chart topology and circuit identities need exact semantic agreement;
an averaged image metric cannot excuse a leak. PLLC's existing per-channel bounds
(coefficient relative 0.001953125; reconstructed light 0.00390625, with small
absolute bounds and exact-half fallback) constrain storage error, not permission
for another solver error. Measure the combined GPU-plus-codec error against the
CPU reference before proposing any budget. Keep strict CPU gates unchanged.

If exact physical bytes remain mandatory, investigate conservative GPU candidate
discovery with CPU `f64` acceptance/ordered arithmetic, or software extended
precision. Neither is validated or timed here; CPU/GPU exchanges could erase the
gain, and conservative eligibility must be proved rather than patched with an
epsilon. Repeated same-adapter bakes also need stable hashes. Record backend,
kernel/precision revision, sample contract and relevant adapter/driver identity
in both package and prepared-stage keys; existing executable/solver hashes alone
cannot distinguish runtime backend selection. CPU fallback must use its own key.

## Benefit and implementation decision

Use `T_gpu = T_cpu - R + R/r + H`: measured replaceable transport wall `R`,
hypothetical GPU transport speedup `r`, and additional packing/setup/transfers/
readback/fallback overhead `H`. Acceleration of triangle tests alone is less than
acceleration of the entire direct/indirect stage used in these projections.

| Projection, not a GPU bake measurement | 2× transport | 4× transport | 10× transport |
|---|---:|---:|---:|
| 4×4 eight-worker process, H = 30 ms | 1.21× | 1.38× | 1.51× |
| Same, H = 300 ms | 0.90× | 0.99× | 1.05× |
| Historical final Hollow, H = 0.5 s | 1.64× | 2.43× | 3.40× |

The H values are explicit scenario assumptions informed by the staging probe,
not measured real-baker overhead. With zero H and infinitely fast direct/indirect,
the measured small fixtures cap at 1.69× / 1.72×; historical Hollow caps at 4.69×.
No GPU transport speedup was measured, so the audit promises none. Startup can
consume small-scene savings, while seconds of overhead matter little in a several
minute solve. Keep a device across a future collection build. Verified package
hits here cost about 100 ms compiler wall / 210 ms polled process wall and perform
no solve; prior Arcade/Home package hits were 0.863 / 0.115 s. Reuse should return
before GPU initialization.

Less risky CPU work has evidence: retain existing shared queries/SAH/batching and
safe package reuse; investigate parallel independent filter outputs (79–87 ms
serial in these controls), preserving each stencil's arithmetic/order. That is
a candidate, not an implemented or measured gain. Legacy large-map prop preparation
also has measured cost. More BVH builders, compression pools or speculative SIMD
are not justified by this audit; the existing CPU optimization report already
removed substantial duplicate work.

Recommended future sequence, with a stop/go decision at each dependency:

1. Define the precision/semantic acceptance contract and run existing bounded ray
   oracles against an isolated GPU intersection candidate. Resolve double precision,
   ties and alpha first; stop if correctness requires an unacceptable compromise.
2. Flatten immutable scene/cache data and implement opt-in batched direct transport,
   with capacity checks, bounded cancellation, CPU fallback and backend-aware keys.
3. Move nearest-hit gather **and** visible cache selection/accumulation together;
   preserve shared layers and previous-order buffers. Then consider probes/filtering.
4. Compare identical small fixtures and one explicitly approved active large map,
   including first/warm setup, transfers, RSS, readback and full archive wall time.
   Proceed to production only with repeatable worthwhile end-to-end savings and
   passing numeric/visual/cache gates; keep CPU available for unsupported devices.

Only this report is a tracked change. Release compilation, 12 accepted fixture
bakes, six verified cache hits and two native staging probes passed; all six
archives per fixture match exactly. Raw logs, source hashes, summaries and isolated
probe source remain in `local/gpu-bake-audit/` outside `target`. An exploratory
batch overlapping probe compilation and its sandbox reflection failure are
explicitly excluded. Documentation/link/arithmetic/whitespace checks apply;
workspace tests/Clippy were not rerun for documentation alone (the audited
baseline's exact-SHA CI passed). No GPU lighting prototype, Linux/Windows/browser
measurement or current large-map ray breakdown was performed.
