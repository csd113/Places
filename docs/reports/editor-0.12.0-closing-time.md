# Closing Time — native editor authoring and playable mode

Work is confined to `0.12.0-staging`. The editable project is
`projects/closing-time/`; the level ID is `closing_time`. It uses the currently
integrated Arcade, party, bowling and shared assets. No pending delivery,
Spooner-Man replacement, mannequin work, main merge or release is part of this task.

## Authored experience

A deserted venue has one short closing shift: return the burgundy ball to its
rack, the star gift to the other gifts, and the cat plush to the lobby lost-property
box; shut down arcade, party and bowling circuits; then lock the front door.
Returns remain outstanding while held or removed. Circuits can be shut down in
any order and repeated presses are idempotent. The closed entrance is the final
interaction and shows a concise good-night/completion world label. Another press
starts a fresh shift. The exit sign also offers a restart during a shift.

The scene has four adjoining floor/material areas, two abbreviated bowling lanes,
a complete pair of pin decks, gutters, arcade cabinets/pinballs, party furniture
and a warm lobby/aisle light that remains after the working circuits go dark.
Installed HVAC emitters progressively stop. Full lighting is the intended
presentation: the existing fixed-prop circuit contract holds authored illumination
in Static. That limitation is an implementation input below, not a claim that
Static provides the same shutdown atmosphere.

## Genuine editor work and source-only operations

Desktop automation could not address the direct, unbundled SDL editor window
(`cua.getApp` returned `Invalid app`). No `.app` wrapper was created. The existing
`places-editor --verify-script` facility clicked **visible** controls and sent
normal text/key/wheel input through the editor's ordinary event handlers. Hidden
controls cannot be invoked. `screencapture` recorded actual desktop pixels during
these operations. The delivered edit is labeled scripted native-editor authoring,
with normal-speed continuous excerpts and simple cuts. Cropping removes desktop
chrome; captions identify the operation and method. No replacement GUI or staged
GUI reconstruction is used. Failed attempts and original recordings are retained. Working recipes were regenerated during authoring; `recorded-inputs/provenance.json` explicitly identifies reconstructed early input recipes. Native video and checkpoints are original. Later recordings retain their exact input before launch.

Recorded native operations include floor creation/resize/materials, exterior and
partition walls, built-in model selection and placement, duplication, numeric
fitting, fixture family/power/range/colour/switchability, sounds, trigger bounds,
closed door creation, movable prop physics, Luau source creation/edit/check/save,
Safety inspection and surface mounting. `tools/authoring/` contains explicit
opt-in recording/input/evidence tools; these do not run in Cargo tests or CI.

The exact raw editor baseline and post-editor source diff are retained at
`local/verification/0.12.0-closing-time/editor-authored-project/` and
`manual-workarounds.json`. Source-only work is limited to:

| Operation | Why source editing was needed | Workaround |
| --- | --- | --- |
| Stable project/map names and IDs; spawn; default wall/floor/ceiling; presentation/fog | No existing-level ID rename, spawn or global defaults/presentation sheet | Initial metadata/source fields before geometry authoring; no generated artwork |
| Descriptive runtime identities, declared bool state and conditional E bindings on switches/door | Script linking is exposed only for triggers; component/state/condition authoring is absent | Ordinary `interactable`, `state`, `run_script`, `set_prompt` bindings in level JSON |
| Exact item **and** unheld return filter; enter and exit handlers | Filter chooser cannot compose exact identity with held policy; sheet edits the first event binding | Existing strict item filter plus two ordinary volume bindings |
| Fixed-prop light components, shared circuits and compatible Static-hold policy | No prop-light/circuit membership inspector | Existing `light`, `switch_group`, fixture `hold_in_static`; no mode code in engine |
| One shared physics reset group and fitted rack standable height | No reset-group text or per-axis collision-size inspector | Existing `reset_group`; rack support height follows the installed ball-support placement |
| Completion label capability and conditional visibility on the GUI-placed lock plate | The hand-only HUD does not draw authored prompt strings; label visibility needs an interactable capability even when the plate is not aimable | Disabled interactable capability plus existing world-label actions, hidden before restart |
| Narrow intentional open-plan/closed-door checker annotations | Architectural perimeter heuristic does not infer adjoining floor-material areas or the separate leaf | Check-specific `geometry_intent` rectangles, with reasons; no confirmed defect is suppressed |

Luau gameplay authoring is intentional, ordinary map scripting. Its source was
entered, checked and saved in the native Scripts sheet. It uses the shared
light/sound/state/door/physics/reset actions. Restart resets the complete item group
before the shared world reset, restoring item poses/ownership and fresh VM state.

## Reusable fixes made within this demo

The existing mounting command placed fixture backs 1 mm from their carrier,
inside the geometry checker's 1.5 mm coplanarity tolerance. An ordinary mount
therefore produced confirmed duplicate-surface findings. Mount clearance is now
2 mm. One small owning-editor regression checks a mounted panel against the real
geometry checker and also proves a genuinely coincident panel is still detected.
It uses two tiny CPU mesh checks, no GLBs, lighting solve or shipped map bake.

Native acceptance gained bounded `view` and `view_at` actions that change yaw/pitch only. It
preserves player position, body/stance, motion and item state. This permits a full
controls-driven playthrough without the diagnostic player-position action. The
existing benchmark opt-in, admission and action-count/time bounds remain active.
No Closing Time routine was added to the engine.

## Verification evidence

The authoritative local receipt and final native results are recorded in the
Closing Time handoff. Initial preview evidence is explicitly labeled Static and
pre-final; it is not final playthrough or throughput acceptance. Native inspection
caught and corrected a wall hiding the pin decks and copied dividers inheriting
the editor's deliberate sideways duplicate offset. Both corrections use the editor.

Strict Clippy, the locked all-feature release build, formatting, asset/PNG/GLB
checks and the small Rust regression suite are required gates. Full gameplay,
roundtrip, lighting/audio and measured performance acceptance are recorded after
those finish. No shipped-map bake is added to ordinary tests or CI.

## Discovered gaps for the later combined implementation/safety task

These are concrete inputs for the authorized later task, after Beam's three
passes, accepted asset integration and the editor readability pass. They are not
implemented here unless explicitly marked fixed. Replay both finished modes after
addressing them, then perform Luau/map-import safety checks.

| Reproduction / impact | Current workaround | Proposed correction and acceptance |
| --- | --- | --- |
| Select a wall switch, open Scripts, choose Link: only a selected trigger can link. There is no bool-state or binding-condition editor. Closing requires conditional final-door/restart behavior. | Source JSON bindings/state | Reusable component/event/condition authoring and script export linking for props and doors. Preserve IDs, validate references, undo/save/reopen/export exact behavior. |
| Trigger Filter can choose an exact physical item or unheld items, but cannot combine both. Only the first event binding is configured. A carried or subsequently removed item must invalidate its return. | Exact+unheld filter and enter/exit JSON bindings | Composable identity/held filter controls and a multi-event binding sheet. Native checks must cover held/unheld transitions, removal, fast crossings and reset. |
| Fixture sheet lacks circuit names; prop sheet lacks light controllers and attached-source controls. A cabinet bank and its ceiling fixtures need one switch. | Shared circuit/light-component JSON; matching hold policy | Expose existing circuit/controller settings with compatible-state checks and real source budgets; preserve unrelated maps and source extensions. |
| Select Static lighting and switch the fixed cabinet/fixture circuit: the existing authored-state hold keeps it lit. Closing's full atmosphere currently requires Full. | Use Full in the supplied native workflow | Support bounded prepared off states consistently for the needed fixed-prop/multi-circuit use, or enforce an explicit map presentation capability. Verify both modes and quality changes without a runtime bake or misleading dark faces over lit rooms. |
| Script `api.notice` is diagnostic output. Authored `set_prompt` strings are not drawn by the hand-only HUD. A valid `toggle_label` target without an interactable component is admitted but silently unavailable at runtime. | Disabled interactable capability on a GUI-placed entrance plate; conditional existing world-label actions | Provide bounded optional completion feedback and validate label-target capability at admission. Preserve minimal interface, reference checks and staged-effect rollback. |
| Physics UI exposes on/pickup/mass but not reset-group identity or a fitted fixed compound collider. The ball rack's decorative upper bound is above its actual ball support. | Shared-group JSON and fitted standable collision height | Expose group identity and existing bounded fixed-shape support, with preview and reset validation; verify real rack/basket/shelf placement without broad invisible boxes. |
| Installed audio has one HVAC ambience and generic clicks/door closures, without arcade standby, bowling motor shutdown or a distinct lock. | Separate spatial HVAC emitters; existing switch/door cue | Supply appropriate normalized installed loop/shutdown/lock cues and preview them in the sound chooser. Retain preloaded shared audio lifecycle and real post-mixer evidence. |
| `check-project --asset-root <valid root>` from the relocated direct payload still fails authored-audio admission unless `PLACES_ASSET_ROOT` is also set. | Explicit environment root in native tooling | Thread the explicit resolved resource root through semantic/audio validation; test a tiny emitter project from an unrelated working directory. No full bake needed. |
| Export a project after a semantic-only source edit: the exporter compiles into a fresh temporary location, so prepared-record reuse cannot see the previous package and repeats the full lighting bake (about six minutes here). | Freeze semantics before final Full export; use Static previews during fitting | Seed reusable prepared records only after exact source/asset/signature validation, retaining atomic publication and source-change rejection. |
| Safety on this 85-prop scene retains seven floor-contact coplanarity reviews and reports the model-pair candidate budget. The entrance drop heuristic does not include the closed leaf. | Review actual native contacts/escape attempts; preserve narrow geometry intent | Distinguish intentional floor contacts and doors, prioritize nearby candidate pairs and report incomplete coverage honestly. Do not float scenery or weaken real duplicate checks. |
| Direct SDL windows are absent from the desktop app inventory. Native editor verification can operate them, but normal desktop automation cannot select them. | Existing native driver plus real OS video recording | Treat this as an environment/automation integration gap, separate from editor functionality. Retain direct-binary support. |
| At 1440×816, the fixed low-resolution bitmap UI and four-row scene list make larger authored scenes slow to inspect and small labels difficult to read. | Visible scene scrolling and numeric entry; full native captures retained | The separately requested readability pass should address sharp text, button sizing/contrast and physical/logical display scaling; add practical object search/selection while keeping the workspace uncluttered. |
| Mounting a fixture produced duplicate-surface errors even after Mount surface. | **Fixed here:** 2 mm mount clearance | Keep the new tiny cross-check and final native mount/check acceptance. Do not weaken coplanarity validation or clear intentional ignores globally. |

Optional, unrelated expansion remains outside this task: traffic reservations,
new AI encounters, new campaigns, broad asset authoring or cinematic systems.
This mode intentionally has no NPC/AI requirement.
