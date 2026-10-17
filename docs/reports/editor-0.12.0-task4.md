# Geometry and safety tools: 0.12.0 task4

Stage 4 extends stage 3 (`816ef4e`) on `0.12.0-staging`. It adds geometry
creation/editing and bounded safety review to the existing Rust editor, shared
engine and project source contract. No trigger/Luau/menu/distribution stage,
asset remake/adoption, main merge or release is included.

## Authoring and interface

`+ Geometry` consolidates floor, wall, ramp and recess creation. The selection
inspector exposes only meaningful scalar fields: a signed ramp rise and
floor-relative offset, one uniform prop scale, or a region offset without a
fictional thickness. Ramp headings identify the ascending axis. Floor `Shape`
provides a snapped world-space rectangular opening and an upper-floor operation.
Detailed explanations precede the explicit mutation. Existing controls were
regrouped rather than supplemented with a permanent feature panel. Build cancel
and Stop Play share one transient slot; Safety occupies the review step in the
file/history/build/review/view workflow.

Openings split an ordinary room into up to four disjoint raw-record copies,
removing that room's floor **and ceiling** inside the chosen rectangle. They do
not invent a floor-portal schema. Upper floors copy an ordinary footprint one
room height above; the lower ceiling remains. Gable transformations, occupied
upper space, ambiguous stacked walking modifiers and unsupported square-ramp
quarter turns are explicitly refused. All accepted geometry operations preserve
unknown data and use the existing atomic validated document/history/save path.

A native pool fixture exposed a preexisting renderer/collision disagreement:
a negative ramp could walk below an unchanged flat carrier floor. The shared
floor grid now cuts at ramp edges and omits carrier cells beneath the slope;
region skirts defer to actual ramp side/end profiles. Higher neighboring deck
faces point into the recessed space, and removed carrier planes no longer make
invisible source-lighting lids. Geometry revision **2** invalidates prepared
geometry and compiler fingerprints. Source/package layouts are unchanged.

## Diagnostics and corrections

The on-demand worker prepares a clone through the compiler's ordinary source
preparation, then reuses its geometry checker. Its optional JSON `surfaces`
field retains measured triangle pairs without parsing human diagnostic text;
suppressed warnings cannot attach their triangles to a previous finding.
Additional bounded samples use the real player radius, step, crouched height
and jump apex to flag exposed ramp sides and floor edges. Basin-depth and global
multi-storey-region warnings explain the actual source semantics. They distinguish
a measured geometry problem from a possible jump/drop path or collision setting.

Static models are loaded from the installed GLBs and transformed through the
engine's placement matrix. Bounds reject candidate pairs; parallel-plane
separation and clipped positive triangle overlap supply actual coplanarity
measurement. Opposing single-sided contact is excluded. Opaque conflicts and
cutout/blended material/depth uncertainties have distinct labels. Catalog-envelope
layering estimates retain their heuristic wording. Intentional decals use their
separate depth-biased pass. Non-coplanar artistic intersections are not
reinterpreted as z-fighting or repaired.

Bounds and actual measured triangles locate findings. View in viewport selects
and frames the location; reopening review retains its cursor. Scope/budget notes
preserve the camera. The only automatic correction removes a raw-identical
simple wall with recognized scalar fields; metadata, openings, per-face data
and every prop disqualify deletion. A snapshot-checked candidate renders without
changing source/history. A compact review bar leaves camera navigation available.
Cancel/Close/Esc restores the original view. Apply creates one undo step and
refreshes source findings; undo/redo after a review invalidates and refreshes
findings. No broad fill, prop motion, depth offset or bulk repair is offered.

Analysis is deliberately bounded: 128 rooms, 512 props, 4096 geometry objects,
a 600k estimated-vertex ceiling and a 32m room-cell/collider-work estimate;
model storage is capped at 200k triangles and broad-phase work at 2m pair visits.
Missing models and exhausted budgets are visible incomplete-review findings.
The ordinary engine/source limits are not reduced. Closing review hides the
panel; the bounded job may finish in the background.

## Verification and evidence

Production checks: formatting and strict workspace/all-target/all-feature Clippy;
locked release workspace/all-feature build; **91 Rust tests**. Eleven added small
regressions cover document transforms/raw round trips, compound opening history,
failed/stale correction safety, actual terrain picking, jumpable boundaries,
static triangle discrimination, audit budgets, shared ramp mesh/collision/visibility,
and diagnostic surface/suppression identity and partially supported ramp ownership. They run without a map bake, renderer
or asset discovery. The full test body ran in under 0.2 seconds after compilation.

The existing Python source-contract suite passed 368 tests (three optional skips).

Asset validation passed: 593 assets, 370 placeable, eight themes, zero catalog
warnings. Read-only texture and prop checks passed; the texture writer reports
96 existing preferred-size/manifest warnings. No PNG, GLB or catalog was changed.

Native direct-binary acceptance is retained under
`local/verification/0.12.0-task4/`, outside `target/`. It uses ordinary SDL editor
handlers and actual GPU framebuffer captures, plus direct compiler/import/player
execution from `/tmp`. It never runs a packaged `.app`. The final handoff JSON
contains executable hashes, assertion paths and the exact commit/CI identity.

- `native/final/editor-create/`: New, ramp/recess creation, numeric snapping,
  transform undo/redo, save and native captures.
- `native/final/editor-opening/`: exact reopen, unknown-field retention, invalid-cut
  refusal without mutation, custom cut, undo/redo and 960×544 layout/review.
- `native/final/editor-safety/`: correction preview/cancel/apply/undo/redo, actual
  candidate viewport object count, refreshed findings, compile and direct play.
- `native/player-final/`: tiny ramp-side escape, recessed basin/ramp exit,
  multi-floor shaft/opening and intentional prop/surface cases; package
  validation/currentness, player coordinates and actual framebuffer evidence.
- `native/final/editor-assertions.json` and `native/player-final/native-summary.json`: acceptance
  results. `try-editor.sh` opens the retained project using the copied payload.

Final direct-binary runs passed all editor assertions and 62 player assertions.
The correction preview kept eight source walls while drawing seven; Cancel
restored the original mesh. Apply changed only one wall (8→7), reducing 22 review
entries to four (both counts include the scope note). Undo restored 22 and redo
restored four. Props, intentional decals and opaque unknown data remained exact.
The repaired Full/Static package validated and verified current, then imported
and played through the editor before returning to the saved document.

Native ramp contact stopped at X≈5.45 with collision enabled; the elevated eye
was already above the 2 m boundary. Jumping cleared that deliberately low wall
and reached beyond X=6 before ordinary void recovery. This is demonstrated
geometry/containment risk, not evidence of an engine response defect disguised
with an added wall. The pool rim stopped ordinary climbing, its descending ramp
provided an exit, and the split upper floor supported an eye Y≈5.2 before a real
fall through the opening to Y≈1.6. Review captures show the corresponding ramp
warning and measured model triangles; intentional table/chair contact remains
manual review.

The editor rendered on Apple M2 Pro / Metal at 1440×816, Fifo, and at the minimum
960×544. The standalone player checks used actual physical 1920×1280 and observed
Immediate presentation after disabling V-sync. The editor-launched player reported
physical 3024×1676. These are functional checks; no FPS/throughput comparison or
average-FPS claim is made. `cargo clean` then removed 8742 files / 7.8 GiB after a process check found no
other target users. The copied editor reopened the exact saved pool source and
rendered successfully from `/tmp` with target absent and unchanged binary hashes.
The retained payload requires this checkout's installed
assets and host SDL3; it is not clean-install or cross-platform signoff.

## Limits and next-stage handoff

The review is not exhaustive watertightness, no-clip, route-reachability or rendered
z-fighting detection. Edge samples do not simulate every trajectory, ceiling,
water or ladder interaction; static authored poses omit motion and alpha-pixel
coverage. Model comparisons cover distinct instances and architecture; internal
self-overlap within one GLB remains asset-tool work. Camera/render-state depth failures need visual investigation and are
not silently “fixed” with geometry. Source-light visibility has no analytic ramp
slope plane; Full transport uses the actual emitted mesh. Top-surface picking
covers fully supported ramps/regions; their side faces and protruding unsupported
ramp footprints can require the scene list. Existing valid overlapping ramps
retain last-authored support precedence and can still render overlapping slopes. Partially supported ramps retain their
rendered top via an overlapping carrier and receive a corner-support warning.

Connect floors using explicit shaft footprints, matching cuts, landings and
carrier floors. Global ramps/regions are not attached to a chosen storey. The
editor preserves other authored collections; it does not yet offer rich gameplay,
water/light/fog/door/trigger controls. Windows/Linux acceptance and distribution
remain later gates. Broad enhancement #18 stays open.

Stage 5 should retain the document's raw-authoritative contract, per-level
history, bounded workers and shared engine/compiler boundaries. Before **every**
new button/control, reassess the entire relevant layout and workflow; move,
regroup, consolidate or redesign existing controls when clearer. Keep the viewport
central, details contextual, labels predictable and core actions discoverable.
Do not layer new permanent panels or append controls into spare space. Include
actual normal/minimum-window visual review with functionality checks.

Stage 5 also owns the new mounting request: placement must use the actual
mounting anchor/bounds and surface normal, not just origin grid snapping.
Ceiling fixtures should sit flush to real ceilings with correct orientation;
wall fixtures attach to walls and floor props remain grounded. Preserve a small
intentional mounting clearance without intersection/depth flicker or visible
floating, account for actual model transforms, and support manual adjustment and
undo. Distinguish the mounting face from the luminous aperture. Validate actual
light models in both the native editor and compiled play. This is handed off;
no new mounting UI or placement behavior is added in stage 4.

Stage 5 must also preserve deliberate embedded/rotated scenery, such as a desk
on its side halfway through the floor. Surface snapping must be optional or
bypassable, with manual adjustment and undo; intentional solid intersections
alone must not force correction or prevent export. Verify the fixed, physics-off
desk through save/reopen/compile/play without snapping it out. Distinguish solid
intersection from actual coplanarity and collision/void defects. The present
`PropDef` contract has yaw only (plus uniform scale/floor-relative Y); arbitrary
pitch/roll and the full sideways-desk acceptance are therefore explicit stage 5
work, not a claimed stage 4 capability. Do not introduce physics work for this.

Persistent placement-specific warning dismissal is **not implemented in stage 4**.
The user's late addition is explicitly deferred to stage 5, where placement
identity/transforms and contextual placement controls are being extended. Existing
coordinate-scoped `geometry_intent` annotations are not a substitute. Stage 5
must provide `Ignore for this placement`, persisted by stable instance identity
and issue-specific affected-geometry/transform context. Significant changes must
invalidate or re-evaluate it; other desks and unrelated warnings on the same
item remain visible. Include clear-ignore/review-ignored state, undo and raw-source
preservation. Loader/source validation for malformed or unsafe data stays active.

Every individual or future bulk/fix-all correction must honor that persisted
intent. Never change a protected placement to repair its ignored issue; unrelated
corrections that alter it require explicit review. Test unchanged ignored desk
placement under individual and bulk corrections, then verify clear-ignore restores
eligibility. Also test save/reopen persistence, distinct desk instances, unrelated
same-item findings, duplicate identities and meaningful geometry/placement edits.
Use contextual review UI, not additional permanent controls. Stage 4's sole
correction removes an identical simple wall; it has no prop-moving or bulk repair
path. The full dismissal/protection acceptance remains incomplete until stage 5.

Asset integration waits until all authorized editor stages 4–7 are complete.
Cloud Blender work may continue independently; this stage does not touch any
remakes. The newer Spoonerman candidate was rejected and must not be integrated.
Menu/distribution stages 8–9 remain unauthorized. After later accepted model
integration, rerun the tiny model/surface/native fixtures because measured
coplanarity evidence is bound to the installed assets. Preserve this stage's
direct binaries/evidence outside target before cleaning build output.
