# It Only Exists in the Beam — three native authoring passes

Implemented on `0.12.0-staging`, preserving Closing Time's local handoff
`71ac4f88b43d8ab3fe5aa2604c3e8a6137f49978`. Local commits and validation only:
no push, GitHub CI, main merge or release. Closing Time deliverables are unchanged.
Native acceptance is **Mac only**, on the Apple M2 Pro/Metal backend.

## Playable delivery

Editable project: `projects/beam/`. Final complete-project distribution:
`local/verification/0.12.0-beam/It-Only-Exists-in-the-Beam.placesmap`.
The verification folder preserves immutable `payload-final/` direct player,
editor and compiler, plus the final `payload-reset-verified/` rebuild with
explicit pose diagnostics, launch scripts, source snapshots, real input recipes,
framebuffer captures, post-mixer audio, original screen recordings and receipts.
Repository assets and the host SDL3 runtime are required; this is not an installer.
Use Full lighting for the intended atmosphere. Static is also packaged.

Recover three physical gifts from a deserted furniture store: red from the lower
office aisle, stars from the lower lounge, green from the actual upstairs office.
Carry and set them onto the broad counter by the warm exit, then use the door.
The ramp, tall enclosed shaft, upper room and openable door are physical geometry.
The installed mannequin has a black albedo preset; it uses the existing rig,
animations and real fitted PNG unchanged. No pending Home/shared/duck/closet or
corrected cloud mannequin delivery, or rejected Spoonerman, was integrated.

F reveals and holds the creature using the renderer's actual flashlight
illumination, including live occlusion. Darkness hides it immediately; an 850 ms
release grace precedes movement. It walks at 0.9 m/s through the shared planner,
can open the upper door, remembers a seen position for six seconds, and patrols
both floors. No teleport action is used. A defeat requires real collision-aware
sight, less than 1.05 m planar distance, less than 0.6 m height difference, one
continuous second outside the beam, and leaving the entrance refuge. Eight
initial seconds permit orientation. The refuge is explained in the opening and
F1 hints. These deliberately forgiving rules make carrying and looking away
tense without unavoidable random kills.

The short opening is dismissible. F1 gives optional current-progress guidance,
including lost-gift reset instructions. There is no persistent tutorial HUD.
Explicit win/death panels freeze gameplay. Enter resets player, creature, goals,
closures, gifts/held state, doors, lights, audio and flashlight; Esc returns to
the menu. The EXIT sign also performs an ordinary in-world restart when unheld.

## Three substantive passes

Every pass was actually played. Original failures remain in the evidence folder;
an attempted capture named `win` does not establish success. `acceptance.json`
records the actual outcome. These are new scenes/scripts and played refinements,
not three renamed copies of the same result.

| Pass | Creation/refinement and observed corrections | Played evidence |
| --- | --- | --- |
| 1: complete loop | Native floor/wall/furniture/upper floor/ramp/door/fixture/trigger/physics authoring, full Luau progression, black mannequin reveal, explicit outcomes/reset. Real attempts exposed out-of-reach pickups, blocked upstairs approach, turn-before-carry-settles, a too-small return table and stale AI reset transforms. Moved the shelf, widened the receiving surface, raised the return band, corrected authored routes and fixed reset synchronization. | `Beam-pass1e.placesmap`, `passes/pass1-source/`; `pass1-counter-win/acceptance.json` passes the complete controls route. `pass1-death-corrected/acceptance.json` passes two deaths/respawns with the corrected runtime. Earlier failed receipts are retained. |
| 2: atmosphere and fairness | Warm entrance landmark, cold search aisles, oak/off-white upper office and oriented counter. Increased release grace and contact dwell; added a quiet existing spatial impact cue for nearby dark movement. Fixed modal audio/input/flashlight lifecycle. Full bake confirmed useful darkness and clear flashlight interaction. | `Beam-pass2.placesmap`, `passes/pass2-source/`; `pass2-win/acceptance.json` and `pass2-death/acceptance.json` pass win and two respawns. Native Full entry/reveal images were inspected. |
| 3: native polish and delivery | Fitted the right fixture away from the shaft wall, closed a small upper wall joint, added lost-gift recovery guidance, reviewed Safety/navigation, saved/reopened/exported and recovered exact sources through the editor. Replayed the actual final baked export and measured uncapped performance separately. | Final package and `passes/pass3-source/`; final campaign receipts, source-recovery hashes and performance receipt below. Genuine gameplay video includes beam behavior, objective carrying/return, win, death and restart. |

## Native acceptance and controls provenance

`tools/authoring/beam_acceptance.py` imports each tested archive into an isolated
state directory and launches the **direct executable**. The route uses ordinary
held W/A/S/D, E/F/Return/F1/jump/crouch events and camera-only aim helpers through
the actual handlers. It never changes player/item/AI poses, navigation goals,
objective flags or invokes gameplay targets directly. `beam_audit.py` checks
captured physical state and actual outcomes. Native input recipes and traces are
retained. This is transparent automated native verification, not a claim that a
human manually authored or played those movements.

Final archive SHA-256:
`12ac18250014116e7ce3f7bb27af17ccead9cc6ebacc11eef0631590d420f332`.
Final accepted player SHA-256:
`ffd1f80fba72fc0a2d5e69d00e931a4e19bb095bf9a1b7affc41ae6ee2828761`.
The full edge campaign/video used the preceding immutable `payload-final` build
(`3e5797a3…`); the final rebuild adds explicit trace fields and strengthens the
small reset regression, with no gameplay/source/asset changes. It replays the
same exported package for immediate reset, win and menu/reentry acceptance.

The final complete winning route physically traverses both floors, opens the
upstairs door with E, carries the green gift down the ramp, rests all three
unheld physical gifts on the counter, and uses the exit for explicit win feedback.
F1 after the first return reports 1 of 3. The store loop stops at completion. The fresh-runtime reset replay additionally
checks the exact `run_restarted` event: AI clock/speed are zero and AI,
renderer-bound frame, the component transform read by `api.actor`, and beam
anchors agree immediately at the authored start before movement. The first
real F reacquisition after each respawn reveals that body; the tiny regression
also invokes actual scoped Luau and checks all five reset-body sampling anchors.
Beam testing measures about 8.5 m versus 2.4 m attenuation, a dimmer 17-degree
edge, exclusion at 35 degrees, immediate disappearance when switched off,
delayed movement release, reacquisition, and real partition occlusion. This
small enclosed map does not offer a straight 24 m maximum-range sightline.

The final campaign separately checks stars-first ordering, pickup invalidating a
return, set-down reentry, dropping elsewhere, missing-objective exit refusal,
ordinary restart sign, death twice/respawn twice, death holding a gift, authored
AI traversal around furniture and onto the upper floor/opening its door, and
live perimeter/upper-floor push/jump/crouch attempts. Every sample checks script,
audio and body-recovery diagnostics. Exact successes and any limitations are in
`beam-handoff.json`; the campaign is explicit developer tooling, not CI or a
proof of every possible input/physics arrangement.

`checks/immediate-reset-agreement.json` retains both zero-time reset snapshots.
The native `accepted-death/acceptance.json` checks them and real F reacquisition;
the actual Luau host read is additionally exercised by the small reset test.

Native editor save/reopen/package export/source recovery is recorded in phases
44–48. `checks/source-recovery.json` confirms all three declared source files
recovered byte-for-byte, including metadata and Luau. Standalone import and play
use the exported baked package, not editable JSON. Normal project `verify
--require-current` still has Closing Time's already reported project-index
limitation; matching source/dependency admission, exact recovery and native play
are the concrete checks here.

## Engine changes and validation

All gameplay is ordinary map-local Luau. Reusable host additions are readonly
player/actor observations (raw illumination, hold state and collision sight),
bounded navigation through existing AI, bounded dismissible guidance and run
outcomes, and optional `hint` dispatch. `light_response.reveal_only` hides
rendering independently of release grace. New exports declare
`scene-actions-v1`, so older hosts refuse the expanded contract explicitly.
The old `luau-v1` capability and original maps remain readable by this runtime.
Existing execution, memory, action, reference and staged rollback bounds remain.

Three small engine regression tests protect distinct contracts: sparse/oversize
guidance bounds and atomic outcome rollback/reset cancellation; immediate beam
visibility versus movement grace and the native-found stale transform reset;
and collision-aware script sight plus stable nearby navigation goals. They use
tiny metadata worlds, no graphics/assets/full-map bake, and are discovered under
the owning `src/engine/tests/`. All 137 Rust tests pass in under a second after
compilation. Format, strict all-target/all-feature Clippy and locked all-feature
release build pass. Release Luau infinite-loop, memory and error refusals pass
without baking a map. Asset validation reports 594 entries, 371 placeable,
zero errors/warnings; existing detailed texture warnings are retained.

The initial broad Python run hit the sandbox's unavailable Metal adapter in the
existing graphical smoke class. The explicit CPU run uses its supported
`PLACES_SKIP_SMOKE=1`; Beam's direct native Metal campaign is separate. This is
an environment limitation, not a successful run of that graphical test class.

The geometry checker reports zero errors and three retained warnings: two
0.00031 m² generated perimeter-trim joint slivers and the exit leak heuristic
which omits closed doors. Ten missing-wall findings at deliberate floor-strip,
ramp and real door connections have narrow, reasoned intent rectangles. Errors
were never suppressed. Native Safety retains floor-contact model reviews and
its explicit triangle-pair budget warning; its ramp-floor edge samples and exit
door sample need contextual review. No blanket placement ignores were added.

Performance uses physical **1920×1080** (960×540 logical), High filtering,
Full lighting, bloom/flashlight on, reflections off, 1200 warmup plus 6000 measured
frames. The entrance view aims at the mannequin: 28 props, one skinned actor,
three gift bodies, two doors and four fixtures (42 visible batches/28,527 vertices).
Actual presentation switches from initial Fifo to **Immediate**, swap interval is
zero, and the Mac display reports **120.00 Hz**. Recording and frame limiters are
off; GPU pass timing is measured.

| Run | Average FPS | Median FPS | Interpretation |
| --- | ---: | ---: | --- |
| Preceding immutable runtime | 603.08 | 762.78 | Clearly above display refresh; uncapped observation. |
| Fresh final runtime | 121.45 | 120.86 | Refresh plateau despite Immediate; **inconclusive throughput**. |
| Fresh final quiet repeat | 304.75 | 590.35 | Alternating uncapped and refresh-paced intervals; **inconclusive stable throughput**. |

No gameplay change separates these runs. The host's variable presentation pacing
prevents a reliable gain comparison or stable uncapped claim for the final build.
All actual means, frame CSVs, near-refresh/fast counts, presentation, settings and
GPU distributions remain in `checks/performance-campaign.json` and the three
native directories. The compact evidence and final receipt include all three runs and the later
pacing limitation. No minimum FPS or before/after gain is claimed.

## Actionable discovered gaps for the queued implementation task

Shared gaps are deduplicated against the Closing Time report. Ordinary Luau
authoring is intentional; missing source/component/link controls are editor gaps.

| Reproduction / impact | Workaround here | Requested correction and acceptance |
| --- | --- | --- |
| Add a door with its initial footprint inside furniture; Add refuses but the old object remains selected. Subsequent numeric fields can modify an unrelated wall. Phases 13/13b exposed this. | Exact source/checkpoint comparison; restore wall 4, frame an empty location and fit the door before changing fields (13d). | Failed Add should clearly preserve/identify selection or disable follow-on creation edits; add a tiny regression and native refusal/recovery sequence. |
| A valid final ramp/header pose can have invalid intermediate numeric edits. Raising the ramp before fitting its footprint is refused; a header edited through the door is refused. | Dimensions/position first, then height/offset (23b/35b). | Atomic multi-field transform/preview transaction with clear collision reason; preserve validation and undo. |
| Stable IDs, typed state and prop/door script links lack inspector controls; exact item plus unheld and enter plus exit cannot be composed in one trigger sheet. | Documented JSON metadata, filters and bindings. | Implement the shared Closing controls for IDs/state/multiple events/composable filters. Verify held removal/reentry and out-of-order progression in both demos. |
| Detailed nav body, light-response thresholds/reveal/hold, attached audio and physics reset group are not exposed by the coarse behavior/pickup UI. | Explicit source components on the GUI-placed mannequin and gifts. | Expose bounded existing component fields with capability/reference checks, authored-pose preview and clean reset. Do not replace script behavior with map-specific engine branches. |
| Prop Y remains relative to the first room, while selecting an upper floor suggests local storey ownership. Cutting a room removes both floor and ceiling; a ramp needs a separate same-plane tall carrier. | Native upper props use explicit 3 m offsets; separate enclosed shaft and landing rooms. | Clarify/store room ownership and coordinate basis, show cuts/clearance, and offer a safe connected-storey workflow. Retain incompatible-overlap refusal. |
| Holding E prioritizes item drop over door use; a player cannot close/open an ordinary door while carrying. | Open the upper door before collecting green, and leave it open on return. | Define a contextual held-item/door interaction that preserves deliberate set-down and obstruction safety. Replay carrying through both directions with no forced drop/clipping. |
| Pickup/return relies on conservative rotated-body overlap; a low floor-level receiver can briefly count a dropped item touching its envelope. | Raised exact/unheld band to 0.6–1.4 m and a wide real support counter. | Offer support/sleep/centre-aware receipt policy and visible receiver fitting; preserve enter/exit and held transitions. Test a rotated small box, not a whole map. |
| Automatic-handler `api.reset` may discard its dispatch report inside `EntityWorld::tick`; interaction reset works and is tested. This is a code-inspection finding, not a reproduced Beam failure. | Beam resets through ordinary interaction/outcome paths. | Create a tiny start/update-reset repro; propagate player/item/audio reset requests safely if confirmed, without callback recursion or stale commands. |
| The scene needed general messages/outcomes, renderer-backed actor queries and script navigation. | **Implemented here** with bounds/capability admission. | Later safety task should audit readonly/scoped callbacks, sparse/metatable/oversize text, action/reference/finite bounds, reset lifetimes and release refusals; replay both modes. |
| Native death/reset left AI component transforms at the catch position although agent/render state reset. | **Fixed here**: synchronize transforms and clear sight; small regression plus repeated native respawn. | Keep the reset regression and replay both modes after combined changes. |
| World audio/input/flashlight could persist through result/help/reset. | **Fixed here**: common gameplay gate, staged modal input clearing, authored audio reconciliation, flashlight reset. | Later integration should replay hints while carrying, outcomes, menu/reentry and ordinary reset with actual mixer evidence. |
| Installed horror audio lacks a suitable soft mannequin step, beam acquisition cue, distinct gift confirmation and restrained success sting. | Existing spatial `impact_soft`, low HVAC, switch click and door closure; no invented asset IDs. | Supply normalized quiet step/return/success assets with chooser preview and shared lifecycle; no loud random scare required. |
| The current mannequin has inherited hand/rig/animation limitations; black tint is a catalog preset of the unchanged installed model/PNG. | Existing asset only; no cloud revision integrated. | Integrate only the separately corrected verified delivery in the next asset stage; keep all fitted PNG/UV/rig/clip contracts and replay reveal/carry occlusion. |
| Safety sees floor-contact coplanarity, misses doors in leak/drop heuristics, and exhausts triangle-pair review. Full bake reports no probe in one tiny floor strip, although the shared field covers the scene. | Reviewed native contacts, preserved warnings and physical play; no raised furniture or blanket ignores. | Improve door/storey-aware edge checks and candidate prioritization; expose incomplete coverage/probe sampling accurately. Preserve strict actual duplicates. |
| Full project verification/resource-root/prepared reuse issues match Closing Time. Direct SDL windows are missing from desktop app inventory. | Explicit asset environment root, exact source recovery, native input driver and original OS recordings. | Implement the already specified shared fixes and environment integration without changing ordinary package/import limits. |
| Small bitmap buttons/text and modal edge strips remain visible at Retina scaling. | Existing font/layout; concise readable panels, no UI redesign during map work. | Apply the separately queued readability pass, including the approved five-dot crosshair and per-map/session behavior. |

Next work remains ordered: accepted asset/corrected mannequin integration (push
combined local work there), editor readability/crosshair, implement concrete
discovered gaps from both demos followed by Luau/import safety and replay of both,
then the separately authorized connected feature-test map/game with honest
feature coverage/manual gaps. That map exercises supported features and exposes
editor gaps; it does not authorize arbitrary missing engine features. Stop and wait for Connor after that queue. No work
on those later stages was started here.

## Evidence delivery

Local archive paths, confirmed Library identities, final campaign/performance
results and cleanup receipt are recorded in
`local/verification/0.12.0-beam/beam-handoff.json`. The compact video uses genuine
native screen recordings cropped to the game content, real post-mixer audio and
simple chronological cuts, without synthetic gameplay reconstruction. Source-only
archive and baked distribution are separate. Original recordings remain local.

Library creation and a separate metadata listing confirm all four identities and
byte sizes; returned version attributes were applied and read back locally.

| Deliverable | Confirmed Library ID | Bytes |
| --- | --- | ---: |
| [Beam-native-gameplay-final.mp4](https://chatgpt.com/api/library/files/libfile_bcc9fe4f74788191956ae1862857ccfa/download) | `libfile_bcc9fe4f74788191956ae1862857ccfa` | 2,698,460 |
| [Beam-screenshots-and-evidence.zip](https://chatgpt.com/api/library/files/libfile_cbf70ec98b9c81918d735ee7a7a8f7b8/download) | `libfile_cbf70ec98b9c81918d735ee7a7a8f7b8` | 2,864,760 |
| [Beam-editable-source-only.zip](https://chatgpt.com/api/library/files/libfile_aa4b29d81cc481919dd07d1102eda3eb/download) | `libfile_aa4b29d81cc481919dd07d1102eda3eb` | 12,805 |
| [It-Only-Exists-in-the-Beam.placesmap](https://chatgpt.com/api/library/files/libfile_fae92ac49f6081918edc971c559cb6e2/download) | `libfile_fae92ac49f6081918edc971c559cb6e2` | 6,212,264 |
