# Places compiler pipeline audit

The implementation, paired benchmark comparisons, repository gates and native
validation passed in the October 3, 2026 audit. Measurements remain dated
observations under the host conditions below.

## Measurement contract

The baseline is working-branch commit
`e66d3a05f6f6cf33eb8657c1d62abf6befb8980e`, after the movement audit and recent
lighting repairs. The development machine is an Apple Silicon M2 Pro MacBook
Pro, 12 CPU cores (8 performance and 4 efficiency), 16 GiB RAM. Rust 1.99.0 is
pinned by `rust-toolchain.toml`. All compile measurements use optimized release
binaries, identical source/assets and all Off, Medium and Full variants.

Clean samples are fresh compiler processes with `--force` and distinct output
packages. Native sampling, builds, tests, gameplay and correctness comparisons
are excluded from their timed intervals. Cold process startup remains included;
filesystem caches are not artificially flushed. Raw first samples remain
available separately. Reported summaries use medians, not fastest runs.
One brief `cargo fmt --all` command at 04:55:14 UTC overlapped the final two
seconds of Demo optimized pair 3 (finished 04:55:16). That sample is retained
and annotated; no owned compilation, test, native sampling or gameplay overlapped
its timed interval.

The user explicitly requested proceeding without waiting for an idle CPU.
The final comparison alternates baseline and optimized builds within each of
three map pairs. Each process records UTC intervals, CPU time, effective busy
cores, Darwin peak RSS, context switches and periodic host load. Executable
names/resource totals before and after each run document desktop, VM and other
background work without reading command arguments. These are measurements under
shared-host load, not claims of exclusive-host performance.

The original baseline binary SHA-256 is
`ad248f4ad2f0f0dd0dd6d8bfbd435e808096582e2e2ad0ad0dd398f3e33a6558`.
An instrumented baseline produces byte-identical packages and adds phase
timings. The frozen final benchmark compiler SHA-256 is
`7c01a36c28ab84f1a24164bf48175a60f0c1df846c56b258e16a9bb2cc49256b`.

## Baseline and bottlenecks

The initial full-pipeline measurements identified transport, rather than disk
I/O, parsing, compression or BVH construction, as the dominant cost.

| Map | Transport triangles | Emitters | Switchable emitters | Original representative wall time | Accounted nested phases |
| --- | ---: | ---: | ---: | ---: | ---: |
| Small `test_room` | 719 | 1 | 0 | 8.875 s | 7.951 s, 89.6% |
| `places_demo` | 174,395 | 73 | 1 | 1,114.175 s | 1,103.401 s, 99.0% |
| `lantern_hollow` | 259,558 | 40 | 0 | 444.521 s | 436.860 s, 98.3% |

These representative runs explain costs; final speedup tables use the later
alternating repeated comparison. One original Demo run included sampling and
symbol linkage and is explicitly excluded from clean-performance medians.
Original Hollow runs overlapping other application work retain their labels.

| Nested phase | Demo | Hollow |
| --- | ---: | ---: |
| Indirect transport | 946.273 s | 339.560 s |
| Direct visibility | 50.656 s | 30.969 s |
| Lightmap filtering | 40.015 s | 8.704 s |
| Prop geometry/charts | 13.781 s | 48.043 s |
| Architecture/charts | 22.268 s | 3.339 s |
| Receivers | 17.509 s | 3.324 s |
| Chart fill | 10.499 s | 1.234 s |
| Probes | 1.210 s | 1.454 s |
| Atlas assembly | 1.029 s | 0.174 s |

Parent preparation timings include these child phases and must not be added
again. A native 15-second GI sample attributed 35.58% of mapped compiler samples
to nearest traversal, 23.76% to occlusion traversal, 23.26% to triangle
intersection and 16.21% to cache representative selection. This supported
algorithm and memory-access changes before further parallelization.

## Optimizations and correctness

1. **Reuse complete CPU preparation.** Each quality variant previously built
   its CPU world again for GPU reflection capture. Capture now consumes the
   already prepared world, while retaining the historical pre-feather decal
   colours required for identical captures. Materials come from the actual
   requested asset root. Probe resolution, mip generation and lighting remain
   unchanged. Packages without probe capture points avoid creating an unused
   GPU capture renderer.
2. **Share independent light-layer geometry queries.** Demo's one switchable
   light repeated the same receiver, cache, stencil and ray geometry work for
   its base and switch layer. Both layers now use the same geometric queries
   and separate original radiance buffers and accumulation order. Sky enters
   only the base layer. The original path remains for other layer counts to
   bound memory. No samples or bounces are removed.
3. **Improve ray traversal.** A deterministic 16-bin surface-area hierarchy and
   contiguous packed triangle records serve the hot ray queries. Prepared rays
   calculate invariant shear/dominant-axis terms once. Opaque visibility uses
   an early-return any-hit path; transmissive surfaces still pass through.
   Point queries retain their original hierarchy.
4. **Preserve exact numerical eligibility.** Tighter nodes exposed boundary
   rounding differences in an experimental candidate. That candidate was
   rejected when strict lightmap comparison found changed HDR channels.
   Conservative SAH bounds now discover candidates, original leaf bounds
   enforce canonical visibility eligibility, and rounded nearest-hit ties
   replay the original traversal/identity rules. Acceptance arithmetic remains
   unchanged. Broad-phase tolerance never changes the accepted ray distance.
5. **Reduce cache and temporary work.** Representative squared distances are
   computed when cache cells are built, lookup stencils use bounded stack
   storage, and already farther fallback cells avoid visibility queries.
   Static props skip redundant initial vertex lighting when their actual
   lightmap receivers will be baked; failure paths restore the original fully
   lit fallback. Archive entries move their buffers instead of cloning them.
6. **Batch bounded workers.** Independent receiver/texel work uses dynamic
   contiguous chunks with stable ordered assembly. Chunk size is bounded to
   32–2,048 indices; scheduling is one atomic fetch per chunk. At most 12 workers
   run, serial work uses the caller, and pipeline stages do not nest pools.
   Cancellation and worker failures propagate before final publication.
7. **Make incremental reuse safe.** Package and stage identities include
   content hashes for the actual catalogue, models and all referenced albedo,
   normal and emissive-mask PNGs, quality/record/geometry/solver fingerprints.
   `asset_inputs_v2` invalidates private experimental pipelines. Only proven
   display metadata/navigation changes reuse prepared lighting; physical edits
   rebuild it. Retained blobs are validated, obsolete navigation records are
   removed, and lightmap content identities are refreshed for the edited source.
   Independent clean builds must produce the exact incremental archive bytes.

The calibrated light range/shape reach rejection already precedes expensive
visibility; global/directional illuminators retain their separate semantics.
No unsafe facing cull, approximate light influence or older lighting path was
introduced. Existing model/texture preprocessing caches are reused within a
build. No partial-GI cache is claimed: lighting geometry/light/material changes
invalidate the complete transport result.

The Rust CLI does not invoke Python for numeric baking. Existing CPU-heavy
Python asset/zoo tools already use bounded process workers, deterministic
assembly and cache identities. Their applicable tests remain part of the gate.
BVH construction, I/O, serialization and compression were outside the dominant
cost; adding pools or changing artifact formats there was not justified.
No dependency, explicit SIMD, unsafe code or architecture-specific CPU path
was added. Ordinary packed numeric kernels remain portable.

## Output comparison and determinism

Strict comparison checks every decoded archive record, not selected screenshots
or an image-error tolerance. Geometry, collision, navigation, entities,
materials, HDR lightmaps, indirect/directional coefficients, probes and all
reflection mip bytes must remain identical. The only accepted differences are
compiler/stage identities and added existing texture dependency hashes/sizes
verified against the actual PNG files. Demo adds two previously untracked normal
map dependencies; the PNGs themselves are unchanged.

Nine repeated builds of the exact-output repaired candidate passed strict
comparison across all three maps/qualities. Repeated package hashes were stable.
All four rebuilt production packages also passed comparison against preserved
pre-audit bundles. The frozen final compiler passed all six Small/Demo paired
comparisons and the full-quality Hollow spot check. Native results are recorded
below after completion.

## Regression coverage

- Seeded prepared-ray tests compare 50,000 cases across four coordinate scales
  with the original watertight intersection, including zero/cardinal/invalid
  directions.
- BVH nearest/opaque/transmissive oracles cover 10,000 rays; another 80,000
  boundary/coincident rays protect distance bits and surface identity.
- Canonical leaf visibility is compared against the original traversal on
  50,000 additional rays. Captured grazing/parallel cases protect the rejection
  found in the experimental native bake.
- Cache connectivity, tie ordering, moments, eight-tap stencils, signed zero and
  fallback paths compare with the reference implementation.
- Shared base/switch layers preserve coefficients and probes with sky, water,
  dark lights, zero lights and zero through three bounces, at one/many workers.
- Parallel results are bit deterministic at 1/2/4/8/12/64 requested workers;
  tasks run exactly once, in output order, on no more than 12 worker threads.
  Cancellation and injected worker failure are covered.
- Compiler integration verifies metadata/navigation reuse versus independent
  clean builds at 1/12/64 workers, no orphan navigation records, catalogue and
  image-class invalidation, final algorithm-version invalidation, malformed
  input and invalid-worker preservation of the previous valid archive.
- An additional retained corruption fixture poisons a Medium lighting blob
  while leaving identities unchanged. Both unchanged and metadata-only builds
  must reject it and match independent clean bytes; the final default workspace
  run has passed this test.
- Nine Python harness tests cover parsing, resources, owned-child timeouts,
  atomic reports, strict output exceptions, preserved incremental edits and
  alternating comparison order. The ninth protects the distinction between
  runtime canonical-manifest identity and the complete ZIP archive hash.

## Clean-build results

The final alternating window completed three pairs for Small and Demo. A
coordinated browser gate paused only outer orchestration while the final timed
Demo child continued normally to completion; its child timing, CPU, RSS and UTC
measurements are intact. The outer job was then ended after all timed children
had exited, before any new baseline began. The completed three-sample Hollow
architecture measurements were reused
instead of repeating expensive baselines, as requested when the user asked to
shorten the remaining work. One final-binary full-quality Hollow spot check
independently confirms exact output. The older architecture median and final
single sample are labelled separately rather than combined into a false median.

| Map | Before median | After median | Time saved | Reduction | Speedup | Repeats per side |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Small `test_room` | 8.002 s | 5.154 s | 2.848 s | 35.59% | 1.553× | 3 |
| `places_demo` | 1,203.784 s | 227.746 s | 976.038 s | 81.08% | 5.286× | 3 |
| `lantern_hollow`, repaired architecture | 444.521 s | 256.102 s | 188.419 s | 42.39% | 1.736× | 3, earlier window |
| `lantern_hollow`, final binary spot | 444.521 s retained baseline median | 246.247 s single sample | 198.273 s indicative | 44.60% indicative | 1.805× indicative | final: 1 |

| Map | Before median CPU-seconds | After median CPU-seconds | Before median effective cores | After median effective cores | Before median peak RSS | After median peak RSS |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Small `test_room` | 52.810 | 34.060 | 6.656 | 6.469 | 337,625,088 B | 289,783,808 B |
| `places_demo` | 11,023.001 | 1,549.782 | 9.154 | 6.778 | 2,221,867,008 B | 2,485,846,016 B |
| `lantern_hollow`, earlier median/final single | 3,631.270 | 2,226.781 | 8.173 | 9.043 | 2,416,967,680 B | 2,447,163,392 B |

Each metric is a median of its own per-run measurements. Demo's baseline wall
samples are 1,702.939 / 1,172.789 / 1,203.784 seconds; optimized samples are
227.746 / 228.741 / 217.882 seconds. All remain included. Different effective
core availability makes the wall-time result a shared-host observation; CPU time
also falls by about 7.11×. Demo's median RSS increases by about 11.9%, a measured
tradeoff rather than an unreported memory improvement.

All three Demo pairs passed strict physical-record equality, and each side's
complete archive hashes are identical across repeats. Sources and quality settings
remain identical. The first two pairs were compared automatically; the third was
also compared with the recorded background host load.

Hollow's repaired-architecture samples were 255.099 / 256.102 / 285.253 seconds
with median CPU time 2,291.408 seconds and median peak RSS 2,080,751,616 bytes.
The later final spot sample used 2,226.781 CPU-seconds and 2,447,163,392 bytes.
It is not a three-run final-binary median, nor a same-window paired speedup.

Hollow's final single-sample resident peak is about 1.25% above the earlier
baseline median; its repaired-architecture median was about 13.9% below it.
Different windows and memory pressure prevent attributing that difference solely
to the final kernel specialization. No universal RAM reduction is claimed.

## Worker scaling

The frozen compiler rebuilt all three Small qualities three times per worker
count. All 15 complete archive hashes were identical:
`077a5fb7e9002d5acb74e9ff8ce00005246a2d847f66a50e4ad1e3be8305e571`.

| Workers | Small median wall | Median CPU-seconds | Median effective cores | Median peak RSS |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 26.188 s | 26.082 | 0.996 | 330,842,112 B |
| 2 | 13.997 s | 26.902 | 1.922 | 376,979,456 B |
| 4 | 7.700 s | 27.183 | 3.530 | 375,881,728 B |
| 8 | 4.614 s | 27.584 | 5.975 | 372,555,776 B |
| 12 | 4.188 s | 33.502 | 7.992 | 373,243,904 B |

Twelve workers give a 6.25× wall-time improvement over one on this complete
fixture; the heterogeneous cores and serial stages prevent ideal 12× scaling.
Eight workers cost less CPU time but take longer. The default uses available
parallelism bounded by 12, honors a smaller configured budget and supports an
explicit single-worker fallback. No nested pool exceeds that shared budget.
Large-map Medium measurements at the same counts are recorded separately;
single samples characterize scaling without pretending to be stable medians.

| Workers | Hollow Medium wall, one sample | CPU-seconds | Effective cores | Peak RSS |
| ---: | ---: | ---: | ---: | ---: |
| 1 | 239.263 s | 239.028 | 0.999 | 1,762,050,048 B |
| 2 | 130.289 s | 241.791 | 1.856 | 1,841,987,584 B |
| 4 | 75.956 s | 246.539 | 3.246 | 1,642,446,848 B |
| 8 | 49.944 s | 261.160 | 5.229 | 1,751,203,840 B |
| 12 | 44.738 s | 301.994 | 6.750 | 1,828,470,784 B |

All five complete Medium archive hashes are identical:
`b7931974e9053c209a959c24d576db535337e2f8ec727731572f322a7787f11a`.
The 12-worker single sample is 5.35× faster than one worker; eight again uses
less CPU but takes longer. These observations support the bounded automatic
default on this Mac, while user budgets remain configurable.

## Incremental builds

Each of the following all-quality cases starts from a fresh copy of a verified
clean package and has three timed trials. Every result is byte-identical to an
independent forced clean build of precisely the edited source. Identical repeated
edits use the same hash-identified clean reference; correctness comparisons are
outside timing and no repeated result skips comparison.

| Small edit | Baseline median | Final median | Reuse decision |
| --- | ---: | ---: | --- |
| Unchanged | 0.314 s | 0.317 s | Valid complete archive retained |
| Name/author | 6.063 s | 0.527 s | Final reuses validated geometry/lighting/probes/collision |
| Move light 0.125 m | 6.377 s | 4.809 s | Complete physical stage rebuilt |
| Move prop 0.125 m | 6.070 s | 4.302 s | Complete physical stage rebuilt |
| Change floor material | 5.966 s | 4.400 s | Complete physical stage rebuilt |

Small metadata rebuilds are 11.51× faster than baseline without stale lighting.
Unchanged-package performance is effectively unchanged. Metadata peak RSS falls
from 474,857,472 to 84,115,456 bytes; CPU time from 51.497 to 0.492 seconds.
Hollow unchanged/metadata checks also passed all six byte-exact comparisons:

| Hollow case | Median wall, 3 trials | Median CPU-seconds | Median peak RSS |
| --- | ---: | ---: | ---: |
| Unchanged archive | 2.192 s | 2.090 | 311,197,696 B |
| Name/author metadata | 7.786 s | 7.739 | 681,984,000 B |

The metadata median is 31.63× shorter than the final 246.247-second clean spot
sample; that comparison is a three-trial incremental median against one clean
sample, explicitly not paired clean medians. One independent forced all-quality
metadata bake protects every repeated identical edit. In total, 36 incremental
trials passed exact clean equality (15 baseline Small, 15 final Small, 6 final
Hollow). Large-map physical-edit incremental medians are not claimed: those edits
invalidate global lighting, as they must.
The observed decisions were nine complete-archive hits, six permitted prepared
stage hits and 21 complete rebuilds. All 18 physical edit trials rebuilt; the
remaining three rebuilds were baseline metadata edits. No unsafe lighting reuse
was observed.

## Remaining measured costs

These gains are cumulative measurements, not isolated attribution of each
optimization. Earlier exact-output prototypes recorded Demo at 535.210 seconds
after preparation/layer/cache reuse and at a 184.531-second median after the
repaired ray architecture; their background-load windows differ from final
paired measurements. A separate deterministic **development-profile** ray
kernel diagnostic measured 2.34× Demo and 2.01× Hollow representative-ray speed
with identical hit results; it is not presented as a release bake benchmark.
There was no full factorial ablation campaign. Process peak RAM, phase timing,
CPU time and context switches were collected; per-allocation counts and
per-thread lock-wait time were not instrumented.

In final Demo pair 3, indirect transport still takes 153.390 seconds of 217.882
total (70.4%). Direct visibility takes 18.720 seconds, filtering 9.236,
architecture/charts 11.760 and prop preparation 4.794. In final Hollow, indirect
transport takes 173.763 seconds of 246.247 (70.6%), direct visibility 20.017,
prop preparation 30.430 and architecture/charts 3.297. Its Full bake still
evaluates 461,531,776 gather directions; horizon rejection is included in that
count, so it is not asserted to be the number of triangle intersection calls.
Medium evaluates 70,314,880 gather directions. Ray traversal remains the measured
hot path; finer dependency-safe geometry caches or portable vector kernels are
possible further work, rather than grounds for silently removing samples.

The measured Demo gain exceeds the 3–5× objective. The retained Hollow median
improvement is 1.736× and does **not** meet 2×; the final single sample is
indicatively 1.805×. This report does not claim that the remaining transport is
at a theoretical hardware limit. Substantial duplicate preparation/query work
has been removed, the preserved quality is computationally expensive, and no
further speculative architecture changes were made after the user requested
shortening the task.

## Final validation

All commands used Rust **1.99.0 (b940084d7 2026-09-28)** and Cargo
**1.99.0 (5f94df478 2026-08-27)**, `RUSTC_WRAPPER=` and
`CARGO_NET_OFFLINE=true`. Compiler regression controls retained their distinct input/package identities.

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | PASS, exit 0 |
| `cargo check --workspace --all-targets --all-features` | PASS, exit 0 |
| `cargo test --workspace` | PASS: 1,975 library tests, 0 failures, 23 ignored; binary tests 1 + 3 passed |
| `cargo test --workspace --all-features` | PASS: 1,975 library tests, 0 failures, 23 ignored; binary tests 1 + 3 passed |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo` | PASS, exit 0; configured policy unchanged |
| `sh tools/verify.sh` | PASS, exit 0; 1,345.436 s total |
| `git diff --check` | PASS, exit 0 |
| `python3 -m unittest tests.test_compiler_bench` after capture assertion correction | PASS: 9 tests, 0 failures |

The complete gate also passed asset validation/inventory, maintained model/clip
checks, four idempotent map-authoring checks, all four bundled package
build/currentness/decoding checks, 20 showcase Python tests, 48 package tests,
23 packaging/accessor/asset tests, 49 tooling/generator/benchmark tests,
14 geometry-repair tests, 10 compiled-build tests and 26 native renderer tests.
Focused Rust preflights passed 9 showcase, 19 static-prop, one explicit large
atlas-planning test and both native low-lighting tests. Expected injected-worker
failure text in the Python suite is a tested refusal path; all suite summaries
are successful.

The complete gate ran after the final Rust/bake implementation. The later small
Python capture assertion correction changed no Rust or bake code; it received
its focused nine-test run and the real native capture comparisons. The earlier
incorrect-assertion attempt is retained separately. Runtime identity hashes the
canonical manifest, while the harness independently retains full archive SHA-256
and verifies the exact loaded source path, selected variant and zero runtime
preparation work. Neither identity nor loading checks were removed.

## Native output and playable evidence

The same saved executable rendered baseline and final packages at authored frame
1 through the real Apple M2 Pro **Metal** backend. Ten fixed views (six Demo,
four Hollow) were captured at each of Low, Medium and High: **60 captures,
30 before/after pairs, zero changed pixel channels**. Every capture is 1,280×720
pixels from the same 640×360 logical window, camera, spawn, quality, bloom and
vsync settings. Every run verified the selected canonical manifest identity,
full archive hash, exact package source path, requested variant and zero runtime
lighting/prop/surface/atlas preparation. Representative office, pool and night
street images were also visually inspected. No GPU validation/device errors
were observed.

Every one of the **29 distinct preserved fixture packages** also opened in the
saved native game: 29 Low runs plus six representative Medium/High runs,
**35 profile cases passed**. Invalid source and corruption reproductions remain
separately labelled expected failures; their last valid packages stay playable.
All four final production bundles passed strict physical-record comparison with
the pre-audit bundles after their final identity rebuilds. There is no unexplained
numeric or visual output difference.

After the Python identity assertion correction, the complete applicable
tooling/generator/benchmark Python group was rerun: **50 tests passed**, zero
failures, 14.621 seconds. No Rust/bake implementation changed after the full gate.

Typed worker errors and controlled archive failures return compiler errors before
publication. Atomic ZIP writing removes failed `.partial` files and preserves
the previous package. The repository's existing release `panic = "abort"` policy
remains: an unexpected panic aborts the process before publication rather than
returning a joined-worker error. Unwinding test builds exercise the joined-worker
failure path. The audit does not claim graceful recovery from process termination.

Shared light layers retain separate radiance buffers to preserve accumulation
order. This can increase peak resident memory; measured figures are reported
above. There is no claim of universal memory reduction or safe
partial lighting reuse after physical edits.

## Reproduction inputs

The dated comparisons require the original authoring sources, analytic fixtures,
matching catalog/assets and valid packages, plus the saved player/compiler and
local SDL dependency. A later checkout or screenshots alone cannot reconstruct
these controls. The independent archive retains 29 distinct valid fixture
packages and separately labelled expected-failure inputs.

```sh
python3 debug-maps/compiler-audit-20261003/launch.py
```

Running the preserved launcher without a map argument lists its fixtures. Use
`tools/bench/compiler_bench.py` and the canonical verification guide for new measurements;
retain the exact source/asset inputs and distinguish canonical runtime manifest
identity from the hash of the full ZIP archive.
