# Gameplay checkpoints

Gameplay saving is an explicit **per-level** option. `"saving": true` enables
local checkpoint writes and Continue. Omission and `false` disable both. Existing
maps keep fresh-run behavior; no shipped map is silently enabled. This is separate
from editor document saving and the existing collection-favor completion record.
Collection completion remains monotonic campaign progress; resuming a visit does
not revoke favors earned later or in other levels.

## Authoring

In **Project**, select the level and change **Gameplay saves: Off / On**.
In **Add to scene**, choose **Checkpoint volume**. The amber volume is an ordinary
trigger: select it, move/resize it with existing geometry controls, and edit its
**Checkpoint name** in Gameplay. It starts with player entry, a two-second
cooldown, and repeat entry enabled. Standing inside does not retrigger it. The
existing Events/actions sheet controls once, cooldown, conditions and removal.
**Use as checkpoint** adds a checkpoint action to a selected existing trigger.
The independent level setting must still be enabled. Undo/redo, source saving,
reopening, export, build and Play use the existing document paths.

Source authors can add the same action to an ordinary event or sequence:

```json
{"action":"save_checkpoint","name":"Reception"}
```

`{"action":"resume_checkpoint"}` requests the current level's compatible save.
Scripts use `api.checkpoint("Reception")` and `api.resume_checkpoint()`.
These queue existing shared actions. They cannot choose a filesystem path, slot,
package, script or arbitrary destination. Disabled levels never write a gameplay
checkpoint and cannot resume one. Refusals use CHECKPOINT subtitles and logs.
Multiple checkpoint actions in a frame coalesce to the last name. A continuous
script request for the same name writes once until at least one frame without
that request. Re-entering an ordinary checkpoint can update it later.

## One checkpoint and explicit fresh starts

Each installed project/level has one latest checkpoint and one recovery copy.
Selecting a level in Explore with a compatible checkpoint offers **Continue checkpoint**,
**Start new**, and **Back**. Invalid or changed-content saves explain why Continue
is unavailable and retain Start new. Start new begins the authored world and
scripts; the prior save is retained until another successful checkpoint. Pausing,
quitting and changing maps do not implicitly save. The last completed checkpoint
survives process exit. No manual slot browser or cloud service is introduced.

Resume uses the real asynchronous loader and loading progress. Reconstruction
happens in a private candidate before GPU admission and world replacement.
Canceling or failing preparation leaves the current visit and checkpoint intact.
An in-flight content change fails admission. A graphics-only rebuild retains the
visited runtime without applying a disk snapshot again.

## Supported persisted state

The format contains semantic values and stable source identities, never raw entity,
renderer, physics or VM handles. It restores:

- Grounded human player feet, yaw, pitch, standing/crouched stance and flashlight.
  Movement momentum and transient input latches restart at rest. The destination
  must fit the restored collision world and remain inside playable space.
- Authored entities' transforms, visibility/collision flags, typed declared state
  bags, interaction enablement/prompts/labels, light state and circuit power,
  material variants, emitter/effect/water enablement, fade state and glow state.
- Hinged door angles, chosen swing, phase, lock and obstruction/reversal state;
  timer countdowns/overrides/fire counts; route position/step/clock; authored
  binding once flags and cooldowns; simulation clock and pose selections.
- Authored rigid item poses, velocities, dormant/sleeping state and held ownership.
  New solver/render keys are resolved from the saved authored item IDs. The hold
  controller replans from the saved pose; contact caches are rebuilt by the solver.
- Explicit script closure data through the hooks below. Module started/disabled
  status is retained. Startup actions and one-shot sounds are not replayed; desired
  looping audio and existing effects publish once after installation. Trigger
  occupancy is reseeded from restored player/items without synthetic entry events.

This is a bounded semantic checkpoint, not an opaque memory dump. Cosmetic looping
animation/audio phases restart. Active and queued subtitles are cleared on resume. No separate health,
weapon inventory or dialogue-choice system exists in the current game; none is
invented by the save format. Physical held objects are the supported held items.

Capture refuses active event tails, object moves, sequences, one-shot animation,
AI, live spawns/despawns, lifetime components, balloons, articulated actors, clip-driven compound doors, transferred
carry assemblies, alternate/cycling weather, session-created developer props,
swimming/climbing/airborne players, transformed/noclip players and modified world
time. A refusal retains the prior valid save and explains the unsupported state.
Authors must choose supported stable checkpoint locations and validate Play;
enabling saving does not make unsupported gameplay replayable.

## Luau closure hooks

Every module in a saving-enabled level must export both hooks. Check/Build rejects
missing hooks. `checkpoint_state()` returns a plain string-keyed table containing
at most 128 bool/number/text values; keys are printable 1–64 bytes, text at most
256 bytes, integers within ±2147483647 and double-precision floats within ±1000000. Nested tables,
functions, userdata, paths and executable VM state are unavailable. Flatten small
arrays into named keys. Hooks receive no gameplay API. Each has the existing
bounded VM memory and a two-millisecond execution budget.

```luau
local visits = 0
return {
    entered = function(api)
        visits += 1
        api.set_state("quest", "visits", visits)
        api.checkpoint("Reception")
    end,
    checkpoint_state = function()
        return { visits = visits }
    end,
    restore_state = function(saved)
        if type(saved.visits) ~= "number" then error("Invalid visit state") end
        visits = saved.visits
    end,
}
```

`checkpoint_state()` must return current values without mutating gameplay locals.
Normal scripted map transitions start a fresh visit; a level author can explicitly
request its checkpoint with `api.resume_checkpoint()` after arrival.

`restore_state(saved)` receives read-only validated scalar data in a newly
initialized VM. It must restore every mutable local/global affecting gameplay and
validate its own semantic values. Scripts using randomness must retain their own
seed/counter and reconstruct the generator; the VM random stream is not persisted. It must not replay startup effects. Top-level
module initialization remains sandboxed source from the verified package. The
engine cannot inspect arbitrary Luau closure meaning; missing/failing hooks refuse
capture or resume instead of pretending globals survived. The maintained Office
quest demonstrates flattened local-state hooks while its level remains disabled
by default.

## Storage, identity and recovery

Saves live below the existing user state root's `saves/` directory. The slot key
hashes declared project identity, canonical installed package path and level ID;
different projects, installations and levels cannot inherit each other's saves.
Moving/reinstalling a package intentionally creates a distinct installation scope.
The loaded visit retains its original declared project identity. Replacing a package
during play refuses subsequent checkpoint capture until reload, even when another
project reuses identical authored level source; it cannot adopt that project's slot.
The envelope also requires the exact current canonical package manifest identity
and save version. Changed content or versions offer a fresh start, without unsafe
compatibility guesses. New checkpoint packages declare `gameplay-checkpoints-v1`.

Reads stop at 4 MiB, reject unknown envelope fields, verify payload SHA-256 and
validate bounded state, IDs and source component shapes. Saved content cannot
select assets, load script code or escape the save directory. Writes create a
unique temporary file, flush it, retain a validated previous copy, and atomically
replace the primary. A failed backup/replacement keeps the old primary. An
interrupted temporary file is ignored. A corrupt/truncated primary can recover
only a matching valid previous copy; two invalid copies require Start new.

Validation uses isolated state roots and copied fixtures. Never erase personal
saves to test this feature. Full production package regeneration is an explicit
release operation after source work; ordinary tests do not bake maps.

Checkpoint requests wait up to two simulation seconds for deferred event tails
to drain, so a handler can update declared state and request a checkpoint together.
Pause, reset, outcome and load boundaries discard pending requests. Active moves,
sequences and other unsupported states still refuse rather than capture halfway.
