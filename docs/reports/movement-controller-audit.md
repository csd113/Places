# Places movement controller audit

Historical acceptance: 2026-10-03 controller audit. Counts, timings and validation below describe
that tested version; the canonical guides govern current contracts.

2026-10-03. Canonical `working` checkout, starting at `540d111920cea71a43ac98f3e3ae175ab28a99bd`. The released lighting baseline was recorded separately as `665bfae2f665d912784d6802b8bd404d81dd6c11`. The previous lighting owner explicitly released the checkout, native jobs, and shared targets before implementation. Lighting/rendering/assets were preserved; production map geometry was not changed. The changes repair the existing upright-cylinder controller without a physics dependency or map/prop special cases.

## Root causes

**The Pit balcony drop.** Room ceilings were selected by X/Z and authored room order, without considering the player's live feet. Entering the footprint of a lower balcony selected its ceiling even while the player was above it. The vertical solver unconditionally assigned `feet = ceiling - body_height - tolerance`, including during descent; the following landing query then restored a prop/support top. This combined downward correction bypassed gravity. The actual Pit regression dropped about 0.537 m in one 60 Hz deterministic frame. The matched native before run dropped 0.4993 m between five-frame samples near the jump apex. No pin or balcony collider was changed.

**Low-ceiling penetration.** Room ceiling lookup sampled the centre, so the cylinder's edge could enter an adjoining low ceiling before its centre did. Sideways entry into a ceiling/raised floor was also missing a physical barrier. A discrete endpoint test could miss a very shallow ceiling crossing around the apex when both interval endpoints were clear. The final solver queries the full disc and solves the first upward impact, including an apex between endpoints.

**Wall/corner penetration and tunnelling.** Horizontal movement relied on endpoint overlap checks and successive local pushes. A thin obstacle could lie between the endpoints; resolving one contact could introduce another. A walking rim's step allowance also admitted physical overlap before support resolution. The repair casts the full displacement and resweeps each tangent segment, with physical side bands and a separate validated step path.

**Incorrect support and step lifecycle.** Jump launch followed horizontal movement, allowing the first jump frame to use the walking step rules. Grounded state could survive without a current supporting contact; spawn initialization inferred support from a floor below the X/Z point. Landing used the centre, missing prop and platform edges touched by the cylinder. Legitimate box steps could remain blocked. Ground state now depends on current support beneath the feet, and jump launch precedes horizontal resolution.

**Adversarial roof/spawn failures.** Roof-edge landing still needed footprint support. A gabled ceiling can cross the feet while its lowest point is already below them; that overlap must be detected independently of an overhead query. Both are protected by additional regressions. Invalid spawn recovery is separate, bounded, deterministic, and selects the nearest validated candidate.

## Controller architecture and invariants

The player remains a 0.30 m radius upright cylinder, 1.8 m standing height / 0.9 m crouched height. Feet are the integrated physical coordinate; eye animation is an offset. Default walking speed remains 3 m/s, jump launch 4.427189 m/s / intended apex 1 m, and downward gravity 9.8 m/s². WASD, arrow-key looking, Escape pause, crouch, water/ladder behavior, and no sprint are retained.

Input, looking, stance, and doors are updated once per frame. Land locomotion shares horizontal/vertical intervals of at most 1/120 s, bounded to twelve intervals under the existing 0.1 s simulation-delta cap. Input edges are consumed once. Gravity uses the actual interval duration and the exact constant-acceleration displacement; fractional 144 Hz time is not deferred in a vertical accumulator. Water keeps its existing separate model.

Horizontal collision uses exact rounded-rectangle casts against boxes, local-space casts against oriented doors, and the existing spatial grid over the entire requested segment. Up to eight contacts project only incoming movement onto each tangent and resweep. Unconsumed motion is discarded at the iteration limit. Floor boundaries and ceiling patches block sideways entry into their live body band. No large margins, thicker map colliders, scene-wide iterative escape search, or new dependency were introduced.

Vertical contact selects physical surfaces by height and full footprint across stacked rooms. Descending feet land on the highest crossed support, including props, room edges, and roof edges. Upward impact solves time of contact, consumes the incoming vertical component, and applies gravity for the remaining interval. Lateral motion remains available. Walls and ceilings cannot establish grounded state.

A step requires current grounding, a reachable top at or below 0.4 m plus the existing step tolerance, head clearance, and a successful elevated sweep to the destination. Airborne side contact cannot invoke it. Drops beyond the intended step-down allowance lose support and fall. Spawn/reset recovery validates complete-body clearance, is limited to 3.6 m, and is never used as an ordinary movement escape teleport. Small horizontal recovery is separately bounded by the radius and rejects residual overlap.

| Numerical quantity | Purpose |
| --- | --- |
| `CONTACT_EPS = 0.0001 m` | Documented contact/nonpenetration tolerance; excludes tangent support/head bands. |
| `STEP_EPS = 0.001 m` | Existing discontinuity/step equality tolerance; not used as landing penetration allowance. |
| Sweep skin `0.00002 m` | Roundoff protection before a hit; enlarged only to one representable f32 coordinate increment at distant coordinates. |
| Motion squared `1e-12 m²` | Negligible displacement termination. |
| Grazing `1e-8 m/sweep` | Incoming-direction/contact-time numerical classification. |

`PLACES_MOVEMENT_DEBUG=1` enables frame input/body/position/velocity state, requested movement, box identities/bounds, sweep fractions/normals, step reasons, depenetration, ceiling impact time, support classification, grounded transitions, and final state. Ordinary gameplay does not emit these traces. Sweep fractions are normalized over the requested segment. No visual overlay was added.

## Reproductions and native behavior

The user's “pin” was clarified as the stacked balconies. The deterministic regression uses the maintained Pit fixture and the actual north balcony route: eye `(50.2, 1.6, -29.2)`, yaw 180°, jump at 0–0.06 s, forward at 0–0.58 s. The old controller failed the per-frame drop bound; the repaired controller resolves contact and falls under gravity onto the real available surface.

The low-ceiling edge fixture starts at X=-0.15 m beside a room whose ceiling is 2.05 m. Native before/after runs use the same source, standing body, and ordinary held jump input. Before: maximum recorded head 2.7313 m, penetrating that ceiling by 0.6813 m. After: maximum recorded head 2.0493 m, then a natural return to the floor. Focused tests additionally verify upward velocity removal, subsequent falling, and lateral preservation.

Native validation used real SDL/Metal on an arm64 Mac, macOS 27.0.1. The complete route pass covers 82 cases: cardinal/diagonal movement, wall slide, narrow lane, steps, headers, stairs, ramps, ledges, prop jumps/landings, crouch transitions, doors, pools/exits, wedges, sloped ceiling, and The Pit. All saved maps also opened and produced PNG/player trajectories with the final native build.

Manual pre-repair checks exercised jump, crouch, arrow-key look, and Escape pause in The Pit. The UI automation supports key taps but no held-key duration, and its connection later became unavailable while the Mac stayed online. The exact before/after route uses the engine's existing held-input script through its ordinary input state on real hardware. A complete manually held-key before/after Pit traverse is therefore not claimed.

## Regression coverage

Twenty-two new controller regressions and three low-level cast regressions cover:

- stacked-storey ceiling reference and actual Pit balcony drop;
- whole-disc prop/room/roof edge landings and ceiling-edge clearance;
- real support at airborne spawn, current ledge departure, and ballistic falling;
- low/near-apex ceilings, stop of upward velocity, lateral preservation, and fall afterward;
- apex crossing between two clear endpoints;
- airborne wall slide with no false ground or upward step;
- repeated floor/two-wall/ceiling corner contacts without new penetration;
- thin floor/overhead slab at high vertical speed;
- sideways entry into low ceilings and raised room floors;
- maximum valid steps, above-limit obstacles, and insufficient headroom;
- narrow passages, crouch, refused standing, and stable repeated jump cycles;
- physical floor-region rims and roofs approached from above;
- bounded deterministic box and gabled-roof spawn recovery;
- a 50 m cast through a 0.1 mm wall, independent corner planes, and a 45° thin door.

The existing movement diagnostics also exercise supported ramps/stair joins, rejected above-limit slopes, pool transitions/exits, doors, props, measured speed, and irregular frames. Four old fixtures were corrected to start on their actual authored floor or clear of a rail; their behavioral assertions remain. Ceiling tests observe an actual contact event rather than requiring the player to remain frozen at impact until the next render frame.

## Timestep results

The same invariant fixtures run at 30, 60, 120, and 144 Hz and a repeating mixed pattern containing 0.1 s frames. A fall starts at feet Y=8 m and runs for exactly one second; the expected result is Y=3.1 m and velocity=-9.8 m/s. Jump values below are sampled frame maxima, so 30 Hz slightly undersamples the apex.

| Pattern | Feet after 1 s (m) | Vertical velocity (m/s) | Sampled jump apex (m) |
| --- | ---: | ---: | ---: |
| 30 Hz | 3.1000054 | -9.799994 | 0.99891037 |
| 60 Hz | 3.0999985 | -9.800001 | 0.99998504 |
| 120 Hz | 3.0999954 | -9.800004 | 0.99998504 |
| 144 Hz | 3.0999815 | -9.800013 | 0.99999857 |
| Mixed/0.1 s | 3.1000023 | -9.799996 | 0.9999716 |

Fall position spread is below 0.03 mm; the largest sampled apex error is about 1.1 mm. The matrices also assert finite state and per-frame solid nonpenetration. Ceilings, corners, repeated jumps, rims, and floating floors run over the same patterns.

## Performance

An opt-in release benchmark runs seven 20,000-frame samples through the real controller, with an ordinary fixture and 4,000 additional off-lane colliders. Before medians were 0.1647375 µs/frame and 0.1770083 µs/frame. Final medians are 0.70056875 µs/frame (ordinary) and 0.6964333 µs/frame (dense), increases of 0.53583125 µs and 0.519425 µs. The larger increase occupies about 0.0032% of a 16.7 ms frame. The measured increase is reported rather than described as noise; its absolute cost is far below meaningful frame-time impact. The swept grid query remains allocation-free and iteration counts are bounded.

## Validation and evidence

Commands ran with the pinned repository toolchain and `RUSTC_WRAPPER=`; offline mode avoided unnecessary network work. Native GPU/window-dependent checks ran outside the restricted filesystem execution context. An initial restricted package test had no Metal adapter; the exact test passed with native access, and the complete native gate passed afterward.

| Command | Result |
| --- | --- |
| `cargo fmt --all --check` | Exit 0. |
| `cargo check --workspace --all-targets --all-features` | Exit 0. |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | Exit 0. |
| Strict policy with additional `-D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo` | Exit 0. |
| `cargo test --lib game::tests -- --nocapture` | 188 passed, 0 failed, 3 opt-in ignored. |
| `cargo test --workspace --all-features` on final source | 1,956 library tests + 3 integration tests passed; 0 failed; 22 intentionally ignored. Library runtime 892.44 s. |
| `cargo build --release` | Exit 0; runnable build preserved outside target. |
| Asset/prop/Halloween/clip/generated-map checks | Exit 0; asset audit reports zero errors. |
| `sh tools/verify.sh` | Exit 0, including full workspace tests, strict lint policy, release build, Python suites, atlas budget opt-in test, package currentness/decode checks, and real native SDL/wgpu bootstrap. |
| Movement native capture harness | 82 final-build SDL/Metal captures, 0 failed. |
| Saved debug-map build/currentness/decode/native load | 36 maps; all currentness/decode checks passed; all 36 opened with the saved build, saved assets, and local SDL, with screenshots and trajectories. |
| `git diff --check` | Exit 0 in the native gate; read-only check also passed with LFS clean filtering bypassed. |

Passing negative-fixture tests intentionally print malformed asset/worker errors; these are not unexplained validation failures. The four shipped packages remain current. Lighting files were hash-checked against the preserved baseline: only `src/level.rs` (controller surface queries) and the small movement clarification in `docs/MAP_AUTHORING_GUIDE.md` differ; the other 275 files matched exactly before publication. No lighting implementation or asset content changed.

## Remaining limits

The engine uses boxes, oriented doors, segmented curved colliders, and analytic floor/ceiling heightfields. It does not expose a general triangle-mesh player collider; rotated prop collision remains the authored conservative AABB contract. Triangle seam behavior therefore cannot be presented as a repaired mesh solver. The existing supported ramp slope limit and stair pitch-line walking convention are preserved; invalid above-limit ramp authoring remains rejected instead of adding arbitrary steep mesh surfaces to this controller.

Malformed fully enclosed spawn geometry with no clear candidate within 3.6 m reports a warning instead of an unbounded escape. The existing 0.1 s simulation-time cap remains. f32 world-coordinate precision limits are handled by a representable horizontal skin; they are not a claim of arbitrary-coordinate exact arithmetic. The manual held-key limitation is described above. No production map/prop special cases or unresolved failures in the corrected regression scenarios remain.

## Native reproduction

```sh
python3 debug-maps/movement-audit-20261003/launch.py
```

The saved collection contains 36 authoring/package controls with matching assets,
player and SDL. Native trajectories sample every five rendered frames; frame
rates vary, so those positions are not uniform-time numerical trajectories.
Exclude initial menu/boot zero-coordinate rows. Standing head height is eye Y
plus 0.2 m. Deterministic timestep regressions supply the quantitative movement
invariants independently of the recorded images.

| Pit stacked-balcony reproduction | Before repair | After repair |
| --- | --- | --- |
| Native route endpoints; differing player heights are intentional | [Before](../images/reports/movement-controller-audit/native-before/pit_stacked_balcony.png) | [After](../images/reports/movement-controller-audit/native-after/pit_stacked_balcony.png) |
