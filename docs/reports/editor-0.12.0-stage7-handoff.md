# Stage 7 handoff: complete project export/import

Stage 6 supplies bounded Luau execution and native editing/linking. It does not
export a complete project as one multi-level `.placesmap`. The current Build/Play
still produces the selected level's ordinary package. Keep that distinction in
UI, documentation, issue progress and acceptance reports. Preserve the required
`panic = "unwind"` production profile, compile-time compatibility guard and
`tools/verify-luau-release.py` build-gate check; Luau errors cross a C++ exception
boundary that the old abort profile could not safely traverse.

## Existing integration boundaries

- `engine::project::ProjectFolder` owns canonical confined source resolution.
  `attach_scripts` snapshots project-wide, selected manifest-level and raw
  level `scripts` references, deduplicating identical paths in declaration order.
  Keep the role/path/symlink checks, 16-module/512 KiB selected-map limits and
  64 KiB source cap. Reserved custom asset folders remain extension points only.
- `map_compiler::project::read_source` combines selected source/manifest identity
  with exact `script_sources` bytes. `check_level` executes only bounded module
  initialization and verifies every authored `run_script` export before baking.
  Standalone JSON cannot resolve scripts; authored inline snapshots are refused.
- `LevelDef.scripts` contains logical source references. Compiler-owned
  `script_sources: [{path, source}]` is the selected runtime snapshot in semantics.
  The player opens no project script files. The required `luau-v1` capability
  prevents older players silently dropping automatic script behavior.
- `engine::scripting` owns VM safety and `ScriptRuntime`; `EntityWorld` owns all
  effects through the existing dispatcher. `RunScript` works in ordinary bindings
  and sequence action steps. Scoped host callbacks expire after each invocation;
  no raw entity handles, arbitrary native calls or dynamic event registrations
  cross the API. Reset/reload rebuilds VMs and re-arms authored gameplay state.
- The editor's raw documents retain unknown data, mounting/rotation intent,
  per-level history and persisted warning ignores. Script buffers have separate
  bounded text history and join save/unsaved/conflict guards. Each file is atomic;
  multi-file save is not a transaction. Shared corrections must continue skipping
  ignored intentional intersections and protecting their support context.
- `Build+Play` imports through the direct game executable into package/level
  isolated editor profiles. Keep this working while adding complete export.

## Work that remains

Design the smallest shared archive/index change that carries all declared levels,
entry-level selection, shared/level metadata, script sources and thumbnails in one
`.placesmap`. Reuse ordinary per-level prepared records, dependency identities,
limits and importer publication rules. Preserve source identity and module order;
same logical script path in another project cannot become a global VM/module.
Do not count a directory of selected-level packages as completed project export.

Import/export must round-trip membership, IDs, entry level, authored script links,
script bytes and thumbnail metadata/content, then load in a clean isolated player
outside the project checkout. Resolve ID/path/case conflicts explicitly, validate
untrusted archive sizes/names/hashes before allocation/publication and preserve
last usable outputs on failure. Keep native source/build/edit/play and ordinary
single-map CLI paths usable. Extend capabilities explicitly where needed.

Thumbnails currently remain metadata references; their distribution/UI is not
implemented. Read the canonical asset specification before introducing their
pixels or resizing rules. Custom model/texture/sound import remains deferred.
No level-transition scripting API or complete game-mode framework is promised by
stage 6; add only capabilities actually required by the agreed export workflow.

Group new export/import controls into the relevant existing project/build UI,
then inspect native normal/minimum layouts. Retain tiny owner-component tests;
full-map baking, native windows and performance campaigns stay explicit developer
tooling. Re-run strict Clippy, the locked all-feature release build and the native
roundtrip. Preserve runnable acceptance payloads outside `target` before cleanup.

The [stage 6 report](editor-0.12.0-task6.md) and local
`local/verification/0.12.0-task6/stage6-handoff.json` identify the exact verified
commit, payload, CI and evidence. No broader editor issue should close until its
remaining platform/distribution acceptance is complete. No merge to main, release
or branch deletion is authorized here. Model integration stays deferred until
all editor stages finish; the rejected newer Spoonerman is excluded. Closing
Time remains future editor-authored content; #25 and #26 remain backlog.

## Later dedicated safety review

The approved queue continues through stage 7, then model integration and creation
of Closing Time in the actual editor. After those are complete, run a dedicated
Luau and map-import safety review using the finished game mode together with
hostile/malformed small fixtures. Cover sandbox escapes; execution, memory and
action budgets; callback lifecycle; unsafe package paths; oversized/decompression
and resource limits; and invalid package/script input handling in production
binaries. This later review does not expand stage 6 or authorize building the
game mode during this stage.

Carry forward the direct-release foreign-unwind regression and native evidence
under `local/verification/0.12.0-task6/native/runtime-errors-final/` and
`checks/rust-gate-final.log`. Current limits need explicit reassessment against
the finished mode: VM deadlines are cooperative, bounded native calls may exceed
them briefly, parser/compiler temporary allocations lie outside the VM allocator,
and this is an in-process sandbox rather than process isolation. The tiny current
fixtures verify concrete failures, reset/reload and atomic action refusal; they
do not establish full-game-mode safety or cover stage 7's future multi-level
archive/index/import surface. Recheck those surfaces with malformed archives and
production binaries after their implementation, preserving last usable outputs.
