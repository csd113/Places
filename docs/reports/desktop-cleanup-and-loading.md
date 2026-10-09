# Desktop cleanup and loading

Implementation and verification record for the September 2026 corrective pass.
The corrective pass stopped with an unbuilt, untested final pacing edit.
Completed measurements and that remaining acceptance gap are distinguished below.
The [feature summary](feature-expansion-summary.md) preserves the preceding
gameplay and tooling delivery.

## Baseline and measurement scope

The baseline includes the user's uncommitted demo, authoring-guide, geometry and
continuity-test changes. A tracked-source copy was preserved outside the checkout;
the ignored Pit input was copied separately before its corrected measurement.
No branch, worktree, commit, user cache purge, or system cache purge was used.
Measurements run serially on macOS / Apple Silicon M2 Pro. Application-cache cold
means a fresh isolated state directory, not cold operating-system or GPU caches.
The controlled release harness uses High, VSync, a 640×360 logical window, camera
74°, and four ready frames. Effective renderer configuration was recorded independently.

The baseline Rust gate compiled in 12.85 seconds. Its cached full invocation took
1140.17 seconds wall time, with 1139.35 seconds reported test execution: 1288 passed,
2 failed and 8 ignored (1298 unique tests). Failures were the allocation-estimate
ratio assertion and cross-room wall continuity.
Over-60-second notices identify candidates, not independently measured durations;
their times must not be added to claim suite wall time.

## Retired implementation and coverage

The standalone `apps/places-pocketchip` implementation and exclusive files were
removed. It was outside the desktop workspace and normal desktop gate, so this
removal saves **zero measured desktop-suite execution time**. No desktop backend,
Linux/ARM support, low-quality setting, or low-resolution safety coverage was
removed. The root dependency lockfile did not need changes.

## Runtime preparation

A single persistent preparation worker owns level reading, immutable asset
snapshots, geometry, lighting, collision-world preparation and character initialization.
Final collision-index/state installation still runs on the main thread. One active
request and one replaceable pending request are bounded by generation IDs. The UI
pumps and presents while preparation runs; cancellation and supersession reject
obsolete completions. Shutdown cancels work and continues pumping until the worker
can be joined. Direct selection prepares the requested level without first building
the demo. The previous complete world stays paused until a compatible replacement
is ready, and input edges and simulation timing are cleared across transitions.

Conservative room and light candidate indexes preserve original predicates and
candidate ordering. Prop expansion samples identical source positions once per
placement while preserving every vertex, UV and color. These remove actual repeated
CPU work rather than merely moving it behind loading feedback.

GPU installation retains the old world while atlas, geometry, textures, materials
and bounded prop batches are uploaded. Finalization and resource disposal still
require large-level trace acceptance; the existence of these phases alone is not
proof of a bounded frame time.

## Cache contracts

Prepared geometry uses exact prepared level/material/catalog/model-input content,
asset root and effective lightmap quality. Its LRU retains at most three builds and
192 MiB of accounted retained data; oversized worlds work but are not retained by
that LRU. Up to eight weak build identity keys can additionally retain raw model-byte
Arcs outside the 192 MiB budget until the next request prunes dead identities.
Active resources remain independently owned. Lightmap memory retention is
four entries / 128 MiB. Disk atlases use a versioned, length-checked, checksummed
single-file envelope, bounded metadata/pages, and atomic same-directory publication.
Corrupt, outdated and interrupted entries miss safely. Semantic version 10 invalidates
old topology after covered wall faces and unnecessary rectangular-cap diagonals
were removed; map content and lighting sampling thresholds were not changed.

PNG identities include encoded content and embedded model images include dimensions
and RGBA content. Exact model-input snapshots detect same-path/same-mtime edits.
Optional prior texture retention is trimmed at load boundaries to 256 entries /
256 MiB for CPU and GPU caches, without evicting assets midway through one level's
resolution. Active level resources can exceed those retention budgets. Model caches
retain the current request's dependency paths rather than accumulating every visit.

## Verification cost

Six read-only render tests now share immutable demo builds by quality (eight complete
bakes become three). Their assertions, including independent atlas refill, remain.
The doorway ownership test no longer repeats the separate full-map coincidence
audit. The light-range audit performs its required independent fill without first
filling an unused complete atlas. Report-only sorting is opt-in. Texture validation
runs once through the package suite, and one duplicated zoo CLI freshness check was
removed while its package-suite equivalent remains. Python worker controls and the
shared execution budget are preserved.

## Reproduction

Run the authoritative gate from the repository root:

```sh
sh tools/verify.sh
```

Measure an already-built binary with fresh application state (output must not exist):

```sh
python3 tools/bench/loading.py --binary target/release/places --out /tmp/places-loading-run
```

The same harness accepts an already-built debug binary. Do not include compilation
in runtime comparisons. Native lifecycle cases are in
`tests.test_wgpu_bootstrap.WgpuRuntimeSmokeTests`; the action fixture exercises actual
SDL event processing and the actual preparation worker under an injected delay.

## Targeted checks and acceptance limits

Strict Clippy passed after cache/loader integration. Prepared-build identity/LRU,
exact model refresh, texture retention and active-ownership regressions pass.
Five native small-fixture scenarios previously passed: direct launch, resize/cancel/
retry, shutdown during preparation, supersession and failure recovery. Their maximum
observed event-pump gap was 42 ms and scripted event latency ranged 0.4–10.3 ms.
These are small-fixture results, not claims about dense-level upload behavior.

The final gate result is recorded below. Large-level cancellation/disposal, a
manual gameplay walkthrough, and baseline/current visual comparisons have not
been completed. Linux/Windows runtime checks were unavailable: this host has no
running Docker daemon or corresponding native runtime. These are acceptance
limits, not passing checks.

## Final controlled runtime measurements

Already-built binaries, one cold/warm pair per level; seconds include process
startup, readiness, four rendered frames and clean exit. These are individual
observations rather than statistical confidence intervals. Renderer configuration, first-ready presentation, phase records, RSS and event
gaps were recorded with each invocation.

| Entry | Before cold | After cold | Before warm | After warm |
|---|---:|---:|---:|---:|
| menu | 29.337 | 15.957 | 2.226 | 1.294 |
| places_demo | 29.191 | 15.704 | 2.250 | 1.309 |
| model_zoo | 37.010 | 6.492 | 2.841 | 0.784 |
| level0_pit | 36.560 | 3.550 | 2.457 | 0.494 |
| capacity_sparse (subsequently retired) | 31.196 | 1.000 | 2.299 | 0.375 |
| capacity_dense | 78.992 | 27.839 | 30.929 | 11.418 |

Unoptimized debug normal startup improved from 346.570 to 181.764 seconds cold
and from 26.885 to 13.811 seconds warm. Debug first present was 2.386 seconds
cold / 0.562 seconds warm, so debug still has a noticeable initial setup delay.
Cargo profiles were not changed. Release
loading feedback first presents in 168–214 ms across these runs; this is distinct
from full world readiness. Dense warm loading still takes 11.4 seconds: the disk
atlas hit avoids baking, but geometry/props are rebuilt in each fresh process.

The initial window/device setup still accounts for gaps above 100 ms. Once the
loading loop is running, the demo cold trace has p99 event spacing of 8.925 ms;
its largest event gap is the initial 162.746 ms. Final GPU work is not universally
bounded for arbitrary imported content. Earlier serial-fill representative traces
had upload steps up to 62.5 ms; small staged-cancel disposal measured 0.044 ms.
No dense-cancellation disposal bound is claimed.

The final atlas fill uses at most three workers, leaving CPU capacity for the
UI. Each has a one-result queue and processes charts in source order; the writer
consumes original order and retains identical texel arithmetic and atlas output.
Layout validation precedes producer allocations, and cancellation is checked per
row. Shipped presets bound chart buffers to about 84 MiB including the consumer,
plus atlas storage; arbitrary custom configuration is not universally size-capped.
Explicit two/three-worker versus serial output tests pass. A proposed greedy
assignment was rejected because an ordered-queue model predicted worse completion;
it was not installed. Runtime serial reference is available using
`tools/bench/loading.py --workers 1`. No final same-binary serial timing sweep was
completed; the earlier serial-fill matrix is preserved separately, not presented
as an isolated parallel speedup.

The first debug pilot and the second accidentally overlapping packaged-app run
are invalid; only `places-loading-before-debug-controlled` is used above.
No packaged prebake requirement was added. Cache misses remain supported.
No new dependencies, production art changes, preference migrations or commits
were introduced.

Cold-process maximum RSS decreased from 709.6 to 656.3 MiB for normal menu entry
and from 1428.9 to 672.8 MiB for the dense fixture. RSS is process resident memory,
not a GPU allocation or process-tree measure. Across release cold runs, measured
maximum event-pump gaps ranged 132.3–170.1 ms including initial setup. Sparse has
few observations and its p99 includes that initial interval; it must not be
summarized as universally sub-10-ms. Dense cold p99 was 8.915 ms. Synthetic action
latencies and repeated-visit RSS samples were recorded;
synthetic focus actions do not prove operating-system focus handling.

## Final integration corrections

The first complete integration run found seven failures (1317 passed, 7 failed,
8 ignored; 694.06 seconds test execution, 703.44 seconds invocation). This initial run was not a passing gate.
The seven targeted reruns all passed after these focused corrections:

- Empty scripted Escape/Quit variants now reject unknown JSON fields. Serde's
  internally tagged unit variants had silently accepted them.
- The material assertion now checks the full content-derived texture key and
  shared image allocation, rather than expecting an obsolete bare logical key.
- Z-axis horizontal caps, floating-box undersides and X-axis wall ends now wind
  outward, preserving per-corner attributes. Falling ramp sides use their surviving
  triangle for orientation when their first triangle is degenerate. The relocated
  winding regressions exposed these latent defects; their assertions were retained.
- The wall-lighting check selects the first exposed corner at x=48.2 after hidden
  overlap removal, using the same geometric tolerance and red-dominance assertion.
- Cache semantic version 10 invalidates prior atlas corner/normal arrangements.

The user's existing wall-coverage changes in `src/render/common/mod.rs` remain;
this pass additionally corrects the unrelated wall-end winding in that file.
The other three originally edited files remain byte-identical to the preserved
baseline. Production asset files remain unchanged.

The runtime table above was collected immediately before these final winding and
script-validation corrections (binary hashes are preserved). It measures the
implemented loader, caches and bounded fill; it is not a post-correction timing
rerun. Final-code validation is listed separately below.

## Stopped at the user's request

Implementation work was stopped on explicit request. This is **not a fully verified
completion**. No build or game process owned by this task remains running.

The last complete Rust run passed 1324 tests with 8 ignored in 683.44 seconds.
Formatting and strict Clippy passed at that point. Asset validation, prop freshness,
43 package tests and 34 tooling tests passed. Seven of eight compiled-build tests
passed; the stale empty-install log assertion was replaced and its targeted rerun
passed. All 23 native SDL/wgpu tests passed in 85.685 seconds.

Geometry checks: Demo exits 0 with one sliver warning; Zoo exits 0 with no findings.
Pit exits 1 both before and after: 16 errors and 76 warnings. All 16 error locations
are unchanged. Three sampled sliver warning locations differ after triangulation;
warning totals/categories are unchanged. No map or suppression rule was altered.
Dense accepted-upload cancellation and cold-fill cancellation passed. Their traces
then exposed an occluded-surface busy loop: skipped acquisition was treated as a
presentation. A final pacing/tracing correction is present in the working tree,
but its validation is **unfinished**. The latest strict Clippy invocation fails
`too_many_lines` in `src/main.rs::render_and_present` (101/100 lines). No blanket
lint allowance was added. The final pacing change has not been rebuilt or rerun
through native tests. Earlier passing results therefore do not certify that edit.

The latest change makes presentation report success explicitly, yields up to 16 ms
on unavailable surfaces, and distinguishes actual presentation in traces. It also
updates the empty-install assertion to check committed world identity alongside
its capture file. These edits need the remaining lint fix and affected checks.

The displayed timing table predates these final corrections. No final timing sweep,
manual gameplay walkthrough or baseline/current visual comparison is claimed.
Linux/Windows runtime verification was unavailable. No commits were created.

The source journal also records the preceding input correction: short key taps
are latched until simulation consumes their edge, with crouch height and a
placed-instance E action checked through the native player. That completed
feature acceptance does not certify the later untested loading pacing edit.
