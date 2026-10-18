# Explicit Closing Time authoring and acceptance

These tools are opt-in desktop workflows, never Cargo or CI fixtures. Run from
the checkout on `0.12.0-staging`. Native SDL windows require a real display.

- `cargo run --bin environment-import-audit -- NEW_DIRECTORY old-map.json ...`
  imports each standalone map through the same `Document::import_level` operation
  as **Project > Import JSON map**. It preserves the input bytes, records actual
  Safety diagnostics before/after conservative corrections, validates built-in
  assets and save/reopen, and publishes migrated JSON baselines. Existing paths
  are refused. A separate isolated copy of the first imported legacy map gets
  two deliberate duplicate simple walls, proving preview/apply/undo and that
  imported per-placement intent skips the protected correction while retaining
  an independent correction. This explicit developer tool performs CPU geometry
  preparation and model checks, never a lightmap bake or ordinary CI test.
  Diagnostics remain bounded review candidates; zero fixes does not mean zero
  concerns. Scripts require source-bearing project recovery, and IDs are never
  renamed automatically.
  `environment-import-audit --review PROJECT LEVEL NEW_DIRECTORY` performs a
  read-only final review with source-hash confirmation, without importing or
  applying corrections.

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

`fixtures/shadow_default_pose.json` is a separate four-actor developer regression
for animation ownership. Explicitly compile it and run native `--case default-pose`:
the placed hidden/visible pair must both retain automatic walk locomotion; the
spawned hidden/visible pair must both retain their default held pose. This tiny
source is opt-in tooling, never an automatic map-bake dependency.

## Environment collection (explicit native acceptance)

`environment_acceptance.py plan --project projects/environments --out NEW_PLAN`
reads the actual multi-map project and writes one ordinary hub-to-environment-
and-return recipe per destination, plus its source inventory and expectations.
Each physical transition door must declare an interact/change-level binding.
Review the generated waypoints against actual geometry before native use;
`--overrides FILE` supplies per-level `hub_waypoints`, `destination_waypoints`,
`capture_waypoints`, `dismiss_intro` and `return_wait_ms`. Generation is not
collision validation. The first-occurrence anchor limitation is handled with
one independent visit per process and an audited wait after returning to hub.

`environment_acceptance.py run --binary PATH --package COLLECTION.placesmap
--out NEW_RUN --level hub --actions PLAN/hub-home.actions.json
--expectations PLAN/hub-home.expect.json` imports into fresh isolated state,
then starts the direct binary. The retained identity, inputs, traces, PNGs and
snapshots disclose exact binary/package hashes and actual focus. All natural
routes prohibit direct map loading, player/item pose, actor navigation-goal and
activation diagnostics. Use the importable `Walk` helper for room, NPC and Office
routes using the same ordinary keys/camera. It enforces 128 actions and a
60-second maximum per first-occurrence ready anchor; split longer cases.

For the final canonical smoke after distribution retirement, `--bundled`
(`--no-import`) skips installation only after the supplied package bytes match
a real, non-symlink `.placesmap` under the selected asset root's `levels/`.
The receipt records the bundled file, hash and skipped import, and rechecks
custody at shutdown. The default still imports into fresh isolated state.

`environment_audit.py NEW_RUN` verifies concrete map/focus/physics receipts and
the adjacent `expectations.json`. Full-lighting runs also require the captured
renderer to have actually applied Full lightmaps and High lighting. Assertions
support dotted snapshot paths and explicit event counts/fields; missing
visibility evidence fails. Retirement events require a finite combined posed
model/prop bound, complete occlusion and the actual character and prop meshes
hidden. Native images still need visual review, including partial limbs/prop at
corners, following, turning back and room composition.
Optional `items` assertions resolve an item ID through its original authored
render key, require exactly one physical body, then check body fields such as
`dormant` or `pose.position`. This verifies reset custody without assuming a
body-array index or accepting a duplicate replacement item.

For smooth native video, repeat a reviewed route with `--record --no-capture`.
This omits synchronous PNG captures while macOS WindowServer records only the
launched game's PID; real-time state/input traces remain. `--window-info PATH`
reuses an existing CoreGraphics PID helper; otherwise the runner prepares one
before launching. It never records a different application's window.

Performance is a separate unrecorded run: `--performance --level LEVEL
--diagnostic --spawn X,Y,Z,YAW --camera YAW,PITCH` samples a disclosed fixed view
after 1,200 warmup frames, then records 1,200 frames. No capture/video pacing,
V-sync or frame limiter is enabled. The audit records actual presentation mode,
physical drawable, current display refresh, arithmetic mean frame time, average
FPS and the proportion faster than refresh. `--require-uncapped` fails unless
the actual run exceeds refresh with Immediate presentation. A refresh plateau
is explicitly inconclusive, even when requested settings say uncapped.
Each performance run also reuses `tools/bench/frame_time.py`'s PID-only RSS
sampling at roughly one-second intervals. Receipts separate startup/loading
from the ready world, disclose sampled peaks and sampling errors, and never
call a sampled maximum an OS true peak. Observed selected-map commits and
process RSS are separate from GPU atlas or analytic residency budgets.
