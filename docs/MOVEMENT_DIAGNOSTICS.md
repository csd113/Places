# Movement diagnostics

`assets/levels/movement_test.json` is the authoring source; compile it with
`places-compile build assets/levels/movement_test.json`. The adjacent
`movement_test.placesmap` is the playable QA map, available in Level Select and
through `PLACES_LEVEL=movement_test`.

## Inspection findings

The controller in `src/game.rs` owns stance, feet, eye easing, grounding and
water/ladder state. Horizontal movement is normalized and collision-swept in
segments no longer than 0.15 m. Vertical motion uses 1/120 s integration with
9.8 m/s² gravity and a roughly 1 m jump apex. The body is a 0.30 m radius,
1.8 m standing / 0.9 m crouched cylinder; eyes ease between 1.6 and 0.8 m above
the feet. Grounded stairs follow a pitch line; airborne landings use real
treads. Floors, ramps, regions and water membership come from `src/level.rs`;
static boxes and oriented live door leaves resolve in `src/collision.rs`.

The initial reproduced defects were: the walking query refused the
last floor's unsupported edge; destination steps used the old floor's head
clearance; and the landing query retained a 0.4 m walking allowance, allowing
support above the descending feet. Walking now loses support over void,
checks the proposed support's headroom and switches the remainder of the
sweep to airborne handling. Landing only accepts crossed support. Authored
worlds have no synthetic off-room Y=0 support. Empty-floor test worlds retain
their legacy plane. Movement constants and controls remain unchanged.

The map reproduced two further defects. A 0.8 m wide, valid maximum-slope
ramp was blocked by 0.4 m of hidden collider backing on each side. Region
rims now have 0.01 m backing; the blocking face stays on the authored boundary
and the existing radius-bounded sweep prevents tunnelling. Both compiler
fingerprints include that width, invalidating cached collision/navigation.
A maximum-riser staircase briefly lost support at its foot because the
pitch-line height includes part of the next riser. Drop classification now
uses the real tread height when feet follow that pitch line, preserving
ordinary prop and ledge falls.

Actual gameplay then exposed a too-high jump target leaving the body inside
its side. Walking rim allowances were active during flight, letting the disc
cross before the feet cleared the real top. Airborne collision now uses the
physical band. When a walkable drop starts with trailing-disc overlap, outward
motion may leave that rim without depenetration; this preserves the demo's
water-entry path without a forward push. Walking/swimming allowances and live
door collision remain unchanged.

The asset validator also rejected valid negative ramp rises used by the
peak/valley station, contrary to the loader and authoring guide. Its basic
shape check now accepts either direction while rejecting nonfinite or
near-zero rise; stair rises remain positive.

Collision queries use the existing spatial index. Its query scratch buffer is
reused (it may grow on a first larger query); the movement changes add no
per-frame collection or allocation. Allocation counts were not measured with a profiler.

## Stations

Coordinates are metres in X/Z. Numbered crate markers show the station name
when aimed at; E toggles their floating label. Pool-tile strips mark aisles.

| Station | Area | Exercise |
| --- | --- | --- |
| 01 Flat | X 2–32, Z 2–20 | 24 m lane with 1 m ticks and perpendicular lines; forward/back/strafe/diagonal distance, long wall sliding, 0.8 m corridor and locked angled panels with oriented collision. |
| 02 Steps | X 34–68, Z 2–20 | Independent 0.10, 0.30, 0.39, 0.40, 0.41 and 0.60 m platforms; 0.35 m riser below a Y=2.0 header reproduces destination headroom. |
| 03 Stairs | X 2–22, Z 24–42 | Shallow/normal/steep, 0.8 m narrow and 3 m wide flights; flush landings, close side walls and a doorway after the last flight. Reverse direction for descent. |
| 04 Ramps | X 26–46, Z 24–42 | Rise/run 0.10, 0.30, 1.0, 1.9 and exact-limit 2.0; flat joins, paired peak/valley and adjacent wall. Over-limit slopes are loader rejection tests, since invalid ramps cannot ship in a playable map. |
| 05 Ledges | X 50–72, Z 24–42 | 0.6, 1.5 and 3 m platforms over real landing floor; stair access to a 1.5 m platform ending at X=72 with genuinely unsupported space beyond. Fall recovery triggers below Y=-10. |
| 06 Jump | X 2–22, Z 46–62 | 0.6, 0.95 and 1.1 m targets, narrow 0.75 m landing, real office desk, wall contact and 2.1 m doorway. |
| 07 Crouch/head | X 26–46, Z 46–62 | 1.85 m standing and 1.05 m crouch-only headers; close corners, forced crouch, opening to full height, 2.05 m jump ceiling and narrow alcove. |
| 08 Doors | X 50–68, Z 46–62 | 0.7/1.2/2 m openings, 0.3 m raised sill, ramp approach, initially closed/open interactive leaves with real frames. |
| 09 Pool | X 2–22, Z 68–84 | 0.2 m shallow and 2.2 m deep water, submerged changes, exit stairs, flat rim, 0.4 m raised exit and non-climbable south wall. |
| 10 Props | X 26–46, Z 68–84 | Desk, chair, crate, narrow water cooler, large vending machine and circular pillar with explicit collision dimensions. |
| 11 Combinations | X 50–68, Z 68–84 | Opposing walls, pillar/wall, a wedge of locked angled panels, stair/header/doorway, ramp/wall and crate at doorway. |
| 12 Sloped ceiling | X 74–84, Z 48–62 | Connected gable room with 1.85 m eaves and 2.85 m ridge for head contact and crouch/stand near slopes. |

WASD moves, arrow keys look, Space jumps/surfaces, C toggles crouch, E interacts
and Escape pauses. There is no sprint. To recover from a blocked test position,
reload the level through the pause menu or restart the capture at a station.

## Repeatable runtime campaign

After a release build and map compilation:

```sh
python3 tools/bench/capture_movement_test.py
python3 tools/bench/capture_movement_test.py --quality low
```

The standard-library harness drives the existing real held-control capture
path through 81 cases. `--case text` selects case names containing that text.
Each run has isolated settings, a PNG, CSV trajectory and log under
`target/movement-qa/captures/{high,low}`, plus `manifest.json`. These are real
SDL3/Metal gameplay captures; deterministic controller tests separately pin
exact grounding, eye/feet consistency and contact bounds. These captures do
not claim a manual OS-keyboard feel pass.

## Verification evidence

Recorded on 2026-10-02 on macOS/SDL3/Metal. Rust commands used
`RUSTUP_TOOLCHAIN=1.98.1`, the project's documented verified toolchain.

### Movement results

All results below refer to the exercised cases, not an exhaustive guarantee
for arbitrary geometry. The 13 source-map regression tests run at 30, 60 and
144 FPS, 100 ms frames and repeating 4/13/41/100/7 ms frames. Every simulated
frame checks finite position/velocity and eye = feet + current eye offset.

| Behavior | Verified result |
| --- | --- |
| Walking | Measured cardinal movement covers 3 m in 1 s at every tested frame pattern; input reversal and tight flat passages remain usable. |
| Strafing/diagonals | Left/right/backward and normalized diagonal movement have the same distance as forward movement. |
| Walls/corners | Long-wall sliding, oriented angled barriers, wedge, pillar/wall and opposing contacts stay bounded; repeated pressed contact does not drift in the deterministic cases. |
| Falling | All ledge heights lose support naturally, land on real floor and settle. The true off-room edge continues below Y=0; the map's explicit recovery trigger resets deep falls. |
| Jumping | Desk and lower/borderline targets are reachable; the 1.1 m target blocks the body outside its face. Forward/back/side, wall and stair-transition captures complete with bounded motion. Existing trajectory tests preserve approximately 1 m apex and 9.8 m/s² gravity. |
| Stairs | All five flights ascend/descend into landings and remain grounded on every tested frame, including the maximum 0.4 m riser and narrow flight. |
| Steps | 0.10/0.30/0.39/0.40 m platforms pass; 0.41/0.60 m platforms block. Standing cannot step beneath the destination header; crouching can. |
| Slopes/ramps | All five slopes up to rise/run 2.0 traverse both ways continuously grounded, including the 0.8 m wide limit ramp. Peak/valley captures pass; the loader rejects an over-limit slope. |
| Crouching | Camera height eases, forced low-header crouch refuses standing, and backing into clear space restores standing height. Rapid stance captures remain finite. |
| Ceilings | Jumping consumes upward velocity at the header without sideways displacement; alcove and sloped-ceiling captures complete. |
| Props | Solid desk/chair/crate/cooler contacts stop at their faces without tunnelling or repeated-contact drift; vending and round-pillar captures pass. Desk-top landing settles at feet Y=0.75. |
| Doorways | Narrow/wide/framed and raised/ramp/stair approaches pass. Closed leaves block and opened leaves allow traversal. The doorway-jump regression bounds each horizontal displacement and sideways motion; the historical blip was not reproduced. |
| Water | Shallow/deep entry, surfacing, submerged stairs, flat and raised exits pass; the high wall refuses climbing. Flat/raised exits remain out of water with consistent eye/feet height. Existing demo water-entry/exit regressions also pass. |

The historical doorway blip, abrupt crouch camera and permanently stuck water
state were not reproduced in the current controller; no speculative fixes or
constant retuning were applied to those behaviors.

### Runtime and map evidence

- `tools/bench/capture_movement_test.py --out target/movement-qa/final-captures`
  and the same command with `--quality low`: 81 cases across all 12 stations
  per profile, **162 captures, 0 capture failures**. PNGs and CSV trajectories
  were inspected/audited; all values were finite and logs confirmed the current
  compiled package, valid controls/settings and no panic or validation error.
  The final runtime identity (manifest SHA-256) is
  `48d60327aaf9b0b268f5bd7ab53492ce3de62f426889b450880ad7e8a77f345d`;
  the package ZIP's byte SHA-256 is
  `9f43945c0fa2e87eecff130e087bcb0b0710830f59ee6b64389e8383be0fabb5`.
- Existing `tools/bench/capture_movement.sh`, selecting
  `kitchen_stove_land|kitchen_stove_leave|pool_walk_in` in High and
  `kitchen_stove_land|pool_walk_in` in Low: **30 + 27 captures, 0 failures**.
  Stove landing/leaving and pool traversal were reviewed against the rebuilt
  demo, with all 57 trajectories finite.
- Model Zoo and Lantern Hollow: compiled-package native boots with brief
  forward/backward input, **2 captures, 0 failures**, finite trajectories and
  confirmed package commits. Their images were reviewed.
- Source geometry check: **0 errors, 0 warnings**. Intent annotations account
  for 21 open internal seams and one intentional off-room fall edge; they do
  not suppress unrelated geometry findings.
- All four bundled maps build, pass `verify --require-current`, and validate
  their `off`, `medium` and `full` package variants. Loader inventory tests
  include Movement Test in normal bundled discovery. Existing packages were
  rebuilt because the shared rim collision width changed.

Evidence is local under `target/movement-qa/`: `final-captures/audit.json`,
per-profile manifests/logs/PNGs/CSVs, station contact sheets,
`normal-map-audit.json`, `normal-map-review.jpg`, `geometry-final.json`,
`demo-runtime-{high,low}.log` and `normal-boots/`.

### Validation commands and totals

All listed final checks passed. Prefix each Cargo command below with
`RUSTUP_TOOLCHAIN=1.98.1` to reproduce this run.

| Command/check | Final result |
| --- | --- |
| `cargo fmt --all --check` | Passed. |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery -D clippy::cargo` | Passed, no warnings. |
| `cargo test --lib game::tests` | 166 passed, 0 failed, 2 existing ignored. |
| `cargo test --lib game::tests::movement_diagnostics` after the final map edit | 13 passed, 0 failed, 0 ignored. |
| `cargo test --workspace --all-features` | 1,912 library + 3 integration = **1,915 passed, 0 failed**; 21 existing ignored library tests. |
| `cargo build --release` | Passed. |
| Asset/catalog/texture/GLB/entity clips/outdoor and Lantern Hollow generator checks from `tools/verify.sh` | Passed; catalog has 247 assets, 134 placeable, 4 themes, 0 warnings. Existing soft texture notices remain. |
| Python showcase/ghost/Lantern Hollow tests | 20 passed. |
| Rust `showcase_audit` and `static_prop_lighting_tests` | 9 + 19 passed; one existing ignored in the latter suite. |
| Explicit ignored `bundled_static_models_fit_medium_and_full_atlas_plans` | 1 passed. |
| Four `places-compile build`, `verify --require-current` and `validate` commands | All passed/current. |
| Python `tests.test_package` | 48 passed, including the signed-ramp validator regression. |
| Python packaging/GLB, tool execution/zoo/metrics/lightmap, geometry repair suites | 16 + 41 + 14 passed. |
| Explicit ignored `render::wgpu::renderer::low_lighting_tests` | 2 passed, 0 ignored; real GPU resources exercised. |
| Python `tests.test_compiled_build` | 10 passed, 0 skipped. |
| Python `tests.test_wgpu_bootstrap` | 26 passed, 0 skipped; real native windows exercised. |
| `git diff --check` | Passed. |

`tools/verify.sh` was executed through the full Rust phase, then stopped because
its Python asset validator rejected the newly authored valid negative ramps.
After repairing that validator, every remaining gate command was run in order
with `set -eu`, without repeating the approximately 29-minute Rust suite.
This is a completed gate across resumed phases, not a claim that the original
single shell invocation exited successfully. Logs: `desktop-gate.log`,
`desktop-assets.log`, `desktop-packages.log`, `clippy-final.log`,
`stations-tests-final.log` and `release-final.log` under `target/movement-qa/`.
The duplicate-catalog and failing-worker messages inside passing Python suites
are deliberate negative-test fixtures.

### Limits

- Real gameplay evidence uses the existing scripted held controls, separately
  from deterministic controller tests. A manual OS-keyboard feel pass was not
  performed.
- Hot-loop allocation behavior was inspected in code; allocation counts were
  not profiled. Recorded whole-game update timing includes startup/background
  work and is not a movement-only benchmark.
- Default Rust 1.99 introduces unrelated pre-existing strict Clippy failures.
  This work passes the project's documented verified 1.98.1 toolchain; neither
  repository lint policy nor the machine's default toolchain was changed.

### Changed files

- Map source/package: `assets/levels/movement_test.json`,
  `assets/levels/movement_test.placesmap`.
- Recompiled collision packages: `assets/levels/places_demo.placesmap`,
  `assets/levels/model_zoo.placesmap`, `assets/levels/lantern_hollow.placesmap`.
- Movement/collision/compiler: `src/game.rs`, `src/collision.rs`,
  `src/level.rs`, `src/compiler.rs`.
- Rust regressions/discovery: `src/game/tests.rs`,
  `src/game/tests/movement_diagnostics.rs`, `src/loader/tests.rs`.
- Validation/capture: `tools/assets/validate.py`, `tests/test_package.py`,
  `tools/verify.sh`, `tools/bench/capture_movement_test.py`.
- Documentation: `docs/MOVEMENT_DIAGNOSTICS.md`, `docs/MAP_AUTHORING_GUIDE.md`,
  `docs/VERIFICATION.md`.

No dependencies, art assets, branches or commits were added. Temporary review
scripts and runtime evidence remain outside the tracked source tree.
