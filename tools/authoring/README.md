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

## Beam (explicit native work, outside CI)

`beam_editor.py` generates initial visible-control input recipes;
`beam_record.py PHASE...` records genuine native-editor work using those recipes.
Later exact input recipes and failed attempts remain in the Beam evidence folder.
`beam_acceptance.py NAME --binary PATH --package PATH --case CASE` imports the
actual supplied package and drives ordinary keys plus camera-only aiming.
Cases include natural win, repeated death/respawn, held-item death, out-of-order
returns, beam/partition occlusion, authored two-floor AI patrol and shell bounds.
`beam_audit.py FOLDER CASE` checks retained live snapshots. Evidence names are
unique; inspect the actual pictures as well as the state receipts. These tools
never run under Cargo tests or ordinary CI. Performance mode requires actual
present-mode/drawable/refresh evidence; capture runs are not FPS acceptance.

## Package safety (local adversarial inputs)

`package_safety.py --compiler PATH --asset-root PATH --package CLOSING.placesmap
--package BEAM.placesmap --out NEW_DIRECTORY` validates both finished packages,
compares every recovered source byte and exercises bounded malformed archives,
unsafe paths, ownership/hash substitutions and transactional import failures.
The compiler runs from an unrelated working directory with no asset-root
environment override, so the explicit root is exercised. Claimed large sizes
are metadata mutations, never allocated payloads. Each child has a 60-second
timeout; fixtures and the machine-readable receipt remain under the new output
directory. No full map bake or native rendering is involved.

`gaps_safety_native.py --binary PATH --closing CLOSING.placesmap --beam
BEAM.placesmap --out NEW_DIRECTORY --case all` runs the combined authoring-gap
acceptance against the supplied direct native executable. It includes both
ordinary wins, Beam death/reset/beam and item edges, held-door opening/traversal,
and Closing's required-Full/quality/circuit recovery. Closing's edge setup uses
explicit item-pose diagnostics; the ordinary routes use actual gameplay keys
and camera aiming. Every capture requires actual SDL window focus. Door motion
still stops at held items; the traversal route leaves clearance before opening.
Use a fresh output directory; retained attempts are evidence, never overwritten.

## Shadow-only entity gallery (explicit native review)

`shadow_only_editor.py` emits bounded native editor input recipes for the gallery's
room, receivers, mannequin placements and independent Shadow only property.
Advanced routes, scripts, mirror pane and interaction remain editable source;
exact custody and subsequent reopen/undo receipts are retained in the stage.
`shadow_only_native.py --binary PATH --package PATH --level ID --out NEW_DIRECTORY
--case acceptance|compare|video|performance` imports the supplied package into
isolated state against the actual installed asset root and uses the direct
native executable. It retains real window focus, screenshots and state traces.
Video writes 540 consecutive native frames at 30 fps (18 seconds), with a paced
fixed simulation; it is never a performance result. Performance uses 120 warmup
and 1200 measured frames without capture pacing, V-sync or a frame cap. Record
actual presentation, drawable and display pacing before interpreting timings.
The gallery README gives runnable commands; these tools never run in ordinary CI.
