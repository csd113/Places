# Predator lost-sight search / macOS acceptance

Baseline predators now investigate an observed position for a bounded period,
then search other reachable areas. Previously the fixed last-known-position
investigation fell back to post/idle behavior without an outward search policy,
which could leave a pursuer near a closed elevator.

## Resulting behavior and authoring

`search_after_seconds` defaults to **8 seconds**, finite range 0.25–120.
`search_radius` defaults to **12 metres per leg**, finite range 2–128. Both appear
as named fields in the editor's AI component form. Existing source files use
these defaults without a migration. They apply to baseline `predator` behavior;
authored tasks, scripts, sequences and their priority/ownership remain intact.

Fresh visual contact starts/resumes pursuit and records the actually perceived
position. Losing sight investigates that remembered position until the deadline,
independent of whether the actor can reach it. The baseline no longer queues an
internal last-known-position task. It then chooses outward walking goals that
are at least 2 m from the remembered location and 1.5 m from its current position.
Each leg stays within the configured radius, honors body-inset `home_bounds`,
and requires a complete bounded navigation path. A failed live route is
abandoned; a location with no alternate route waits and retries through the
existing planning cadence/budget. Too-small home bounds are handled safely.
There is no teleport, global map scan or lookup of a hidden target's live pose.

Fresh sound remains useful information and can prompt up to two seconds of
investigation. One retained stimulus cannot continuously restart the timer.
Hearing never updates visual-sighting memory; ongoing fresh noise may keep
attracting attention. New sight resumes pursuit on the ordinary 0.2 s perception
cadence. Search pauses briefly between completed legs, rather than camping at
the lost-target location. This is bounded roaming, not an exhaustive map sweep.

Observed search position and deadline survive checkpoints and ordinary NPC
cabin transport, with the position transformed into the receiving cabin frame.
Optional serialized memory preserves compatibility with older checkpoints and
is validated before restoration. Search telemetry is emitted only by the
explicit developer snapshot path.

## Validation

Tested source base: **9a771f1b**, retaining the hub, shared world-audio and manual
elevator-control work. The separate lighting task's unpublished renderer changes
were not modified or incorporated. All builds use isolated output under
`local/predator-search/target`; accepted direct executables are retained in
`local/predator-search/production`. No Mac app bundle is used.

- All **444 workspace tests** pass. Three focused new regressions cover the
  lost-sight deadline and complete-route admission, hidden-position privacy,
  fresh sound/sight, trapped/narrow areas, transported memory, and named editor
  settings saving/reopening with atomic rejection of invalid values. Synthetic
  navigation uses at most 81 cells and real matching floor support, no assets,
  GPU or map bake. The whole test runtime is approximately 1.1 seconds.
- Strict workspace/all-target/all-feature Clippy, locked workspace/all-feature
  release build, formatting, Python compilation and diff whitespace checks pass.
- Actual direct-binary elevator hiding passes with default 8 s and configured
  3 s delays. Native assertions verify those deadlines against the last visual
  sighting and distinguish the earlier search stage in the shorter-delay case.
- In the final lit demo, visual contact stops near actor time 4.203 s. The
  investigation deadline is 12.203 s. With doors still closed, the actor walks
  to approximately (-1.83, 0, -2.60), and after save/resume to (-0.99, 0, -3.68).
  The player remains at (0.28, 1.60, 0.73) on Ground. Closed-door sight remains
  blocked; the saved deadline is retained. Reopening and leaving the cabin
  restores real visual contact and pursuit.
- Ordinary non-elevator pursuit loses sight behind a wall, retains observed
  memory and starts outward search. A disconnected-room case installs no
  impossible goal, does not cross walls or teleport, and retries within bounds.
- The held-crate/NPC campaign passes boarding, local and both cross-level trips,
  local/foreign checkpoint resume, custom state, outside-object exclusion and
  single-instance return. The empty three-stop door-control campaign also passes
  reversal, closure, save/resume, obstruction, canceled departure, local travel,
  Annex door operation and return.

The preliminary navigation test lacked a physical floor matching its synthetic
grid; the proper floor-backed test passes. The first extra door-control run used
the occupied passenger demo with an invalid forced rider pose; arrival correctly
refused the overlap. That capture is excluded. The intended empty control fixture
passes, while the independent ordinary passenger campaign remains accepted.

Native evidence is bounded macOS Metal coverage, with gameplay presentation
pacing enabled. No throughput or cross-platform visual claim is made. Sources,
actions, raw traces, checkpoints, exact binary/package hashes, compiler currentness
receipts and screenshots remain under `local/predator-search/`. The final named,
lit playable case is `native-elevator-final/`; `assertions-final.json` records the
strengthened deadline assertions against all four native search captures.

## Deliverables, reproduction and integration

`tools/bench/predator_search.py --write-fixtures <new-directory>` writes four
tiny source projects: `elevator`, `fast`, `ordinary`, `blocked`. It does not build
Rust or bake anything. Export a selected project explicitly, then run the direct
binary with matching `--mode`, `--package` and a fresh `--out`. The lit default
project has a distinct **TEMPORARY / Monster search / elevator** menu name.

The temporary main elevator demo is **1.0.5**, and the physical lab **1.0.4**.
The main demo, lab, passenger fixture, empty controls fixture and all four search
fixtures were exported with the accepted compiler and passed integrity/currentness
checks. Only tiny fixtures were prepared; no full shipped-map bake was performed.
`deliverables/` retains the updated main demo, separate playable monster-search
demo and a screenshot comparison from actual game views.

Existing exported hub/collection packages still need refreshing against the final
compiler identity after integration. Use the hub task's authored sources/cache;
none of those maps or the shared elevator artwork were changed for this feature.
The separate lighting work remains independently owned. The baseline predator
still has no built-in player-damage/catch action; custom attacks are unchanged.

Accepted game SHA-256:
`7cbb6cd07568d92c9c80430dce602c1dfb4fb77d93abfd52c696ec4003f64a61`.
Compiler SHA-256:
`513b1a9284eff2a86b408952391a7acc55f56e7cfef1825c55f4b2847725b6d3`.
