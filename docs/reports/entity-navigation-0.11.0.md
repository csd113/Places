# Entity updates and navigation correctness — 0.11.0 staging

Final engine pass based on `bd81c9f5a1e02e7341a1c16e76f900a92484df03` on
the user's Mac. Scope is entity lifecycle/scheduling, navigation, animation
transitions and avoidable CPU work. The completed renderer, shader, physics,
audio, loading and settings audits were not repeated.

## Demonstrated defects and corrections

- **Sequence ownership.** Taking all runs out of the world while executing
  actions hid them from cancellation and discarded newly started runs when the
  retained collection was restored. Runs now stay authoritative in the world;
  a monotonic revision distinguishes even a same-sequence replacement. Only
  runs present at tick start execute, so new starts retain a consistent next-tick
  boundary. Cancellation clears its controller without emitting completion.
- **Sequence publication.** Completion omitted its documented sequence-id key;
  sequence actions dropped reset and other report fields; move/face steps did
  not dirty frames, and dynamic turns omitted renderer commands. These now use
  the existing event/command contracts. Reset ends the tick. Successful sequence
  transforms also update AI pose, so perception, renderer frames and resumed
  locomotion do not jump back to an older position.
- **Stopped time and release.** Zero-time AI ticks could plan and consume
  waypoints, and the mover treated zero travel as arrival. Simulation phases
  now require positive finite time while queued player actions can still
  dispatch. Agents leave `Scripted` when sequence ownership ends. Flashlight
  holds retain the authored state, route and timers and continue facing the player.
- **Navigation topology.** Nearest-point search returned the first occupied
  Chebyshev ring even when a later axial cell was closer. Searches compare
  candidates until a geometric bound proves the result. A* diagonal corners
  and shortcuts now share live door passability; shortcuts retain home bounds
  and the baked step/slope rule instead of accepting a flat cliff as a slope.
- **Pose transitions.** The first explicit cue ignored its saved source pose,
  and interrupted fades sampled an old clip instead of the displayed blend.
  Transitions now retain the displayed node/morph pose. Paused, completed and
  settled scrub cues retain their pose revision, with explicit restart preserved.
  A route that reaches its endpoint publishes completion in that update.

## Maintainability and bounded CPU work

`engine/entities/sequence_runtime.rs` owns the sequence dispatcher and its
movement primitives. It copies the active step rather than the full authored
definition each tick and resolves the definition index once per run. Existing
64-live-run/64-step budgets and authored wait rounding remain intact.

Trapped prey now respect the existing 0.75-second retry cadence. Perception
uses eight stable phases, hidden agents skip sight work, and sight checks reuse
the world's door collider slice. Settled flashlight holds publish no redundant
transform; cue changes still dirty the renderer handoff. Named clip lookup no
longer allocates a lowercase string on every lookup. Completed routes retain
their handoff, and fades avoid a duplicate pre-update frame rebuild.

The deterministic fixture demonstrates fourteen successive 50 ms trapped-prey
ticks with zero path queries, followed by a permitted retry. Only one of eight
agents checks perception in the initial 10 ms phase. Held-pose regressions verify
unchanged revision across repeated updates. These are bounded work reductions,
not measured FPS improvements; distant agents keep their authored autonomous
behavior rather than acquiring a new distance-based sleep policy.

## Regression value and evidence

Thirteen focused regressions live in the owning `src/engine/tests/` tree: seven
sequence lifecycle/publication/handoff checks, three stopped-time/retry/topology
checks and three route/pose checks. Fixtures contain at most 25 navigation
cells, eight agents, two metadata props or one synthetic node with three
two-key clips. They load no shipped assets, bake no map and need no GPU/device.
The first twelve all fail against the preserved pre-pass implementation while
the previous engine checks pass. The additional handoff regression protects
the integration defect found during review. The final workspace passes all
67 CPU tests in 0.08 seconds, with zero failures or ignores.

`tools/bench/entity_navigation.py` is explicit direct-executable acceptance
tooling, outside ordinary CI. It uses existing Office/Home packages and retains
isolated settings, action scripts, snapshots, PNGs and native logs. All receipts,
baseline failures, executable hashes, final exact-commit CI/remote verification
and cleanup/custody records are preserved in `local/entity-navigation-0.11.0/`.

The complete `RUSTC_WRAPPER= sh tools/check-rust.sh` gate passes: formatting,
all-target/all-feature check, strict debug and release Clippy, workspace tests
and the locked all-feature production build. The four production executables
are preserved under `bin/` with SHA-256 receipts; the player hash is
`f22b267f90e7848474754e156baed5a993a94a60681c614b1bca3be22eef3b98`.
Python source contracts pass 372 tests with 17 explicit skips in 94.129 seconds.

Three successful direct native processes pass 31 assertions across 77 actions
and 19 captures. Snapshots confirm mannequin freeze/look/rest, its retained
moving clip, stopped-time player movement with unchanged agents, pause/resume,
physical roach refuge arrival, hidden countdown, moving visible return and a
fresh Office roster after Home/Office reload. The final roach debug sample is
idle near its original position, with no stuck time. Moving/resting mannequin,
retreating roach and both reloaded environments were visually inspected.

Native runs use Metal/Fifo at 1280 × 720 physical pixels, Full lighting and High
textures. Four transient ready-surface skips are recorded across the successful
clock/shelter processes; reload has none. Initial attempts lost OS focus, correctly
freezing simulation and making wall-time acceptance deadlines inconclusive.
Those traces remain preserved; explicit focus restoration completed the intended
simulation intervals. These are functional checks, not uncapped throughput
measurements or a new focus-system audit, and no FPS improvement is claimed.

## Stage boundary and limits

No map, texture, model, package format or compiler behavior was changed or baked.
Office's arms-forward moving/arms-down resting mannequin, flashlight hold/look
and roach physical shelter/hidden/return behavior remain authored as before.
Existing wait/substep rounding and same-clip replay identity are unchanged;
this pass does not promise network determinism or cross-platform native coverage.
No matching open issue covers this pass; future roadmap issues remain open.
No main merge, release, branch transition, editor, Luau, packaging overhaul or
0.12.0 work is part of this stage.
