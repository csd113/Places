# Optional gameplay checkpoints (Windows, 0.13.1)

The implementation adds one local gameplay checkpoint per installed project/level.
`saving` defaults to false; existing shipped levels retain fresh visits. Editor
document saving and monotonic campaign completion remain their existing systems.
See [the authoring and persistence contract](../CHECKPOINTS.md).

Project exposes Gameplay saves, Add to scene exposes Checkpoint volume, and a
selected trigger exposes Use as checkpoint and its editable checkpoint name.
Checkpoint volumes use ordinary source IDs, geometry, event actions and history.
Luau uses the existing sandbox/API: `api.checkpoint(name)` and
`api.resume_checkpoint()`. Saving-enabled scripts must supply scalar closure
capture/restore hooks. The maintained Office quest demonstrates those hooks,
without enabling saving on its production level.

The semantic snapshot reconstructs player pose/view/stance/flashlight, authored
rigid items and held ownership, entity visibility/collision/interaction/declared
and supported runtime state, hinged doors, lights/circuits, timers, routes,
binding once/cooldown state, simulation clock and explicit script variables.
It resolves new engine keys from stable authored IDs, clears initialization
events, and reseeds trigger occupancy. Startup effects and item ownership are
not replayed. Cosmetic animation/audio phases restart; active and queued subtitles clear on resume. Player movement
momentum starts at rest. The engine has no separate weapon/health inventory or
dialogue-choice state to serialize.

Unsupported states refuse capture and retain the prior save: AI, live spawns or
despawns, articulated/compound doors and actors, transferred carry assemblies,
lifetimes/balloons, active moves/sequences/one-shot animations, nondefault weather,
developer/session items or modified time, and airborne/swimming/climbing/alternate
player forms. These are explicit limits, not claims of full replay. Script authors
must retain their gameplay locals and any random seed/counter in the hooks.

Save paths are hashed from declared project, canonical installation and level
identity. The envelope pins the exact package manifest and format version.
Reads are bounded to 4 MiB, verify payload SHA-256 and reject invalid values,
unknown members and mismatched identities/shapes. Payloads cannot select paths,
assets or executable code. Atomic temporary-file publication and a validated
previous copy preserve the last valid checkpoint on backup/write failure;
truncated primary recovery never promotes unfinished temporary data.

Continue appears through the existing Explore entry flow, with Start new and
Back. Disabled levels bypass the offer and reject script checkpoint/resume calls.
Starting new retains the old save until another successful checkpoint. Pause,
quit and switching maps do not implicitly save. Resume reconstruction uses a
private candidate with the cancellable loader; failed/canceled work retains the
live visit and disk checkpoint. Graphics-only reload retains the live runtime.

## Validation record

Final formatting, strict workspace/all-target/all-feature Clippy, all 390 Rust
regressions and the locked release workspace/all-feature build pass. Production
profile Clippy and the actual abort-profile Luau boundary verifier pass, including
maintained Environments script initialization. The 371 Python source tests pass
(18 expected skips); 674 catalog assets have zero warnings. Texture/prop checks
pass with their existing art advisories. Rust runs use one test thread after an
earlier concurrent compile exposed the existing two-millisecond handler-budget
test's scheduling sensitivity; no budget, assertion or lint was weakened.

Final native Windows acceptance passes for placed and scripted checkpoints, real
process exit/relaunch, held ownership, declared state and script continuation with
startup once, light/door/player/item values at the exact commit boundary, nondefault
view and flashlight, repeated activation without continuous retrigger, graphics
rebuilds, independent levels and explicit fresh map switches. Canceling and failing
resume preserve live progress and disk saves. A blocked recovery-copy replacement
preserves the primary hash; truncated primary recovery, two corrupt copies and
changed content offer the correct recovery/fresh path. Rebuilding an enabled level
as disabled removes Continue and refuses writes while retaining its previous save.
Replacing the package under an active visit with a different project using the
same level source refuses capture and preserves both projects' save hashes.

Native editor acceptance covers the visible per-level switch, checkpoint creation,
name, undo/redo, save/reopen, wide/tall layouts, project build/validation and actual
checkpoint activation in Play. The player uses the existing stable editor Play
profile below its isolated cache. Its checksum-verified save names the authored
trigger and edited checkpoint; the capture visibly reads `Checkpoint saved:
Editor reception`. The shared editor/compiler/import/player diagnostic trace had
multiple writers and is excluded from acceptance. The actual gameplay save,
capture, normal Play return, source history/reopen and project validation provide
independent evidence. Failed controller attempts remain retained.

The maintained Office script and actual paper profiles run in a tiny checkpoint
witness with quest activation retained. The retained full shipped Office package
also loads and displays the Night Archivist subtitles with saving disabled and
no gameplay save. This is not a claim that its AI or item-transfer state can be
checkpointed; those remain explicit unsupported states.

Actual pixels were inspected for Continue/fresh error choices, Project setting,
checkpoint tool placement and amber volume, editor Play notice, flashlight notice
and shipped Office dialogue. Early overlapping creation controls and Explore-card
drawing in the Continue modal were corrected before final acceptance. These are
correctness sessions with V-sync, not FPS comparisons. Background compilers stay
BelowNormal and native applications Normal; no hardware/security settings change.

Evidence is outside target at `local/verification-0.13.1-checkpoints/`, using copied
tiny projects, isolated profiles and retained production packages; personal saves
are never touched. `validation-receipt.json` indexes final gates, accepted case
assertions, binaries and captures. `custody.json` records source commit, exact
remote SHA/CI and final idle cleanup. Prior base `67397f19` has successful exact
CI run 40852575519. Native Mac validation remains explicitly deferred.

Four focused Rust regressions cover reconstruction/closure and held ownership,
real atomic storage/recovery/bounds, and editor source/history persistence. They
use tiny CPU fixtures and no full-map bake. Native acceptance covers actual
Windows production executables and real process exit/relaunch. Initial failed
attempts remain in the evidence tree and are excluded from acceptance.

## Package currency and scope

This changes the conservative compiler engine-source fingerprint and adds the
`gameplay-checkpoints-v1` package capability. Previous completed production
packages and hash receipts are preserved. Older disabled packages remain admitted
by integrity/runtime loading; require-current reports source currency honestly.
Only the checkpoint fixtures are freshly baked for this task. Full production
package regeneration belongs after the remaining queued source work, rather than
repeated whole-project baking. No compatibility migration layer is introduced.

Mac validation remains deferred at Connor's explicit request. This task does not
implement the queued FPS optimizations or Hub wall-sign editor. Main/v0.13.0,
public releases/tags, and uploaded distribution ZIPs are outside this change.
