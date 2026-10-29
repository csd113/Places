# Luau scripting

The production profile requires `panic = "unwind"` for Luau’s C++ exception
boundary; incompatible abort profiles fail compilation. The release-binary
refusal check in `tools/verify-luau-release.py` covers this separately from Cargo
tests, whose harness always uses unwind.

The shared engine runs project-local Luau through `mlua` 0.12.2 / Luau 0.740.
Scripts produce existing `ActionDef` actions; doors, lighting, audio, physics,
spawns, entity state and triggers keep their existing implementations and limits.
No game mode or model-specific routine is embedded in the engine.

Open **Scripts >**, choose **+ New module** or an existing module, and edit its
source in the native sheet. Enter/Tab insert text; arrows/Home/End move the caret;
wheel scrolls. Cmd/Ctrl A selects all, C/V copy/paste, Z/Shift-Z (or Y) undo/redo
up to 16 source edits. Source history is separate from scene history. Save and
Cmd/Ctrl S save the visited map/script buffers together. Closing a sheet retains
its buffer; project close/open/new uses the ordinary unsaved-change guard.
External file changes refuse saving before publication. Each file is atomic;
a disk failure during a multi-file save may leave a successfully saved subset.
At most 64 script buffers are retained per open project; save/reopen releases them.

**Check** checks current buffers, bounded top-level initialization and linked
export names. Invalid source can still be saved as a draft. Check does not run
handlers or perform static type analysis. Errors show path/line and a bounded
trace in **Diagnostics**; the last Play messages also appear there after returning.
The source sheet is a small native editor, without an LSP, arbitrary text
selection, syntax colouring or filesystem browser. Files may also be edited
externally, then the project reopened.

Select a trigger in the scene, configure its enter/exit, once/cooldown and actor
policy, choose **Actions > / Script handler >**, open a module, then **Link
handler >**. The paged chooser lists exported functions. The native trigger sheet retains
its existing eight-action limit and refuses additional links with a diagnostic. It adds an ordinary
ordered trigger action, removable through the existing Actions sheet. A source
reference is a portable `scripts/*.luau` path, never an absolute/native path.
Named `run_script` actions also work in existing entity/door/timer/effect bindings
and sequence action steps; those other event links are currently source-authored.

```json
{
  "scripts": ["scripts/entry.luau"],
  "volumes": [{
    "id": "threshold", "x": 1, "z": 1, "width": 1, "depth": 1,
    "bindings": [{"on": "enter_volume", "once": true,
      "actions": [{"action": "run_script",
                   "script": "scripts/entry.luau", "handler": "entered"}]}]
  }]
}
```

This is a fragment of an ordinary level. Project-wide `scripts` and the selected
manifest level's `scripts` also load, followed by the level JSON's references,
with identical paths deduplicated in first-declared order. Each module returns
a plain table of named functions. Export names contain 1–64 ASCII letters,
digits or underscores. Export tables and the supplied API/event tables are
read-only. Exports cannot have a metatable. Modules have separate globals and
closure state; `require` and dynamic handler registration are unavailable.

```luau
--!strict
local visits: number = 0
return {
    entered = function(api, event)
        visits += 1
        api.door("exit_door", "open")
        api.light("ceiling_lamp", false)
        api.sound("ambience", true)
        api.notice("Threshold visits: " .. tostring(visits))
    end,
}
```

The example requires corresponding authored entities and a switchable light.
Use [places.d.luau](places.d.luau) as the host type reference. Runtime argument
checks remain authoritative; `--!strict` is accepted Luau syntax, not a promise
that the editor performs Luau typechecking.

| API | Behavior |
| --- | --- |
| `api.player()` | Read-only current-tick player feet `{x,y,z}`; no native handle |
| `api.actor(id)` | Read-only entity transform plus `illuminated` (including release grace), raw `intensity`, and current collision-checked `player_visible` for AI bodies |
| `api.ai_task(id,key,request)` | Queues shared task data as a plain table or JSON string; nil cancels that key; see the shared contracts in MAP_AUTHORING_GUIDE section 33.8 |
| `api.ai_pause(id,key,paused)` | Suspends/resumes the named continuation and its clock |
| `api.ai_senses(id,rules)` | Replaces at most sixteen typed hearing/sight rules with mapped responses |
| `api.navigate(id,x,y,z)` | Queues ordinary AI navigation; finite coordinates within ±100000 m, AI body required; shared body classes, routing budgets, live doors and physical obstacles still apply |
| `api.message(title,lines)` | Queued subtitles: printable speaker/title ≤80 bytes and a dense plain array of 1–6 printable lines ≤72 bytes each. Enter/PgDown advances, PgUp rereads, Backspace skips; explicit F1 help retains its dismissible guidance panel |
| `api.finish(won,title,detail)` | Ends the run with printable title/detail ≤80/160 bytes; Enter restarts world/VM/player and authored grouped items, Escape returns to menu |
| `api.exists(id: string): boolean` | Tests a currently live entity identity |
| `api.door_status(id)` | Read-only invocation-start hinged-leaf snapshot: actual `phase`, `locked`, `obstructed` and signed `angle`; a pending close/lock request is not proof of closure |
| `api.prompt(id,text)` | Queues the existing interaction-prompt action; printable nonempty text at most 96 bytes; requires a live interactable |
| `api.door(id, "open" / "close" / "toggle" / "lock" / "unlock")` | Uses shared door actions; trigger origin retains away-facing two-way opening |
| `api.light(id, on: boolean)` | Requires a switchable light; ordinary prepared lighting limits apply |
| `api.sound(id, playing: boolean)` | Starts/stops an existing emitter using its authored cue and loop policy |
| `api.physics(group, activate: boolean)` | True activates, false restores an authored movable physics group |
| `api.ragdoll(id, enabled: boolean, vx?, vy?, vz?)` | Queues the shared articulated mannequin handoff or authored reset; optional finite additive world velocity ≤15 m/s; unsupported rigs are refused by native rig validation |
| `api.spawn(point)` | Uses that point's template/group, existing spawn/lifetime/render budgets |
| `api.state(id, key)` | Reads an existing declared bool/integer/float/text state |
| `api.set_state(id, key, value)` | Stages a same-type write to a declared slot; cannot grow state bags |
| `api.change_level(id)` | Requests a declared same-package destination from this level’s `transitions`; requires complete project export |
| `api.reset()` | Uses the shared reset-to-start action, recreates VM state and cancels pending navigation |
| `api.notice(text)` | Bounded diagnostic, retained/logged with `[luau]` |

Identity/key/group arguments are nonempty printable UTF-8, at most 96 bytes;
notices and text state values at most 256 bytes. Boolean arguments must be actual
booleans. Navigation coordinates require actual numbers, without numeric-string
coercion. Integer writes are finite integral numbers within ±2,147,483,647;
float writes are finite within ±1,000,000. Queries see world state at invocation
start. Effects are queued in order, published after success, and become visible
to the next invocation. A host argument/reference error, script error or resource
failure discards the entire current invocation's staged effects and disables
that module for the remainder of the run. Earlier successful invocations remain.
Shared dispatcher refusals (such as a full spawn group) retain existing semantics.

All handlers receive `(api, event)`. `event.subject` is the binding owner, and
`event.kind`, `event.key`, `event.actor` describe the current shared event when
available. `event.x/y/z` are the triggering occupant origin when known; `dt` is
zero for linked action calls. Missing context is nil. Optional `start` runs once
on the first playing simulation tick; optional `update` runs each playing tick
with `event.dt` in seconds. Automatic calls have no subject/event kind/origin.
They stop while simulation is paused. An optional `hint` export runs only on a
fresh F1 press during active play, through the same scoped invocation and action
budgets. F1 is reserved by the native shell. Messages from `start` and ordinary
events use the same non-blocking subtitle queue as NPC speech. Only explicit F1
guidance pauses movement, AI, physics and world audio until Enter/Escape.
Subtitle reading time pauses with gameplay, F1, focus loss, loading and the
console; map changes, replay and run completion clear speech. Identical
consecutive queued messages coalesce; different messages retain their order.
This API defines no dialogue choices. Do not store the API: its scoped Rust
callbacks expire after each invocation. Store ordinary Luau values instead.

A fresh map load owns fresh VMs. Reset/replay drops VMs, closures, globals,
scoped callbacks, diagnostics and disabled status, restores authored world state,
and re-runs initialization; `start` runs again on the next playing tick. Changing
levels drops the previous runtime. Editing source during Play affects the next
save/build/play, never the running package; there is no live hot reload.

| Resource | Hard admission bound / cooperative execution limit |
| --- | --- |
| Sources | 16 modules/map; 1–64 KiB/module; 512 KiB total UTF-8 |
| VM memory | 2 MiB/module, including its Luau heap/bytecode/host tables; at most 32 MiB admitted VM heaps |
| Initialization | 10 ms/module and 20,000 VM interrupts |
| Handler | 2 ms and 20,000 interrupts; 256 native host calls including queries; 32 staged actions; 4 notices |
| Simulation frame | 64 invocations, 128 published script actions, 8 ms cumulative invocation time |
| Diagnostics | 32 messages/run, 512 printable characters/message |
| Exports | 64 functions/module; no subscriptions or dynamically created event types |

Time is checked cooperatively at Luau calls/loop interrupts, not a hard realtime
scheduler. Initialization timing begins after text compilation; handler timing
begins after scoped capability/event setup. Setup still counts toward cumulative
frame time. All host capabilities, including read-only queries, share the hard
native-call count and fail the invocation atomically if it is exhausted.
Source length also bounds compiler input; the compiler's temporary native
allocations are outside the VM allocator. Ordinary bounded native library
calls can briefly exceed the requested wall deadline. Native pattern/substring search
operations (`find`, `match`, `gmatch`, `gsub`, `split`) are omitted because worst-case native
search work can evade VM interrupts. `table.move` retains ordinary source,
overlap and optional-target behavior, but requires actual integral 32-bit numeric
indices and moves at most 8192 elements per call. Sparse negative-index moves
otherwise consume native time without growing the bounded Lua heap or checking
VM interrupts. Table/string/math/UTF-8 basics remain available.

No filesystem, network, OS, package loading, FFI, arbitrary native calls, debug
library, bytecode loading, dynamic source loading, coroutines or protected calls
are available. Removing `pcall`/`xpcall` prevents scripts catching and retrying
execution-budget errors. Luau sandboxing freezes standard libraries/metatables.
Only Rust creates scoped capabilities; no raw pointers or runtime handles cross
the boundary. Future APIs must preserve these limits and reuse shared actions.

New scripted packages require `luau-v1` and `scene-actions-v1`, so older players refuse the expanded host explicitly. Original `luau-v1` packages remain supported. This rejects unsupported guidance/scene calls before play, rather than allowing a missing host function to disable the module. Players without `luau-v1` refuse original scripts
explicitly. Project builds snapshot source into this selected map's `script_sources` and
include the exact bytes in compiler identity. Edits to Luau source files reuse the
existing prepared geometry/lighting while refreshing semantics and package identity. Inline `script_sources` in an
authoring project is refused. The player loads embedded UTF-8 and never opens
project script paths. This supplies the current single-level Build/Play loop;
complete multi-level project export/import, thumbnails and roundtrip distribution
remain stage 7. Custom model/texture/sound importing and model integration remain
outside this stage.

The sandbox follows the upstream [mlua API](https://docs.rs/mlua/0.12.2/mlua/struct.Lua.html)
and [Luau embedding guidance](https://luau.org/sandbox/). Focused CPU regressions
live under the owning engine/editor/compiler tests directories; native evidence
and measured limitations belong in the [stage 6 report](reports/release-0.12.0.md#editor-0120-task6).

Actor queries also expose the authored navigation body's `radius` and `height`,
or nil without a navigation body. These are physical clearance dimensions, not
posed-mesh bounds; authors must add a conservative allowance for animated limbs
when proving whole-body room containment. New script exports declare
`world-observations-v1`, so older hosts reject them before play.

Actor queries use the previous rendered frame's beam observations and current
simulation-tick collision visibility. `player_visible` checks static walls,
live door leaves and physical-item bounds; it is not a renderer visibility
oracle. Map attack rules must also check distance and vertical separation.
Navigation goals never teleport an actor. Repeated goals within 25 cm retain
the current route/retry state. `light_response.reveal_only` hides a body when
raw direct illumination falls below its exit threshold, while the independent
release hold may continue to pause movement. Default behavior is unchanged.

`illuminated` and `intensity` describe an authored `light_response` component;
without that component they return false and zero. The component requires the
existing AI/navigation body. A shadow-only mesh can cast its flashlight shadow
without this response component, but a script that checks whether the beam has
reached it must author that capability separately.

`api.actor(id).shadow_only` reports an entity's authored mesh policy independently
of `illuminated` and `player_visible`. `api.spawn(point)` preserves the selected
`spawn_templates[].shadow_only` from its first rendered frame. Scripts still
control animation, navigation, sound and interaction through the existing APIs;
mesh invisibility changes none of those capabilities or `solid` collision.
There is no second runtime visibility setter. See the [shadow-only map contract](MAP_AUTHORING_GUIDE.md#shadow-only-entities-0120).

### Authored item handoff and hidden retirement

A navigating actor may author one `carry` component. The editor's **Capabilities
> NPC item handoff** form exposes its exact requested item, animated node, local
metre offset, local XYZ Euler degrees and idle/walk clips; **+ Carry** chooses
an existing pickup item as the initial reference. The actor needs `ai` and
`nav_agent`. The item must be an authored movable pickup prop. Instance renaming
updates this typed reference before scripts are linked; scripted maps retain the
existing stable-identity guard.

```json
{"component":"carry","item":"requested_cushion","socket":"hand.R",
 "offset":[0,0,0],"rotation_degrees":[0,0,0],
 "idle_clip":"carry_idle","walk_clip":"carry_walk"}
```

The approved R2 mannequin provides the named hand and carry clips above. Fit the
item's local attachment in native Play; its actual model/PNG and authored scale
remain unchanged. This is a rigid posed-node grasp controller, without runtime
IK or two-hand constraint solving. The approved finger animation supplies the
hand pose; the original physical prop follows that posed node every drawn frame.

`api.held_item()` returns the player's exact authored item identity or nil.
`api.handoff(actor)` queues acceptance of that actor's authored requested item.
It succeeds only if the exact body is currently held, the actor is the ordinary
reachable aimed interaction target, and the live socket, clips and original
render resource resolve. A wrong object, distant invocation or repeat changes
nothing. The next invocation's `api.actor(actor).carried_item` confirms the
accepted identity; show thanks and begin navigation only after that confirmation.
Queries remain invocation-start snapshots. Plain actions
`{"action":"handoff_item","target":"caretaker"}` use the same transaction.

Acceptance removes player/physics ownership, cancels stale drop/throw input and
retains the original rendered prop. Pickup, physics recovery and activation
cannot reclaim it. Reset/activation of a physics group containing an NPC-owned
item refuses the entire group with `physics_group_owned_refusal`, including after
retirement. Use the full map reset/reentry lifecycle to restore the quest, item,
actor interaction, navigation and fresh script state together. Same-map reset
reinstalls only transferred bodies at their original authored dormant poses and
reuses their existing render keys; unrelated existing/deleted items retain their
state.

At the departure destination, call `api.retire_hidden(actor)` once (or use the
`retire_hidden` action). Retirement pauses with gameplay. The proof uses the
actual frame camera, including a third-person camera or native camera override.
The request waits until one real opaque uncut wall face
provably covers the projection of the **combined current posed mesh and carried
prop bounds**. Every vertex of both actual resources contributes to those
bounds, with a 35 mm safety expansion. Coverage of all eight bound corners by a
single rectangular face proves coverage of the convex bound; separately blocked
sample rays or a hidden centre do not. Only explicit-height `walls` with no
openings and opaque resolved materials qualify. Candidates overlapping a
nonopaque wall are excluded: the wall renderer can give a later coincident
member ownership of the emitted face. Collision-only containment,
prop approximation boxes, transparent/cutout walls, doorway envelopes and merely
looking away cannot justify retirement. Author a simple solid-wall bend, with
enough room for the complete carried silhouette to pass it. If the player follows
and keeps any limb/prop exposed, the actor waits at its destination and both
meshes remain present until the proof succeeds. The conservative guard may keep
a fully occluded actor longer where several faces jointly hide it; it does not
relax the disappearance invariant to infer an answer.

Successful retirement hides both actual resources in colour, reflection and
shadow passes before the current draw, freezes the retained actor, disables its
interaction and sets `api.actor(actor).retired`. It does not remove the authored
actor, so reset restores it. A graphics-only rebuild retains its visibility.
Newly built scripted packages and packages using this component require
`carry-assemblies-v1`; older players refuse them explicitly instead of failing
a missing expanded-host function during play. Existing compiled packages retain
their original capability requirements.

Opt-in native traces record `npc_item_handoff`, `npc_retired_hidden` and
`carry_assemblies`: item/key/socket ownership, actual `item_mesh_visible`, combined
bounds (`character_bounds`, `item_bounds`, and their `assembly_bounds` union),
retirement request/state and `whole_assembly_hidden`. Existing character
animation diagnostics independently report actual actor mesh/shadow presence.
The older `player_visible` torso query retains its existing perception meaning;
it never decides carry retirement.

### Explicit performances and physical putdown (0.13.0)

`api.animate(id, clip, looped)` and `api.toggle_animation(id, clip)` stage the
existing typed animation actions. The target must own an animation component.
An AI actor yields locomotion to an explicit performance; one-shot completion,
a new navigation goal, despawn or reset releases that ownership. A carried prop
continues to follow its real posed hand while pickup/putdown clips play.

`api.release_item(actor)` returns only that actor's original carried body at its
current socket pose, retaining its render key, scale, physics profile and original
authored reset destination. Repeats and actors without ownership do nothing.
The map must coordinate the action with the actual contact phase of its clip;
there is no runtime IK or inferred surface placement. Reset and unload cancel
pending performance state. Newly compiled scripts require `script-animation-v1`.

The non-playable bundled menu also permits an actor to collect its own requested
movable item with `api.handoff(actor)` when the actual body is within 1.7 metres.
This path is confined to the running application-owned menu world; gameplay
retains the exact player-held item and aimed-target handoff contract above.
`projects/menu/scripts/menu.luau` authors a single bounded delayed contact action
between its five-second beats, without retaining the scoped host API.

### Articulated ownership

`api.ragdoll("actor", true, 1.5, 0, -0.4)` transfers the currently displayed
mannequin skeleton to the ordinary Rapier world. The renderer retains its mesh,
materials, skin weights and normal/lighting path. AI, movement, sequences and
animation stop owning the actor. Its script position then follows the live
pelvis, and a carried item returns to physical simulation at its current socket.
A failed item release cancels the transfer without changing the displayed pose.

`api.ragdoll("actor", false)` retires all bone bodies/joints and restarts authored
animation/AI at the authored placement. It does not perform a stand-up animation.
Released items remain ordinary items; whole-map reload restores authored item
placement. Calls are queued and use invocation-start observations, like other
host capabilities. Pause/time scale and single-step share the existing world
clock. Eight skeletons and 128 shared physical item/bone bodies bound allocation.
`projects/ragdoll-lab` and `tools/bench/ragdoll.py` provide editable/native examples.

### Shared AI authoring

Tables and JSON use the same serialized TaskRequest/SenseRule contract as
`ai.tasks`, `ai.senses` and shared actions. The key argument must equal
`request.key`. A request has priority 0..200 and a `definition`; pass nil
to cancel it. All three APIs require a live AI body. `api.actor(id)` also
returns optional `ai_owner`, `ai_task`, `ai_phase`, `ai_result` snapshots.

```luau
api.ai_task("porter", "delivery", {
    key = "delivery", priority = 80,
    definition = {task = "fetch", item = {match = "id", value = "parcel"},
        search_radius = 20, destination = {target = "shelf", kind = "shelf",
            approach = {2, 0, 3}, contact = {2.1, 0.2, 3.3}, reach = 1.5}},
})
api.ai_pause("porter", "delivery", true)
api.ai_pause("porter", "delivery", false)
-- api.ai_task("porter", "delivery", nil) cancels and returns cargo to physics.
```

Use dense plain arrays/maps with no metatables, functions, cycles or sparse
keys. Each conversion is bounded to 512 nodes, depth 12, 32 entries per table
and 16 KiB JSON. Empty arrays for named list fields are accepted. Handler
effect atomicity and existing VM budgets still apply. Configure shared engine
behaviours here; keep quest conditions, notices and authored timing in Luau.
See MAP_AUTHORING_GUIDE section 33.8 for contact, recovery, filters and bounds.


### Native template and visual ownership

The native editor's event forms generate ordinary modules with `start` or
`handle_event`. They use the same `api.message`, `api.door`, `api.sound`,
`api.ai_task`, `api.ai_pause` and `api.ai_senses` contracts documented above.
Creation always uses a new filename; regeneration cannot overwrite edited code.
The visual behavior composer serializes TaskRequest/SenseRule directly, or
emits readable Luau tables for those same shared APIs. Its cancellation handler
removes its keyed tasks and senses if the NPC still exists. A custom module owns
execution until the author explicitly selects Use visual. Source files and
buffers remain retained when execution ownership changes. See
[the editor workflow](EDITOR.md#event-templates-and-behavior-blocks).

## Gameplay checkpoints (0.13.1)

`api.checkpoint(name)` and `api.resume_checkpoint()` queue current-level shared
actions. Levels opt in with `saving: true`; every module must supply bounded
`checkpoint_state()` / `restore_state(saved)` hooks. See [checkpoint authoring,
supported state and limits](CHECKPOINTS.md). Existing levels remain disabled.
