# Stage 6: bounded Luau and native editor integration

Implemented on `0.12.0-staging`, starting from stage 5 commit `34903df`.
[Stage 5 CI](https://github.com/csd113/Places/actions/runs/39077053443) completed
successfully. No maintained map, model, texture, sound or catalog asset changed.
Main, release creation and branch deletion remain outside this work.

## Result and architecture

Authors can create/open Luau modules in the native editor, edit multiline source,
save/reopen it, check errors, link exported handlers to ordinary trigger events,
and build/import/play the result. Scripts use the shared engine action dispatcher
for doors, switchable lights, configured sound emitters, physics groups, spawns
and declared entity state. There is no game-specific engine routine or duplicate
physics/audio/trigger implementation. The future Closing Time mode was not built.

`engine::scripting` uses statically built `mlua` 0.12.2 / Luau 0.740 without JIT.
Native hostile-script acceptance exposed that the earlier `panic = "abort"`
release profile aborted when Luau unwound an ordinary budget error through Rust.
The production profile now uses `panic = "unwind"`, incompatible abort builds
fail compilation, and the Rust build gate runs three tiny refusal cases through
the actual release compiler. This checks the FFI boundary that Cargo’s unwind
unit-test harness cannot distinguish; it never bakes a map. See the upstream
[Rust FFI unwinding contract](https://doc.rust-lang.org/nomicon/ffi.html#ffi-and-unwinding).

Plain module exports provide optional first-playing-tick `start`, playing-frame
`update`, and explicitly linked named handlers. Binding event kind/key/actor,
subject, delta and available occupant origin cross as read-only values. Two-way
doors keep the shared away-facing origin behavior. Typed Rust host entry points
validate exact boolean/string/value types, existing entities/capabilities,
authored groups/points and declared same-type state slots.

Each loaded map owns its VMs. Reset/replay recreates closures/globals, diagnostics
and disabled status; changing maps drops the previous runtime. Scoped API
callbacks expire after every invocation. Host actions are staged in order and
published only after a successful call. A failed module is disabled for that run;
other modules continue. Existing event depth, occupancy, spawn, render and audio
limits remain authoritative. Maps without modules do not clone events for scripts.

Project resolution admits only confined `scripts/*.luau` references. Selected
project/manifest-level/raw-level references are snapshotted into semantics and
exact source bytes participate in build identity. Edits to Luau source files reuse
prepared geometry/lighting and refresh the package semantics. The compiler checks bounded
initialization and named links before baking. The player opens no script paths.
`luau-v1` is a required package capability, preventing older clients from silently
omitting automatic script behavior. Authoring inline snapshots are refused.

## Safety and authoring behavior

The host exposes no filesystem, network, OS, FFI, arbitrary native calls, module
loading, dynamic source/bytecode loading, debug library, coroutines or protected
calls. Luau freezes builtin tables/metatables; export tables cannot have a
metatable and automatic lookup uses raw reads. Pattern/substring search functions
that can spend expensive native work outside VM interrupts are omitted.

Limits are 16 modules/map, 64 KiB/source, 512 KiB total source and 2 MiB VM heap
per module. Initialization has a 10 ms / 20,000-interrupt budget; handlers have
2 ms / 20,000 interrupts, 32 staged actions and four notices. One frame admits
64 invocations, 128 published script actions and 8 ms cumulative invocation time.
Diagnostics retain at most 32 messages of 512 printable characters per run.
These are cooperative wall-time checks, not a hard realtime scheduler. Bounded
native library calls and compiler temporary allocations have the limits described
in [SCRIPTING.md](../SCRIPTING.md); no stronger process-isolation claim is made.

The toolbar uses the existing transient build-control space for Scripts. A focused
source sheet groups edit/save/check/link/diagnostics; trigger Actions includes its
handler entry point and removal. Export choices are paged. Text buffers have
bounded independent undo/redo and join existing unsaved/conflict/save guards.
Check leaves invalid drafts editable and saveable. Source files publish atomically;
a multi-file save can leave a successfully saved subset after an I/O failure.
The manifest and unrelated raw metadata stay intact.

Stage 5 mounting snap, arbitrary prop rotations, persistent per-placement ignores
and correction protection remain in the existing document/runtime systems. Their
existing regressions remain enabled in the full test gate. No auto-fix gateway or
placement-intent rule was changed.

## Validation and evidence

The final working implementation passes formatting, strict all-target/all-feature
Clippy, the locked all-feature production build and **118 Rust tests**. Nine added
owner-component regressions use tiny metadata/file fixtures: valid event/action
wiring, sandbox/resource failures and atomic refusal, stale API/reset/reload,
automatic lifecycle, frame budgets, handler/source contracts, script edit/save/
reopen/conflicts and script-byte build invalidation. No test bakes a shipped map.
The retained Python source suite passes **368 tests** (three optional/native skips).
The catalog check reports 593 assets, 370 placeable, eight themes and zero warnings;
prop checks pass. Texture checks retain 96 pre-existing warnings.

Native macOS/Metal acceptance uses the direct editor/compiler/player executables,
never an `.app` wrapper. A six-metre room with built-in assets is the explicit
fixture. Actual source and handler sheets were inspected at 1440×816 and 960×544.
Native event handlers perform create/edit/undo/redo/save/reopen/check/link/build/
import/play and return to editing. The script opens the existing door, switches
a prepared light layer off, starts ambience, activates physics and spawns through
the existing runtime. Reset restores the door/light and the next trigger reports
`entered 1`; repeated map loads do the same with fresh closures/diagnostics.

Independent runtime modules intentionally exhaust execution, memory and action
budgets or supply an invalid boolean. Their staged door changes are discarded,
bounded errors remain visible and the working module continues. A native syntax
error and failed build retain source buffers and the last package hash; correction
and the next build/play recover. The 55 native assertions also verify that oversized select-all clipboard paste
leaves the original source and clean savepoint intact. The hashes identify the
actual completed cases rather than treating screenshots as exhaustive proof.

Evidence is retained under `local/verification/0.12.0-task6/`:

- `checks/`: Rust, Python and asset/build logs.
- `native/editor-edit/`, `native/editor-link-play/`: source editing, handler linking
  and normal/minimum interface captures/checkpoints.
- `native/editor-errors/`: syntax, failed-build preservation, correction and runtime
  diagnostics through the native editor.
- `native/gameplay-final/`, `native/runtime-errors-final/`: direct-player captures,
  trace/audio/physics state and repeated load/reset outcomes.
- `native/editor-final-reopen/`: fresh source reopen, bounded clipboard-paste
  refusal, copy/paste, save and Check.
- `native/assertions.json`: executable acceptance assertions, produced by
  `assert_native.py`; `payload-manifest.json` pins preserved direct executables.
- `stage6-handoff.json`: exact final commit, remote/CI identities, native assertion
  count, performance evidence, cleanup and any remaining limitations.
- `try-editor.sh`: opens the retained authoring project with direct payload tools
  and explicit repository assets. The host still supplies SDL3.

## Remaining scope

The current output carries selected-map script snapshots. It is not complete
project export/import: multi-level membership, shared/level metadata, thumbnails
and a clean one-package distribution roundtrip remain stage 7. The source editor
has no LSP/typechecker, syntax colouring or arbitrary selection; other entity/
timer/sequence event links remain source-authored. There is no live hot reload,
custom asset importer or game-mode demo. Native Windows/Linux and clean installed
multi-level distribution are not claimed. The broader editor issue remains open.

The approved later queue adds a dedicated Luau/map-import safety review after
stage 7, model integration and editor-authored Closing Time. The handoff records
known budget/compiler/isolation limits and current hostile-fixture evidence for
that review.

Follow the [stage 7 handoff](editor-0.12.0-stage7-handoff.md). Model integration
remains deferred until all editor stages finish; the rejected newer Spoonerman
and backlog modes #25/#26 are unchanged.
