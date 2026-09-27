# Offline Python tooling

Heavy commands run sequentially under one allocation of at most 12 CPU-heavy
workers. `--workers N` selects a count within the caller's `PLACES_TOOL_WORKERS`
allocation, available CPUs and independent job count. `--workers 1` executes the
same algorithms directly (for native Blender, it caps the native CPU allocation). Do not overlap these commands with Cargo builds or
other expensive jobs. All dependencies are Python standard library and repository
modules except the explicitly named Blender workflows. Python 3.10+ syntax was
already used by these tools; validation here uses Python 3.13.5.

`tools/execution.py` uses macOS-safe spawn processes, at most two submitted jobs
per worker per batch, stable input-order merging, and parent-only publication.
Child allocation is one CPU slot. Current CPU workers use no native numerical
backend; initializer thread settings are not evidence of BLAS control for a future
module that initializes native libraries at import time. Geometry audit automatic
selection is eight workers; seam filtering defaults to four to bound resident
float/pixel buffers. Other tools use the available allocation and job count.
Repeated lattice-corner hashes use a bounded 65,536-entry cache; per-pixel grain remains uncached. Values and arithmetic are unchanged.
The animation bake has six independent grounded clip jobs; it never duplicates a
search or changes sample counts. Resize jobs reuse source pixels in the initializer
and preserve the original box coverage and integer rounding.

Generators compute their complete selected batch before publication; encoded files
are replaced atomically by the parent. This prevents truncated files and worker-error
partial batches. Multi-file publication is not a filesystem transaction: an I/O
failure during final publication can leave an already-published subset. Geometry
repair additionally checks original hashes before applying proposals. Approved assets
must be regenerated in scratch first. A geometry repair is still opt-in `--apply`;
parallel execution does not authorize new repairs. Existing art dimension/rig guards
remain active.

For contact sheets, `--workers` preserves its existing meaning of concurrent Blender processes; use `--blender-threads 1` as well for a single-CPU reference.
Blender contact sheets use Python threads only to orchestrate separate headless
Blender processes, each with `--threads`; processes × native threads fits the shared
allocation. Rat Boolean union remains one dependent Blender scene, with its native
thread allocation explicitly bounded. Contact sheets reject ambiguous duplicate asset
stems and require every expected cell, successful exit and the completion marker.
Native game capture/benchmark jobs remain serial to preserve meaningful measurements;
only completed image comparisons are parallelized.

## Commands

```sh
python3 -m unittest tests.test_tool_execution tests.test_zoo_generator
python3 tools/props/preview.py --all --workers 8 --out target/previews
python3 tools/textures/build.py --check
python3 tools/textures/build.py --workers 4 --only core:tex_pool_tile_deck_01
python3 tools/props/build.py --workers 4 --only home:fork home:bowl
python3 tools/props/animate_spooner_man.py --workers 6
python3 tools/props/repair_geometry.py --workers 8 --report target/geometry.json
python3 tools/props/resize_embedded_textures.py target/import.glb --size 256 --workers 8
python3 tools/textures/seam_repair.py --workers 4 --check assets/environment/pool/textures/walls/*.png
python3 tools/bench/check_holes.py --workers 8 target/captures
python3 tools/bench/compare_captures.py target/before target/after --workers 8
python3 tools/bench/visual_check.py --baseline target/before/places --current target/release/places --workers 8
python3 tools/entities/build_rat.py --workers 8 --out target/rat.glb
python3 tools/entities/validate_entities.py --workers 8
python3 tools/entities/render_contact_sheets.py --workers 6 --blender-threads 2
python3 tools/entities/render_contact_sheets.py --workers 1 --blender-threads 1  # single CPU reference
python3 tools/levels/build_model_zoo.py --check --no-cache --workers 4
```

The performance/equivalence command is separate from fast CI correctness checks:

```sh
python3 tools/bench/check_tool_parallelism.py --baseline target/run09/baseline --out target/new-tool-check --repeat 2
```

It requires a preserved **working-tree** source and input snapshot made before the
change being evaluated, not Git HEAD. It refuses to reuse its output directory.
It runs original, updated serial and 4/8/12-worker CLI workloads in copied trees,
compares deterministic output hashes, and saves command logs and timings. This is a
bounded representative check, not proof of every possible imported asset.

## Complete first-party inventory

A = CPU work partitioned at the listed consumer; B = already parallel; C = I/O or
external orchestration; D = bounded/inherently serial; E = obsolete, unavailable or failing prerequisites.
Libraries return in-memory data; their consumers publish the listed outputs.
Dependencies: S = standard library, P = prop toolkit, T = texture toolkit,
E = entity toolkit, B = Blender. No new third-party Python dependencies were added.

| Entry point | Purpose / consumers | Dependencies / output | Class and execution  Status / functional evidence |
| --- | --- | --- | --- | --- |
| `apps/places-pocketchip/tests/test_assets.py` | Separate app asset graph regression | S/app validator; stdout | D; bounded graph check; unchanged  Unchanged (execution reason at left); PocketChip validator and asset unittest |
| `apps/places-pocketchip/tools/assets/validate.py` | Separate app catalog/map checks | S; stdout | D; fast structural checks; unchanged  Unchanged (execution reason at left); PocketChip validator and asset unittest |
| `tests/test_compiled_build.py` | Packaged native windowed smoke | S/Places; scratch runtime output | C; sequential native subprocesses  Unchanged (execution reason at left); 8 native packaged/fresh-install tests |
| `tests/test_package.py` | Repository/asset regressions; verify.sh | S/validators; scratch/stdout | D; bounded assertions  Unchanged (execution reason at left); 43 package/asset regressions |
| `tests/test_tool_execution.py` | Known answers, failure/cancellation, ordering and budgets | S/tools; scratch/stdout | B; new fast spawn regressions  New; known-answer, spawn/failure/cancellation/input regressions |
| `tests/test_wgpu_bootstrap.py` | Native renderer lifecycle/quality regressions | S/Places; scratch captures | C; sequential native subprocesses  Modified; 14 native renderer tests; stale count replaced, file handle closed |
| `tests/test_zoo_generator.py` | Catalog growth/determinism regressions | S/zoo; scratch/stdout | B; serial and spawn inspection  Modified; growth/removal/determinism/budget unittest |
| `tools/assets/validate.py` | Catalog/map schema checks; gate/package tests | S; stdout | D; fast structural validation  Unchanged (execution reason at left); actual catalog and map validation |
| `tools/bench/bench_local.py` | Native game repeated timing; evidence | S/Places; JSON/logs | C; serial to avoid measurement interference  Unchanged (execution reason at left); three native demo and zoo High runs |
| `tools/bench/check_holes.py` | Capture dark-pixel audit | S; stdout/JSON | A per-image spawn  Modified; original vs updated 1/4/8/12 CLI reports; pixel known answers |
| `tools/bench/check_tool_parallelism.py` | Original/serial/parallel scratch benchmark | S/tools; scratch hashes/logs/times | C; serial orchestration of bounded heavy jobs  New; real CLI comparisons recorded in durable results JSON |
| `tools/bench/compare_baseline.py` | Frozen baseline byte comparisons | S; stdout | C; file reads and byte comparison  Unchanged (execution reason at left); 25 High + 25 Low frozen-image self comparisons |
| `tools/bench/compare_captures.py` | Per-view capture differences | S; stdout | A per-pair spawn  Modified; original vs updated 1/4/8/12 CLI reports; pixel known answers |
| `tools/bench/lightmap_report.py` | Native capture and lighting telemetry | S/Places; PNG/log/report | C; serial game, retired duplicate removed  Modified; native demo/pool capture and telemetry |
| `tools/bench/visual_check.py` | Two-build capture and connected-difference audit | S/Places; captures/report | C serial capture; A spawn comparisons  Modified; same-named binaries, differing/same pixels, workers 1/4/12 |
| `tools/entities/build_mannequin.py` | Mannequin mesh and three poses; entity authoring | S/P/E; GLB and concrete PNG | D; bounded direct construction  Unchanged (execution reason at left); scratch generation and rig GLB reimport |
| `tools/entities/build_rat.py` | Rat mesh and clips; entity authoring | S/P/E/B; GLB, fur PNG, reports | C; serial owning scene, bounded native Boolean  Modified; original/1/4/8/12 exact GLB exports; rig consumer checks |
| `tools/entities/build_skeleton.py` | Skeleton mesh and three poses; entity authoring | S/P/E; GLB, bone PNG, optional preview | D; bounded construction, renderer used for preview  Unchanged (execution reason at left); scratch generation and rig GLB reimport; preview consumer separately tested |
| `tools/entities/check_clip_boundaries.py` | Exported loop/transition checks; acceptance | S/E; stdout | D; small endpoint comparisons  Unchanged (execution reason at left); all four canonical entities and transition/weight invariants |
| `tools/entities/rat_surface.py` | Exact anatomical union; rat builder | S/B; isolated numerical JSON | C; explicit native CPU budget  Modified; original/1/4/8/12 exact GLB exports; rig consumer checks |
| `tools/entities/render_contact_sheets.py` | GLB reimport and clip/pose renders; acceptance | S/P/B; scratch cells and parent PNG sheets | B; bounded native processes, thread orchestration  Modified; Blender reimport/three exact PNG sheets at 1/4/8/12; failure gate |
| `tools/entities/rig.py` | Rig/GLB API, --check/--selftest; builders | S/P; GLB bytes and checks | D; bounded math/writer  Unchanged (execution reason at left); self-test plus generated and canonical GLB checks |
| `tools/entities/validate_entities.py` | Sampled skin/contact checks; acceptance/zoo | S/E; report JSON/stdout | B; initialized spawn frame chunks, shared budget  Modified; full sampled report original/1/4/8/12 equivalence |
| `tools/execution.py` | Shared worker budget, spawn executor and atomic output | S; results/encoded files | B; new shared execution helper  New; known-answer, spawn/failure/cancellation/input regressions |
| `tools/levels/build_capacity_fixtures.py` | Two capacity fixture layouts; tests | S/catalog; fixture JSONs | D; bounded layout synthesis  Unchanged (execution reason at left); both checked-in fixtures pass --check |
| `tools/levels/build_fixture_levels.py` | Showcase/stress layouts; tests | S/catalog; fixture JSONs | D; bounded layout synthesis  Modified; two scratch regenerations byte-identical |
| `tools/levels/build_model_zoo.py` | Catalog inspection and zoo regeneration | S/E; zoo JSON and cache | B; cached spawn inspection, shared budget  Modified; original/1/4/8/12 bytes; no-cache --check and growth tests |
| `tools/props/animate_spooner_man.py` | Canonical cat clip baking and --check | S/P; guarded canonical GLB | A; six spawn clip jobs, parent export  Modified; six-clip original/1/4/8/12 exact GLB; canonical rig/clip checks |
| `tools/props/build.py` | Catalog prop build/check; verify.sh and authoring | S/P; catalog GLBs | A build jobs; D cheap checks  Modified; full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/cat_motion.py` | IK and grounded clip sampling; animation baker | S/P; pose arrays | A; initialized model per worker  Modified; six-clip original/1/4/8/12 exact GLB; canonical rig/clip checks |
| `tools/props/generate_spooner_man.py` | Static fallback wrapper; documented CLI | S/P; guarded cat GLB | D/E; guard-only legacy wrapper; forwards flags, forced static fallback fails current size contract  Modified; guard preserves canonical GLB; forced legacy size failure explicitly retained |
| `tools/props/geometry.py` | Adjacency/winding; builders and repair | S; audit/mesh arrays | D; near-linear bounded meshes  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/glb.py` | GLB codec; builders/previews/validators | S; bytes and model arrays | D; bounded format operations  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/glyphs.py` | Font/polygon painter; signs/decals | S; pixels | D; small glyph work  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/mesh.py` | Primitive mesh API; builders | S/P; mesh arrays | D; bounded primitives  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/palette.py` | Color helpers; builders | S; tuples | D; trivial arithmetic  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/parts/__init__.py` |   init   builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/parts/appliances.py` | appliances builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/parts/decor.py` | decor builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/parts/domestic_remade.py` | domestic remade builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/parts/duck_remade.py` | duck remade builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/parts/furniture.py` | furniture builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/parts/home.py` | home builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/parts/pool.py` | pool builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/parts/pool_remade.py` | pool remade builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/parts/refreshed.py` | refreshed builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/parts/signage.py` | signage builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/parts/spooner_man.py` | spooner man builders; prop build registry | S/P; mesh/texture buffers | E; legacy static cat has obsolete dimensions; protected shipped rig remains canonical  Modified; guard preserves canonical GLB; forced legacy size failure explicitly retained |
| `tools/props/parts/tableware.py` | tableware builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/parts/utility.py` | utility builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/preview.py` | Software raster previews; authoring/skeleton | S/P; PNG/contact sheets | A; per-model spawn jobs  Modified; 48 original/1/4/8/12 exact raster outputs |
| `tools/props/repair_geometry.py` | Reviewed topology audit/repair; authoring | S/P; optional GLBs and report | A; per-model proposals, parent hash preflight  Modified; injected reversed triangle: exact original/1/4/8/12 repair |
| `tools/props/resize_embedded_textures.py` | Embedded PNG conversion; imports | S; in-place GLB | A; initialized source, output row bands  Modified; 1254→256 embedded PNG exact original/1/4/8/12; known-answer tiny/uneven |
| `tools/props/test_geometry.py` | Winding/cavity/seam regressions | S/P; unittest output | D; tiny fixtures  Unchanged (execution reason at left); geometry unittest fixtures |
| `tools/props/tex.py` | PNG codec and pixel API; builders/previews | S/P; pixels/PNG bytes | A via consumer batches  Unchanged (execution reason at left); full scratch pack generation and GLB consumer checks; four-builder exact A/B |
| `tools/props/validate_domestic_remake.py` | Ten-model structure/texture/switch audit | S/P; fixed report JSON | D; bounded audit; run in scratch  Unchanged (execution reason at left); ten actual domestic model/texture/switch audits |
| `tools/textures/artkit.py` | Canvas/noise/PNG API; painters | S; pixels/PNG bytes | A via painter jobs  Modified; 47 generated sheets: exact original/1/4/8/12; current full scratch regeneration/check |
| `tools/textures/build.py` | Texture painter/check CLI; verify.sh | S/T; catalog PNGs | A spawn painter jobs; D header checks  Modified; 47 generated sheets: exact original/1/4/8/12; current full scratch regeneration/check |
| `tools/textures/decal_art.py` | decal art painters; textures/build.py | S/T; canvas | A through parent painter batches; lattice cache shared where applicable  Unchanged (execution reason at left); 47 generated sheets: exact original/1/4/8/12; current full scratch regeneration/check |
| `tools/textures/diagnostic_art.py` | diagnostic art painters; textures/build.py | S/T; canvas | A through parent painter batches; lattice cache shared where applicable  Unchanged (execution reason at left); 47 generated sheets: exact original/1/4/8/12; current full scratch regeneration/check |
| `tools/textures/extra_art.py` | extra art painters; textures/build.py | S/T; canvas | A through parent painter batches; lattice cache shared where applicable  Unchanged (execution reason at left); 47 generated sheets: exact original/1/4/8/12; current full scratch regeneration/check |
| `tools/textures/home_art.py` | home art painters; textures/build.py | S/T; canvas | A through parent painter batches; lattice cache shared where applicable  Modified; 47 generated sheets: exact original/1/4/8/12; current full scratch regeneration/check |
| `tools/textures/lights_art.py` | lights art painters; textures/build.py | S/T; canvas | A through parent painter batches; lattice cache shared where applicable  Unchanged (execution reason at left); 47 generated sheets: exact original/1/4/8/12; current full scratch regeneration/check |
| `tools/textures/office_art.py` | office art painters; textures/build.py | S/T; canvas | A through parent painter batches; lattice cache shared where applicable  Concurrent user edits, preserved; user changes preserved; baseline held fixed for A/B; current full scratch generation |
| `tools/textures/pool_art.py` | pool art painters; textures/build.py | S/T; canvas | A through parent painter batches; lattice cache shared where applicable  Unchanged (execution reason at left); 47 generated sheets: exact original/1/4/8/12; current full scratch regeneration/check |
| `tools/textures/seam_repair.py` | PNG seam report/check/repair; authoring | S; PNGs/diagnostics | A per-image spawn, parent writes  Modified; four real PNG original/1/4/8/12 exact repairs; malformed-batch no-write test |
| `tools/textures/water_art.py` | water art painters; textures/build.py | S/T; canvas | A through parent painter batches; lattice cache shared where applicable  Unchanged (execution reason at left); 47 generated sheets: exact original/1/4/8/12; current full scratch regeneration/check |

Historical `sitsearch.py` is referenced without an exact path in the handoff;
no copy exists in the authorized checkout/target search. Its old performance numbers
are not current validation. `tools/entities/build_editor_assets.py` was explicitly
retired; no editor work is reintroduced. Disposable baseline/reviewer copies and
Blender-generated scripts are excluded. The first-party inventory contains 64 existing
files plus three new execution/test/benchmark files. `tools/verify.sh` is the shell
consumer of the Python asset/texture/prop and native suites; no other active CI
configuration is tracked. Per-run measurements and limitations live in
[the integrated handoff](reports/feature-expansion-handoff.md).
