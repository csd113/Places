# Pool water — 0.10.0 staging

Pool now contains physical swimming water, a floating rubber duck and a sinking
metal drum. Authored items remain dormant until pickup, impact or the demo's
Interact control activates them. Player entry preserves the existing **1.8 m
standing body / 1.6 m eye height**, with **9.8 m/s² downward gravity** shared by
player and items. No automatic crouch or sprint was introduced.

## Verified starting condition and authored correction

The preserved `2e4174735262754ccce4a978255950f702daae2e` source/package and native
player confirmed the suspicion: the main basin had recessed collision geometry
but **no water descriptor**; only the hot tub had water. The original spawn was
inside the tub. The baseline includes an honest initial capture and a repeated
fixed-camera view for comparison.

`levels/pool_showcase.json` and its idempotent `tools/levels/refine_pool.py`
generator now author a 9 × 4.5 m main volume: floor −1.5 m, surface −0.15 m,
depth 1.35 m. The adjacent 4 × 0.9 m shelf has floor −0.35 m and depth 0.20 m.
The original hot tub remains. Spawn is on clear dry deck `(3, 9)`, yaw 56°.
Existing water PNG/material, transmission, reflections, lights and probes are
reused. Coping, overflow grates and two correctly proportioned submerged lane
decals provide focused polish; no new image/model assets or light-strength
compensation were needed.

The existing east ladder has an authored climb volume plus eight item-only
rail/tread colliders. Native pickup exposed a pre-existing guardrail problem:
west pieces had already-rotated dimensions that the resolver rotated again.
Their local long axis now matches their actual models, freeing the dry route.

![Preserved native Pool baseline](../images/water-0.10.0/pool-before.png)
![Matched native Pool with physical water](../images/water-0.10.0/pool-after.png)

## Physics and lighting contract

Rapier supplies contacts, sleeping, CCD and external impulses; Places supplies
the immersion and buoyancy model beside it. This follows Rapier's documented
[forces and impulses API](https://rapier.rs/docs/user_guides/rust/rigid_body_forces_and_impulses/).
There is no fluid solver, water current or scripted bobbing/waterline clamp.

`src/physics/water.rs` clips posed convex solids against enabled swimming
rectangles/circular chords, finite bottoms and the existing last-authored volume
priority. Distributed 6 × 6 columns produce displaced volume and its buoyancy
centre; off-centre lift produces rotation. Fully immersed solids use cached
volume/centroid. Upright cuboids and cylinders use exact vertical slices,
preserving their analytic centre to avoid numerical torque away from the origin.
Compounds sum their parts. More than sixteen competing water regions use a
bounded whole-body sampling fallback. Each fixed step allocates no new water
sampling buffers; dry maps allocate no water response state.

Lift is `1000 kg/m³ × 9.8 m/s² × submerged volume`. Exponential linear/angular
drag scales with displaced-water-mass/body-mass ratio, yielding useful damping
at floating equilibrium. The strict optional `physics.water` profile authors
source displaced cubic metres (scaled cubically) and bounded drag coefficients;
omission uses solid collider displacement. Source/scaled density and collider
stiffness bounds reject excessive lift. No behavior branches on an asset name.

The reusable duck profile describes a hollow float. Pool's 2.5× duck has 1 kg
mass and 10.156 L full displacement; it settles near 9.8% immersion. The open
metal drum has 6.4 kg mass and 0.8 L displacement, so it sinks. The ordinary
demo control toggles activate/reset through existing generic group actions.
Reset restores authored poses, zero motion and dormancy. Held/suspended/dormant
bodies receive no water impulses. Water enable changes wake active sleepers.

Native lighting exposed another existing defect: strict room lookup rejected
valid air below a nominal floor, despite prepared recessed probes. The indexed
compiled floor interface now supplies the local room-floor offset. A tiny
serialized two-storey regression verifies prepared/spatial/direct lighting,
floor/ceiling occlusion and below-floor rejection. No package-format change or
brightness workaround was introduced. Texture quality and Static/Full lighting
remain independent.

![Native standing-height waterline and reachable duck](../images/water-0.10.0/floating-duck.png)

## Validation and retained delivery

Final validation results and measured performance are recorded below before
delivery. Explicit native campaigns are outside default tests and CI; only Pool
was compiled for this stage. Existing Arcade packages and model bytes were used
unchanged for the shared-engine regression.

The five deterministic water tests use tiny bodies and take 0.01 s. They protect
strict/scaled metadata, partial rectangle/circle/bottom immersion and torque,
float/sink equilibrium, dormancy/toggle/reset and millimetre stiffness stability.
The existing immersion fixture additionally reproduces off-origin centroid
round-trip torque before the correction. One small Python table verifies the
matching authoring validation; existing Pool coverage now checks actual ladder
and guardrail collision contracts.

Formatting, workspace target/feature check, strict debug/release Clippy, release
build and default/all-feature workspace tests passed: **2,316 / 2,322 total
tests**, including 2,307 / 2,313 library tests; nineteen existing opt-in GPU tests
remain ignored. Clippy also enforced all/pedantic/nursery/cargo warnings. Source
Python tests passed 355 tests with four skips using `PLACES_SKIP_SMOKE=1`; the
asset validator reports 571 assets / 348 placeable / eight themes, zero warnings.
Texture/prop checks and the unchanged Arcade generator passed; Pool regeneration
is byte-idempotent. Commands and complete results remain in the local gate logs.

Eight final native Pool action cases passed **325 checks**: dormant activation
and reset; buoyancy/partial submersion/settling; wet/dry pickup/drop/throw;
High/Medium/Low textures with independent Full/Static/fallback lighting; standing
entry, explicit crouch/swim, surface and shelf exit; pair/wall contacts including
20 m/s stress; eighteen real physical bodies; and the recorded interaction.
No player reset or prop escape recovery occurred. The final stress run reached
seventeen simultaneous active bodies, then eighteen sleeping bodies, with at
most two substeps and no clamped simulation time. Existing Arcade bowling and
the tiny contact campaign passed another **944 / 334 checks**, with unchanged
packages and zero recoveries. Matched baseline/hero capture provenance also
passed; native PNGs and decoded video entry/exit/ownership frames were inspected.

Two alternating pairs per workload used the same compatible Pool package,
preserved visual assets/gameplay defaults, High/Full, a fixed west camera and
1280 × 720 drawable. Each measured 600 frames after 120 warmup frames (600 for
eighteen bodies), with capture, video and per-frame body tracing disabled.
Every measured frame retained the requested activated body count and a closed
console. The old strict parser cannot read new displacement overrides, so this
shared fixture omits them and measures default solid displacement; the actual
authored float/sink profiles are covered by the separate acceptance/stress runs.
Values below are medians of per-run statistics, in milliseconds:

| Bodies / metric | Prior physics median / p95 | Water median / p95 |
| --- | ---: | ---: |
| 2 / frame loop | 8.284 / 9.576 | 8.245 / 10.226 |
| 2 / CPU update | 0.265 / 0.443 | 0.342 / 1.082 |
| 2 / unique GPU elapsed span | 1.001 / 1.241 | 1.394 / 1.781 |
| 2 / physics step | 0.0049 / 0.0188 | 0.0051 / 0.0296 |
| 18 / frame loop | 8.299 / 8.983 | 8.296 / 9.025 |
| 18 / CPU update | 1.736 / 2.080 | 1.297 / 6.146 |
| 18 / unique GPU elapsed span | 1.803 / 2.290 | 1.553 / 2.013 |
| 18 / physics step | 0.0229 / 0.0291 | 0.0232 / 0.0313 |

Draw calls stayed 44 / 60 for the two/eighteen-body views. Median peak native
RSS was 275.6→275.3 / 275.7→276.1 MiB. Frame cadence remains near 120 Hz; the
eighteen-body CPU-update tail increased and is reported above. Different solved
poses and lighting work prevent attributing renderer timing changes solely to
the water force code, and short runs do not establish a speed improvement.
GPU spans are elapsed hardware samples, not utilization. Actual authored-water
stress with eighteen bodies present and active substeps measured **0.027 / 0.049
/ 0.330 ms median / p95 / worst** over 1,121 samples, including diagnostic trace
and capture overhead; peak RSS was 273.4 MiB.

The genuine clip is **600 consecutive samples of native rendered frames, 20 s,
1280 × 720, H.264/YUV420P, 30 fps, BT.709**. It samples every second ready frame
from a 60 Hz recording pace, without interpolated/synthesized images. Source
frames 1–1199 show float/sink, standing entry/exit and normal pickup/throw, with
diagnostic camera setup disclosed in the receipt. SHA-256:
`0221630e88880aa10be2b6b9767893b7a45f4d87df22144a004af91f28ce66cc`.

Confirmed Library identities (version 0, retain these identities for edits):

| Artifact | Library file ID |
| --- | --- |
| Baseline PNG | `libfile_85c63e286d08819187e0774df5672b8f` |
| Updated Pool PNG | `libfile_a74cf9be7c608191825f291d5164c8a4` |
| Duck waterline PNG | `libfile_ed0beed8e9c881918608532c355cf4fd` |
| Genuine MP4 | `libfile_033cc1c499708191bfe5c3509cdab155` |

`library-results.json` records exact local bytes, hashes and confirmed identities;
returned version attributes were applied and verified on the original files.
Consumers must materialize those Library files to their own executor destination
and verify hashes there; a cloud workspace path is not a Mac path.

Runnable binaries, current Pool package, isolated settings, curated receipts and
the genuine clip are retained in `local/water-0.10.0/` outside Cargo's target.
Launch `local/water-0.10.0/run-pool.sh`; aim at the east-deck table control and
press Interact to activate/reset the float/sink demonstration. Final Git/remote
SHA, CI identity, artifact hashes and custody release live in its `handoff.json`.

## Limits

This is a bounded rigid-body approximation: no displaced surface waves, fluid
exchange through hollow meshes or caustics. Default cups use economical solid
cylinders. Native acceptance was on this Mac's Metal renderer with real input
dispatch plus explicitly labelled diagnostic setup poses/velocities. Other OS
and browser gameplay are unverified. Audio, Pool caustics and separate Arcade
model delivery/integration remain subsequent work. The previously reported
intermittent native crash is not resolved or claimed reproduced here.
