# Offline Python tooling

Heavy commands run sequentially under one allocation of at most 12 CPU-heavy
workers. `--workers N` selects a count within the caller's `PLACES_TOOL_WORKERS`
allocation, available CPUs and independent job count. `--workers 1` executes the
same algorithms directly (for native Blender, it caps the native CPU allocation). Do not overlap these commands with Cargo builds or
other expensive jobs. All dependencies are Python standard library and repository
modules except the explicitly named Blender workflows. These tools require Python 3.10 or newer.

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
python3 tools/bench/check_tool_parallelism.py --baseline /path/to/preserved-baseline --out target/new-tool-check --repeat 2
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
E = entity toolkit, B = Blender. No third-party Python packages are required.

| Entry point | Purpose / consumers | Dependencies / output | Class and execution |
| --- | --- | --- | --- |
| `tests/test_compiled_build.py` | Packaged native windowed smoke | S/Places; scratch runtime output | C; sequential native subprocesses |
| `tests/test_bench_metrics.py` | Trace interval and committed-world metric known answers | S/bench helpers; scratch JSON | D; tiny deterministic fixtures |
| `tests/test_lightmap_harness.py` | Capture environment and missing-output rejection | S/mock subprocess; scratch paths | D; no native game |
| `tests/test_package.py` | Repository/asset regressions; verify.sh | S/validators; scratch/stdout | D; bounded assertions |
| `tests/test_tool_execution.py` | Known answers, failure/cancellation, ordering and budgets | S/tools; scratch/stdout | B; fast spawn regressions |
| `tests/test_wgpu_bootstrap.py` | Native renderer lifecycle/quality regressions | S/Places; scratch captures | C; sequential native subprocesses |
| `tests/test_zoo_generator.py` | Catalog growth/determinism regressions | S/zoo; scratch/stdout | B; serial and spawn inspection |
| `tools/assets/validate.py` | Catalog/map schema checks; gate/package tests | S; stdout | D; fast structural validation |
| `tools/bench/bench_local.py` | Native game repeated timing; evidence | S/Places; JSON/logs | C; serial to avoid measurement interference |
| `tools/bench/check_holes.py` | Capture dark-pixel audit | S; stdout/JSON | A per-image spawn |
| `tools/bench/check_tool_parallelism.py` | Original/serial/parallel scratch benchmark | S/tools; scratch hashes/logs/times | C; serial orchestration of bounded heavy jobs |
| `tools/bench/compare_baseline.py` | Frozen baseline byte comparisons | S; stdout | C; file reads and byte comparison |
| `tools/bench/compare_captures.py` | Per-view capture differences | S; stdout | A per-pair spawn |
| `tools/bench/lightmap_report.py` | Native capture and lighting telemetry | S/Places; PNG/log/report | C; serial native game |
| `tools/bench/loading.py` | Already-built native startup and application-cache comparison | S/Places; isolated state, raw logs, traces and timings JSON | C; serial native runs; cold means an empty application cache, with OS/GPU caches untouched |
| `tools/bench/visual_check.py` | Two-build capture and connected-difference audit | S/Places; captures/report | C serial capture; A spawn comparisons |
| `tools/entities/build_mannequin.py` | Mannequin mesh and three poses; entity authoring | S/P/E; GLB and concrete PNG | D; bounded direct construction |
| `tools/entities/build_rat.py` | Rat mesh and clips; entity authoring | S/P/E/B; GLB, fur PNG, reports | C; serial owning scene, bounded native Boolean |
| `tools/entities/build_skeleton.py` | Skeleton mesh and three poses; entity authoring | S/P/E; GLB, bone PNG, optional preview | D; bounded construction, renderer used for preview |
| `tools/entities/check_clip_boundaries.py` | Exported loop/transition checks; acceptance | S/E; stdout | D; small endpoint comparisons |
| `tools/entities/rat_surface.py` | Exact anatomical union; rat builder | S/B; isolated numerical JSON | C; explicit native CPU budget |
| `tools/entities/render_contact_sheets.py` | GLB reimport and clip/pose renders; acceptance | S/P/B; scratch cells and parent PNG sheets | B; bounded native processes, thread orchestration |
| `tools/entities/rig.py` | Rig/GLB API, --check/--selftest; builders | S/P; GLB bytes and checks | D; bounded math/writer |
| `tools/entities/validate_entities.py` | Sampled skin/contact checks; acceptance/zoo | S/E; report JSON/stdout | B; initialized spawn frame chunks, shared budget |
| `tools/execution.py` | Shared worker budget, spawn executor and atomic output | S; results/encoded files | B; shared execution helper |
| `tools/levels/build_capacity_fixtures.py` | Two capacity fixture layouts; tests | S/catalog; fixture JSONs | D; bounded layout synthesis |
| `tools/levels/build_fixture_levels.py` | Showcase/stress layouts; tests | S/catalog; fixture JSONs | D; bounded layout synthesis |
| `tools/levels/build_model_zoo.py` | Catalog inspection and zoo regeneration | S/E; zoo JSON and cache | B; cached spawn inspection, shared budget |
| `tools/props/animate_spooner_man.py` | Canonical cat clip baking and --check | S/P; guarded canonical GLB | A; six spawn clip jobs, parent export |
| `tools/props/build.py` | Catalog prop build/check; verify.sh and authoring | S/P; catalog GLBs | A build jobs; D cheap checks |
| `tools/props/cat_motion.py` | IK and grounded clip sampling; animation baker | S/P; pose arrays | A; initialized model per worker |
| `tools/props/geometry.py` | Adjacency/winding; builders and repair | S; audit/mesh arrays | D; near-linear bounded meshes |
| `tools/props/glb.py` | GLB codec; builders/previews/validators | S; bytes and model arrays | D; bounded format operations |
| `tools/props/glyphs.py` | Font/polygon painter; signs/decals | S; pixels | D; small glyph work |
| `tools/props/mesh.py` | Primitive mesh API; builders | S/P; mesh arrays | D; bounded primitives |
| `tools/props/palette.py` | Color helpers; builders | S; tuples | D; trivial arithmetic |
| `tools/props/parts/__init__.py` |   init   builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches |
| `tools/props/parts/appliances.py` | appliances builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches |
| `tools/props/parts/decor.py` | decor builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches |
| `tools/props/parts/domestic_remade.py` | domestic remade builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches |
| `tools/props/parts/duck_remade.py` | duck remade builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches |
| `tools/props/parts/furniture.py` | furniture builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches |
| `tools/props/parts/home.py` | home builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches |
| `tools/props/parts/pool.py` | pool builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches |
| `tools/props/parts/pool_remade.py` | pool remade builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches |
| `tools/props/parts/refreshed.py` | refreshed builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches |
| `tools/props/parts/signage.py` | signage builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches |
| `tools/props/parts/tableware.py` | tableware builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches |
| `tools/props/parts/utility.py` | utility builders; prop build registry | S/P; mesh/texture buffers | D individually; A through parent build batches |
| `tools/props/preview.py` | Software raster previews; authoring/skeleton | S/P; PNG/contact sheets | A; per-model spawn jobs |
| `tools/props/repair_geometry.py` | Reviewed topology audit/repair; authoring | S/P; optional GLBs and report | A; per-model proposals, parent hash preflight |
| `tools/props/resize_embedded_textures.py` | Embedded PNG conversion; imports | S; in-place GLB | A; initialized source, output row bands |
| `tools/props/test_geometry.py` | Winding/cavity/seam regressions | S/P; unittest output | D; tiny fixtures |
| `tools/props/tex.py` | PNG codec and pixel API; builders/previews | S/P; pixels/PNG bytes | A via consumer batches |
| `tools/props/validate_domestic_remake.py` | Ten-model structure/texture/switch audit | S/P; fixed report JSON | D; bounded audit; run in scratch |
| `tools/textures/artkit.py` | Canvas/noise/PNG API; painters | S; pixels/PNG bytes | A via painter jobs |
| `tools/textures/build.py` | Texture painter/check CLI; verify.sh | S/T; catalog PNGs | A spawn painter jobs; D header checks |
| `tools/textures/decal_art.py` | decal art painters; textures/build.py | S/T; canvas | A through parent painter batches; lattice cache shared where applicable |
| `tools/textures/diagnostic_art.py` | diagnostic art painters; textures/build.py | S/T; canvas | A through parent painter batches; lattice cache shared where applicable |
| `tools/textures/extra_art.py` | extra art painters; textures/build.py | S/T; canvas | A through parent painter batches; lattice cache shared where applicable |
| `tools/textures/home_art.py` | home art painters; textures/build.py | S/T; canvas | A through parent painter batches; lattice cache shared where applicable |
| `tools/textures/lights_art.py` | lights art painters; textures/build.py | S/T; canvas | A through parent painter batches; lattice cache shared where applicable |
| `tools/textures/office_art.py` | office art painters; textures/build.py | S/T; canvas | A through parent painter batches; lattice cache shared where applicable |
| `tools/textures/pool_art.py` | pool art painters; textures/build.py | S/T; canvas | A through parent painter batches; lattice cache shared where applicable |
| `tools/textures/seam_repair.py` | PNG seam report/check/repair; authoring | S; PNGs/diagnostics | A per-image spawn, parent writes |
| `tools/textures/water_art.py` | water art painters; textures/build.py | S/T; canvas | A through parent painter batches; lattice cache shared where applicable |

The inventory covers maintained entry points and their execution contracts.
`tools/verify.sh` is the shell consumer of the Python validation and native
suites; no other active CI configuration is tracked. Historical functional
results, equivalence measurements and limitations are recorded in the
[integrated handoff](reports/feature-expansion-handoff.md) and
[tooling results](reports/run09-tooling-results.json). Those records describe
specific runs, not current gate status.

Native startup measurements use an already-built binary and a fresh output directory:

```sh
python3 tools/bench/loading.py --binary target/release/places --out target/loading-check --repeat 2
```

The tool runs each level sequentially with isolated application state, first
with an empty application cache and then reusing it. It does not flush OS or
GPU caches. Preserve the binary identity, logs and timing JSON when comparing
runs; do not run competing builds or CPU-heavy tooling during measurement.
