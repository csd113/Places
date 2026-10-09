# Stage 7 native execution orchestration

Preparation is complete; no native, compiler, Cargo, bake or target job was
started by validation. The stopped v3 campaign at
`target/verification/map-regression-20261008T235019Z-94829` is partial and is
superseded by the Hallows Full availability regression. Its eight passed
per-case receipts remain evidence of those commands, without final acceptance.
The subsequent v4 campaign at
`target/verification/map-regression-20261009T002850Z-19070` also stopped after
eight passed cases: the actual Demo ten-page Full atlas exceeded its old typed
bound. Its [stop receipt](../execution/normal-desktop-gate-v4-stop.json) and failed
command remain partial evidence. The [atlas integration update](atlas-bound-parity.md)
records the approved measured bound and pending actual memory checks.

The ignored one-offs `prepare_native_execution.py` and `execute_native_plan.py`
live under `debug-maps/art-style-hero/evidence/stage7-validation/`. The preparer
only writes a new command plan and fresh sequence paths. It reuses the existing
capture helper, normal 48-map/49-geometry runner, strict quality checker,
scene-budget checker and prepared journey. The wrapper runs the prepared argv
serially after the primary's explicit sole native allocation. It records each
command's cwd, start/end UTC, elapsed time, exit, executable/script hashes and
raw log. It preserves independent failures, blocks dependent checks, stops
changed frozen inputs, and cleans its own child groups on interruption. It
contains no Cargo/compiler command and changes no source, asset or assertion.

```sh
PYTHONDONTWRITEBYTECODE=1 python3 debug-maps/art-style-hero/evidence/stage7-validation/prepare_native_execution.py \
  --campaign COMPLETED_FINAL_PACKAGE_CAMPAIGN \
  --compiler FROZEN_FINAL_COMPILER \
  --normal-binary FROZEN_NORMAL_C2_PLAYER \
  --feature-binary FROZEN_VISUAL_DIAGNOSTICS_PLAYER \
  --demo-field-audit PASSED_FINAL_DEMO_FIELD_AUDIT.json \
  --out NEW_NATIVE_CAMPAIGN_DIRECTORY --require-ready
```

After explicit sole allocation, use that exact plan:

```sh
PYTHONDONTWRITEBYTECODE=1 python3 debug-maps/art-style-hero/evidence/stage7-validation/execute_native_plan.py \
  --plan NEW_NATIVE_CAMPAIGN_DIRECTORY/execution-plan.json \
  --allocation-label ACTUAL_PRIMARY_ALLOCATION_HANDOFF
```

The preparation specimen is
`debug-maps/art-style-hero/evidence/stage7-validation/native-preparation-v3-20261009T0030Z/execution-plan.json`.
Its 209 argv entries, unique outputs and dependency IDs were inspected; its
`ready:false` and explicit pending binary/audit paths prevent execution. The
v1/v2 specimens remain superseded preparation records. Generate a fresh plan
with actual final campaign/binaries, never edit these specimens in place.

| Evidence | Exact scope and outputs below the new root |
| --- | --- |
| Original availability | `prior-availability-final.json`; all original installed variants and exact-source matched prior hero bundles retain accepted successful atlas and irradiance availability before captures |
| Normal native maps | `normal48/<source-key>/capture/`; every supported source once, in Home fixture → Office fixture → Pool fixture → Outdoors fixture → Winter → Hallows → remaining stable paths order |
| Geometry | `geometry49/<source-key>/`; 48 supported plus the named invalid loader control; planted defects retain their exact expected findings |
| Quality | `quality/<theme>/quality/` and `independent/`; 12 runs, 72 directed preset transitions, 54 independent filter×lightmap endpoints, plus Low override, restored High and Low/Medium texture-budget hybrids with Full lighting data |
| Normal/feature parity | `parity/<theme>/normal/` and `feature/`; six matched pairs at actual ready frame 120 with identical unit-scale chair spawn, camera, source, package and settings |
| Zoo additive chair | `zoo-apron/`; one normal High apron image after normal Zoo spawn, at the documented actual refined-chair display |
| Demo corridor controls | `demo/before/room{7,8,10}/` and `after/room{7,8,10}/`; two images per run and actual entity support checks; before is explicitly the preserved superseded C1 package/player |
| Installed journey | `journey13/`; 13 distinct installed IDs and return Home, actual action/commit/presentation/RSS receipts, bounded quit, no Focus events |
| Native accounting | `budget48/<source-key>/normal.json`; rejected normal context retained where counters are zero; only then optional genuine `feature-capture/` and `feature.json` |

Thirty staged sequences rewrite only capture image paths. Every original action,
model, position, scale, yaw and ready frame remains identical. Parity intentionally
uses the original frame-1 spawn and first frame-120 capture from the quality
sequence; normal and feature use the same 360-frame bound. The staging receipt
pins original/staged bytes and commands. No image is created during preparation.
The Demo checker now accepts an explicit immutable staged plan via `--plan`;
its actual support assertions are unchanged.

## Restoration and actual lighting support

`check_native_endpoints.py` reuses `check_quality_regression.py` for genuine final
capture submission, requested/applied/resident graphics, actual archive resource
availability and scripted chair identity preservation. It independently pairs
`[entity-light]` centre traces with `[entity-spatial]` and the subsequent actual
feature capture event. Nonnull uploaded JSON is insufficient: every applicable
non-Low endpoint with an available prepared field requires `spatial.enabled`,
at least one actual valid anchor and centre source `Prepared`. It records these
facts at every endpoint, including restored High. Low-profile hybrids retain
actual support observations without claiming a High lighting profile.

Off or an originally absent prepared field records the actual manifest and
`NoField`/disabled/zero-validity fallback. A previously successful variant cannot
be reclassified as a legitimate missing-field fallback. The original availability
gate first compares accepted entry packages and the 23 prior runnable bundles;
exact authored-source SHA matches retain all eleven hero source paths despite
their shared level ID. Twenty-one supported source paths currently have matched
accepted archives (ten installed and eleven hero fixtures). Other supported paths
are explicitly marked as having no matched prior archive; they still receive all
ordinary build/currentness/resource/native checks.

Initial High versus every matched restored High image records the genuine PNG
SHA. Static Home/Office mismatches are saved and return a triage result requiring
primary review. Pool water, outdoor/seasonal weather and actor/emission animation
qualify whole-image comparisons elsewhere; actual identities and native visual
inspection remain required. The six normal/feature parity pairs also retain
actual images, actual frame 120, requested/applied/resident final state and entity
support. Documentary Git-diff hashes are preserved in native receipts but do not
substitute for, or falsely invalidate, the exact rendering input identities when
primary documentation changes.

## Native accounting limits

Each successful ordinary map capture first runs the unchanged `scene_budget.py`
against its actual normal log and selected Full package. Its declared-role and
typed atlas ceiling now match 320 MiB +64 KiB (335,609,856 bytes); the ordinary
256 MiB, mesh/props 512 MiB, aggregate 1 GiB and entry-count 512 limits remain
unchanged. Only the exact populated
scene zero-draw rejection, paired with the real nonpositive BENCH summary counters,
activates a separate feature `final` capture of that map/camera. Any package,
identity, native lifecycle or integrity failure stays a failure. The rejected
normal command/log/counter context remains immutable alongside the feature audit.
The feature path must report nonzero actual `capture_submission` draws and visible
indexed vertices; a zero surface loop never supplies a populated scene budget.

Existing capture counters provide base-scene submitted draws/indices/triangles,
material bindings and CPU-frustum accepted vertices. Actual resident world/prop/
entity/atlas/probe receipts and prepared package inventories remain separate.
They do not measure total driver GPU memory, fragment overdraw or physical
occlusion visibility. Existing GPU/readback tests and Stage 6 Metal capture scope
retain their limits: capture/readback/CPU wall time and zero-draw loop rates do not
establish gameplay FPS. No new numerical draw-call ceiling is invented; ordinary
role/count/size limits and the capacity/u32 contracts remain in the normal gate.
A single journey gives bounded process RSS context, not a repeated-cycle plateau.

Demo's measured ten-page, one-switch Full atlas has 40 RGBA16F layers at 1024
pixels per edge. Its encoded record is 335,544,516 bytes; decoded f32 lighting
and moments occupy 640 MiB. Decoded arrays plus the encoded half-float container
and an additional codec texel copy can approach 1.25 GiB before meshes, props,
actors, staging and driver allocations. This component estimate is not measured
RSS or GPU memory. Final package aggregate validation, actual adapter layer
limits and genuine native process RSS/resource receipts must still pass. No
page, edge, switch-group, ordinary-entry or aggregate limit is increased by
the atlas integration.

## Pending execution inputs

The final ordinary campaign must finish all 48 supported maps with a stable
compiler after the atlas integration, including retained original Hallows
availability and the fully resolved Demo field. The exact final compiler,
normal C2 player and feature player's reviewed build identities and accompanying
runtime libraries must be frozen outside target. The final fully resolved Demo
field/navigation audit must pass and match the actual native Demo package. These
inputs, plus explicit sole native allocation, remain pending. Actual native
budget counters are unavailable until the genuine captures; the existing feature
capture submission provides the required fallback signal without engine changes.

Eight bounded in-memory preparation checks passed, including disabled/invalid/
Unresolved support rejection, original-availability loss rejection and the
incomplete-plan launch guard. Their [receipt](native-orchestration-preparation-checks-v2.json)
is preparation logic evidence, never a native capture. The earlier tracked-tool
freeze and 45-test receipt are preserved. The later narrow atlas-bound allocation
changed only `scene_budget.py` and its focused tests; [20 focused checks
passed](atlas-bound-parity-checks.json). The previous generated specimen also
contains the superseded scene-budget identity and must be regenerated after the
final freeze. Owned one-offs and plans are frozen and released; no validation
jobs remain.
