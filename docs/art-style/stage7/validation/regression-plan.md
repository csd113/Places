# Stage 7 independent validation plan — 2026-10-08 UTC

Entry `26486b8538424f013c243ae6edea8720ac07d7f2`; supported reviewed controls,
GPT-6.1-sol/xhigh delegated selection. This document records scope and pending
execution, not successful native/build results. Primary owns Git, index, compiler
builds and installed package migration. Validation owns the test/layout fixes,
small reusable bench checks and this evidence directory. No target, bake, native
or profiler process was started by validation during preparation.

[Complete source matrix](regression-map-matrix.md) lists every actual recursive
source under `assets/levels`, `levels` and `tests/fixtures/levels`. Five shipped,
five local and forty fixtures give fifty paths. The recovered Blizzard/Snowfall
sources are the old package's serialized LevelDef, not the original author JSON;
[content recovery receipt](../content-recovered-local-sources.json) preserves that
distinction. Eleven hero variants intentionally share one level ID, so coverage
and output names use source paths, never a deduplicated set of IDs.

New package and native campaigns run Home fixture → Office fixture → Pool fixture
→ Outdoors fixture → shipped Winter → shipped Hallows first, then all remaining
source paths in stable lexical order. The inventory-order test independently
requires every recursive path exactly once and retains coincident hero IDs.
Earlier immutable inventories and interrupted receipts retain their original order.

## Exact scope

Forty-eight sources receive normal Off/Medium/Full compilation, required-current
verification, package decode/integrity, explicit geometry inspection and a genuine
native representative image. This includes the loader-valid planted-defect
`geometry_broken`, `repair/wall_step_x`, `repair/wall_step_z` and review-only
`repair/gap_review` sources. Their checker findings are retained and classified;
no content is deleted, warning suppressed or assertion weakened. Ordinary
supported-map confirmed geometry errors remain failures requiring primary triage.
The authored open decorative hero garden remains inaccessible and its explicit
review warning is retained. The generic ghost-collider fix must keep rejection of
unsupported/partial boxes and actual holes.

`invalid/geometry_invalid` is a negative loader control, not a playable map. The
normal compiler must reject it specifically for Arc wall 0's impossible thickness
and publish no package. The existing geometry test must also retain all named
degenerate curve findings. A generic process failure does not pass this control.

`capacity_beyond_former_limits` is a positive CPU loader/count/u32-material-index
witness with synthetic `cap:*` IDs, rather than catalogue-authored playable
content. Its generator and `src/zoo_audit.rs` explicitly establish that purpose.
Retain and execute `the_2026_raised_caps_accept_content_past_their_former_boundary`,
`the_beyond_former_limits_fixture_carries_content_the_old_caps_refused`, widened
material-index codec tests and the ordinary budget suite. The package-reader test
that returns when an optional package is absent is not represented as a real
compiled/native execution. The retired sparse fixture is absent from inventory;
its far-coordinate CPU controls remain. No impossible all-quality bake is used
as a substitute for these actual contracts.

## Normal package path and correctness

After primary preserves every old installed package and freezes source/assets:

```sh
python3 tools/bench/regression_maps.py --run-packages \
  --compiler target/release/places-compile \
  --out NEW_RECEIPT_DIRECTORY \
  --prepared-root target/verification/maps --install-source-packages
```

This command uses default three-variant compiler settings, twelve workers, and
ordinary nonforced builds. Shipped/local output is the source sibling only with
the explicit install flag. Fixture packages use a stable prepared root, enabling
real unchanged reuse. Logs and command/UTC/exit receipts always use a new campaign
directory. It neither edits sources nor invokes Cargo. Every supported source is
enumerated recursively; no new source-validator exclusions are introduced.

The final compiler must pass `verify SOURCE --package PACKAGE --require-current`
and `validate PACKAGE` for all supported outputs. For ten installed affected
packages compare their ordinary incremental migration output with a separate
`build --force` archive using the same frozen compiler/assets/source and every
variant. Require archive byte equality through `compare_packages.py` without any
key exception. A second unchanged normal build must leave the accepted archive
bytes untouched. Catalogue/executable revisions can legitimately cause a one-time
miss; record the actual reason.

Reuse Stage 6's accepted 22 real edits/independent full comparisons and native
texture/light parity. Do not rerun the whole earlier campaign. The final Rust gate
retains changed catalogue/images, runtime-only models/unused spawn overrides,
same-size model/PNG substitutions, source changes, corrupt or wrong-role/revision
provenance, failed-output preservation and package/prepared-stage invalidation
tests. Stage 7's changed inputs need its affected-package comparison above.

For recovered Blizzard/Snowfall compare canonical prepared `semantics.json` and
collision/navigation payloads to the preserved original review contracts. Do not
claim old raw author source SHA equality. Other legitimate geometry-format
partition changes require actual occupied-volume/traversal review rather than
loosening a raw-byte assertion. Keep Home's local legacy source distinct from the
completed Home fixture, and verify the refined Zoo chair while retaining all 217
old placements.

## Native controls and map changes

[Native manifests](native-manifests/) include one normal authored-spawn view for
every supported source. Some diagnostic spawns require camera adjustment after
inspection; changes must be recorded, not silently relabelled as matched evidence.
Use `capture_art_style_hero.py` with the corresponding manifest and a frozen normal
binary. Existing source/asset/package/tool/settings identities and drawable/write
receipts are mandatory. Images are raw native framebuffer results.

After the normal Zoo spawn capture, take the separate normal High
[refined-chair apron view](zoo-refined-chair-manifest.json) at the actual additive
`zoo:home-dining_chair_refined:floor` placement `[24.8,0,140.8]`. The planned interior
camera is `[25.8,142.25]`, yaw −34.59°, pitch −34°, using the existing spawn/camera
override and ordinary full lighting. Its actual native image must show the display;
cache/count receipts alone do not. This adds one view to the forty-eight-source
campaign without editing Zoo's source or its prior 217 placements.

[Six environment plans](native-quality-plan.json) use existing immutable Home,
Office, Pool, Outdoors, Winter and Hallows cameras. Each same-process preset
sequence covers all six directed pairs twice: High→Low→Medium→High→Medium→Low→High,
repeated once. Seventy-two directed transitions retain one visible ordinary
runtime chair through changes. The prescribed chair positions are camera plans;
genuine capture inspection must confirm that entity and environment are visible.

Each independent sequence keeps High texture quality while crossing all nine
filtering Low/Medium/High × atlas Off/Medium/Full combinations, then Low-lighting
override on/off. This gives fifty-four grid endpoints plus twelve override
endpoints. Each then selects Low overall followed by Full prepared lighting data,
Full reflections and High filtering; repeats that hybrid at Medium overall; and
returns to High. These are twelve supported combinations with Low/Medium texture and lighting profiles
hybrids plus six restoration endpoints, not an invented independent High lighting
profile. The quality preset cascades occur at ready frames 2160/2340/2520; separate
graphics overrides use adjacent frames 2161..2163/2341..2343, with captures 120 frames
after the preset and a 2780-frame bound. Pass both `--quality-cycle` (the independent
plan's separate cycle) and `--graphics-cycle` to the existing capture tool.
Changes use distinct ready frames to respect the existing graphics sequencer.
The feature build runs diagnostic `final`; check a matched
normal/feature final pair per environment before relying on its receipts.
`check_quality_regression.py` requires actual nonzero submitted indices/draws,
requested/applied/resident agreement, surviving dynamic objects, valid visibility
resources, unchanged 1280×720 drawable and ordinary final composition. Use its
`--package` argument to require atlas availability, pages/edge/charts/texels/bytes
and entity-field presence to match the actual selected archive, including a named
prepared fallback. Capture prepared probe/direct-sidecar inventory, texture residency,
dynamic/character/source counts and restored filtering identities. Full atlas
availability must agree with the actual package metadata; fallback cannot be
silently mistaken for a resident prepared atlas.

All twelve themed runs also pass `--entity-light-trace`. The existing capture path
emits `[entity-spatial]` name/path/position/uploaded spatial JSON immediately before
its visual capture receipt. Use `--chair-sequence` with the actual sequence input:
the checker uniquely finds the scripted dining chair in capture 1 near its planned
centre, then requires the same actual name, model path and exact centre in every
capture block, with a nonnull uploaded spatial payload. Its actual GLB bounds are
`[-.25,0,-.245]..[.25,.902,.245]`, giving the known local centre `[0,.451,0]` at unit
scale. Low lighting can disable spatial response without deleting the uploaded
object. A surviving unrelated door or actor cannot pass this chair check. Counts,
uniforms and transforms do not establish pixel visibility; inspect the native
images separately.

Captures remain ready-gated settled endpoints. They do not prove every presented
loading transient. Animated bodies/weather may differ in real native pixels;
do not invent whole-image equality. Inspect actual named images and qualify
animation explicitly. The static hero returned High/filter control can still
supply a precise byte comparison when its inputs are fixed.

[Map journey](map-journey-plan.json) drives ordinary `PLACES_BENCH_ACTIONS`
Load/Quality APIs through Home→Office→Pool→Outdoors→Winter, Hallows, Zoo, movement,
Demo and every local map, then back to Home. Stage fixture packages in one genuine
isolated installed root, with real catalogue/dependencies; keep the historical
local Home package in its independent native run. All action IDs must be consumed,
each committed game/renderer map must agree, final High/Full must settle, actual
presented scenes must follow matching GPU-ready commits, and shutdown must finish
exactly once. Use the existing trace/RSS/loading harness for bounded memory and
cache/LRU evidence; include its established repeated-visit/failure/cancel/retry
suite. No injected Focus events or unmeasured foreground assumptions. The original
plan omitted a concrete action-aware launch and bounded Quit: `loading.py` clears
the action environment and runs four startup frames. The primary-requested
[one-off executor](../../../../debug-maps/art-style-hero/evidence/stage7-validation/run_map_journey.py)
imports its package/trace helpers but launches the ordinary native action API
directly, records actual game-PID RSS and `/usr/bin/time -l` peak, and appends a
30-second Snowfall-anchor Quit after scheduling the return Home. A strict final
Home/High/Full commit and real matching scene presentations are required; the
timer itself is no evidence that those states settled.

## Gameplay, resources and performance

Retain normal source controller/route, doors, panes, round-water membership,
swimming, ladder/stairs/ramp, weather shelter/sightline, ice traction, AI/navigation,
entity spawn and collision tests. Run the whole workspace and existing native
loading suites after the three stale local packages are restored. The staged
discovery fixture now copies the actual catalogue and each package's external
manifest dependency, checks copied size/SHA, and asserts the installed drop-in row.
It cannot pass through an incomplete asset-root fallback.

Budget audits use the actual package and genuine nonzero capture counters through
`scene_budget.py`; no source-size safety limit, atlas page cap, quality resource,
reflection cap, caster count or validation assertion is raised or weakened.
Its entry-size preflight now mirrors the existing role and typed runtime limits,
including the 512 MiB mesh/props bound that admits the maintained dense witness,
while retaining the 1 GiB aggregate and stricter metadata/collision bounds. The
independent finding and focused boundary tests are recorded in
[preparation handoff](preparation-handoff.md). The all-map resource one-off reuses
that preflight rather than introducing a different package limit.
Report map-specific charts/pages, collision/nav inventory, archive storage,
texture/prop residency and submitted indices/triangles. Fragment overdraw remains
unmeasured.

Meaningful new comparisons are matched final-theme package/atlas/resource changes,
unchanged reuse versus forced full adoption and a frozen original/refined hero
same-camera cost comparison if a renderer change warrants it. Use normal binaries,
same native dimensions/quality/camera/source and alternating bounded samples;
record catalogue/material changes in the context. Do not repeat historical stages
just to create new numbers. Stage 6's moving receiver kernel result remains scoped
and its conservative global invalidation cost remains disclosed.

Existing GPU traces mix capture readback and presented scene contexts; Stage 6
before/after are not a causal GPU delta. Any new trace must label actual owned-PID
scene encoders, capture/readback versus presented work and active interval unions.
Zero-draw surface telemetry supplies no scene timing. CPU loop/readback values,
offscreen encoded scenes and GPU occupancy do not establish gameplay FPS or
physical display cadence. No native Linux/Windows hardware result is available.

## Required gates and preservation

Primary runs the normal inventory-integrated `tools/verify.sh`, the exact mandatory
`cargo clippy --workspace --all-targets --all-features -- -D warnings` and
`cargo test --workspace`, plus shared locked all-feature debug/release gates.
No test skip, exclusion or ignored-native suite is relabelled as success. Preserve
failed commands/results and assign any fix through primary before rerunning.
Exact final-source CI, final snapshot/gallery and cleanup remain primary-owned.

[Entry preservation](prior-preservation-entry.json) independently hash-checks all
23 accepted bundles /1,010 files against their existing receipts, all seven
concepts and eleven baseline/repeat images: zero mismatches. Failed/incomplete
historical ghost bundles are retained but are outside the accepted 23. This is a
preservation check, not new native replay acceptance.

Preparation Python contracts pass: forty-five tests across environment authoring,
recursive matrix/install safety, quality receipt rejection and existing hero
capture contracts, native coverage, exact geometry-negative classification,
compiler bench contracts and role-specific archive safety bounds.
The capacity generator reproduces the maintained dense fixture bytes and explicit
pressure counts under the current or expanded/reordered catalogue, and refuses
missing/nonplaceable witness IDs; no fixture regeneration is performed by tests.
The initial water test setup omitted required `surface_y`; a native-plan fixture
also compared `/var` to its canonical `/private/var` path. Both setups were
corrected without changing production assertions. Cargo/native execution is
pending serialized allocation and final compiler/content freeze.
