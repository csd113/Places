# Hollow-metal exit doors: 0.13.1 Mac integration, 2026-11-07

Scope: original single and paired flush hollow-metal exit doors with
surface-mounted vertical-rod exit devices (panic hardware). Generic game assets,
not branded, fire-rated or certified assemblies. No production map or final duck
layout was authored. Windows acceptance is a separate following gate.

## Source and identities

Integration base: `f7f6c77034914f86672dba8b2d28e5c05d139dcd` on `0.13.1-dev`
(the exact base is recorded in the private implementation handoff).
The committed `assets/shared/doors/` exports and `tools/doors/source/` editable
Blender/GIMP files are authoritative. They supersede the original preparation ZIP
with SHA256 `a9a474a57a47b03ec220db22f005b62aced27703f2c21a0ddca7f7d378c31e6c`.
**Do not use that historical ZIP's flawed hinge exports on Windows.**

| Runtime asset | SHA256 | Triangles |
| --- | --- | ---: |
| `hollow_metal_exit_door_single.glb` | `1982688f4cfc7e688f27c506c00411134a0993fa7d5a99d25532bae472f2ab48` | 728 |
| `hollow_metal_exit_door_double.glb` | `d61ac7660d1059ee9f21ac89fdb8106830c8145e4646c7ae2ef311ea831a9c0a` | 1,268 |

Both: zero joints, two opaque single-sided materials, one shared 256² PNG;
unchanged 6,000-triangle cap. Source/export hashes, node identities, bounds,
UV/normal/topology checks and 281 sampled animation poses are in
`tools/doors/metadata/`. Corrected Blender files reopened and re-exported exactly;
the original GIMP paint/XCF remained unchanged. Actual engine wall-fit coverage
found the initial push-side hinge sweep intersecting the jamb; the Blender hinge
axes/barrels/plates and real closer links were corrected on the pull/rear side.
No wall-collision or budget bypass was used.

## Behavioral contract and test value

[Authoring/API contract](../EXIT_DOORS.md) covers explicit editor presets,
160 mm host fit, shared actions/locks/Luau queries, two leaves moving together,
no autoclose, collision, independent grounding, reset and checkpoint limitations.
The same accepted pose drives visual rig and oriented collision. Cosmetic hinge
fastenings embedded into the host wall are not oversized solid blockers. The
8 mm visible threshold is not a passage-spanning door collider because that
collider route has no step semantics.

Three focused Rust regressions, including the actual shared AI sight query, extend 536 preserved tests to 539, with no ignored
tests or full-map bake dependency:

1. Actual exported single/pair motion in fitted host walls, blocked closed and
   truly clear 0.8 m player paths when open, obstruction/reversal/reset; independent
   posed frame/leaf floor contacts do not cover the empty aperture, and animated
   bounds include the open leaf. Uses tiny CPU-only geometry, no GPU/bake.
2. Shared compound actions, locks, conditions, real Luau `api.door`/`door_status`,
   accepted collision publication, navigation-clearance revision and reset.
   One tiny in-memory entity world, no duplicate simulation suite.
3. Editor preset placement, refusal of destructive model replacement, atomic
   undo/redo and save/reopen. One tiny temporary project, no native startup.

Full Rust suite completed in 2.30 s after compilation; these tests protect actual
integration boundaries rather than duplicating source-schema assertions.

## Verified native scope

Actual SDL/Metal player and editor, not Blender-only previews. Private evidence:
`/Users/connordawkins/Documents/Codex/2026-11-07/exit-door-integration/`.
`native-acceptance.json` records 31 passing assertions; `native-evidence.json`
hashes 67 player/editor logs, snapshots and captures. Final tested executables
and hashes are retained under `bin/` and `binaries.json`, outside the Cargo target.

- `native-behavior`: normal E opens the single and both pair leaves. W stops at
  the closed leaf (Z=.4955 m), then crosses the open single to Z=-2.6 m and the
  open pair to Z=-1.363 m. A player in the closing sweep holds the accepted pose;
  reversal, clear closing, lock refusal/unlock and authored reset work.
- `native-npc-flashlight`: a real navigation agent waits at Z=.316 m before the
  closed pair; opening updates clearance, replans and reaches Z=-1.765 m behind
  the wall. The same AI sight query separately blocks/clears/reblocks in the
  CPU exported-geometry test. Flashlight-on closed/open captures show the real
  moving geometry and no stale closed-door obstruction. Planar reflection is
  resident and actually executes; the NPC reflection is visible in the open view.
- `native-culling-final`: at the narrow 600×800 logical window, eye X=-1.9,
  Z=-.75, yaw -90°, the fixed frame stock is outside the main horizontal frustum
  while the open leaf remains drawn and interactable. The wide view shows both
  fitted openings and real articulated closers. This is not a benchmark.
- `native-behavior/05-low-static.png`: Low + Static (`lightmaps:off`) applies;
  High + Full returns correctly, with actual planar-pass diagnostics. Opposite
  practical-light switches change the appearance instead of retaining a
  mesh-attached dark patch. Self-shadow opt-in stays off, zero posed ambient
  self-visibility vertices. Independent supported floor contacts stay within
  the shared eight-subject limit; there is no broad footprint across an empty hole.
- `native-editor-final`, `native-editor-reopen`, `native-editor-select`: visible
  Single/Double browser choices place explicit usable presets and show editable
  prop properties. Placement plus ordinary mount adjustment undo/redo exactly;
  saved documents reopen unchanged. Both entries select through actual Scene
  controls. Test E operates the fitted single, Stop discards runtime changes,
  and restarting Test restores its closed interaction target. An overlapped
  staging world-click selected the smaller single behind the pair, so no generic
  frontmost-overlap picker improvement is claimed. Normal gameplay door picking
  is proved separately by E on each fitted option.

### Explicit lighting boundary

The reused compound `animated_door` mechanism is rigid node animation in
CharacterScene. It is **not** the native generated-slab DynamicScene lighting
route. Physical collision, AI sight/navigation, interaction and flashlight
casting use accepted live poses. Switched/prepared receiving light is preserved,
but these leaves do not cast practical room-light silhouettes onto the prepared
room or become dynamic ray casters for other movable receivers. `occludes:false`
avoids a permanently baked closed-door blocker. Source inspection found no simple
compatible reuse: `sync_doors` consumes native slabs, and DynamicVisibility admits
visible DynamicScene objects, not independently animated CharacterScene nodes.
A whole-GLB proxy would freeze leaves or duplicate visible geometry; a correct
shadow-only per-node cache and receiver-self exclusion would be a renderer
extension. No such expansion, silhouette solver, body bake or expensive character
self-shadow workaround was introduced. Do not market these as slab-lighting parity.

## Reproduction and remaining limits

```sh
python3 tools/doors/check.py --check
python3 tools/doors/acceptance.py --out /absolute/fresh/private-evidence
export PLACES_ASSET_ROOT="$PWD/assets"
# Use the freshly built platform compiler; not the old preparation ZIP.
places-compile export-project /absolute/fresh/private-evidence/private-project \
  --out /absolute/fresh/private-evidence/exit-doors.placesmap --asset-root assets --workers 4
places-compile validate /absolute/fresh/private-evidence/exit-doors.placesmap
places-compile verify /absolute/fresh/private-evidence/private-project --level exit_doors \
  --package /absolute/fresh/private-evidence/exit-doors.placesmap --asset-root assets --require-current
```

Private `prepare_native.py`, `prepare_editor.py`, `run_native.py`, `analyze_native.py`
and the exact `actions.json`/`script.json` files preserve tested input recipes.
For another platform set `PLACES_ASSET_ROOT`, `PLACES_STATE_ROOT`,
`PLACES_CONFIG_ROOT` and `PLACES_CACHE_ROOT` to isolated absolute directories;
put the package under that state root's `levels/`, select `PLACES_LEVEL=exit_doors`
and supply `PLACES_BENCH_ACTIONS`, `PLACES_LOAD_TRACE`, `PLACES_STATE_LOG`,
`PLACES_BENCH_OUT`. The committed `tools/doors/acceptance/` recipes are copied into fresh isolated
run directories by `acceptance.py`; it also copies a separate `editor-project`.
The editor accepts the private project with `--asset-root`, `--compiler`, `--game`,
`--verify-script` and `--capture-dir` as recorded in each `run.json`.

V-sync was on; no FPS/throughput claim. No broad catalog/830-view rerun, production
map bake, all placements/rotations, independent pair-leaf control, automatic closing,
compound-door checkpoint save/resume, or Windows acceptance is claimed. The
coordinator must build and validate the committed current sources on Windows,
then return to Mac. Final duck-map layout/build remains deferred until the user
is awake and approves it. Audio/capture/cinematics files and asset caps are unchanged.

## Final gates and artifact boundary

Passed: locked all-target/all-feature workspace check; format; strict debug and
release Clippy; 539 Rust tests; all-feature production build; 377 source Python
tests (16 expected explicit/native skips); model, texture, asset and clip gates;
Luau production error boundary (11 bounded refusals, no full-map bake).
The final rebuild matched all 14 preserved executable SHA256s byte-for-byte.
The tiny fixture validates and verifies current after final Single/Double display
labels; 16 content-addressed records remained identical across that metadata
refresh. A fresh native frame-out/culling replay used the final current package.
No GitHub CI wait or broad shipped-map build was introduced.
