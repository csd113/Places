# Explicit Closing Time authoring and acceptance

These tools are opt-in desktop workflows, never Cargo or CI fixtures. Run from
the checkout on `0.12.0-staging`. Native SDL windows require a real display.

- `closing_time_record.py PHASE`: record real display pixels while the native
  editor drives an existing `local/verification/0.12.0-closing-time/editor-scripts/`
  recipe through visible controls and ordinary event handlers. Uses preserved
  immutable `payload-final/` executables, overridable with `PLACES_EDITOR_PAYLOAD`.
  Keeps original video, input, native captures and receipt.
- `closing_time_editor.py`: initial geometry/furniture/lighting UI recipe
  generator; it emits input, never level JSON. Intended for a fresh starter,
  not rerunning on the finished project. Final later correction/export inputs
  and an honest provenance note are in the delivered evidence bundle.
- `closing_time_acceptance.py NAME --package PATH --case CASE`: launch the
  direct player with an isolated package import and native evidence. Cases are
  `smoke`, `natural`, `edges`, `collisions`; `--performance` uses an uncapped
  1200-frame run after 120 warmup frames. Never infer an FPS gain from requested
  settings: record actual present mode, drawable, display refresh and workload.
- `closing_time_audit.py RESULT_FOLDER CASE`: assert concrete gameplay outcomes
  from the native snapshots. Inspect native images separately for visible label
  and lighting quality. Diagnostic poses in edge/collision cases are explicitly
  separate from the ordinary-input natural route.

A unique evidence NAME is mandatory; existing evidence is not overwritten.
Do not copy over a running executable. Preserve runnable tools outside `target`
before any clean, and check other active work first. The full map bake is an
explicit developer activity. See the Closing Time authoring report for exact
source-only work, validation, limitations and the later authorized queue.
