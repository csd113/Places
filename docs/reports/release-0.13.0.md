# Places 0.13.0 implementation and validation report

This is the consolidated report for the approved 0.13.0 queue on
`0.13.0-staging`. Sections retain the evidence and limitations recorded at each
stage; earlier next-stage statements are historical handoffs. Raw local receipts
and direct binaries remain outside `target` in their original stage directories.
No main merge or release is implied by this staging report.

## Final staging acceptance — 2026-10-21

The approved queue is complete in staging, subject to the explicit platform,
delivery and GUI coverage limits below. This stage consolidates reports, corrects
source-check ownership and native acceptance tooling, updates Cargo/lock/changelog
to 0.13.0, and validates integrated candidates. It adds no gameplay architecture,
map artwork or enhancement backlog. No release, tag, main merge or deployment is
authorized or performed. Exact commits, remote verification, terminal CI, candidate
hashes and cleanup are recorded in `local/final-validation-0.13.0/FINAL-HANDOFF.json`.

### Reports and final source checks

There is one canonical release report for 0.9.0, 0.10.0, 0.11.0, 0.12.0 and
0.13.0, listed in the [report index](README.md). Fifty former feature/stage reports
were integrated: thirteen into 0.10.0, fourteen into 0.11.0 and twenty-three into
0.12.0. Unique substantive implementation, measurements, limitations and evidence
references remain; superseded queue instructions are retrospective. Technical and
authoring guides, distinct historical audits, raw checklists, images and original
local evidence remain. The two unrelated untracked Python audits are unchanged.
`report-mapping.json` and `reports-before/` retain the exact consolidation inventory
and original bytes outside target. All local Markdown paths and anchors pass.

The baseline CI failure on `b7459119` was an engine test including optional
`projects/relay-annex/scripts/relay_intake.luau`, while CI deliberately excludes
projects. The existing recovery test now uses a minimal local Luau fixture and
keeps its progress/reward/group-isolation assertions. Its scope is the shared host
contract; production project checking and native play cover the authored script.
No Rust tests were added and no map bake was added to Cargo or CI.

The first source Python run found obsolete constructor goldens after accepted
model/art adoption. Six redundant tests tied to superseded byte layouts were
removed; useful source-geometry, topology, contact, asset and clip checks remain.
The delivered ghost cat no longer satisfies its historical COLOR_0 construction
heuristics. Accepted Aero native atlases now require exact ledger model identity
and standalone/embedded PNG parity; the accepted cockroach verifies its ledger
identity and GLB structure and refuses constructor overwrites. No GLB or PNG bytes
were changed. The packaging fixture now includes the required editor preview
folder. Native installation tooling waits for actual ready-world wall-clock input,
capture and quit, and expects the current actionable missing-dependency refusal.
Earlier failed attempts and their evidence are preserved.

| Gate | Final result |
| --- | --- |
| Formatting and locked all-target/all-feature source check | Passed |
| Strict workspace/all-target/all-feature debug and release Clippy | Passed with `-D warnings`, no lint policy reductions |
| Essential Rust suite | 245 passing, no failed/ignored cases or shipped-map bake |
| Locked release workspace/all-feature build | Passed; initial build 194.702 s; final incremental gate passed and preserved executable hashes match |
| Source Python | 366 passing, three explicit optional skips, 120.547 s |
| Asset catalog and maintained generators | 621 assets, zero warnings; generator checks passed |
| Production Luau | Eleven bounded refusals and four finished-project checks passed, no map bake |
| Native integrated assertions | 36 core plus 22 additional native/package assertions passing on the final production executables |
| Hosted CI | Source checkpoint `5fcb8514` passed [run 39614334703](https://github.com/csd113/Places/actions/runs/39614334703); final tooling checkpoint `a7a5b2d4` passed [run 39620501057](https://github.com/csd113/Places/actions/runs/39620501057). The final documentation commit and terminal CI receipt are recorded in the handoff |

This Mac uses the documented `RUSTC_WRAPPER=` override because its restricted
sccache cannot start; Rust/Cargo are 1.99.1. The native host is Mac14,9 / Apple
M2 Pro / 16 GB / macOS 27.0.1 (26A434), arm64, Metal. Hosted source CI on macOS
does not establish Windows/Linux execution or physical GPU acceptance.

### Integrated native acceptance

The final editor/browser campaign verifies bounded actual-asset thumbnails,
pagination/search/no-match and material controls. A copied two-member project
reports Workshop as metadata-refresh while Second stays current, refreshes only
Workshop using verified prepared geometry/light/probe/collision reuse, then
reports both current. A later chair edit marks only Workshop for baking. Build
and checker cancellation preserve the last published package SHA-256
`b393520e4264af618377854dcf5644d868898887d398794a4921e5e30f17757d`.

The ordinary W/S stair route reaches eye Y4.95 on the 3.35 m upper landing and
returns to Y1.6 with zero void resets and noclip off. Structural-side captures and
current package checks retain the solid backing/landing geometry. Annex source
is unchanged in finalization: the current checker has **zero errors and 29
heuristic warnings** (14 missing-wall, 12 reversed-face, three room-leak), with no
suppressions. The editor stage's zero errors/48 warnings remain its historical
result. The intervening shared structural-edge fix removed duplicate upper slab
bands; the remaining warnings are not an exhaustive containment proof.

Final collection replays assert the runtime member-manifest SHA against the new
package, rather than only a matching level name. Home pickup/release preserves the
real cushion identity. Pool traversal covers submersion, resurface, rim exit and
reentry with zero void resets; moving-door/contact entry returns to the starting
corridor. A tiny authored current fixture moves the player east with actual fluid
velocity 0.8 m/s, then walks its sloped bank to dry floor without a water-jump or
reset. Relay Discharge swim/ramp exit produces finite audio and releases voices.
The original Annex upper-ramp trigger now reaches eye Y11.2 on solid support with
zero resets. Earlier probes that accidentally selected the older bundled collection
are preserved and explicitly excluded from final acceptance.

Final native ragdoll contact cases sleep all fifteen bodies for wall, stair and
table trials, with fourteen joints and zero sampled settled penetration. Carry
release preserves item identity and removes its socket attachment; explicit reset
releases joints and bone caches. Eight rigs own 122 solver bodies/112 joints,
paused bone positions remain exact, and single-step advances once. Real unload
records zero bodies/colliders/joints/ragdolls/caches, zero active prepared bytes,
zero scripts/VM bytes and zero retired prepared owners. Menu return contains no
ragdolls; reload restores the authored world. The previous independent long
settle/performance and 24-activation/reset campaigns remain applicable to the
unchanged production code. Explicit reset is not automatic stand-up; the recorded
9.25 cm transient grip overlap remains an explicit limitation.

The live-menu campaign completes two Explore → gameplay → pause/settings → return
round trips, empty Explore and small-window layouts, credits and style selection.
The displayed version is 0.13.0. Master zero persists while effects/ambience remain
0.8/0.6; restart captures 266,240 PCM samples exactly zero. Shutdown has no live or
queued voices or audio errors. The unchanged scene.bin hash preserves the prior
611-second/120-beat reset, five successful carriers, nineteen handoffs, fourteen
putdowns and bounded ownership evidence; a new full ten-minute recording was not
needed. The committed font-atlas wordmark and diagnostic-video limitations remain.

All new integration runs are functional diagnostics, often concurrent with the
explicit compiler export. Their frame summaries are not FPS comparisons. The
accepted performance campaigns remain the separate capture-free, V-Sync/limiter-off
runs with actual Immediate mode, physical drawable and pacing qualification. The
Pool CPU cause was unrelated moving doors/props globally refreshing water lighting:
accepted door update cost fell about 16.9 → 1.7 ms and entry/contact about
11.6–11.7 → 1.2–1.7 ms with exact visual parity. macOS presentation pacing remains
separate and unresolved. Historical +312 MiB residency remains unexplained even
though map-owned systems release. Editor Pool UI upload blocking fell 1595 → 79 ms;
total readiness stayed about 2.83 → 2.86 s. These limits and flat/regressed
measurements remain explicit; final acceptance does not claim every performance
or memory question is fixed.

### Distribution, delivery and remaining coverage

Final package/archive identities and all current member checks are in
`package-index.json`, `archive-final-receipt.json` and the final handoff. Candidates are
created separately from preceding stage distributions. All fourteen curated
members pass current-source verification and archive integrity. Exact declared
source recovery passes for all four projects. Separate lab and workshop packages
also pass; the 29 export/validate/current gates are in `package-gates-final.json`.
The full collection export is explicit developer work, outside Cargo/CI; it took
4,139.116 s on this Mac and establishes no before/after throughput claim.

| Candidate | Bytes | SHA-256 |
| --- | ---: | --- |
| environments.placesmap (nine levels) | 455,058,040 | `b798141bce7b4c682dba71cd44b35f76dd20d54ea1437745195f0628e0c86c6a` |
| relay-annex.placesmap (three levels) | 23,415,155 | `3858aa5de1def5c3cec5f0cd8aa199165dc186bdfdcbfad33ac84664d8f4d2d1` |
| closing-time.placesmap | 33,957,286 | `7d303b20e0d1805cf145232aa6e587eb92040028587f378e3e8160fb92a39cbe` |
| beam.placesmap | 6,235,122 | `ca43e27b7e38958ff4ea9fcd4926cd32a726fb7622e42a1069756bf39d1c04fa` |
| macOS arm64 final candidate ZIP | 706,535,175 | `e3eddb31e66f77525e165eefef096334e345d3023496892b98a95483ff7cc153` |

The ZIP is extracted into a path containing spaces and Unicode outside the
checkout. Every extracted file hash matches, the app signature verifies, and all
four campaign launchers run their expected world and controls. The extracted app
also loads Pool's shared/map resources and completes its entry/contact return with
no asset override. The final guide refresh preserves all 1,492 flat runtime files
byte for byte, updates README/installation text to the actual bundled-menu policy,
and repeats signature and extracted-file checks; the preceding ZIP remains intact.
This is a local candidate, not a published or notarized release.

Actual first-run/import
acceptance passes all 27 checks: empty live menu, writable folder setup, override
precedence, inbox import, duplicate/corrupt/conflicting packages, missing resources,
restart from another cwd, read-only installation, unavailable storage, Unicode
paths and exact settings/save custody. The historical task-1 fixture is preserved;
only a private copy's comment-only placeholder was adapted to the current empty
Luau-module contract.

A copied final editor runs from `/tmp/places-final-0130-5fcb8514/Places`, cwd `/tmp`,
with no asset-root override: edit → save → exit → reopen → build → sibling-player
import/play → return → export → exact JSON/Luau recovery all pass. The copied
map-free player presents the live menu and empty Explore from that installation.
Flat and .app metadata are 0.13.0; strict ad-hoc signatures and relocated SDL3
checks pass. The bundled menu scene is **8,022,170 bytes**, SHA-256
`e582c5e5cef1f360fcc1a61fae09847c0df78f93554aa51c599443b2fb3eb954`, and does
not depend on gameplay maps. Current acceptance supplies functional startup and
component size; it does not claim an isolated before/after startup-latency gain.

Windows/Direct3D 12 and Linux/Vulkan native build/runtime, Linux VM installation
and display-session checks, physical GPU coverage, clean-OS installers,
Developer ID/notarization and downloaded Gatekeeper trust remain unverified. The
repository exposes macOS source CI and no connected Windows/Linux runner/device.
Issues #17 and #21 stay open with these gaps. Cross-compilation or static checks
are not substituted for native execution.

The standalone final compiler GUI launcher executed, but its native UI
control/read tool timed out after 1375 seconds. The parent reported an approval
request; its resolution was not confirmed. No GUI bake/control action was sent,
no uncertain operation was retried and no approval/denial was bypassed. CLI builds,
installed compiler workflows and native editor build/reuse/cancel paths pass;
interactive standalone GUI coverage on this final candidate remains unverified.

`deliverable-index.json` preserves 35 earlier requested files / 102,340,418 bytes,
including editor screenshots/stair video, complete menu evidence and ragdoll clip
and editable export, with exact local paths/hashes. Final candidates and binaries
are indexed separately. The prior Library preparations repeatedly failed with
`prepare_uploads failed` and returned no confirmed IDs. Current supported tools
still require the same prepared route; no materially different legitimate recovery
was available and no identical retry was made. These local files are preserved,
not represented as delivered attachments or invented download links.

All evidence, source recovery, requested media, executables and candidates remain
outside target. Authorized cleanup follows completed Rust/build/native/package
work, terminal final-commit CI and a no-target-users check. `cleanup.json` and the
final handoff record the actual cargo-clean result and post-clean preserved launch.
Unfinished platform/delivery coverage does not authorize the deferred architecture
#31, player-model/multiplayer 0.14 work or game modes #25/#26.


## Stage 1 — baseline and reproduction

Completed 2026-10-20 on `0.13.0-staging`, baseline source `02120c79`.
This stage records existing behavior and measurement limits; it fixes no game
bugs. Stage 2 should begin with targeted core-bug reproductions below. Model
rebuilding remains paused. No release, main merge, map bake or asset edit was
performed during recovery.

### Evidence and measurement conditions

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

### Throughput and resource baseline

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

### Reproductions and stage 2 targets

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

### Storage and compiler controls

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

### Tooling review and delivery scope

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

## Stage 2 — core correctness

Completed on the connected Mac on 2026-10-20, continuing `0.13.0-staging` from
`f70dbd1b560a1ea759cd22fc76c7154a1bb72035`. Local HEAD and the live remote were
verified before editing and again before delivery. The previous stage was idle;
no competing game, compiler or Cargo writer was present. The unrelated Python
audit/inventory reports and all stage 1 evidence remain untouched.

The current #27/#28 descriptions, repository instructions, canonical map guide,
the Stage 1 baseline section above, checkpoint evidence, 15 native receipts and relevant
captures informed this work. The baseline matrix was not repeated. Raw stage 2
receipts, action scripts, captures, logs, summaries and direct binaries remain
outside `target` under `local/stage2-0.13.0/` on this Mac.

### Causes and changes

**Airborne ramp collision.** Horizontal terrain sweeps closed ramp perimeters,
but omitted the interior contour where the rising slope crosses the feet plane.
The airborne height query discarded the now-higher ramp and treated that space
as unsupported. Once inside the wedge, descent also discarded the surface above
the previous feet, allowing a fall through to the void. A tiny elevated-ramp
controller fixture reproduced this before the fix. The shared floor model now
emits that airborne contour and lands on the highest real slope point within the
circular feet footprint, using the circle's chord at side edges. Walking retains
its existing step rule. No map coordinates, invisible replacement floor, larger
recovery envelope or respawn workaround were added to the controller.

The reported stair failure remains less specific: neither stage 1's ascent nor
this stage's ascent, jumping descent and crouched descent reproduced it. Existing
riser/perimeter handling is retained; a small tread/side-edge landing regression
protects it while the shared terrain path changes. This is evidence for the
exercised paths, not proof that every reported stair trajectory is fixed.

**Dormant movable props.** The player controller stopped before dormant shapes,
while activation only considered explicit actions and active rigid-body impacts.
An all-dormant world then skipped solving entirely. Before that fast path, the
physics world now recognizes the player's exact shape contact, including its
small swept contact gap, and activates configured movable bodies. Fixed bodies
stay fixed. Pause/zero scale, disabled player collision, suspended bodies and the
existing player-penetration refusal remain enforced. Ordinary solver sleep after
activation remains distinct from authored dormancy. The established rule that
movable rigid props are pushed rather than controller-owned standable floors is
unchanged.

**NPC animation ownership.** Unaddressed placed characters explicitly inherited
the player's locomotion snapshot. That input is removed from the shared scene
update API. Each actor follows its own action cue, explicit locomotion input or
frame displacement; stationary skinned actors advance idle. Rigid animated props
retain their pose without an action. The player's visual representation still
receives its own simulation snapshot separately. Authored poses and intentional
stationary walk cues retain precedence.

**Required supply recovery.** Intake had a player-only well catch and whole-visit
reset, with no item-loss policy. The required box now has its own
`essential_supply` reset group. An exact-instance, unheld-item volume in the well
uses the existing atomic `reset_physics_group` action. The accessible well sign
also requests that group, providing recovery from other inaccessible placements
and retry after clearing an occupied return point. It preserves other objects
and objective state, creates no duplicate and refuses unsafe placement. After
acceptance, the sign preserves the delivered box; the existing delivery guard
prevents repeated rewards. Player falls still use the established visit reset.
This is authored policy using shared capabilities, with no per-map engine branch
or new editor/schema subsystem. Guidance and the authoring guide document it.

**Release and feedback.** `can_drop` treated every shape intersection as unsafe,
including a harmless resting overlap of ten microns. The focused fixture failed
before the change. Intersecting shapes now receive a distance check against the
existing `CONTACT_EPS` (0.1 mm); genuine penetration and failed shape queries still
refuse release and retain ownership. Camera clearance and swept carrying remain.
The ordinary HUD uses short feedback, “Make room to set this down.” or “The item
cannot follow here.” Raw diagnostic labels and the unbounded authored-item ID are
removed from that surface. Detailed body state and blocked-action events remain
in developer traces/authoring diagnostics. This fixes a demonstrated false
refusal, without claiming that every historical warning had that cause.

### Native validation

Direct preserved executable:
`local/stage2-0.13.0/source-binaries/places`, SHA-256
`7c70e8f76d4475de6fcdc0bb6e3def5e8c080d33f54e390554b2b8b025bd4497`.
MacBook Pro Mac14,9 / M2 Pro, macOS 27.0.1, Metal, High textures/filtering/lighting,
Full lightmaps/reflections, bloom, 60-degree FOV. All cases used V-Sync off and no
frame limiter, **640×360 logical / 1280×720 actual drawable**, actual Immediate
presentation and zero missed ready surfaces. Gameplay used real delta. The ramp
unit fixture additionally covers 30 Hz, 144 Hz and mixed intervals on positive
and negative slopes along both axes. Diagnostic captures and interactions are
correctness evidence; no FPS improvement or matched performance comparison is
claimed. No temporary app wrapper was used.

Starting placements and fixture manipulations below are explicit diagnostics;
walking, jumping, pickup, drop, throw and door input use the production paths.
`native-summary.json` provides compact selected events; per-case environments,
receipts and action scripts preserve the exact setup.

| Evidence directory | Result |
| --- | --- |
| `tower-ramp-r1` | Saved far-ramp start eye (16.75,8,1.65), forward and four jumps: reaches eye (16.75,11.2,9.70), feet 9.6, grounded, reset count 0. Stage 1's corresponding path fell and reset once. |
| `tower-negative-ramp-r1` | Middle negative-rise ramp: jumping ascent reaches feet 6.4 at the north landing; descent reaches feet 3.2 at the south landing, reset count 0. A separate side setup was displaced by spawn clearance near an adjoining guardrail, so it does not prove the intended side traversal. |
| `office-ramp-r2` | Correctly elevated east-shaft control: repeated-jump ascent reaches feet 7.2 and descent reaches feet 3.6, grounded with reset count 0. Existing environment package and geometry unchanged. |
| `tower-stairs-r1` | Repeated-jump ascent reaches feet 3.2; jumping and crouched descent return to feet 0, reset count 0. Outside-foot side approach is blocked safely; it does not establish an ascent along the full outer edge. |
| `pool-contact-r1` | Wet-floor sign contact changes 19 dormant bodies to 18 dormant plus one active, then sleeping. Player passes to z11.99 and backs to z13.47, reset count 0. Actual initial eye z15.55 after clearance; this is not a matched timing comparison with stage 1's z15.4 setup. |
| `tower-npc-r1`, `home-npc-r1` | Technician remains `pose_stand` at (3,0,4) during player lateral walking/stopping. Upper technician and witness retain their independent authored cues. Home caretaker moves from x5.5 to x4.55 via its navigation goal, using walk then standing, while player eye stays (5.5,1.6,6). Captures visually inspected. |
| `well-delivery-r2` | Two ordinary E pickup/drop losses into the well return the same box to (10,0,10). Ordinary delivery settles it at approximately (3.957,0.002,7.977), emits one acceptance and unlocks the airlock. Post-completion sign leaves it sleeping there. Unrelated spray bottle stays at its authored pose. |
| `recovery-retry-r1` | Occupying the return point with another prop causes automatic and manual reset refusal with identical before/after body state. Moving that blocker away and using the sign returns the box. Delivery succeeds, the ordinary door/airlock path enters Tower with empty hands, and a diagnostic reload returns to fresh Intake with exactly one required supply. |
| `release-r1`, `home-carry-r1` | A diagnostic overlapping prop in paused Pool produces a real blocked drop and retained ownership; capture shows short feedback. Clearing it permits retry, pickup and throw, then normal settling on resume. Home ordinary cushion pickup/drop/pickup/throw also succeeds. |
| `long-error-r2` | Tiny unsafe-path ZIP import is rejected by existing validation. Its long error remains within two fitted on-screen status rows; native capture visually inspected. |

The three-map Relay export is retained as
`local/stage2-0.13.0/Relay-Annex-Stage2.placesmap`, 24,070,169 bytes, SHA-256
`8c3ae62322a515d5208163f4c793341544ffa7180e3425bb770e5106bf5acbd1`.
The production compiler validated/exported the changed gameplay sources. Its
prepared-cache checks missed for all three members, so that project was prepared
once; no environment-collection rebake or geometry/art edit was performed.
Original packages remain intact. The compiler executable is preserved beside
the player, with its hash and command in the local checkpoint/export log.

### Checks and review

All required checks exit 0:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace`: **198 passed**, no full-map bake; suite execution about 0.4 seconds.
- `cargo build --locked --release --workspace --all-features`
- `python3 tools/assets/validate.py`
- `python3 tools/textures/build.py --check`
- `python3 tools/props/build.py --check`
- `git diff --check`

Six focused tests add concrete protection for airborne ramp entry, stair-edge
landing, first dormant contact/fixed policy, resting release/real-overlap
refusal, actor animation ownership, and the actual Intake script's recovery and
single-reward policy. Fixtures are tiny metadata worlds or one synthetic rig,
with no renderer, texture generation, source-map baking or broad suite
restoration. Test-only interval expansion did not change the preserved production
binaries. Final logs are `*-final.log`; red/green reproduction logs remain.
The catalog validator reports zero warnings. Texture checking retains 96 existing
preferred-resolution/manifest advisories, and prop checking retains existing art
budget annotations; no visual asset was changed to silence them.

Review checked the shared ownership boundaries, tolerances, fixed-body exclusion,
pause and penetration guards, atomic reset refusal, quest identity/reward guards,
player representation, ordinary HUD and scoped source diff. No blanket lint
suppression, format/cap increase or unrelated cleanup was introduced.

### Remaining limits and next-stage custody

- The original unspecified stair fall-through remains unconfirmed beyond the
  paths above. Keep its report and stage 1 evidence; successful attempts are not
  universal proof. Broader stair/ramp authoring/export validation belongs to the
  editor stage unless a new concrete runtime reproduction identifies a defect.
- Severe Home cushion/books-on-bed and Pool closet slowdowns remain for the full
  performance stage. These correctness runs do not establish a performance fix.
- The Relay ceiling artifact is still visible in retained evidence; no general
  ceiling, editor or geometry rewrite was attempted.
- Essential-item loss handling is explicit authored policy. The well auto-returns
  this box; the sign supplies manual recovery for other inaccessible placements.
  This is not an automatic reachability solver for arbitrary pits/water/puzzles.
  Persistent campaign saves are unsupported by the existing game; section
  transition/reload intentionally starts fresh local state and was checked.
- Setup failures are retained honestly: `well-recovery-r1` released before the
  distant swept hand arrived, and missed delivery; `long-error-r1` selected a map
  instead of Import; `office-ramp-r1` started beneath the east shaft's actual
  floor. They are not passing evidence for those attempted checks.
- Water implementation, storage/compiler optimization, full performance, audio,
  UI/menu redesign and visual styles remain later stages. Cloud model rebuilding
  stays paused; Spoonerman and Windows/Linux environment work remain excluded.

Stage 2 stops here. Final integrated CI waiting remains with the final queue
stage; this handoff does not merge main or publish a release.

## Stage 3 — water motion, currents and player interaction

Completed on the Mac staging branch from stage 2 commit
`ecdd0463b858739bae0818a12efd200fb3d5e32f`. No cloud model export was
integrated, no other environment was rebuilt, and land movement parity remains
future work. Raw receipts, direct binaries, captures, action scripts and the
runnable collection remain in `local/stage3-0.13.0/`, outside `target`.

### Authored controls and rendering

Water now accepts the optional, strictly validated `motion` object:

| Control | Default | Range and behavior |
| --- | --- | --- |
| `speed` | 1 | 0..8; ripple clock multiplier; 0 holds the ripple pattern |
| `strength` | 0.35 | 0..1; ripple UV/normal amount; 0 restores the flat artwork |
| `flow_speed` | 0 | 0..4 metres/second; 0 gives still fluid |
| `direction_degrees` | 0 | 0..360 Places yaw; 0 is −Z, 90 +X, 180 +Z, 270 −X |

All four controls are exposed on the editor's existing paginated water form.
For completely static water, set `motion.strength: 0` and `flow_speed: 0`;
set independent `caustics.speed: 0` too if the receiver pattern should freeze.
A current can exist without surface ripples, and ripples without a current.
Non-finite/out-of-range controls and misspelled nested fields fail validation.

The existing committed PNG supplies the surface imagery. The renderer advects
it in world metres using the material's actual UV frame, adds restrained analytic
wave slopes to the material normal, and applies a small continuous ripple UV
offset. It keeps the authored mean plane, collision boundary, lightmap UVs and
mesh fixed. Equal adjacent controls share world phase. There is no scrolling
time wrap, per-frame mesh construction, new render pass or generated texture.
Different authored controls intentionally produce different behavior at their
boundary; authors should match controls across a continuous sheet.
Texture quality and filtering remain independent from lighting quality.

Caustics retain the existing source eligibility, RGB/intensity/tint, visibility
and receiver rules. Their PNG pattern follows the same current; their own speed
and strength remain independent. Surface motion has an independent water-node
count/clock, so darkness, disabled caustics or a missing focus texture cannot
freeze water. Moon/fluorescent eligibility was not broadened. A native GPU smoke
caught an updated storage-buffer minimum mismatch; all world/decal layouts now
share the correct header-plus-node size (128 bytes), including empty water.

### Player and object behavior

The inspected behavior reference is Valve's **Source SDK 2013 singleplayer**
commit [`77567eb4bce8d7b6b7f73e929fcde3a331c0216e`](https://github.com/ValveSoftware/source-sdk-2013/blob/77567eb4bce8d7b6b7f73e929fcde3a331c0216e/src/game/shared/gamemovement.cpp),
including WaterMove, feet/waist/eye classification, CheckWaterJump, WaterJump,
swim rise and relevant HL2 overrides. Reference files are private local evidence;
no SDK implementation was copied into Places. This is independently implemented
HL2-style behavior, without a claim of measured 1:1 parity.

`src/game/water.rs` owns water locomotion. Feet-only immersion retains walking;
waist depth selects swimming; eyes determine the underwater state. The upright
standing/crouched collision shape and eased eye remain Places' existing contract.
Forward/back swim follows view pitch, strafe stays horizontal, looking up boosts
forward rise, Jump rises, and idle input sinks. Incoming plunge momentum decays
through 3D water friction instead of an arbitrary entry clamp. Floor/head contacts
continue to constrain the body, and the former hard eye float line/bob is removed.
Waist crossings hand back to ordinary gravity, with a modest natural surface
cycle when rise is held.

Reference coefficients are friction 4/s, acceleration 10/s and wish-speed cap
`0.8 × walk_speed`. The 1.8 m/72-unit standing-hull ratio gives a 1.5 m/s idle
sink wish (before 0.8 scaling), 2.5 m/s held-rise seed and 1.25 m/s water-jump
forward speed. Places retains its 9.8 m/s² gravity and ≤1/120 s locomotion
intervals. Acceleration projects velocity onto the requested direction rather
than charging a blocked perpendicular current against the whole speed budget.
This explicit adaptation keeps a swimmer controllable along a narrow bank.

A water jump requires forward intent at waist depth, a touched real walkable
rim no more than 0.5 m above the waterline, and a clear destination. Side/back
pressure, fast plunges, full eye immersion and wall/prop tops do not qualify.
The collision-checked ballistic rise is sized for waist depth plus rim clearance:
1.5 m standing or 1.05 m crouched. It ends on landing, head contact, descent or a
two-second bound. Jump remains latched across landing. Ramps use shared floor
support; stage 2's slope/feet-disc corrections remain intact. A native sideways
current originally triggered an unintended bank jump; forward-intent contact
selection plus an approach cone fixes that case. Both 0.8 and maximum 4 m/s
side-current cases have an essential CPU regression.

Player drag and object drag share the same authored fluid vector. Five footprint
samples ease player crossings; wading applies immersion weighting. Rigid-body
flow is weighted by actual displaced submerged volume, retaining density,
Archimedes buoyancy, centre-of-buoyancy torque and stable exponential drag.
Sleeping active bodies can wake in current; held, dormant, suspended and fixed
ownership stays intact. Dense objects still sink and bottom friction can arrest
their drift. `linear_drag: 0` intentionally removes an object's current coupling.

### Source/package compatibility and deliverables

Level format remains 1 and collision/geometry record layouts are unchanged.
Validated embedded source hydrates motion alongside compiled water records.
New water exports require capability `water-motion-v1`; older players reject
that named requirement. Existing packages load with calm ripples and zero flow.
Changing only motion changes source identity while reusing the prepared-world
key; changing the mean water plane still invalidates preparation. A tiny key
regression and an actual reversed-current export verify both sides.

Pool's five existing water volumes now author restrained controls: matching
main/shelf/secondary/bank speed 0.85, strength 0.35, zero flow; hot tub 0.65/0.15.
Only Pool was prepared, in 244.419 seconds with six workers, static/full variants,
inside a 480-second bound. The standalone export preserves Pool's hub transition
and therefore needs its complete project to play. The runnable
`The-Quiet-Collection-Stage3.placesmap` combines that verified Pool member with
all unchanged original members. All eight other member mappings and eleven
other source records were verified unchanged; the repository's original bundle
was preserved as `The-Quiet-Collection-Stage2.placesmap` (SHA-256
`9e78fbe3f6a2cf00f19f410da76271f1acb6c6b14d91b2e5ba0dd8682e532ee8`).
The new collection is installed atomically at the ignored runnable
`assets/levels/environments.placesmap`; `collection-install-receipt.json` records
both identities. Production dependency validation reports no warnings and Pool
verification reports current=true, differences=[]. Composition took 12.532 s,
streams bounded chunks, and does not rebake the collection.

`tests/fixtures/levels/water_flow.json` is the manual authoring example: three
identical 2.25 m deep bays, left still, centre +X 0.8 m/s, right −X 1.6 m/s, with
matching floating ducks, dense baskets and real walk-in banks. Surface controls
also differ visibly (centre 0.8/0.4, right 1.6/0.8). It exports separately in
4.063 seconds. `Water-Flow-Reversed-Stage3.placesmap` changes only the centre's
motion to −X 1.6 m/s; its 0.2-second export reuses verified geometry, lightmaps,
probes and collision. The editor/source controls provide still water directly.

To inspect the installed Pool, run the preserved player with `PLACES_LEVEL=pool`.
For the isolated authoring example, from the repository root:

```sh
PLACES_ASSET_ROOT="$PWD/local/stage3-0.13.0/resource-root" \
PLACES_STATE_ROOT="$PWD/local/stage3-0.13.0/flow-example" \
PLACES_LEVEL=water_flow local/stage3-0.13.0/source-binaries/places
```

### Checks and native evidence

Required checks pass: `cargo fmt --all --check`, strict workspace/all-targets/
all-features Clippy, `cargo test --workspace` (**206 passed**, 0.41-second suite
execution, no map bake), and locked release workspace/all-features build.
Eight new focused tests protect source/collision hydration, invalid controls,
relative-current density response, depth/view/drag/timestep behavior, impact/rise,
real ledge/ramp/ceiling/held-Jump behavior, strong sideways-current banks,
maximum authored rim reach and compiler key separation. Existing caustics
eligibility tests continue passing. Asset validation passes (620 assets,
0 warnings); texture/prop check results retain their existing art advisories.

The accepted campaign uses direct release candidate D (`f5d59e6f…`, preserved
as `source-binaries/places-native-candidate-d`), real gameplay delta, isolated
settings/state and one native process at a time. Final comment/source-location
corrections produced player `fb38b2b9…` with unchanged implementation; its exact
binary repeats the Pool loop/current-bank smoke and matched uncapped timing in
`pool-loop-release`, `flow-bank-release` and `perf-reviewed-after`.
Each case has a 40-second budget and exits 0 with correct package/member
identity, unchanged inputs, actual **1280×720** drawable (640×360 logical, 2×),
V-Sync off, Immediate presentation and zero missed ready surfaces. Settings:
High textures, High filtering, High lighting, Full lightmaps/reflections, bloom
on, 60° FOV; `flow-low-light-accepted` instead uses Low lighting/Static lightmaps
while retaining High texture/filter settings. Action/capture runs are correctness
evidence, not FPS comparisons. Explicit initial placements/item activation are
diagnostics; walking, jump, crouch, pickup and drop use production input paths.

| Directory | Observed result |
| --- | --- |
| `pool-loop-accepted` | Walk entry reaches feet −1.5, waist depth 2; crouch gives eye −0.7/depth 3. Rise reaches eye 0.511; west rim exit lands feet 0, grounded. Repeated jump entry settles to feet −1.5/depth 2; reset count remains 0. The capture named jump-entry is still airborne; its later settled capture proves entry. |
| `pool-bank-accepted` | First ascent reaches deck feet 0; descent returns feet −1.2/depth 2; second ascent reaches feet −0.017 near the top of the real ramp, grounded. No reset or water jump. |
| `pool-carry-accepted` | Normal E picks up the buoyant duck, carries it through waist water and up the bank to feet 0, then drops it on the deck with ownership cleared. |
| `flow-dive-accepted` | Looking down/forward while standing reaches eye −0.8, depth 3; held rise reaches eye 0.816/depth 1; releasing rise returns to depth 2. No collision recovery/reset. |
| `flow-bank-final` | At 0.8 m/s sideways current, feet follow the actual bank: −2.094 → −1.447 → −0.637 → 0. Grounded throughout; no water-jump activation, launch or reset. This fixes `flow-bank-r1`'s reproduced false ledge jump. |
| `flow-still-accepted` | Player X remains 5 with zero horizontal velocity. Duck remains X4 near waterline; basket sinks to Y−2.398. |
| `flow-east-accepted` | Player approaches +0.8 m/s (0.795 mid-run, 0.801 end); duck X12→13.894. Basket sinks and moves only X14→14.110 before bottom friction arrests it. Player is separated from props at Z8 versus Z6. |
| `flow-west-accepted` | Player approaches −1.6 m/s, then the real bank blocks it at X18.300; duck X20→18.052. Dense basket sinks. No boundary penetration/reset. |
| `flow-reversed-accepted` | Same centre-bay input/source except authored −X1.6: player reaches west boundary X10.300; duck X12→10.052. No geometry/lightmap rebake. |
| `flow-light-accepted`, `flow-low-light-accepted` | Turning sun focus off changes header count 11→0 and receiver energy to zero; restoring RGB [1,.15,.05] restores count 11 and receiver energy in that ratio. Motion clock continues while off. Matching static-camera 280×200 pixel regions are identical in the still bay but change 28,444/18,951 pixels in the moving bays. |
| `pool-main-view-after` | Matched render eye (8,1.6,7.9), yaw 0°, pitch −25°, compared with `pool-main-view-before`. Final surface and basin are visible without a broken rim or abrupt pattern seam. Both captures visually inspected. |

The delivered flow screenshot is `flow-overview/flow-bays.jpg` (original
`view.png` preserved), Library ID `libfile_64e1a1913b3c8191a9fb8403eea1dadc`.
The actual three-bay view was visually inspected and delivered to the main chat.
`flow-video/flow-bays.mp4` retains 24 native frames at 100 ms capture intervals
(2.4 s; 10 fps video cadence is not game throughput). Earlier Pool surface
screenshot `pool-surface-debug/surface-b.png` came from the corrected debug
candidate; the final release comparison is the separate view named above.

Failed exploratory attempts are preserved: the first native shader smoke exposed
the fixed 112/128-byte binding mismatch; a symlinked asset root was correctly
rejected by package containment; the first Pool preparation hit its timeout and
a sandboxed GPU preparation found no Metal adapter; native compilation then
completed successfully. The standalone Pool native launch cannot resolve its
hub transition without the full collection. Initial flow player/prop runs shared
Z6 and allowed player contact; the accepted comparisons isolate the player at Z8.
None of those attempts is presented as passing acceptance.

| Preserved artifact | SHA-256 |
| --- | --- |
| `source-binaries/places` (final reviewed) | `fb38b2b9731345593052d05d721b4a3248aa271ccc8871c73d589d600ec0204d` |
| `source-binaries/places-native-candidate-d` (accepted campaign) | `f5d59e6fcd276e3a44a2f114c87debafaedebed321f11a671a114bfc022338f8` |
| `source-binaries/places-compile` | `e663a7f0a7b361dbd5948ae418e037cd2852d120d602bbff5d80d8e8c8d8e2e5` |
| `The-Quiet-Collection-Stage3.placesmap` | `d6ee71343abbdd3a65c85eacc5ab54e8dd6944a5910eebd92ea12037712253ff` |
| `Pool-Stage3.placesmap` | `86236017987e9617b0dc02cf854c8a5f74b46ab94b485c1d5d36b663cbb293dc` |
| `Water-Flow-Stage3.placesmap` | `91a4b4a76f6b33d3f1e3501868102e9c1f03575aff6dad81b393afca5fd7121f` |
| `Water-Flow-Reversed-Stage3.placesmap` | `4dfef9e2c0df0bb889fc35d6940c8852455e73df063eeb012493eb2d22a35558` |

### Measured performance

Two interleaved pairs compare the preserved stage 2 player against water
release candidate D on the **same original collection package**, same fixed render eye
(8,1.6,7.9), yaw 0°, pitch −25°. Each measures 1,200 frames after 180 warmup
frames. No compiler, bake or other game overlaps these runs. Apple M2 Pro/Metal,
actual 1280×720 drawable, settings above. V-Sync is explicitly off; final present
mode is Immediate after initial surface setup. The loop has no active frame cap;
all measured surfaces present successfully. The display reports 120 Hz, while
measured throughput is 169–183 fps across the matched controls, median loop
times 3.1–4.2 ms, and mean swap only 0.067–0.077 ms. This verifies actual uncapped presentation rather than a
refresh plateau. GPU timestamps include scheduling stalls, not active utilization.

| Receipt | Mean loop ms | p95 loop ms | FPS | Mean GPU ms |
| --- | ---: | ---: | ---: | ---: |
| `perf-before-a` | 5.738 | 14.304 | 174.27 | 2.868 |
| `perf-after-a` | 5.775 | 14.854 | 173.16 | 2.915 |
| `perf-before-b` | 5.920 | 15.500 | 168.93 | 2.917 |
| `perf-after-b` | 5.893 | 14.614 | 169.69 | 2.965 |
| `perf-authored-after` (new collection) | 5.891 | 14.887 | 169.74 | 3.027 |
| `perf-reviewed-after` (exact final binary) | 5.883 | 15.417 | 169.99 | 3.146 |
| `perf-before-c` (fresh stage 2 control) | 5.587 | 14.959 | 178.98 | 2.781 |
| `perf-after-c` (fresh final pair) | 5.459 | 15.055 | 183.18 | 2.770 |

Equal-frame means of the first two pairs are 5.8289→5.8340 ms (**+0.09%**),
within observed run variation; no throughput improvement is claimed. GPU mean is
2.8928→2.9401 ms (+0.0472 ms / 1.63%) in those pairs. Checking the receipts
confirms unchanged base scene counts (32 draws, 20,216 triangles), no extra
render pass/mesh rebuild and no current in this control. Pairwise loop changes
are +0.64% and −0.45%.

The exact final binary's later smoke had GPU mean 3.146 ms, so a fresh bounded
baseline/final pair investigated that increase before handoff. It measured
5.587→5.459 ms loop (−2.30%) and 2.781→2.770 ms GPU (−0.36%): the higher GPU
sample did **not** reproduce as a persistent regression. This variation also
prevents attributing a fixed 1.63% GPU cost uniquely to surface motion. The
new authored collection control is 5.891 ms loop / 3.027 ms GPU, within the
observed ranges. These short runs establish no meaningful throughput regression
or optimization gain. High p95 tails exist in both versions and remain a stage 5
profiling target. This pass does not claim full-game performance or equivalence
at other resolutions/views.

Performance CSVs, per-run settings/environment, actual native identities and
`performance-comparison.json` preserve the evidence. No unbounded profiler trace
or archive was created.

### Limitations and next stages

Water is a planar low-poly presentation approximation; it does not deform the
silhouette or simulate fluid pressure/wakes. Adjacent sheets need matching motion
controls for a seamless continuous phase. The controller targets HL2 swimming
feel, with the explicit geometry/unit/current adaptations above; direct HL2
comparative input trajectories were not recorded. Land acceleration, air control,
bunnyhopping and crouch-jump parity remain out of scope.

The collection was repacked with bounded streaming compression; it is
467,503,143 bytes versus the original 449,857,977 bytes. Other map records are
unchanged, and this is not a storage improvement. Stage 4 should handle compiler/
storage work, including the broad executable-identity invalidation and repacking
cost. Full performance, editor polish, audio, UI/menu and visual styles remain
later queue stages. Cloud model integration is queued for a safe slot after this
handoff; no model assets were changed here. Final consolidated CI waiting/fixes
remain at the queue gate. No main merge, release or force push occurred.

## Cloud model integration — released native queue slot

This integration continues from water-stage commit
`5ebf0fab086cc527e9dfed290acd9301cbe1996a` on `0.13.0-staging`; local HEAD and the
live remote matched before work. The Mac queue had no competing game, compiler
or Cargo writer. All intake, checkpoints, old bytes, prepared packages, direct
binaries and native receipts are retained outside `target` in
`local/model-integration-0.13.0/`. The two pre-existing untracked Python audit
reports are untouched. Historical report consolidation and compiler/storage
optimization remain separate queue tasks.

### Exact runtime intake and asset contracts

Supported owned-Library materialization delivered the final runtime archives and
manifests to a separate intake directory. Archive CRC, traversal, duplicate-name,
link, encryption, bounded-size and path checks preceded adoption. Expected archive
byte counts/SHA-256 and every canonical file's manifest hash match. No private
URL was inferred, source archive regenerated, or authoring imagery substituted.
The unified `PLACES-144-VERIFIED-DELIVERY-INDEX.json` reconciles 243 unique runtime
files: **144 replacement GLBs and 99 PNGs**. The separate new Home door adds one
GLB and one PNG: **245 exact runtime files** in total. Full paths, byte counts and
SHA-256 values are retained in `adoption-candidate.json` in the local evidence
root; the frozen Library index is `libfile_4132eac1ec9c81918e2a8fe70d7ef599`.

| Cohort | Replacement GLBs | Runtime PNGs | Canonical repository locations |
| --- | ---: | ---: | --- |
| Outdoor houses/porches | 58 | 58 | `assets/environments/outdoor/props/` |
| Outdoor landscape | 28 | 28 | `assets/environments/outdoor/props/` |
| Frutiger Aero, 22 families | 24 | 6 | `assets/environments/frutiger_aero/props/` |
| Beach nature | 11 | 1 | `assets/environments/beach/props/`, `beach_nature_fitted.png` |
| Beach architecture/props | 18 | 1 | `assets/environments/beach/props/`, `beach_fitted.png` |
| Entity rigs | 5 | 5 | `assets/entities/{cockroach,sheet-ghost-cat,beach_seagull,beach_crab,beach_fish}/` |
| New Home folding door | 1 new | 1 | `assets/environments/home/props/closet_folding_door.{glb,png}` |

The catalog and accepted-export ledger pin actual geometry hashes, bounds,
triangle counts, texture edges and semantic slots. All 230 earlier accepted
ledger records remain equal. Six Landscape review-only modules, the ordinary
sheet ghost and Spoonerman remain untouched/excluded from this replacement
batch. Existing collision/placement/rig contracts and source masters remain
intact. The ghost-cat hem and Landscape MASK grass are intentional open surfaces;
reviewed compound contact seams were assessed using their specific notes rather
than a single watertightness heuristic. Office's 44 painted porch-post overrides
now cover the delivered trim slot as well as the original stock slot, retaining
the authored paint material. No global art, importer, draw or texture limit was
raised. Editable archives and review imagery remain outside runtime distribution.

The installed GLB set grows from 381 to 382 distinct paths. Decoded pixel
inventory is **87,797,760 → 91,926,528 bytes**, a **4,128,768-byte / 3.9375 MiB**
increase. Replacements account for 3,866,624 bytes / 3.6875 MiB; the new door adds
262,144 bytes. This is not zero overall texture growth. The production
`PropAssets` cache independently confirms the final total: 418 image references
share **187 actual decoded image allocations**, exactly 91,926,528 bytes. The
PNG-identity conservative checker reports 93,499,392 bytes / 89.168 MiB, below
the unchanged 106 MiB budget. These are installed decoded model-image totals,
not a claim that every image is simultaneously uploaded by a level, nor a GPU
allocation/utilization counter. Raw-image and native per-level receipts retain
those distinct scopes.

### Functional Home closet door

The actual Home showcase placement is `home_closet_folding_door`, position
**(0,0,8.6), yaw 90°**, at the bedroom's west wall, with a real backed alcove,
floor and ceiling. The continuous 300 mm west wall retains its existing passage and has a
1.96×2.18 m zero-sill rough opening for the full casing around the nominal
1.80×2.10 m aperture. This prevents frame stock/lighting anchors being embedded
in the wall and avoids the vertical seam from an earlier segmented wall.
The supplied final GLB remains SHA-256
`8e713092feff04f385606ad2b9e69bfdf07638ad5d3e44981cbb72894227e9d9`:
792 triangles, ten primitives, two opaque materials, one opaque 256² PNG and the
original LINEAR `fold_open` clip lasting 1.4 seconds.

Reusable `animated_door` support resolves named rigid nodes and samples that
same clip for both visual pose and live oriented collision. Four leaf cores,
two pulls and three fixed frame boxes leave the real aperture open; there is no
full-width solid prop box or single native door slab. Prospective motion probes
at most 5 ms of clip time, limits hitch consumption to 100 ms, stops at the last
safe pose and retries when clear. A new interaction reverses from that fraction.
Player, AI, fixed geometry, held/loose props and other leaf sets constrain motion.
The same live set feeds movement, interaction occlusion, item physics and
navigation clearance; posed bounds update culling. `animated-door-v1` makes the
new package contract explicit. Native slab doors and stage 2/3 safeguards retain
their interfaces. This supports upright rigid mechanisms, not general skinned
or triangle-mesh moving collision.

### Local gates and native acceptance

Strict workspace/all-target/all-feature Clippy passes (`clippy-reviewed2.log`).
The locked all-feature workspace release build passes (`build-reviewed-release.log`,
3m04s). All **210 Rust tests** pass in 0.40 s (`tests-reviewed.log`): the 206 prior
regressions plus three tiny delivered-rig pose/obstruction tests and one paused
held-body obstacle-pose contract. No full-map bake is part of ordinary Rust tests.
Asset validation reports 621 assets and zero warnings; prop budget validation
passes, while texture validation retains 96 pre-existing advisories.

Focused native fixture checks have exercised actual four-leaf opening,
mid-travel reversal, repeated traversal, 37° yawed placement, occupied close,
clear-close and loose-prop obstruction. A settled mug stops closing near progress
0.703 and clearing it permits the closed endpoint. During a held-mug close, the
existing carry controller moves the mug out of the approaching leaf path;
16 native captures record zero penetration. A suspected stale-query explanation
was disproved by matching live obstacle/body bounds, so no speculative physics
change was made. Ten delivered entity clips advance with original clip names;
`tail_tip` resolves and zero opacity suppresses its light. The full-lighting
checks below complete native acceptance. Diagnostic action/capture runs are not
performance comparisons.

Selective rebuilding already exists: `places-compile build <project> --level <id>`
reads the selected level (`src/map_compiler/project.rs:17`) and emits its own
package. The ordinary current-package check returns without rebuilding when
that member's inputs match (`src/map_compiler/mod.rs:349`); a separate prepared
lighting fingerprint can retain solved lighting for supported non-lighting edits.
Editing one level therefore does not inherently require rebaking every level.
Here, seven members actually use changed models. Pool and Arcade use none.

There is nevertheless over-broad catalog invalidation: `load_catalog_identity`
hashes the entire catalog, including unused entries
(`src/map_compiler/mod.rs:2058`), both package and prepared-stage keys include that
hash, and runtime validation requires its exact installed value
(`src/engine/package/world.rs:326`). This integration's **local, task-specific
script** `compose_collection.py` automates dependency-file SHA/size and used
catalog-entry equivalence checks, then rebinds only catalog provenance and its
manifest hashes for Pool/Arcade. Member selection is explicit. Their prepared
geometry, lighting, probes and original source/compiler provenance remain intact;
this is not a new supported automatic compiler reconciliation facility. Automatic
used-entry invalidation and the broad executable-identity key are recorded for
the **next storage/compiler optimization pass**, with no compiler optimization
implemented here. The revised later queue is engine/game performance, mannequin
ragdolls, then historical documentation cleanup.

Project distribution export also seeds each staged member from the previous
collection (`src/map_compiler/distribution.rs:178`, `project_cache.rs:9`) before
those checks. Thus a normal unchanged member can already be reused during a
collection export; it is the broad global identities above that made automatic
reuse miss for these otherwise unaffected members.

All seven changed members pass `verify --require-current`: Home, Hub, Office,
Winter, Beach, Aero and Lantern Hollow. The final nine-member collection passes
production validation with intact dependencies and zero warnings. Pool/Arcade
retain their original prepared payloads and tool provenance; their developer
currentness receipts explicitly remain false with `compiler executable changed`.
This limitation is preserved rather than representing retained work as a fresh
compile. The local rebinder preserves the engine's pretty manifest serialization,
so raw manifest hashes and actual native identity logs agree. Its first compact
JSON attempt passed content validation but failed the identity receipt gate;
that exploratory Pool receipt is not accepted evidence.

| Focused native evidence | Accepted result |
| --- | --- |
| `home-door-mounted-reviewed` | Actual Home, Full lighting. Closed walk stops at X 0.367; open entry reaches X −0.673, close reaches progress 0, interruption changes opening 0.426→closing 0.318, reopening reaches 1, exit reaches X 0.666. All 12 captures have reset_count=0. Ordinary E activates the real leaves/frame. |
| `door-fixture-r2`, `door-props-yaw-r1`, `door-loose-r1` | Player obstruction holds near 0.684; clear-close/reopen and repeated traversal pass. 37° yawed placement crosses normally; instances remain independent. Settled mug blocks near 0.703 and removal restores close. Earlier missed-pickup labels in props-yaw are not prop acceptance. |
| `door-carried-sweep-reviewed` | Actual E pickup and carry routing; all 16 captured penetration depths are 0. The carry system retracts the mug clear, rather than the closing leaf passing through it. |
| `entity-clips-reviewed`, `entity-close-full-reviewed` | Five replacement rigs, ten original clips advance; native sockets resolve. Full-lighting front/side views retain partial-opacity sorting and emission. Thirteen cycle captures span opacity 0.0158–0.9844 with the moving `tail_tip` attachment. Local node/skin/scene/clip JSON and animation/inverse-bind bytes match the originals across 67 joints. |
| `ghost-zero-reviewed` vs `ghost-absent-reviewed` | Identical room, lights and camera with the sole zero-opacity rig removed in the control: **0 changed pixels out of 921,600**, max channel difference 0. This directly checks absent depth, shadow and emission contribution. |
| `lantern_hollow-models-reviewed` | Four house facades, porch joins, pond landscape, ghost and campfire inspected. Fitted stock, aligned rail/post contacts, MASK grass, opaque masses and warm emission/light response remain coherent. Reviewed bargeboard/platform/core contact seams and inherited roof clearances remain intact. |
| `beach-models-reviewed`, `beach-town-final-reviewed` | Native pier, town/stairs/bunting, lighthouse, sea arch, grounded offset palm, cove/foam edge and three animal types inspected. Static OPAQUE surfaces, fitted materials, water-edge alignment and original collision surfaces/placement remain intact. Initial town camera was obstructed; the accepted town view uses the explicit fixed render eye(8,4,20). |
| `aero-models-reviewed` | Dome and partition glazing, fountain/bubbles, ceiling emission, upright fitted banners, reception and green/city backdrops inspected under Full lighting/reflections. |
| `office-models-reviewed`, `winter-models-reviewed`, `hub-models-reviewed` | Painted rail/post slots, actual upper ramp rails, winter landscape/boulder response, and all three changed Hub preview props inspected. An obstructed Office upper camera was superseded by the ramp capture. |
| `pool-water-final-reviewed`, `office-ramp-final-reviewed` | Pool walk/jump entry, submersion, view-directed dive, resurfacing and rim exit retain reset_count=0; ordinary upper-ramp ascent/descent reaches eye Y 8.8/5.2 with reset_count=0. These focus on preservation of accepted stage 2/3 behavior. |

Native captures were actually viewed for scale/orientation, texture fitting,
opacity/emission, lighting/shadows, joins and animation. Home's initial segmented
wall had a visible vertical join; a continuous wall with a clear-aperture-sized
cut then embedded the outer frame and darkened its spatial lighting. Cutting the
rough opening to the **outer casing dimensions** fixes both defects while
retaining the supplied model and ordinary lighting policy. Only Home was
re-prepared for these observed placement defects. The final Home overview,
close/open/inside/reversal captures retain the correct painted stock and visible
frame; no lighting boost, emissive override or artwork simplification was used.

Initial all-peer catalog rejection, missed prop-pickup setups, the too-short
480-second Office preparation, noncanonical rebound-manifest identity and the
Home placement prototypes remain local exploratory evidence. They are not
counted as passing acceptance. No extra full-map or quality matrix was repeated.

### Focused uncapped performance

Two interleaved baseline/candidate pairs per view measure 1,200 frames after 180
warmup frames, fixed render eye and 1/60 simulation delta. The preserved stage3
player/package/assets are the baseline; the reviewed player and new assets are
the candidate. No compiler or other game overlapped. Apple M2 Pro/Metal,
640×360 logical / **1280×720 actual drawable**, High textures/filtering/lighting,
Full lightmaps/reflections, bloom, 60° FOV. Every run records actual **Immediate**,
V-Sync off, zero missed ready surfaces, and no frame limiter. Throughput 184–623 fps,
median loop times below 8.33 ms, and mean swap 0.014–0.030 ms exclude a 120 Hz refresh
plateau. Capture/interaction runs supply no FPS claims.

| Fixed view, two equal-frame means | Before→after mean loop ms | Before→after FPS | Loop change | Base scene draws / triangles before→after |
| --- | --- | --- | ---: | --- |
| Lantern house front, eye(9,1.6,11), yaw 0 | 5.2349→5.3559 | 191.03→186.71 | +2.31% | 409 /1,367,966→419 /1,379,130 |
| Beach pier, eye(10,1.6,−3), yaw 0 | 1.6114→1.6635 | 620.57→601.16 | +3.23% | 22 /3,069→27 /3,605 |
| Aero atrium, eye(2.5,1.6,10.1), yaw 38 | 1.6564→1.6512 | 603.73→605.62 | −0.31% | 85 /20,378→102 /23,734 |

Pairwise loop changes are Lantern +3.30/+1.32%, Beach +2.47/+4.00%,
Aero +1.20/−1.82%. GPU means are respectively 5.300→5.377 ms,
1.576→1.430 ms and 1.118→1.185 ms; timestamps include scheduling stalls, not
utilization. These short controls show modest measured costs with the additional
art/draws in two views and variation around equal throughput in Aero. They do
not establish a performance gain, confidence interval, thermal-controlled result
or full-game performance guarantee. Raw CSV/settings/native identities and
`performance-comparison.json` preserve every run. The later Home-only rough-cut
refinement leaves all three timed member records **byte-identical**, so no FPS
repeat or unrelated optimization was performed.

### Artifacts, limits and handoff

| Preserved final artifact | SHA-256 |
| --- | --- |
| `source-binaries/places` | `644eb4f92641105c7cdc21c12723157e7c8d714c46a919459f5055ece51d8076` |
| `source-binaries/places-compile` | `d5e37d74e3c35a523126d142efcab1d2e202821906fed55c52df977026a23e9f` |
| `The-Quiet-Collection-Models-Accepted.placesmap` | `1a4ecdf4ae0f87d0403c9477e090288373557b8e8b5d044e6894bc6afc6c9f73` |

The accepted collection is **472,639,184 bytes**, versus 467,503,143 at stage3;
this is not a storage improvement. The final package is installed as
`assets/levels/environments.placesmap`; old packages, model/catalog bytes and
all receipts remain available locally. Home/Hub/Office/Winter/Beach/Aero/Lantern
preparations took approximately 65/48/1,200/474/252/245/1,239 seconds before the
small Home fit iterations. Lantern's roughly eleven minutes of Static preparation on one CPU core
and subsequent transport progress with six workers are retained for the next compiler
pass; no duplicate process was launched during connection fluctuations.

Acceptance is scoped to this Mac/Metal, the measured resolution/settings and
representative placements. The mechanism is upright rigid-node OBB collision,
not general skinned/mesh collision, native slab lock parity or a full-game
platform campaign. Two legacy geometry-check service-passage warnings and 96
texture advisories remain unchanged. No global limits were raised and no art
bytes simplified. The next work is **storage/compiler optimization**, then
engine/game performance, mannequin ragdolls and historical docs cleanup. Those
passes are not started here. All work remains on staging; no main merge,
release, force push, CI wait or `cargo clean` was performed.

## Map storage and compiler optimization

This stage starts from integration commit `655a6d8433a1ac24fbafe4e9fd6fa15d1d7cc3ad`.
It preserves the accepted art, geometry, collision, animation, every supported
lighting mode and circuit state. Evidence is under
`local/map-storage-0.13.0/final/`; the original collection, compiler/player,
rejected storage prototypes and earlier receipts remain outside `target`.

### Implemented behavior

New compiler provenance revision 2 records a validated, transitive catalog
closure for each level and preparation-source identity. Referenced materials,
textures/masks, fixtures, placed/template models, decals and missing lookups
participate. File dependencies still require their size and SHA-256. An unused
catalog entry or unrelated CLI/game/editor source change does not invalidate
lighting; shared preparation/engine source changes conservatively do. Compiler
identity includes target/features, dependency lockfile, fixed preparation assets
and the compiled-in model registry. It deliberately remains conservative within
shared engine code, including comments and renderer changes.

`verify --level ID --json` and `map_compiler::verify_project` expose per-level
`current`, `refresh`, `bake` or `invalid` status, `bake_required`, integrity and
named reasons for the later editor pass. Metadata/navigation refresh retains
prepared lighting. A complete unchanged export verifies source bytes, source
manifest/index agreement and every member's payload hashes, then leaves the
archive untouched. Partial export directly streams verified current members;
only affected members enter the existing seed/build pipeline.

Static directional visibility now uses bounded conservative trees for walls,
horizontal solids and oriented props, retaining the original exact leaf tests.
Unsupported/nonfinite or oversized geometry falls back to the linear oracle.
The derived trees are absent from wire data and bounded below 24 MiB combined.
Immutable visibility/index allocations are shared between Static switch-state
clones; state-dependent light values and baked colors remain separate. Vertex
preparation now observes the existing cancellation token inside long batches.

Static (`off`) still computes room/fixture lighting, hard-shadow visibility and
vertex colors from local light, five sky rays and authored directional lights;
it also prepares authored circuit-off vertex colors. It means no lightmap
atlas, not no lighting (`render/common/api.rs:762`, `lighting/bake.rs:1939`,
`map_compiler/mod.rs:1720`, all under `src/engine/` except the last).
Full adds chart planning, direct/two-bounce transport, switchable-light data,
irradiance probes, HDR atlases and reflection captures. The reported forced
Static command includes navigation, records and compression, not Full transport.
Static is a separate playable lighting variant; the default export builds both
Static and Full, rather than treating this timing as the entire Full solve.

The explicit `optimize-storage INPUT --out NEW.placesmap` operation validates
the existing package/dependencies and performs no lighting or geometry bake.
It chooses established PPS1 revision 2 or exact byte-lane shuffled revision 4
by measured compressed cost, with bounded scratch and unchanged decode limits.
Readers retain revisions 1–4; new required capabilities make older players
refuse unsupported packages explicitly. Archive format remains version 1.
Archival migration uses Deflate-9; normal builds retain Deflate-6. No geometry,
float, texture, lightmap or probe precision is reduced. Source/provenance and
all non-prop payload bytes survive unchanged. Input/output guards, atomic
publication and owned temporary cleanup preserve the last usable package.

### Measured storage and compilation

The current accepted nine-level collection was remeasured, rather than using
the historical 449.9 MB baseline. Actual ZIP payload sets count shared records
once; per-map rows overlap and must not be summed.

| Collection measure | Before | After |
| --- | ---: | ---: |
| Complete archive bytes | 472,639,184 | **446,058,430** |
| Unique uncompressed ZIP record bytes | 1,499,883,500 | 1,467,307,398 |
| Physical ZIP records | 207 | 205 |
| Unique reconstructed PMP1 prop bytes (21 records) | 1,009,761,466 | 1,009,761,466 |
| Static-only payload bytes compressed | 52,854,244 | 55,249,011 |
| Full-only payload bytes compressed | 269,170,699 | 255,836,420 |
| Static/Full shared payload bytes compressed | 149,073,748 | 133,645,923 |

Savings are **26,580,754 bytes (5.62%)** overall and 32,576,102 uncompressed ZIP
record bytes; reconstructed prop sizes remain unchanged. Static-only cost increases by 2.39 MB as the chosen factoring
changes shared-record boundaries; the collection total includes this cost.
Pure Deflate-9 of the original representation yields 448,866,575 bytes;
adaptive structural storage therefore saves another **2,808,145 bytes** beyond
that compression-only control. This is not a 26.6 MB structural-only gain.

| Map, both modes, unique referenced compressed payload bytes | Before | After |
| --- | ---: | ---: |
| Hub | 5,610,257 | 5,295,731 |
| Home | 12,406,882 | 11,689,561 |
| Pool | 21,068,800 | 19,692,897 |
| Office | 44,476,313 | 41,027,309 |
| Arcade | 93,796,206 | 87,600,005 |
| Lantern Hollow | 185,598,040 | 176,779,884 |
| Winter | 72,771,362 | 69,369,323 |
| Beach | 19,222,979 | 17,926,294 |
| Aero | 16,452,819 | 15,558,619 |

The per-map figures are actual referenced ZIP payloads, excluding archive
headers/index and external assets, rather than hypothetical independent
archives. `storage-before.json` and `final/storage-after.json` also retain each
map's Static/Full/shared and uncompressed breakdown. Standalone real Static
builds are 76,163,556→76,171,479 bytes for Lantern and 549,574→554,033 for Home;
their small provenance growth is not a storage win. Unconditional shuffled
storage grew Lantern Static substantially in a rejected prototype; adaptive
selection retains its established representation.

Lantern Static forced compilation fell from **700.919 s to 23.115/22.853 s**
in two final-binary repeats, with identical reconstructed props and every other
prepared record. Earlier candidate repeats were 22.439/22.289 s and remain
preserved. Prop geometry/color preparation fell from 680.321 s to
7.469/7.398 s; architecture from 9.810 s to 0.688/0.680 s. The baseline was one
historical controlled invocation, with release linking overlapping part of it;
these are observed timings, not a statistical confidence interval. Candidate
repeats run serially, record host load/CPU/RSS, and emit identical archives.
The final warm integrity-checked no-op is 0.545 s. Maximum final repeat compile
RSS is 1,365,999,616 bytes; a complete baseline peak was not captured.

The dominant hotspot was Static vertex lighting in prop preparation (97% of
baseline wall time), across about 5.21 million vertex slots. Prop coloring calls
`global_surface_light` (`src/engine/render/common/props.rs:274,924`), including
the unchanged five sky rays in `src/engine/lighting/bake.rs:1965` and authored
directional lights. Previously each visibility ray scanned all walls,
floor/ceiling solids and props. `Visibility::occludes_direction`
(`src/engine/lighting/visibility.rs:2592`) now rejects entire groups that the ray
cannot intersect through `src/engine/lighting/directional_index.rs:106`, then
runs the same exact leaf tests. This skips irrelevant intersection work without
changing samples or geometry. Forced builds bypass reuse, so the large cold
gain is not a cache hit; Full transport is unchanged. Architecture also saves
about nine seconds, while trying both lossless storage candidates adds work.

Home Static remains approximately 0.546→0.542 s, and the tiny Static/Full
switch fixture 2.180→2.189 s: neither establishes a speed gain. Home switch
clones share 158,200 visibility/index bytes, clone in 0.008 ms, and prepare the
alternate state in 20.888 ms. The tiny two-level no-op takes about 0.11 s in
both tools, but the new tool reports `rebuilt=false` and performs no publication.
Costly Full light transport and the remaining prop factoring/compression time
are retained findings for later work, rather than another full collection bake.

A user-requested bounded second outdoor comparison uses Beach, identical source,
catalog/existing dependency hashes, 12-worker budget and forced Static-only requests:
**57.048→2.472 s (23.1×)**, with exact props and every other prepared payload.
Its architecture phase falls **49.720→0.469 s**, props **5.515→0.096 s**; the
same ray acceleration therefore helps architecture as well as model coloring.
Warm reuse takes 0.223 s. Peak RSS is 77,299,712→82,919,424 bytes, and standalone
archive size 1,210,482→1,214,207 bytes (provenance overhead, not a size win).
Together with Home's unchanged small indoor case, these single-pair controls
demonstrate gains on two expensive outdoor cases, not a universal speed factor
or a Full-bake measurement. `campaign-beach.json` and `beach-inputs-exact.json`
retain the measurements and equivalence checks; no full collection bake ran.
The new compiler also records Beach's unchanged `collection_hub.png` decal,
which the old dependency collector omitted; this is tracking, not an asset edit.

Archival migration takes 100.821 s (100.919 s process wall), with measured peak
RSS 2,079,588,352 bytes and observed temporary disk peak 1,913,365,828 bytes
(250 ms sampling). A final-binary cancellation/restart repeat takes 102.440 s
and emits the identical archive SHA-256. It processes one member's prop streams at a time and
disk-spools unique project records under existing caps. These are measured
campaign costs, not an assertion that whole-process RSS equals the decoder's
1 GiB charged working-set limit. The 65,536-vertex shuffle scratch is bounded
to 2,949,120 bytes and included in that limit; no hostile-input cap was raised.

### Verification and native preservation

All nine member prop reconstructions, every non-prop prepared payload and all
12 recovered source files are byte-identical to the accepted input. Complete
package validation passes with 375 unique installed dependency identities.
Repeated Lantern builds are archive-identical. Focused reuse checks cover:

- Unchanged export and unrelated catalog/asset/code edits leave the archive unchanged;
  runtime validation also accepts unrelated catalog metadata changes.
- One level's metadata refresh retains lighting; a light edit rebakes that
  level while its peer directly reuses verified records.
- A shared material edit invalidates both dependents; changed shared model
  bytes fail `--require-current` for both. Corrupt payloads are refused.
- Failed source admission preserves the previous output. Observed in-flight
  cancellation exits 130, preserves it, removes owned staging, and restarts
  successfully. The storage operation also cancels/restarts with unchanged input
  and identical completed output; incomplete standalone transition packages are
  refused. Source-manifest/index disagreement cannot return a no-op hit.
- An older compiler explicitly refuses the new storage capability. Legacy
  revision 1 provenance remains runtime-valid under its original checks;
  acquiring revision 2 reuse keys requires one real rebuild. Storage migration
  intentionally preserves old provenance and does not claim all nine are current.

Native Mac/Metal checks use the actual selected package/member identities,
**V-Sync off, Immediate presentation, 1280×720 drawable**, High textures/filter,
Full reflections and bloom. Package comparisons hold the stage player constant
to isolate storage; derived index storage is charged separately by its retained
byte accounting. Matched Home/Lantern captures and all six
Static/Full on/off/restored captures have **zero changed channels**. Native
Home folding-door traversal/reversal retains nine collision boxes and zero
resets; Pool entry, underwater dive, resurfacing, rim exit and jump entry pass.
No runtime source preparation occurs.

Two alternating cold-application/warm-OS-cache Hub→Home→Pool→Hub runs give
per-run load-window medians 390.7/379.4 ms before and 394.3/385.3 ms after;
maxima stay approximately 3.23–3.27 s. Short Home initial loads are
819.6→828.3 ms, Lantern 5,147.8→4,992.2 ms. These do not establish a load-time
gain or material regression. Retained prepared CPU bytes are unchanged:
Home 84,680,421 and Lantern 965,299,278. Lantern's observed native peak RSS
increases **1,915,797,504→2,243,395,584 bytes** in the short capture pair;
Home stays approximately 499 MB. The peak increase is a limitation for the
performance pass, not a resident-memory saving. At world commit, RSS also
remained higher: **1,896,677,376→2,227,077,120 bytes**. These short runs do not
establish the increase's lifetime or cause. GPU allocations and RSS are not
added to the decoded-record totals.

Strict workspace/all-target/all-feature Clippy, workspace tests (**212 passed**),
format, diff checks and locked all-feature workspace release build pass. Added
tests are small exact-oracle/catalog/tile-boundary regressions; none bake a
shipped map. No full-map CI workload was added. Initial harness runs selecting
the installed package or redundantly toggling a shared circuit were caught,
excluded and retained; corrected package-identity/state runs pass.

The validated smaller collection is installed at
`assets/levels/environments.placesmap`, SHA-256
`8012dbb71ba73fbc85d81582511e0d99a4e7a2103d5dfb48fbce1a33e0cf0783`.
The original `1a4ecdf4…` collection and prior stage3 assets remain preserved.
Acceptance is representative Mac/Metal coverage, not every map/platform/view
or a fresh nine-level Full bake. Capture/action diagnostics make no FPS gain
claim. Runtime tracing by source shows a conditional frame path through moved
props/characters' center-light refresh (`wgpu/renderer.rs:1317,2458` →
`common/light_transport.rs:710` → `fallback_entity_texel` → `occludes_direction`),
when no valid irradiance sample resolves. Exact placement/lighting keys skip
unchanged receivers; valid Full probe samples avoid this fallback. Directional
caustics receiver setup also uses it (`wgpu/caustics_field.rs:380,517`), then
caches its factors. These are runtime profiling leads, not established FPS
gains; steady static geometry uses prepared colors/atlases. Paths are under
`src/engine/render/`, with fallback in `src/engine/lighting/bake.rs:2007`.
The next stage is engine/game performance, then the separately authorized
additional engine optimization sweep, mannequin ragdolls, docs cleanup, editor,
audio, menu/UI, styles and final validation.
No main merge, release, force push, CI wait or `cargo clean` occurred.

## First engine/game performance pass — 2026-10-21

This pass implements measured CPU fixes and explicit inactive-world teardown for
[#27](https://github.com/csd113/Places/issues/27) and
[#28](https://github.com/csd113/Places/issues/28). It does not close either issue
or replace the separately authorized fresh engine sweep. Baseline is
`f38c638e7a7ba6fa93136636600b82e0ae4f8937`; the initial checkpoint player SHA-256 is
`b0c07d737012ced1c6cbdf4b2a81cb506e664453a8aa21ce8ef5167c8755b216`.
Checkpoint `2c9671b2a850deb7585f082a522c3da8824f4329` binds that binary's source;
the expanded final binary and results appear below. Baseline and
final executables, the 446,058,430-byte collection, lab packages, inputs and raw
clips are preserved outside `target` under `local/performance-0.13.0/`.
The collection's SHA-256 remains `8012dbb7…0cf0783`; no collection rebake ran.

### Implemented causes and preserved behavior

- Moving-caster revisions caused stationary receivers to repeat their static
  probe/transport/room-light ray work. Each receiver now retains up to 2,048
  exact directed endpoint/type/label answers. Model/bounds, field/scene,
  lighting-state or resource-revision changes invalidate them. Every movable
  transmittance query still runs; overflow computes normally. No shadow taps,
  probes, sources, visibility support or quality are removed. This is separate
  from the previous compiler pass's directional index; no isolated runtime FPS
  claim is made for that older change.
- Rigidly weighted skin normals reuse each joint's inverse transpose for its
  current pose. Only exact `[1,0,0,0]` weights take this path. Blended weights
  retain the previous blended-matrix inverse, including singular behavior.
  Posed positions and whole-body visibility/bounds are unchanged.
- Sequential AI work now loans one world-owned navigation scratch allocation to
  each agent, preserving query order, replanning/visibility budgets and results.
  The former three cell arrays could retain about 24 MiB per agent at the
  2,097,152-cell limit, or about 3 GiB at 128 agents, before heap storage.
  This removes that multiplication; it is not a claimed path-search speedup.
- Missing GPU textures prepare/upload in eight-sheet chunks instead of staging
  every miss together. Eight 1024-square RGBA mip chains require about 43 MiB
  of prepared pixels; decoding/image caches and driver allocations are separate.
  Upload order, duplicate plans, pixels, filtering and worker ceiling are kept.
- Explicit Main Menu entry cancels CPU/GPU preparation, drops pending commits
  and world audio, scripts/AI, physics/carry, map geometry/materials/textures,
  atlas/probes, effects and map-owning GPU bindings. Worker-owned builds/images/
  models retire serially. Two cancellation orderings have deterministic small
  regressions, including maintenance replaced and then canceled after a running
  job's last release check. Pause retains its world. Shared device/surface,
  fonts/UI, drawable targets and the fixed flashlight target remain. The old
  whole-map menu backdrop is intentionally gone; the later independent menu
  scene is outside this pass. Maintenance owns no loading status UI.
- Prop-batch readers and writers now agree on 65,536 addressable vertices;
  65,537 is refused before vertex allocation. No cap was raised, and validation
  of the existing full collection passes.

### Benchmark and measurement contract

`projects/performance-lab` is a 16×12 m ordinary editor project: fifteen
bookshelves/forty-five book stacks; twenty-four dynamic cups and a pickup
cushion with bounded activation/reset; twelve animated wandering mannequins;
four room fixtures, fog and normal flashlight support; and explicit Luau
per-frame actor queries plus enter/exit physics and three-second, one-active
visitor spawn handlers. It supplements the real maps. It adds no runtime-only
map construct, new visual asset, raised authoring limit or CI map bake.
The existing editor's Save+Build, close/reopen and complete Project Export
succeeded. A final-editor copy was also nudged 0.25 m, saved, closed, reopened,
restored and exported through visible controls. Its only serialization
normalization is explicit `rooms[0].floor_y=0`; all other authored values are
preserved, and all fifteen non-project exported members are identical.
`lab-editor/`, `lab-export/`, `editor-roundtrip-receipt.json` retain the evidence.
The baseline level package is 7,686,490 bytes; both lighting variants build in
about two minutes. Source/usage and repeatable inputs are in the project's
README and `tools/bench/performance/`.

All accepted FPS runs use the same M2 Pro 12/19-core, 16 GiB unified-memory
Mac14,9, macOS 27.0.1/26A434, Metal, Rust 1.99.1 release profile
(opt-level 3/LTO/one codegen unit). High textures/filtering/lighting, Full
lightmaps/reflections, bloom and 60° FOV are held. Logs verify actual Immediate,
V-Sync off, no frame limiter and zero missed ready surfaces. Throughput is not
a refresh-rate plateau. Drawable sizes below are physical pixels: logical
640×360 yields 1280×720; logical 1280×720 yields 2560×1440.

The final lab ABBA set has 2,400 samples after 120 warmup frames per run,
matched fixed 1/60 simulation work. Real interaction sets last twelve ready
wall-clock seconds with normal delta; matching phases are selected from native
ready/present timestamps, excluding startup. Recovery is 6–11.8 s, held work
1.4–3.2 s, closet near 2.4–4 s, facing away 4.4–5.3 s and retreat 7–11.8 s.
A fixed frame count with wall-clock actions was rejected for interaction claims.
GPU timestamps are asynchronous pass spans including scheduling stalls, not
utilization or a quantity to add to CPU time. Capture runs supply no timings.
There are 26 primary comparison runs and 57 completed timing-related receipts
including exploratory/diagnostic runs; these are not 57 independent replications.

| Case / drawable | FPS before → final | Mean CPU update ms | Loop p99 ms before → final | Interpretation |
| --- | --- | --- | --- | --- |
| Lab / 1280×720, two runs/build | 377.22–381.73 → 412.36–412.63 | 2.00 → 1.80 | 3.51–5.97 → 3.57–4.71 | About 8.7% throughput and 10% CPU savings; tail ranges overlap. |
| Home books recovery / 1280×720, two runs/build | 513.5–539.9 → 627.6–781.7 | 1.23–1.36 → 0.55–0.65 | 2.62–2.68 → 2.15–2.80 | About 54% less CPU update work; pacing varies. |
| Arcade bowling post-drop / 1280×720, two runs/build | 94.9–115.9 → 135.0–149.5 | 7.32–9.27 → 5.20–6.11 | 13.53–13.59 → 9.02–9.48 | About 32% less CPU work; substantial residual cost remains. |
| Home cushion post-drop / 1280×720, one pair | 573.0 → 643.4 | 1.11 → 0.50 | 2.56 → 2.60 | A CPU saving, not proof of resolving the reported persistent collapse. |
| Pool closet near / 1280×720, one final pair | 627.4 → 375.0 | 0.196 → 0.234 | 3.04 → 17.52 | Regressed pacing run; cause unresolved, no gain claimed. Other repeats span 525–776 FPS. |
| Busy Arcade / 2560×1440 | about 244 → 244.62 | 1.16 → 1.05 | 6.47 → 6.14 | Effectively unchanged throughput; GPU mean 4.44 ms and 308 scene draws remain. |
| Arcade bathroom / 2560×1440, one pair | 366.64 → 364.90 | 1.14 → 1.02 | 6.60 → 6.84 | No reliable throughput gain; GPU mean 4.13 → 4.12 ms. |

An initial lab pair of 176→413 FPS was superseded by alternating repeats;
it is not the accepted gain. Pool's final near-phase GPU mean is 0.958→0.992 ms
and p99 1.27→1.74 ms while the loop tail grows much more. Its broader scheduling/
submission regression is a lead, not explained away as a physics fix. Quiet Retina Home is also unchanged (512.6→517.7 FPS, CPU 0.107→0.109 ms). Busy
Arcade GPU p99 is 4.59→4.51 ms; bathroom is 4.97→4.97 ms. Books' sleeping
physics takes about 0.007–0.008 ms/frame. Bowling recovery solver means are
0.043–0.047→about 0.042 ms, with zero to 0.202 mean awake bodies across runs.
Thus neither CPU saving is presented as a solver optimization. Real-time
trajectories/drop positions vary slightly; the workload and input contract
match, rather than identical physics states at every rendered frame.

### Actual repros, scripts and visual correctness

Home cushion idle→aim/pickup→moving carry→drop→sleep/recovery passes, with no
player reset. Books placed on the bed, ordinary jump/landing and retreat pass;
two bodies settle, an item-pair contact is observed, and the player recovers to
the floor. The original severe persistent Home slowdowns remain unconfirmed
and unresolved; stage 1 non-reproduction is not a fix. Pool approach/near/facing/
retreat runs cover the closet at the recorded distances; the original severe
case is not resolved. Arcade's busy/bathroom views and bowling held/contact
case are measured separately above. The bowling residual and Pool pacing remain
fresh-sweep leads. Final-binary folding-door blocking/open/traverse/close/reverse
and Pool entry/dive/resurface/rim-exit/jump-entry checks pass, preserving stage 2
collision/controller/prop and water behavior.

The lab activates all 25 bodies, then returns all to dormant through its normal
Luau reset. Observed peak solver-frame time is 0.165 ms. Visitor count is
12→13→12→13 on spawn, expiry and reentry. Twelve host actor queries per update
cost 0.078 ms mean, 0.105 ms p95 and 0.134 ms maximum across 64 diagnostic
invocations, including native capability setup. A separate early visitor
invocation costs 0.034 ms with one native call; creation is visible by the
0.8-second capture and expired by four seconds. This is an input-to-observation
bound including movement, not an isolated spawn-only speedup. Scripts were not
rewritten to manufacture an FPS gain. A tiny normal project deliberately
requests a 3 MiB Luau string under the existing 2 MiB VM cap: it fails closed,
discards staged actions, remains playable, repeats after reload at the same
467,256 VM bytes and releases to zero at menu. General host/driver OOM recovery
is not demonstrated by that controlled VM failure.

Matched frame-300 Home, Pool, Arcade and lab captures have **zero changed RGB
channels**. Native moving/carry and animated-doorway views remain intact; no
culling, source state, simulation accuracy or visual quality was reduced.
The native editor round trip, all captured frame paths and the accepted input
packages remain preserved, including failed/excluded harness attempts.

### Inactive maps, active work and memory

The heavy cycle is Lantern→Hub→Home→Lantern→Hub→Arcade→Lantern→Hub→menu.
Every retired Lantern prepared-record/build weak probe reaches zero after
leaving it. Hub/Home may retain passive prepared records in the existing bounded
LRU. The final menu has zero active prepared bytes, scripts/VM bytes, AI,
physics bodies, pending commit and map texture-cache entries; every retired
probe is zero. Canceled Lantern cannot resurrect after menu; Home reload and a
second menu release pass. Pause retains its world until explicit menu entry. A direct Lantern pause→resume→menu check additionally preserves all 965,299,278 prepared bytes during pause/resume, then releases its final record/build owners to zero; its menu cleanup callback takes 52.462 ms in one diagnostic event (`heavy-menu-results.json`).

A final 319.3-second campaign completes 42 requests/commits, 28 graphics
transitions, 29 drawable observations and seven menu returns while repeatedly
activating/resetting physics and spawning/expiring visitors. Every menu has
zero map ownership. Live menu GPU counts stay bounded at 16 textures and 23
bind groups, with 29–35 buffers and 27–31 views depending on fixed/drawable and
in-flight state. Prepared-cache pressure through Hub→Home→Beach→Aero→Hub
releases evicted Hub/Home/Beach records; menu releases the remainder.
Lantern active RSS snapshots are 2046.9/1922.2/1862.6 MiB; the heavy-cycle final
menu is 794 MiB. Extended menu RSS varies 603.9–841.6 MiB and later falls again.
RSS alone does not measure live old-map ownership: passive bounded caches and
allocator/driver capacity are distinct. CPU weak ownership and live GPU objects
are checked independently. Metal HAL byte counters are unavailable;
engine texel accounting is partial, with sharing, and is never added to RSS as
if M2 RAM and VRAM were independent pools.

The user's prior-map-CPU hypothesis was tested with exactly the same Hub
position (12,1.6,7), view (0,0,−1), settings and 39 scene draws. Fresh process
2–10 s measures **582.6 FPS / 0.167 ms CPU update**. After one and two heavy
cycles the same windows measure **625.2/635.9 FPS / 0.132/0.149 ms**, loop p99
3.05→2.98/2.91 ms. Old script modules, AI agents and physics work are absent;
the single prepare worker is idle after the completed result, with no pending
commit. RSS at Hub commits is 354.5→1130.5→1617.9 MiB. Passive cache/capacity
retention therefore does not establish active ticking, and no transition
slowdown is supported in this tested case. This does not rule out memory-pressure
or other-view effects generally. Menu retention was a real ownership defect;
it has not been established as the cause of gameplay FPS drops.

A separate no-capture heavy-transition run records a kernel native peak RSS of **2,679,783,424 bytes (2.50 GiB)**. Fresh Home/lab/Arcade run peaks are about 488/466–483/1,440 MB. One serial run's request-to-commit latencies are Lantern 4.80/4.72 s, Hub 0.405/0.298 s, Home 0.677 s and Arcade 8.21 s; these are observed operation costs, not a matched speedup claim. `transition-peak-results.json` retains exact identities, peaks and timings.

The earlier approximately 312 MiB loading-peak increase remains a documented
lead, not a proven transient or a solved resident-memory regression. The
historical world-commit RSS increase is retained above. Lower active-map RAM
was not pursued at the cost of speed or quality.

### Resource-limit audit and limits of acceptance

`local/performance-0.13.0/resource-audit.json` retains the full dated inventory:
units, source/enforcement, producer/reader agreement, rationale, failure behavior
and unbounded paths. MiB means 1,048,576 bytes. Counts/bytes/overflow, archives,
temporary staging and in-flight ownership were inspected; caps are admission
ceilings, not a promise that all maxima fit simultaneously.

| Domain / source owner | Limits and failure contract audited |
| --- | --- |
| Package/archive readers and writers | Ordinary/project transport 1/2 GiB plus framing; 8,192 entries/8 MiB directory; validated project allowance; decoded aggregate 1/2 GiB; default member 256 MiB, mesh/props 512 MiB; smaller manifest/material/chart/lighting/collision/nav limits. Bounded decompression, checked counts/offsets, exact identity/hash and source recovery remain. |
| Lossless prop storage | 16 MiB segments, 64 MiB recipe, 512 MiB reconstructed record, 1 GiB charged working set, bounded shuffle scratch; 65,536 batch vertices now enforced by writer and reader. Segment staging plus reconstructed output remains a peak lead. |
| Authoring and models | 32 MiB level JSON, 24M estimated vertices, numeric extent/area/height and every per-feature count; GLB 64 MiB, depth/visits, 65,535 vertices, primitive/material/image/joint/clip/channel/morph limits and reviewed triangle exceptions; 4,096 static model identities/24M transformed vertices. Refusal/fallback preserves prior valid state. |
| CPU image/material/prepared ownership | Decoded material table 1 GiB; PNG 32 MiB encoded and class-specific dimensions; prefetch 64 images/four workers; optional image cache 256 entries/256 MiB; prepared LRU three entries/192 MiB, oversized worlds uncached. One running/newest-pending preparation and bounded channels; menu serially clears caches. Prefetch and old/new install overlap are distinct from retained accounting. |
| GPU images, atlases, uploads and frames | Optional texture retention 256 entries/256 MiB at level boundaries; eight-sheet staging and at most eight workers; one pending install, nominal 4 ms slice, two in-flight frames, device/drawable limits. KTX dimensions/mips/layers/512 MiB; atlas 384 MiB plus framing, page/switch caps; reflection encoded/runtime probe limits. One large upload can exceed its slice. |
| Lighting/visibility/fog/effects | Field 262,144 probes/256 MiB/axis 64; direct/switch/blacklight/dynamic slots; 4,194,304 transport triangles, BVH/bounce/worker and spatial-grid bounds; new 2,048 fixed-size exact answers per receiver, lazily allocated and world-owned; fog/void/effect/particle/snow/shelter limits. Overflow never changes a cached ray's answer. |
| Dynamic rendering and physics | 256 render objects, aggregate vertex/index admission, 128 characters; 128 physics bodies and 1,024 explicit fixed-prop placements (not all world collision shapes); 12×1/120 steps and 0.1 s delta bound; compound/hull/extent/position/recovery/impact caps. The 256 item-contact diagnostics and 1,024 onset history do not truncate Rapier solver contacts. |
| AI/navigation | 2,097,152 cells/columns, class/portal/ID and 128 MiB nav limits, A* expansion cap, eight queries/tick, stimulus bound; one shared world scratch now. Failure/defer and query ordering remain unchanged. |
| Entities/spawns/sequences/Luau | Binding/action/queue/drain/depth/timer/volume/item/path limits; templates/points/groups, 512 live spawns and 64 requests/tick subject to lower downstream caps; sequence count/steps/active/duration caps. Luau 16×64 KiB sources, 2 MiB VM/module, 64 calls/128 actions per frame, 256 native calls/32 actions per invocation, time/instruction/diagnostic caps. Staged failure/reset contracts remain. |
| Audio/project/editor/diagnostics | Audio file/decode/voice/callback bounds; project manifest/256 levels/scripts/recovered source/thumbnail limits; editor history/serialized bytes, one oversized revision, script buffers and bounded job feedback; console/input/history/output, acceptance scripts, GPU timing slots, log and developer-object limits. Persistent installed packages/saves/editor outputs are not silently evicted. |

Boundary coverage includes archive framing/decompression/aggregate validation,
material admission before later decodes, dynamic overflow preservation, Luau
host/frame budgets and the new four-byte prop boundary regression. Native
oversized-cache bypass/eviction, canceled jobs, settings/resize retirement and
controlled VM allocation failure are checked. No safety cap was blindly raised.

Known policy gaps remain: no aggregate process/unified-memory admission budget;
no aggregate decoded shared-model byte budget; installed-package directory
discovery lacks aggregate count/byte caps (warning identities are now bounded
in the expansion below);
persistent user data and explicit unlimited diagnostic traces/recordings have no aggregate disk quota; editor serialized history accounting is
not actual `Value` heap size; and most general host/driver allocations do not
have recoverable OOM injection. On 16 GiB, measured scenes/transitions are the
validated tier. Existing 192/256 MiB optional-cache ceilings are each a small
fraction of 8/16 GiB, but shared accounting must not be summed or treated as a
hardware guarantee. An 8 GiB or discrete-GPU tier, hours-long sessions, arbitrary
custom content and a complete pressure/eviction policy require further work.
The audit is an inventory with demonstrated fixes and explicit gaps, not a claim
that issue #27's entire resource acceptance is closed.

### Deliverables and handoff

All raw receipts, `final-phase-results.json`, `hypothesis-results.json`,
`lifetime-results.json`, `extended-results.json`, `resource-audit.json`, native
logs/actions, baseline/final binaries and packages remain in
`local/performance-0.13.0/`. The editable montage inputs and builder live in
`montage/`; its seven matched cards include the regressed Pool run and unchanged
Retina views. It is 56 seconds, 1920×1080 H.264/yuv420p/30 fps. Recordings are
explicitly paced at 60 Hz; labels use separate uncapped measurements without
capture. Retina footage is downsampled only for delivery. Raw clips are retained.
Full decoding of all 1,680 frames succeeds; representative motion frames and readable labels are inspected. The confirmed video Library ID is `libfile_27a4369238608191bbab0aef74a21fe0` (`VIDEO-LIBRARY.json`).
Three inspected in-game JPEGs are already in Library: dense models
`libfile_ba6cc0561c408191b396e4882920e015`, physics/pickup
`libfile_dddc2ce782fc81918eb2cfc7f5a6c593`, and animated AI/passages
`libfile_c7039a69e1888191af367d02785ce297`.

At this checkpoint, format, strict all-target/all-feature workspace Clippy,
locked all-feature workspace release build and all **217 tests** pass. The five added regressions
use tiny scalar/channel/pose/header fixtures; no Cargo test or CI step bakes a
map. Native final-editor/game checks above pass. The two unrelated untracked
Python audit reports, old packages/receipts and `target` are preserved. No CI
wait, force push, main merge, release or credential change occurs in this pass.

### Current 27-item expansion

The first validated checkpoint was pushed as
`2c9671b2a850deb7585f082a522c3da8824f4329`. The user's later request brought all
27 ideas into this same stage. The outcomes below supersede the checkpoint's
performance summary where noted; its binaries, clips and receipts are preserved.
`local/performance-0.13.0/checklist-27.json` records each item's detailed outcome.
The measured final player SHA-256 is
`fe1f2097fa0735052d72489be965515fd301187054f9fcda7a62f4872eb3e5a5`.

| Items | Implemented or verified outcome |
|---|---|
| 1 | Unchanged dynamic matrices reuse normal transforms, full bounds and winding; independent lighting/receiver dependencies still update. |
| 2 | Cull transparent entries before sorting with that pass's actual frustum and complete posed/world bounds. |
| 3 | Bind shared dynamic geometry once/object; retain compatible decal pipelines while replacing their old-map caustics bindings. |
| 4 | Pack attached lights/flashlight once/frame; exact full-state guards suppress unchanged uploads in all targets, including bake/empty-set clearing. Existing pose/uniform guards remain. |
| 5 | Existing compatible static/character batching retained. Per-object lighting/environment bindings prevent a simple general instancing substitution; no unmeasured renderer rewrite. |
| 6 | Empty static classes skip binding setup. Existing emission-visible bloom, absent-flashlight, empty-effect and zero-index guards remain; transparent emission occluders remain. |
| 7–8 | Shadow/reflection dependency audit: preserve moving/held/posed/opacity/shadow-only visibility and live lights/materials/caustics. Prepared probes are reused; no incomplete dirty shadow/planar key introduced. |
| 9 | Bounded eight-sheet staging and persistent pose/effect/GPU buffers retained; redundant light writes removed. No unsupported staging-ring speedup claim. |
| 10 | Exact unchanged solver-pose/interpolation/trigger-path work skipped; final settling motion and rotation/reversal sweeps retained. All wake/solver/force thresholds preserved. |
| 11 | Query refresh follows both player and door changes once; static structures and unchanged Rapier setters already guarded. |
| 12 | Expanded AABBs reject distant dormant/player pairs before the original exact test, preserving key order. Existing broad-phase/swept pickup/controller/camera queries remain. |
| 13–15 | Existing goal/door/age/retry replanning gates, eight-query fairness, staggered perception, events/timers and change-only state events retained. Perception now clones only the selected contact's role and skips non-winning farther sight rays. |
| 16 | Bounded 32-slot stack schedule replaces automatic-handler path clones/allocation/search; each invocation still dispatches before the next and checks disabled/frame budgets. |
| 17 | Cache immutable validated start/update functions within their owning VM. Source already compiles once/load; resets still create isolated closures/globals. |
| 18 | Separate script timing and bounded VM allocation/automatic GC retained. No demonstrated benefit warrants changing compiler/debug/GC policy or caching bytecode across fresh VMs. |
| 19 | Existing spatial trigger tree, swept queries, occupied retry set and bounded scratch retained. No trigger-contract or polling-cadence change. |
| 20 | Share immutable navigation/grid statistics and probe faces; prepare validated navigation once/variant, upload captures then move them, and release parsed recipe wire bytes early. Collision ownership and bounded segment/PMP overlap remain. |
| 21 | Remove duplicate segment hashing: bounded descriptor-derived reads still receive complete length/SHA verification before output allocation and final PMP verification before decoding. ZIP/alias/source-recovery contracts remain. |
| 22 | Keep existing content-keyed cache budgets/eviction; charge shared backing capacities, pointees and control blocks. No cap increase or path/mtime validity shortcut. |
| 23 | Skip expensive world snapshots when neither explicit tracing nor the persistent lifecycle journal consumes the event. Embedded shaders/preloaded audio remain outside ordinary frame work. |
| 24 | Replace repeated device-loss mutex locks with first-reason `OnceLock`; normal GPU polling/timestamp readback remain nonblocking. Explicit readback/evidence waits remain. |
| 25 | Reuse transparent scratch/precomputed distances, with insertion ordinal preserving exact stable ties; release all five stores on menu. No-blend decals avoid order-vector allocation. |
| 26 | Remove duplicate lighting/transform/physics/hash/immutable-storage work above. Bound process warning identities at 4,096 keys/1 MiB UTF-8 payload, with one terminal saturation notice; ordinary warnings/fatals remain independent. |
| 27 | Verify release optimization 3/full LTO/one codegen unit/strip. Preserve Luau-required unwind. No unsupported PGO, fast-math, target or worker-count change. |

A concrete additional root cause was stationary rotated-prop interpolation.
Using an actual sleeping Bowling ball pose, the old equal-endpoint formula
produced **10 canonical model matrices and 74 consecutive changes across 257
frame fractions**, with at most 1.91 micrometres of position error. Those matrix
bits are inputs to shared visibility and receiver-lighting keys. The identity
return preserves the exact settled pose, without a movement epsilon or reduced
simulation. A 257-scalar regression protects it. In four isolated Group1→Group2
Bowling trials, recovery CPU time fell **4.17–4.99→1.04–1.05 ms** and loop p99
**8.80–8.94→3.07–3.12 ms**; bodies were asleep in both. Physics-step time accounts
for only about 0.02 ms of that difference. The isolated group includes the other
small physics bookkeeping changes, so its entire gain is not assigned to one
line. The separate headless 480-frame campaign retains exact before/after state
hashes and one/four-worker parity; its inactive-player contacts/water/dormant
throughput is effectively unchanged (128-contact serial median ~0.244 ms).

### Expanded final timings and limits

Twenty-eight new capture-free alternating trials use the original `f38c638`
baseline and the final production binary above: two runs/build in ABBA order for
each case. Settings/backend/drawable/presentation evidence follows the earlier
method: M2 Pro/Metal, Release, High/Full, VSync and limiters off, actual Immediate,
no missed ready surfaces, and measured throughput exceeding display refresh.
Static samples use 120 warmup frames then 2,400 lab/1,800 Retina frames. Interaction
runs last 12 real seconds; recovery is 6–11.8 s and closet-near is 2.4–4 s after
ready. GPU samples are deduplicated by completion ID. All means, p95/p99/worst,
stutter counts and physics/held counts remain in `expanded-results.json` and raw
receipts. No compile/recording competes with authoritative trials.

| Matched view/window | Average FPS before→after, two runs/build | CPU update mean before→after | Loop p99 before→after |
|---|---|---|---|
| Lab, 1280×720 | 388–392→429–431 | 1.95–1.96→1.75–1.77 ms | 3.30–4.47→3.13–3.14 ms |
| Home books recovery, 1280×720 | 505–537→594–600 | 1.18–1.40→0.227–0.236 ms | 2.77–5.23→3.05–3.19 ms |
| Bowling recovery, 1280×720 | 91–108→451–452 | 8.03–9.66→1.078–1.081 ms | 13.53–13.78→2.83–2.95 ms |
| Home cushion recovery, 1280×720 | 552–580→557–588 | 1.08–1.16→0.234–0.235 ms | 2.64–2.66→3.01–8.26 ms |
| Pool closet near, 1280×720 | 622–640→626–630 | 0.175–0.182→0.180–0.201 ms | 3.02–8.67→2.94–3.68 ms |
| Arcade busy, 2560×1440 | 244.05–244.62→244.36–244.80 | 1.163–1.165→1.042–1.046 ms | 5.84–6.65→6.16–7.50 ms |
| Bathroom, 2560×1440 | 367–370→364 / **209** | 1.124–1.148→1.007 / 1.211 ms | 5.02–6.62→6.69 / **11.25** ms |

The lab improves about 10.3% in throughput. Books/Bowling recovery CPU savings
are substantial; real-time trajectories differ slightly, and per-frame solver
cost also falls when fewer frames contain a fixed step. These are not changes
to the fixed-step rate, collision accuracy or sleep policy. Cushion throughput
ranges overlap despite lower CPU cost. Closet throughput is flat; its earlier
627→375 pacing pair remains unexplained. Arcade GPU span remains ~4.44 ms.
Bathroom GPU span is 4.11–4.14 ms before, 4.03 ms in the normal after trial and
4.51 ms in the regressed trial; CPU render mean rises from ~1.42 to 3.35 ms in
that regressed run. New-rendering-only repeats also showed pacing variation in
both builds, so no cause or reliable GPU-throughput gain is assigned. The
regressions are retained in evidence and montage labels.

Four matched fresh-process Lantern loads, comparing checkpoint→expanded code
with warm OS cache, measure request-to-commit **4,778–4,832→4,669–4,679 ms** and
kernel peak RSS **2,121–2,126→2,099–2,104 MiB**. Commit RSS is ~2,099–2,101 versus
~2,079–2,082 MiB. This small matched improvement is distinct from the earlier
+312 MiB observation, whose cause/lifetime remains unproven. Retained prepared
payload is 965,299,278→965,299,582 bytes; the 304-byte increase is updated shared
navigation/statistics accounting, not more scene content. Active-map RAM
reduction remains secondary to safe inactive-map release and speed/quality.

The final 318.89-second native campaign completed **42 requests/42 commits,
28 graphics-setting changes and 29 drawable observations**. At all seven menu
captures, active prepared bytes, script modules/VM bytes/calls, AI agents, physics
bodies, pending commits and retired prepared-record/build owners were zero.
The final menu GPU counts were **35 buffers, 19 textures, 35 bind groups and 34
views**, identical at all seven captures. These bounded device/UI/pipeline
resources include the three retained decal pipelines rebound to empty caustics;
fresh/reloaded lanes and mirrors now have identical pixels and draw counts.
Menu RSS ranged 613–849 MiB, ending at 752 MiB; retained allocator/driver capacity
is not called live old-map ownership. Kernel peak was 2,706,325,504 bytes. This
five-minute campaign establishes the tested plateau, not an hours-long guarantee.

A final 123.51-second Lantern→Hub→Home→Lantern→Hub→Arcade→Lantern→Hub→menu
reconfirmation on the measured player completes all eight commits. Every retired
Lantern record/build reaches zero; bounded passive Hub/Home/Arcade cache entries
release at menu, where all eight retired owners and all map subsystems are zero.
Menu GPU counts again match 35/19/35/34. Heavy RSS is 1,544/1,919/1,899 MiB,
menu 737 MiB, kernel peak 2,476,916,736 bytes. This checks the final shared
navigation/probe ownership changes; `expanded-heavy-lifetime-results.json`
retains the snapshots, rather than inferring release from RSS alone.

Final native cancellation/retry confirms a canceled Lantern load cannot commit
later or resurrect map resources. The controlled 3 MiB Luau allocation against
the existing 2 MiB VM cap still disables the failed module, discards staged
actions, repeats safely after reload and releases VM ownership on menu. Lab
contact activation/reset and visitor counts **12→13→12→13** pass; water entry,
dive, resurfacing and rim exit, and the folding door's closed collision,
traversal, inside close and mid-travel reversal pass. Walking into and stepping
onto authored dormant movable props and genuine Rapier sleepers wake them in
focused side/top regressions; fixed props remain fixed. No sleep/solver policy
was relaxed. Home cushion pickup/carry/drop and books jump/landing/recovery, and
Arcade held/contact-heavy paths pass the recorded inputs. The original severe
persistent Home collapse and Pool proximity fault are not fully reproduced or
declared resolved by these inputs.

The earlier same-view Hub comparison remains relevant to the prior-map CPU
hypothesis: fresh **583 FPS / 0.167 ms update** versus **625/636 FPS /
0.132/0.149 ms** after repeated heavy transitions, with the same 39 draws and
zero old scripts/AI/physics. Old-map active ticking did not explain a slowdown
in that tested case. Higher RSS alone establishes neither a leak nor a cause of
other pacing cases. The isolated heavy-menu cleanup callback's 52.46 ms remains
a latency lead; skipping discarded snapshots does not prove that drop cost gone.

All four final matched static Home/Pool/Arcade/lab frames have **zero changed RGB
channels** against the original baseline. The final native editor again edited,
saved, closed, reopened the edited document, reversed the edit and exported the
whole lab project normally. Restored source values match except serialization
of the existing `floor_y=0` default. Prepared export payloads are identical;
manifest/provenance fingerprints legitimately differ for the rebuilt compiler.
The existing large collection passes validation without rebaking. Separate lab
Luau invocation timing (including capability setup) is 0.0899 ms mean, 0.1285 ms
p95, 0.1362 ms maximum across its bounded 64-call diagnostic, versus checkpoint
0.0781/0.1052/0.1336 ms. There is no isolated VM execution speedup claim; automatic
schedule allocation/search occurs outside that invocation interval.

The warning-key policy gap above is now closed: key payload and cardinality are
both bounded, duplicates allocate no string, and pressure stops new identities
with one notice. It is diagnostic suppression state, not an asset-admission
limit. Hash-table/String metadata is additionally bounded by cardinality; the
1 MiB label is UTF-8 key payload, not total allocator residency. Other aggregate
process/shared-model/directory/disk/OOM/tier-policy gaps remain explicit in
`resource-audit.json`; issue #27 is not declared fully closed.

The updated **56-second, 1920×1080, H.264/yuv420p, 30 FPS montage** is confirmed
in Library as **`libfile_d5af9e225d148191b29083e5883c28aa`**. Its 1,680 frames
fully decode; sampled full-resolution frames have readable labels and the
matched actual game views. The nine new raw after clips, original before clips,
editable `edit.json`/builder, QA frames and decode/probe receipts are retained in
`local/performance-0.13.0/`; the new assembly is in `montage-expanded/`, upload
receipt in `EXPANDED-VIDEO-LIBRARY.json`. The first delivered montage
(`libfile_27a4369238608191bbab0aef74a21fe0`) remains untouched. Capture is explicitly
paced separately; all labels use capture-free timings and show overlapping or
regressed results honestly. No captured overlay supplies a benchmark number.

Final format, strict `cargo clippy --workspace --all-targets --all-features --
-D warnings`, locked all-feature workspace release build, and **222 tests**
(0.43 s) pass. Added scalar, contact, sorting and diagnostic-budget fixtures have
specific regression value and require no map bake. The final test-only rebuild
produces the exact same player SHA as the accepted timings. Gate logs are
`final-expanded-{fmt,clippy,tests,build}.log`; the actual final player, compiler,
editor and physics tool plus hashes are preserved in `validated-binaries/`.
Native receipts/results include `expanded-results.json`,
`expanded-extended-results.json`, `expanded-final-*-results.json`,
`expanded-visual-results.json` and `expanded-editor-roundtrip-receipt.json`.

The separate fresh engine sweep follows below. Remaining leads at that handoff included the
unexplained render/acquisition pacing cases, unchanged busy-view GPU cost,
residual lighting/animation work, complete safe shadow/reflection dirty keys,
PPS segment/output overlap, duplicate request-local package opening and unused
graphics-only gameplay assembly, aggregate unified-memory/model/discovery
budgets and recoverable host/driver pressure behavior. Already solved guards
and the 27 outcomes above should not be reimplemented. Mannequin ragdolls, docs
cleanup and later editor/audio/menu/UI/style work remain separate queue stages.


### Separate fresh engine sweep

This pass starts from staging `f160493f8f0f93593a7981298e61cbc12ff8f473`.
Its evidence is under `local/performance-0.13.0/fresh-sweep/`; original evidence
and the preceding 27 dispositions are preserved. Shared rendering, lighting,
geometry, physics integration, AI/Luau, loading and cache ownership were read
again; `inspection.json` records the fresh leads. Issue #31's GPU skinning/pose
sharing, portal occlusion, streaming, codec, lightmap-storage and new job-system
projects remain deferred. A reproducible Pool opening/entry CPU fault is traced
and corrected below; the independent native presentation wait remains a separate
limit and does not establish that every historical Home/Pool fault is solved.

The first three localized improvements were validated in player SHA
`8eeeda45053826f81cad1bece2382202fef410fdec0751e85b685ee61efe95a7`;
the later bounded surface/model diagnostics build is
`051ca0743777e340934bae0300fb6607bd849d52cd439a24a94da164ceb8a552`:

- Skinning reuses the last exact joint/weight blend's normal inverse within one
  skin pass. The scratch borrows an immutable animator, cannot survive a pose
  change, allocates nothing and preserves rigid-joint handling and arithmetic.
  Mannequin blended inverses fall from 968 vertices to 47 adjacent runs;
  skeleton from 626 to 151. A small deterministic fixture protects exact bits
  across different keys, poses, zero/tiny weights, invalid joints, reflections
  and singular scales; it needs no model load, bake or GPU. This is CPU normal
  preparation, with no GPU skinning, animation cadence or crowd sharing change.
- Cold loading uses one validated semantic/dependency snapshot rather than two
  simultaneous complete opens. The variant reader still independently checks
  the reopened archive's manifest, every payload hash and all existing bounds.
  No integrity check is skipped using a filename, timestamp or unchecked cache.
- Large dynamic-caster sets reject segment/AABB misses before transform/mesh-BVH
  work. A padded f64 slab guard refines the original enclosing-box guard and
  keeps original candidate order and exact triangle/alpha tests. Padding also
  accounts for large model pivots canceled by placement translation. Sets below
  64 retain the original monomorphized path after the all-size prototype added
  about 0.05 ms to the lab. Three narrow scalar/actual-triangle/canceled-pivot
  fixtures protect against false rejection without baking or loading maps.

On the same M2 Pro/Metal hardware, all timings below are capture-free with
VSync/limiters off, actual Immediate and the recorded physical drawable. High
textures/filtering/lighting, Full lightmaps/reflections, bloom and FOV60 remain.
**Immediate alone did not establish uncapped throughput in this sweep:** several
views exhibit acquisition pacing. Their FPS changes are retained as observations,
not accepted throughput gains. No recording overlays supply timings.

| Case and evidence | Before → candidate | Disposition |
| --- | --- | --- |
| Isolated normal-cache lab, CPU update | 2.149/2.148 → 2.116/2.108 ms | Small repeatable CPU reduction; FPS overlaps under presentation pacing. |
| One visible moving bowling ball, CPU update, final ABBA | 7.832/7.886 → 6.936/6.883 ms | About 12% less CPU update work. Exactly one active body throughout all selected samples; physics step remains about 0.05 ms. |
| Same moving-ball view, observed FPS at 1280×720 | 109.78/110.64 → 117.08/118.09 | Pacing-limited observation; no robust uncapped throughput claim. |
| Final lab, 1280×720 | 168.49/172.53 → 165.76/166.53 FPS | Presented regression retained despite CPU 2.198/2.204 → 2.067/2.155 ms. |
| Bathroom, 2560×1440 | 367.33/367.74 → 363.71/365.56 FPS | Flat/slightly regressed. Earlier 209 FPS fault did not recur; not declared solved. |
| Busy Arcade, 2560×1440 | 241.26/244.10 → 245.02/244.26 FPS | Effectively unchanged; GPU work about 4.4 ms remains. |
| Fixed near closet, 1280×720 | 172.25/178.81 → 179.25/184.45 FPS | Overlap under acquisition pacing; original severe fault remains open. |
| Lantern request-to-commit, fresh process/warm OS cache | 4719/4771 → 4707/4703 ms | No robust end-to-end load-time or peak-RAM reduction claim. Duplicate validated work is removed. |

`accepted-results.json` and raw per-run receipts preserve the 24-run final ABBA
set. Original real-time interactions use matched twelve-second runs and
6–11.8 s recovery windows. The initial original closet sequence regressed from
184.8/173.4 to 159.8/151.6 FPS; fixed-eye controls do not erase that result.
Bowling repeats also show why movement state must be retained: one BEFORE run
still had active bodies and CPU 6.62 ms, whereas the other BEFORE and both AFTER
runs were settled at about 1.28–1.30 ms. The earlier round had the active outlier
on AFTER instead. No general persistent Home collapse is declared solved.

Pool causal investigation is retained in `pool-cause/`. Bounded render spans
locate the largest stalls in drawable acquisition, with CPU preparation and
encoding small. Discarded frames are explicitly labeled render iterations, never
presented FPS. Enforcing GPU completion in both paths rules out CPU run-ahead as
the sole discard advantage: far/near completed discard GPU spans are about
0.61/0.90 ms, versus about 5 ms normal presented render time. This locates a
presentation cost but does not establish its cause. Eight alternating native
window controls verify actual Metal `displaySyncEnabled=false`,
`presentsWithTransaction=false`, three drawables, Immediate, 1280×720 and a 120 Hz
display. Raising the window establishes real native INPUT_FOCUS throughout the
selected frames without removing the slowdown. Interior controls also retain small CPU update cost. The open-door fixed-eye
probe initially obstructed the leaf at 29 degrees; it is not proof of traversal.
Eight SDL-owned Metal-view versus raw-host trials were flat and that prototype
was rejected. All 15 unique closet GLBs (16 placements) have finite geometry,
13,372 triangles in total, opaque double-sided materials and no exact duplicate
position triangles or degenerates. Hiding all closet model draws changes the
near submission from 81 draws/54,390 indices to 28/13,602 but leaves roughly
175–180 FPS; binary partitions and interior repeats do not identify one primary
asset culprit. This draw-only control preserves physics and baked lighting; it
does not rule out every asset/system interaction in the original fault.

An attached 12-second native Metal System Trace (successful target PID, unlike
the preserved invalid launch trace) finds 6.819 seconds of drawable-acquisition
wait in a nine-second steady window. About 4.884 seconds of those waits have no
recorded GPU execution overlap; Places GPU-stage union is 3.114 seconds. These
are merged API intervals, not a GPU utilization percentage. Native SDL/Metal
controls with no Rust/wgpu/engine/assets also wait about 5 ms for drawables.
Direct/scheduled presentation, transaction flush, event pumping and temporary
App Nap suppression do not remove it. An independent AppKit/Metal control also
reproduces the wait with real native focus. This establishes a general native
presentation limitation in this session, **not the cause of the user's original
location-specific severe closet drop**.

The user authorized one temporary display-refresh control. System Settings
verified ProMotion → 60 Hertz → ProMotion, with the original setting restored
immediately after the bounded control. At 60 Hz the empty Metal control runs
59.998/60.000 cycles/s and both full-quality Pool near/far views run 60 FPS
despite actual display-sync false. They are refresh-capped and excluded from
throughput claims. After restoration, empty controls initially run 318/324
cycles/s and focused near/far Pool views 269–275 FPS. Later ordinary-window
old/candidate ABBA overlaps around 137–143 FPS; native focus and session state
remain material qualifications. Changing overall presentation state has not
proven that the original near/far differential is fixed. Continuous original/minimal-map controls are recorded under
`pool-cause/map-isolation/`. The minimal project retains the exact closet room,
three exterior walls, shared door opening, all 18 prop records with physics and
both switches, and relevant lights. It replaces outside Pool context with a
small enclosed approach and rebakes Full lighting in that diagnostic context.
Both sources have zero geometry errors/warnings; the 2.16 MB package builds in
6.47 s, validates and verifies current. Native editor open/save/reopen passes
with exact manifest/source bytes preserved. Closed-door first approaches show
only modest differences in both maps (original 516→467 and 537→511 FPS; minimal
563→514 and 570→514). Return legs hit the minimal approach's west boundary at
X12.85 instead of the original X12.05–12.08, so those are not called matched
positions. Later camera-following open routes escape a 120 FPS presentation
plateau: original far 516, interior east 721, interior west 509 and return 499
FPS; the minimal copy also reaches several hundred FPS. None of those clear
routes reproduces the severe fault. No shipped map or asset has been changed, and no
quality reduction, frame discard or limiter is proposed as a fix.

The user's clarification—**after opening or entering**—led to a supply-contact
route at Z13.6. It reproduces a distinct CPU-heavy fault in the full original
map: far 527 FPS / 0.319 ms update, pushing/entry 140 FPS / 6.009 ms update, with
physics stepping only 0.020 ms. The minimal closet retains its supplies and
physics yet runs about 536 FPS / 0.654 ms on that segment. A bounded profile
captures about 16 ms in dynamic-lighting preparation. The earlier draw-hiding
control retained physics and CPU occluders, so it did not exclude this work.
The existing caustics-off diagnostic only suppresses displayed focus; it still
prepares visibility and is not an isolation of this CPU path.

**The fourth shared-engine improvement corrects unnecessary water visibility
invalidation.** A door or movable prop changes one global visibility revision.
Previously every active water-light source then revisited its supported cells
and possible movable blockers, even when all changed objects were outside any
light-to-water path. Pool has 65,120 receiver records and 209,420 sparse factors.
Props do not contribute light here; they are possible blockers. A padded bounds
rejection before the unchanged full separating-axis certificate first reduces
this profiled preparation to 4.0–4.1 ms. The final change retains the latest
revision's old/new changed-caster bounds plus the field's receiver envelope.
Each source reuses its existing certificates only when its finite emitter hull
or infinite directional prism is disjoint from that changed geometry. Moving
out includes the old bounds; admission/removal conservatively includes former
and current casters; a consumer missing revisions performs a full refresh.
Source activation retains its existing full invalidation. No map/asset IDs,
quality changes, throttling or deferred architecture are involved. These new
dependency records are scalar bounds/revisions, with no new retained map owner
or per-frame heap allocation.

Isolation compares the earlier core checkpoint `051ca074…` with final player
`1e5629935f032aa07dd8aee68a18a2da80cb2015643a2acd89823687827349d6`:

| Matched native control, 1280×720 High/Full | Before → final | Evidence and limit |
| --- | --- | --- |
| Door-only update, 128 predetermined motion frames/run, ABBA | 16.987/16.864 → 1.719/1.768 ms | About 90% less update CPU. Every selected frame has zero active physics bodies; physics step is about 0.001–0.002 ms. |
| Door-only observed FPS | 59.26/59.52 → 136.12/136.44 | Settled controls vary 121–151 FPS; presentation pacing remains, so this is not a general uncapped-throughput multiplier. |
| Original contact-route update during pushing/entry, ABBA | 11.726/11.624 → 1.658/1.227 ms | Normal real-time input and full props/physics; positions and body state retained in raw receipts. |
| Same entry observed FPS | 69.6/69.8 → 124.2/123.6 | Far views sit near a 120–128 FPS presentation floor. The CPU cliff is removed; the platform wait is not. |
| Focused off-region water visibility diagnostic | About 16 ms combined preparation initially; 3.43–3.52 ms water visibility after bounds rejection → 0.002–0.005 ms water visibility final | Ordinary dynamic receivers remain about 0.8–0.9 ms in the last profile. Diagnostic logging is excluded from authoritative timing runs. |

The final player's lab/one-body recheck against `f160493` also retains the
CPU gains (`final-journal-timings.json`): lab 2.356/2.340 → 2.280/2.284 ms;
one moving visible bowling ball 7.913/7.907 → 6.973/7.093 ms, with exactly one
active body in every selected sample. Observed lab FPS is 141–143 → 144–145,
and moving-ball FPS 109 → 115–117; acquisition pacing still qualifies those
throughput observations. The older flat/regressed Retina and interaction sets
above remain recorded rather than being replaced with only favorable trials.

A few thousand scalar boxes compare bounds rejection with the existing full
certificate across tiny/large scales and large translations. A two-triangle,
0.4 m water fixture checks both finite light and sunlight bases, unrelated
motion reuse, rotation, scale, movement in/out, hidden/restored/despawned
blockers and missed revisions against complete fresh field samples. It uses
no assets, GPU or atlas bake. Ten deterministic native High/Full captures—five
Pool states and five Aero circular-water/sunlight states—have **zero changed
RGB channels** versus the earlier core checkpoint: initial, off-water move,
over-water placement, move out and despawn. The actual prepared fields contain
209,420 and 1,650 factors respectively. These checks protect the supported
visibility calculation; they do not promise that arbitrary future in-water
workloads are free.

Both the core implementation and later diagnostic source pass format, strict
all-target/all-feature Clippy, locked workspace release build and **228 tests,
zero failures/ignored**. The final `final-integrated-*` logs record 0.40 s tests
and a 3m17s locked workspace release build; all four executables and SHA256 values
are preserved in `final-validated/`. The final built player is byte-identical
to the one used for the accepted Pool controls. Default-feature compilation is included in the
workspace tests. Earlier failed diagnostic feature-gating logs are retained
and superseded by this successful run. Four matched lab/Home/Pool/Arcade frame-180 captures
have zero changed RGB channels. Native lab controls retain 25 contacts, activate
and reset the intended props, and spawn/reset the visitor. Luau's bounded 64-call
host-inclusive diagnostic averages 0.106 ms, p95 0.189 ms, max 0.209 ms; no isolated
VM speedup is claimed. Folding-door obstruction/traversal/reversal, Pool water
entry/dive/resurface/rim-exit/reentry, original cushion/books/bowling interactions,
loading cancellation, graphics restore and allocation-failure checks pass.
The lab source is unchanged; this pass relies on the preceding native editor
edit/save/reopen/reverse/export proof rather than claiming another editor pass.

The earlier eight-transition heavy-map cycle completes all eight requests and
commits. Every retired prepared owner is zero at menu; active prepared bytes,
script modules/VM, AI, physics and pending loads are zero. Final settled menu GPU
counts are 35 buffers/19 textures/35 bind groups/35 views (one transient buffer
persists in two earlier tail samples). Active Lantern prepared data remains
965,299,582 bytes, accepted by the user. Heavy RSS samples are 2023/1972/2265 MiB,
menu 1016 MiB, peak 2418 MiB. This is not monotonic growth evidence, and neither
this menu residency nor the historical +312 MiB has a proven cause. Map owners
releasing does not prove all allocator/driver residency is explained. Metal HAL
byte counters remain unavailable. Existing process/shared-model aggregate
budgets, discovery directory/disk quotas, general recoverable allocation failure
and 8 GiB/discrete-hardware gaps remain explicit in the original resource audit;
arbitrary active-map RAM reduction is not substituted for a demonstrated defect.

On the final `1e562993…` player, lab contacts activate/reset and the Luau visitor
again follows 12→13→12→13; Pool water entry/dive/resurface/rim exit/reentry and
Home carrying pass. A final Pool→Aero→Hub→menu check leaves all three retired
prepared-build/record owners at zero, active prepared bytes zero, no scripts/VM,
AI, physics, pending commit or preparation. Worker caches report prepared,
identity, image and model counts zero. Menu GPU tail returns to 35 buffers,
19 textures, 35 bind groups and 35 views after transient capture/command buffers.
Menu RSS is about 595 MiB in this shorter run; it does not explain the earlier
menu residency or historical +312 MiB. Final receipts and state witnesses are
in `final-native-checks.json`; no new ownership-bearing cache was introduced.

Fourteen new matched short raw clips and editable assemblies are preserved
outside `target/`. The final decoded and visually inspected **112-second**
montage keeps the original 56-second preceding montage and six core-sweep cards,
then appends the newly matched Pool opening/entry pair with CPU savings and
pacing qualifications. Local file:
`fresh-sweep/montage-final/places-0.13.0-performance-fresh-engine.mp4`,
1920×1080 H.264 High/yuv420p, 30 FPS, 3,360 frames, 7,199,587 bytes, SHA256
`d851a7e6150f7cdfc78e13433ea6b693d780c25830bd281f42025f165e50623e`.
It replaces only this sweep's checkpoint item as **Library
`libfile_bb4cac714b748191aeb7324d98396e43`, version 1**, file
`file_00000000d624823091171dd807ae54ba`; a read verifies its ID/name/size/version,
and its local Library xattr is verified as 1. The checkpoint's version-0 local
video/receipt and all raw evidence remain. A read also confirms the untouched
preceding Library `libfile_d5af9e225d148191b29083e5883c28aa`. Captures are paced
at 60 Hz with 30 FPS output; numeric panels use separate capture-free runs.

No deferred issue #31 project, shipped map, asset, physics policy or integrity
check is changed. The reproduced Pool CPU fault has a validated shared-engine
fix; the native OS/Metal presentation wait and broader historical Home/Pool,
RAM-residency and hardware-tier limits remain explicit. CI is owned by the
final integrated queue stage and is not awaited here. The next stage is
editor/audio polish, then main-menu/visual styles; mannequin ragdolls is second
to last, before docs cleanup/final integration. The approved stair-generation
and structural-thickness work is recorded for that next owner, not implemented
in this performance pass.

## Editor and audio polish stage — 2026-10-21

This stage starts from `6c1127ff869d5c0c44f06725efd70b489c708ed6` on
`0.13.0-staging`. The single writer retained the previous performance evidence,
packages and two unrelated untracked Python audit reports. Issues #18 and #29
and their comments were read; completed 0.12 authoring work was reused. Issue #31
architecture projects remain deferred. No main merge, release or CI wait belongs
to this stage. Cargo's release version remains the queue's existing 0.12.0 value;
this work does not independently change release metadata.

### Delivered behavior

- **Visual selection:** 525 indexed, committed 160×120 PNG previews cover the
  actual prop/entity GLBs, catalog tint, material images, six native fixture
  bodies and all three native door styles. Eight-tile grids retain names, theme
  filtering, search, paging, keyboard access and missing-preview feedback.
  Room material selection distinguishes Floor/Ceiling. Door styles and door
  materials preserve the rest of the authored instance. A visible tile is decoded
  at most once per frame; CPU artwork retention is capped at 24 tiles (1.76 MiB),
  and the existing GPU UI image cache remains capped at eight. Offline generation
  uses shipped assets; the editor never renders the entire catalog continuously.
  `--with-editor` distributions include the preview files and index.
- **Bake review:** Project → Bake status uses the installed compiler's dependency
  and integrity contracts on a background worker. It considers both individual
  outputs and the complete export, with valid / metadata refresh / stale rebake /
  invalid or unknown states, causes per row and paging. Valid rows cannot start
  redundant builds. An affected row builds only its selected map. Existing
  compiler progress, cancellation and atomic publication retain the last usable
  package. The size review counts physical shared project blobs once, classifies
  their virtual roles, and explicitly distinguishes ZIP archive bytes, entry
  bytes before internal codecs, installed cache and RAM. Index/size work also
  runs off the UI thread.
- **Structure:** the shared default is **0.15 m** in the existing metre units,
  compared with the previous 0.12 m rigid terrain backing. Floors, raised regions,
  slopes and stairs have actual downward faces and exposed edge bands from the
  same convex patches as collision. Existing solid ramp/stair wedge semantics
  remain. Equal internal seams and wall-covered bands are clipped, and lower
  ceilings own matching underside material. Player head contact uses that same
  backing. Geometry revision 4 intentionally requires older prepared geometry
  to be rebuilt; the planner must not call that global contract change unrelated
  per-level motion.
- **Stack safeguards and Annex:** Safety/export reject insufficient lower-ceiling
  to upper-floor clearance, including a ceiling intruding into the upper occupied
  room. An enclosing hall roof above several storeys retains its ownership.
  Safety provides an exact-source, reviewable ceiling proposal with adequate
  standing clearance; nearby explicit flush-fixture drops are retained in the
  same undoable proposal. Nothing silently moves a room or fills a shaft. Add
  Upper Floor places the new floor above the unchanged lower ceiling. Tower
  rooms 0–2 keep their walking elevations and have 3.05 m ceiling height; three
  redundant landing ceilings inside the tall stair hall are open. Only the
  corresponding three flush-fixture heights are reanchored by 0.15 m. Existing
  ramps, stairs, passages, floor elevations and intentional layering remain.
- **Stairs:** Add Geometry → Connect two floors selects the lower/upper rooms and
  width. The conservative straight-flight workflow supports overlapping flat
  floors, 0.9–4 m width (1.2 m default), risers at most 0.2 m, 0.28 m treads,
  landings at least width/1.2 m, a real cut shaft and upper standing clearance.
  It checks the flight/landing envelope against walls, transformed prop bounds,
  fixture footprints, door swing envelopes and intervening floors. Ambiguous
  global terrain or insufficient fit gives an actionable refusal. Preview/Cancel
  preserve source; Apply creates ordinary editable room/stair records in one
  undo step. Manual Staircase and numeric controls remain available.
- **Editing:** colored viewport axes translate props, fixtures and isolated rooms
  with toolbar snapping, private previews, Escape/focus-loss cancellation and one
  history commit on release. XYZ labels share the actual red/green/blue viewport
  colors. Isolated-room moves carry supported contained records and fog boxes;
  shared boundaries, crossing boxes and coordinate-bearing global geometry/scripts
  require explicit review. This is not arbitrary connected-room group dragging.
  Existing per-map history bounds (64 revisions / 32 MiB per undo or redo stack),
  coalesced preparation and cache pruning remain; diagnostic receipts now expose
  retained counts and preview latency. Safety remains explicit/background work,
  with no continuous all-pairs validation added.
- **Audio:** Kira 0.12.5/SDL3 is retained. The 63 WAVs now supply 25 cues; a restrained
  `water_transition` reuses recorded splashes for real swim entry/exit. Loading
  and reset do not manufacture a transition. The unusually loud third switch
  and plastic takes have reproducible post-normalization gains of 0.65 / 0.5;
  source identity, trim and edge fades remain. Provenance and packaged WAVs are
  updated. Standalone emitters expose gain and optional near/far attenuation;
  authored/triggered playback carries the same override. Preview supports
  stop/replay and is cleared on Back, assignment, close and map replacement.
  Missing references remain named validation errors, and voices retain the
  engine's bound of 48.

### Acceptance and limits

The stage's authoritative logs, exact package/binary identities, native source
fixtures, screenshots, recipes and receipts are under
`local/editor-audio-0.13.0/`; the final index and ownership state are in
`FINAL-HANDOFF.json`. Deliverables remain outside `target/`. Tiny deterministic
regressions protect stair shaft/landing support and undo/save/reopen, structural
undersides matching collision, reviewable ceiling/fixture correction and room
fog translation/refusal. They perform no shipped-map bake. Existing trigger,
fog, door, physics, placement-intent and shared water-lighting regressions remain
in the owning suites.

Native audio campaigns pass controls (37 checks), lifecycle (43), settings (28)
and movement/material footsteps (32), with real post-mixer PCM and voice traces.
The actual master-zero restart records saved master `0.0`, zero active/new voice
gains and 64,512 stereo PCM frames with exact zero peak. The next menu stage must
reuse `Settings::set_master_volume`, normal settings persistence and the shared
`AudioSettings::master_volume` mix path for main/pause settings. No new main-menu
scene or second master bus was built. These observations do **not** establish
subjective sound quality or physical device hotplug behavior; neither was tested.
Browser audio and other desktop platforms were not part of native acceptance.

Native generation exercised Preview/Cancel/Apply/Undo/Redo, save/reopen and export;
ordinary W/S movement, with noclip off, reached and returned from the generated
3.35 m landing. Native drag receipts distinguish actual cancellation after
reselecting objects following undo; all three cancellation sources are unchanged.
The 80-edit history run retains 64 revisions / 33,102 serialized bytes, and after
64 undos retains 64 redo revisions / 33,103 bytes. Its native peak footprint was
371,967,224 bytes; serialized history accounting is not process RAM.

Initial diagnostic failures are retained: catalog/transition/script admission
correctly refused incompatible baseline packages; corrected geometry-only
baseline fixtures retain compiled scripts and remove transition admission only
in the copied diagnostic source. One baseline capture run had two unavailable
surface frames and is excluded from throughput evidence. Early UI recipes that
requested hidden scene controls or used Close instead of Back were corrected;
an extra diagnostic nudge correctly produced an unsaved-close dialog. None was
used to bypass an access denial, and no user project was discarded.

The matched seven-map editor baseline did not show a general map-switch speedup:
ready times before/after were Hub 399/416 ms, Home 413/414 ms, Pool 2700/2780 ms,
Office 397/399 ms, Arcade 1246/1148 ms, Lantern Hollow 5806/5893 ms and Winter
1470/1465 ms. One short native peak-footprint sample was 3,095,349,408 versus
3,069,003,912 bytes; that is not a general memory improvement claim. A subsequent
nine-member project loop completed twice and stabilized at nine documents and
69 decoded images, returning to each map's same retained model count. Its peak
footprint was 3,118,221,472 bytes. Serialized history and image cache bytes are
not total process residency.

The retained Pool UI stall was investigated before handoff. Native sampling with
symbols traced it to `Caustics::upload → ReceiverField::build`, including source
visibility work on the UI thread. The editor now prepares that exact CPU field
on its existing coalesced preview worker and attaches it to the same immutable
prepared scene. No GPU handles cross threads, no new scheduler/streaming project
was started, and the ordinary player path retains the existing preparation.
The final matched, unsampled release pair records the longest UI upload handler
**1,595.38 → 78.56 ms**, while total request-to-ready stays approximately unchanged
at **2,828.78 → 2,860.48 ms**. Both retain 149 viewport objects, 38 models, 43
images, 19,136,512 model texture bytes and exactly 65,120 water receiver samples /
208,716 sparse factors / 1,398,100 mask bytes / mean 0.140651. The new worker field
cost is 1,547.09 ms. The symbol witness locates receiver preparation on that
worker. The remaining ~79 ms upload span, large-map CPU preparation and
multi-gigabyte residency are explicit limits; broader issue #31 work stays deferred.
These editor operation measurements use Fifo at physical 2880×1632 and the
editor's normal pacing, and are **not FPS throughput evidence**. Player captures
request Immediate, retain actual drawable/settings in receipts, and show the
known ~120 Hz presentation plateau; this stage claims no engine FPS gain.

A native two-level bake review demonstrated metadata refresh, both current,
then only Workshop requiring a bake while Second stayed current; checker and
build cancellation retained usable output. An additional export-only regression
selects the **non-entry** Second member with no standalone output, changes only
its name and builds it. It reports a verified `prepared_world` hit in 51.84 ms,
with no illumination solve or probe capture, publishes a current standalone
package and leaves the complete export byte-for-byte unchanged. Compiler reuse
now resolves project member records before checking provenance and can read the
standard complete export as a read-only cache fallback. The tiny existing
project-archive fixture also covers this virtual record selection. Structural
Safety proposals share one immutable source and retain at most 256 proposals;
full candidates are constructed only when reviewed, avoiding a full document
copy per stacked-room pair.

Annex Tower's explicit geometry check reports **zero errors / 48 retained
heuristic warnings**: 14 missing-wall runs, 31 reversed faces and three room-leak
samples. No new placement ignores or broad suppression were added. Native final
views inspect the SECOND ROOM ceiling, flush fixtures, exposed upper-floor edge
and real ramp/stair backing. The final three-level export passes integrity and
current-source verification for every member, and import preserves its project,
three source JSONs and three Luau scripts byte-for-byte. Full collection rebaking
is reserved for final integrated validation after subsequent stages.

Actual native audio receipts cover a triggered, authored emitter at gain 0.6
with 0.5/6 m attenuation, preview stop/replay/Back cleanup, and door/physics/
trigger/fog authoring controls. Its far voice gain is exactly zero while the
near voice has positive mixer gain; unrelated attached water retains its
catalog falloff. A finished Discharge basin/ramp route produces real entry/exit
cues and finite nonzero post-mixer PCM. Continuous ordinary movement exposed
brief swim-state oscillation on the ramp; a 750 ms real-time sound retrigger
guard suppresses repeated splashes without altering collision, water physics or
queuing audio. It resets on unload. Diagnostic pose-assisted and continuous
routes, including the initial repeated-splash witness, are retained separately.
The final continuous route uses W/D/S from the initial dry fixture with no
intermediate player pose, observes the entry and exit sound without subsequent
retrigger chatter, reaches grounded dry support with noclip off and records
492,544 stereo PCM frames (peak 0.221519). Shutdown reports zero live/queued
voices, dropped requests and errors. No subjective listening claim is made.

The final macOS distribution includes the player, compiler and editor in flat
and app forms, with every one of the 63 WAVs and 525 previews byte-identical to
source. The packaged editor opens from `/tmp` with no asset-root override and
exercises paging, search, no-match feedback and ceiling materials; 12 page
checkpoints retain at most 16 decoded tiles. The first package recipe searched
an authored prop's ID as though it were its model label and was corrected to the
existing Workshop scene; the failed log remains. No user content was removed.

Six native screenshots and a 12.97-second, 389-frame silent stair traversal demo
are preserved in `local/editor-audio-0.13.0/deliverables/`. Library saving failed
during preparation in the required batch workflow, including one normal-network
retry; it returned no confirmed file IDs. The local deliverables and failure
logs remain ready for the parent to save later. No alternate upload path was
used after starting the required workflow.

Remaining acceptance boundaries are explicit: generated stairs require the
supported overlapping-flat-floor straight-flight fit; connected-room group
moves, arbitrary stair layouts, subjective audio balance, physical device
hotplug, browser audio and other desktop platforms were not validated. Main/pause
master-volume UI belongs to the following menu stage. Mannequin ragdolls remain
second to last, then docs cleanup/final integration. No main-menu scene, main
merge or release was made here.


The final gates pass: `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --all-features -- -D warnings`,
`cargo test --workspace` (232 passing; no shipped-map bake), and
`cargo build --locked --release --workspace --all-features`. Native structural
review additionally confirms Preview/Cancel leave source exact, Apply changes
the lower ceiling 3.0→2.85 m and flush fixture 2.98→2.83 m together, Undo restores
the original, and Redo/Save retain the reviewed proposal. Rejected export of the
zero-gap source leaves its previous package hash unchanged. The scope page is
advanced before choosing the measured finding; the initial mistaken recipe is
preserved. Staging commit and verified remote identity are recorded in the
on-disk handoff after publication; CI remains the final integrated queue's job.


Final local deliverables are `Relay-Annex-Complete.placesmap` (23,465,925 bytes,
static and full variants), `Sound-Authoring-Complete.placesmap`, and the flat/app
`payload-complete/` distribution under the stage evidence directory. All older
stage packages remain. Their SHA256 identities and exact release binaries are
in the final handoff. The next menu owner can start at
`src/game/settings.rs:1260` and `src/game/app/audio_runtime.rs` for the existing
persistent master control/mix path.


## Main menu, miniature and visual styles stage

Both supplied PNGs were inspected as actual pixels. The UI original is the
preserved local image from “Draft Places menu concept art”; the scene reference
was materialized on this Mac through the Library download helper. The native
menu uses the reference's cream/yellow restrained palette, compact pixel Places
wordmark, slogan and left-side Explore, Settings, Credits and Exit. The wordmark
is composed with the committed font atlas; three unsuccessful generated alpha
cutouts remain local and are not shipped. There is no main-page level carousel.
Explore has a separate four-card grid, actual package thumbnails, a font-atlas
fallback, full paged descriptions, pinned import/back actions and keyboard
navigation. Its metadata/image reads do not prepare the collection's worlds.

The application owns an independent 8.02 MB prepared scene at
`assets/shared/menu/scene.bin`, authored in `projects/menu`. Office, Pool, Home
and Arcade are its four spatial depth layers. It uses the rebuilt catalog,
a recessed flowing pool with caustics, practical fixtures, six mannequins,
five pickup items, three reusable buoyant ducks and the real folding closet.
The camera is fixed at `(0,1.6,3.5)` with a 55° vertical lens. The resource is
packaged under shared assets and does not need any gameplay package or checkout.

The Luau module contains 120 five-second cues across twelve vignettes: delivery,
books, towels, a cushion/closet inspection, Arcade prizes, duck flotillas,
greetings, coordinated crossings and closing rounds. These are 120 chronological
beats, rather than 120 unrelated complete encounters. One pending contact action
aligns a real pickup/putdown clip with transfer at 0.9 seconds. Carried objects
use posed hand sockets; putdown restores the same key, mass, scale and authored
reset position to the ordinary solver. The menu-only pickup rule cannot bypass
normal gameplay's held-item and reach requirements. No script spawns entities
or accumulates timers. Reset at 600 seconds cancels the VM's pending work and
restores actors, item groups, closet, lamps, navigation and audio.

Main/Explore/settings advance only the menu world; gameplay replaces its
simulation and GPU resources and explicitly releases worker cache ownership.
Pause/settings retain gameplay. Return unloads gameplay through the recent
cleanup path and loads a fresh menu scene. Existing lighting invalidation is
preserved. The basin review exposed overlapping upper structural bands and
terrain skirts; correcting face ownership removes the duplicate without
changing collision backing. Geometry revision is now 5. The menu geometry
check reports zero errors and zero warnings. The first full native cycle caught
an unreachable Pool starting position and an off-camera closet; final authoring
clears the attendant, moves the Home alcove into view and separates the Office
desk from actor traffic. The original full-cycle recording is preserved. Existing gameplay packages remain
readable; final integration owns rebaking those sources against the revision.

Standard, CRT, Legacy and VHS are separate persisted presentation styles. CRT
adds scanline/mask/vignette treatment; Legacy supplies a warmer reduced display
palette; VHS adds chroma/time-base/head-switch/noise treatment. They use the
existing postprocessing path and leave quality, textures, lightmaps, reflections
and bloom independently selected. Legacy is a display treatment, not a revival
of an older lighting solver. Main and pause Audio pages share a draggable master
slider and keyboard adjustment, routed through the already verified persistent
Kira master gain. No competing audio volume path was added.

Errors retain their complete message, wrap long path components and page with
reachable dismissal controls. Native inspection includes an 858-character path
at 480×270 logical / 960×540 physical pixels, both pages and dismissal. Other
receipts cover zero maps, missing thumbnails, 64-member browsing to the final
card, paged descriptions, resize, cancellation, Credits and two full menu →
Explore → play → pause/settings → return round trips. Gameplay has zero menu
script modules; paused settings keep the same gameplay world.

Final acceptance ran for **611.004 seconds**, observed beats 1–120, reset at
600.611 seconds from launch, and continued into the next cycle. The 1600×900
recording contains **1,217 frames at a nominal 2 fps / 608.5 encoded seconds**;
it samples the actual uncapped renderer every 500 ms and is not an FPS test.
Chronological dense frame sheets cover the whole cycle, with separate full-size
pickup/putdown, Home and final-reset views. All six actors move; all five carriers
succeed. Receipts record 19 handoffs (including the next cycle), 14 physical
putdowns, no script diagnostics, one module, at most eight physical bodies,
567,360–649,160 sampled VM bytes and zero sampled item penetration. Peak sampled
process RSS over the full trace is 474,120,192 bytes; this is not total GPU/system
memory. No objects or script histories accumulate. The original two recordings
remain alongside the final one.

Final native checks repeat both gameplay round trips and confirm zero retired
menu owners during gameplay and zero retired gameplay owners after returning.
Pause/settings preserve the gameplay clock/world; native focus diagnostics stop
and resume the menu clock. Main and pause Audio show the shared master slider;
restart retains zero, with **260,096 float PCM samples all exactly zero** while
effects/ambience remain 0.8/0.6. Style selection through the UI persists VHS and
retains High quality and Full lighting. The final flat distribution runs from
`/tmp/places-menu-ui-0130-shipping/Places/places`, cwd `/tmp`, with no asset-root
override and zero gameplay maps. Flat/app distributions, compiler and editor are
retained in `local/menu-ui-0.13.0/payload-shipping/`.

The final separate 1,800-frame native cost sample uses actual Immediate
presentation, swap interval 0, 1600×900 physical / 800×450 logical pixels,
High quality/textures, Full lighting/reflections and bloom, with no limiter,
recording or capture. It observes a 1.713 ms mean loop (583.74 frames/s),
0.431 ms update, 1.120 ms CPU-side render submission/wait and 1.394 ms GPU pass
span mean (1.370 ms median, 1.487 ms p95). This is a short tiny-scene sample,
not an engine-wide result or comparison. Earlier otherwise matched samples
plateaued near 120 Hz despite Immediate and are retained as inconclusive;
the presentation change is not attributed to an engine improvement. GPU spans
include scheduling stalls and are not active utilization.

Final gates pass: `cargo fmt --all -- --check`, strict all-target/all-feature
Clippy with `-D warnings`, `cargo test --workspace` (238 passing; 0.44 seconds of
fixture execution, no shipped-map bake), and the locked release workspace build.
The final menu package passes dependency integrity and `verify --require-current`;
its fingerprint is `4b70a6f853575ad79cfdbd2238c47ffff053bc9250b565ae03c4f164dcab4a52`.

Library saving failed at preparation in the required unchanged batch-helper
workflow: **`library upload failed: prepare_uploads failed`**. The one ordered
19-file batch included this stage's ten screenshots, cycle overview and complete
video plus the seven preserved editor/stair files (their one authorized retry).
No confirmed Library IDs were returned; no alternate transfer path or additional
retry was used. The 43,959,967-byte local delivery set, exact request/result/log
and delivery-status JSON remain under the stage evidence directory. This is the
remaining delivery blocker. Other boundaries are the font-atlas reconstruction
rather than a shipped raster logo cutout, nominal 2 fps diagnostic video,
unvalidated Windows/Linux native behavior, and final integrated CI/collection
rebakes reserved for the following queue stage.

All evidence is under `local/menu-ui-0.13.0/`, outside target. Earlier failed
runs, generated cutouts and previous stage packages remain. The two unrelated
untracked Python audit reports retain their exact SHA256 identities. No CI wait,
main merge, release, force push, credential change, ragdoll work or architecture
project #31 was started. Ragdolls remain second to last, followed by documentation
cleanup and final integrated checks. Exact commit, verified remote identity,
binary/package hashes, confirmed Library IDs or delivery blockers and ownership
release are recorded in the final on-disk handoff after publication.

## Articulated mannequin ragdoll stage — 2026-10-21

Completed against menu-stage baseline
`dca83d4b2c52dc07e1b9c33ede22dd158a956a7f` on `0.13.0-staging`. This is the
approved second-to-last stage. Final documentation cleanup, integrated validation
and CI remain the following owner's work. No shipped map, mannequin model/PNG,
menu choreography, material quality or architecture project #31 was changed.

### Engine, ownership and use

The existing mannequin now transfers its current displayed skeleton into the
ordinary Rapier item world: **15 bodies, 14 real constrained joints and 68 kg**.
The inspected GLB has 39 nodes and 37 skin-joint slots; pelvis, spine, head/neck,
upper/lower arms, hands, thighs, shins and feet have physical segments. Fingers
and the head descendant follow their physical parent with their entry local pose.
Mapping uses node identity and the actual skin-slot order. Unsupported hierarchy,
proportions, transforms, scales or skin mappings fail before allocation.

Pelvis/chest boxes and fitted capsules retain full world, stair, prop, item,
door and player-proxy collision. Each skeleton excludes its own internal contacts;
different mannequins collide. Joint translations are constrained, all angular
axes have finite anatomical limits, and an already violated authored entry angle
expands that bound to preserve pose continuity. CCD, friction 0.65, zero bounce,
linear/angular damping 0.35/2, eight additional solver iterations and overdamped
120 Hz joint compliance produce bounded contacts. A full quiet second with the
0.1 m/s normalized Rapier sleep threshold allows resting islands to sleep;
ordinary item contacts wake them. Stronger damping of the constraints resolved
the raised-arm wall fixture's persistent small-hand rotation without reducing
collision or visual quality.

Current/previous displayed bones and actor placement provide bounded inherited
linear/angular velocities, including submillisecond render frames. Physics owns
the pose exclusively after activation: the actual AI agent leaves the active
controller/perception sets, movement/sequences stop, and animation actions cannot
reclaim the bones. Skinning, inverse-transpose normals, lighting, shadows and
reflection draws use the existing character path. Bind-space joint-influence
boxes cache a conservative posed visibility/lighting envelope without another
full vertex-skinning pass. The recent normal cache and draw sharing remain intact.
Interaction bounds and script position track the physical envelope/live pelvis.

Use `/ragdoll floor_actor on 1.5 0 -0.4` and
`/ragdoll floor_actor reset`, an ordinary JSON `ragdoll` binding, or sandboxed
`api.ragdoll(id, enabled, vx?, vy?, vz?)`. Optional additive velocity is finite and
at most 15 m/s. Reset removes bodies/joints/caches and restores a fresh authored
animation clock, placement, routes and controller. Runtime-spawned actors retain
their authored spawn transform too. Reset/reload is explicit; no stand-up or
recovery animation was implemented. Eight ragdolls share the existing 128-body
item/bone budget; duplicate/over-budget requests allocate no partial rig.

A carried item returns to ordinary physics at the current posed socket before
the controller changes. Failed release rolls back the unchanged entry pose.
The native carry trial preserves the exact cushion key `9223372036854775817`,
removes its attachment and retains one ordinary item body, with no hidden carrier
or orphan. Individual actor reset leaves that released item physical; map reload
restores the authored world. One contact-onset sample has 9.25 cm of item/hand
collider overlap inherited from the grip; the capture 0.45 seconds after release
is clear and the actor/item settle without explosion. This is a transient socket
release overlap, not a claim of zero penetration at every solver tick.

### Native acceptance and focused gates

`projects/ragdoll-lab` is a separate ordinary editor project with eight existing
mannequins, routes/AI, a six-step stair, table, lamps, reflective floor patch and
carry cushion. Console, E bindings and its Luau module exercise production paths.
`tools/bench/ragdoll.py` supplies explicit native acceptance outside Cargo/CI.
Source project checks, geometry (zero errors/warnings), package dependencies,
integrity and `verify --require-current` pass. Complete project export/import
round-trips the manifest, level JSON and Luau bytes exactly. The final editor
opens the project cleanly and its Scripts Check passes with `dirty:false`.
Preserved executables need their normal asset-root override when run outside a
distribution; initial missing-asset diagnostics are retained beside corrected runs.

All nine final native runs exit zero on the final locked release player,
SHA-256 `eddc3adc3fdf60be60aa482adc5bab35b1e8b149ea82f1901a76dabeafd70ea6`.
The final lab package SHA-256 is
`8267320450c585f3d2ae98e1aa46e11c119a2ba4bdfe99263b3ddaa6f7785d9b`;
its current fingerprint is
`5bf25fc785b572387b9408fb10dd8499358c129ed451deba8ca8d89e35ac884e`.
Contact/multiple views use diagnostic camera/noclip placement; activation and
ownership changes still use ordinary console, authored bindings and Luau paths.

- Two animated → fall → settle → reset cycles both reach 15/15 sleeping segments,
  with settled joint error 0.32/0.21 mm. Wall, stair and table trials also sleep
  all segments, with settled errors 0.41/0.44/0.96 mm and zero sampled resting
  core penetration. Native screenshots show connected limbs, preserved normals,
  lighting and moving contact/shadow bounds; the reflective floor pass remains on.
- Three eight-mannequin cycles perform 24 activations and 24 resets. Eight duplicate
  requests are refused. Peak ownership is 122 solver bodies (120 bones, one item,
  one player proxy), 112 joints and 312 cached pose nodes. Reset returns to two
  bodies, zero joints/cached nodes and the original two AI agents. The early stress
  capture still has two moving actors; it is not an all-sleep claim. In the separate
  extended timing run all 120 bones sleep by 34.16 seconds from ready, about
  29.1 seconds after activation, and stay asleep through the reset at 40 seconds.
- Physics pause holds identical bone positions and fixed-step count 31; one
  explicit step advances exactly to 32. Resume, Medium/High graphics rebuilds
  and ordinary pause-menu suspension retain the same articulated ownership.
  Actual map unload records zero bodies, colliders, joints, pose caches, script
  modules/VM bytes and retired prepared owners. The fresh menu has zero ragdolls
  and normal cast controllers; reloading the lab restores its authored actors.
- Rigid/unknown targets and nonfinite launch velocity leave baseline resources
  intact. No script diagnostics occur. Full native evidence includes ordinary
  carry/socket release, unsupported commands and the final editor script check.

Final formatting, strict workspace/all-target/all-feature Clippy with
`-D warnings`, `cargo test --workspace`, and
`cargo build --locked --release --workspace --all-features` pass. **245 tests pass**
(238 existing + seven focused regressions; 0.49 seconds fixture execution).
The new tests cover displayed-pose/skin-slot continuity, angular/translation
constraints, contact stability, sleep/contact wake, pause/single-step/retirement,
atomic unsupported/budget rejection, actual AI/reset ownership, conservative
weighted-vertex bounds, table chatter and raised-arm wall settling. The focused
seven execute in 0.26 seconds using the installed rig and analytic room/box/wall
fixtures, with no shipped-map bake or GPU. Asset validation passes 621 assets,
zero warnings; Python syntax and `git diff --check` pass.

### Idle, active and sleeping cost

Separate 45-second native runs contain no screenshots/video and run after the
build/compiler jobs finish. They retain High quality/filtering, Full lightmaps
and reflections, bloom, one actual reflection pass, 800×450 logical / **1600×900
physical drawable**, actual Immediate presentation and V-Sync off. Metal records
`display_sync_enabled:false`; phase averages of 454–576 FPS and sub-refresh loop
times establish uncapped presentation on the 120 Hz display. Each run has zero
missed ready surfaces (23,068 / 23,663 presented gameplay frames). Earlier machine
refresh plateaus remain a known measurement limitation and are not used here.

The following means are CPU wall-stage costs, not CPU utilization. Active windows
are 5.2–8.2 seconds; initial idle is 1–4.8 seconds; sleeping windows require every
bone asleep with zero active bodies, and reset samples start after 40.5 seconds.
GPU spans deduplicate timestamp IDs and include scheduling stalls, not active GPU
utilization. The single/eight cameras differ, so render/FPS columns describe
their respective workloads rather than an isolated scaling experiment.

| Workload / phase | CPU update ms | CPU render ms | Physics ms / executed 120 Hz substep | GPU pass span ms | Frames/s |
| --- | ---: | ---: | ---: | ---: | ---: |
| One / authored idle | 1.016 | 0.853 | no active solver step | 1.207 | 493.1 |
| One / active | 1.034 | 0.830 | 0.136 | 1.202 | 494.5 |
| One / sleeping | 0.769 | 0.889 | 0.027 | 1.147 | 543.9 |
| One / reset | 0.981 | 0.893 | no active solver step | 1.215 | 492.1 |
| Eight / authored idle | 0.999 | 0.933 | no active solver step | 1.253 | 479.2 |
| Eight / active | 1.448 | 0.604 | 0.799 | 1.323 | 453.7 |
| Eight / sleeping | 0.178 | 1.369 | 0.042 | 1.264 | 575.9 |
| Eight / reset | 1.028 | 0.860 | no active solver step | 1.205 | 481.4 |

Idle includes eight authored animated models; one physical actor leaves seven
animated, while eight sleeping actors stop all competing animation/controllers.
Sleeping retains exact joints/poses until reset; there is no collision/lighting
quality reduction. Single/eight peak whole-process RSS is 338.17/346.45 MiB,
including loading, not isolated ragdoll allocation. Sampled live GPU objects stay
bounded (at most 119 buffers, 31 textures and 133 bind groups after ready) across
activation/reset. Metal does not expose complete allocation-byte counters; no
total GPU-memory claim or engine-wide speedup follows from these two tiny-lab runs.

### Evidence, delivery and stage boundary

All raw runs, failed tuning probes, logs, CSVs, settings, old binaries/packages
and videos remain under `local/ragdoll-0.13.0/`, outside `target`. Final receipts
are `gates-validated.json`, `final-campaign-complete.json`, `acceptance-final.json`,
`gpu-resources-final.json`, package checks and SHA-256 manifests. The silent final
clip has **383 native frames at nominal 10 Hz / 38.3 encoded seconds**, 1600×900;
it is visual evidence and supplies no performance measurements. Ten selected
screenshots and the complete editable project export are in `deliverables/`
alongside `Places-0.13.0-Ragdoll-Cycle.mp4`. The delivery manifest lists exact bytes
and hashes. Native acceptance covers this Mac/Metal host; Windows/Linux and final
integrated CI remain unvalidated by this stage.

The current Library skill's unmodified supported helper was called **once** after
reading both prior failure logs. Its ordered 31-file batch preserved the previous
19-file request and appended the final 12 ragdoll artifacts. It exited 1 with
**`library upload failed: prepare_uploads failed`**, empty result and **no confirmed
Library IDs**. No alternate transfer route or further retry was used. Exact
request/result/log and `library-delivery-status.json` preserve the service blocker;
the local files are not claimed as delivered.

The two unrelated untracked Python audit reports retain their original SHA-256
identities. No unique evidence, shared target or previous package was removed.
No CI wait, force push, release, main merge or credential change was performed.
The final on-disk handoff records the exact staging commit, verified remote,
artifact paths, delivery blocker and explicit ownership release; final docs and
integrated validation are the next stage.
