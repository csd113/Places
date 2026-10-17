# Basic editor: 0.12.0 task3

This completes the authorized basic-editor stage on `0.12.0-staging`, following
project-contract stage1 `af94e3f` and installation/path stage2 `169748c`.
No main merge, release, branch deletion or later editor stage is included.

`places-editor` is a separate SDL/wgpu application using the shared Places engine,
renderer, asset catalog, geometry and project/path contracts. See
[controls and launch options](../EDITOR.md). It creates/opens projects, selects
real 3D geometry, edits floor/wall footprints and built-in passive props, and
provides position/dimension/yaw/scale entry, snapping, duplication/deletion,
per-level undo/redo, protected save, structured build progress and direct play.

## Implementation boundaries

- `src/editor/document/`: authoritative raw JSON, typed validation, independent
  per-level history/savepoints, confined atomic saves and external-change checks.
  Unknown fields survive editing; ambiguous duplicate keys or numeric data that
  cannot be preserved exactly are refused. The manifest is never reserialized.
- `src/editor/viewport/`: camera, picking, selection/grid overlay and coalesced
  background preview preparation. Shared render code supplies an unlit ceiling
  cutaway preview; original scene lighting and ceilings remain in the document.
- `src/editor/{ui,input,actions}.rs`: controls, modal dirty-state protection and
  application actions, using the existing engine font/renderer infrastructure.
- `src/editor/process/` and `src/map_compiler/events.rs`: bounded subprocess
  capture, protocol-v1 NDJSON progress/results, cooperative cancellation with
  kill/reap fallback, and isolated per-package/level player profiles. Save/build
  and project switches cannot race an active build; only one child job runs.
- `src/editor/verification.rs`: explicit bounded developer scripts through the
  normal native event handlers, framebuffer captures and document checkpoints.
  This has no effect during ordinary editing and does not bake maps in tests.

The player now resolves an exact requested level ID before a display-name
fallback, so a different map with the same display name cannot intercept play.

## Verification

All production checks passed locally: formatting; strict workspace/all-target/
all-feature Clippy in debug and release; locked release workspace build; all
80 Rust tests; 368 Python tests (16 existing skips); asset validation (593 assets,
370 placeable, eight themes, zero warnings). Six compact editor tests cover raw
data/history/save failure and subprocess protocol/result/ownership regressions.
They perform no render, asset discovery or full-map bake. Existing production
tests remain intact. Local `RUSTC_WRAPPER=` avoids the host's sandboxed sccache;
no repository build settings were relaxed.

Native acceptance used the direct release executables from `/tmp`, with no
`PLACES_ASSET_ROOT` override and no change of working directory. Copies of all
three verified tools and a repository-assets symlink are retained outside
`target/`, alongside one reusable six-metre project and all evidence under
`local/verification/0.12.0-task3/`. `try-editor.sh` opens that project. The payload
requires this checkout's assets and the host SDL3 installation.

The editor rendered on Apple M2 Pro / Metal at physical 1440×816, Fifo. Its
launched game rendered windowed at physical 1920×1088. These are functional
checks, with no FPS/throughput comparison. Desktop automation could not address
the unbundled raw executable, so explicit native verification scripts drove
ordinary SDL event handlers and captured the actual GPU framebuffer. No packaged
application or OS security workaround was used.

| Native exercise | Evidence/result |
| --- | --- |
| New project, floor, four walls, built-in chair | `native/edit/`, correct catalog collision size/solid metadata |
| Pick, snapped XYZ/yaw/uniform scale, undo/redo | `edit-assertions.json`, exact scene restoration and visible selected bounds |
| Orbit/fly, release, frame | Camera checkpoints show movement and no release drift |
| Unsaved close → Cancel, then save/close/reopen | Raw document equals saved source and reopened document; clean savepoint |
| Unknown nested fields through edit/save/build/play | Original unknown data retained exactly in source/checkpoints |
| Save/build with visible pass progress → play → editor | `workflow-assertions.json`; selected ID loaded, actual room/chair visible |
| Wall/floor collision and jump | `collision-assertions.json`, player stops at Z=0.55, eye Y=1.60 on floor and reaches 2.4999 during jump without crossing wall |
| Cancel changed-scene bake, then rebuild | Cancelled result, previous package SHA unchanged, later success publishes changed package |
| Real compiler output failure, retry, repeat play | Task-owned output directory temporarily read-only; failure retained package SHA; restored permissions, retry and second play returned successfully |
| Repeated open/build/play | Three editor launches and two player sessions completed without a freeze; owned children reaped |

Screenshots and the short demo are retained outside `target/`. The demo combines
actual captured editor checkpoints with native game footage; it is an edited
demonstration rather than a continuous desktop recording. The chat copy is below
9 MB. Visual inspection prompted a steeper starting camera and a quieter grid;
the final screenshot shows the room interior, selected prop and readable controls.

## Remaining scope

Windows/Linux native acceptance and final distribution signoff remain deferred.
The preview is static, with authored light/gameplay visible through Play. Rooms
and walls remain axis-aligned; unsupported destructive transformations are
refused. Rich light/trigger/fog/door authoring, advanced floors/pools/ramps,
diagnostics/auto-fixes, Luau, custom asset importers, complete multi-level bundle
export and a bundled menu scene are later stages. Bundled-level ID conflicts use
the existing import rejection; new projects generate distinct IDs.

Broad editor issue #18 and installation/export issue #21 remain open because
their complete requested scope extends beyond this stage.
