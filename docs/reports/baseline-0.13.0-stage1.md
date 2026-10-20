# Places 0.13.0 stage 1 baseline and reproduction handoff

Completed 2026-10-20 on `0.13.0-staging`, baseline source `02120c79`.
This stage records existing behavior and measurement limits; it fixes no game
bugs. Stage 2 should begin with targeted core-bug reproductions below. Model
rebuilding remains paused. No release, main merge, map bake or asset edit was
performed during recovery.

## Evidence and measurement conditions

Raw evidence remains outside `target` at `local/stage1-0.13.0/`: `checkpoint.json`
contains metrics, `checkpoint-evidence.json` contains selected native action/body
events, and each case retains settings, environment, native log, frame/player CSV,
load trace, identity and resource receipts. Diagnostic cases also retain physics
traces and captures. `compiler/report.json` and `package-storage.json` retain
compiler commands/hashes and storage accounting. These local files are not part
of this tracked report; preserve them on this Mac for subsequent stages.

- MacBook Pro Mac14,9, M2 Pro (12 CPU / 19 GPU cores), 16 GB unified memory,
  macOS 27.0.1 (26A434), Metal; Rust/Cargo 1.99.1.
- Direct native preserved player `local/release-0.12.0/source-binaries/places`,
  SHA-256 `89691692caff33853ef02cf70610d28a4e1883519bb40b112c49bdd21f3e4908`.
  Only release documentation changed between its build source `7240e2f9` and
  `02120c79`; stage 1 changes only developer tooling/reporting.
- Windowed **640×360 logical / 1280×720 physical drawable**, High texture quality,
  filtering and lighting, Full lightmaps/reflections, bloom on, 60° FOV.
- **V-Sync off, no frame limiter in successfully presented gameplay**. Logs show
  the actual switch to Immediate and BENCH `swap_interval=0`; the initial Fifo
  renderer line precedes settings application. Accepted runs have zero missed
  ready surfaces. FPS exceeds the 120 Hz display rate and many frame times are
  below 8.33 ms, establishing uncapped throughput rather than a refresh plateau.
  Compositor scheduling remains visible in render/loop tails.
- Static viewpoints use fixed 1/60 simulation delta; explicit `--real-delta`
  reproductions use wall-clock movement. Item physics uses its normal clock.
  Match environment/settings and trajectories before comparing later changes.
- Recovery checked all 15 saved CSV summaries and runtime member-manifest
  identities against native logs and preserved packages, plus drawable/presentation
  evidence. All four storage reports exactly match read-only recomputation.
  No game/compiler measurements or passed builds were repeated.

## Throughput and resource baseline

FPS is `1000 / mean(loop_ms)`. CSV percentiles use nearest rank. GPU means
deduplicate sample IDs; GPU spans include scheduling stalls and are not GPU
utilization. CPU stages below are wall times, not CPU utilization. Peak RSS is
the whole process high-water mark, including startup/loading.

| Saved case | Frames | FPS | Loop mean / p95 / p99 ms | CPU update / render mean ms | GPU mean ms | Peak RSS MiB |
| --- | ---: | ---: | --- | --- | ---: | ---: |
| hub-r1 | 240 | 202.43 | 4.940 / 14.735 / 17.509 | 0.225 / 4.380 | 1.174 | 437.3 |
| home-idle-r1 | 900 | 208.06 | 4.806 / 13.411 / 15.768 | 0.334 / 4.137 | 1.203 | 473.2 |
| pool-far-r1 | 900 | 172.28 | 5.805 / 14.585 / 16.629 | 0.330 / 5.145 | 2.649 | 512.0 |
| pool-near-r1 | 900 | 166.43 | 6.008 / 13.723 / 16.542 | 0.286 / 5.451 | 2.093 | 514.0 |
| arcade-r1 | 900 | 175.02 | 5.714 / 14.425 / 14.824 | 1.366 / 4.164 | 2.503 | 1266.9 |

These are short, single-run baselines without confidence intervals or thermal
controls, not optimization gains. Pool far/near are different authored views.
GPU utilization and driver allocation counters were not collected. Unified
memory means RSS and inferred GPU allocations must not be added as independent
physical pools. Home's native atlas report is 67,108,864 bytes including its light
state; the inspector's 33,554,432-byte base-page shape is not total residency.

Action/capture/physics-trace runs are diagnostic and include their stalls. Home
cushion/carry/books cases averaged 205.26/194.50/172.02–183.67 FPS without a
sustained severe collapse. `pool-release-r1` averaged 85.83 FPS (loop p95 17.970 ms,
CPU update mean 9.814 ms); its interaction/capture workload needs phase profiling
before assigning a cause or comparing it to steady viewpoints. Ramp correctness
evidence failed the scene-submission timing gate and supplies no accepted FPS.

## Reproductions and stage 2 targets

| Report / saved case | Observed behavior | Remaining targeted work |
| --- | --- | --- |
| Relay ramp fall-through (#27), `tower-ramp-r1` | Far rising ramp x15.5–18, z2–8, offset 6.4 / rise 3.2. Start eye (16.75,8,1.65), yaw180, ordinary forward and jumps at 0.9/1.8/2.7/3.6 s after ready. Eye falls below support (y8.117,z4.950 at 2.57 s), reaches y−2.065, then void recovery increments reset_count to 1. | Isolate ramp support/contact failure with a small fixture; do not assume the stairs share its cause. |
| Pool dormant contact (#27), `pool-contact-r1` | Forward/jump into wet-floor sign near (18.65,0,14.5): eye y1.707,z14.876, all 19 bodies stay dormant, forward progress stops. Backward input escapes to z15.55. | Compare already-active prop contact and escape paths. Failed activation is observed; a persistent trap is unproven. |
| Relay technician walking in place (#28), `tower-npc-r2` | Ground-floor tower_tech remains at (3,0,4), automatic clip pose_stand → walk while player walks → pose_stand on stop; native idle/walk/stop captures retained. | Isolate fallback to player locomotion in character advancement. Upper technician/witness have explicit cues; do not generalize to every NPC. |
| Required supply in well (#28), `well-r1` | Ordinary E pickup; diagnostic Player placement at lip (12,1.6,13.3), then ordinary E drop. Supply sleeps at (12.179,−2.216,15.839); repeated aimed E from tested lip fails to retrieve/reset it. | Check far lip/crouch and intended recovery. Local reach/progression obstruction is reproduced, not impossibility of every recovery route; player well_catch and whole-visit reset are distinct paths. |
| Second-room ceiling (#29), `tower-ceiling-r1` | Native `second-room-ceiling.png`, eye (3,4.8,6), yaw0/pitch70, shows severe black striped/coplanar artifacts. | Obtain geometry/export checker evidence before choosing a fix; no rebake establishes the cause. |
| Home cushion/books slowdowns | Pickup/drop and moving carry were exercised without sustained collapse. Book stacks were diagnostically placed on the bed and jumped onto; books end sleeping. | Confirm exact book/body support contacts and original collision trajectory before declaring trigger coverage. Neither fixed nor disproved. |
| Central stair failure, `tower-stairs-r1` | Forward + four jumps reaches south landing eye y4.8,z9.7, reset_count0. | Side/tread-edge, descending and variable-time paths remain untested. Successful tested ascent does not disprove the report. |
| Raw RELEASE BLOCKED HUD text | Bin pickup/drop near closet shelf succeeded in `pool-release-r1`; raw diagnostic emitter is present in `src/game/app/item_runtime.rs`. | Reproduce an actual blocked release and its HUD text; attempted capture name is not proof of triggering the fault. |
| Long offscreen error text | Source review found two-line menu status and fit_text in small_line. | Native long-error reproduction and other message surfaces remain unvalidated. |

Scripts `tower-ramp-actions.json`, `tower-stairs-actions.json`,
`tower-npc-view-actions.json`, `well-actions.json` and per-case environments are
retained in the local evidence root. Diagnostic placement is explicit above;
these are not claims of an entirely ordinary user traversal. Failed setup
`hub-probe` (wrong member ID) and `tower-npc-r1` (AimEntity before entity frame)
were corrected in subsequent cases and establish no game defect.

## Storage and compiler controls

Collection `assets/levels/environments.placesmap` is **449,857,977 bytes**, SHA-256
`9e78fbe3f6a2cf00f19f410da76271f1acb6c6b14d91b2e5ba0dd8682e532ee8`.
Static-only compressed payload beyond Full is **38,388,876 bytes (8.53%)**,
40 records / 101,476,379 logical bytes. Lantern Hollow accounts for 34,458,404
compressed bytes, Home 103,923, Pool 562,294 and Arcade 2,183,912. Full-only payload
is 273,487,654 compressed / 977,381,763 logical bytes; shared payload is counted
once across members. Static has distinct vertex-lit/prop recipes and switched
states, no lightmap atlas. Both modes remain preserved.

Other audited archives: Relay Annex 24,434,167 bytes (Static-only 82,412), Closing
Time 34,116,781 (44,597), Beam 6,314,015 (23,344). Costs are existing compressed ZIP
payloads including referenced PPS1 shared segments, excluding hypothetical
re-export metadata/index/header changes and external catalog assets. Per-member
totals can overlap. Installed/cache allocation was not inventoried; historical
release ZIP size 687,722,141 bytes was not rehashed in stage1.

Compiler: preserved `places-compile`, SHA-256
`8bdb4951d2ef69ce35c57dfe9abeb7897df48c25db1aaca17b78f745c9985c4b`,
`--workers 6 --variants off,full --force`, fresh process/package per sample,
120-second bound, no concurrent owned build/runtime/bake.

| Tiny existing fixture | Wall samples s | Median wall / CPU s | Median peak RSS MiB |
| --- | --- | --- | ---: |
| test_room.json | 8.070 / 7.804 | 7.937 / 38.580 | 278.2 |
| material_transport.json | 6.509 / 6.429 | 6.469 / 31.724 | 322.2 |

test_room Full prepare/encode was 7.549/7.609 s versus Static 8.451/7.833 ms;
material_transport Full includes offline reflection capture. These controls do
not estimate shipped collection bake time or editor latency. Cache-hit/reuse
timing and deterministic output equality remain gaps for the storage/compiler
stage, not reasons to repeat this baseline campaign.

## Tooling review and delivery scope

`tools/bench/frame_time.py` adds explicit package/member identity verification,
wall-clock interaction option, physics diagnostics, mean/FPS and process CPU/RSS
receipts. `tools/bench/package_storage.py` is a read-only trusted-output inspector
with bounded metadata reads, shared-segment accounting and explicit caveats; it
is not an untrusted-package validator. Two tiny ZIP/log identity regressions in
`tests/test_compiler_bench.py` protect acceptance of the planned member/content
and variant without baking or GPU cost. No Rust tests were added.

Saved validation in `tooling-validation/status.json`, `review.json`,
`python-checks.log` and `offline-review.json` confirms format exit 0 (1.51 s), strict
workspace/all-target/all-feature Clippy exit 0 (24.97 s), locked all-feature release
build exit 0 (88.06 s), Python syntax and 37 relevant tests passed (0.673 s).
Recovery verified all three tooling source hashes still match that review;
there is no build invalidation. The unrelated Python audit/inventory reports
remain untouched. No surviving game/compiler/Cargo/benchmark writer was found.
All local raw outputs and preserved release binaries remain available; no cleanup
was performed. Remaining reproductions belong to the subsequent bug-fix stages.
