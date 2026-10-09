# Stage 7 independent integration evidence plan

2026-10-08 UTC. This is a bounded proposal from renderer/compiler source review,
not a claim that the following native or package campaign has already run.
[Contract audit and focused checker result](integration-contract-audit.md).
The primary/validation owners retain execution and test/tool write ownership.

## Inventory and package acceptance

The [entry inventory](validation/inventory-entry/inventory.json) records 50
source files, 48 classified as supported, keyed by source path. Five bundled
maps are `places_demo`, `model_zoo`, `lantern_hollow`, `movement_test` and `winter`.
Maintained local packages include `home_showcase`, `geometry_intentional`,
`level0_pit`, `blizzard_review` and `snowfall_contrast`. Catalogue-authored fixture
and hero arrangements complete the source inventory. Invalid/repair controls
must keep their explicit negative/review classifications rather than become
silently passing production rows.

For every accepted source/package pair, preserve original source/package hashes,
prepare with the final ordinary tool, validate every record/dependency, inspect
all Off/Medium/Full variants and require currentness. Record exact output,
compiler/catalogue/dependency identities and unchanged reuse. A decoder pass
alone cannot establish currentness; a successful prepare alone cannot establish
visual quality. Duplicate `art_style_hero` IDs in additive sources remain distinct
source-path evidence and need isolated staging, rather than conflicting normal
catalogue rows. Preserve custom debug maps and prior runnable snapshots.

The inherited three discovery failures need actual source/package migration:
Home cabinet recorded 97,224/current 59,144 bytes; geometry pool wall
1,390,031/30,873; pit guardrail 65,196/31,800. The final ordinary discovery gate
must see those supported packages without hiding, deleting or excluding them.
New-chair Zoo adoption must preserve existing prop placements and genuine runtime
spawn-template closure. Gallery/reference refreshes should name the exact new
source/package/tool and leave previous images/receipts unchanged.

## Directed quality and independent controls

Exercise all six directed preset changes, starting each from a genuinely settled
state: Low→Medium, Medium→Low, Low→High, High→Low, Medium→High and High→Medium.
For each, retain request/upload/commit lifecycle trace, requested/effective/
resident settings, world and renderer level IDs, entity/trigger/navigation counts,
and capture the settled endpoint. Compare to a direct launch with the same final
settings, time, map, camera and drawable. A returned endpoint does not establish
absence of intermediate loading transients; retain the lifecycle evidence too.

Include independent High+Off, High+Medium, Low+Full lightmaps; reflection
Off/Medium/Full at fixed overall quality; filtering changes without changing
lightmaps; lighting-profile changes without replacing texture quality; and
bloom off/on where supported. Verify the actual settings receipt instead of
inferring them from the preset label. Keep authored presentation/brightness fixed.
Water/ice/sky and ordinary opaque/cutout/Blend models exercise the existing shared
paths; optional environment/water controls and absent fields exercise defaults.

## Same-process map changes through the normal loader

Existing `src/perf/actions.rs::Action::Load { level }` already uses
`PLACES_BENCH_ACTIONS` under `PLACES_BENCH=1`. The lib handler selects the ID
from `LevelManager.entries` and calls the ordinary `load_level_at` path. Reuse
that supported mechanism; no LevelCycle framework or bespoke renderer entry is
needed. `tools/bench/loading.py` supports installing prepared packages into a
staged root with their actual dependency closure and catalogue.

One bounded route loads the five bundled maps successively and returns to the
demo in one PID. Unique `ready:<id>` anchors can trigger the next load; the return
demo load is anchored to the first ready Winter (or final unique map). Use real
quality/lightmap actions between settled map commits, including a return to the
initial configuration. Retain package IDs, accepted variants, resource creation/
replacement logs, shader errors, world/entity counts and the final settled capture.

Action anchors are the **first** `start/request/upload/ready/failed` occurrence,
not recurring visit events. A script is 1..128 actions, at most 64 KiB, each delay
at most 60,000 ms. Repeated routes must schedule against a known first occurrence
or use deliberately bounded start offsets; do not assume ready anchors rearm.
Map commits reset `ready_frames`; graphics changes preserve it. Existing lighting
sequence steps are consumed once, so capture schedules must explicitly account
for that reset instead of treating per-map frames as a global clock.

Existing world traces report requested and renderer quality/lightmaps/lighting,
window focus, level identities and world/entity counts. They do not report a
complete GPU heap. Combine their actual resident resource logs with bounded
OS RSS observations during repeated map/quality cycles, comparing late return
states after warm-up to earlier returns. Separate legitimate cached assets/driver
allocation from continuing growth; do not present one process peak as a leak
proof or summed resource columns as total GPU memory. Stable known capacities
and repeated settled plateaus are useful bounded evidence, with that limit stated.

Use ordinary foreground submitted frames for performance claims. Synthetic
`Focus` actions and offscreen capture encoding do not prove surface presentation
or gameplay pacing. A no-draw frame loop is not scene FPS. Fixed per-map direct
captures can verify visual endpoints; the same-process trace proves the loader/
resource transition. Neither substitutes for the other.

## Final serialized gates

The primary runs format, strict debug/release all-target/all-feature Clippy,
`cargo test --workspace`, shared locked/all-feature gate, assets/texture/prop and
package tests, final native hero and supported-map regressions, and exact-SHA CI.
Run the compiler C1/demo-refresh/C2 identity experiment described in the audit,
then verify all final normal packages and unchanged byte-preserving reuse after
ordinary and diagnostic builds are restored to the intended identities.
Record executable, package, source, catalogue, capture-tool, backend, drawable
and effective settings for every accepted capture. Preserve failures separately
and name actual remaining limitations rather than transferring historical green
results onto the final source. No tests, warnings or guards should be weakened.
