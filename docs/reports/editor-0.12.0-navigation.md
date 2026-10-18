# Layered navigation and native editor inspection

Implemented on `0.12.0-staging` after Stage 7 head `588a77fe`. This pass is
navigation only: no later game-mode authoring, asset integration, main merge,
release, version bump or app packaging.

## Behavior and architecture

The prior audit was substantially correct: automatic 0.20 m sampling, one Y per
XZ cell, bounded A*, and separate runtime AI/perception/movement. The decisive
limitation was losing overlapping floors. PNV2 retains vertically ordered spans
in each XZ column and stores eight explicit per-body directional links. This
keeps automatic authoring and reusable bounded query scratch without introducing
a polygon-mesh toolchain. Region labels, endpoint projection, diagonal traversal
and shortcut smoothing retain floor identity. Shortcut attempts have a shared
128-attempt budget; exhaustion preserves the certified unsmoothed chain.

Clearance considers body radius/height, ceilings and upper-floor undersides.
Continuous slopes respect slope limits independently of the small integration
step; discrete steps and drops use the step limit. Continuous segment probes and
exact swept discs prevent thin solids between sample centres disappearing.
Step sweeps cover the complete vertical body envelope, including short bodies
whose old and new height bands do not overlap. Static, live-item, door and
ceiling checks share this conservative rule.
Adjacent climbable landing edges are distinguished from a low floor directly
over the body. Runtime support stays near current feet and reports actual
movement rather than requested travel. Movement also checks the baked class
links, avoiding a separate permissive steering path.

Doors remain live portals, assigned to the correct height. Capable agents request
unlocked doors; locked and obstructed doors refuse passage. A stuck opening leaf
invalidates the waiting route after a bounded delay. Bounded angular probes check
moving leaves against AI bodies and current physical bounds. Physics colliders
supply a small live AABB overlay, shared by planning and movement, without
rebaking global navigation. Actor body probes and local shoulder passing avoid
ordinary opposing-actor collisions. Repathing rotates across actors with eight
queries per tick, 8,192 A* expansions per runtime query and per-agent cooldowns;
unreachable/blocked destinations remain retryable when the world clears.

`Navigation >` is an optional native editor panel: explicit current-document bake
without lighting/package export; class, actor and spawn-point selection; body and
door-capability labels; floor slices; coverage and exact-span route endpoints.
Feedback distinguishes invalid, unreachable, complete and budget-limited routes.
Edits, undo, level changes and recovery hide stale results. Authored door state
is inspectable; live props and crowds are checked in Play. Coverage rendering is
capped at 12,000 displayed cells with counts; queries retain the whole graph.

## Compatibility

Only the navigation record changes to PNV2/version 2. The package container,
source/project formats and lighting records keep their existing versions. Every
navigation reader/writer and the compiler identity use the new version. PNV1
playback is safely refused with a recompilation instruction. Complete Stage 7
archives still recover exact editable sources for rebuilding. Counts, offsets,
strict height ordering, class masks/regions, reciprocal adjacency and traversal
bounds are validated before use. Count and 128 MiB byte budgets are checked
before large allocations.

Stage 7 export/import and production Luau safety paths remain in place. This is
not the later dedicated Luau/map-import review of the two finished modes.

## Verification

Retained evidence is under `local/verification/0.12.0-navigation/`, outside
`target`. `navigation-handoff.json` records final binary/source/package hashes,
the pushed commit, exact CI run and cleanup receipt. `payload/` contains direct
player/editor/compiler binaries; `try-editor.sh`, `try-player.sh` and
`replay-native.sh` retain isolated runnable workflows. No `.app` was made.

The final locked release build, formatting, debug/release strict Clippy and
package validation/current-source verification pass. The source Python suite
passes 368 cases with three optional smoke skips; asset validation reports 593
entries and zero errors. Final production Luau checks pass three bounded
refusals without baking. Stage 7 compatibility passes 19 checks: exact recovery
of the old archive, new export, validation, exact recovery, project check and
player import/listing. Stage 7 and native acceptance are explicit developer
checks outside ordinary CI; ordinary Rust tests never bake a shipped map.

Final native player receipts are `native-player-r4/receipt.json`, with 106 actual
framebuffer captures, live actor/physics snapshots and direct clean exits:

| Scenario | Verified native result |
| --- | --- |
| Stacked furnished rooms and ramp | Lower actor travelled a sampled 30.48 m through the side ramp/bridge to `(3.04, 3.00, 4.02)`; upper actor stayed on its storey |
| Body clearance | Radius 0.16 m actor crossed the 0.8 m aperture; radius 0.48 m actor refused it; trials start clear of other bodies and run sequentially |
| Live door | Locked leaf held the actor; an unlock-only action let AI open/pass; close/lock interrupted return; unlock let AI reopen and recover |
| Moving physics crate | Actual dynamic body blocked both directions; moving it away twice resumed the same actor without resetting its pose |
| Opposing actors | Both reached their goals with lateral avoidance; minimum sampled centre separation was 0.695 m for combined radii 0.60 m; the small Rust fixture also checks every tick |
| Unreachable island | Actor stayed on its island and settled; 13 queries over 9.71 simulated seconds, rather than per-frame retries |

Final native editor receipts are `native/editor/`, checked by
`check_editor_inspection.py`: current-source bake, all bodies and authored actor
selection, stacked route, slices/all-floor coverage, invalid goal, unreachable
island/large passage/locked door, edit invalidation, undo and explicit rebake.
Actual 1440×816 and 960×544 captures were inspected; route feedback and Clear
route remain visible without overlapping ordinary footer controls.

Native findings fixed actual 3D/planar arrival disagreement and long doorway
steering probes. Earlier failed runs are retained. Harness corrections removed
initially overlapping clearance bodies and simultaneous doorway contention,
and aligned the ramp assertion with its one-second position sampling.

The final timing case averaged **572.16 FPS** over 1,200 measured frames after
120 warm-up frames: actual **Immediate**, V-sync off and no frame limiter,
**1600×900** drawable (800×450 logical), Apple M2 Pro/Metal. The display receipt
records 120 Hz; throughput substantially exceeds refresh, excluding that cap.
The compact scene has nine goal-driven actors, four baked body classes including
the reference, one dynamic body and blocked/unreachable goals; High textures,
Static (`off` variant) lighting, bloom on, reflections off, no navigation dumps,
capture or video recording. Fixed 1/60 s simulation covers 22 simulated seconds;
at completion no actor was still moving and 122 queries had run. Mean update
cost was 1.19 ms, with about 19.66 draws and 2,507 visible triangles per frame.
This is an absolute compact-workload measurement, not an improvement comparison.
An earlier refresh-adjacent run remains explicitly inconclusive in its receipt.

Eleven added small regressions have distinct purposes:

| Owning component | Protected failure and unique value | Cost |
| --- | --- | --- |
| Compiler | Both stacked surfaces survive, a real ramp connects them, serial/parallel output agrees | At most 1,200 tiny sampled nodes; no assets |
| Compiler | A 2 mm wall between clear cell centres must break links | One small two-room-width strip |
| Compiler | A low overlapping slab remains overhead rather than a landing exception | Two small overlapping surfaces |
| Engine query/codec | Close-storey smoothing keeps endpoint identity, blocked upper endpoints cannot fall through, PNV2 round trip and obsolete/forged record refusal | Four nodes |
| Engine movement | Correct storey and real partial displacement against a live thin solid; close floors retain support and reject oversized bodies | Small floor samplers; no bake |
| Engine AI | Live blocker removal resumes movement through ordinary bounded retries | 35 nodes |
| Engine AI | Two actors pass without overlap; a twelve-actor query burst remains capped and fair | 49 nodes, at most 160 fixed ticks |
| Engine door/AI | Fast leaf motion cannot tunnel through a mid-swing body; blocked portal becomes usable after clearing | One leaf, three nodes |
| Engine movement | 3D and planar waypoint arrival cannot disagree and strand a ramp walker | One sloped sampler and short movement |
| Engine AI | An actor inside a blocked closing portal can request reopening after unlock, respecting lock/cooldown | 21 nodes |
| Engine movement/query | A short body cannot lift through a suspended static, live-item or door solid between clear endpoints; legal unobstructed steps still work and closed openable doors are requested | One floor sampler, two nodes |

`cargo test --workspace` passes all **133 tests** (0.26 seconds of test bodies),
including the 122 inherited cases. No full shipped map is baked in ordinary tests
or CI. The moving-solid case found and fixed an actual empty-spatial-index sweep
fallback that silently skipped supplied collision boxes. Required strict Clippy
passes with all workspace targets/features and `-D warnings`.

## Remaining limits

- A layered 0.20 m sample grid can conservatively miss small passages. No polygon
  mesh, manual link editor, jumping, climbing or swimming AI is added.
- Very large detours can exhaust the 8,192-expansion runtime query budget and
  remain incomplete; editor inspection allows 65,536 expansions. Searches do
  not persist their frontier between ticks.
- Dynamic physics uses conservative current AABBs; rotated and hollow objects
  can exclude extra space. Actors do not push/manipulate props to solve a route.
- Full vertical step envelopes can conservatively refuse a tight passage that
  would require more detailed timing between horizontal travel and lifting.
- Local passing handles ordinary encounters where space exists. Tight crowds
  and passages without passing space may wait; there is no traffic reservation
  planner. A solid moved into an actor can trap it until the obstruction clears;
  forced depenetration is not implemented.
- Editor routes use authored static/door state, not a running world simulation.
- Native platform acceptance is macOS/Metal only. Performance numbers apply to
  the recorded compact workload and are not a comparison with the old build.

## Primary reference comparison

The design separates topology, route queries and local collision probes, informed
by Valve's [AI navigator](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/server/ai_navigator.cpp),
[pathfinder](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/server/ai_pathfinder.cpp),
[local navigator](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/server/ai_localnavigator.cpp),
[move probes](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/server/ai_moveprobe.cpp),
[network manager](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/server/ai_networkmanager.cpp)
and [nav generation](https://github.com/ValveSoftware/source-sdk-2013/blob/master/src/game/server/nav_generate.cpp).
This is functional inspiration, not Source feature parity or a copied network format.
