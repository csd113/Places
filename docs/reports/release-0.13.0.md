# Places 0.13.0 implementation and validation report

This is the consolidated report for the approved 0.13.0 queue on
`0.13.0-staging`. Sections retain the evidence and limitations recorded at each
stage; earlier next-stage statements are historical handoffs. Raw local receipts
and direct binaries remain outside `target` in their original stage directories.
No main merge or release is implied by this staging report.

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
`f38c638e7a7ba6fa93136636600b82e0ae4f8937`; the final player SHA-256 is
`b0c07d737012ced1c6cbdf4b2a81cb506e664453a8aa21ce8ef5167c8755b216`.
The commit containing this section binds that binary's source. Baseline and
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
no aggregate decoded shared-model byte budget; process-global warning-key set
and installed-package directory discovery lack aggregate count/byte caps;
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

Format, strict all-target/all-feature workspace Clippy, locked all-feature
workspace release build and all **217 tests** pass. The five added regressions
use tiny scalar/channel/pose/header fixtures; no Cargo test or CI step bakes a
map. Native final-editor/game checks above pass. The two unrelated untracked
Python audit reports, old packages/receipts and `target` are preserved. No CI
wait, force push, main merge, release or credential change occurs in this pass.
The current pass now also includes the user's 27-item checklist, organized into
five focused groups: rendering/GPU reuse/culling/batching; physics/spatial
queries; AI/Luau scheduling/allocation; loading/memory copies/bounded reuse;
then frame pacing/synchronization/scratch/duplicate work and release-PGO checks.
The static-ray, rigid-normal, shared-nav-scratch, eight-sheet staging,
vertex-boundary and menu/cancellation fixes above are validated checkpoint
inputs. Group outcomes will extend this section without duplicating those
fixes. The separate fresh engine sweep remains afterward, followed by mannequin
ragdolls and the other queued stages.
