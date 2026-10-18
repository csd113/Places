# Luau scripting (0.12.0 stage 6)

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
| `api.exists(id: string): boolean` | Tests a currently live entity identity |
| `api.door(id, "open" / "close" / "toggle" / "lock" / "unlock")` | Uses shared door actions; trigger origin retains away-facing two-way opening |
| `api.light(id, on: boolean)` | Requires a switchable light; ordinary prepared lighting limits apply |
| `api.sound(id, playing: boolean)` | Starts/stops an existing emitter using its authored cue and loop policy |
| `api.physics(group, activate: boolean)` | True activates, false restores an authored movable physics group |
| `api.spawn(point)` | Uses that point's template/group, existing spawn/lifetime/render budgets |
| `api.state(id, key)` | Reads an existing declared bool/integer/float/text state |
| `api.set_state(id, key, value)` | Stages a same-type write to a declared slot; cannot grow state bags |
| `api.change_level(id)` | Requests a declared same-package destination from this level’s `transitions`; requires complete project export |
| `api.reset()` | Uses the shared reset-to-start action, recreates VM state and cancels pending navigation |
| `api.notice(text)` | Bounded diagnostic, retained/logged with `[luau]` |

Identity/key/group arguments are nonempty printable UTF-8, at most 96 bytes;
notices and text state values at most 256 bytes. Boolean arguments must be actual
booleans. Integer writes are finite integral numbers within ±2,147,483,647;
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
They stop while simulation is paused. Do not store the API: its scoped Rust
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
| Handler | 2 ms and 20,000 interrupts; 32 staged actions; 4 notices |
| Simulation frame | 64 invocations, 128 published script actions, 8 ms cumulative invocation time |
| Diagnostics | 32 messages/run, 512 printable characters/message |
| Exports | 64 functions/module; no subscriptions or dynamically created event types |

Time is checked cooperatively at Luau calls/loop interrupts, not a hard realtime
scheduler. Source length also bounds compiler input; the compiler's temporary
native allocations are outside the VM allocator. Ordinary bounded native library
calls can briefly exceed the requested wall deadline. Native pattern/substring search
operations (`find`, `match`, `gmatch`, `gsub`, `split`) are omitted because worst-case native
search work can evade VM interrupts. Table/string/math/UTF-8 basics remain available.

No filesystem, network, OS, package loading, FFI, arbitrary native calls, debug
library, bytecode loading, dynamic source loading, coroutines or protected calls
are available. Removing `pcall`/`xpcall` prevents scripts catching and retrying
execution-budget errors. Luau sandboxing freezes standard libraries/metatables.
Only Rust creates scoped capabilities; no raw pointers or runtime handles cross
the boundary. Future APIs must preserve these limits and reuse shared actions.

Scripted packages require the `luau-v1` capability, so older players refuse them
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
and measured limitations belong in the [stage 6 report](reports/editor-0.12.0-task6.md).
