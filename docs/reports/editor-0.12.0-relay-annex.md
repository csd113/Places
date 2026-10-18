# Relay Annex — feature-test campaign, 0.12.0

Separate campaign on `0.12.0-staging`, based on accepted shadow-only
head `0f87416c`. Closing Time and It Only Exists in the Beam are preserved.
This is a commissioning shift, not the later environment-demo hub.

The three visits are Snow Intake, The Four Floors, and Discharge. Staff Ives,
Mira and Sol give short text conversations and practical work. The player returns
a supply from an icy yard, traverses four physically connected floors at
0/3.2/6.4/9.6 m, completes an impact return and power check, finds a real
flashlight shadow, and retrieves a floating duck before leaving. Forward airlocks
create fresh local state and clear held equipment. Progress is local to each visit;
whole-campaign persistent variables/inventory remain unsupported.

## Supported feature coverage

The inventory used the implemented level definitions, editor Add/property/menu
controls, scripting API and installed catalog, rather than proposed APIs. The
campaign uses supported room/storey geometry, ramps/stairs, doors, typed trigger
volumes, dynamic item bodies/material response, Full lighting/circuits/flashlight,
text guidance, audio emitters, AI/animation/spawn, water/fog, project transitions,
source recovery and native thumbnails. It does not promise every combination.

| Supported combination | Playable use / evidence |
| --- | --- |
| Winter exterior, sky/weather, snow and ice | Intake yard, existing trees/drifts/lantern; carried box retrieved from ice with real sliding/braking. |
| Four connected storeys | Tower floors 0/3.2/6.4/9.6 m; 16 real steps, two opposite ramps, landings and edge rails; every height walked. |
| NPC text and hands-on progression | Ives supply return, Mira impact/power/shadow work, Sol duck rescue and signoff; contextual F1 advice. |
| Doors, locks, triggers and map load | Actual opening leaves and threshold crossings Intake → Tower → Discharge; no injected Load in complete route. |
| Pickup, set down, throw, receipt filters | Settled exact supply/duck centres; floor-two sphere thrown onto target; no recovered-body workaround. |
| Physics/material behavior | Ice friction, authored sphere restitution, carrying/throwing across upper-floor collision, buoyant duck in pool. |
| Lights, Full circuit and flashlight | Floor-three wall switch restores observation; beam finds hidden fourth-floor witness. |
| Shadow-only entity and animation | Invisible mesh remains an actual flashlight caster; supported AI walk cue advances in place. Legacy upper-floor walking route is recorded as a gap. |
| Audio and scripted entity behavior | Existing ambience, switch, bell, impact and footsteps; relief spawns once, arms-up pose; native mixer output has no errors. |
| Water and regional fog | Swimming, carrying duck underwater, shallow east ramp exit, dry return and fog/reflection presentation. |
| Transition state and inventory | Same declared false stamp asserted at each map start; prior stamp, held parcel and flashlight do not persist. Fresh visits are the supported contract. |
| Graphics/settings and icons | Native Low/High changes retain required Full lighting and active circuit; three imported native thumbnails render. Non-entry names use stable IDs. |
| Ending, reset and safe recovery | Win, Enter resets final visit; whole-campaign board returns to Intake; real recovery-well walk and deliberate labelled off-map placement recover safely. |
| Package/source lifecycle | Complete three-map package validates, requires-current verifies, ten editable files recover exactly and verified-cache re-export is byte-identical. |

## Authoring provenance

The actual direct native editor created the main yard, four main tower floors
(three `Add upper floor` operations), tower shaft, three principal shells, NPC and
challenge placements, Winter dressing, intake door/trigger, basin and its ramp,
and nine additional tower furnishings. Thirteen initial/final passes plus three
furnishing passes have ordinary-handler input recipes, focused-window document
checkpoints and real framebuffer captures. A short desktop recording shows the
final native Winter drift creation. This is **scripted visible-control input**,
not a claim of manual mouse authoring. Failed recipes are retained.

The explicit source passes consolidated native working projects into the final
three-map manifest; assigned stable IDs and fitted collision sizes; added wall
openings, two airlock vestibules, three shaft flights/landings/rails, per-storey
placement heights, ceiling/material policy, Winter sky/weather, water, light and
audio definitions, detailed physics, receipt filters, reset volumes, timer/spawn
animation bindings, and Luau conversations/objectives/gates. Many of these have
existing native controls; using an external source pass for efficiency is not
itself evidence that a feature is missing. Native script editing, state/event
forms, light/audio configuration and project membership already exist.

## Concrete gaps and next-stage reproductions

| ID | Expected editor feature / actual finding | Workaround and reproduction |
| --- | --- | --- |
| R1 | Create a ramp at a valid destination even when the current camera target is invalid. Add validates its default pose before a full transform can be entered. | Tower: after four stacked floors, Add Ramp at the main room target refuses `spans rooms with different floors`, clears selection. Retained `07-tower-shaft-attempt` then shows no Field control. Source-authored three side flights; no editor rewrite. |
| R2 | Staircase authoring and connected-storey landing/rail construction. Geometry offers Room, Wall, Ramp, Recess; floor tools copy/split but do not build connected stairs. | Tower: native upper floors and shaft, source 16-step first flight, two ramps and three landing rooms. Enter all four storeys in Play; no implicit floor cuts or global regions used. |
| R3 | Per-storey overrides and explicit carrier ownership. Floor patches/regions are global over overlapping rooms. | Tower: an ice target patch affected every floor and caused unplanned slide during traversal. Replaced with a dry contrast patch; room materials distinguish floors. Request a storey selector/preview and diagnostic before assuming local ownership. |
| R4 | Height-aware legacy route controls and preview. A route on the witness with authored `y:9.6` rejects its first 2D move as a >.30 m step. | Tower: retained checked route failure; use the supported AI `idle_clip: "walk"` override in place, with an explicit beam-response body. AI owns its cue; the startup timer alone is overwritten by ordinary idling. Navigation AI and legacy routes are different contracts. Do not claim an upper-floor walking route passed. |
| R5 | Physical contact/clearance diagnostics for dormant items, and a useful warning when pickup succeeds but the held item cannot leave contact. | Tower: sphere at exact `y:3.2` is picked up yet its pose remains unchanged; drop/throw is refused. A 5 mm placement clearance also fails. A two-room CPU regression proves the lower ceiling's 12 cm upward backing overlaps the upper floor; the narrow runtime correction clips that redundant flat-ceiling backing against real coplanar floor coverage. No format change or broad editor work. |
| R6 | Direct water creation/configuration and water/flight vertical slice aids. Basin editor text explicitly calls water a separate feature; there is no Water Add entry. | Discharge: native basin/ramp, source `water[]`, buoyancy parameters and exact unheld/settled return box. Editor currently preserves these fields. A continuous replay exposed that the original rectangle omitted the submerged ramp; a separate validated shallow rectangle now covers that ramp. Inspect the vertical slice and walk continuously out of the water. |
| R7 | Outdoor room/sky/weather and directional-illuminator controls. Map capabilities expose spawn/default materials/presentation/fog/Full requirement, not this complete exterior setup. | Intake: native snow floor and props, source open ceiling, existing Winter sky, snow and moon. Reopen Inspect versus authored Environment/Play. |
| R8 | Clear physical-item aim/receipt feedback during authoring. Exact identity and settled-centre filters exist, but the test square is source geometry with no game-visible receipt preview. | Intake/Discharge: contrast floor pad plus bell and NPC/F1 advice. Future editor could preview actual item centre, sleep state and receiver band in Play. |
| R9 | A graphics cache hit for a non-entry map must inspect that map's manifest. The native Low request falsely reported a changed package. | Fixed the one cache-hit call to use existing `inspect_level(path,id)`. The manifest digest check remains intact. Native quality replay covers actual retained state/circuit/shadow behavior; existing tiny two-map archive regressions already protect selected-manifest isolation. |
| R10 | Connected-storey geometry review should distinguish shared ceiling/floor planes and shaft portals from defects. | Tower's read-only checker reports zero errors and 218 warnings: 201 opposite-face pairs on intended shared structural planes, 14 intentionally open connection runs, three single-height flood candidates inside the multi-height shaft. Retained without blanket ignores; native four-floor/edge checks are separate evidence, not an exhaustive closure proof. |
| R11 | Friendly non-entry map titles in the imported menu without loading inactive semantics. | The three native thumbnails display correctly, but non-entry rows use stable IDs (`relay_tower`, `relay_discharge`). Keep entry-based campaign launch; propose explicit per-map display metadata for the later editor/distribution work. |
| R12 | Message editing should preview panel width or wrap authored prose. | All maps: native captures showed long lines ellipsized, including the goal. Source prose was shortened to at most 46 characters per line; no typography/runtime rewrite. Reopen intro/NPC/F1 and compare the fixed-width native panel. |

Ordinary Luau objective logic and NPC conversation writing are intentional authoring,
not missing engine APIs. Project add/switch/transition/thumbnail controls exist;
this campaign's manifest consolidation was an automation choice. Existing Full
lighting, shadow-only toggle, physics details, event and script forms were not
removed or broadened.

Missing or useful sound work: distinct receipt/clearance success, locked-airlock
refusal, staff acknowledgment/voice, floor/wayfinding announcements, swim-entry and
exit splashes, large door pressure/equalization, and safe-respawn cue. Existing bell,
switch, door, impact, footsteps and ambience are reused honestly; no new sound was
synthesized. Useful future engine work includes explicit campaign-state/inventory
transfer policy, upper-storey route ownership, authored item-contact diagnostics,
visible receipt feedback, animation/prop grasp and door synchronization, fixed-light
dynamic shadows, and better wet/ice interaction feedback. These are backlog items,
not an exhaustive promise or authorization for this stage.

## Essential regression value

One new CPU regression uses two four-metre rooms, one sphere and sixty fixed
steps. It fails before the correction because the held sphere cannot lift, and
passes after. It also checks actual throw/release and collision on the uncovered
ceiling and upper floor. It uniquely protects the shared-storey backing bug in
about 0.01 seconds, without assets, GPU or any map bake. Existing tests are retained.

## Validation and delivery

Strict workspace/all-target/all-feature Clippy, release Clippy, formatting and the
locked all-feature workspace release build pass. All 167 Rust tests pass; all 369
Python tooling tests pass with three supported skips. This adds one small CPU
regression; no campaign bake was added to routine CI. Asset validation reports
zero warnings. Package adversarial checks pass all 30 cases, including bounded
archive rejection, source recovery, no overwrite and no partial publication.
The final source/package identity verifies current, and the recovered ten source
files plus verified-cache re-export are exact.

The final 60-assertion native audit passes. A single continuous run reaches the
ending with zero unintended resets, three actual map requests and all four
walked floor heights. Direct native chapter checks cover all three staff conversations, pickup/drop,
the floor-two actual throw, four heights, switch/beam/posed hidden caster, physical
airlocks, both fresh-state transitions, swimming/carrying/dry receipt, relief spawn,
win, Enter reset, whole-campaign restart, F1, quality rebuilds, edge rails and
recovery. Diagnostic placement is limited to the labelled quality/edge/void
checks; the complete route uses camera aims and ordinary keys/mouse only. Actual
SDL and native focus are checked before interpreting results. Both accepted
Closing Time and Beam natural win regressions pass with the delivery executable;
Beam also exercises Enter reset. Their six original source hashes are unchanged.

The replay recipes and failed attempts are retained. In particular, early FIFO
framebuffer video added enough overhead to invalidate two timed routes and its
fixed-rate output sped up time; those clips are not accepted gameplay evidence.
Subsequent continuous replay exposed the missing ramp water rectangle and a
near-edge duck drop. The authored shallow water fix and a deeper ordinary drop
approach were verified before final acceptance. No diagnostic teleport, forced
objective completion or direct Load substitutes for the continuous route.

Unrecorded throughput samples use High/Full, native Metal on Apple M2 Pro,
1280×720 physical drawable (640×360 logical, 2× backing), 1,200 warmup and 1,200
measured frames per pinned scene. V-sync and all limiters are off. Startup logs
first show Fifo, then confirm the applied Immediate presentation mode. The display
is 120 Hz; actual average throughput exceeds refresh and the timing receipts show
sub-refresh frame intervals, so these samples are not refresh-rate plateaus.
Average FPS is **183.0 Intake, 211.4 Tower, 159.9 final Discharge**. These are static
scene samples, not a traversal average or a before/after improvement claim. The
final Discharge sample includes the corrected ramp water. No video, framebuffer
readback or audio capture runs during throughput measurement; asynchronous GPU
timestamps remain enabled. Movie playback cadence is separate from this evidence.

Delivery is local under `local/relay-annex-0.12.0/`: `play.sh` launches the imported
complete campaign; `editor.sh` opens editable source. The package includes all
three prepared maps, scripts and native thumbnails. Preserved direct macOS
executables live in `payload-delivery/`, outside `target`. After confirming no
active target users, one `cargo clean` removed 8.8 GiB. Preserved compiler
validation, focused native editor reopen with byte-identical sources, and focused
player/F1 smoke all pass after cleanup. Installed SDL3 and the
checkout asset catalog remain required. Source and evidence are packaged with the
Library media. The short reel is a real-time WindowServer capture of the native
game window, cropped to remove its title bar, normalized to 720p, cut to real-time
highlights and muxed with the original engine audio. Its static native ending
receives a labelled-in-provenance 1.5-second editorial hold. The full run PCM
capture has a 60-second cap; Discharge clips therefore use the complete accepted
chapter recording and its original audio. The separate six-second editor clip shows a
representative native Winter dressing operation through scripted visible controls.

Exact commits, exact-head GitHub Actions status, Library identities, media timing,
individual checks, final native assertions and post-clean smoke receipts are in
`local/relay-annex-0.12.0/handoff.json`. No main merge, release, branch deletion,
environment-demo rebuild or next-stage editor implementation is part of delivery.
Native evidence is macOS/Metal only. Campaign persistence, upper-storey legacy
routes, the 218 candidate tower geometry warnings, missing cues and R1–R12 remain
explicit next-stage limitations; the campaign is a bounded feature sample.
